use alloy_primitives::Address;
use foundry_common::provider::tempo::{
    TempoProviderBuilder, TempoRetryProvider, TempoRetryProviderWithSigner,
};
use foundry_config::Config;
use foundry_wallets::WalletSigner;
use std::{str::FromStr, time::Duration};

/// Returns a [foundry_common::provider::RetryProvider] instantiated using [Config]'s
/// RPC
pub fn get_tempo_provider(config: &Config) -> eyre::Result<TempoRetryProvider> {
    get_tempo_provider_builder(config)?.build()
}

pub fn get_tempo_signer_provider(
    config: &Config,
    signer: WalletSigner,
) -> eyre::Result<TempoRetryProviderWithSigner> {
    let wallet = alloy_network::EthereumWallet::from(signer);
    get_tempo_provider_builder(config)?.build_with_wallet(wallet)
}

pub fn get_tempo_provider_builder(config: &Config) -> eyre::Result<TempoProviderBuilder> {
    let url = config.get_rpc_url_or_localhost_http()?;
    let mut builder = TempoProviderBuilder::new(url.as_ref());

    builder = builder.accept_invalid_certs(config.eth_rpc_accept_invalid_certs);

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
pub fn parse_fee_token_address(address_or_suffix: &str) -> eyre::Result<Address> {
    if let Ok(addr) = Address::from_str(address_or_suffix) {
        return Ok(addr);
    }

    let zeros = "0".repeat(37 - address_or_suffix.len());
    Ok(Address::from_str(format!("0x20C{zeros}{address_or_suffix}").as_str())?)
}
