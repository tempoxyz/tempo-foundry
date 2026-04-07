use std::str::FromStr;

use crate::{
    cmd::send::cast_send,
    tempo::{is_key_provisioned, sign_with_access_key},
    tx::{CastTxSender, SendTxOpts, get_provider_with_wallet},
};
use alloy_network::TransactionBuilder;
use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_provider::Provider;
use alloy_sol_types::SolCall;
use clap::Parser;
use eyre::bail;
use foundry_cli::{
    opts::RpcOpts,
    utils::{LoadConfig, get_provider, get_tempo_provider_with_curl, is_tempo_devnet_chain},
};
use foundry_common::shell;
use foundry_config::Chain;
use foundry_wallets::{
    ACCOUNT_KEYCHAIN_ADDRESS, WalletSigner, authorize_key_calldata, revoke_key_calldata,
    update_spending_limit_calldata,
};
use tempo_alloy::{TempoNetwork, provider::TempoProviderExt, rpc::TempoTransactionRequest};
use tempo_contracts::precompiles::{
    IAccountKeychain::{CallScope, KeyRestrictions, SelectorRule, SignatureType, TokenLimit},
    ITIP20,
};

use super::erc20::{Erc20TxOpts, apply_tempo_tx_opts};

/// Interact with the Tempo Account Keychain precompile.
#[derive(Debug, Parser, Clone)]
pub enum KeychainSubcommand {
    /// Authorize a new access key for the caller's account
    #[command(visible_alias = "auth")]
    Authorize {
        /// The key identifier (address derived from public key)
        key_id: Address,

        /// Signature type: secp256k1, p256, or webauthn
        #[arg(value_parser = parse_signature_type)]
        signature_type: SignatureType,

        /// Block timestamp when key expires (0 for never)
        expiry: u64,

        /// Spending limit (repeatable). Format: TOKEN:AMOUNT or TOKEN:AMOUNT:PERIOD_SECONDS
        #[arg(long, value_name = "TOKEN:AMOUNT[:PERIOD]")]
        limit: Vec<String>,

        /// Call scope restriction (repeatable). Format: ADDRESS or ADDRESS:SELECTORS
        ///
        /// SELECTORS is a comma-separated list of: transfer, transfer_with_memo, approve.
        ///
        /// Examples:
        ///   --scope 0xTIP20:transfer,approve   (scoped to transfer+approve on this TIP-20)
        ///   --scope 0xDEX                       (unrestricted calls to this address)
        #[arg(long, value_name = "ADDRESS[:SELECTORS]")]
        scope: Vec<String>,

        /// Call scope restrictions as JSON array (mutually exclusive with --scope).
        ///
        /// Format: [{"target":"0x...","selectors":["transfer","approve"]}, ...]
        #[arg(long, value_name = "JSON", conflicts_with = "scope")]
        scopes: Option<String>,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Permanently revoke an access key
    #[command(visible_alias = "rev")]
    Revoke {
        /// The key identifier to revoke
        key_id: Address,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Update spending limit for a key-token pair
    #[command(visible_alias = "ul")]
    UpdateLimit {
        /// The key identifier
        key_id: Address,

        /// The token address
        token: Address,

        /// The new spending limit
        new_limit: U256,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Query access key information
    #[command(visible_alias = "info")]
    KeyInfo {
        /// The account that owns the key
        account: Address,

        /// The key identifier to query
        key_id: Address,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Query remaining spending limit
    #[command(visible_alias = "rl")]
    RemainingLimit {
        /// The account that owns the key
        account: Address,

        /// The key identifier
        key_id: Address,

        /// The token address
        token: Address,

        #[command(flatten)]
        rpc: RpcOpts,
    },
}

impl KeychainSubcommand {
    fn rpc(&self) -> &RpcOpts {
        match self {
            Self::Authorize { send_tx, .. }
            | Self::Revoke { send_tx, .. }
            | Self::UpdateLimit { send_tx, .. } => &send_tx.eth.rpc,
            Self::KeyInfo { rpc, .. } | Self::RemainingLimit { rpc, .. } => rpc,
        }
    }

    pub async fn run(self) -> eyre::Result<()> {
        // Resolve signer + access key for write commands.
        let (signer, tempo_access_key) = match &self {
            Self::Authorize { send_tx, .. }
            | Self::Revoke { send_tx, .. }
            | Self::UpdateLimit { send_tx, .. } => {
                if send_tx.eth.wallet.from.is_some() {
                    send_tx.eth.wallet.maybe_signer().await?
                } else {
                    (None, None)
                }
            }
            _ => (None, None),
        };

        let config = self.rpc().load_config()?;

        // Check we're on a Tempo chain before doing anything.
        let chain_id = get_provider(&config)?.get_chain_id().await?;
        if !is_tempo_chain(chain_id) {
            bail!("cast keychain is only supported on Tempo networks");
        }

        // Macro to DRY the keychain-vs-normal send pattern for state-changing ops.
        macro_rules! keychain_send {
            ($send_tx:expr, $tx_opts:expr, $calldata:expr) => {{
                let timeout = $send_tx.timeout.unwrap_or(config.transaction_timeout);
                let is_legacy = config.chain.is_some_and(|c| c.is_legacy());
                let mut tx = build_keychain_tx($calldata);
                apply_tempo_tx_opts(&mut tx, &$tx_opts, is_legacy);

                if let Some(ref access_key) = tempo_access_key {
                    let signer = signer.as_ref().expect("signer required for access key");
                    let provider = get_tempo_provider_with_curl(&config, $send_tx.eth.rpc.curl)?;
                    tx.key_id = Some(access_key.key_address);
                    tx.set_from(access_key.wallet_address);
                    send_keychain_with_access_key(
                        &provider,
                        tx,
                        signer,
                        access_key,
                        $send_tx.cast_async,
                        $send_tx.sync,
                        $send_tx.confirmations,
                        timeout,
                    )
                    .await?
                } else {
                    let provider = if signer.is_some() {
                        let (s, _) = $send_tx.eth.wallet.maybe_signer().await?;
                        let config = $send_tx.eth.load_config()?;
                        let wallet =
                            alloy_network::EthereumWallet::new(s.expect("signer was Some"));
                        foundry_cli::utils::get_tempo_provider_builder(
                            &config,
                            $send_tx.eth.rpc.curl,
                        )?
                        .build_with_wallet(wallet)?
                    } else {
                        get_provider_with_wallet(&$send_tx, $send_tx.eth.rpc.curl).await?
                    };
                    send_keychain_tx(provider, tx, &$send_tx, timeout).await?
                }
            }};
        }

        match self {
            // --- Read-only (via TempoProviderExt) ---
            Self::KeyInfo { account, key_id, .. } => {
                let provider = get_tempo_provider_with_curl(&config, false)?;
                let key = provider.get_keychain_key(account, key_id).await?;

                let sig_type_str = match key.signatureType {
                    SignatureType::Secp256k1 => "secp256k1",
                    SignatureType::P256 => "p256",
                    SignatureType::WebAuthn => "webauthn",
                    _ => "unknown",
                };

                if shell::is_json() {
                    sh_println!(
                        "{}",
                        serde_json::json!({
                            "keyId": format!("{}", key.keyId),
                            "signatureType": sig_type_str,
                            "expiry": key.expiry,
                            "enforceLimits": key.enforceLimits,
                            "isRevoked": key.isRevoked,
                        })
                    )?;
                } else {
                    sh_println!("Key ID:          {}", key.keyId)?;
                    sh_println!("Signature Type:  {sig_type_str}")?;
                    sh_println!("Expiry:          {}", key.expiry)?;
                    sh_println!("Enforce Limits:  {}", key.enforceLimits)?;
                    sh_println!("Revoked:         {}", key.isRevoked)?;
                }
            }

            Self::RemainingLimit { account, key_id, token, .. } => {
                let provider = get_tempo_provider_with_curl(&config, false)?;
                let remaining =
                    provider.get_keychain_remaining_limit(account, key_id, token).await?;

                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&remaining.to_string())?)?;
                } else {
                    sh_println!("{}", remaining)?;
                }
            }

            // --- State-changing (via calldata builders) ---
            Self::Authorize {
                key_id,
                signature_type,
                expiry,
                limit,
                scope,
                scopes,
                send_tx,
                tx: tx_opts,
            } => {
                let limits: Vec<TokenLimit> =
                    limit.iter().map(|l| parse_token_limit(l)).collect::<eyre::Result<_>>()?;
                let enforce_limits = !limits.is_empty();
                let expiry = if expiry == 0 { u64::MAX } else { expiry };

                let allowed_calls = parse_call_scopes(&scope, scopes.as_deref())?;
                let allow_any_calls = allowed_calls.is_none();

                let config = KeyRestrictions {
                    expiry,
                    enforceLimits: enforce_limits,
                    limits,
                    allowAnyCalls: allow_any_calls,
                    allowedCalls: allowed_calls.unwrap_or_default(),
                };

                let calldata = authorize_key_calldata(key_id, signature_type, config);
                keychain_send!(send_tx, tx_opts, calldata)
            }

            Self::Revoke { key_id, send_tx, tx: tx_opts } => {
                let calldata = revoke_key_calldata(key_id);
                keychain_send!(send_tx, tx_opts, calldata)
            }

            Self::UpdateLimit { key_id, token, new_limit, send_tx, tx: tx_opts } => {
                let calldata = update_spending_limit_calldata(key_id, token, new_limit);
                keychain_send!(send_tx, tx_opts, calldata)
            }
        }

        Ok(())
    }
}

// --- Private helpers ---

/// Returns true if the chain_id belongs to a Tempo network.
fn is_tempo_chain(chain_id: u64) -> bool {
    Chain::from(chain_id).named().is_some_and(|c| c.is_tempo()) || is_tempo_devnet_chain(chain_id)
}

/// Parse a signature type string into the sol enum variant.
fn parse_signature_type(s: &str) -> Result<SignatureType, String> {
    match s.to_lowercase().as_str() {
        "secp256k1" => Ok(SignatureType::Secp256k1),
        "p256" => Ok(SignatureType::P256),
        "webauthn" => Ok(SignatureType::WebAuthn),
        _ => Err(format!("unknown signature type '{s}', expected: secp256k1, p256, webauthn")),
    }
}

/// Parse "0xToken:1000000" or "0xToken:1000000:3600" into a `TokenLimit`.
fn parse_token_limit(s: &str) -> eyre::Result<TokenLimit> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() < 2 {
        eyre::bail!("invalid limit format '{s}', expected TOKEN:AMOUNT or TOKEN:AMOUNT:PERIOD");
    }

    let token = Address::from_str(parts[0])
        .map_err(|e| eyre::eyre!("invalid token address '{}': {e}", parts[0]))?;
    let amount =
        U256::from_str(parts[1]).map_err(|e| eyre::eyre!("invalid amount '{}': {e}", parts[1]))?;
    let period = if parts.len() == 3 {
        parts[2].parse::<u64>().map_err(|e| eyre::eyre!("invalid period '{}': {e}", parts[2]))?
    } else {
        0
    };

    Ok(TokenLimit { token, amount, period })
}

/// Parse a named selector into a 4-byte function selector.
fn parse_selector_name(s: &str) -> eyre::Result<FixedBytes<4>> {
    match s.to_lowercase().as_str() {
        "transfer" => Ok(ITIP20::transferCall::SELECTOR.into()),
        "transfer_with_memo" => Ok(ITIP20::transferWithMemoCall::SELECTOR.into()),
        "approve" => Ok(ITIP20::approveCall::SELECTOR.into()),
        _ => eyre::bail!("unknown selector '{s}', expected: transfer, transfer_with_memo, approve"),
    }
}

/// Parse a single `--scope` value: `ADDRESS` or `ADDRESS:selector1,selector2,...`
fn parse_scope(s: &str) -> eyre::Result<CallScope> {
    // Split on first colon only — address may contain "0x" but selectors come after first ":"
    let (addr_str, selectors_str) = match s.find(':') {
        // Skip the colon in "0x" prefix — find the colon after the address (42 chars for 0x + 40
        // hex)
        Some(pos) if pos < 3 => {
            // This is the "0x" colon in the address; look for the next one
            match s[pos + 1..].find(':') {
                Some(next) => {
                    let split = pos + 1 + next;
                    (&s[..split], Some(&s[split + 1..]))
                }
                None => (s, None),
            }
        }
        Some(pos) => (&s[..pos], Some(&s[pos + 1..])),
        None => (s, None),
    };

    let target = Address::from_str(addr_str)
        .map_err(|e| eyre::eyre!("invalid address '{addr_str}': {e}"))?;

    let selector_rules = match selectors_str {
        Some(sel_str) if !sel_str.is_empty() => sel_str
            .split(',')
            .map(|name| {
                let selector = parse_selector_name(name.trim())?;
                Ok(SelectorRule { selector, recipients: vec![] })
            })
            .collect::<eyre::Result<Vec<_>>>()?,
        _ => vec![],
    };

    Ok(CallScope { target, selectorRules: selector_rules })
}

/// JSON representation for `--scopes`.
#[derive(serde::Deserialize)]
struct JsonCallScope {
    target: Address,
    #[serde(default)]
    selectors: Vec<String>,
}

/// Parse call scopes from `--scope` flags or `--scopes` JSON.
///
/// Returns `None` if no scopes were provided (= allow any calls).
fn parse_call_scopes(
    scope: &[String],
    scopes_json: Option<&str>,
) -> eyre::Result<Option<Vec<CallScope>>> {
    if let Some(json) = scopes_json {
        let entries: Vec<JsonCallScope> =
            serde_json::from_str(json).map_err(|e| eyre::eyre!("invalid --scopes JSON: {e}"))?;
        let call_scopes = entries
            .into_iter()
            .map(|entry| {
                let selector_rules = entry
                    .selectors
                    .iter()
                    .map(|name| {
                        let selector = parse_selector_name(name)?;
                        Ok(SelectorRule { selector, recipients: vec![] })
                    })
                    .collect::<eyre::Result<Vec<_>>>()?;
                Ok(CallScope { target: entry.target, selectorRules: selector_rules })
            })
            .collect::<eyre::Result<Vec<_>>>()?;
        return Ok(Some(call_scopes));
    }

    if scope.is_empty() {
        return Ok(None);
    }

    let call_scopes = scope.iter().map(|s| parse_scope(s)).collect::<eyre::Result<Vec<_>>>()?;
    Ok(Some(call_scopes))
}

/// Build a `TempoTransactionRequest` targeting the keychain precompile.
fn build_keychain_tx(calldata: Vec<u8>) -> TempoTransactionRequest {
    let mut tx = TempoTransactionRequest::default();
    tx.set_to(ACCOUNT_KEYCHAIN_ADDRESS);
    tx.set_input(Bytes::from(calldata));
    tx
}

/// Send a keychain transaction via the standard `cast_send` flow (handles async/sync/receipt).
async fn send_keychain_tx<P: Provider<TempoNetwork>>(
    provider: P,
    tx: TempoTransactionRequest,
    send_tx: &SendTxOpts,
    timeout: u64,
) -> eyre::Result<()> {
    cast_send(provider, tx, send_tx.cast_async, send_tx.sync, send_tx.confirmations, timeout).await
}

/// Sends a keychain transaction using access key (keychain mode).
#[allow(clippy::too_many_arguments)]
async fn send_keychain_with_access_key<P: Provider<TempoNetwork>>(
    provider: &P,
    mut tx: TempoTransactionRequest,
    signer: &WalletSigner,
    access_key: &foundry_wallets::TempoAccessKeyConfig,
    cast_async: bool,
    sync: bool,
    confirmations: u64,
    timeout: u64,
) -> eyre::Result<()> {
    let from = access_key.wallet_address;
    tx.set_from(from);
    tx.set_chain_id(provider.get_chain_id().await?);

    if tx.nonce().is_none() {
        tx.set_nonce(provider.get_transaction_count(from).await?);
    }
    if tx.nonce_key.is_none() {
        tx.set_nonce_key(alloy_primitives::U256::ZERO);
    }

    let estimate = provider.estimate_eip1559_fees().await?;
    if tx.max_fee_per_gas().is_none() {
        tx.set_max_fee_per_gas(estimate.max_fee_per_gas);
    }
    if tx.max_priority_fee_per_gas().is_none() {
        tx.set_max_priority_fee_per_gas(estimate.max_priority_fee_per_gas);
    }

    let key_already_provisioned =
        is_key_provisioned(provider, access_key.wallet_address, access_key.key_address).await;

    if !key_already_provisioned && let Some(ref auth) = access_key.key_authorization {
        tx.key_authorization = Some(auth.clone());
    }

    if tx.gas_limit().is_none() {
        let gas = provider.estimate_gas(tx.clone()).await?;
        tx.set_gas_limit(gas);
    }

    let raw_tx = sign_with_access_key(tx, signer, access_key.wallet_address).await?;

    let cast = CastTxSender::new(provider);

    if sync {
        let receipt = cast.send_raw_sync(&raw_tx).await?;
        sh_println!("{receipt}")?;
    } else {
        let pending_tx = provider.send_raw_transaction(&raw_tx).await?;
        let tx_hash = pending_tx.tx_hash();
        if cast_async {
            sh_println!("{tx_hash:#x}")?;
        } else {
            let receipt = cast
                .receipt(format!("{tx_hash:#x}"), None, confirmations, Some(timeout), false)
                .await?;
            sh_println!("{receipt}")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_signature_type() {
        assert_eq!(parse_signature_type("secp256k1").unwrap(), SignatureType::Secp256k1);
        assert_eq!(parse_signature_type("P256").unwrap(), SignatureType::P256);
        assert_eq!(parse_signature_type("WebAuthn").unwrap(), SignatureType::WebAuthn);
        assert!(parse_signature_type("ed25519").is_err());
        assert!(parse_signature_type("").is_err());
    }

    #[test]
    fn test_parse_token_limit() {
        let limit =
            parse_token_limit("0x0000000000000000000000000000000000000001:1000000").unwrap();
        assert_eq!(
            limit.token,
            Address::from_str("0x0000000000000000000000000000000000000001").unwrap()
        );
        assert_eq!(limit.amount, U256::from(1000000));
        assert_eq!(limit.period, 0);

        // With period
        let limit =
            parse_token_limit("0x0000000000000000000000000000000000000001:1000000:3600").unwrap();
        assert_eq!(limit.period, 3600);

        // Invalid formats
        assert!(parse_token_limit("no_colon").is_err());
        assert!(parse_token_limit("not_an_address:100").is_err());
        assert!(
            parse_token_limit("0x0000000000000000000000000000000000000001:not_a_number").is_err()
        );
    }

    #[test]
    fn test_parse_selector_name() {
        assert_eq!(
            parse_selector_name("transfer").unwrap(),
            FixedBytes::from(ITIP20::transferCall::SELECTOR)
        );
        assert_eq!(
            parse_selector_name("transfer_with_memo").unwrap(),
            FixedBytes::from(ITIP20::transferWithMemoCall::SELECTOR)
        );
        assert_eq!(
            parse_selector_name("approve").unwrap(),
            FixedBytes::from(ITIP20::approveCall::SELECTOR)
        );
        assert!(parse_selector_name("unknown").is_err());
        assert!(parse_selector_name("0xaabbccdd").is_err());
    }

    #[test]
    fn test_parse_scope() {
        // Address only — unrestricted
        let scope = parse_scope("0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D").unwrap();
        assert_eq!(
            scope.target,
            Address::from_str("0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D").unwrap()
        );
        assert!(scope.selectorRules.is_empty());

        // With named selectors
        let scope =
            parse_scope("0x20c0000000000000000000000000000000000001:transfer,approve").unwrap();
        assert_eq!(scope.selectorRules.len(), 2);
        assert_eq!(
            scope.selectorRules[0].selector,
            FixedBytes::from(ITIP20::transferCall::SELECTOR)
        );
        assert_eq!(
            scope.selectorRules[1].selector,
            FixedBytes::from(ITIP20::approveCall::SELECTOR)
        );
    }

    #[test]
    fn test_parse_call_scopes() {
        // No scopes = None (allow any calls)
        assert!(parse_call_scopes(&[], None).unwrap().is_none());

        // From --scope flags
        let scopes = vec![
            "0x20c0000000000000000000000000000000000001:transfer".to_string(),
            "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D".to_string(),
        ];
        let result = parse_call_scopes(&scopes, None).unwrap().unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].selectorRules.len(), 1);
        assert!(result[1].selectorRules.is_empty());

        // From --scopes JSON
        let json = r#"[{"target":"0x20c0000000000000000000000000000000000001","selectors":["transfer","approve"]},{"target":"0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D"}]"#;
        let result = parse_call_scopes(&[], Some(json)).unwrap().unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].selectorRules.len(), 2);
        assert!(result[1].selectorRules.is_empty());
    }
}
