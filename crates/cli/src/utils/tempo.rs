use alloy_primitives::{Address, hex};
use eyre::Result;
use foundry_common::provider::tempo::{TempoProviderBuilder, TempoRetryProvider};
use foundry_config::Config;
use std::{str::FromStr, time::Duration};

/// Returns a [foundry_common::provider::RetryProvider] instantiated using [Config]'s
/// RPC
pub fn get_tempo_provider(config: &Config) -> eyre::Result<TempoRetryProvider> {
    get_tempo_provider_builder(config, false)?.build()
}

/// Returns a [RetryProvider] with curl mode option.
///
/// When `curl_mode` is true, the provider will print equivalent curl commands
/// to stdout instead of executing RPC requests.
pub fn get_tempo_provider_with_curl(
    config: &Config,
    curl_mode: bool,
) -> Result<TempoRetryProvider> {
    get_tempo_provider_builder(config, curl_mode)?.build()
}

pub fn get_tempo_provider_builder(
    config: &Config,
    curl_mode: bool,
) -> eyre::Result<TempoProviderBuilder> {
    let url = config.get_rpc_url_or_localhost_http()?;
    let mut builder = TempoProviderBuilder::new(url.as_ref());

    builder = builder.accept_invalid_certs(config.eth_rpc_accept_invalid_certs);
    builder = builder.curl_mode(curl_mode);

    if let Ok(chain) = config.chain.unwrap_or_default().try_into() {
        builder = builder.chain(chain);
    }

    if let Some(jwt) = config.get_rpc_jwt_secret()? {
        builder = builder.jwt(jwt.as_ref());
    }

    if let Some(rpc_timeout) = config.eth_rpc_timeout {
        builder = builder.timeout(Duration::from_secs(rpc_timeout));
    }

    if let Some(rpc_headers) = config.eth_rpc_headers.clone() {
        builder = builder.headers(rpc_headers);
    }

    Ok(builder)
}

/// Parses a fee token address.
pub fn parse_fee_token_address(address_or_id: &str) -> eyre::Result<Address> {
    Address::from_str(address_or_id).or_else(|_| Ok(token_id_to_address(address_or_id.parse()?)))
}

fn token_id_to_address(token_id: u64) -> Address {
    let mut address_bytes = [0u8; 20];
    address_bytes[..12].copy_from_slice(&hex!("20C000000000000000000000"));
    address_bytes[12..20].copy_from_slice(&token_id.to_be_bytes());
    Address::from(address_bytes)
}
