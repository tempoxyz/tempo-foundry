use alloy_evm::precompiles::DynPrecompile;
use alloy_primitives::Address;
use std::fmt::Debug;

/// Object-safe trait that enables injecting extra precompiles when using
/// `anvil` as a library.
pub trait PrecompileFactory: Send + Sync + Unpin + Debug {
    /// Returns a set of precompiles to extend the EVM with.
    fn precompiles(&self) -> Vec<(Address, DynPrecompile)>;
}
