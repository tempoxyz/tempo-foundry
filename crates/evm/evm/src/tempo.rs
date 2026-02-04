use alloy_primitives::U256;
use foundry_evm_core::{
    constants::{CALLER, TEST_CONTRACT_ADDRESS},
    tempo::{FoundryStorageProvider, initialize_tempo_genesis},
};
use foundry_evm_hardforks::FoundryHardfork;
use tempo_precompiles::error::TempoPrecompileError;

use crate::executors::Executor;

/// Initialize Tempo precompiles and contracts for the given executor.
/// This initialization should be kept aligned with Tempo's genesis file to ensure
/// executor environments accurately reflect production behavior.
///
/// Ref: <https://github.com/tempoxyz/tempo/blob/main/xtask/src/genesis_args.rs>
pub fn initialize_tempo_precompiles_and_contracts(
    executor: &mut Executor,
    hardfork: Option<FoundryHardfork>,
) -> Result<(), TempoPrecompileError> {
    let sender = CALLER;
    let admin = TEST_CONTRACT_ADDRESS;

    let chain_id = executor.env().evm_env.cfg_env.chain_id;
    let timestamp = U256::from(executor.env().evm_env.block_env.timestamp);
    let tempo_hardfork = hardfork
        .and_then(|hf| match hf {
            FoundryHardfork::Tempo(t) => Some(t),
            _ => None,
        })
        .unwrap_or_default();
    let mut storage =
        FoundryStorageProvider::new(executor.backend_mut(), chain_id, timestamp, tempo_hardfork);

    initialize_tempo_genesis(&mut storage, admin, sender)
}
