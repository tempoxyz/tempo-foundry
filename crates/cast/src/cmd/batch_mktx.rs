//! `cast batch-mktx` command implementation.
//!
//! Creates a signed or unsigned batch transaction using Tempo's native call batching.
//! Outputs the RLP-encoded transaction hex.

use crate::{
    call_spec::CallSpec,
    tempo::{parse_function_args, sign_with_access_key},
    tx::{self, CastTxBuilder},
};
use alloy_eips::eip2718::Encodable2718;
use alloy_network::{EthereumWallet, TransactionBuilder};
use alloy_primitives::{Address, Bytes, hex};
use alloy_provider::Provider;
use alloy_signer::Signer;
use clap::Parser;
use eyre::{Result, eyre};
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils::{LoadConfig, get_tempo_provider},
};
use tempo_alloy::rpc::TempoTransactionRequest;
use tempo_primitives::transaction::Call;

/// CLI arguments for `cast batch-mktx`.
///
/// Creates a signed (or unsigned) batch transaction.
#[derive(Debug, Parser)]
pub struct BatchMakeTxArgs {
    /// Call specifications in format: to[:value][:sig[:args]] or to[:value][:0xdata]
    ///
    /// Examples:
    ///   --call "0x123:0.1ether" (ETH transfer)
    ///   --call "0x456::transfer(address,uint256):0x789,1000" (ERC20 transfer)
    ///   --call "0xabc::0x123def" (raw calldata)
    #[arg(long = "call", value_name = "SPEC", required = true)]
    pub calls: Vec<String>,

    #[command(flatten)]
    pub tx: TransactionOpts,

    #[command(flatten)]
    pub eth: EthereumOpts,

    /// Generate a raw RLP-encoded unsigned transaction.
    #[arg(long)]
    pub raw_unsigned: bool,

    /// Call `eth_signTransaction` using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from", conflicts_with = "raw_unsigned")]
    pub ethsign: bool,
}

impl BatchMakeTxArgs {
    pub async fn run(self) -> Result<()> {
        let Self { calls, tx, eth, raw_unsigned, ethsign } = self;
        let fee_token = tx.tempo.fee_token;

        if calls.is_empty() {
            return Err(eyre!("No calls specified. Use --call to specify at least one call."));
        }

        let config = eth.load_config()?;
        let provider = get_tempo_provider(&config)?;

        // Get access key config early
        let access_key_config = eth.wallet.access_key_config();

        // Parse all call specs
        let call_specs: Vec<CallSpec> =
            calls.iter().map(|s| CallSpec::parse(s)).collect::<Result<Vec<_>>>()?;

        // Get chain for parsing function args
        let chain = crate::tempo::get_chain(config.chain, &provider).await?;
        let etherscan_api_key = config.get_etherscan_api_key(Some(chain));

        // Build Vec<Call> from specs
        let mut tempo_calls = Vec::with_capacity(call_specs.len());
        for (i, spec) in call_specs.iter().enumerate() {
            let input = if let Some(data) = &spec.data {
                data.clone()
            } else if let Some(sig) = &spec.sig {
                let (encoded, _) = parse_function_args(
                    sig,
                    spec.args.clone(),
                    Some(spec.to),
                    chain,
                    &provider,
                    etherscan_api_key.as_deref(),
                )
                .await
                .map_err(|e| eyre!("Failed to encode call {}: {}", i + 1, e))?;
                Bytes::from(encoded)
            } else {
                Bytes::new()
            };

            tempo_calls.push(Call { to: spec.to.into(), value: spec.value, input });
        }

        sh_println!("Building batch transaction with {} call(s)...", tempo_calls.len())?;

        // Build transaction request with calls
        let mut builder =
            CastTxBuilder::<_, _, TempoTransactionRequest>::new(&provider, tx.clone(), &config)
                .await?;

        // Set key_id for access key transactions
        if let Some(ref config) = access_key_config {
            builder = builder.with_key_id(config.key_id);
        }

        // Set calls on the transaction
        builder.tx.calls = tempo_calls;

        // Set dummy "to" from first call
        let first_call_to = call_specs.first().map(|s| s.to);
        let builder = builder.with_to(first_call_to.map(Into::into)).await?;
        let tx_builder = builder.with_code_sig_and_args(None, None, vec![]).await?;

        if raw_unsigned {
            // Check requirements for unsigned tx
            if eth.wallet.from.is_none() && tx.nonce.is_none() {
                eyre::bail!(
                    "Missing required parameters for raw unsigned transaction. When --from is not provided, you must specify: --nonce"
                );
            }

            let from = eth.wallet.from.unwrap_or(Address::ZERO);
            let raw_tx = tx_builder.build_unsigned_raw(from, fee_token).await?;
            sh_println!("{raw_tx}")?;
            return Ok(());
        }

        if ethsign {
            let (tx, _) = tx_builder.build(config.sender, fee_token).await?;
            let signed_tx = provider.sign_transaction(tx.inner).await?;
            sh_println!("{signed_tx}")?;
            return Ok(());
        }

        // Default: use local signer
        let signer = eth.wallet.signer().await?;
        let from = if let Some(ref config) = access_key_config {
            config.root_account
        } else {
            Signer::address(&signer)
        };

        if access_key_config.is_none() {
            tx::validate_from_address(eth.wallet.from, from)?;
        }

        let (tx, _) = if access_key_config.is_some() {
            tx_builder.build(from, fee_token).await?
        } else {
            tx_builder.build(&signer, fee_token).await?
        };

        let signed_tx = if let Some(ref config) = access_key_config {
            let raw_tx = sign_with_access_key(tx.inner, &signer, config.root_account).await?;
            hex::encode(raw_tx)
        } else {
            let envelope = tx.inner.build(&EthereumWallet::new(signer)).await?;
            hex::encode(envelope.encoded_2718())
        };

        sh_println!("0x{signed_tx}")?;

        Ok(())
    }
}
