use alloy_contract::private::Network;
use alloy_primitives::Address;
use alloy_provider::Provider;
use tempo_alloy::contracts::precompiles::{
    ACCOUNT_KEYCHAIN_ADDRESS, account_keychain::IAccountKeychain,
};

pub use foundry_wallets::tempo::sign_with_access_key;

/// Checks whether an access key is already provisioned on-chain.
///
/// Queries the AccountKeychain precompile's `getKey` function. A key is considered
/// provisioned if the returned `keyId` is non-zero (i.e. the key exists and has not
/// been revoked).
///
/// Returns `false` if the key is not found. Logs a warning on RPC/transport errors
/// and conservatively returns `false` (which causes `key_authorization` to be included).
pub async fn is_key_provisioned<N: Network, P: Provider<N>>(
    provider: &P,
    wallet_address: Address,
    key_address: Address,
) -> bool {
    let keychain = IAccountKeychain::new(ACCOUNT_KEYCHAIN_ADDRESS, provider);
    match keychain.getKey(wallet_address, key_address).call().await {
        Ok(info) => info.keyId != Address::ZERO,
        Err(e) => {
            // Distinguish "key not found" (contract revert) from transport errors.
            // Contract reverts (KeyNotFound) mean the key is not provisioned.
            // Transport errors are unexpected — log them so users can debug.
            let err_str = e.to_string();
            if !err_str.contains("KeyNotFound") && !err_str.contains("execution reverted") {
                warn!("failed to check key provisioning status: {e}");
            }
            false
        }
    }
}
