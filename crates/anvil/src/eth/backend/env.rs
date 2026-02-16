use alloy_evm::EvmEnv;
use foundry_evm::{EnvMut, core::AsEnvMut};
use foundry_evm_networks::NetworkConfigs;
use foundry_primitives::FoundryTempoTxEnv;
use tempo_chainspec::hardfork::TempoHardfork;
use tempo_evm::TempoBlockEnv;
use tempo_revm::TempoTxEnv;

/// Helper container type for [`EvmEnv`] and [`FoundryTempoTxEnv`].
///
/// The transaction is wrapped in `FoundryTempoTxEnv` to provide the necessary
/// trait implementations for compatibility with `EitherEvm`.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub evm_env: EvmEnv<TempoHardfork, TempoBlockEnv>,
    pub tx: FoundryTempoTxEnv,
    pub networks: NetworkConfigs,
}

impl Env {
    pub fn new(
        evm_env: EvmEnv<TempoHardfork, TempoBlockEnv>,
        tx: TempoTxEnv,
        networks: NetworkConfigs,
    ) -> Self {
        Self { evm_env, tx: FoundryTempoTxEnv::new(tx), networks }
    }

    /// Creates a new `Env` with a `FoundryTempoTxEnv`, preserving the `enveloped_tx` field.
    ///
    /// Use this when the transaction has OP-stack L1 fee data that must be preserved.
    pub fn with_foundry_tx(
        evm_env: EvmEnv<TempoHardfork, TempoBlockEnv>,
        tx: FoundryTempoTxEnv,
        networks: NetworkConfigs,
    ) -> Self {
        Self { evm_env, tx, networks }
    }
}

impl AsEnvMut for Env {
    fn as_env_mut(&mut self) -> EnvMut<'_> {
        EnvMut {
            block: &mut self.evm_env.block_env,
            cfg: &mut self.evm_env.cfg_env,
            tx: &mut self.tx.inner,
        }
    }
}
