use clap::Parser;
use eyre::{Result, eyre};
use foundry_cli::opts::EthereumOpts;
use foundry_wallets::{ParsedAccessKey, WalletSigner};
use mpp::client::tempo::signing::{KeychainVersion, TempoSigningMode};
use mpp::client::{Fetch, TempoProvider};

/// CLI arguments for `cast mpp`.
#[derive(Debug, Parser)]
pub struct MppArgs {
    /// The URL of the MPP-enabled endpoint.
    #[arg(value_name = "URL")]
    url: String,

    /// HTTP method to use.
    #[arg(long, default_value = "GET", value_name = "METHOD")]
    method: String,

    /// HTTP headers to include (can be specified multiple times).
    #[arg(short = 'H', long = "header", value_name = "HEADER")]
    headers: Vec<String>,

    /// Request body data.
    #[arg(short = 'd', long = "data", value_name = "DATA")]
    data: Option<String>,

    /// Print verbose output including response headers and payment receipt.
    #[arg(short = 'v', long)]
    verbose: bool,

    #[command(flatten)]
    eth: EthereumOpts,
}

impl MppArgs {
    pub async fn run(self) -> Result<()> {
        let Self { url, method, headers, data, verbose, eth } = self;

        // Build the MPP provider from wallet opts
        let provider = build_mpp_provider(&eth).await?;

        // Build the HTTP request
        let client = reqwest::Client::new();
        let http_method: reqwest::Method =
            method.parse().map_err(|_| eyre!("Invalid HTTP method: {method}"))?;

        let mut request = client.request(http_method, &url);

        // Add headers
        for header in &headers {
            let (key, value) = header
                .split_once(':')
                .ok_or_else(|| eyre!("Invalid header format (expected 'Key: Value'): {header}"))?;
            request = request.header(key.trim(), value.trim());
        }

        // Add body
        if let Some(body) = data {
            request = request.body(body);
        }

        // Send with MPP payment handling
        let resp: reqwest::Response =
            request.send_with_payment(&provider).await.map_err(|e| eyre!("{e}"))?;

        if verbose {
            sh_eprintln!("Status: {}", resp.status())?;
            for (key, value) in resp.headers() {
                if let Ok(v) = value.to_str() {
                    sh_eprintln!("{}: {}", key, v)?;
                }
            }
            sh_eprintln!()?;
        }

        let body = resp.text().await.map_err(|e| eyre!("Failed to read response body: {e}"))?;
        sh_println!("{body}")?;

        Ok(())
    }
}

/// Build an MPP TempoProvider from the wallet and RPC options.
async fn build_mpp_provider(eth: &EthereumOpts) -> Result<TempoProvider> {
    let wallet = &eth.wallet;

    // Get the RPC URL
    let rpc_url = eth.rpc.url.as_deref().unwrap_or("https://rpc.tempo.xyz");

    // Resolve signer - MPP requires a local signer
    let signer = wallet.signer().await?;
    let local_signer = match signer {
        WalletSigner::Local(s) => s,
        _ => {
            return Err(eyre!(
                "MPP requires a local signer (private key, mnemonic, or access key). \
                 Hardware wallets and browser wallets are not supported."
            ));
        }
    };

    let mut provider = TempoProvider::new(local_signer, rpc_url)
        .map_err(|e| eyre!("Failed to create MPP provider: {e}"))?;

    // If using an access key, set up Keychain signing mode
    if wallet.is_access_key() {
        let root_account = wallet
            .root_account
            .ok_or_else(|| eyre!("--tempo.root-account is required when using --tempo.access-key"))?;

        let key_authorization = resolve_key_authorization(wallet.access_key.as_deref())?;

        provider = provider.with_signing_mode(TempoSigningMode::Keychain {
            wallet: root_account,
            key_authorization,
            version: KeychainVersion::V2,
        });
    }

    Ok(provider)
}

/// Resolve the key authorization from the access key string, if it's in export format.
fn resolve_key_authorization(
    access_key: Option<&str>,
) -> Result<Option<Box<tempo_primitives::transaction::SignedKeyAuthorization>>> {
    let Some(access_key) = access_key else {
        return Ok(None);
    };

    let parsed = ParsedAccessKey::parse(access_key)?;
    let Some(rlp_bytes) = parsed.key_authorization_rlp else {
        return Ok(None);
    };

    // Decode the RLP-encoded SignedKeyAuthorization
    let auth = alloy_rlp::Decodable::decode(&mut rlp_bytes.as_slice())
        .map_err(|e| eyre!("Failed to decode key authorization RLP: {e}"))?;

    Ok(Some(Box::new(auth)))
}
