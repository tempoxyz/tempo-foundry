use std::str::FromStr;

use crate::{
    cmd::send::cast_send,
    format_uint_exp,
    tempo::iso4217::{is_iso4217_currency, iso4217_warning_message},
    tx::{SendTxOpts, get_provider_with_wallet},
};
use alloy_eips::BlockId;
use alloy_ens::NameOrAddress;
use alloy_network::TransactionBuilder;
use alloy_primitives::{B256, U64, U256};
use alloy_provider::Provider;
use alloy_sol_types::sol;
use clap::{Args, Parser};
use foundry_cli::{
    opts::{RpcOpts, TempoOpts},
    utils::{LoadConfig, get_provider},
};
use foundry_common::shell;
#[doc(hidden)]
pub use foundry_config::{Chain, utils::*};
use tempo_alloy::{TempoNetwork, rpc::TempoTransactionRequest};
use tempo_contracts::precompiles::TIP20_FACTORY_ADDRESS;

sol! {
    #[sol(rpc)]
    interface IERC20 {
        #[derive(Debug)]
        function name() external view returns (string);
        function symbol() external view returns (string);
        function decimals() external view returns (uint8);
        function totalSupply() external view returns (uint256);
        function balanceOf(address owner) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function mint(address to, uint256 amount) external;
        function burn(uint256 amount) external;
    }

    #[sol(rpc)]
    interface ITIP20Factory {
        function createToken(
            string memory name,
            string memory symbol,
            string memory currency,
            address quoteToken,
            address admin,
            bytes32 salt
        ) external returns (address token);
    }
}

/// Transaction options for ERC20 operations.
///
/// This struct contains only the transaction options relevant to ERC20 token interactions
#[derive(Debug, Clone, Args)]
pub struct Erc20TxOpts {
    /// Gas limit for the transaction.
    #[arg(long, env = "ETH_GAS_LIMIT")]
    pub gas_limit: Option<U256>,

    /// Gas price for legacy transactions, or max fee per gas for EIP1559 transactions.
    #[arg(long, env = "ETH_GAS_PRICE")]
    pub gas_price: Option<U256>,

    /// Max priority fee per gas for EIP1559 transactions.
    #[arg(long, env = "ETH_PRIORITY_GAS_PRICE")]
    pub priority_gas_price: Option<U256>,

    /// Nonce for the transaction.
    #[arg(long)]
    pub nonce: Option<U64>,

    #[command(flatten)]
    pub tempo: TempoOpts,
}

/// Apply transaction options to a TempoTransactionRequest for ERC20 operations.
fn apply_tempo_tx_opts(tx: &mut TempoTransactionRequest, tx_opts: &Erc20TxOpts, is_legacy: bool) {
    if let Some(gas_limit) = tx_opts.gas_limit {
        tx.set_gas_limit(gas_limit.to());
    }

    if let Some(gas_price) = tx_opts.gas_price {
        if is_legacy {
            tx.set_gas_price(gas_price.to());
        } else {
            tx.set_max_fee_per_gas(gas_price.to());
        }
    }

    if !is_legacy && let Some(priority_fee) = tx_opts.priority_gas_price {
        tx.set_max_priority_fee_per_gas(priority_fee.to());
    }

    if let Some(nonce) = tx_opts.nonce {
        tx.set_nonce(nonce.to());
    }

    // Apply Tempo-specific options
    tx.fee_token = tx_opts.tempo.fee_token;

    if let Some(nonce_key) = tx_opts.tempo.sequence_key {
        tx.set_nonce_key(nonce_key);
    }
}

/// Send an ERC20 transaction using TempoNetwork provider.
async fn send_erc20_tx<P: Provider<TempoNetwork>>(
    provider: P,
    tx: TempoTransactionRequest,
    send_tx: &SendTxOpts,
    timeout: u64,
) -> eyre::Result<()> {
    cast_send(provider, tx, send_tx.cast_async, send_tx.sync, send_tx.confirmations, timeout).await
}
/// Interact with ERC20 tokens.
#[derive(Debug, Parser, Clone)]
pub enum Erc20Subcommand {
    /// Query ERC20 token balance.
    #[command(visible_alias = "b")]
    Balance {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The owner to query balance for.
        #[arg(value_parser = NameOrAddress::from_str)]
        owner: NameOrAddress,

        /// The block height to query at.
        #[arg(long, short = 'B')]
        block: Option<BlockId>,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Transfer ERC20 tokens.
    #[command(visible_aliases = ["t", "send"])]
    Transfer {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The recipient address.
        #[arg(value_parser = NameOrAddress::from_str)]
        to: NameOrAddress,

        /// The amount to transfer.
        amount: String,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Approve ERC20 token spending.
    #[command(visible_alias = "a")]
    Approve {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The spender address.
        #[arg(value_parser = NameOrAddress::from_str)]
        spender: NameOrAddress,

        /// The amount to approve.
        amount: String,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Query ERC20 token allowance.
    #[command(visible_alias = "al")]
    Allowance {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The owner address.
        #[arg(value_parser = NameOrAddress::from_str)]
        owner: NameOrAddress,

        /// The spender address.
        #[arg(value_parser = NameOrAddress::from_str)]
        spender: NameOrAddress,

        /// The block height to query at.
        #[arg(long, short = 'B')]
        block: Option<BlockId>,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Query ERC20 token name.
    #[command(visible_alias = "n")]
    Name {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The block height to query at.
        #[arg(long, short = 'B')]
        block: Option<BlockId>,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Query ERC20 token symbol.
    #[command(visible_alias = "s")]
    Symbol {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The block height to query at.
        #[arg(long, short = 'B')]
        block: Option<BlockId>,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Query ERC20 token decimals.
    #[command(visible_alias = "d")]
    Decimals {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The block height to query at.
        #[arg(long, short = 'B')]
        block: Option<BlockId>,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Query ERC20 token total supply.
    #[command(visible_alias = "ts")]
    TotalSupply {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The block height to query at.
        #[arg(long, short = 'B')]
        block: Option<BlockId>,

        #[command(flatten)]
        rpc: RpcOpts,
    },

    /// Mint ERC20 tokens (if the token supports minting).
    #[command(visible_alias = "m")]
    Mint {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The recipient address.
        #[arg(value_parser = NameOrAddress::from_str)]
        to: NameOrAddress,

        /// The amount to mint.
        amount: String,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Burn ERC20 tokens.
    #[command(visible_alias = "bu")]
    Burn {
        /// The ERC20 token contract address.
        #[arg(value_parser = NameOrAddress::from_str)]
        token: NameOrAddress,

        /// The amount to burn.
        amount: String,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },

    /// Create a new TIP-20 token via the TIP20Factory.
    #[command(visible_alias = "c")]
    Create {
        /// The token name (e.g. "US Dollar Coin").
        name: String,

        /// The token symbol (e.g. "USDC").
        symbol: String,

        /// The ISO 4217 currency code (e.g. "USD", "EUR", "GBP").
        /// This field is IMMUTABLE after creation and affects fee payment
        /// eligibility, DEX routing, and quote token pairing.
        currency: String,

        /// The TIP-20 quote token address used for exchange pricing.
        #[arg(value_parser = NameOrAddress::from_str)]
        quote_token: NameOrAddress,

        /// The admin address to receive DEFAULT_ADMIN_ROLE on the new token.
        #[arg(value_parser = NameOrAddress::from_str)]
        admin: NameOrAddress,

        /// A unique salt for deterministic address derivation (hex-encoded bytes32).
        salt: B256,

        /// Skip the ISO 4217 currency code validation warning.
        #[arg(long)]
        force: bool,

        #[command(flatten)]
        send_tx: SendTxOpts,

        #[command(flatten)]
        tx: Erc20TxOpts,
    },
}

impl Erc20Subcommand {
    fn rpc(&self) -> &RpcOpts {
        match self {
            Self::Allowance { rpc, .. } => rpc,
            Self::Approve { send_tx, .. } => &send_tx.eth.rpc,
            Self::Balance { rpc, .. } => rpc,
            Self::Transfer { send_tx, .. } => &send_tx.eth.rpc,
            Self::Name { rpc, .. } => rpc,
            Self::Symbol { rpc, .. } => rpc,
            Self::Decimals { rpc, .. } => rpc,
            Self::TotalSupply { rpc, .. } => rpc,
            Self::Mint { send_tx, .. } => &send_tx.eth.rpc,
            Self::Burn { send_tx, .. } => &send_tx.eth.rpc,
            Self::Create { send_tx, .. } => &send_tx.eth.rpc,
        }
    }

    pub async fn run(self) -> eyre::Result<()> {
        let config = self.rpc().load_config()?;

        match self {
            // Read-only
            Self::Allowance { token, owner, spender, block, .. } => {
                let provider = get_provider(&config)?;
                let token = token.resolve(&provider).await?;
                let owner = owner.resolve(&provider).await?;
                let spender = spender.resolve(&provider).await?;

                let allowance = IERC20::new(token, &provider)
                    .allowance(owner, spender)
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;

                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&allowance.to_string())?)?
                } else {
                    sh_println!("{}", format_uint_exp(allowance))?
                }
            }
            Self::Balance { token, owner, block, .. } => {
                let provider = get_provider(&config)?;
                let token = token.resolve(&provider).await?;
                let owner = owner.resolve(&provider).await?;

                let balance = IERC20::new(token, &provider)
                    .balanceOf(owner)
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;

                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&balance.to_string())?)?
                } else {
                    sh_println!("{}", format_uint_exp(balance))?
                }
            }
            Self::Name { token, block, .. } => {
                let provider = get_provider(&config)?;
                let token = token.resolve(&provider).await?;

                let name = IERC20::new(token, &provider)
                    .name()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;

                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&name)?)?
                } else {
                    sh_println!("{}", name)?
                }
            }
            Self::Symbol { token, block, .. } => {
                let provider = get_provider(&config)?;
                let token = token.resolve(&provider).await?;

                let symbol = IERC20::new(token, &provider)
                    .symbol()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;

                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&symbol)?)?
                } else {
                    sh_println!("{}", symbol)?
                }
            }
            Self::Decimals { token, block, .. } => {
                let provider = get_provider(&config)?;
                let token = token.resolve(&provider).await?;

                let decimals = IERC20::new(token, &provider)
                    .decimals()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;
                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&decimals)?)?
                } else {
                    sh_println!("{}", decimals)?
                }
            }
            Self::TotalSupply { token, block, .. } => {
                let provider = get_provider(&config)?;
                let token = token.resolve(&provider).await?;

                let total_supply = IERC20::new(token, &provider)
                    .totalSupply()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;

                if shell::is_json() {
                    sh_println!("{}", serde_json::to_string(&total_supply.to_string())?)?
                } else {
                    sh_println!("{}", format_uint_exp(total_supply))?
                }
            }
            // State-changing
            Self::Transfer { token, to, amount, send_tx, tx: tx_opts, .. } => {
                let provider = get_provider_with_wallet(&send_tx, send_tx.eth.rpc.curl).await?;
                let is_legacy = config.chain.is_some_and(|c| c.is_legacy());
                let mut tx = IERC20::new(token.resolve(&provider).await?, &provider)
                    .transfer(to.resolve(&provider).await?, U256::from_str(&amount)?)
                    .into_transaction_request();

                // Apply transaction options using helper
                apply_tempo_tx_opts(&mut tx, &tx_opts, is_legacy);

                send_erc20_tx(
                    provider,
                    tx,
                    &send_tx,
                    send_tx.timeout.unwrap_or(config.transaction_timeout),
                )
                .await?
            }
            Self::Approve { token, spender, amount, send_tx, tx: tx_opts, .. } => {
                let provider = get_provider_with_wallet(&send_tx, send_tx.eth.rpc.curl).await?;
                let is_legacy = config.chain.is_some_and(|c| c.is_legacy());
                let mut tx = IERC20::new(token.resolve(&provider).await?, &provider)
                    .approve(spender.resolve(&provider).await?, U256::from_str(&amount)?)
                    .into_transaction_request();

                // Apply transaction options using helper
                apply_tempo_tx_opts(&mut tx, &tx_opts, is_legacy);

                send_erc20_tx(
                    provider,
                    tx,
                    &send_tx,
                    send_tx.timeout.unwrap_or(config.transaction_timeout),
                )
                .await?
            }
            Self::Mint { token, to, amount, send_tx, tx: tx_opts, .. } => {
                let provider = get_provider_with_wallet(&send_tx, send_tx.eth.rpc.curl).await?;
                let is_legacy = config.chain.is_some_and(|c| c.is_legacy());
                let mut tx = IERC20::new(token.resolve(&provider).await?, &provider)
                    .mint(to.resolve(&provider).await?, U256::from_str(&amount)?)
                    .into_transaction_request();

                // Apply transaction options using helper
                apply_tempo_tx_opts(&mut tx, &tx_opts, is_legacy);

                send_erc20_tx(
                    provider,
                    tx,
                    &send_tx,
                    send_tx.timeout.unwrap_or(config.transaction_timeout),
                )
                .await?
            }
            Self::Burn { token, amount, send_tx, tx: tx_opts, .. } => {
                let provider = get_provider_with_wallet(&send_tx, send_tx.eth.rpc.curl).await?;
                let is_legacy = config.chain.is_some_and(|c| c.is_legacy());
                let mut tx = IERC20::new(token.resolve(&provider).await?, &provider)
                    .burn(U256::from_str(&amount)?)
                    .into_transaction_request();

                // Apply transaction options using helper
                apply_tempo_tx_opts(&mut tx, &tx_opts, is_legacy);

                send_erc20_tx(
                    provider,
                    tx,
                    &send_tx,
                    send_tx.timeout.unwrap_or(config.transaction_timeout),
                )
                .await?
            }
            Self::Create {
                name,
                symbol,
                currency,
                quote_token,
                admin,
                salt,
                force,
                send_tx,
                tx: tx_opts,
            } => {
                // Validate currency code against ISO 4217
                if !is_iso4217_currency(&currency) && !force {
                    sh_warn!("{}", iso4217_warning_message(&currency))?;
                    let response: String = foundry_common::prompt!("\nContinue anyway? [y/N] ")?;
                    if !matches!(response.trim(), "y" | "Y") {
                        sh_println!("Aborted.")?;
                        return Ok(());
                    }
                }

                let provider = get_provider_with_wallet(&send_tx, send_tx.eth.rpc.curl).await?;
                let is_legacy = config.chain.is_some_and(|c| c.is_legacy());
                let quote_token_addr = quote_token.resolve(&provider).await?;
                let admin_addr = admin.resolve(&provider).await?;

                let mut tx = ITIP20Factory::new(TIP20_FACTORY_ADDRESS, &provider)
                    .createToken(name, symbol, currency, quote_token_addr, admin_addr, salt)
                    .into_transaction_request();

                // Apply transaction options using helper
                apply_tempo_tx_opts(&mut tx, &tx_opts, is_legacy);

                send_erc20_tx(
                    provider,
                    tx,
                    &send_tx,
                    send_tx.timeout.unwrap_or(config.transaction_timeout),
                )
                .await?
            }
        };
        Ok(())
    }
}
