//! MPP (Machine Payments Protocol) HTTP transport.
//!
//! Wraps a standard reqwest HTTP transport with automatic 402 Payment Required
//! handling via the MPP protocol. When the RPC endpoint returns a 402 response,
//! this transport automatically pays the challenge and retries the request.

use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::{TransportError, TransportErrorKind, TransportFut, TransportResult};
use mpp::{
    client::PaymentProvider,
    protocol::core::{
        AUTHORIZATION_HEADER, WWW_AUTHENTICATE_HEADER, format_authorization, parse_www_authenticate,
    },
};
use reqwest::StatusCode;
use std::{fmt, sync::Mutex, task};
use tower::Service;
use tracing::{Instrument, debug, debug_span, trace};
use url::Url;

use super::{keys::discover_mpp_config, session::SessionProvider};

/// Default deposit amount for new channels (in base units).
const DEFAULT_DEPOSIT: u128 = 100_000;

/// Resolve the deposit amount from `MPP_DEPOSIT` env var or the default.
fn default_deposit() -> u128 {
    std::env::var("MPP_DEPOSIT").ok().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_DEPOSIT)
}

/// Production transport: lazily discovers MPP keys from the Tempo wallet on
/// first 402 response.
pub type LazyMppHttpTransport = MppHttpTransport<LazySessionProvider>;

/// A payment provider that lazily initializes a [`SessionProvider`] from the
/// Tempo wallet configuration on first use.
#[derive(Clone, Debug)]
pub struct LazySessionProvider {
    inner: std::sync::Arc<Mutex<Option<SessionProvider>>>,
    origin: String,
}

impl LazySessionProvider {
    fn new(origin: String) -> Self {
        Self { inner: std::sync::Arc::new(Mutex::new(None)), origin }
    }

    fn mark_key_not_provisioned(&self) {
        if let Some(p) = self.inner.lock().unwrap().as_ref() {
            p.set_key_provisioned(false);
        }
    }

    fn clear_channels(&self) {
        if let Some(p) = self.inner.lock().unwrap().as_ref() {
            p.clear_channels();
        }
    }

    fn get_or_init(&self) -> TransportResult<SessionProvider> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(ref provider) = *guard {
            return Ok(provider.clone());
        }

        let config = discover_mpp_config().ok_or_else(|| {
            TransportErrorKind::custom(std::io::Error::other(
                "RPC endpoint returned HTTP 402 Payment Required. \
                 This endpoint requires payment via the Machine Payments Protocol (MPP).\n\n\
                 To configure MPP, install the Tempo wallet CLI and create a key:\n\
                 \n  curl -sSL https://tempo.xyz/install.sh | bash\
                 \n  tempo wallet login\
                 \n\nSee https://docs.tempo.xyz for more information.",
            ))
        })?;

        let signer: mpp::PrivateKeySigner = config.key.parse().map_err(|e| {
            TransportErrorKind::custom(std::io::Error::other(format!("invalid MPP key: {e}")))
        })?;

        let signing_mode = if let Some(wallet) = config.wallet_address {
            let key_authorization = config
                .key_authorization
                .as_ref()
                .map(|hex_str| {
                    crate::tempo::decode_key_authorization(hex_str).map(Box::new).map_err(|e| {
                        TransportErrorKind::custom(std::io::Error::other(format!(
                            "invalid MPP key_authorization: {e}"
                        )))
                    })
                })
                .transpose()?;

            mpp::client::tempo::signing::TempoSigningMode::Keychain {
                wallet,
                key_authorization,
                version: mpp::client::tempo::signing::KeychainVersion::V2,
            }
        } else {
            mpp::client::tempo::signing::TempoSigningMode::Direct
        };

        let mut provider = SessionProvider::new(signer, self.origin.clone())
            .with_signing_mode(signing_mode)
            .with_default_deposit(default_deposit());

        if let Some(addr) = config.key_address {
            provider = provider.with_authorized_signer(addr);
        }

        *guard = Some(provider.clone());
        Ok(provider)
    }
}

/// HTTP transport with automatic MPP (Machine Payments Protocol) 402 handling.
///
/// Generic over the payment provider `P`. Works as a normal HTTP transport until
/// a 402 Payment Required response is received, then delegates payment to `P`.
#[derive(Clone, Debug)]
pub struct MppHttpTransport<P> {
    client: reqwest::Client,
    url: Url,
    provider: P,
}

impl MppHttpTransport<LazySessionProvider> {
    /// Create a new lazy MPP transport that discovers keys on first 402.
    ///
    /// Builds a separate reqwest client with a longer timeout (120s) for MPP
    /// requests, since channel open/topUp involves on-chain transaction
    /// settlement which can take much longer than normal RPC calls.
    pub fn lazy(client: reqwest::Client, url: Url) -> Self {
        let origin = url.to_string();
        Self { client, url, provider: LazySessionProvider::new(origin) }
    }
}

impl<P> MppHttpTransport<P> {
    /// Create a new MPP transport with an explicit payment provider.
    pub fn new(client: reqwest::Client, url: Url, provider: P) -> Self {
        Self { client, url, provider }
    }

    /// Returns a reference to the underlying reqwest client.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

#[allow(private_bounds)]
impl<P: ResolveProvider + Clone + Send + Sync + 'static> MppHttpTransport<P>
where
    P::Provider: Send + Sync + 'static,
{
    async fn do_request(self, req: RequestPacket) -> TransportResult<ResponsePacket> {
        let body = serde_json::to_vec(&req).map_err(TransportErrorKind::custom)?;
        let headers = req.headers();

        let resp = self
            .client
            .post(self.url.clone())
            .headers(headers.clone())
            .header("content-type", "application/json")
            .body(body.clone())
            .send()
            .await
            .map_err(TransportErrorKind::custom)?;

        if resp.status() != StatusCode::PAYMENT_REQUIRED {
            return Self::handle_response(resp).await;
        }

        let www_auth = resp
            .headers()
            .get(WWW_AUTHENTICATE_HEADER)
            .or_else(|| resp.headers().get("www-authenticate"))
            .ok_or_else(|| {
                TransportErrorKind::custom(std::io::Error::other(
                    "402 response missing WWW-Authenticate header",
                ))
            })?
            .to_str()
            .map_err(|e| {
                TransportErrorKind::custom(std::io::Error::other(format!(
                    "invalid WWW-Authenticate header: {e}"
                )))
            })?;

        let challenge = parse_www_authenticate(www_auth).map_err(|e| {
            TransportErrorKind::custom(std::io::Error::other(format!("invalid MPP challenge: {e}")))
        })?;

        debug!(id = %challenge.id, method = %challenge.method, intent = %challenge.intent, "received MPP 402 challenge, paying");

        let resolved = self.provider.resolve()?;

        if !resolved.supports(challenge.method.as_str(), challenge.intent.as_str()) {
            return Err(TransportErrorKind::custom(std::io::Error::other(format!(
                "MPP challenge requires method={} intent={}, which is not supported",
                challenge.method, challenge.intent,
            ))));
        }

        let credential = resolved.pay(&challenge).await.map_err(|e| {
            TransportErrorKind::custom(std::io::Error::other(format!("MPP payment failed: {e}")))
        })?;

        let auth_header = format_authorization(&credential).map_err(|e| {
            TransportErrorKind::custom(std::io::Error::other(format!(
                "failed to format MPP credential: {e}"
            )))
        })?;

        // Use a longer per-request timeout because the server may need to
        // settle an on-chain transaction (channel open/topUp) before responding.
        let retry_resp = self
            .client
            .post(self.url.clone())
            .headers(headers.clone())
            .header("content-type", "application/json")
            .header(AUTHORIZATION_HEADER, &auth_header)
            .body(body.clone())
            .send()
            .await
            .map_err(TransportErrorKind::custom)?;

        // 204 No Content → topUp accepted, re-pay with voucher
        if retry_resp.status() == StatusCode::NO_CONTENT {
            debug!("MPP topUp accepted (204), retrying with voucher");

            let resolved = self.provider.resolve()?;
            let credential = resolved.pay(&challenge).await.map_err(|e| {
                TransportErrorKind::custom(std::io::Error::other(format!(
                    "MPP payment failed: {e}"
                )))
            })?;
            let auth_header = format_authorization(&credential).map_err(|e| {
                TransportErrorKind::custom(std::io::Error::other(format!(
                    "failed to format MPP credential: {e}"
                )))
            })?;

            let voucher_resp = self
                .client
                .post(self.url.clone())
                .headers(headers.clone())
                .header("content-type", "application/json")
                .header(AUTHORIZATION_HEADER, &auth_header)
                .body(body.clone())
                .send()
                .await
                .map_err(TransportErrorKind::custom)?;

            return Self::handle_response(voucher_resp).await;
        }

        // 410 Gone → channel stale
        if retry_resp.status() == StatusCode::GONE {
            debug!("MPP channel not found (410), clearing stale local state");
            self.provider.clear_channels();

            return Err(TransportErrorKind::custom(std::io::Error::other(
                "MPP channel not found on server (410 Gone). \
                 The server may have restarted or the channel was closed externally.\n\
                 Local channel state has been cleared. Re-run to open a new channel.",
            )));
        }

        // Retry 402 → try with key_authorization
        if retry_resp.status() == StatusCode::PAYMENT_REQUIRED {
            self.provider.mark_key_not_provisioned();
            let resolved = self.provider.resolve()?;

            if resolved.supports(challenge.method.as_str(), challenge.intent.as_str()) {
                debug!("first MPP attempt returned 402, retrying with key_authorization");

                let credential = resolved.pay(&challenge).await.map_err(|e| {
                    TransportErrorKind::custom(std::io::Error::other(format!(
                        "MPP payment failed: {e}"
                    )))
                })?;
                let auth_header = format_authorization(&credential).map_err(|e| {
                    TransportErrorKind::custom(std::io::Error::other(format!(
                        "failed to format MPP credential: {e}"
                    )))
                })?;

                let final_resp = self
                    .client
                    .post(self.url.clone())
                    .headers(headers)
                    .header("content-type", "application/json")
                    .header(AUTHORIZATION_HEADER, auth_header)
                    .body(body)
                    .send()
                    .await
                    .map_err(TransportErrorKind::custom)?;

                return Self::handle_response(final_resp).await;
            }
        }

        Self::handle_response(retry_resp).await
    }

    async fn handle_response(resp: reqwest::Response) -> TransportResult<ResponsePacket> {
        let status = resp.status();
        debug!(%status, "received response from MPP transport");

        let body = resp.bytes().await.map_err(TransportErrorKind::custom)?;

        if tracing::enabled!(tracing::Level::TRACE) {
            trace!(body = %String::from_utf8_lossy(&body), "response body");
        } else {
            debug!(bytes = body.len(), "retrieved response body");
        }

        if !status.is_success() {
            return Err(TransportErrorKind::http_error(
                status.as_u16(),
                String::from_utf8_lossy(&body).into_owned(),
            ));
        }

        serde_json::from_slice(&body)
            .map_err(|err| TransportError::deser_err(err, String::from_utf8_lossy(&body)))
    }
}

/// Trait for resolving a concrete `PaymentProvider` from a potentially lazy wrapper.
pub(crate) trait ResolveProvider {
    type Provider: PaymentProvider;
    fn resolve(&self) -> TransportResult<Self::Provider>;
    fn mark_key_not_provisioned(&self) {}
    fn clear_channels(&self) {}
}

impl<P: PaymentProvider + Clone> ResolveProvider for P {
    type Provider = P;
    fn resolve(&self) -> TransportResult<P> {
        Ok(self.clone())
    }
}

impl ResolveProvider for LazySessionProvider {
    type Provider = SessionProvider;
    fn resolve(&self) -> TransportResult<SessionProvider> {
        self.get_or_init()
    }
    fn mark_key_not_provisioned(&self) {
        Self::mark_key_not_provisioned(self)
    }
    fn clear_channels(&self) {
        Self::clear_channels(self)
    }
}

impl<P> fmt::Display for MppHttpTransport<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MppHttpTransport({})", self.url)
    }
}

#[allow(private_bounds)]
impl<P: ResolveProvider + Clone + Send + Sync + fmt::Debug + 'static> Service<RequestPacket>
    for MppHttpTransport<P>
where
    P::Provider: Send + Sync + 'static,
{
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    #[inline]
    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    #[inline]
    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let this = self.clone();
        let span = debug_span!("MppHttpTransport", url = %this.url);
        Box::pin(this.do_request(req).instrument(span.or_current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        mpp::keys::discover_mpp_key, runtime_transport::RuntimeTransportBuilder,
    };
    use alloy_json_rpc::{Id, Request, RequestMeta};
    use axum::{
        extract::State, http::StatusCode as AxumStatusCode, response::IntoResponse, routing::post,
    };
    use mpp::{
        MppError,
        client::tempo::signing::{KeychainVersion, TempoSigningMode},
        protocol::core::{
            Base64UrlJson, PaymentChallenge, PaymentCredential, PaymentPayload,
            format_www_authenticate, parse_authorization,
        },
    };
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    #[derive(Clone, Debug)]
    struct MockPaymentProvider;

    impl PaymentProvider for MockPaymentProvider {
        fn supports(&self, method: &str, intent: &str) -> bool {
            method == "tempo" && intent == "charge"
        }

        async fn pay(&self, challenge: &PaymentChallenge) -> Result<PaymentCredential, MppError> {
            Ok(PaymentCredential::with_source(
                challenge.to_echo(),
                "did:pkh:eip155:42431:0xmockpayer",
                PaymentPayload::hash("0xmocktxhash"),
            ))
        }
    }

    fn test_challenge() -> (mpp::PaymentChallenge, String) {
        let request = Base64UrlJson::from_value(&serde_json::json!({
            "amount": "1000",
            "currency": "0x20c0000000000000000000000000000000000000",
            "recipient": "0x742d35Cc6634C0532925a3b844Bc9e7595f1B0F2",
            "methodDetails": { "chainId": 42431 }
        }))
        .unwrap();
        let challenge =
            mpp::PaymentChallenge::new("test-id-42", "rpc.example.com", "tempo", "charge", request);
        let header = format_www_authenticate(&challenge).unwrap();
        (challenge, header)
    }

    fn test_request() -> RequestPacket {
        let req = Request {
            meta: RequestMeta::new("eth_blockNumber".into(), Id::Number(1)),
            params: serde_json::value::RawValue::from_string("[]".into()).unwrap(),
        };
        req.serialize().unwrap().into()
    }

    async fn spawn_server(app: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn test_mpp_transport_non_402_passthrough() {
        let app = axum::Router::new().route(
            "/",
            post(|| async {
                axum::Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1234"
                }))
            }),
        );

        let (base_url, handle) = spawn_server(app).await;
        let mut transport = MppHttpTransport::new(
            reqwest::Client::new(),
            Url::parse(&base_url).unwrap(),
            MockPaymentProvider,
        );

        let resp = tower::Service::call(&mut transport, test_request()).await.unwrap();

        match resp {
            ResponsePacket::Single(r) => {
                assert!(r.is_success());
            }
            _ => panic!("expected single response"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_mpp_transport_402_then_200() {
        let (_, www_auth) = test_challenge();
        let call_count = Arc::new(AtomicU32::new(0));

        #[derive(Clone)]
        struct AppState {
            www_auth: String,
            call_count: Arc<AtomicU32>,
        }

        let state = AppState { www_auth, call_count: call_count.clone() };

        let app =
            axum::Router::new()
                .route(
                    "/",
                    post(
                        |State(state): State<AppState>,
                         req: axum::http::Request<axum::body::Body>| async move {
                            state.call_count.fetch_add(1, Ordering::SeqCst);

                            if req.headers().get("authorization").is_some() {
                                (
                                    AxumStatusCode::OK,
                                    axum::Json(serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": 1,
                                        "result": "0xpaid"
                                    })),
                                )
                                    .into_response()
                            } else {
                                (
                                    AxumStatusCode::PAYMENT_REQUIRED,
                                    [("www-authenticate", state.www_auth)],
                                    "Payment Required",
                                )
                                    .into_response()
                            }
                        },
                    ),
                )
                .with_state(state);

        let (base_url, handle) = spawn_server(app).await;
        let mut transport = MppHttpTransport::new(
            reqwest::Client::new(),
            Url::parse(&base_url).unwrap(),
            MockPaymentProvider,
        );

        let resp = tower::Service::call(&mut transport, test_request()).await.unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst), 2);

        match resp {
            ResponsePacket::Single(r) => {
                assert!(r.is_success());
            }
            _ => panic!("expected single response"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_mpp_transport_402_credential_is_valid() {
        let (_, www_auth) = test_challenge();

        #[derive(Clone)]
        struct AppState {
            www_auth: String,
        }

        let state = AppState { www_auth };

        let app =
            axum::Router::new()
                .route(
                    "/",
                    post(
                        |State(state): State<AppState>,
                         req: axum::http::Request<axum::body::Body>| async move {
                            if let Some(auth) = req.headers().get("authorization") {
                                let auth_str = auth.to_str().unwrap();
                                let credential = parse_authorization(auth_str).unwrap();
                                assert_eq!(credential.challenge.id, "test-id-42");
                                assert_eq!(credential.challenge.method.as_str(), "tempo");
                                assert!(credential.source.is_some());

                                (
                                    AxumStatusCode::OK,
                                    axum::Json(serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": 1,
                                        "result": "0xvalidated"
                                    })),
                                )
                                    .into_response()
                            } else {
                                (
                                    AxumStatusCode::PAYMENT_REQUIRED,
                                    [("www-authenticate", state.www_auth)],
                                    "Payment Required",
                                )
                                    .into_response()
                            }
                        },
                    ),
                )
                .with_state(state);

        let (base_url, handle) = spawn_server(app).await;
        let mut transport = MppHttpTransport::new(
            reqwest::Client::new(),
            Url::parse(&base_url).unwrap(),
            MockPaymentProvider,
        );

        let resp = tower::Service::call(&mut transport, test_request()).await.unwrap();
        match resp {
            ResponsePacket::Single(r) => assert!(r.is_success()),
            _ => panic!("expected single response"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_mpp_transport_402_missing_www_authenticate() {
        let app = axum::Router::new()
            .route("/", post(|| async { (AxumStatusCode::PAYMENT_REQUIRED, "pay up") }));

        let (base_url, handle) = spawn_server(app).await;
        let mut transport = MppHttpTransport::new(
            reqwest::Client::new(),
            Url::parse(&base_url).unwrap(),
            MockPaymentProvider,
        );

        let err = tower::Service::call(&mut transport, test_request()).await.unwrap_err();
        assert!(
            err.to_string().contains("WWW-Authenticate"),
            "expected WWW-Authenticate error, got: {err}"
        );

        handle.abort();
    }

    #[tokio::test]
    async fn test_plain_http_402_shows_mpp_setup_instructions() {
        let (_, www_auth) = test_challenge();

        let app = axum::Router::new().route(
            "/",
            post(move || {
                let www_auth = www_auth.clone();
                async move {
                    (
                        AxumStatusCode::PAYMENT_REQUIRED,
                        [("www-authenticate", www_auth)],
                        "Payment Required",
                    )
                }
            }),
        );

        let (base_url, handle) = spawn_server(app).await;

        unsafe {
            std::env::set_var("TEMPO_HOME", "/nonexistent/path");
            std::env::remove_var("TEMPO_PRIVATE_KEY");
        }

        let transport = RuntimeTransportBuilder::new(Url::parse(&base_url).unwrap()).build();
        let err = transport.request(test_request()).await.unwrap_err();
        let msg = err.to_string();

        assert!(
            msg.contains("402 Payment Required"),
            "expected 402 Payment Required in error, got: {msg}"
        );
        assert!(
            msg.contains("tempo wallet login"),
            "expected setup instructions in error, got: {msg}"
        );

        handle.abort();
        unsafe { std::env::remove_var("TEMPO_HOME") };
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_mpp_live_402() {
        let client = reqwest::Client::new();
        let resp = client
            .post("https://rpc.mpp.tempo.xyz")
            .header("content-type", "application/json")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);

        let www_auth = resp
            .headers()
            .get(WWW_AUTHENTICATE_HEADER)
            .expect("missing WWW-Authenticate header")
            .to_str()
            .unwrap();

        let challenge = parse_www_authenticate(www_auth).unwrap();
        assert_eq!(challenge.realm, "rpc.mpp.tempo.xyz");
        assert_eq!(challenge.method.as_str(), "tempo");
    }

    #[tokio::test]
    #[ignore = "requires network access and a funded Tempo wallet"]
    async fn test_mpp_live_pay() {
        let _mpp_key = discover_mpp_key().expect(
            "no MPP key found; set TEMPO_PRIVATE_KEY or configure ~/.tempo/wallet/keys.toml",
        );

        let config = discover_mpp_config()
            .expect("no MPP config found; configure ~/.tempo/wallet/keys.toml");

        let signer: mpp::PrivateKeySigner =
            config.key.parse().expect("failed to parse MPP key as PrivateKeySigner");

        let wallet_address = config.wallet_address.expect("missing wallet_address");
        let signer_address = config.key_address.expect("missing key_address");

        let signing_mode = TempoSigningMode::Keychain {
            wallet: wallet_address,
            key_authorization: None,
            version: KeychainVersion::V2,
        };

        let service_url = "https://rpc.mpp.tempo.xyz";
        let provider = super::super::session::SessionProvider::new(signer, service_url.to_string())
            .with_signing_mode(signing_mode)
            .with_authorized_signer(signer_address)
            .with_default_deposit(100_000);

        let mut transport = MppHttpTransport::new(
            reqwest::Client::new(),
            Url::parse(service_url).unwrap(),
            provider,
        );

        let resp = tower::Service::call(&mut transport, test_request()).await.unwrap();

        match resp {
            ResponsePacket::Single(r) => {
                assert!(r.is_success(), "expected successful JSON-RPC response, got: {r:?}");
            }
            _ => panic!("expected single response"),
        }
    }
}
