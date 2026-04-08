use std::str::FromStr;

use alloy_rpc_types::BlockNumberOrTag;
use op_revm::OpSpecId;
use revm::primitives::hardfork::SpecId;
use serde::{Deserialize, Serialize};

pub use alloy_hardforks::EthereumHardfork;
pub use alloy_op_hardforks::OpHardfork;
pub use tempo_chainspec::hardfork::TempoHardfork;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(into = "String")]
pub enum FoundryHardfork {
    Ethereum(EthereumHardfork),
    Optimism(OpHardfork),
    Tempo(TempoHardfork),
}

impl From<FoundryHardfork> for String {
    fn from(fork: FoundryHardfork) -> Self {
        match fork {
            FoundryHardfork::Ethereum(h) => format!("{h}"),
            FoundryHardfork::Optimism(h) => format!("optimism:{h}"),
            FoundryHardfork::Tempo(h) => format!("tempo:{h:?}"),
        }
    }
}

impl<'de> Deserialize<'de> for FoundryHardfork {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl FromStr for FoundryHardfork {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();

        let Some((ns, fork_raw)) = raw.split_once(':') else {
            return EthereumHardfork::from_str(raw)
                .map(Self::Ethereum)
                .map_err(|_| format!("unknown ethereum hardfork '{raw}'"));
        };

        let ns = ns.trim().to_ascii_lowercase();
        let fork = fork_raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");

        match ns.as_str() {
            "eth" | "ethereum" => EthereumHardfork::from_str(&fork)
                .map(Self::Ethereum)
                .map_err(|_| format!("unknown ethereum hardfork '{fork_raw}'")),

            "op" | "optimism" => OpHardfork::from_str(&fork)
                .map(Self::Optimism)
                .map_err(|_| format!("unknown optimism hardfork '{fork_raw}'")),

            "t" | "tempo" => TempoHardfork::from_str(&fork)
                .map(Self::Tempo)
                .map_err(|_| format!("unknown tempo hardfork '{fork_raw}'")),
            _ => EthereumHardfork::from_str(&fork)
                .map(Self::Ethereum)
                .map_err(|_| format!("unknown hardfork '{raw}'")),
        }
    }
}

impl FoundryHardfork {
    pub fn ethereum(h: EthereumHardfork) -> Self {
        Self::Ethereum(h)
    }

    pub fn optimism(h: OpHardfork) -> Self {
        Self::Optimism(h)
    }

    pub fn tempo(h: TempoHardfork) -> Self {
        Self::Tempo(h)
    }
}

impl From<EthereumHardfork> for FoundryHardfork {
    fn from(value: EthereumHardfork) -> Self {
        Self::Ethereum(value)
    }
}

impl From<FoundryHardfork> for EthereumHardfork {
    fn from(fork: FoundryHardfork) -> Self {
        match fork {
            FoundryHardfork::Ethereum(hardfork) => hardfork,
            _ => Self::default(),
        }
    }
}

impl From<OpHardfork> for FoundryHardfork {
    fn from(value: OpHardfork) -> Self {
        Self::Optimism(value)
    }
}

impl From<FoundryHardfork> for OpHardfork {
    fn from(fork: FoundryHardfork) -> Self {
        match fork {
            FoundryHardfork::Optimism(hardfork) => hardfork,
            _ => Self::default(),
        }
    }
}

impl From<TempoHardfork> for FoundryHardfork {
    fn from(value: TempoHardfork) -> Self {
        Self::Tempo(value)
    }
}

impl From<FoundryHardfork> for TempoHardfork {
    fn from(fork: FoundryHardfork) -> Self {
        match fork {
            FoundryHardfork::Tempo(hardfork) => hardfork,
            _ => Self::default(),
        }
    }
}

impl From<FoundryHardfork> for SpecId {
    fn from(fork: FoundryHardfork) -> Self {
        match fork {
            FoundryHardfork::Ethereum(hardfork) => spec_id_from_ethereum_hardfork(hardfork),
            FoundryHardfork::Optimism(hardfork) => spec_id_from_optimism_hardfork(hardfork).into(),
            FoundryHardfork::Tempo(hardfork) => hardfork.into(),
        }
    }
}

/// Map an `EthereumHardfork` enum into its corresponding `SpecId`.
pub fn spec_id_from_ethereum_hardfork(hardfork: EthereumHardfork) -> SpecId {
    match hardfork {
        EthereumHardfork::Frontier => SpecId::FRONTIER,
        EthereumHardfork::Homestead => SpecId::HOMESTEAD,
        EthereumHardfork::Dao => SpecId::DAO_FORK,
        EthereumHardfork::Tangerine => SpecId::TANGERINE,
        EthereumHardfork::SpuriousDragon => SpecId::SPURIOUS_DRAGON,
        EthereumHardfork::Byzantium => SpecId::BYZANTIUM,
        EthereumHardfork::Constantinople => SpecId::CONSTANTINOPLE,
        EthereumHardfork::Petersburg => SpecId::PETERSBURG,
        EthereumHardfork::Istanbul => SpecId::ISTANBUL,
        EthereumHardfork::MuirGlacier => SpecId::MUIR_GLACIER,
        EthereumHardfork::Berlin => SpecId::BERLIN,
        EthereumHardfork::London => SpecId::LONDON,
        EthereumHardfork::ArrowGlacier => SpecId::ARROW_GLACIER,
        EthereumHardfork::GrayGlacier => SpecId::GRAY_GLACIER,
        EthereumHardfork::Paris => SpecId::MERGE,
        EthereumHardfork::Shanghai => SpecId::SHANGHAI,
        EthereumHardfork::Cancun => SpecId::CANCUN,
        EthereumHardfork::Prague => SpecId::PRAGUE,
        EthereumHardfork::Osaka => SpecId::OSAKA,
        EthereumHardfork::Bpo1 | EthereumHardfork::Bpo2 => SpecId::OSAKA,
        EthereumHardfork::Bpo3 | EthereumHardfork::Bpo4 | EthereumHardfork::Bpo5 => {
            unimplemented!()
        }
        f => unreachable!("unimplemented {}", f),
    }
}

/// Map an `OptimismHardfork` enum into its corresponding `OpSpecId`.
pub fn spec_id_from_optimism_hardfork(hardfork: OpHardfork) -> OpSpecId {
    match hardfork {
        OpHardfork::Bedrock => OpSpecId::BEDROCK,
        OpHardfork::Regolith => OpSpecId::REGOLITH,
        OpHardfork::Canyon => OpSpecId::CANYON,
        OpHardfork::Ecotone => OpSpecId::ECOTONE,
        OpHardfork::Fjord => OpSpecId::FJORD,
        OpHardfork::Granite => OpSpecId::GRANITE,
        OpHardfork::Holocene => OpSpecId::HOLOCENE,
        OpHardfork::Isthmus => OpSpecId::ISTHMUS,
        OpHardfork::Interop => OpSpecId::INTEROP,
        OpHardfork::Jovian => OpSpecId::JOVIAN,
        f => unreachable!("unimplemented {}", f),
    }
}

/// Convert a `BlockNumberOrTag` into an `EthereumHardfork`.
pub fn ethereum_hardfork_from_block_tag(block: impl Into<BlockNumberOrTag>) -> EthereumHardfork {
    let num = match block.into() {
        BlockNumberOrTag::Earliest => 0,
        BlockNumberOrTag::Number(num) => num,
        _ => u64::MAX,
    };

    EthereumHardfork::from_mainnet_block_number(num)
}

/// Tempo mainnet chain ID.
const TEMPO_MAINNET_CHAIN_ID: u64 = 4217;
/// Tempo moderato testnet chain ID.
const TEMPO_MODERATO_CHAIN_ID: u64 = 42431;

/// Resolves the active hardfork for a known chain at the given block.
///
/// Uses the same approach as reth: walk hardfork activation conditions (block number for Ethereum,
/// timestamp for Tempo) to find the latest active fork.
///
/// Returns `None` for unknown chains — the caller should fall back to heuristics.
pub fn hardfork_for_chain(
    chain_id: u64,
    block_number: u64,
    timestamp: u64,
) -> Option<FoundryHardfork> {
    match chain_id {
        1 => Some(FoundryHardfork::Ethereum(
            EthereumHardfork::from_mainnet_block_number(block_number),
        )),
        TEMPO_MAINNET_CHAIN_ID => Some(FoundryHardfork::Tempo(tempo_hardfork_at(
            TempoHardfork::mainnet_activation_timestamp,
            timestamp,
        ))),
        TEMPO_MODERATO_CHAIN_ID => Some(FoundryHardfork::Tempo(tempo_hardfork_at(
            TempoHardfork::moderato_activation_timestamp,
            timestamp,
        ))),
        _ => None,
    }
}

/// Walk `TempoHardfork::VARIANTS` in reverse to find the latest active fork at the given
/// timestamp, mirroring `TempoHardforks::tempo_hardfork_at` from reth but without requiring a
/// `ChainSpec` instance.
fn tempo_hardfork_at(
    activation_fn: fn(&TempoHardfork) -> Option<u64>,
    timestamp: u64,
) -> TempoHardfork {
    for &fork in TempoHardfork::VARIANTS.iter().rev() {
        if let Some(ts) = activation_fn(&fork)
            && timestamp >= ts
        {
            return fork;
        }
    }
    TempoHardfork::Genesis
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_hardforks::ethereum::mainnet::*;

    #[test]
    fn test_ethereum_spec_id_mapping() {
        assert_eq!(spec_id_from_ethereum_hardfork(EthereumHardfork::Frontier), SpecId::FRONTIER);
        assert_eq!(spec_id_from_ethereum_hardfork(EthereumHardfork::Homestead), SpecId::HOMESTEAD);

        // Test latest hardforks
        assert_eq!(spec_id_from_ethereum_hardfork(EthereumHardfork::Cancun), SpecId::CANCUN);
        assert_eq!(spec_id_from_ethereum_hardfork(EthereumHardfork::Prague), SpecId::PRAGUE);
        assert_eq!(spec_id_from_ethereum_hardfork(EthereumHardfork::Osaka), SpecId::OSAKA);
    }

    #[test]
    fn test_optimism_spec_id_mapping() {
        assert_eq!(spec_id_from_optimism_hardfork(OpHardfork::Bedrock), OpSpecId::BEDROCK);
        assert_eq!(spec_id_from_optimism_hardfork(OpHardfork::Regolith), OpSpecId::REGOLITH);

        // Test latest hardforks
        assert_eq!(spec_id_from_optimism_hardfork(OpHardfork::Holocene), OpSpecId::HOLOCENE);
        assert_eq!(spec_id_from_optimism_hardfork(OpHardfork::Interop), OpSpecId::INTEROP);
    }

    #[test]
    fn test_tempo_spec_id_mapping() {
        assert_eq!(SpecId::from(TempoHardfork::Genesis), SpecId::OSAKA);
    }

    #[test]
    fn test_hardfork_from_block_tag_numbers() {
        assert_eq!(
            ethereum_hardfork_from_block_tag(MAINNET_HOMESTEAD_BLOCK - 1),
            EthereumHardfork::Frontier
        );
        assert_eq!(
            ethereum_hardfork_from_block_tag(MAINNET_LONDON_BLOCK + 1),
            EthereumHardfork::London
        );
    }

    #[test]
    fn test_hardfork_for_chain_ethereum() {
        assert_eq!(
            hardfork_for_chain(1, MAINNET_LONDON_BLOCK + 1, 0),
            Some(FoundryHardfork::Ethereum(EthereumHardfork::London))
        );
    }

    #[test]
    fn test_hardfork_for_chain_tempo_mainnet() {
        // Pre-T1: should resolve to T0
        assert_eq!(
            hardfork_for_chain(TEMPO_MAINNET_CHAIN_ID, 0, 0),
            Some(FoundryHardfork::Tempo(TempoHardfork::T0))
        );
        // T1 activation timestamp
        assert_eq!(
            hardfork_for_chain(TEMPO_MAINNET_CHAIN_ID, 0, 1_770_908_400),
            Some(FoundryHardfork::Tempo(TempoHardfork::T1A))
        );
        // T1B activation timestamp
        assert_eq!(
            hardfork_for_chain(TEMPO_MAINNET_CHAIN_ID, 0, 1_771_858_800),
            Some(FoundryHardfork::Tempo(TempoHardfork::T1B))
        );
        // T2 activation timestamp
        assert_eq!(
            hardfork_for_chain(TEMPO_MAINNET_CHAIN_ID, 0, 1_774_965_600),
            Some(FoundryHardfork::Tempo(TempoHardfork::T2))
        );
    }

    #[test]
    fn test_hardfork_for_chain_unknown() {
        assert_eq!(hardfork_for_chain(999999, 0, 0), None);
    }
}
