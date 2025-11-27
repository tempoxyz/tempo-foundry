//! `cast` subcommands.
//!
//! All subcommands should respect the `foundry_config::Config`.
//! If a subcommand accepts values that are supported by the `Config`, then the subcommand should
//! implement `figment::Provider` which allows the subcommand to override the config's defaults, see
//! [`foundry_config::Config`].

use std::time::Duration;
use alloy_primitives::{TxHash};
use alloy_provider::{PendingTransactionBuilder, Provider};
use alloy_serde::WithOtherFields;
use tempo_alloy::rpc::TempoTransactionRequest;
use tempo_alloy::TempoNetwork;
use foundry_common::{shell};
use crate::tempo::transactions::{get_pretty_tx_receipt_attr, TempoTransactionReceiptWithRevertReason};
use foundry_common_fmt::UIfmt;

use std::{
    str::FromStr,
};
use eyre::WrapErr;

pub mod tx;
pub mod mktx;
pub mod send;
pub mod provider;

pub mod transactions;

pub struct TempoCastSender<P> {
    provider: P,
}

impl<P: Provider<TempoNetwork>> TempoCastSender<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Sends a transaction and waits for receipt synchronously
    pub async fn send_sync(&self, tx: WithOtherFields<TempoTransactionRequest>) -> eyre::Result<String> {
        let mut receipt: TempoTransactionReceiptWithRevertReason =
            self.provider.send_transaction_sync(tx.inner).await?.into();

        // Allow to fail silently
        let _ = receipt.update_revert_reason(&self.provider).await;

        self.format_receipt(receipt, None)
    }

    pub async fn send(
        &self,
        tx: WithOtherFields<TempoTransactionRequest>,
    ) -> eyre::Result<PendingTransactionBuilder<TempoNetwork>> {
        let res = self.provider.send_transaction(tx.inner).await?;

        Ok(res)
    }

    pub async fn receipt(
        &self,
        tx_hash: String,
        field: Option<String>,
        confs: u64,
        timeout: Option<u64>,
        cast_async: bool,
    ) -> eyre::Result<String> {
        let tx_hash = TxHash::from_str(&tx_hash).wrap_err("invalid tx hash")?;

        let mut receipt: TempoTransactionReceiptWithRevertReason =
            match self.provider.get_transaction_receipt(tx_hash).await? {
                Some(r) => r,
                None => {
                    // if the async flag is provided, immediately exit if no tx is found, otherwise
                    // try to poll for it
                    if cast_async {
                        eyre::bail!("tx not found: {:?}", tx_hash)
                    } else {
                        PendingTransactionBuilder::new(self.provider.root().clone(), tx_hash)
                            .with_required_confirmations(confs)
                            .with_timeout(timeout.map(Duration::from_secs))
                            .get_receipt()
                            .await?
                    }
                }
            }
                .into();

        // Allow to fail silently
        let _ = receipt.update_revert_reason(&self.provider).await;

        self.format_receipt(receipt, field)
    }

    /// Helper method to format transaction receipts consistently
    fn format_receipt(
        &self,
        receipt: TempoTransactionReceiptWithRevertReason,
        field: Option<String>,
    ) -> eyre::Result<String> {
        Ok(if let Some(ref field) = field {
            get_pretty_tx_receipt_attr(&receipt, field)
                .ok_or_else(|| eyre::eyre!("invalid receipt field: {}", field))?
        } else if shell::is_json() {
            // to_value first to sort json object keys
            serde_json::to_value(&receipt)?.to_string()
        } else {
            receipt.pretty()
        })
    }
}