use alloy_primitives::{Address, Bytes, U256};
use foundry_evm_core::{
    constants::{CALLER, TEST_CONTRACT_ADDRESS},
    tempo::FoundryStorageProvider,
};
use revm::state::{AccountInfo, Bytecode};
use tempo_precompiles::{
    NONCE_PRECOMPILE_ADDRESS, STABLECOIN_EXCHANGE_ADDRESS, TIP_ACCOUNT_REGISTRAR,
    TIP_FEE_MANAGER_ADDRESS, TIP20_FACTORY_ADDRESS, TIP20_REWARDS_REGISTRY_ADDRESS,
    TIP403_REGISTRY_ADDRESS, VALIDATOR_CONFIG_ADDRESS,
    error::TempoPrecompileError,
    tip20::{ISSUER_ROLE, ITIP20, TIP20Token, address_to_token_id_unchecked},
    tip20_factory::{ITIP20Factory, TIP20Factory},
    validator_config,
};

use crate::executors::Executor;

/// Initialize Tempo precompiles for the given executor.
/// This initialization should be kept aligned with Tempo's genesis file to ensure
/// executor environments accurately reflect production behavior.
pub fn initialize_tempo_precompiles(executor: &mut Executor) -> Result<(), TempoPrecompileError> {
    let sender = CALLER;
    let admin = TEST_CONTRACT_ADDRESS;

    // Set bytecode for all precompiles
    let bytecode = Bytecode::new_legacy(Bytes::from_static(&[0xef]));
    for precompile in [
        NONCE_PRECOMPILE_ADDRESS,
        STABLECOIN_EXCHANGE_ADDRESS,
        TIP20_FACTORY_ADDRESS,
        TIP20_REWARDS_REGISTRY_ADDRESS,
        TIP403_REGISTRY_ADDRESS,
        TIP_ACCOUNT_REGISTRAR,
        TIP_FEE_MANAGER_ADDRESS,
        VALIDATOR_CONFIG_ADDRESS,
    ] {
        executor.backend_mut().insert_account_info(
            precompile,
            AccountInfo {
                code_hash: bytecode.hash_slow(),
                code: Some(bytecode.clone()),
                ..Default::default()
            },
        );
    }

    executor
        .backend_mut()
        .insert_account_storage(
            VALIDATOR_CONFIG_ADDRESS,
            validator_config::slots::OWNER,
            admin.into_word().into(),
        )
        .expect("failed to initialize validator config state");

    let chain_id = executor.env().evm_env.cfg_env.chain_id;
    let timestamp = U256::from(executor.env().evm_env.block_env.timestamp);
    let mut storage_provider =
        FoundryStorageProvider::new(executor.backend_mut(), chain_id, timestamp);

    // Create PathUSD token
    let (path_usd_token_address, path_usd_token_id) = {
        let mut tip20_factory = TIP20Factory::new(&mut storage_provider);
        let token_address = tip20_factory
            .create_token(
                admin,
                ITIP20Factory::createTokenCall {
                    name: "PathUSD".to_string(),
                    symbol: "PathUSD".to_string(),
                    currency: "USD".to_string(),
                    quoteToken: Address::ZERO,
                    admin,
                },
            )
            .expect("Could not create token");

        (token_address, address_to_token_id_unchecked(token_address))
    };

    let mut path_usd = TIP20Token::new(path_usd_token_id, &mut storage_provider);
    path_usd
        .grant_role_internal(admin, *ISSUER_ROLE)
        .expect("failed to grant ISSUER_ROLE to admin");
    path_usd
        .mint(admin, ITIP20::mintCall { to: sender, amount: U256::from(u64::MAX) })
        .expect("failed to mint initial supply to sender");

    // Create AlphaUSD token
    let (_alpha_usd_token_address, alpha_usd_token_id) = {
        let mut tip20_factory = TIP20Factory::new(&mut storage_provider);
        let token_address = tip20_factory
            .create_token(
                admin,
                ITIP20Factory::createTokenCall {
                    name: "AlphaUSD".to_string(),
                    symbol: "AlphaUSD".to_string(),
                    currency: "USD".to_string(),
                    quoteToken: path_usd_token_address,
                    admin,
                },
            )
            .expect("Could not create token");

        (token_address, address_to_token_id_unchecked(token_address))
    };

    let mut alpha_usd = TIP20Token::new(alpha_usd_token_id, &mut storage_provider);
    alpha_usd
        .grant_role_internal(admin, *ISSUER_ROLE)
        .expect("failed to grant ISSUER_ROLE to admin");
    alpha_usd
        .mint(admin, ITIP20::mintCall { to: sender, amount: U256::from(u64::MAX) })
        .expect("failed to mint initial supply to sender");

    Ok(())
}
