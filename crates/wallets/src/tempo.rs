use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_signer::Signer;
use alloy_sol_types::SolCall;
use eyre::{Result, eyre};
use foundry_common::tempo;
use tempo_alloy::{TempoNetwork, provider::TempoProviderExt, rpc::TempoTransactionRequest};
use tempo_primitives::transaction::{
    SignedKeyAuthorization, TempoTxEnvelope,
    tt_signature::{KeychainSignature, PrimitiveSignature, TempoSignature},
    tt_signed::AASigned,
};

use crate::{WalletSigner, utils};

pub use tempo_contracts::precompiles::{
    ACCOUNT_KEYCHAIN_ADDRESS,
    IAccountKeychain::{
        self, CallScope, KeyRestrictions, SignatureType, TokenLimit,
        authorizeKey_1Call as authorizeKeyCall, revokeKeyCall, updateSpendingLimitCall,
    },
};

/// Configuration for a Tempo access key resolved from `keys.toml`.
///
/// When a Tempo wallet entry uses keychain mode (`wallet_address != key_address`), the signer
/// is an access key that signs on behalf of the root wallet.
#[derive(Debug, Clone)]
pub struct TempoAccessKeyConfig {
    /// The root wallet address (the `from` address for transactions).
    pub wallet_address: Address,
    /// The access key's address (derived from the private key that actually signs).
    pub key_address: Address,
    /// Decoded key authorization for on-chain provisioning.
    ///
    /// When present, callers should check whether the key is already provisioned on-chain
    /// (via the AccountKeychain precompile) before including this in a transaction.
    pub key_authorization: Option<SignedKeyAuthorization>,
}

/// Result of looking up an address in Tempo's key store.
pub enum TempoLookup {
    /// A direct (EOA) signer was found — `wallet_address == key_address`.
    Direct(WalletSigner),
    /// A keychain (access key) signer was found — `wallet_address != key_address`.
    Keychain(WalletSigner, Box<TempoAccessKeyConfig>),
    /// No matching entry was found.
    NotFound,
}

/// Looks up a signer for the given address in Tempo's `keys.toml`.
///
/// Returns [`TempoLookup::Direct`] if a direct-mode (EOA) key is found,
/// [`TempoLookup::Keychain`] if a keychain-mode access key is found,
/// or [`TempoLookup::NotFound`] if no entry matches.
pub fn lookup_signer(from: Address) -> Result<TempoLookup> {
    let file = match tempo::read_tempo_keys_file() {
        Some(f) => f,
        None => return Ok(TempoLookup::NotFound),
    };

    for entry in &file.keys {
        if entry.wallet_address != from {
            continue;
        }

        let Some(key) = &entry.key else {
            continue;
        };

        // Direct mode: wallet_address == key_address (or key_address is absent).
        let is_direct =
            entry.key_address.is_none() || entry.key_address == Some(entry.wallet_address);

        let signer = utils::create_private_key_signer(key)?;

        if is_direct {
            return Ok(TempoLookup::Direct(signer));
        }

        // Keychain mode: the key is an access key signing on behalf of wallet_address.
        let key_authorization = entry
            .key_authorization
            .as_deref()
            .map(tempo::decode_key_authorization::<SignedKeyAuthorization>)
            .transpose()?;

        let config = TempoAccessKeyConfig {
            wallet_address: entry.wallet_address,
            // SAFETY: `is_direct` was false, so `key_address` is `Some` and != wallet_address.
            key_address: entry.key_address.unwrap(),
            key_authorization,
        };
        return Ok(TempoLookup::Keychain(signer, Box::new(config)));
    }

    Ok(TempoLookup::NotFound)
}

/// Signs a transaction request with an access key, producing a type 0x76 AA transaction
/// with a Keychain signature.
///
/// Returns the RLP-encoded signed transaction bytes.
pub async fn sign_with_access_key<S: Signer>(
    tx_request: TempoTransactionRequest,
    signer: &S,
    root_account: Address,
) -> Result<Vec<u8>> {
    // Build TempoTransaction from the request
    let tempo_tx =
        tx_request.build_aa().map_err(|e| eyre!("Failed to build AA transaction: {:?}", e))?;

    // Compute the V2 signing hash: keccak256(0x04 || sig_hash || user_address)
    let sig_hash = tempo_tx.signature_hash();
    let signing_hash = KeychainSignature::signing_hash(sig_hash, root_account);

    // Sign the V2 hash with the access key
    let raw_sig = signer.sign_hash(&signing_hash).await?;

    // Wrap in KeychainSignature with root account address
    let primitive_sig = PrimitiveSignature::Secp256k1(raw_sig);
    let keychain_sig = KeychainSignature::new(root_account, primitive_sig);
    let tempo_sig = TempoSignature::Keychain(keychain_sig);

    // Create signed AA transaction and encode
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::from(signed_tx);
    Ok(envelope.encoded_2718())
}

/// Checks whether an access key is already provisioned on-chain.
///
/// Queries the AccountKeychain precompile's `getKey` function. A key is considered
/// provisioned if the returned `keyId` is non-zero (i.e. the key exists and has not
/// been revoked).
pub async fn is_key_provisioned<P: Provider<TempoNetwork>>(
    provider: &P,
    wallet_address: Address,
    key_address: Address,
) -> bool {
    match provider.get_keychain_key(wallet_address, key_address).await {
        Ok(info) => info.keyId != Address::ZERO,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Calldata builders (pure — no provider needed)
// ---------------------------------------------------------------------------

/// ABI-encodes an `authorizeKey` call for the AccountKeychain precompile (T3).
pub fn authorize_key_calldata(
    key_id: Address,
    sig_type: SignatureType,
    config: KeyRestrictions,
) -> Vec<u8> {
    authorizeKeyCall { keyId: key_id, signatureType: sig_type, config }.abi_encode()
}

/// ABI-encodes a `revokeKey` call for the AccountKeychain precompile.
pub fn revoke_key_calldata(key_id: Address) -> Vec<u8> {
    revokeKeyCall { keyId: key_id }.abi_encode()
}

/// ABI-encodes an `updateSpendingLimit` call for the AccountKeychain precompile.
pub fn update_spending_limit_calldata(key_id: Address, token: Address, new_limit: U256) -> Vec<u8> {
    updateSpendingLimitCall { keyId: key_id, token, newLimit: new_limit }.abi_encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    const TEST_KEY: Address = address!("0x1111111111111111111111111111111111111111");
    const TEST_TOKEN: Address = address!("0x2222222222222222222222222222222222222222");

    #[test]
    fn test_authorize_key_calldata() {
        let config = KeyRestrictions {
            expiry: u64::MAX,
            enforceLimits: false,
            limits: vec![],
            allowAnyCalls: true,
            allowedCalls: vec![],
        };
        let data = authorize_key_calldata(TEST_KEY, SignatureType::Secp256k1, config);
        assert!(!data.is_empty());
        assert_eq!(&data[..4], &authorizeKeyCall::SELECTOR);
    }

    #[test]
    fn test_revoke_key_calldata() {
        let data = revoke_key_calldata(TEST_KEY);
        assert!(!data.is_empty());
        assert_eq!(&data[..4], &revokeKeyCall::SELECTOR);
    }

    #[test]
    fn test_update_spending_limit_calldata() {
        let data = update_spending_limit_calldata(TEST_KEY, TEST_TOKEN, U256::from(1000));
        assert!(!data.is_empty());
        assert_eq!(&data[..4], &updateSpendingLimitCall::SELECTOR);
    }
}
