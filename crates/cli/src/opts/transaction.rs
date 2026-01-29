use std::str::FromStr;

use alloy_eips::{eip2930::AccessList, eip7702::SignedAuthorization};
use alloy_primitives::{hex, Address, U256, U64};
use alloy_rlp::Decodable;
use clap::Parser;

use crate::utils::{parse_ether_value, parse_json};

/// CLI helper to parse a EIP-7702 authorization list.
/// Can be either a hex-encoded signed authorization or an address.
#[derive(Clone, Debug)]
pub enum CliAuthorizationList {
    /// If an address is provided, we sign the authorization delegating to provided address.
    Address(Address),
    /// If RLP-encoded authorization is provided, we decode it and attach to transaction.
    Signed(SignedAuthorization),
}

impl FromStr for CliAuthorizationList {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = Address::from_str(s) {
            Ok(Self::Address(addr))
        } else if let Ok(auth) = SignedAuthorization::decode(&mut hex::decode(s)?.as_ref()) {
            Ok(Self::Signed(auth))
        } else {
            eyre::bail!("Failed to decode authorization")
        }
    }
}

#[derive(Clone, Debug, Parser)]
#[command(next_help_heading = "Transaction options")]
pub struct TransactionOpts {
    /// Gas limit for the transaction.
    #[arg(long, env = "ETH_GAS_LIMIT")]
    pub gas_limit: Option<U256>,

    /// Gas price for legacy transactions, or max fee per gas for EIP1559 transactions, either
    /// specified in wei, or as a string with a unit type.
    ///
    /// Examples: 1ether, 10gwei, 0.01ether
    #[arg(
        long,
        env = "ETH_GAS_PRICE",
        value_parser = parse_ether_value,
        value_name = "PRICE"
    )]
    pub gas_price: Option<U256>,

    /// Max priority fee per gas for EIP1559 transactions.
    #[arg(
        long,
        env = "ETH_PRIORITY_GAS_PRICE",
        value_parser = parse_ether_value,
        value_name = "PRICE"
    )]
    pub priority_gas_price: Option<U256>,

    /// Ether to send in the transaction, either specified in wei, or as a string with a unit type.
    ///
    ///
    ///
    /// Examples: 1ether, 10gwei, 0.01ether
    #[arg(long, value_parser = parse_ether_value)]
    pub value: Option<U256>,

    /// Nonce for the transaction.
    #[arg(long)]
    pub nonce: Option<U64>,

    /// Nonce key for 2D nonce support (Tempo parallelizable nonces).
    ///
    /// Allows multiple transactions with the same nonce but different keys
    /// to be executed in parallel.
    #[arg(long, value_name = "NONCE_KEY")]
    pub nonce_key: Option<U256>,

    /// Use expiring nonce mode (TIP-1009).
    ///
    /// Automatically sets nonce-key to U256::MAX and nonce to 0.
    /// Requires --valid-before to be set. Expiring nonces use transaction hash
    /// for replay protection instead of sequential nonces, avoiding state bloat.
    #[arg(long, requires = "valid_before")]
    pub expiring_nonce: bool,

    /// Transaction valid before timestamp (Tempo expiring nonces).
    ///
    /// Transaction can only be included in a block before this timestamp.
    /// Required when using --expiring-nonce. Maximum expiry window is 30 seconds.
    #[arg(long, value_name = "TIMESTAMP")]
    pub valid_before: Option<u64>,

    /// Transaction valid after timestamp (Tempo expiring nonces).
    ///
    /// Transaction can only be included in a block after this timestamp.
    /// Must be less than --valid-before if both are set.
    #[arg(long, value_name = "TIMESTAMP")]
    pub valid_after: Option<u64>,

    /// Send a legacy transaction instead of an EIP1559 transaction.
    ///
    /// This is automatically enabled for common networks without EIP1559.
    #[arg(long)]
    pub legacy: bool,

    /// Send a EIP-4844 blob transaction.
    #[arg(long, conflicts_with = "legacy")]
    pub blob: bool,

    /// Gas price for EIP-4844 blob transaction.
    #[arg(long, conflicts_with = "legacy", value_parser = parse_ether_value, env = "ETH_BLOB_GAS_PRICE", value_name = "BLOB_PRICE")]
    pub blob_gas_price: Option<U256>,

    /// EIP-7702 authorization list.
    ///
    /// Can be either a hex-encoded signed authorization or an address.
    #[arg(long, conflicts_with_all = &["legacy", "blob"])]
    pub auth: Vec<CliAuthorizationList>,

    /// EIP-2930 access list.
    ///
    /// Accepts either a JSON-encoded access list or an empty value to create the access list
    /// via an RPC call to `eth_createAccessList`. To retrieve only the access list portion, use
    /// the `cast access-list` command.
    #[arg(long, value_parser = parse_json::<AccessList>)]
    pub access_list: Option<Option<AccessList>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_priority_gas_tx_opts() {
        let args: TransactionOpts =
            TransactionOpts::parse_from(["foundry-cli", "--priority-gas-price", "100"]);
        assert!(args.priority_gas_price.is_some());
    }
}
