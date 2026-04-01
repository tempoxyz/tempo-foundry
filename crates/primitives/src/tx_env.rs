//! Transaction environment conversions for Tempo.
//!
//! This module provides newtype wrappers to bridge Tempo's transaction types
//! with existing Foundry/OP-stack infrastructure.

use alloy_evm::{FromRecoveredTx, IntoTxEnv};
use alloy_op_evm::OpTx;
use alloy_primitives::{Address, Bytes};
use op_revm::{OpTransaction, transaction::deposit::DepositTransactionParts};
use std::ops::{Deref, DerefMut};
use tempo_revm::TempoTxEnv;

use crate::FoundryTxEnvelope;

/// Transaction wrapper for `EitherEvm` that can hold Tempo-specific transaction data.
///
/// This preserves Tempo AA fields (valid_before, valid_after, nonce_key, etc.) that would
/// otherwise be lost when converting through `OpTransaction<TxEnv>`.
#[derive(Clone, Debug, Default)]
pub struct EitherTx {
    /// Base OP transaction (used for Eth and Op EVM variants).
    pub base: OpTx,
    /// Tempo transaction environment (used for Tempo EVM variant).
    /// When present, the Tempo EVM uses this directly instead of converting from `base`.
    pub tempo_tx: Option<TempoTxEnv>,
}

impl IntoTxEnv<Self> for EitherTx {
    fn into_tx_env(self) -> Self {
        self
    }
}

impl IntoTxEnv<EitherTx> for OpTx {
    fn into_tx_env(self) -> EitherTx {
        EitherTx { base: self, tempo_tx: None }
    }
}

/// A newtype wrapper around `TempoTxEnv` that implements conversions needed
/// for compatibility with `EitherEvm`.
///
/// This wrapper allows `TempoTxEnv` to be used in contexts that expect
/// `OpTransaction<TxEnv>`, bridging the gap between Tempo and OP-stack types.
#[derive(Clone, Debug, Default)]
pub struct FoundryTempoTxEnv {
    /// The inner Tempo transaction environment.
    pub inner: TempoTxEnv,
    /// The RLP-encoded transaction bytes for OP-stack L1 fee calculation.
    /// This is only set when running in Optimism mode.
    pub enveloped_tx: Option<Bytes>,
    /// OP-stack deposit transaction parts.
    /// This is only set when running in Optimism mode with deposit transactions.
    pub deposit: DepositTransactionParts,
}

impl FoundryTempoTxEnv {
    /// Creates a new `FoundryTempoTxEnv` from a `TempoTxEnv`.
    pub fn new(tx: TempoTxEnv) -> Self {
        Self { inner: tx, enveloped_tx: None, deposit: DepositTransactionParts::default() }
    }

    /// Creates a new `FoundryTempoTxEnv` with enveloped tx bytes for OP-stack.
    pub fn with_enveloped_tx(tx: TempoTxEnv, enveloped_tx: Option<Bytes>) -> Self {
        Self { inner: tx, enveloped_tx, deposit: DepositTransactionParts::default() }
    }

    /// Consumes the wrapper and returns the inner `TempoTxEnv`.
    pub fn into_inner(self) -> TempoTxEnv {
        self.inner
    }
}

impl From<TempoTxEnv> for FoundryTempoTxEnv {
    fn from(tx: TempoTxEnv) -> Self {
        Self { inner: tx, enveloped_tx: None, deposit: DepositTransactionParts::default() }
    }
}

impl From<FoundryTempoTxEnv> for TempoTxEnv {
    fn from(wrapper: FoundryTempoTxEnv) -> Self {
        wrapper.inner
    }
}

impl Deref for FoundryTempoTxEnv {
    type Target = TempoTxEnv;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for FoundryTempoTxEnv {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Implementation of `IntoTxEnv<EitherTx>` for `FoundryTempoTxEnv`.
///
/// This preserves Tempo-specific fields (fee_token, valid_before, valid_after, nonce_key, etc.)
/// when executing Tempo transactions, while still providing the base `OpTransaction<TxEnv>`
/// for Eth/Op variants.
impl IntoTxEnv<EitherTx> for FoundryTempoTxEnv {
    fn into_tx_env(self) -> EitherTx {
        EitherTx {
            base: OpTx(OpTransaction {
                base: self.inner.inner.clone(),
                enveloped_tx: self.enveloped_tx,
                deposit: self.deposit,
            }),
            // Preserve the full TempoTxEnv for Tempo EVM
            tempo_tx: Some(self.inner),
        }
    }
}

/// Implementation of `FromRecoveredTx<FoundryTxEnvelope>` for `FoundryTempoTxEnv`.
///
/// This allows creating a `FoundryTempoTxEnv` from a recovered `FoundryTxEnvelope`,
/// which is needed for transaction execution in anvil.
impl FromRecoveredTx<FoundryTxEnvelope> for FoundryTempoTxEnv {
    fn from_recovered_tx(tx: &FoundryTxEnvelope, caller: Address) -> Self {
        match tx {
            // Handle Tempo transactions natively using TempoTxEnv's FromRecoveredTx impl
            FoundryTxEnvelope::Tempo(aa_signed) => Self {
                inner: TempoTxEnv::from_recovered_tx(aa_signed, caller),
                enveloped_tx: None,
                deposit: DepositTransactionParts::default(),
            },
            // For all other transaction types, convert through OpTransaction<TxEnv>
            _ => {
                let op_tx: OpTx = FromRecoveredTx::from_recovered_tx(tx, caller);
                Self {
                    inner: TempoTxEnv { inner: op_tx.0.base, ..Default::default() },
                    enveloped_tx: op_tx.0.enveloped_tx,
                    deposit: op_tx.0.deposit,
                }
            }
        }
    }
}
