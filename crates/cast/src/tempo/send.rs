use std::{str::FromStr, time::Duration};

use crate::{
    tempo::TempoCastSender,
    tx::{self, CastTxBuilder},
};
use alloy_ens::NameOrAddress;
use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use clap::Parser;
use eyre::{Result, eyre};
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils::{LoadConfig, get_tempo_provider},
};
use foundry_wallets::WalletSigner;
use tempo_alloy::{TempoNetwork, rpc::TempoTransactionRequest};

/// CLI arguments for `cast send`.
#[derive(Debug, Parser)]
pub struct SendTempoTxArgs {
    /// The destination of the transaction.
    ///
    /// If not provided, you must use cast send --create.
    #[arg(value_parser = NameOrAddress::from_str)]
    to: Option<NameOrAddress>,

    /// The signature of the function to call.
    sig: Option<String>,

    /// The arguments of the function to call.
    #[arg(allow_negative_numbers = true)]
    args: Vec<String>,

    /// Only print the transaction hash and exit immediately.
    #[arg(id = "async", long = "async", alias = "cast-async", env = "CAST_ASYNC")]
    cast_async: bool,

    /// Wait for transaction receipt synchronously instead of polling.
    /// Note: uses `eth_sendTransactionSync` which may not be supported by all clients.
    #[arg(long, conflicts_with = "async")]
    sync: bool,

    /// The number of confirmations until the receipt is fetched.
    #[arg(long, default_value = "1")]
    confirmations: u64,

    /// Polling interval for transaction receipts (in seconds).
    #[arg(long, alias = "poll-interval", env = "ETH_POLL_INTERVAL")]
    poll_interval: Option<u64>,

    #[command(subcommand)]
    command: Option<SendTempoTxSubcommands>,

    /// Send via `eth_sendTransaction` using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from")]
    unlocked: bool,

    /// Timeout for sending the transaction.
    #[arg(long, env = "ETH_TIMEOUT")]
    pub timeout: Option<u64>,

    #[command(flatten)]
    tx: TransactionOpts,

    #[command(flatten)]
    eth: EthereumOpts,

    /// Fee token to use for transaction.
    #[arg(long)]
    fee_token: Option<Address>,
}

#[derive(Debug, Parser)]
pub enum SendTempoTxSubcommands {
    /// Use to deploy raw contract bytecode.
    #[command(name = "--create")]
    Create {
        /// The bytecode of the contract to deploy.
        code: String,

        /// The signature of the function to call.
        sig: Option<String>,

        /// The arguments of the function to call.
        #[arg(allow_negative_numbers = true)]
        args: Vec<String>,
    },
}

impl SendTempoTxArgs {
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            eth,
            to,
            mut sig,
            cast_async,
            sync,
            mut args,
            tx,
            confirmations,
            command,
            unlocked,
            timeout,
            poll_interval,
            fee_token,
        } = self;

        let code = if let Some(SendTempoTxSubcommands::Create {
            code,
            sig: constructor_sig,
            args: constructor_args,
        }) = command
        {
            // ensure we don't violate settings for transactions that can't be CREATE: 7702 and 4844
            // which require mandatory target
            if to.is_none() && tx.auth.is_some() {
                return Err(eyre!(
                    "EIP-7702 transactions can't be CREATE transactions and require a destination address"
                ));
            }

            sig = constructor_sig;
            args = constructor_args;
            Some(code)
        } else {
            None
        };

        let config = eth.load_config()?;
        let provider = get_tempo_provider(&config)?;

        if let Some(interval) = poll_interval {
            provider.client().set_poll_interval(Duration::from_secs(interval))
        }

        let builder = CastTxBuilder::<_, _, TempoTransactionRequest>::new(&provider, tx, &config)
            .await?
            .with_to(to)
            .await?
            .with_code_sig_and_args(code, sig, args)
            .await?;

        let timeout = timeout.unwrap_or(config.transaction_timeout);

        // Case 1:
        // Default to sending via eth_sendTransaction if the --unlocked flag is passed.
        // This should be the only way this RPC method is used as it requires a local node
        // or remote RPC with unlocked accounts.
        if unlocked && !eth.wallet.browser {
            // only check current chain id if it was specified in the config
            if let Some(config_chain) = config.chain {
                let current_chain_id = provider.get_chain_id().await?;
                let config_chain_id = config_chain.id();
                // switch chain if current chain id is not the same as the one specified in the
                // config
                if config_chain_id != current_chain_id {
                    sh_warn!("Switching to chain {}", config_chain)?;
                    provider
                        .raw_request::<_, ()>(
                            "wallet_switchEthereumChain".into(),
                            [serde_json::json!({
                                "chainId": format!("0x{:x}", config_chain_id),
                            })],
                        )
                        .await?;
                }
            }

            let (tx, _) = builder.build(config.sender, fee_token).await?;

            cast_send(provider, tx, cast_async, sync, confirmations, timeout).await
        // Case 2:
        // An option to use a local signer was provided.
        // If we cannot successfully instantiate a local signer, then we will assume we don't have
        // enough information to sign and we must bail.
        } else {
            // Retrieve the signer, and bail if it can't be constructed.
            let signer = eth.wallet.signer().await?;
            let from = signer.address();

            tx::validate_from_address(eth.wallet.from, from)?;

            // Browser wallets work differently as they sign and send the transaction in one step.
            if eth.wallet.browser
                && let WalletSigner::Browser(ref browser_signer) = signer
            {
                let (tx_request, _) = builder.build(from, fee_token).await?;
                let tx_hash =
                    browser_signer.send_transaction_via_browser(tx_request.inner.inner).await?;

                if cast_async {
                    sh_println!("{tx_hash:#x}")?;
                } else {
                    let receipt = TempoCastSender::new(&provider)
                        .receipt(format!("{tx_hash:#x}"), None, confirmations, Some(timeout), false)
                        .await?;
                    sh_println!("{receipt}")?;
                }

                return Ok(());
            }

            let (tx_request, _) = builder.build(&signer, fee_token).await?;

            let wallet = EthereumWallet::from(signer);
            let provider = ProviderBuilder::<_, _, TempoNetwork>::default()
                .wallet(wallet)
                .connect_provider(&provider);

            cast_send(provider, tx_request, cast_async, sync, confirmations, timeout).await
        }
    }
}

pub async fn cast_send<P: Provider<TempoNetwork>>(
    provider: P,
    tx: WithOtherFields<TempoTransactionRequest>,
    cast_async: bool,
    sync: bool,
    confs: u64,
    timeout: u64,
) -> Result<()> {
    let cast = TempoCastSender::new(&provider);

    if sync {
        // Send transaction and wait for receipt synchronously
        let receipt = cast.send_sync(tx).await?;
        sh_println!("{receipt}")?;
    } else {
        let pending_tx = cast.send(tx).await?;
        let tx_hash = pending_tx.inner().tx_hash();

        if cast_async {
            sh_println!("{tx_hash:#x}")?;
        } else {
            let receipt =
                cast.receipt(format!("{tx_hash:#x}"), None, confs, Some(timeout), false).await?;
            sh_println!("{receipt}")?;
        }
    }

    Ok(())
}
