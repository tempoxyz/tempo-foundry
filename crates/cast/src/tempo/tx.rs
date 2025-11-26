use crate::traces::identifier::SignaturesIdentifier;
use alloy_consensus::{SidecarBuilder, SignableTransaction, SimpleCoder};
use alloy_dyn_abi::ErrorExt;
use alloy_ens::NameOrAddress;
use alloy_json_abi::Function;
use alloy_network::{
    AnyNetwork, AnyTypedTransaction, TransactionBuilder, TransactionBuilder4844,
    TransactionBuilder7702,
};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex};
use alloy_provider::Provider;
use alloy_rpc_types::{AccessList, Authorization, TransactionInputKind, TransactionRequest};
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use alloy_transport::TransportError;
use eyre::Result;
use foundry_cli::{
    opts::{CliAuthorizationList, TransactionOpts},
    utils::{self, parse_function_args},
};
use foundry_common::{
    fmt::format_tokens,
    provider::{RetryProvider, RetryProviderWithSigner},
};
use foundry_config::{Chain, Config};
use foundry_wallets::{WalletOpts, WalletSigner};
use itertools::Itertools;
use serde_json::value::RawValue;
use std::fmt::Write;
use tempo_alloy::rpc::TempoTransactionRequest;
use tempo_primitives::transaction::TempoTypedTransaction;
use crate::tx::{CastTxBuilder, InitState, InputState, SenderKind, ToState};

impl<P: Provider<AnyNetwork>> CastTxBuilder<P, InitState, TempoTransactionRequest> {
    /// Creates a new instance of [CastTxBuilder] filling transaction with fields present in
    /// provided [TransactionOpts].
    pub async fn new(provider: P, tx_opts: TransactionOpts, config: &Config) -> Result<Self> {
        let mut tx = WithOtherFields::<TempoTransactionRequest>::default();

        let chain = utils::get_chain(config.chain, &provider).await?;
        let etherscan_api_key = config.get_etherscan_api_key(Some(chain));
        // mark it as legacy if requested or the chain is legacy and no 7702 is provided.
        let legacy = tx_opts.legacy || (chain.is_legacy() && tx_opts.auth.is_none());

        if let Some(gas_limit) = tx_opts.gas_limit {
            tx.set_gas_limit(gas_limit.to());
        }

        if let Some(value) = tx_opts.value {
            tx.set_value(value);
        }

        if let Some(gas_price) = tx_opts.gas_price {
            if legacy {
                tx.set_gas_price(gas_price.to());
            } else {
                tx.set_max_fee_per_gas(gas_price.to());
            }
        }

        if !legacy && let Some(priority_fee) = tx_opts.priority_gas_price {
            tx.set_max_priority_fee_per_gas(priority_fee.to());
        }

        if let Some(nonce) = tx_opts.nonce {
            tx.set_nonce(nonce.to());
        }

        Ok(Self {
            provider,
            tx,
            legacy,
            blob: tx_opts.blob,
            chain,
            etherscan_api_key,
            auth: tx_opts.auth,
            access_list: tx_opts.access_list,
            state: InitState,
        })
    }

    /// Sets [TxKind] for this builder and changes state to [ToState].
    pub async fn with_to(self, to: Option<NameOrAddress>) -> Result<CastTxBuilder<P, ToState, TempoTransactionRequest>> {
        let to = if let Some(to) = to { Some(to.resolve(&self.provider).await?) } else { None };
        Ok(CastTxBuilder {
            provider: self.provider,
            tx: self.tx,
            legacy: self.legacy,
            blob: self.blob,
            chain: self.chain,
            etherscan_api_key: self.etherscan_api_key,
            auth: self.auth,
            access_list: self.access_list,
            state: ToState { to },
        })
    }
}

impl<P: Provider<AnyNetwork>> CastTxBuilder<P, ToState, TempoTransactionRequest> {
    /// Accepts user-provided code, sig and args params and constructs calldata for the transaction.
    /// If code is present, input will be set to code + encoded constructor arguments. If no code is
    /// present, input is set to just provided arguments.
    pub async fn with_code_sig_and_args(
        self,
        code: Option<String>,
        sig: Option<String>,
        args: Vec<String>,
    ) -> Result<CastTxBuilder<P, InputState, TempoTransactionRequest>> {
        let (mut args, func) = if let Some(sig) = sig {
            parse_function_args(
                &sig,
                args,
                self.state.to,
                self.chain,
                &self.provider,
                self.etherscan_api_key.as_deref(),
            )
                .await?
        } else {
            (Vec::new(), None)
        };

        let input = if let Some(code) = &code {
            let mut code = hex::decode(code)?;
            code.append(&mut args);
            code
        } else {
            args
        };

        if self.state.to.is_none() && code.is_none() {
            let has_value = self.tx.value().is_some_and(|v| !v.is_zero());
            let has_auth = self.auth.is_some();
            // We only allow user to omit the recipient address if transaction is an EIP-7702 tx
            // without a value.
            if !has_auth || has_value {
                eyre::bail!("Must specify a recipient address or contract code to deploy");
            }
        }

        Ok(CastTxBuilder {
            provider: self.provider,
            tx: self.tx,
            legacy: self.legacy,
            blob: self.blob,
            chain: self.chain,
            etherscan_api_key: self.etherscan_api_key,
            auth: self.auth,
            access_list: self.access_list,
            state: InputState { kind: self.state.to.into(), input, func },
        })
    }
}

impl<P: Provider<AnyNetwork>> CastTxBuilder<P, InputState, TempoTransactionRequest> {
    /// Builds [TempoTransactionRequest] and fills missing fields. Returns a transaction which is ready
    /// to be broadcasted.
    pub async fn build(
        self,
        sender: impl Into<SenderKind<'_>>,
    ) -> Result<(WithOtherFields<TempoTransactionRequest>, Option<Function>)> {
        self._build(sender, true, false).await
    }

    /// Builds [TempoTransactionRequest] without filling missing fields. Used for read-only calls such as
    /// eth_call, eth_estimateGas, etc
    pub async fn build_raw(
        self,
        sender: impl Into<SenderKind<'_>>,
    ) -> Result<(WithOtherFields<TempoTransactionRequest>, Option<Function>)> {
        self._build(sender, false, false).await
    }

    /// Builds an unsigned RLP-encoded raw transaction.
    ///
    /// Returns the hex encoded string representation of the transaction.
    pub async fn build_unsigned_raw(self, from: Address) -> Result<String> {
        let (tx, _) = self._build(SenderKind::Address(from), true, true).await?;
        let tx = tx.inner.build_unsigned()?;
        match tx {
            TempoTypedTransaction::Legacy(t) => Ok(hex::encode_prefixed(t.encoded_for_signing())),
            _ => eyre::bail!("Cannot generate unsigned transaction for non-Ethereum transactions"),
        }
    }

    async fn _build(
        mut self,
        sender: impl Into<SenderKind<'_>>,
        fill: bool,
        unsigned: bool,
    ) -> Result<(WithOtherFields<TempoTransactionRequest>, Option<Function>)> {
        let sender = sender.into();
        let from = sender.address();

        self.tx.set_kind(self.state.kind);

        // we set both fields to the same value because some nodes only accept the legacy `data` field: <https://github.com/foundry-rs/foundry/issues/7764#issuecomment-2210453249>
        self.tx.set_input_kind(self.state.input.clone(), TransactionInputKind::Both);

        self.tx.set_from(from);
        self.tx.set_chain_id(self.chain.id());

        let tx_nonce = if let Some(nonce) = self.tx.nonce() {
            nonce
        } else {
            let nonce = self.provider.get_transaction_count(from).await?;
            if fill {
                self.tx.set_nonce(nonce);
            }
            nonce
        };

        if !unsigned {
            self.resolve_auth(sender, tx_nonce).await?;
        } else if self.auth.is_some() {
            let Some(CliAuthorizationList::Signed(signed_auth)) = self.auth.take() else {
                eyre::bail!(
                    "SignedAuthorization needs to be provided for generating unsigned 7702 txs"
                )
            };

            self.tx.inner.inner.set_authorization_list(vec![signed_auth]);
        }

        if let Some(access_list) = match self.access_list.take() {
            None => None,
            // --access-list provided with no value, call the provider to create it
            Some(None) => {
                let tx = WithOtherFields::new(self.tx.inner.clone().inner);
                Some(self.provider.create_access_list(&tx).await?.access_list)
            },
            // Access list provided as a string, attempt to parse it
            Some(Some(access_list)) => Some(access_list),
        } {
            self.tx.set_access_list(access_list);
        }

        if !fill {
            return Ok((self.tx, self.state.func));
        }

        if self.legacy && self.tx.gas_price().is_none() {
            self.tx.set_gas_price(self.provider.get_gas_price().await?);
        }

        if self.blob && self.tx.inner.inner.max_fee_per_blob_gas.is_none() {
            self.tx.inner.inner.max_fee_per_blob_gas = Some(self.provider.get_blob_base_fee().await?)
        }

        if !self.legacy
            && (self.tx.max_fee_per_gas().is_none() || self.tx.max_priority_fee_per_gas().is_none())
        {
            let estimate = self.provider.estimate_eip1559_fees().await?;

            if self.tx.max_fee_per_gas().is_none() {
                self.tx.set_max_fee_per_gas(estimate.max_fee_per_gas);
            }

            if self.tx.max_priority_fee_per_gas().is_none() {
                self.tx.set_max_priority_fee_per_gas(estimate.max_priority_fee_per_gas);
            }
        }

        if self.tx.inner.inner.gas.is_none() {
            self.estimate_gas().await?;
        }

        Ok((self.tx, self.state.func))
    }

    /// Estimate tx gas from provider call. Tries to decode custom error if execution reverted.
    async fn estimate_gas(&mut self) -> Result<()> {
        let tx = WithOtherFields::new(self.tx.inner.clone().inner);
        match self.provider.estimate_gas(tx).await {
            Ok(estimated) => {
                self.tx.inner.inner.gas = Some(estimated);
                Ok(())
            }
            Err(err) => {
                if let TransportError::ErrorResp(payload) = &err {
                    // If execution reverted with code 3 during provider gas estimation then try
                    // to decode custom errors and append it to the error message.
                    if payload.code == 3
                        && let Some(data) = &payload.data
                        && let Ok(Some(decoded_error)) = crate::tx::decode_execution_revert(data).await
                    {
                        eyre::bail!("Failed to estimate gas: {}: {}", err, decoded_error)
                    }
                }
                eyre::bail!("Failed to estimate gas: {}", err)
            }
        }
    }

    /// Parses the passed --auth value and sets the authorization list on the transaction.
    async fn resolve_auth(&mut self, sender: SenderKind<'_>, tx_nonce: u64) -> Result<()> {
        let Some(auth) = self.auth.take() else { return Ok(()) };

        let auth = match auth {
            CliAuthorizationList::Address(address) => {
                let auth = Authorization {
                    chain_id: U256::from(self.chain.id()),
                    nonce: tx_nonce + 1,
                    address,
                };

                let Some(signer) = sender.as_signer() else {
                    eyre::bail!("No signer available to sign authorization");
                };
                let signature = signer.sign_hash(&auth.signature_hash()).await?;

                auth.into_signed(signature)
            }
            CliAuthorizationList::Signed(auth) => auth,
        };

        self.tx.inner.inner.set_authorization_list(vec![auth]);

        Ok(())
    }
}

impl<P, S> CastTxBuilder<P, S, TempoTransactionRequest>
where
    P: Provider<AnyNetwork>,
{
    /// Populates the blob sidecar for the transaction if any blob data was provided.
    pub fn with_blob_data(mut self, blob_data: Option<Vec<u8>>) -> Result<Self> {
        let Some(blob_data) = blob_data else { return Ok(self) };

        let mut coder = SidecarBuilder::<SimpleCoder>::default();
        coder.ingest(&blob_data);
        let sidecar = coder.build()?;

        self.tx.inner.inner.set_blob_sidecar(sidecar);
        self.tx.inner.inner.populate_blob_hashes();

        Ok(self)
    }
}
