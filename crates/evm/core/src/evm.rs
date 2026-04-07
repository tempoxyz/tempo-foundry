use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{
    Env, InspectorExt, backend::DatabaseExt, constants::DEFAULT_CREATE2_DEPLOYER_CODEHASH,
};
use alloy_consensus::constants::KECCAK_EMPTY;
use alloy_evm::{Evm, EvmEnv, precompiles::PrecompilesMap};
use alloy_primitives::{Address, Bytes, U256};
use foundry_fork_db::DatabaseError;
use revm::{
    Context, Journal,
    context::{
        ContextTr, CreateScheme, JournalTr, LocalContext, LocalContextTr,
        result::{EVMError, ExecResultAndState, ExecutionResult, ResultAndState, ResultGas},
    },
    handler::{EvmTr, FrameResult, FrameTr, Handler, ItemOrResult},
    inspector::{InspectorEvmTr, InspectorHandler},
    interpreter::{
        CallInput, CallInputs, CallOutcome, CallScheme, CallValue, CreateInputs, CreateOutcome,
        FrameInput, Gas, InitialAndFloorGas, InstructionResult, InterpreterResult, SharedMemory,
        interpreter::EthInterpreter, interpreter_action::FrameInit, return_ok,
    },
};
use tempo_chainspec::hardfork::TempoHardfork;
use tempo_evm::{TempoBlockEnv, TempoHaltReason};
use tempo_revm::{
    TempoEvm, TempoInvalidTransaction, TempoTxEnv, evm::TempoContext, gas_params::tempo_gas_params,
    handler::TempoEvmHandler,
};

pub fn new_evm_with_inspector<'db, I: InspectorExt>(
    db: &'db mut dyn DatabaseExt,
    env: Env,
    inspector: I,
) -> FoundryEvm<'db, I> {
    // Apply TIP-1000 gas params for Tempo hardforks (T0/T1)
    let mut cfg = env.evm_env.cfg_env;
    cfg.gas_params = tempo_gas_params(cfg.spec);

    let mut ctx = TempoContext {
        journaled_state: {
            let mut journal = Journal::new(db);
            journal.set_spec_id(cfg.spec.into());
            journal
        },
        block: env.evm_env.block_env,
        cfg,
        tx: env.tx,
        chain: (),
        local: LocalContext::default(),
        error: Ok(()),
    };
    ctx.cfg.tx_chain_id_check = true;
    let mut evm = FoundryEvm { inner: TempoEvm::new(ctx, inspector) };

    evm.inspector().get_networks().inject_precompiles(evm.precompiles_mut());
    evm
}

pub fn new_evm_with_existing_context<'a>(
    ctx: TempoContext<&'a mut dyn DatabaseExt>,
    inspector: &'a mut dyn InspectorExt,
) -> FoundryEvm<'a, &'a mut dyn InspectorExt> {
    let mut evm = FoundryEvm { inner: TempoEvm::new(ctx, inspector) };

    evm.inspector().get_networks().inject_precompiles(evm.precompiles_mut());
    evm
}

/// Get the call inputs for the CREATE2 factory.
fn get_create2_factory_call_inputs(
    salt: U256,
    inputs: &CreateInputs,
    deployer: Address,
) -> CallInputs {
    let calldata = [&salt.to_be_bytes::<32>()[..], &inputs.init_code()[..]].concat();
    CallInputs {
        caller: inputs.caller(),
        bytecode_address: deployer,
        known_bytecode: None,
        target_address: deployer,
        scheme: CallScheme::Call,
        value: CallValue::Transfer(inputs.value()),
        input: CallInput::Bytes(calldata.into()),
        gas_limit: inputs.gas_limit(),
        is_static: false,
        return_memory_offset: 0..0,
    }
}

pub struct FoundryEvm<'db, I: InspectorExt> {
    inner: TempoEvm<&'db mut dyn DatabaseExt, I>,
}
impl<'db, I: InspectorExt> FoundryEvm<'db, I> {
    /// Consumes the EVM and returns the inner context.
    pub fn into_context(self) -> TempoContext<&'db mut dyn DatabaseExt> {
        self.inner.inner.ctx
    }

    pub fn run_execution(
        &mut self,
        frame: FrameInput,
    ) -> Result<FrameResult, EVMError<DatabaseError, TempoInvalidTransaction>> {
        let mut handler = FoundryHandler::<I>::default();

        // Create first frame
        let memory = SharedMemory::new_with_buffer(
            self.inner.inner.ctx().local().shared_memory_buffer().clone(),
        );
        let first_frame_input = FrameInit { depth: 0, memory, frame_input: frame };

        // Run execution loop
        let mut frame_result = handler.inspect_run_exec_loop(&mut self.inner, first_frame_input)?;

        // Handle last frame result
        handler.last_frame_result(&mut self.inner, &mut frame_result)?;

        Ok(frame_result)
    }
}

impl<'db, I: InspectorExt> Evm for FoundryEvm<'db, I> {
    type Precompiles = PrecompilesMap;
    type Inspector = I;
    type DB = &'db mut dyn DatabaseExt;
    type Error = EVMError<DatabaseError, TempoInvalidTransaction>;
    type HaltReason = TempoHaltReason;
    type Spec = TempoHardfork;
    type Tx = TempoTxEnv;
    type BlockEnv = TempoBlockEnv;

    fn block(&self) -> &TempoBlockEnv {
        &self.inner.block
    }

    fn chain_id(&self) -> u64 {
        self.inner.inner.ctx.cfg.chain_id
    }

    fn components(&self) -> (&Self::DB, &Self::Inspector, &Self::Precompiles) {
        (
            &self.inner.inner.ctx.journaled_state.database,
            &self.inner.inner.inspector,
            &self.inner.inner.precompiles,
        )
    }

    fn components_mut(&mut self) -> (&mut Self::DB, &mut Self::Inspector, &mut Self::Precompiles) {
        (
            &mut self.inner.inner.ctx.journaled_state.database,
            &mut self.inner.inner.inspector,
            &mut self.inner.inner.precompiles,
        )
    }

    fn db_mut(&mut self) -> &mut Self::DB {
        &mut self.inner.inner.ctx.journaled_state.database
    }

    fn precompiles(&self) -> &Self::Precompiles {
        &self.inner.inner.precompiles
    }

    fn precompiles_mut(&mut self) -> &mut Self::Precompiles {
        &mut self.inner.inner.precompiles
    }

    fn inspector(&self) -> &Self::Inspector {
        &self.inner.inner.inspector
    }

    fn inspector_mut(&mut self) -> &mut Self::Inspector {
        &mut self.inner.inner.inspector
    }

    fn set_inspector_enabled(&mut self, _enabled: bool) {
        unimplemented!("FoundryEvm is always inspecting")
    }

    fn transact_raw(
        &mut self,
        tx: Self::Tx,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        self.inner.inner.ctx.tx = tx;

        let mut handler = FoundryHandler::<I>::default();
        let result = handler.inspect_run(&mut self.inner)?;

        Ok(ResultAndState::new(result, self.inner.inner.ctx.journaled_state.inner.state.clone()))
    }

    fn transact_system_call(
        &mut self,
        _caller: Address,
        _contract: Address,
        _data: Bytes,
    ) -> Result<ExecResultAndState<ExecutionResult<TempoHaltReason>>, Self::Error> {
        unimplemented!()
    }

    fn finish(self) -> (Self::DB, EvmEnv<Self::Spec, TempoBlockEnv>)
    where
        Self: Sized,
    {
        let Context { block: block_env, cfg: cfg_env, journaled_state, .. } = self.inner.inner.ctx;

        (journaled_state.database, EvmEnv { block_env, cfg_env })
    }
}

impl<'db, I: InspectorExt> Deref for FoundryEvm<'db, I> {
    type Target = TempoContext<&'db mut dyn DatabaseExt>;

    fn deref(&self) -> &Self::Target {
        &self.inner.inner.ctx
    }
}

impl<I: InspectorExt> DerefMut for FoundryEvm<'_, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.inner.ctx
    }
}

pub struct FoundryHandler<'db, I: InspectorExt> {
    create2_overrides: Vec<(usize, CallInputs)>,
    inner: TempoEvmHandler<&'db mut dyn DatabaseExt, I>,
    _phantom: PhantomData<(&'db mut dyn DatabaseExt, I)>,
}

impl<I: InspectorExt> Default for FoundryHandler<'_, I> {
    fn default() -> Self {
        Self { create2_overrides: Vec::new(), inner: TempoEvmHandler::new(), _phantom: PhantomData }
    }
}

// Blanket Handler implementation for FoundryHandler, needed for implementing the InspectorHandler
// trait.
impl<'db, I: InspectorExt> Handler for FoundryHandler<'db, I> {
    type Evm = TempoEvm<&'db mut dyn DatabaseExt, I>;
    type Error = EVMError<DatabaseError, TempoInvalidTransaction>;
    type HaltReason = TempoHaltReason;

    #[inline]
    fn run(
        &mut self,
        evm: &mut Self::Evm,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        self.inner.run(evm)
    }

    #[inline]
    fn execution(
        &mut self,
        evm: &mut Self::Evm,
        init_floor_and_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        self.inner.execution(evm, init_floor_and_gas)
    }

    #[inline]
    fn validate_against_state_and_deduct_caller(
        &self,
        evm: &mut Self::Evm,
    ) -> Result<(), Self::Error> {
        self.inner.validate_against_state_and_deduct_caller(evm)
    }

    #[inline]
    fn reimburse_caller(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.inner.reimburse_caller(evm, exec_result)
    }

    #[inline]
    fn reward_beneficiary(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.inner.reward_beneficiary(evm, exec_result)
    }

    #[inline]
    fn validate_env(&self, evm: &mut Self::Evm) -> Result<(), Self::Error> {
        self.inner.validate_env(evm)
    }

    #[inline]
    fn validate_initial_tx_gas(
        &self,
        evm: &mut Self::Evm,
    ) -> Result<InitialAndFloorGas, Self::Error> {
        self.inner.validate_initial_tx_gas(evm)
    }

    #[inline]
    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
        result_gas: ResultGas,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        self.inner.execution_result(evm, result, result_gas)
    }

    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        self.inner.catch_error(evm, error)
    }
}

/// Handles CREATE2 frame initialization, potentially transforming it to use the CREATE2 factory.
fn handle_create_frame<I: InspectorExt>(
    create2_overrides: &mut Vec<(usize, CallInputs)>,
    evm: &mut TempoEvm<&mut dyn DatabaseExt, I>,
    init: &mut FrameInit,
) -> Result<Option<FrameResult>, EVMError<DatabaseError, TempoInvalidTransaction>> {
    if let FrameInput::Create(inputs) = &init.frame_input
        && let CreateScheme::Create2 { salt } = inputs.scheme()
    {
        let (ctx, inspector) = evm.ctx_inspector();

        if inspector.should_use_create2_factory(ctx, inputs) {
            let gas_limit = inputs.gas_limit();

            // Get CREATE2 deployer.
            let create2_deployer = evm.inspector().create2_deployer();

            // Generate call inputs for CREATE2 factory.
            let call_inputs = get_create2_factory_call_inputs(salt, inputs, create2_deployer);

            // Push data about current override to the stack.
            create2_overrides.push((evm.journal().depth(), call_inputs.clone()));

            // Sanity check that CREATE2 deployer exists.
            let code_hash = evm.journal_mut().load_account(create2_deployer)?.info.code_hash;
            if code_hash == KECCAK_EMPTY {
                return Ok(Some(FrameResult::Call(CallOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: Bytes::from(
                            format!("missing CREATE2 deployer: {create2_deployer}").into_bytes(),
                        ),
                        gas: Gas::new(gas_limit),
                    },
                    memory_offset: 0..0,
                    was_precompile_called: false,
                    precompile_call_logs: vec![],
                })));
            } else if code_hash != DEFAULT_CREATE2_DEPLOYER_CODEHASH {
                return Ok(Some(FrameResult::Call(CallOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: "invalid CREATE2 deployer bytecode".into(),
                        gas: Gas::new(gas_limit),
                    },
                    memory_offset: 0..0,
                    was_precompile_called: false,
                    precompile_call_logs: vec![],
                })));
            }

            // Rewrite the frame init
            init.frame_input = FrameInput::Call(Box::new(call_inputs));
        }
    }
    Ok(None)
}

/// Transforms CREATE2 factory call results back into CREATE outcomes.
fn handle_create2_override<I: InspectorExt>(
    create2_overrides: &mut Vec<(usize, CallInputs)>,
    evm: &mut TempoEvm<&mut dyn DatabaseExt, I>,
    result: FrameResult,
) -> FrameResult {
    if create2_overrides.last().is_some_and(|(depth, _)| *depth == evm.journal().depth()) {
        let (_, call_inputs) = create2_overrides.pop().unwrap();
        let FrameResult::Call(mut call) = result else {
            unreachable!("create2 override should be a call frame");
        };

        // Decode address from output.
        let address = match call.instruction_result() {
            return_ok!() => Address::try_from(call.output().as_ref())
                .map_err(|_| {
                    call.result = InterpreterResult {
                        result: InstructionResult::Revert,
                        output: "invalid CREATE2 factory output".into(),
                        gas: Gas::new(call_inputs.gas_limit),
                    };
                })
                .ok(),
            _ => None,
        };

        FrameResult::Create(CreateOutcome { result: call.result, address })
    } else {
        result
    }
}

impl<I: InspectorExt> InspectorHandler for FoundryHandler<'_, I> {
    type IT = EthInterpreter;

    /// Overrides `inspect_run` to seed keychain `tx.origin` from the effective
    /// (pranked/broadcast) caller context and load Tempo fee fields before delegating
    /// to the default execution pipeline.
    fn inspect_run(
        &mut self,
        evm: &mut Self::Evm,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        let ctx = evm.ctx();
        let tx_caller = ctx.tx.caller;
        let call_depth = ctx.journaled_state.depth();
        let tx_origin = evm.inspector().tx_origin(tx_caller, call_depth);

        let ctx = evm.ctx_mut();
        tempo_precompiles::storage::StorageCtx::enter_evm(
            &mut ctx.journaled_state,
            &ctx.block,
            &ctx.cfg,
            &ctx.tx,
            || {
                let mut keychain = tempo_precompiles::account_keychain::AccountKeychain::new();
                keychain.set_tx_origin(tx_origin)
            },
        )
        .map_err(|e| EVMError::Custom(e.to_string()))?;

        match self.inspect_run_without_catch_error(evm) {
            Ok(output) => Ok(output),
            Err(e) => self.catch_error(evm, e),
        }
    }

    /// Delegates to `TempoEvmHandler::inspect_execution_with`, injecting the CREATE2 factory
    /// routing exec loop for standard transactions.
    ///
    /// Tempo-specific gas and AA multi-call dispatch are handled by `inspect_execution_with`.
    #[inline]
    fn inspect_execution(
        &mut self,
        evm: &mut Self::Evm,
        init_and_floor_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        let overrides = &mut self.create2_overrides;
        self.inner.inspect_execution_with(evm, init_and_floor_gas, |_handler, evm, init| {
            create2_exec_loop(overrides, evm, init)
        })
    }

    fn inspect_run_exec_loop(
        &mut self,
        evm: &mut Self::Evm,
        first_frame_input: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameInit,
    ) -> Result<FrameResult, Self::Error> {
        create2_exec_loop(&mut self.create2_overrides, evm, first_frame_input)
    }
}

/// Runs the CREATE2 factory routing exec loop.
fn create2_exec_loop<I: InspectorExt>(
    create2_overrides: &mut Vec<(usize, CallInputs)>,
    evm: &mut TempoEvm<&mut dyn DatabaseExt, I>,
    first_frame_input: FrameInit,
) -> Result<FrameResult, EVMError<DatabaseError, TempoInvalidTransaction>> {
    let res = evm.inspect_frame_init(first_frame_input)?;

    if let ItemOrResult::Result(frame_result) = res {
        return Ok(frame_result);
    }

    loop {
        let call_or_result = evm.inspect_frame_run()?;

        let result = match call_or_result {
            ItemOrResult::Item(mut init) => {
                // Handle CREATE/CREATE2 frame initialization
                if let Some(frame_result) = handle_create_frame(create2_overrides, evm, &mut init)?
                {
                    return Ok(frame_result);
                }

                match evm.inspect_frame_init(init)? {
                    ItemOrResult::Item(_) => continue,
                    ItemOrResult::Result(result) => result,
                }
            }
            ItemOrResult::Result(result) => result,
        };

        let result = handle_create2_override(create2_overrides, evm, result);

        if let Some(result) = evm.frame_return_result(result)? {
            return Ok(result);
        }
    }
}
