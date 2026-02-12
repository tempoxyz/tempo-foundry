use alloy_primitives::{Address, Signature, ruint::aliases::U256};
use clap::Parser;
use std::str::FromStr;

/// Parse a hex-encoded signature string into a Signature.
fn parse_signature(sig_hex: &str) -> eyre::Result<Signature> {
    let sig_hex = sig_hex.strip_prefix("0x").unwrap_or(sig_hex);
    Signature::from_str(sig_hex).map_err(|e| eyre::eyre!("Invalid signature: {e}"))
}

/// CLI options for Tempo transactions.
#[derive(Clone, Debug, Default, Parser)]
#[command(next_help_heading = "Tempo")]
pub struct TempoOpts {
    /// Fee token address for Tempo transactions.
    ///
    /// When set, builds a Tempo (type 0x76) transaction that pays gas fees
    /// in the specified token.
    ///
    /// If this is not set, the fee token is chosen according to network rules. See the Tempo docs
    /// for more information.
    #[arg(long = "tempo.fee-token")]
    pub fee_token: Option<Address>,

    /// Nonce sequence key for Tempo transactions.
    ///
    /// When set, builds a Tempo (type 0x76) transaction with the specified nonce sequence key.
    ///
    /// If this is not set, the protocol sequence key (0) will be used.
    ///
    /// For more information see <https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#parallelizable-nonces>.
    #[arg(long = "tempo.seq")]
    pub sequence_key: Option<U256>,

    /// Pre-signed sponsor signature for sponsored (gasless) transactions.
    ///
    /// Hex-encoded signature (with or without 0x prefix) that commits the sponsor
    /// to paying gas fees. The signature must be over the fee_payer_signature_hash
    /// which can be obtained using --tempo.print-sponsor-hash.
    #[arg(
        long = "tempo.sponsor-signature",
        value_name = "SIGNATURE",
        env = "TEMPO_SPONSOR_SIGNATURE"
    )]
    pub sponsor_signature: Option<String>,

    /// Print the fee_payer_signature_hash and exit without sending.
    ///
    /// Use this to obtain the hash that the sponsor must sign. The sponsor signs
    /// this hash with their private key, then provides it via --tempo.sponsor-signature.
    #[arg(long = "tempo.print-sponsor-hash")]
    pub print_sponsor_hash: bool,

    /// Nonce key for 2D nonce support (Tempo parallelizable nonces).
    ///
    /// Allows multiple transactions with the same nonce but different keys
    /// to be executed in parallel.
    #[arg(long = "tempo.nonce-key", value_name = "NONCE_KEY")]
    pub nonce_key: Option<U256>,

    /// Use expiring nonce mode (TIP-1009).
    ///
    /// Automatically sets nonce-key to U256::MAX and nonce to 0.
    /// Requires --tempo.valid-before to be set. Expiring nonces use transaction hash
    /// for replay protection instead of sequential nonces, avoiding state bloat.
    #[arg(long = "tempo.expiring-nonce", requires = "valid_before")]
    pub expiring_nonce: bool,

    /// Transaction valid before timestamp (Tempo expiring nonces).
    ///
    /// Transaction can only be included in a block before this timestamp.
    /// Required when using --tempo.expiring-nonce. Maximum expiry window is 30 seconds.
    #[arg(long = "tempo.valid-before", value_name = "TIMESTAMP")]
    pub valid_before: Option<u64>,

    /// Transaction valid after timestamp (Tempo expiring nonces).
    ///
    /// Transaction can only be included in a block after this timestamp.
    /// Must be less than --tempo.valid-before if both are set.
    #[arg(long = "tempo.valid-after", value_name = "TIMESTAMP")]
    pub valid_after: Option<u64>,
}

impl TempoOpts {
    /// Returns true if sponsor signature is provided (not just print-hash mode).
    pub fn is_sponsor(&self) -> bool {
        self.sponsor_signature.is_some()
    }

    /// Returns true if we should print the sponsor hash and exit.
    pub fn should_print_hash(&self) -> bool {
        self.print_sponsor_hash
    }

    /// Parses the provided sponsor signature.
    pub fn get_signature(&self) -> eyre::Result<Option<Signature>> {
        self.sponsor_signature.as_ref().map(|s| parse_signature(s)).transpose()
    }
}
