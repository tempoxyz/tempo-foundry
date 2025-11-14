use std::fmt::Debug;

use alloy_evm::{
    Database, Evm,
    eth::EthEvmContext,
    precompiles::{DynPrecompile, PrecompilesMap},
};
use alloy_primitives::Address;
use foundry_evm::core::either_evm::EitherEvm;
use op_revm::OpContext;
use revm::Inspector;

/// Object-safe trait that enables injecting extra precompiles when using
/// `anvil` as a library.
pub trait PrecompileFactory: Send + Sync + Unpin + Debug {
    /// Returns a set of precompiles to extend the EVM with.
    fn precompiles(&self) -> Vec<(Address, DynPrecompile)>;
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use crate::PrecompileFactory;
    use alloy_evm::{
        EthEvm, Evm, EvmEnv,
        eth::EthEvmContext,
        precompiles::{DynPrecompile, PrecompilesMap},
    };
    use alloy_op_evm::OpEvm;
    use alloy_primitives::{Address, Bytes, TxKind, U256, address};
    use foundry_evm::core::either_evm::EitherEvm;
    use foundry_evm_networks::NetworkConfigs;
    use itertools::Itertools;
    use op_revm::{L1BlockInfo, OpContext, OpSpecId, OpTransaction, precompiles::OpPrecompiles};
    use revm::{
        Journal,
        context::{CfgEnv, Evm as RevmEvm, JournalTr, LocalContext, TxEnv},
        database::{EmptyDB, EmptyDBTyped},
        handler::{EthPrecompiles, instructions::EthInstructions},
        inspector::NoOpInspector,
        interpreter::interpreter::EthInterpreter,
        precompile::{PrecompileOutput, PrecompileSpecId, Precompiles},
        primitives::hardfork::SpecId,
    };
    use tempo_revm::{TempoTxEnv, evm::TempoContext};

    // A precompile activated in the `Prague` spec.
    const ETH_PRAGUE_PRECOMPILE: Address = address!("0x0000000000000000000000000000000000000011");

    // A precompile activated in the `Isthmus` spec.
    const OP_ISTHMUS_PRECOMPILE: Address = address!("0x0000000000000000000000000000000000000100");

    // A custom precompile address and payload for testing.
    const PRECOMPILE_ADDR: Address = address!("0x0000000000000000000000000000000000000071");
    const PAYLOAD: &[u8] = &[0xde, 0xad, 0xbe, 0xef];

    #[derive(Debug)]
    struct CustomPrecompileFactory;

    impl PrecompileFactory for CustomPrecompileFactory {
        fn precompiles(&self) -> Vec<(Address, DynPrecompile)> {
            use alloy_evm::precompiles::PrecompileInput;
            vec![(
                PRECOMPILE_ADDR,
                DynPrecompile::from(|input: PrecompileInput<'_>| {
                    Ok(PrecompileOutput {
                        bytes: Bytes::copy_from_slice(input.data),
                        gas_used: 0,
                        gas_refunded: 0,
                        reverted: false,
                    })
                }),
            )]
        }
    }

    /// Custom precompile that echoes the input data.
    /// In this example it uses `0xdeadbeef` as the input data, returning it as output.
    fn custom_echo_precompile(input: &[u8], _gas_limit: u64) -> PrecompileResult {
        Ok(PrecompileOutput { bytes: Bytes::copy_from_slice(input), gas_used: 0, reverted: false })
    }
}
