use alloy_evm::{Database, EthEvm, Evm, EvmEnv, eth::EthEvmContext, precompiles::PrecompilesMap};
use alloy_op_evm::{OpEvm, OpTxError};
use alloy_primitives::{Address, Bytes};
use op_revm::{OpContext, OpHaltReason, OpSpecId, OpTransactionError};
use revm::{
    DatabaseCommit, Inspector,
    context::{
        BlockEnv,
        result::{EVMError, ExecResultAndState, ExecutionResult, HaltReason, ResultAndState},
    },
    handler::PrecompileProvider,
    interpreter::InterpreterResult,
    primitives::hardfork::SpecId,
};
use tempo_revm::{TempoHaltReason, TempoInvalidTransaction, TempoTxEnv, evm::TempoContext};

pub use foundry_primitives::EitherTx;

/// Alias for result type returned by [`Evm::transact`] methods.
type EitherEvmResult<DBError, HaltReason, TxError> =
    Result<ResultAndState<HaltReason>, EVMError<DBError, TxError>>;

/// Alias for result type returned by [`Evm::transact_commit`] methods.
type EitherExecResult<DBError, HaltReason, TxError> =
    Result<ExecutionResult<HaltReason>, EVMError<DBError, TxError>>;

/// [`EitherEvm`] delegates its calls to one of the three evm implementations: [`EthEvm`],
/// [`OpEvm`], or [`tempo_revm::TempoEvm`].
///
/// Calls are delegated to [`OpEvm`] if optimism is enabled, or [`tempo_revm::TempoEvm`] if
/// tempo is enabled.
///
/// The call delegation is handled via its own implementation of the [`Evm`] trait.
///
/// The [`Evm::transact`] and other such calls work over the [`OpTransaction<TxEnv>`] type.
///
/// However, the [`Evm::HaltReason`] and [`Evm::Error`] leverage the optimism [`OpHaltReason`] and
/// [`OpTransactionError`] as these are supersets of the eth types. This makes it easier to map eth
/// types to op types and also prevents ignoring of any error that maybe thrown by [`OpEvm`].
/// Tempo errors are mapped to custom errors in these types.
#[allow(clippy::large_enum_variant)]
pub enum EitherEvm<DB, I, P>
where
    DB: Database,
{
    /// [`EthEvm`] implementation.
    Eth(EthEvm<DB, I, P>),
    /// [`OpEvm`] implementation.
    Op(OpEvm<DB, I, P>),
    /// [`tempo_revm::TempoEvm`] implementation.
    Tempo(tempo_revm::TempoEvm<DB, I>),
}

impl<DB, I, P> EitherEvm<DB, I, P>
where
    DB: Database,
    I: Inspector<EthEvmContext<DB>> + Inspector<OpContext<DB>> + Inspector<TempoContext<DB>>,
    P: PrecompileProvider<EthEvmContext<DB>, Output = InterpreterResult>
        + PrecompileProvider<OpContext<DB>, Output = InterpreterResult>,
{
    /// Converts the [`EthEvm::transact`] result to [`EitherEvmResult`].
    fn map_eth_result(
        &self,
        result: Result<ExecResultAndState<ExecutionResult>, EVMError<DB::Error>>,
    ) -> EitherEvmResult<DB::Error, OpHaltReason, OpTxError> {
        match result {
            Ok(result) => Ok(ResultAndState {
                result: result.result.map_haltreason(OpHaltReason::Base),
                state: result.state,
            }),
            Err(e) => Err(self.map_eth_err(e)),
        }
    }

    /// Converts the [`EthEvm::transact_commit`] result to [`EitherExecResult`].
    fn map_exec_result(
        &self,
        result: Result<ExecutionResult, EVMError<DB::Error>>,
    ) -> EitherExecResult<DB::Error, OpHaltReason, OpTxError> {
        match result {
            Ok(result) => {
                // Map the halt reason
                Ok(result.map_haltreason(OpHaltReason::Base))
            }
            Err(e) => Err(self.map_eth_err(e)),
        }
    }

    /// Maps [`EVMError<DBError>`] to [`EVMError<DBError, OpTxError>`].
    fn map_eth_err(&self, err: EVMError<DB::Error>) -> EVMError<DB::Error, OpTxError> {
        match err {
            EVMError::Transaction(invalid_tx) => {
                EVMError::Transaction(OpTxError(OpTransactionError::Base(invalid_tx)))
            }
            EVMError::Database(e) => EVMError::Database(e),
            EVMError::Header(e) => EVMError::Header(e),
            EVMError::Custom(e) => EVMError::Custom(e),
        }
    }

    /// Converts a [`tempo_revm::TempoEvm::transact`] result to [`EitherEvmResult`].
    fn map_tempo_result(
        &self,
        result: Result<
            ResultAndState<TempoHaltReason>,
            EVMError<DB::Error, TempoInvalidTransaction>,
        >,
    ) -> EitherEvmResult<DB::Error, OpHaltReason, OpTxError> {
        match result {
            Ok(result) => Ok(ResultAndState {
                result: result.result.map_haltreason(map_tempo_halt_to_op),
                state: result.state,
            }),
            Err(e) => Err(map_tempo_err_to_op(e)),
        }
    }

    /// Converts a [`tempo_revm::TempoEvm::transact_commit`] result to [`EitherExecResult`].
    fn map_tempo_exec_result(
        &self,
        result: Result<
            ExecutionResult<TempoHaltReason>,
            EVMError<DB::Error, TempoInvalidTransaction>,
        >,
    ) -> EitherExecResult<DB::Error, OpHaltReason, OpTxError> {
        match result {
            Ok(result) => Ok(result.map_haltreason(map_tempo_halt_to_op)),
            Err(e) => Err(map_tempo_err_to_op(e)),
        }
    }
}

/// Maps [`TempoHaltReason`] to [`OpHaltReason`].
fn map_tempo_halt_to_op(halt: TempoHaltReason) -> OpHaltReason {
    match halt {
        TempoHaltReason::Ethereum(h) => OpHaltReason::Base(h),
        TempoHaltReason::SubblockTxFeePayment => {
            // Map Tempo fee payment halt to PrecompileError since fee payment
            // involves interactions with Tempo precompiles (FeeAMM, etc.)
            OpHaltReason::Base(HaltReason::PrecompileError)
        }
    }
}

/// Maps [`EVMError<DBError, TempoInvalidTransaction>`] to [`EVMError<DBError,
/// OpTransactionError>`].
fn map_tempo_err_to_op<DBError>(
    err: EVMError<DBError, TempoInvalidTransaction>,
) -> EVMError<DBError, OpTxError> {
    match err {
        EVMError::Transaction(tempo_err) => match tempo_err {
            TempoInvalidTransaction::EthInvalidTransaction(eth_err) => {
                EVMError::Transaction(OpTxError(OpTransactionError::Base(eth_err)))
            }
            other => EVMError::Custom(other.to_string()),
        },
        EVMError::Database(e) => EVMError::Database(e),
        EVMError::Header(e) => EVMError::Header(e),
        EVMError::Custom(e) => EVMError::Custom(e),
    }
}

impl<DB, I, P> Evm for EitherEvm<DB, I, P>
where
    DB: Database,
    I: Inspector<EthEvmContext<DB>> + Inspector<OpContext<DB>> + Inspector<TempoContext<DB>>,
    P: PrecompileProvider<EthEvmContext<DB>, Output = InterpreterResult>
        + PrecompileProvider<OpContext<DB>, Output = InterpreterResult>
        + From<PrecompilesMap>,
{
    type DB = DB;
    type Error = EVMError<DB::Error, OpTxError>;
    type HaltReason = OpHaltReason;
    type Tx = EitherTx;
    type Inspector = I;
    type Precompiles = P;
    type Spec = SpecId;
    type BlockEnv = BlockEnv;

    fn block(&self) -> &BlockEnv {
        match self {
            Self::Eth(evm) => evm.block(),
            Self::Op(evm) => evm.block(),
            Self::Tempo(evm) => &evm.inner.ctx.block.inner,
        }
    }

    fn chain_id(&self) -> u64 {
        match self {
            Self::Eth(evm) => evm.chain_id(),
            Self::Op(evm) => evm.chain_id(),
            Self::Tempo(evm) => evm.inner.ctx.cfg.chain_id,
        }
    }

    fn components(&self) -> (&Self::DB, &Self::Inspector, &Self::Precompiles) {
        match self {
            Self::Eth(evm) => evm.components(),
            Self::Op(evm) => evm.components(),
            Self::Tempo(_) => {
                // Tempo variant doesn't support components() due to type mismatch
                // This should not be called for Tempo - use specific accessors instead
                panic!("components() not supported for Tempo EVM variant")
            }
        }
    }

    fn components_mut(&mut self) -> (&mut Self::DB, &mut Self::Inspector, &mut Self::Precompiles) {
        match self {
            Self::Eth(evm) => evm.components_mut(),
            Self::Op(evm) => evm.components_mut(),
            Self::Tempo(_) => {
                panic!("components_mut() not supported for Tempo EVM variant")
            }
        }
    }

    fn db_mut(&mut self) -> &mut Self::DB {
        match self {
            Self::Eth(evm) => evm.db_mut(),
            Self::Op(evm) => evm.db_mut(),
            Self::Tempo(evm) => &mut evm.inner.ctx.journaled_state.database,
        }
    }

    fn into_db(self) -> Self::DB
    where
        Self: Sized,
    {
        match self {
            Self::Eth(evm) => evm.into_db(),
            Self::Op(evm) => evm.into_db(),
            Self::Tempo(evm) => evm.inner.ctx.journaled_state.database,
        }
    }

    fn finish(self) -> (Self::DB, EvmEnv<Self::Spec>)
    where
        Self: Sized,
    {
        match self {
            Self::Eth(evm) => evm.finish(),
            Self::Op(evm) => {
                let (db, env) = evm.finish();
                (db, map_env(env))
            }
            Self::Tempo(evm) => {
                let spec_id: SpecId = evm.inner.ctx.cfg.spec.into();
                let env = EvmEnv::new(
                    evm.inner.ctx.cfg.with_spec_and_mainnet_gas_params(spec_id),
                    evm.inner.ctx.block.inner,
                );
                (evm.inner.ctx.journaled_state.database, env)
            }
        }
    }

    fn precompiles(&self) -> &Self::Precompiles {
        match self {
            Self::Eth(evm) => evm.precompiles(),
            Self::Op(evm) => evm.precompiles(),
            Self::Tempo(evm) => {
                // SAFETY: This transmute is sound only when P == PrecompilesMap.
                // In Anvil, EitherEvm is always instantiated with P = PrecompilesMap
                // (see executor.rs and mem/mod.rs). The `From<PrecompilesMap>` bound
                // ensures P is at least convertible from PrecompilesMap, and in practice
                // P is always exactly PrecompilesMap in this codebase.
                unsafe { std::mem::transmute::<&PrecompilesMap, &P>(&evm.inner.precompiles) }
            }
        }
    }

    fn precompiles_mut(&mut self) -> &mut Self::Precompiles {
        match self {
            Self::Eth(evm) => evm.precompiles_mut(),
            Self::Op(evm) => evm.precompiles_mut(),
            Self::Tempo(evm) => {
                // SAFETY: This transmute is sound only when P == PrecompilesMap.
                // In Anvil, EitherEvm is always instantiated with P = PrecompilesMap
                // (see executor.rs and mem/mod.rs). The `From<PrecompilesMap>` bound
                // ensures P is at least convertible from PrecompilesMap, and in practice
                // P is always exactly PrecompilesMap in this codebase.
                unsafe {
                    std::mem::transmute::<&mut PrecompilesMap, &mut P>(&mut evm.inner.precompiles)
                }
            }
        }
    }

    fn inspector(&self) -> &Self::Inspector {
        match self {
            Self::Eth(evm) => evm.inspector(),
            Self::Op(evm) => evm.inspector(),
            Self::Tempo(_) => {
                panic!("inspector() not supported for Tempo EVM variant")
            }
        }
    }

    fn inspector_mut(&mut self) -> &mut Self::Inspector {
        match self {
            Self::Eth(evm) => evm.inspector_mut(),
            Self::Op(evm) => evm.inspector_mut(),
            Self::Tempo(_) => {
                panic!("inspector_mut() not supported for Tempo EVM variant")
            }
        }
    }

    fn enable_inspector(&mut self) {
        match self {
            Self::Eth(evm) => evm.enable_inspector(),
            Self::Op(evm) => evm.enable_inspector(),
            Self::Tempo(_) => {
                // Tempo always has inspector enabled
            }
        }
    }

    fn disable_inspector(&mut self) {
        match self {
            Self::Eth(evm) => evm.disable_inspector(),
            Self::Op(evm) => evm.disable_inspector(),
            Self::Tempo(_) => {
                // Tempo doesn't support disabling inspector
            }
        }
    }

    fn set_inspector_enabled(&mut self, enabled: bool) {
        match self {
            Self::Eth(evm) => evm.set_inspector_enabled(enabled),
            Self::Op(evm) => evm.set_inspector_enabled(enabled),
            Self::Tempo(_) => {
                // Tempo doesn't support toggling inspector
                let _ = enabled;
            }
        }
    }

    fn into_env(self) -> EvmEnv<Self::Spec>
    where
        Self: Sized,
    {
        match self {
            Self::Eth(evm) => evm.into_env(),
            Self::Op(evm) => map_env(evm.into_env()),
            Self::Tempo(evm) => {
                let spec_id: SpecId = evm.inner.ctx.cfg.spec.into();
                EvmEnv::new(
                    evm.inner.ctx.cfg.with_spec_and_mainnet_gas_params(spec_id),
                    evm.inner.ctx.block.inner,
                )
            }
        }
    }

    fn transact(
        &mut self,
        tx: impl alloy_evm::IntoTxEnv<Self::Tx>,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        let tx_env = tx.into_tx_env();
        match self {
            Self::Eth(evm) => {
                let eth = evm.transact(tx_env.base.0.base);
                self.map_eth_result(eth)
            }
            Self::Op(evm) => evm.transact(tx_env.base),
            Self::Tempo(evm) => {
                use revm::ExecuteEvm;
                // Use tempo_tx if present (Tempo AA transactions), otherwise convert from base
                let tempo_tx =
                    tx_env.tempo_tx.unwrap_or_else(|| TempoTxEnv::from(tx_env.base.0.base));
                let result = evm.transact(tempo_tx);
                self.map_tempo_result(result)
            }
        }
    }

    fn transact_commit(
        &mut self,
        tx: impl alloy_evm::IntoTxEnv<Self::Tx>,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error>
    where
        Self::DB: DatabaseCommit,
    {
        let tx_env = tx.into_tx_env();
        match self {
            Self::Eth(evm) => {
                let eth = evm.transact_commit(tx_env.base.0.base);
                self.map_exec_result(eth)
            }
            Self::Op(evm) => evm.transact_commit(tx_env.base),
            Self::Tempo(evm) => {
                use revm::ExecuteCommitEvm;
                // Use tempo_tx if present (Tempo AA transactions), otherwise convert from base
                let tempo_tx =
                    tx_env.tempo_tx.unwrap_or_else(|| TempoTxEnv::from(tx_env.base.0.base));
                tracing::warn!(target: "backend", has_tempo_tx_env = tempo_tx.tempo_tx_env.is_some(), "transact_commit tempo tx");
                let result = evm.transact_commit(tempo_tx);
                self.map_tempo_exec_result(result)
            }
        }
    }

    fn transact_raw(
        &mut self,
        tx: Self::Tx,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        match self {
            Self::Eth(evm) => {
                let res = evm.transact_raw(tx.base.0.base);
                self.map_eth_result(res)
            }
            Self::Op(evm) => evm.transact_raw(tx.base),
            Self::Tempo(evm) => {
                use revm::ExecuteEvm;
                // Use tempo_tx if present (Tempo AA transactions), otherwise convert from base
                let tempo_tx = tx.tempo_tx.unwrap_or_else(|| TempoTxEnv::from(tx.base.0.base));
                let result = evm.transact(tempo_tx);
                self.map_tempo_result(result)
            }
        }
    }

    fn transact_system_call(
        &mut self,
        caller: Address,
        contract: Address,
        data: Bytes,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        match self {
            Self::Eth(evm) => {
                let eth = evm.transact_system_call(caller, contract, data);
                self.map_eth_result(eth)
            }
            Self::Op(evm) => evm.transact_system_call(caller, contract, data),
            Self::Tempo(_evm) => {
                // Tempo doesn't have a specific system call implementation
                // Use a regular call with system-like parameters
                Err(EVMError::Custom("system calls not supported for Tempo".to_string()))
            }
        }
    }
}

/// Maps [`EvmEnv<OpSpecId>`] to [`EvmEnv`].
fn map_env(env: EvmEnv<OpSpecId>) -> EvmEnv {
    let eth_spec_id = env.spec_id().into_eth_spec();
    let cfg = env.cfg_env.with_spec_and_mainnet_gas_params(eth_spec_id);
    EvmEnv { cfg_env: cfg, block_env: env.block_env }
}
