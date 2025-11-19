use alloy_primitives::{Address, U256};
use revm::{
    Database,
    state::{AccountInfo, Bytecode},
};
use tempo_chainspec::hardfork::TempoHardfork;
use tempo_precompiles::error::TempoPrecompileError;

use crate::backend::Backend;

/// Storage provider adapter for Foundry's backend to work with Tempo precompiles.
///
/// This wraps Foundry's backend to implement the `PrecompileStorageProvider` trait,
/// enabling use of canonical Tempo initialization logic.
pub struct FoundryStorageProvider<'a> {
    backend: &'a mut Backend,
    chain_id: u64,
    timestamp: U256,
    gas_used: u64,
}

impl<'a> FoundryStorageProvider<'a> {
    pub fn new(backend: &'a mut Backend, chain_id: u64, timestamp: U256) -> Self {
        Self { backend, chain_id, timestamp, gas_used: 0 }
    }
}

impl<'a> tempo_precompiles::storage::PrecompileStorageProvider for FoundryStorageProvider<'a> {
    fn spec(&self) -> TempoHardfork {
        self.backend.spec_id().into()
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn timestamp(&self) -> U256 {
        self.timestamp
    }

    fn set_code(&mut self, address: Address, code: Bytecode) -> Result<(), TempoPrecompileError> {
        self.backend.insert_account_info(
            address,
            AccountInfo { code_hash: code.hash_slow(), code: Some(code), ..Default::default() },
        );
        Ok(())
    }

    fn get_account_info(
        &mut self,
        _address: Address,
    ) -> Result<&AccountInfo, TempoPrecompileError> {
        // Not needed for test initialization
        Err(TempoPrecompileError::Fatal(
            "get_account_info not supported in test initialization".to_string(),
        ))
    }

    fn sstore(
        &mut self,
        address: Address,
        key: U256,
        value: U256,
    ) -> Result<(), TempoPrecompileError> {
        self.backend
            .insert_account_storage(address, key, value)
            .map_err(|e| TempoPrecompileError::Fatal(e.to_string()))
    }

    fn sload(&mut self, address: Address, key: U256) -> Result<U256, TempoPrecompileError> {
        self.backend.storage(address, key).map_err(|e| TempoPrecompileError::Fatal(e.to_string()))
    }

    fn tstore(
        &mut self,
        _address: Address,
        _key: U256,
        _value: U256,
    ) -> Result<(), TempoPrecompileError> {
        // This is a no-op during initialization as temporal storage is not persisted
        Ok(())
    }

    fn tload(&mut self, _address: Address, _key: U256) -> Result<U256, TempoPrecompileError> {
        // Temporal storage is empty during initialization
        Ok(U256::ZERO)
    }

    fn emit_event(
        &mut self,
        _address: Address,
        _event: alloy_primitives::LogData,
    ) -> Result<(), TempoPrecompileError> {
        // Events during initialization are not captured in test setup
        // This is acceptable as initialization events aren't tested
        Ok(())
    }

    fn deduct_gas(&mut self, gas: u64) -> Result<(), TempoPrecompileError> {
        // Track gas for accounting purposes, but don't enforce limits during init
        self.gas_used = self.gas_used.saturating_add(gas);
        Ok(())
    }

    fn gas_used(&self) -> u64 {
        self.gas_used
    }

    fn beneficiary(&self) -> Address {
        // note(onbjerg): this doesn't matter during initialization so we can safely set it to
        // address zero. during execution the evm will set this to an appropriate value.
        Address::ZERO
    }
}
