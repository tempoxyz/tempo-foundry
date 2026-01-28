use crate::tx::{self, CastTxBuilder};

use crate::tempo::sign_with_access_key;
use alloy_eips::eip2718::Encodable2718;
use alloy_ens::NameOrAddress;
use alloy_network::{EthereumWallet, TransactionBuilder};
use alloy_primitives::{Address, hex};
use alloy_provider::Provider;
use alloy_signer::Signer;
use clap::Parser;
use eyre::Result;
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils::{LoadConfig, get_tempo_provider, parse_fee_token_address},
};
use std::{path::PathBuf, str::FromStr};
use tempo_alloy::rpc::TempoTransactionRequest;

/// CLI arguments for `cast mktx`.
#[derive(Debug, Parser)]
pub struct MakeTxArgs {
    /// The destination of the transaction.
    ///
    /// If not provided, you must use `cast mktx --create`.
    #[arg(value_parser = NameOrAddress::from_str)]
    to: Option<NameOrAddress>,

    /// The signature of the function to call.
    sig: Option<String>,

    /// The arguments of the function to call.
    #[arg(allow_negative_numbers = true)]
    args: Vec<String>,

    #[command(subcommand)]
    command: Option<MakeTxSubcommands>,

    #[command(flatten)]
    tx: TransactionOpts,

    /// The path of blob data to be sent.
    #[arg(
        long,
        value_name = "BLOB_DATA_PATH",
        conflicts_with = "legacy",
        requires = "blob",
        help_heading = "Transaction options"
    )]
    path: Option<PathBuf>,

    #[command(flatten)]
    eth: EthereumOpts,

    /// Generate a raw RLP-encoded unsigned transaction.
    ///
    /// Relaxes the wallet requirement.
    #[arg(long)]
    raw_unsigned: bool,

    /// Call `eth_signTransaction` using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from", conflicts_with = "raw_unsigned")]
    ethsign: bool,

    /// Fee token to use for transaction.
    #[arg(long, value_parser = parse_fee_token_address)]
    fee_token: Option<Address>,
}

#[derive(Debug, Parser)]
pub enum MakeTxSubcommands {
    /// Use to deploy raw contract bytecode.
    #[command(name = "--create")]
    Create {
        /// The initialization bytecode of the contract to deploy.
        code: String,

        /// The signature of the constructor.
        sig: Option<String>,

        /// The constructor arguments.
        #[arg(allow_negative_numbers = true)]
        args: Vec<String>,
    },
}

impl MakeTxArgs {
    pub async fn run(self) -> Result<()> {
        let Self {
            to,
            mut sig,
            mut args,
            command,
            tx,
            path,
            eth,
            raw_unsigned,
            ethsign,
            fee_token,
        } = self;

        let blob_data = if let Some(path) = path { Some(std::fs::read(path)?) } else { None };

        let code = if let Some(MakeTxSubcommands::Create {
            code,
            sig: constructor_sig,
            args: constructor_args,
        }) = command
        {
            sig = constructor_sig;
            args = constructor_args;
            Some(code)
        } else {
            None
        };

        let config = eth.load_config()?;

        let provider = get_tempo_provider(&config)?;

        // Clone tx_opts if sponsor is present (need it for build_sponsored)
        let sponsor_opts = if tx.sponsor.is_some() { Some(tx.clone()) } else { None };

        // Get access key config early so we can set key_id before gas estimation
        let access_key_config = eth.wallet.access_key_config();

        let mut tx_builder =
            CastTxBuilder::<_, _, TempoTransactionRequest>::new(&provider, tx.clone(), &config)
                .await?
                .with_to(to)
                .await?
                .with_code_sig_and_args(code, sig, args)
                .await?
                .with_blob_data(blob_data)?;

        // Set key_id before build() so gas estimation includes Keychain signature overhead
        if let Some(ref config) = access_key_config {
            tx_builder = tx_builder.with_key_id(config.key_id);
        }

        if raw_unsigned {
            // Build unsigned raw tx
            // Check if nonce is provided when --from is not specified
            // See: <https://github.com/foundry-rs/foundry/issues/11110>
            if eth.wallet.from.is_none() && tx.nonce.is_none() {
                eyre::bail!(
                    "Missing required parameters for raw unsigned transaction. When --from is not provided, you must specify: --nonce"
                );
            }

            // Use zero address as placeholder for unsigned transactions
            let from = eth.wallet.from.unwrap_or(Address::ZERO);

            let raw_tx = tx_builder.build_unsigned_raw(from, fee_token).await?;

            sh_println!("{raw_tx}")?;
            return Ok(());
        }

        if ethsign {
            // Use "eth_signTransaction" to sign the transaction only works if the node/RPC has
            // unlocked accounts.
            let (tx, _) = if let Some(ref opts) = sponsor_opts {
                tx_builder.build_sponsored(config.sender, fee_token, opts).await?
            } else {
                tx_builder.build(config.sender, fee_token).await?
            };
            let signed_tx = provider.sign_transaction(tx.inner).await?;

            sh_println!("{signed_tx}")?;
            return Ok(());
        }

        // Default to using the local signer.
        // Get the signer from the wallet, and fail if it can't be constructed.
        let signer = eth.wallet.signer().await?;

        // For access keys, `from` is the root account; otherwise it's the signer address
        let from = if let Some(ref config) = access_key_config {
            config.root_account
        } else {
            Signer::address(&signer)
        };

        // Only validate from address if not using access key
        if access_key_config.is_none() {
            tx::validate_from_address(eth.wallet.from, from)?;
        }

        // For access keys, pass the root account address so gas estimation and nonce lookup
        // use the correct address. For regular transactions, pass the signer so EIP-7702
        // authorization signing can work.
        let (tx, _) = match (&access_key_config, &sponsor_opts) {
            (Some(_), Some(opts)) => tx_builder.build_sponsored(from, fee_token, opts).await?,
            (Some(_), None) => tx_builder.build(from, fee_token).await?,
            (None, Some(opts)) => tx_builder.build_sponsored(&signer, fee_token, opts).await?,
            (None, None) => tx_builder.build(&signer, fee_token).await?,
        };

        let signed_tx = if let Some(ref config) = access_key_config {
            let raw_tx = sign_with_access_key(tx.inner, &signer, config.root_account).await?;
            hex::encode(raw_tx)
        } else {
            // Standard signing through EthereumWallet
            let envelope = tx.inner.build(&EthereumWallet::new(signer)).await?;
            hex::encode(envelope.encoded_2718())
        };

        sh_println!("0x{signed_tx}")?;

        Ok(())
    }
}
