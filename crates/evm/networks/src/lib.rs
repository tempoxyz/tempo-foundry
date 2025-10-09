//! # foundry-evm-networks
//!
//! Foundry EVM network configuration.

use crate::celo::transfer::{
    CELO_TRANSFER_ADDRESS, CELO_TRANSFER_LABEL, PRECOMPILE_ID_CELO_TRANSFER,
};
use alloy_chains::NamedChain;
use alloy_evm::precompiles::PrecompilesMap;
use alloy_primitives::{Address, map::AddressHashMap};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod celo;

#[derive(Clone, Debug, Default, Parser, Copy, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfigs {
    /// Enable Optimism network features.
    #[arg(help_heading = "Networks", long, visible_alias = "optimism", conflicts_with_all = ["celo", "tempo"])]
    // Skipped from configs (forge) as there is no feature to be added yet.
    #[serde(skip)]
    optimism: bool,
    /// Enable Celo network features.
    #[arg(help_heading = "Networks", long, conflicts_with_all = ["optimism", "tempo"])]
    #[serde(default)]
    celo: bool,
    /// Enable Tempo network features.
    #[arg(help_heading = "Networks", long, conflicts_with_all = ["celo", "optimism"])]
    #[serde(default)]
    tempo: bool,
}

impl NetworkConfigs {
    pub fn with_optimism() -> Self {
        Self { optimism: true, ..Default::default() }
    }

    pub fn with_celo() -> Self {
        Self { celo: true, ..Default::default() }
    }

    pub fn with_tempo() -> Self {
        Self { tempo: true, ..Default::default() }
    }

    pub fn is_optimism(&self) -> bool {
        self.optimism
    }

    pub fn is_celo(&self) -> bool {
        self.celo
    }

    pub fn is_tempo(&self) -> bool {
        self.tempo
    }

    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        if let Ok(NamedChain::Celo | NamedChain::CeloSepolia) = NamedChain::try_from(chain_id) {
            self.celo = true;
        }
        self
    }

    /// Inject precompiles for configured networks.
    pub fn inject_precompiles(self, precompiles: &mut PrecompilesMap) {
        if self.celo {
            precompiles.apply_precompile(&CELO_TRANSFER_ADDRESS, move |_| {
                Some(celo::transfer::precompile())
            });
        }
        if self.tempo {
            // todo(onbjerg): chain ID
            tempo_precompiles::precompiles::extend_tempo_precompiles(precompiles, 1337);
        }
    }

    /// Returns precompiles label for configured networks, to be used in traces.
    pub fn precompiles_label(self) -> AddressHashMap<String> {
        let mut labels = AddressHashMap::default();
        if self.celo {
            labels.insert(CELO_TRANSFER_ADDRESS, CELO_TRANSFER_LABEL.to_string());
        }
        labels
    }

    /// Returns precompiles for configured networks.
    pub fn precompiles(self) -> BTreeMap<String, Address> {
        let mut precompiles = BTreeMap::new();
        if self.celo {
            precompiles
                .insert(PRECOMPILE_ID_CELO_TRANSFER.name().to_string(), CELO_TRANSFER_ADDRESS);
        }
        precompiles
    }
}
