use crate::{format_uint_exp, tempo::send::cast_send};
use alloy_eips::BlockId;
use alloy_ens::NameOrAddress;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_serde::WithOtherFields;
use alloy_sol_types::sol;
use clap::{Args, Parser};
use foundry_cli::{
    opts::{EthereumOpts, RpcOpts},
    utils::{LoadConfig, get_tempo_provider, get_tempo_signer_provider},
};
use foundry_common::provider::tempo::TempoRetryProviderWithSigner;
#[doc(hidden)]
pub use foundry_config::utils::*;
use std::{str::FromStr, time::Duration};

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
}

#[derive(Debug, Clone, Args)]
pub struct Erc20TempoTxOpts {
    /// Ethereum options
    #[command(flatten)]
    pub eth: EthereumOpts,

    /// Only print the transaction hash and exit immediately.
    #[arg(id = "async", long = "async", alias = "cast-async", env = "CAST_ASYNC")]
    pub cast_async: bool,

    /// Wait for transaction receipt synchronously instead of polling.
    /// Note: uses `eth_sendTransactionSync` which may not be supported by all clients.
    #[arg(long, conflicts_with = "async")]
    pub sync: bool,

    /// The number of confirmations until the receipt is fetched.
    #[arg(long, default_value = "1")]
    pub confirmations: u64,

    /// Timeout for sending the transaction.
    #[arg(long, env = "ETH_TIMEOUT")]
    pub timeout: Option<u64>,

    /// Polling interval for transaction receipts (in seconds).
    #[arg(long, alias = "poll-interval", env = "ETH_POLL_INTERVAL")]
    pub poll_interval: Option<u64>,

    /// Fee token to use for transaction.
    #[arg(long)]
    pub fee_token: Option<Address>,
}

/// Interact with ERC20 tokens.
#[derive(Debug, Parser, Clone)]
pub enum Erc20TempoSubcommand {
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
    #[command(visible_alias = "t")]
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
        tx_opts: Erc20TempoTxOpts,
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
        tx_opts: Erc20TempoTxOpts,
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
        tx_opts: Erc20TempoTxOpts,
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
        tx_opts: Erc20TempoTxOpts,
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
        tx_opts: Erc20TempoTxOpts,
    },
}

impl Erc20TempoSubcommand {
    fn rpc(&self) -> &RpcOpts {
        match self {
            Self::Allowance { tx_opts, .. } => &tx_opts.eth.rpc,
            Self::Approve { tx_opts, .. } => &tx_opts.eth.rpc,
            Self::Balance { rpc, .. } => rpc,
            Self::Transfer { tx_opts, .. } => &tx_opts.eth.rpc,
            Self::Name { rpc, .. } => rpc,
            Self::Symbol { rpc, .. } => rpc,
            Self::Decimals { rpc, .. } => rpc,
            Self::TotalSupply { rpc, .. } => rpc,
            Self::Mint { tx_opts, .. } => &tx_opts.eth.rpc,
            Self::Burn { tx_opts, .. } => &tx_opts.eth.rpc,
        }
    }

    pub async fn run(self) -> eyre::Result<()> {
        let config = self.rpc().load_config()?;
        let provider = get_tempo_provider(&config)?;

        match self {
            // Read-only
            Self::Allowance { token, owner, spender, block, .. } => {
                let token = token.resolve(&provider).await?;
                let owner = owner.resolve(&provider).await?;
                let spender = spender.resolve(&provider).await?;

                let allowance = IERC20::new(token, &provider)
                    .allowance(owner, spender)
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;

                sh_println!("{}", format_uint_exp(allowance))?
            }
            Self::Balance { token, owner, block, .. } => {
                let token = token.resolve(&provider).await?;
                let owner = owner.resolve(&provider).await?;

                let balance = IERC20::new(token, &provider)
                    .balanceOf(owner)
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;
                sh_println!("{}", format_uint_exp(balance))?
            }
            Self::Name { token, block, .. } => {
                let token = token.resolve(&provider).await?;

                let name = IERC20::new(token, &provider)
                    .name()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;
                sh_println!("{}", name)?
            }
            Self::Symbol { token, block, .. } => {
                let token = token.resolve(&provider).await?;

                let symbol = IERC20::new(token, &provider)
                    .symbol()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;
                sh_println!("{}", symbol)?
            }
            Self::Decimals { token, block, .. } => {
                let token = token.resolve(&provider).await?;

                let decimals = IERC20::new(token, &provider)
                    .decimals()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;
                sh_println!("{}", decimals)?
            }
            Self::TotalSupply { token, block, .. } => {
                let token = token.resolve(&provider).await?;

                let total_supply = IERC20::new(token, &provider)
                    .totalSupply()
                    .block(block.unwrap_or_default())
                    .call()
                    .await?;
                sh_println!("{}", format_uint_exp(total_supply))?
            }
            // State-changing
            Self::Transfer { token, to, amount, tx_opts } => {
                let provider = get_erc20_provider(&tx_opts).await?;
                let timeout = tx_opts.timeout.unwrap_or(config.transaction_timeout);
                let token = token.resolve(&provider).await?;
                let to = to.resolve(&provider).await?;
                let amount = U256::from_str(&amount)?;
                let mut tx =
                    IERC20::new(token, &provider).transfer(to, amount).into_transaction_request();
                tx.fee_token = tx_opts.fee_token;
                cast_send(
                    provider,
                    WithOtherFields::new(tx),
                    tx_opts.cast_async,
                    tx_opts.sync,
                    tx_opts.confirmations,
                    timeout,
                )
                .await?
            }
            Self::Approve { token, spender, amount, tx_opts } => {
                let provider = get_erc20_provider(&tx_opts).await?;
                let timeout = tx_opts.timeout.unwrap_or(config.transaction_timeout);
                let token = token.resolve(&provider).await?;
                let spender = spender.resolve(&provider).await?;
                let amount = U256::from_str(&amount)?;
                let mut tx = IERC20::new(token, &provider)
                    .approve(spender, amount)
                    .into_transaction_request();
                tx.fee_token = tx_opts.fee_token;
                cast_send(
                    provider,
                    WithOtherFields::new(tx),
                    tx_opts.cast_async,
                    tx_opts.sync,
                    tx_opts.confirmations,
                    timeout,
                )
                .await?
            }
            Self::Mint { token, to, amount, tx_opts } => {
                let provider = get_erc20_provider(&tx_opts).await?;
                let timeout = tx_opts.timeout.unwrap_or(config.transaction_timeout);
                let token = token.resolve(&provider).await?;
                let to = to.resolve(&provider).await?;
                let amount = U256::from_str(&amount)?;
                let mut tx =
                    IERC20::new(token, &provider).mint(to, amount).into_transaction_request();
                tx.fee_token = tx_opts.fee_token;
                cast_send(
                    provider,
                    WithOtherFields::new(tx),
                    tx_opts.cast_async,
                    tx_opts.sync,
                    tx_opts.confirmations,
                    timeout,
                )
                .await?
            }
            Self::Burn { token, amount, tx_opts } => {
                let provider = get_erc20_provider(&tx_opts).await?;
                let timeout = tx_opts.timeout.unwrap_or(config.transaction_timeout);
                let token = token.resolve(&provider).await?;
                let amount = U256::from_str(&amount)?;
                let mut tx = IERC20::new(token, &provider).burn(amount).into_transaction_request();
                tx.fee_token = tx_opts.fee_token;
                cast_send(
                    provider,
                    WithOtherFields::new(tx),
                    tx_opts.cast_async,
                    tx_opts.sync,
                    tx_opts.confirmations,
                    timeout,
                )
                .await?
            }
        };
        Ok(())
    }
}

async fn get_erc20_provider(
    tx_opts: &Erc20TempoTxOpts,
) -> eyre::Result<TempoRetryProviderWithSigner> {
    let config = tx_opts.eth.load_config()?;
    let provider = get_tempo_provider(&config)?;
    if let Some(interval) = tx_opts.poll_interval {
        provider.client().set_poll_interval(Duration::from_secs(interval))
    }
    let signer = tx_opts.eth.wallet.signer().await?;
    let provider = get_tempo_signer_provider(&config, signer)?;

    Ok(provider)
}
