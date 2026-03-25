use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, hex};
use alloy_provider::Provider;
use alloy_rlp::Decodable;
use alloy_signer::Signer;
use eyre::{Result, eyre};
use std::path::PathBuf;
use tempo_alloy::{TempoNetwork, provider::TempoProviderExt, rpc::TempoTransactionRequest};
use tempo_primitives::transaction::{
    SignedKeyAuthorization, TempoTxEnvelope,
    tt_signature::{KeychainSignature, PrimitiveSignature, TempoSignature},
    tt_signed::AASigned,
};

use crate::{WalletSigner, utils};

/// Wallet type: how this wallet was created.
#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum WalletType {
    #[default]
    Local,
    Passkey,
}

/// Cryptographic key type.
#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum KeyType {
    #[default]
    Secp256k1,
    P256,
    WebAuthn,
}

/// A single entry from Tempo's `keys.toml`.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct KeyEntry {
    #[serde(default)]
    wallet_type: WalletType,
    #[serde(default)]
    wallet_address: Address,
    #[serde(default)]
    chain_id: u64,
    #[serde(default)]
    key_type: KeyType,
    #[serde(default)]
    key_address: Option<Address>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    key_authorization: Option<String>,
    #[serde(default)]
    expiry: Option<u64>,
    #[serde(default)]
    limits: Vec<StoredTokenLimit>,
}

/// Per-token spending limit stored in `keys.toml`.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct StoredTokenLimit {
    currency: Address,
    limit: String,
}

/// The top-level structure of `~/.tempo/wallet/keys.toml`.
#[derive(serde::Deserialize)]
struct KeysFile {
    #[serde(default)]
    keys: Vec<KeyEntry>,
}

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

/// Returns the path to Tempo's keys file.
///
/// Respects `TEMPO_HOME` env var, defaulting to `~/.tempo`.
fn keys_path() -> Option<PathBuf> {
    let base = std::env::var_os("TEMPO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".tempo")))?;
    Some(base.join("wallet").join("keys.toml"))
}

/// Decodes a hex-encoded, RLP-encoded [`SignedKeyAuthorization`].
fn decode_key_authorization(hex_str: &str) -> Result<SignedKeyAuthorization> {
    let bytes = hex::decode(hex_str)?;
    let auth = SignedKeyAuthorization::decode(&mut bytes.as_slice())?;
    Ok(auth)
}

/// Looks up a signer for the given address in Tempo's `keys.toml`.
///
/// Returns [`TempoLookup::Direct`] if a direct-mode (EOA) key is found,
/// [`TempoLookup::Keychain`] if a keychain-mode access key is found,
/// or [`TempoLookup::NotFound`] if no entry matches.
pub fn lookup_signer(from: Address) -> Result<TempoLookup> {
    let path = match keys_path() {
        Some(p) if p.is_file() => p,
        _ => return Ok(TempoLookup::NotFound),
    };

    let contents = std::fs::read_to_string(&path)?;
    let file: KeysFile = toml::from_str(&contents)?;

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
        let key_authorization =
            entry.key_authorization.as_deref().map(decode_key_authorization).transpose()?;

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
