use alloy_primitives::{Address, Bytes, U256};
use foundry_evm_core::{
    constants::{CALLER, TEST_CONTRACT_ADDRESS},
    tempo::FoundryStorageProvider,
};
use revm::state::Bytecode;
use tempo_chainspec::hardfork::TempoHardfork;
use tempo_contracts::{
    ARACHNID_CREATE2_FACTORY_ADDRESS, CREATEX_ADDRESS, CreateX, MULTICALL_ADDRESS, Multicall,
    PERMIT2_ADDRESS, Permit2, SAFE_DEPLOYER_ADDRESS, SafeDeployer,
    contracts::ARACHNID_CREATE2_FACTORY_BYTECODE,
};
use tempo_precompiles::{
    ACCOUNT_KEYCHAIN_ADDRESS, NONCE_PRECOMPILE_ADDRESS, STABLECOIN_EXCHANGE_ADDRESS,
    TIP_FEE_MANAGER_ADDRESS, TIP20_FACTORY_ADDRESS, TIP20_REWARDS_REGISTRY_ADDRESS,
    TIP403_REGISTRY_ADDRESS, VALIDATOR_CONFIG_ADDRESS,
    error::TempoPrecompileError,
    storage::StorageCtx,
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

    let chain_id = executor.env().evm_env.cfg_env.chain_id;
    let timestamp = U256::from(executor.env().evm_env.block_env.timestamp);
    let mut storage = FoundryStorageProvider::new(
        executor.backend_mut(),
        chain_id,
        timestamp,
        TempoHardfork::default(),
    );

    StorageCtx::enter(&mut storage, || -> Result<(), TempoPrecompileError> {
        let mut ctx = StorageCtx;

        let sentinel = Bytecode::new_legacy(Bytes::from_static(&[0xef]));
        for precompile in [
            NONCE_PRECOMPILE_ADDRESS,
            STABLECOIN_EXCHANGE_ADDRESS,
            TIP20_FACTORY_ADDRESS,
            TIP20_REWARDS_REGISTRY_ADDRESS,
            TIP403_REGISTRY_ADDRESS,
            TIP_FEE_MANAGER_ADDRESS,
            VALIDATOR_CONFIG_ADDRESS,
            ACCOUNT_KEYCHAIN_ADDRESS,
        ] {
            ctx.set_code(precompile, sentinel.clone())?;
        }

        // Create PathUSD token: 0x20C0000000000000000000000000000000000000
        let path_usd_token_address = create_and_mint_token(
            "PathUSD",
            "PathUSD",
            "USD",
            Address::ZERO,
            admin,
            sender,
            U256::from(u64::MAX),
        )?;

        // Create AlphaUSD token: 0x20C0000000000000000000000000000000000001
        let _alpha_usd_token_address = create_and_mint_token(
            "AlphaUSD",
            "AlphaUSD",
            "USD",
            path_usd_token_address,
            admin,
            sender,
            U256::from(u64::MAX),
        )?;

        // Create BetaUSD token: 0x20C0000000000000000000000000000000000002
        let _beta_usd_token_address = create_and_mint_token(
            "BetaUSD",
            "BetaUSD",
            "USD",
            path_usd_token_address,
            admin,
            sender,
            U256::from(u64::MAX),
        )?;

        // Create ThetaUSD token: 0x20C0000000000000000000000000000000000003
        let _theta_usd_token_address = create_and_mint_token(
            "ThetaUSD",
            "ThetaUSD",
            "USD",
            path_usd_token_address,
            admin,
            sender,
            U256::from(u64::MAX),
        )?;

        // Initialize ValidatorConfig with admin as owner
        ctx.sstore(
            VALIDATOR_CONFIG_ADDRESS,
            validator_config::slots::OWNER,
            admin.into_word().into(),
        )?;

        // Set bytecode for all contracts
        ctx.set_code(
            MULTICALL_ADDRESS,
            Bytecode::new_legacy(Bytes::from_static(&Multicall::DEPLOYED_BYTECODE)),
        )?;
        ctx.set_code(
            CREATEX_ADDRESS,
            Bytecode::new_legacy(Bytes::from_static(&CreateX::DEPLOYED_BYTECODE)),
        )?;
        ctx.set_code(
            SAFE_DEPLOYER_ADDRESS,
            Bytecode::new_legacy(Bytes::from_static(&SafeDeployer::DEPLOYED_BYTECODE)),
        )?;
        ctx.set_code(
            PERMIT2_ADDRESS,
            Bytecode::new_legacy(Bytes::from_static(&Permit2::DEPLOYED_BYTECODE)),
        )?;
        ctx.set_code(
            ARACHNID_CREATE2_FACTORY_ADDRESS,
            Bytecode::new_legacy(ARACHNID_CREATE2_FACTORY_BYTECODE),
        )?;

        Ok(())
    })?;

    Ok(())
}

/// Helper function to create and mint a TIP20 token.
fn create_and_mint_token(
    symbol: &str,
    name: &str,
    currency: &str,
    quote_token: Address,
    admin: Address,
    recipient: Address,
    mint_amount: U256,
) -> Result<Address, TempoPrecompileError> {
    let mut tip20_factory = TIP20Factory::new();
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
    let mut token = TIP20Token::new(token_id);
    token.grant_role_internal(admin, *ISSUER_ROLE)?;
    token.mint(admin, ITIP20::mintCall { to: recipient, amount: mint_amount })?;

    Ok(token_address)
}
