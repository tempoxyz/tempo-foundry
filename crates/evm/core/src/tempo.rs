use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use revm::{
    Database,
    state::{AccountInfo, Bytecode},
};
use tempo_chainspec::hardfork::TempoHardfork;
use tempo_precompiles::{error::TempoPrecompileError, storage::PrecompileStorageProvider};

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
    gas_refunded: i64,
    transient: HashMap<(Address, U256), U256>,
    beneficiary: Address,
    spec: TempoHardfork,
}

impl<'a> FoundryStorageProvider<'a> {
    pub fn new(
        backend: &'a mut Backend,
        chain_id: u64,
        timestamp: U256,
        spec: TempoHardfork,
    ) -> Self {
        Self {
            backend,
            chain_id,
            timestamp,
            gas_used: 0,
            gas_refunded: 0,
            transient: HashMap::new(),
            beneficiary: Address::ZERO,
            spec,
        }
    }
}

impl<'a> PrecompileStorageProvider for FoundryStorageProvider<'a> {
    fn spec(&self) -> TempoHardfork {
        self.spec
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
            AccountInfo {
                code_hash: code.hash_slow(),
                code: Some(code),
                nonce: 1,
                ..Default::default()
            },
        );
        Ok(())
    }

    fn with_account_info(
        &mut self,
        _address: Address,
        _f: &mut dyn FnMut(&AccountInfo),
    ) -> Result<(), TempoPrecompileError> {
        Ok(())
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
        address: Address,
        key: U256,
        value: U256,
    ) -> Result<(), TempoPrecompileError> {
        self.transient.insert((address, key), value);
        Ok(())
    }

    fn tload(&mut self, address: Address, key: U256) -> Result<U256, TempoPrecompileError> {
        Ok(self.transient.get(&(address, key)).copied().unwrap_or(U256::ZERO))
    }

    fn emit_event(
        &mut self,
        _address: Address,
        _event: alloy_primitives::LogData,
    ) -> Result<(), TempoPrecompileError> {
        Ok(())
    }

    fn deduct_gas(&mut self, gas: u64) -> Result<(), TempoPrecompileError> {
        self.gas_used = self.gas_used.saturating_add(gas);
        Ok(())
    }

    fn gas_used(&self) -> u64 {
        self.gas_used
    }

    fn gas_refunded(&self) -> i64 {
        self.gas_refunded
    }

    fn refund_gas(&mut self, gas: i64) {
        self.gas_refunded = self.gas_refunded.saturating_add(gas);
    }

    fn beneficiary(&self) -> Address {
        self.beneficiary
    }
}
