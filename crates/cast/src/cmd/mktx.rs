use crate::tx::{self, CastTxBuilder};
use alloy_consensus::{SignableTransaction, Signed};
use alloy_eips::Encodable2718;
use alloy_ens::NameOrAddress;
use alloy_network::{Ethereum, EthereumWallet, Network, NetworkTransactionBuilder};
use alloy_primitives::{Address, hex};
use alloy_provider::Provider;
use alloy_signer::{Signature, Signer};
use clap::Parser;
use eyre::Result;
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils::LoadConfig,
};
use foundry_common::{FoundryTransactionBuilder, provider::ProviderBuilder};
use std::{path::PathBuf, str::FromStr};
use tempo_alloy::TempoNetwork;

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
        if self.tx.tempo.is_tempo() {
            self.run_generic::<TempoNetwork>().await
        } else {
            self.run_generic::<Ethereum>().await
        }
    }

    pub async fn run_generic<N: Network>(self) -> Result<()>
    where
        N::TxEnvelope: From<Signed<N::UnsignedTx>>,
        N::UnsignedTx: SignableTransaction<Signature>,
        N::TransactionRequest: FoundryTransactionBuilder<N>,
    {
        let Self { to, mut sig, mut args, command, tx, path, eth, raw_unsigned, ethsign } = self;

        let print_sponsor_hash = tx.tempo.print_sponsor_hash;

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

        let provider = ProviderBuilder::<N>::from_config(&config)?.build()?;

        let tx_builder =
            CastTxBuilder::<_, _, TempoTransactionRequest>::new(&provider, tx.clone(), &config)
                .await?
                .with_to(to)
                .await?
                .with_code_sig_and_args(code, sig, args)
                .await?
                .with_blob_data(blob_data)?;

        // If --tempo.print-sponsor-hash was passed, build the tx, print the hash, and exit.
        if print_sponsor_hash {
            // Resolve the signer to derive the actual sender address, since the
            // sponsor hash commits to the sender.
            let signer = eth.wallet.signer().await?;
            let from = signer.address();
            let (tx, _) = tx_builder.build(from).await?;
            let hash = tx.compute_sponsor_hash(from).ok_or_else(|| {
                eyre::eyre!("This network does not support sponsored transactions")
            })?;
            sh_println!("{hash:?}")?;
            return Ok(());
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

            let (tx, _) = tx_builder.build(from).await?;
            let raw_tx = hex::encode_prefixed(tx.build_unsigned()?.encoded_for_signing());

            sh_println!("{raw_tx}")?;
            return Ok(());
        }

        if ethsign {
            // Use "eth_signTransaction" to sign the transaction only works if the node/RPC has
            // unlocked accounts.
            let (tx, _) = tx_builder.build(config.sender, fee_token).await?;
            let signed_tx = provider.sign_transaction(tx.inner).await?;

            sh_println!("{signed_tx}")?;
            return Ok(());
        }

        // Default to using the local signer.
        // Get the signer from the wallet, and fail if it can't be constructed.
        let signer = eth.wallet.signer().await?;

        // Check if we're using an access key (signs on behalf of root account)
        let access_key_config = eth.wallet.access_key_config();

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
        let (mut tx, _) = if access_key_config.is_some() {
            tx_builder.build(from, fee_token).await?
        } else {
            tx_builder.build(&signer, fee_token).await?
        };

        // For access keys, set the key_id
        if let Some(ref config) = access_key_config {
            tx.key_id = Some(config.key_id);
        }

        let signed_tx = if access_key_config.is_some() {
            // For access keys, build unsigned then sign directly to avoid
            // EthereumWallet's address validation (which expects from == signer address)
            let mut unsigned_tx = tx.inner.build_unsigned()?;
            let sig = signer.sign_transaction(unsigned_tx.as_dyn_signable_mut()).await?;
            let envelope = unsigned_tx.into_envelope(sig);
            hex::encode(envelope.encoded_2718())
        } else {
            // Standard signing through EthereumWallet
            let envelope = tx.inner.build(&EthereumWallet::new(signer)).await?;
            hex::encode(envelope.encoded_2718())
        };

        sh_println!("0x{signed_tx}")?;

        Ok(())
    }
}
