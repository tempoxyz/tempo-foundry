//! Tempo wallet keystore integration.
//!
//! Reads keys from the Tempo CLI wallet keystore (`$TEMPO_HOME/wallet/keys.toml`,
//! defaulting to `~/.tempo/wallet/keys.toml`) and resolves a signer by matching
//! the `--from` address against `wallet_address` or `key_address` entries.

use alloy_primitives::Address;
use alloy_signer::Signer;
use eyre::Result;
use serde::Deserialize;
use std::{env, path::PathBuf};

use crate::{WalletSigner, utils::create_private_key_signer};

/// A single key entry from Tempo's `keys.toml`.
#[derive(Debug, Deserialize)]
struct KeyEntry {
    #[serde(default)]
    wallet_address: String,
    #[serde(default)]
    key_address: Option<String>,
    #[serde(default)]
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Keystore {
    #[serde(default)]
    keys: Vec<KeyEntry>,
}

/// Returns the path to the Tempo wallet keystore file.
fn keystore_path() -> Option<PathBuf> {
    let base = env::var("TEMPO_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".tempo")))?;
    let path = base.join("wallet").join("keys.toml");
    path.is_file().then_some(path)
}

/// Try to resolve a signer from the Tempo wallet keystore for the given address.
///
/// Reads `$TEMPO_HOME/wallet/keys.toml` (default `~/.tempo/wallet/keys.toml`) and looks
/// for a key entry whose `wallet_address` or `key_address` matches `sender`. If a match
/// with an inline private key is found, returns the corresponding [`WalletSigner`].
pub fn try_resolve_tempo_signer(sender: Address) -> Result<Option<WalletSigner>> {
    let Some(path) = keystore_path() else {
        trace!("tempo keystore not found");
        return Ok(None);
    };

    trace!(?path, "reading tempo keystore");
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            warn!(?path, %e, "failed to read tempo keystore, skipping");
            return Ok(None);
        }
    };
    match resolve_from_toml(&contents, sender) {
        Ok(result) => Ok(result),
        Err(e) => {
            warn!(?path, %e, "failed to parse tempo keystore, skipping");
            Ok(None)
        }
    }
}

/// Resolve a signer from raw TOML keystore contents.
fn resolve_from_toml(contents: &str, sender: Address) -> Result<Option<WalletSigner>> {
    let keystore: Keystore = toml::from_str(contents)?;
    let sender_lower = format!("{sender:#x}");

    for entry in &keystore.keys {
        let wallet_match =
            !entry.wallet_address.is_empty() && entry.wallet_address.to_lowercase() == sender_lower;
        let key_match =
            entry.key_address.as_deref().is_some_and(|addr| addr.to_lowercase() == sender_lower);

        if (wallet_match || key_match) && entry.key.as_ref().is_some_and(|k| !k.is_empty()) {
            let signer = create_private_key_signer(entry.key.as_ref().unwrap())?;
            // Only return the signer if the derived address matches the requested sender.
            // This prevents returning a signer for a different address (e.g. when
            // wallet_address != key_address in keychain-style entries).
            if signer.address() == sender {
                trace!("found matching key in tempo keystore");
                return Ok(Some(signer));
            }
            trace!(
                derived = %signer.address(),
                requested = %sender,
                "tempo keystore entry matched but derived address differs, skipping"
            );
        }
    }

    Ok(None)
}

/// Helper to resolve home directory.
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const TEST_ADDR: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";

    #[test]
    fn resolves_by_wallet_address() {
        let toml = format!(
            r#"[[keys]]
wallet_address = "{TEST_ADDR}"
key = "{TEST_KEY}"
"#
        );
        let sender: Address = TEST_ADDR.parse().unwrap();
        let signer = resolve_from_toml(&toml, sender).unwrap().unwrap();
        assert_eq!(signer.address(), sender);
    }

    #[test]
    fn resolves_by_key_address() {
        let toml = format!(
            r#"[[keys]]
wallet_address = "0x0000000000000000000000000000000000000001"
key_address = "{TEST_ADDR}"
key = "{TEST_KEY}"
"#
        );
        let sender: Address = TEST_ADDR.parse().unwrap();
        let signer = resolve_from_toml(&toml, sender).unwrap().unwrap();
        assert_eq!(signer.address(), sender);
    }

    #[test]
    fn returns_none_when_no_match() {
        let toml = format!(
            r#"[[keys]]
wallet_address = "0x1111111111111111111111111111111111111111"
key = "{TEST_KEY}"
"#
        );
        let sender: Address = "0x2222222222222222222222222222222222222222".parse().unwrap();
        assert!(resolve_from_toml(&toml, sender).unwrap().is_none());
    }

    #[test]
    fn skips_entry_without_key() {
        let toml = format!(
            r#"[[keys]]
wallet_type = "passkey"
wallet_address = "{TEST_ADDR}"
key_type = "webauthn"
"#
        );
        let sender: Address = TEST_ADDR.parse().unwrap();
        assert!(resolve_from_toml(&toml, sender).unwrap().is_none());
    }

    #[test]
    fn case_insensitive_match() {
        let toml = format!(
            r#"[[keys]]
wallet_address = "0xF39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
key = "{TEST_KEY}"
"#
        );
        let sender: Address = TEST_ADDR.parse().unwrap();
        assert!(resolve_from_toml(&toml, sender).unwrap().is_some());
    }

    #[test]
    fn empty_keystore() {
        let sender: Address = TEST_ADDR.parse().unwrap();
        assert!(resolve_from_toml("", sender).unwrap().is_none());
    }

    #[test]
    fn skips_when_wallet_address_differs_from_derived_key() {
        // wallet_address matches sender, but the private key derives to a different address.
        // This simulates a keychain-style entry where wallet_address != key_address.
        let other_addr = "0x0000000000000000000000000000000000000042";
        let toml = format!(
            r#"[[keys]]
wallet_address = "{other_addr}"
key_address = "{TEST_ADDR}"
key = "{TEST_KEY}"
"#
        );
        // Looking up by wallet_address (0x42) — the key derives to TEST_ADDR, not 0x42.
        let sender: Address = other_addr.parse().unwrap();
        assert!(resolve_from_toml(&toml, sender).unwrap().is_none());
    }
}
