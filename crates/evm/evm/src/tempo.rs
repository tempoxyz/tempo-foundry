use alloy_primitives::{Address, Bytes, U256};
use foundry_evm_core::{
    constants::{CALLER, TEST_CONTRACT_ADDRESS},
    tempo::FoundryStorageProvider,
};
use revm::state::{AccountInfo, Bytecode};
use tempo_contracts::{
    ARACHNID_CREATE2_FACTORY_ADDRESS, CREATEX_ADDRESS, CreateX, MULTICALL_ADDRESS, Multicall,
    PERMIT2_ADDRESS, Permit2, SAFE_DEPLOYER_ADDRESS, SafeDeployer,
    contracts::ARACHNID_CREATE2_FACTORY_BYTECODE,
};
use tempo_precompiles::{
    NONCE_PRECOMPILE_ADDRESS, STABLECOIN_EXCHANGE_ADDRESS, TIP_ACCOUNT_REGISTRAR,
    TIP_FEE_MANAGER_ADDRESS, TIP20_FACTORY_ADDRESS, TIP20_REWARDS_REGISTRY_ADDRESS,
    TIP403_REGISTRY_ADDRESS, VALIDATOR_CONFIG_ADDRESS,
    error::TempoPrecompileError,
    tip_fee_manager::TipFeeManager,
    tip20::{ISSUER_ROLE, ITIP20, TIP20Token, address_to_token_id_unchecked},
    tip20_factory::{ITIP20Factory, TIP20Factory},
    validator_config,
};

use crate::executors::Executor;

/// Initialize Tempo precompiles and contracts for the given executor.
/// This initialization should be kept aligned with Tempo's genesis file to ensure
/// executor environments accurately reflect production behavior.
///
/// Ref: <https://github.com/tempoxyz/tempo/blob/main/xtask/src/genesis_args.rs>
pub fn initialize_tempo_precompiles_and_contracts(
    executor: &mut Executor,
) -> Result<(), TempoPrecompileError> {
    let sender = CALLER;
    let admin = TEST_CONTRACT_ADDRESS;

    // Set bytecode for all precompiles
    let bytecode = Bytecode::new_legacy(Bytes::from_static(&[0xef]));

    // for precompile in [
    //     NONCE_PRECOMPILE_ADDRESS,
    //     STABLECOIN_EXCHANGE_ADDRESS,
    //     TIP20_FACTORY_ADDRESS,
    //     TIP20_REWARDS_REGISTRY_ADDRESS,
    //     TIP403_REGISTRY_ADDRESS,
    //     TIP_ACCOUNT_REGISTRAR,
    //     TIP_FEE_MANAGER_ADDRESS,
    //     VALIDATOR_CONFIG_ADDRESS,
    // ] {
    //     executor.backend_mut().insert_account_info(
    //         precompile,
    //         AccountInfo {
    //             code_hash: bytecode.hash_slow(),
    //             code: Some(bytecode.clone()),
    //             ..Default::default()
    //         },
    //     );
    // }

    // Initialize TIP403 Registry
    // https://github.com/tempoxyz/tempo/blob/main/crates/precompiles/src/tip403_registry/mod.rs
    executor.backend_mut().insert_account_info(
        TIP403_REGISTRY_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode.clone()),
            nonce: 1,
            ..Default::default()
        },
    );

    // Initialize TIP20 Factory
    // https://github.com/tempoxyz/tempo/blob/main/crates/precompiles/src/tip20_factory/mod.rs
    executor.backend_mut().insert_account_info(
        TIP20_FACTORY_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode.clone()),
            nonce: 1,
            ..Default::default()
        },
    );

    let chain_id = executor.env().evm_env.cfg_env.chain_id;
    let timestamp = U256::from(executor.env().evm_env.block_env.timestamp);
    let mut storage_provider =
        FoundryStorageProvider::new(executor.backend_mut(), chain_id, timestamp);

    // Create PathUSD token
    let path_usd_token_address = create_and_mint_token(
        &mut storage_provider,
        "PathUSD",
        "PathUSD",
        "USD",
        Address::ZERO,
        admin,
        sender,
        U256::from(u64::MAX),
    )?;

    // Create AlphaUSD token
    let _alpha_usd_token_address = create_and_mint_token(
        &mut storage_provider,
        "AlphaUSD",
        "AlphaUSD",
        "USD",
        path_usd_token_address,
        admin,
        sender,
        U256::from(u64::MAX),
    )?;

    // Create BetaUSD token
    let beta_usd_token_address = create_and_mint_token(
        &mut storage_provider,
        "BetaUSD",
        "BetaUSD",
        "USD",
        path_usd_token_address,
        admin,
        sender,
        U256::from(u64::MAX),
    )?;

    // Create ThetaUSD token
    let theta_usd_token_address = create_and_mint_token(
        &mut storage_provider,
        "ThetaUSD",
        "ThetaUSD",
        "USD",
        path_usd_token_address,
        admin,
        sender,
        U256::from(u64::MAX),
    )?;

    // Initialize TIP20RewardsRegistry
    // https://github.com/tempoxyz/tempo/blob/main/crates/precompiles/src/tip20_rewards_registry/mod.rs
    executor.backend_mut().insert_account_info(
        TIP20_REWARDS_REGISTRY_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode.clone()),
            nonce: 1,
            ..Default::default()
        },
    );

    // Initialize ValidatorConfig
    // https://github.com/tempoxyz/tempo/blob/main/crates/precompiles/src/validator_config/mod.rs#L48
    executor.backend_mut().insert_account_info(
        VALIDATOR_CONFIG_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode.clone()),
            nonce: 1,
            ..Default::default()
        },
    );
    executor
        .backend_mut()
        .insert_account_storage(
            VALIDATOR_CONFIG_ADDRESS,
            validator_config::slots::OWNER,
            admin.into_word().into(),
        )
        .expect("failed to initialize validator config state");

    // Initialize FeeManager
    executor.backend_mut().insert_account_info(
        TIP_FEE_MANAGER_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode.clone()),
            nonce: 1,
            ..Default::default()
        },
    );

    // Initialize stablecoin exchange
    executor.backend_mut().insert_account_info(
        STABLECOIN_EXCHANGE_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
            nonce: 1,
            ..Default::default()
        },
    );

    // Initialize nonce manager
    executor.backend_mut().insert_account_info(
        NONCE_PRECOMPILE_ADDRESS,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
            nonce: 1,
            ..Default::default()
        },
    );

    // Mint pairwise liquidity
    let mut fee_manager = TipFeeManager::new(storage_provider);

    // for b_token_address in b_tokens {
    //     fee_manager
    //         .mint(admin, a_token, b_token_address, amount, amount, admin)
    //         .expect("Could not mint A -> B Liquidity pool");
    // }

    let a_token = _alpha_usd_token_address;
    let b_tokens = vec![path_usd_token_address, beta_usd_token_address, theta_usd_token_address];

    // mint_pairwise_liquidity(
    //     alpha_token_address,
    //     vec![PATH_USD_ADDRESS, beta_token_address, theta_token_address],
    //     U256::from(10u64.pow(10)),
    //     admin,
    //     &mut evm,
    // );

    // Set bytecode for all contracts
    insert_contract(executor, MULTICALL_ADDRESS, Bytes::from_static(&Multicall::DEPLOYED_BYTECODE));
    insert_contract(executor, CREATEX_ADDRESS, Bytes::from_static(&CreateX::DEPLOYED_BYTECODE));
    insert_contract(
        executor,
        SAFE_DEPLOYER_ADDRESS,
        Bytes::from_static(&SafeDeployer::DEPLOYED_BYTECODE),
    );
    insert_contract(executor, PERMIT2_ADDRESS, Bytes::from_static(&Permit2::DEPLOYED_BYTECODE));
    insert_contract(executor, ARACHNID_CREATE2_FACTORY_ADDRESS, ARACHNID_CREATE2_FACTORY_BYTECODE);

    Ok(())
}

/// Helper function to insert a contract's bytecode into the executor's state.
fn insert_contract(executor: &mut Executor, addr: Address, bytes: Bytes) {
    let bytecode = Bytecode::new_legacy(bytes);
    executor.backend_mut().insert_account_info(
        addr,
        AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
            nonce: 1,
            ..Default::default()
        },
    );
}

/// Helper function to create and mint a TIP20 token.
#[expect(clippy::too_many_arguments)]
fn create_and_mint_token(
    storage_provider: &mut FoundryStorageProvider<'_>,
    symbol: &str,
    name: &str,
    currency: &str,
    quote_token: Address,
    admin: Address,
    recipient: Address,
    mint_amount: U256,
) -> Result<Address, TempoPrecompileError> {
    let mut tip20_factory = TIP20Factory::new(storage_provider);
    let token_address = tip20_factory.create_token(
        admin,
        ITIP20Factory::createTokenCall {
            name: name.to_string(),
            symbol: symbol.to_string(),
            currency: currency.to_string(),
            quoteToken: quote_token,
            admin,
        },
    )?;
    let token_id = address_to_token_id_unchecked(token_address);
    let mut token = TIP20Token::new(token_id, storage_provider);
    token.grant_role_internal(admin, *ISSUER_ROLE)?;
    token.mint(admin, ITIP20::mintCall { to: recipient, amount: mint_amount })?;

    Ok(token_address)
}
