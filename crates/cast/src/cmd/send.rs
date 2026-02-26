use std::{str::FromStr, time::Duration};

use crate::{
    tempo::sign_with_access_key,
    tx::{self, CastTxBuilder, CastTxSender, SendTxOpts},
};
use alloy_ens::NameOrAddress;
use alloy_network::EthereumWallet;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer::Signer;
use clap::Parser;
use eyre::{Result, eyre};
use foundry_cli::{
    opts::TransactionOpts,
    utils::{LoadConfig, get_tempo_provider_with_curl},
};
use foundry_wallets::WalletSigner;
use tempo_alloy::{TempoNetwork, rpc::TempoTransactionRequest};

/// CLI arguments for `cast send`.
#[derive(Debug, Parser)]
pub struct SendTxArgs {
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

    /// Raw hex-encoded data for the transaction. Used instead of \[SIG\] and \[ARGS\].
    #[arg(
        long,
        conflicts_with_all = &["sig", "args"]
    )]
    data: Option<String>,

    #[command(flatten)]
    send_tx: SendTxOpts,

    #[command(subcommand)]
    command: Option<SendTxSubcommands>,

    /// Send via `eth_sendTransaction` using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from")]
    unlocked: bool,

    #[command(flatten)]
    tx: TransactionOpts,
}

#[derive(Debug, Parser)]
pub enum SendTxSubcommands {
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

impl SendTxArgs {
    pub async fn run(self) -> eyre::Result<()> {
        let Self { to, mut sig, mut args, send_tx, tx, command, unlocked, data } = self;
        let fee_token = tx.tempo.fee_token;

        if let Some(data) = data {
            sig = Some(data);
        }

        let code = if let Some(SendTxSubcommands::Create {
            code,
            sig: constructor_sig,
            args: constructor_args,
        }) = command
        {
            // ensure we don't violate settings for transactions that can't be CREATE: 7702 and 4844
            // which require mandatory target
            if to.is_none() && !tx.auth.is_empty() {
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

        let config = send_tx.eth.load_config()?;
        let provider = get_tempo_provider_with_curl(&config, send_tx.eth.rpc.curl)?;

        if let Some(interval) = send_tx.poll_interval {
            provider.client().set_poll_interval(Duration::from_secs(interval))
        }

        // Clone tx_opts if sponsor is present or print-sponsor-hash mode
        let sponsor_opts = if tx.tempo.is_sponsor() || tx.tempo.should_print_hash() {
            Some(tx.clone())
        } else {
            None
        };

        // Get access key config early so we can set key_id before gas estimation
        let access_key_config = send_tx.eth.wallet.access_key_config();

        let mut builder =
            CastTxBuilder::<_, _, TempoTransactionRequest>::new(&provider, tx, &config)
                .await?
                .with_to(to)
                .await?
                .with_code_sig_and_args(code, sig, args)
                .await?;

        // Set key_id before build() so gas estimation includes Keychain signature overhead
        if let Some(ref config) = access_key_config {
            builder = builder.with_key_id(config.key_id);
        }

        let timeout = send_tx.timeout.unwrap_or(config.transaction_timeout);

        // Check if this is a Tempo transaction - requires special handling for local signing
        let is_tempo = builder.is_tempo();

        // Tempo transactions with browser wallets are not supported
        if is_tempo && send_tx.eth.wallet.browser {
            return Err(eyre!("Tempo transactions are not supported with browser wallets."));
        }

        // Case 1:
        // Default to sending via eth_sendTransaction if the --unlocked flag is passed.
        // This should be the only way this RPC method is used as it requires a local node
        // or remote RPC with unlocked accounts.
        if unlocked && !send_tx.eth.wallet.browser {
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

            let (tx, _) = if let Some(ref opts) = sponsor_opts {
                builder.build_sponsored(config.sender, fee_token, opts).await?
            } else {
                builder.build(config.sender, fee_token).await?
            };

            cast_send(
                provider,
                tx.into_inner(),
                send_tx.cast_async,
                send_tx.sync,
                send_tx.confirmations,
                timeout,
            )
            .await
        // Case 2:
        // An option to use a local signer was provided.
        // If we cannot successfully instantiate a local signer, then we will assume we don't have
        // enough information to sign and we must bail.
        } else {
            // Retrieve the signer, and bail if it can't be constructed.
            let signer = send_tx.eth.wallet.signer().await?;

            // For access keys, `from` is the root account; otherwise it's the signer address
            let from = if let Some(ref config) = access_key_config {
                config.root_account
            } else {
                Signer::address(&signer)
            };

            // Only validate from address if not using access key
            if access_key_config.is_none() {
                tx::validate_from_address(send_tx.eth.wallet.from, from)?;
            }

            // Browser wallets work differently as they sign and send the transaction in one step.
            if send_tx.eth.wallet.browser
                && let WalletSigner::Browser(ref browser_signer) = signer
            {
                let (tx_request, _) = if let Some(ref opts) = sponsor_opts {
                    builder.build_sponsored(from, fee_token, opts).await?
                } else {
                    builder.build(from, fee_token).await?
                };
                let tx_hash =
                    browser_signer.send_transaction_via_browser(tx_request.inner.inner).await?;

                if send_tx.cast_async {
                    sh_println!("{tx_hash:#x}")?;
                } else {
                    let receipt = CastTxSender::new(&provider)
                        .receipt(
                            format!("{tx_hash:#x}"),
                            None,
                            send_tx.confirmations,
                            Some(timeout),
                            false,
                        )
                        .await?;
                    sh_println!("{receipt}")?;
                }

                return Ok(());
            }

            // For access keys, pass the root account address so gas estimation and nonce lookup
            // use the correct address. For regular transactions, pass the signer so EIP-7702
            // authorization signing can work.
            let (tx_request, _) = match (&access_key_config, &sponsor_opts) {
                (Some(_), Some(opts)) => builder.build_sponsored(from, fee_token, opts).await?,
                (Some(_), None) => builder.build(from, fee_token).await?,
                (None, Some(opts)) => builder.build_sponsored(&signer, fee_token, opts).await?,
                (None, None) => builder.build(&signer, fee_token).await?,
            };

            if let Some(ref config) = access_key_config {
                let raw_tx =
                    sign_with_access_key(tx_request.inner, &signer, config.root_account).await?;

                let cast = CastTxSender::new(&provider);
                let pending_tx = cast.send_raw(&raw_tx).await?;
                let tx_hash = pending_tx.inner().tx_hash();

                if send_tx.cast_async {
                    sh_println!("{tx_hash:#x}")?;
                } else {
                    let pending_tx = provider.send_raw_transaction(&raw_tx).await?;
                    let tx_hash = pending_tx.tx_hash();
                    if send_tx.cast_async {
                        sh_println!("{tx_hash:#x}")?;
                    } else {
                        let receipt = cast
                            .receipt(
                                format!("{tx_hash:#x}"),
                                None,
                                send_tx.confirmations,
                                Some(timeout),
                                false,
                            )
                            .await?;
                        sh_println!("{receipt}")?;
                    }
                }
            } else {
                // Standard flow: use EthereumWallet for signing
                let wallet = EthereumWallet::from(signer);
                let provider = ProviderBuilder::<_, _, TempoNetwork>::default()
                    .wallet(wallet)
                    .connect_provider(&provider);

                cast_send(
                    provider,
                    tx_request.inner,
                    send_tx.cast_async,
                    send_tx.sync,
                    send_tx.confirmations,
                    timeout,
                )
                .await?;
            }

            Ok(())
        }
    }
}

pub(crate) async fn cast_send<P: Provider<TempoNetwork>>(
    provider: P,
    tx: TempoTransactionRequest,
    cast_async: bool,
    sync: bool,
    confs: u64,
    timeout: u64,
) -> Result<()> {
    let cast = CastTxSender::new(&provider);

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
