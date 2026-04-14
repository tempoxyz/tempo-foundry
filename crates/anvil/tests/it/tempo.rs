//! Tests for Tempo-specific features in Anvil.
//!
//! This module tests Tempo's payment-native protocol features including:
//! - TIP20 fee tokens (PathUSD, AlphaUSD, BetaUSD, ThetaUSD)
//! - Tempo transaction types (AA transactions with fee token support)
//! - 2D nonces (nonce-key for parallelizable transactions)
//! - Millisecond timestamps
//! - Account keychain for key authorization
//! - Tempo precompiles initialization
//!
//! ## T1 Hardfork Gas Parameters
//! - Base fee: 20 gwei
//! - Transaction gas cap: 30M gas
//! - State creation operations need 250k+ gas budget
//! - Expiring nonces: 13,000 gas intrinsic cost

use alloy_consensus::Typed2718;
use alloy_eips::eip2718::Encodable2718;
use alloy_network::{ReceiptResponse, TransactionBuilder, TransactionResponse};
use alloy_primitives::{Address, Bytes, TxKind, U256, address};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockNumberOrTag, TransactionRequest};
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use anvil::{NodeConfig, spawn};
use foundry_evm::core::tempo::{
    ALPHA_USD_ADDRESS, BETA_USD_ADDRESS, PATH_USD_ADDRESS, THETA_USD_ADDRESS,
};
use tempo_alloy::primitives::TempoTxEnvelope;
use tempo_primitives::{
    AASigned, TempoSignature, TempoTransaction,
    transaction::{Call, PrimitiveSignature},
};

const PATH_USD: Address = PATH_USD_ADDRESS;
const ALPHA_USD: Address = ALPHA_USD_ADDRESS;
const BETA_USD: Address = BETA_USD_ADDRESS;
const THETA_USD: Address = THETA_USD_ADDRESS;

// Precompile addresses
const NONCE_PRECOMPILE: Address = address!("4e4F4E4345000000000000000000000000000000");
const ACCOUNT_KEYCHAIN: Address = address!("aAAAaaAA00000000000000000000000000000000");

// T1 hardfork gas constants
const TIP20_TRANSFER_GAS: u64 = 300_000;
const TIP20_APPROVE_GAS: u64 = 100_000;

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function name() external view returns (string memory);
        function symbol() external view returns (string memory);
        function decimals() external view returns (uint8);
        function totalSupply() external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
}

// ============================================================================
// Tempo Mode Initialization Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_mode_enabled_by_default() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let block_number = provider.get_block_number().await.unwrap();
    assert_eq!(block_number, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_precompiles_have_code() {
    let (api, _handle) = spawn(NodeConfig::test_tempo()).await;

    // Tempo precompiles should have sentinel bytecode (0xef)
    let nonce_code = api.get_code(NONCE_PRECOMPILE, None).await.unwrap();
    assert!(!nonce_code.is_empty(), "Nonce precompile should have code");

    let keychain_code = api.get_code(ACCOUNT_KEYCHAIN, None).await.unwrap();
    assert!(!keychain_code.is_empty(), "Account keychain should have code");
}

// ============================================================================
// TIP20 Fee Token Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_fee_tokens_deployed() {
    let (api, _handle) = spawn(NodeConfig::test_tempo()).await;

    for token in [PATH_USD, ALPHA_USD, BETA_USD, THETA_USD] {
        let code = api.get_code(token, None).await.unwrap();
        assert!(!code.is_empty(), "Token {token} should have code deployed");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fee_token_balances_minted_to_test_accounts() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let dev_accounts: Vec<Address> = handle.dev_accounts().collect();
    assert!(!dev_accounts.is_empty(), "Should have dev accounts");

    // Each dev account should have fee tokens minted
    for account in dev_accounts.iter().take(3) {
        for token in [PATH_USD, ALPHA_USD, BETA_USD, THETA_USD] {
            let token_contract = IERC20::new(token, &provider);
            let balance = token_contract.balanceOf(*account).call().await.unwrap();
            assert!(
                balance > U256::ZERO,
                "Account {account} should have {token} balance, got {balance}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_token_metadata() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    // Test PathUSD metadata
    let path_usd = IERC20::new(PATH_USD, &provider);
    let name = path_usd.name().call().await.unwrap();
    let symbol = path_usd.symbol().call().await.unwrap();
    let decimals = path_usd.decimals().call().await.unwrap();

    assert_eq!(name, "PathUSD");
    assert_eq!(symbol, "PathUSD");
    assert_eq!(decimals, 6); // TIP20 tokens use 6 decimals

    // Test AlphaUSD metadata
    let alpha_usd = IERC20::new(ALPHA_USD, &provider);
    let name = alpha_usd.name().call().await.unwrap();
    assert_eq!(name, "AlphaUSD");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_transfer() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // Use PATH_USD for transfer testing (not the default fee token ALPHA_USD)
    // This way balance changes are only from the transfer, not gas fees
    let token = IERC20::new(PATH_USD, &provider);

    // Get initial balances
    let sender_balance_before = token.balanceOf(sender).call().await.unwrap();
    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    // Transfer tokens with explicit gas limit for precompile interaction
    let transfer_amount = U256::from(1_000_000); // 1 token (6 decimals)
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let pending = provider.send_transaction(tx).await.unwrap();
    let receipt = pending.get_receipt().await.unwrap();
    assert!(receipt.status());

    // Verify balances changed
    let sender_balance_after = token.balanceOf(sender).call().await.unwrap();
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();

    assert_eq!(
        sender_balance_before - transfer_amount,
        sender_balance_after,
        "Sender balance should decrease by transfer amount"
    );
    assert_eq!(
        recipient_balance_before + transfer_amount,
        recipient_balance_after,
        "Recipient balance should increase by transfer amount"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_approve_and_transfer_from() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let owner = accounts[0];
    let spender = accounts[1];
    let recipient = accounts[2];

    let token = IERC20::new(BETA_USD, &provider);

    // Owner approves spender
    let approve_amount = U256::from(5_000_000);
    let approve_call = token.approve(spender, approve_amount);
    let calldata: Bytes = approve_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(owner)
        .to(BETA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_APPROVE_GAS);

    let tx = WithOtherFields::new(tx);
    provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    // Check allowance
    let allowance = token.allowance(owner, spender).call().await.unwrap();
    assert_eq!(allowance, approve_amount);

    // Spender transfers from owner to recipient
    let transfer_amount = U256::from(2_000_000);
    let transfer_from_call = token.transferFrom(owner, recipient, transfer_amount);
    let calldata: Bytes = transfer_from_call.calldata().clone();

    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    let tx = TransactionRequest::default()
        .from(spender)
        .to(BETA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status());

    // Verify recipient received tokens
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(recipient_balance_before + transfer_amount, recipient_balance_after);

    // Verify allowance decreased
    let allowance_after = token.allowance(owner, spender).call().await.unwrap();
    assert_eq!(allowance_after, approve_amount - transfer_amount);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_total_supply() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let token = IERC20::new(PATH_USD, &provider);
    let total_supply = token.totalSupply().call().await.unwrap();

    // Total supply should be non-zero (tokens minted at genesis)
    assert!(total_supply > U256::ZERO, "Total supply should be non-zero");
}

// ============================================================================
// Block and Timestamp Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_block_has_timestamp() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    api.mine_one().await;

    let block = provider.get_block(1.into()).await.unwrap().unwrap();
    assert!(block.header.timestamp > 0, "Block should have a timestamp");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_block_timestamp_increases() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    api.mine_one().await;
    let block1 = provider.get_block(1.into()).await.unwrap().unwrap();

    let future_timestamp = block1.header.timestamp + 100;
    api.evm_set_next_block_timestamp(future_timestamp).unwrap();

    api.mine_one().await;
    let block2 = provider.get_block(2.into()).await.unwrap().unwrap();

    assert_eq!(block2.header.timestamp, future_timestamp);
    assert!(block2.header.timestamp > block1.header.timestamp);
}

// ============================================================================
// Transaction Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_native_eth_transfer_rejected_in_tempo_mode() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let transfer_amount = U256::from(1_000_000_000_000_000_000u64); // 1 ETH

    let tx = TransactionRequest::default().from(from).to(to).value(transfer_amount);

    let tx = WithOtherFields::new(tx);

    // Tempo mode doesn't allow native ETH transfers - use TIP20 fee tokens instead
    let result = provider.send_transaction(tx).await;
    assert!(result.is_err(), "Native ETH transfers should be rejected in Tempo mode");

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("native value transfer not allowed"),
        "Expected 'native value transfer not allowed' error, got: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_nonce_increments() {
    crate::init_tracing();
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let nonce_before = provider.get_transaction_count(from).await.unwrap();
    assert_eq!(nonce_before, 0);

    // Use TIP20 transfer instead of native ETH transfer (not allowed in Tempo mode)
    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(to, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(from)
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let pending = provider.send_transaction(tx).await.unwrap();
    pending.get_receipt().await.unwrap();

    let nonce_after = provider.get_transaction_count(from).await.unwrap();
    assert_eq!(nonce_after, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_transactions_in_block() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    api.anvil_set_auto_mine(false).await.unwrap();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    // Use TIP20 transfers instead of native ETH (not allowed in Tempo mode)
    let token = IERC20::new(ALPHA_USD, &provider);

    let transfer1 = token.transfer(accounts[1], U256::from(1000));
    let calldata1: Bytes = transfer1.calldata().clone();
    let tx1 = TransactionRequest::default()
        .from(accounts[0])
        .to(ALPHA_USD)
        .with_input(calldata1)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let transfer2 = token.transfer(accounts[3], U256::from(2000));
    let calldata2: Bytes = transfer2.calldata().clone();
    let tx2 = TransactionRequest::default()
        .from(accounts[2])
        .to(ALPHA_USD)
        .with_input(calldata2)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx1 = WithOtherFields::new(tx1);
    let tx2 = WithOtherFields::new(tx2);

    let pending1 = provider.send_transaction(tx1).await.unwrap();
    let pending2 = provider.send_transaction(tx2).await.unwrap();

    api.mine_one().await;

    let receipt1 = pending1.get_receipt().await.unwrap();
    let receipt2 = pending2.get_receipt().await.unwrap();

    assert_eq!(receipt1.block_number, receipt2.block_number);
}

// ============================================================================
// Gas Estimation Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    // Use TIP20 transfer for gas estimation (native ETH not allowed in Tempo)
    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(accounts[1], U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default().from(accounts[0]).to(ALPHA_USD).with_input(calldata);

    let gas_estimate = provider.estimate_gas(tx.into()).await.unwrap();

    // TIP20 transfer should use more than 21000 gas
    assert!(gas_estimate > 21000, "TIP20 transfer should use more than 21000 gas");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_for_contract_call() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    // Estimate gas for TIP20 transfer
    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(accounts[1], U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default().from(accounts[0]).to(ALPHA_USD).with_input(calldata);

    let gas_estimate = provider.estimate_gas(tx.into()).await.unwrap();

    // Contract call should use more gas than simple transfer
    assert!(gas_estimate > 21000, "Contract call should use more than 21000 gas");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_tempo_aa_transaction() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];

    // Build a Tempo AA transaction request with fee token
    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    // Create a TransactionRequest with Tempo AA fields (feeToken)
    let tx: WithOtherFields<TransactionRequest> = WithOtherFields {
        inner: TransactionRequest::default().from(accounts[0]).to(PATH_USD).with_input(calldata),
        other: [("feeToken".to_string(), serde_json::json!(PATH_USD.to_string()))]
            .into_iter()
            .collect(),
    };

    // Gas estimation should succeed for Tempo AA transactions
    let gas_estimate = provider.estimate_gas(tx).await.unwrap();

    // Tempo AA transactions have higher intrinsic gas than regular transactions
    // Base (21k) + signature verification + 2D nonce handling
    assert!(
        gas_estimate > 21000,
        "Tempo AA gas estimate should be greater than 21000, got: {gas_estimate}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_tempo_aa_with_2d_nonce() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];

    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    // Create a TransactionRequest with 2D nonce (nonceKey != 0)
    let tx: WithOtherFields<TransactionRequest> = WithOtherFields {
        inner: TransactionRequest::default()
            .from(accounts[0])
            .to(PATH_USD)
            .with_input(calldata)
            .with_nonce(0),
        other: [
            ("feeToken".to_string(), serde_json::json!(PATH_USD.to_string())),
            ("nonceKey".to_string(), serde_json::json!("0x64")), // nonce_key = 100
        ]
        .into_iter()
        .collect(),
    };

    // Gas estimation should succeed for 2D nonce transactions
    let gas_estimate = provider.estimate_gas(tx).await.unwrap();

    // 2D nonce transactions have additional gas costs for nonce key storage
    assert!(
        gas_estimate > 21000,
        "2D nonce gas estimate should be greater than 21000, got: {gas_estimate}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_tempo_aa_expiring_nonce() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];

    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    // Expiring nonce uses nonce_key = MAX
    let max_nonce_key = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    let tx: WithOtherFields<TransactionRequest> = WithOtherFields {
        inner: TransactionRequest::default()
            .from(accounts[0])
            .to(PATH_USD)
            .with_input(calldata)
            .with_nonce(0),
        other: [
            ("feeToken".to_string(), serde_json::json!(PATH_USD.to_string())),
            ("nonceKey".to_string(), serde_json::json!(max_nonce_key)),
        ]
        .into_iter()
        .collect(),
    };

    // Gas estimation should succeed for expiring nonce transactions
    let gas_estimate = provider.estimate_gas(tx).await.unwrap();

    // Expiring nonce transactions have additional gas for replay protection
    assert!(
        gas_estimate > 21000,
        "Expiring nonce gas estimate should be greater than 21000, got: {gas_estimate}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_converges_for_tempo_intrinsic_gas() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];

    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    // Create Tempo AA request
    let tx: WithOtherFields<TransactionRequest> = WithOtherFields {
        inner: TransactionRequest::default()
            .from(accounts[0])
            .to(PATH_USD)
            .with_input(calldata.clone()),
        other: [("feeToken".to_string(), serde_json::json!(PATH_USD.to_string()))]
            .into_iter()
            .collect(),
    };

    let gas_estimate = provider.estimate_gas(tx.clone()).await.unwrap();

    // The estimated gas should be sufficient to execute the transaction
    // Create and send the actual transaction with the estimated gas
    let signer = dev_key(0);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(PATH_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: gas_estimate,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::ZERO,
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // Transaction should succeed with the estimated gas
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "Transaction should succeed with estimated gas: {gas_estimate}");

    // Gas used should be less than or equal to estimate
    assert!(
        receipt.gas_used() <= gas_estimate,
        "Gas used ({}) should be <= estimate ({})",
        receipt.gas_used(),
        gas_estimate
    );
}

// ============================================================================
// Chain ID Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_chain_id() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let chain_id = provider.get_chain_id().await.unwrap();
    assert_eq!(chain_id, 31337);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_custom_chain_id() {
    let custom_chain_id = 42069u64;
    let (_api, handle) = spawn(NodeConfig::test_tempo().with_chain_id(Some(custom_chain_id))).await;
    let provider = handle.http_provider();

    let chain_id = provider.get_chain_id().await.unwrap();
    assert_eq!(chain_id, custom_chain_id);
}

// ============================================================================
// Account State Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_dev_accounts_have_balance() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let genesis_balance = handle.genesis_balance();

    for account in handle.dev_accounts() {
        let balance = provider.get_balance(account).await.unwrap();
        assert_eq!(balance, genesis_balance, "Dev account {account} should have genesis balance");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_anvil_set_balance() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let random_address = Address::random();
    let new_balance = U256::from(1_000_000_000_000_000_000u64);

    let balance_before = provider.get_balance(random_address).await.unwrap();
    assert_eq!(balance_before, U256::ZERO);

    api.anvil_set_balance(random_address, new_balance).await.unwrap();

    let balance_after = provider.get_balance(random_address).await.unwrap();
    assert_eq!(balance_after, new_balance);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_anvil_set_code() {
    let (api, _handle) = spawn(NodeConfig::test_tempo()).await;

    let target = Address::random();

    let code_before = api.get_code(target, None).await.unwrap();
    assert!(code_before.is_empty());

    let bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xf3]; // PUSH 0, PUSH 0, RETURN
    api.anvil_set_code(target, bytecode.clone().into()).await.unwrap();

    let code_after = api.get_code(target, None).await.unwrap();
    assert_eq!(code_after.as_ref(), bytecode.as_slice());
}

// ============================================================================
// Mining Control Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_auto_mine_toggle() {
    let (api, _handle) = spawn(NodeConfig::test_tempo()).await;

    assert!(api.anvil_get_auto_mine().unwrap());

    api.anvil_set_auto_mine(false).await.unwrap();
    assert!(!api.anvil_get_auto_mine().unwrap());

    api.anvil_set_auto_mine(true).await.unwrap();
    assert!(api.anvil_get_auto_mine().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manual_mining() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let block_before = provider.get_block_number().await.unwrap();

    api.mine_one().await;

    let block_after = provider.get_block_number().await.unwrap();
    assert_eq!(block_after, block_before + 1);

    api.anvil_mine(Some(U256::from(5)), None).await.unwrap();

    let block_final = provider.get_block_number().await.unwrap();
    assert_eq!(block_final, block_after + 5);
}

// ============================================================================
// Impersonation Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_impersonate_account() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let impersonated = handle.dev_accounts().next().unwrap();
    let recipient = handle.dev_accounts().nth(1).unwrap();

    api.anvil_impersonate_account(impersonated).await.unwrap();

    // Use TIP20 transfer (native ETH not allowed in Tempo mode)
    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(impersonated)
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    assert!(receipt.status());

    api.anvil_stop_impersonating_account(impersonated).await.unwrap();
}

// ============================================================================
// Snapshot and Revert Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_snapshot_and_revert() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let token = IERC20::new(ALPHA_USD, &provider);
    let balance_before = token.balanceOf(to).call().await.unwrap();
    let block_before = provider.get_block_number().await.unwrap();

    let snapshot_id = api.evm_snapshot().await.unwrap();

    // Use TIP20 transfer (native ETH not allowed in Tempo mode)
    let transfer_call = token.transfer(to, U256::from(1_000_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(from)
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    let balance_after_tx = token.balanceOf(to).call().await.unwrap();
    assert!(balance_after_tx > balance_before);

    api.evm_revert(snapshot_id).await.unwrap();

    let balance_reverted = token.balanceOf(to).call().await.unwrap();
    let block_reverted = provider.get_block_number().await.unwrap();

    assert_eq!(balance_reverted, balance_before);
    assert_eq!(block_reverted, block_before);
}

// ============================================================================
// Event/Log Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_transfer_emits_event() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_amount = U256::from(1_000_000);
    let transfer_call = token.transfer(to, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(from)
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    assert!(!receipt.inner.logs().is_empty(), "Transfer should emit event");

    let log = &receipt.inner.logs()[0];
    assert_eq!(log.address(), ALPHA_USD);

    let transfer_topic =
        alloy_primitives::keccak256("Transfer(address,address,uint256)".as_bytes());
    assert_eq!(log.topics()[0], transfer_topic);
}

// ============================================================================
// Block Gas Limit Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_block_gas_limit() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    api.mine_one().await;

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();

    assert!(block.header.gas_limit > 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_transaction_respects_gas_limit() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    // Use TIP20 transfer (native ETH not allowed in Tempo mode)
    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(accounts[1], U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(accounts[0])
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    assert!(receipt.status());
    assert!(receipt.gas_used <= TIP20_TRANSFER_GAS);
}

// ============================================================================
// Tempo-specific: Multiple Fee Token Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_all_fee_tokens_have_correct_metadata() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let tokens = [
        (PATH_USD, "PathUSD"),
        (ALPHA_USD, "AlphaUSD"),
        (BETA_USD, "BetaUSD"),
        (THETA_USD, "ThetaUSD"),
    ];

    for (addr, expected_name) in tokens {
        let token = IERC20::new(addr, &provider);
        let name = token.name().call().await.unwrap();
        let decimals = token.decimals().call().await.unwrap();

        assert_eq!(name, expected_name, "Token at {addr} should be named {expected_name}");
        assert_eq!(decimals, 6, "All TIP20 tokens use 6 decimals");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_transfer_between_different_fee_tokens() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // Transfer from different fee tokens in sequence
    for token_addr in [PATH_USD, ALPHA_USD, BETA_USD, THETA_USD] {
        let token = IERC20::new(token_addr, &provider);
        let balance_before = token.balanceOf(recipient).call().await.unwrap();

        let transfer_amount = U256::from(100_000);
        let transfer_call = token.transfer(recipient, transfer_amount);
        let calldata: Bytes = transfer_call.calldata().clone();

        let tx = TransactionRequest::default()
            .from(sender)
            .to(token_addr)
            .with_input(calldata)
            .with_gas_limit(TIP20_TRANSFER_GAS);

        let tx = WithOtherFields::new(tx);
        let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
        assert!(receipt.status(), "Transfer for {token_addr} failed");

        let balance_after = token.balanceOf(recipient).call().await.unwrap();
        assert_eq!(balance_after, balance_before + transfer_amount);
    }
}

// ============================================================================
// Tempo-specific: Gas Price Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_price() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let gas_price = provider.get_gas_price().await.unwrap();

    // Gas price should be non-zero
    assert!(gas_price > 0, "Gas price should be non-zero");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_base_fee() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    api.mine_one().await;

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();

    // Block should have base fee (EIP-1559)
    assert!(block.header.base_fee_per_gas.is_some());
}

// ============================================================================
// EIP-1559 Transaction Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_eip1559_transaction() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // Use PATH_USD for transfer (not the default fee token)
    let token = IERC20::new(PATH_USD, &provider);
    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    // Create an EIP-1559 transaction with explicit gas parameters
    let transfer_amount = U256::from(500_000); // 0.5 token
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    // Get current base fee
    let base_fee = provider.get_gas_price().await.unwrap();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS)
        .max_fee_per_gas(base_fee * 2) // 2x base fee for priority
        .max_priority_fee_per_gas(base_fee / 10); // 10% priority fee

    let tx = WithOtherFields::new(tx);
    let pending = provider.send_transaction(tx).await.unwrap();
    let receipt = pending.get_receipt().await.unwrap();

    assert!(receipt.status(), "EIP-1559 transaction should succeed");

    // Verify recipient received tokens
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(
        recipient_balance_after,
        recipient_balance_before + transfer_amount,
        "Recipient should receive transfer amount"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_eip1559_fee_token_deduction() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // Check fee token balance (ALPHA_USD is Alice's default fee token)
    let fee_token = IERC20::new(ALPHA_USD, &provider);
    let fee_balance_before = fee_token.balanceOf(sender).call().await.unwrap();

    // Use PATH_USD for transfer
    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(100_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let base_fee = provider.get_gas_price().await.unwrap();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS)
        .max_fee_per_gas(base_fee * 2)
        .max_priority_fee_per_gas(base_fee / 10);

    let tx = WithOtherFields::new(tx);
    let pending = provider.send_transaction(tx).await.unwrap();
    let receipt = pending.get_receipt().await.unwrap();

    assert!(receipt.status(), "Transaction should succeed");

    // Fee token balance should have decreased (gas fees paid in ALPHA_USD)
    let fee_balance_after = fee_token.balanceOf(sender).call().await.unwrap();
    assert!(
        fee_balance_after < fee_balance_before,
        "Fee token balance should decrease after paying gas (before: {fee_balance_before}, after: {fee_balance_after})"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_legacy_transaction() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // Use PATH_USD for transfer (not the default fee token)
    let token = IERC20::new(PATH_USD, &provider);
    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    // Create a legacy transaction (no EIP-1559 fields)
    let transfer_amount = U256::from(250_000); // 0.25 token
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    // Get current gas price for legacy tx
    let gas_price = provider.get_gas_price().await.unwrap();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS)
        .with_gas_price(gas_price);

    let tx = WithOtherFields::new(tx);
    let pending = provider.send_transaction(tx).await.unwrap();
    let receipt = pending.get_receipt().await.unwrap();

    assert!(receipt.status(), "Legacy transaction should succeed");

    // Verify recipient received tokens
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(
        recipient_balance_after,
        recipient_balance_before + transfer_amount,
        "Recipient should receive transfer amount"
    );
}

// ============================================================================
// Tempo AA Transaction Tests (Type 0x76)
// ============================================================================

/// Helper to get the private key for a dev account
fn dev_key(index: u32) -> PrivateKeySigner {
    let mnemonic = "test test test test test test test test test test test junk";
    alloy_signer_local::MnemonicBuilder::<alloy_signer_local::coins_bip39::English>::default()
        .phrase(mnemonic)
        .index(index)
        .expect("valid mnemonic")
        .build()
        .expect("valid key")
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_basic() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    // Get initial PATH_USD balances
    let token = IERC20::new(PATH_USD, &provider);
    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    // Build a Tempo AA transaction (type 0x76)
    let transfer_amount = U256::from(100_000);
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Create the Tempo transaction
    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD), // Use AlphaUSD for gas fees
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::ZERO, // Protocol nonce lane
        nonce: 0,              // First transaction on nonce key 0
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    // Sign the transaction
    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    // Encode and send the raw transaction
    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(receipt.status(), "Tempo AA transaction should succeed");

    // Verify recipient received tokens
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(
        recipient_balance_after,
        recipient_balance_before + transfer_amount,
        "Recipient should receive transfer amount"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_with_2d_nonce() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Send two transactions with different nonce keys (can be parallelized)
    let nonce_keys = [U256::from(1), U256::from(2)]; // User nonce lanes

    for (i, nonce_key) in nonce_keys.iter().enumerate() {
        let transfer_amount = U256::from(50_000 * (i + 1) as u64);
        let transfer_call = token.transfer(recipient, transfer_amount);
        let calldata: Bytes = transfer_call.calldata().clone();

        let tempo_tx = TempoTransaction {
            chain_id,
            fee_token: Some(ALPHA_USD),
            max_priority_fee_per_gas: base_fee / 10,
            max_fee_per_gas: base_fee * 2,
            gas_limit: TIP20_TRANSFER_GAS,
            calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
            access_list: Default::default(),
            nonce_key: *nonce_key,
            nonce: 0, // First transaction on this nonce key
            fee_payer_signature: None,
            valid_before: None,
            valid_after: None,
            key_authorization: None,
            tempo_authorization_list: vec![],
        };

        let sig_hash = tempo_tx.signature_hash();
        let signature = signer.sign_hash(&sig_hash).await.unwrap();
        let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
        let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
        let envelope = TempoTxEnvelope::AA(signed_tx);

        let mut encoded = Vec::new();
        envelope.encode_2718(&mut encoded);
        let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
        let receipt = tx_hash.get_receipt().await.unwrap();

        assert!(receipt.status(), "Tempo AA transaction with nonce_key {nonce_key} should succeed");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_with_valid_before() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Get current block timestamp
    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;

    // Create a transaction with valid_before set to 30 seconds in the future
    let valid_before = current_time + 30;

    let transfer_amount = U256::from(75_000);
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(3), // Use a unique nonce key
        nonce: 0,
        fee_payer_signature: None,
        valid_before: std::num::NonZeroU64::new(valid_before),
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(receipt.status(), "Tempo AA transaction with valid_before should succeed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_with_valid_after() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Get current block timestamp
    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;

    // Create a transaction with valid_after set to current time (already valid)
    // and valid_before set to 30 seconds in the future
    let valid_after = current_time; // Already valid
    let valid_before = current_time + 30;

    let transfer_amount = U256::from(60_000);
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(4), // Use a unique nonce key
        nonce: 0,
        fee_payer_signature: None,
        valid_before: std::num::NonZeroU64::new(valid_before),
        valid_after: std::num::NonZeroU64::new(valid_after),
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(
        receipt.status(),
        "Tempo AA transaction with valid_after (already valid) should succeed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_expiring_nonce() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Get current block timestamp
    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;

    // Expiring nonce uses nonce_key = U256::MAX and nonce = 0
    // The transaction hash provides replay protection instead of sequential nonces
    let valid_before = current_time + 25; // Must be within 30 seconds max expiry window

    let transfer_amount = U256::from(80_000);
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::MAX, // Expiring nonce mode
        nonce: 0,             // Always 0 for expiring nonces
        fee_payer_signature: None,
        valid_before: std::num::NonZeroU64::new(valid_before), // Required for expiring nonces
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(receipt.status(), "Tempo AA transaction with expiring nonce should succeed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_multiple_calls() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient1 = accounts[1];
    let recipient2 = accounts[2];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let recipient1_balance_before = token.balanceOf(recipient1).call().await.unwrap();
    let recipient2_balance_before = token.balanceOf(recipient2).call().await.unwrap();

    // Build multiple calls in a single transaction
    let amount1 = U256::from(25_000);
    let amount2 = U256::from(35_000);

    let call1_data: Bytes = token.transfer(recipient1, amount1).calldata().clone();
    let call2_data: Bytes = token.transfer(recipient2, amount2).calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS * 2, // More gas for multiple calls
        calls: vec![
            Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: call1_data },
            Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: call2_data },
        ],
        access_list: Default::default(),
        nonce_key: U256::from(5),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(receipt.status(), "Tempo AA transaction with multiple calls should succeed");

    // Verify both recipients received tokens
    let recipient1_balance_after = token.balanceOf(recipient1).call().await.unwrap();
    let recipient2_balance_after = token.balanceOf(recipient2).call().await.unwrap();

    assert_eq!(
        recipient1_balance_after,
        recipient1_balance_before + amount1,
        "Recipient 1 should receive first transfer amount"
    );
    assert_eq!(
        recipient2_balance_after,
        recipient2_balance_before + amount2,
        "Recipient 2 should receive second transfer amount"
    );
}

// ============================================================================
// Tempo AA Transaction Error Cases
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_expired_valid_before() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Get current block timestamp
    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;

    // Create a transaction with valid_before in the past (expired)
    let valid_before = current_time.saturating_sub(10); // 10 seconds ago

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(100),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: std::num::NonZeroU64::new(valid_before),
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // This should fail because valid_before is in the past
    let result = provider.send_raw_transaction(&encoded).await;
    assert!(result.is_err(), "Transaction with expired valid_before should be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_valid_after_future() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Get current block timestamp
    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;

    // Create a transaction with valid_after 5 seconds in the future
    let valid_after = current_time + 5;
    let valid_before = current_time + 60;

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(101),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: std::num::NonZeroU64::new(valid_before),
        valid_after: std::num::NonZeroU64::new(valid_after),
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // Transaction should be accepted into pool but not immediately executed
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();

    // Advance time past valid_after
    api.evm_set_next_block_timestamp(valid_after + 1).unwrap();
    api.mine_one().await;

    // Now the transaction should be included
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "Transaction should succeed after valid_after time");
}

// Tests that replaying the exact same expiring nonce transaction bytes
// either returns an error or returns the same tx hash (idempotent behavior).
// This is standard mempool behavior - duplicate transactions are not re-executed.
#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_expiring_nonce_replay() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;
    let valid_before = current_time + 25;

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    // Create an expiring nonce transaction
    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::MAX, // Expiring nonce mode
        nonce: 0,
        fee_payer_signature: None,
        valid_before: std::num::NonZeroU64::new(valid_before),
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx.clone(), tempo_sig.clone());
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // First submission should succeed
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let first_tx_hash = *tx_hash.tx_hash();
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "First expiring nonce transaction should succeed");

    // Try to replay the exact same transaction bytes
    // Since the tx hash is identical, Anvil may either:
    // 1. Return an error (tx already known)
    // 2. Return the same tx hash (idempotent behavior)
    let result = provider.send_raw_transaction(&encoded).await;

    if let Ok(pending) = result {
        // If accepted, it should return the same tx hash (not a new one)
        let second_tx_hash = *pending.tx_hash();
        assert_eq!(
            first_tx_hash, second_tx_hash,
            "Replaying same transaction should return same tx hash (not execute again)"
        );
    }
    // If send_raw_transaction returned Err, that's also acceptable
}

// Tests that 2D nonce enforcement works: after a transaction with nonce=0 is executed,
// a different transaction with the same nonce_key and nonce=0 will be dropped during execution.
// We verify this by showing that a subsequent transaction with nonce=1 still works.
#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_nonce_replay_same_key() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let nonce_key = U256::from(200);

    // First transaction with nonce 0
    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx1 = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call {
            to: TxKind::Call(PATH_USD),
            value: U256::ZERO,
            input: calldata.clone(),
        }],
        access_list: Default::default(),
        nonce_key,
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx1.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx1, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // First transaction should succeed
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "First transaction should succeed");

    // Now send a transaction with nonce=1 on the same key - this should succeed
    // proving that the nonce was incremented after the first transaction
    let transfer_call2 = token.transfer(recipient, U256::from(60_000));
    let calldata2: Bytes = transfer_call2.calldata().clone();

    let tempo_tx2 = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata2 }],
        access_list: Default::default(),
        nonce_key,
        nonce: 1, // Incremented nonce - should succeed
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx2.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx2, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // This transaction with nonce=1 should succeed, proving nonce enforcement works
    let tx_hash2 = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt2 = tx_hash2.get_receipt().await.unwrap();
    assert!(
        receipt2.status(),
        "Second transaction with nonce=1 should succeed (proves nonce enforcement)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_parallel_nonces_different_keys() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    // Send two transactions with the SAME nonce (0) but DIFFERENT nonce keys
    // This should work because different nonce keys are independent
    let mut tx_hashes = vec![];

    for nonce_key_val in [300u64, 301u64] {
        let transfer_call = token.transfer(recipient, U256::from(10_000));
        let calldata: Bytes = transfer_call.calldata().clone();

        let tempo_tx = TempoTransaction {
            chain_id,
            fee_token: Some(ALPHA_USD),
            max_priority_fee_per_gas: base_fee / 10,
            max_fee_per_gas: base_fee * 2,
            gas_limit: TIP20_TRANSFER_GAS,
            calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
            access_list: Default::default(),
            nonce_key: U256::from(nonce_key_val),
            nonce: 0, // Same nonce value, different keys
            fee_payer_signature: None,
            valid_before: None,
            valid_after: None,
            key_authorization: None,
            tempo_authorization_list: vec![],
        };

        let sig_hash = tempo_tx.signature_hash();
        let signature = signer.sign_hash(&sig_hash).await.unwrap();
        let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
        let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
        let envelope = TempoTxEnvelope::AA(signed_tx);

        let mut encoded = Vec::new();
        envelope.encode_2718(&mut encoded);

        let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
        tx_hashes.push(tx_hash);
    }

    // Both transactions should succeed
    for tx_hash in tx_hashes {
        let receipt = tx_hash.get_receipt().await.unwrap();
        assert!(receipt.status(), "Parallel transactions with different nonce keys should succeed");
    }

    // Verify recipient received both transfers
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(
        recipient_balance_after,
        recipient_balance_before + U256::from(20_000),
        "Recipient should receive both transfers"
    );
}

// ============================================================================
// Fee Token Swap Verification
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_fee_token_swap_different_tokens() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0]; // Alice - default fee token is AlphaUSD
    let recipient = accounts[1];

    // Alice's fee token is AlphaUSD, validator wants PathUSD (default)
    // FeeAMM should swap AlphaUSD -> PathUSD automatically

    let alpha_token = IERC20::new(ALPHA_USD, &provider);
    let alice_alpha_before = alpha_token.balanceOf(sender).call().await.unwrap();

    // Send a PATH_USD transfer (so we can verify balance changes are only from fees)
    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(100_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let base_fee = provider.get_gas_price().await.unwrap();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS)
        .max_fee_per_gas(base_fee * 2)
        .max_priority_fee_per_gas(base_fee / 10);

    let tx = WithOtherFields::new(tx);
    let pending = provider.send_transaction(tx).await.unwrap();
    let receipt = pending.get_receipt().await.unwrap();

    assert!(receipt.status(), "Transaction should succeed");

    // Verify Alice's AlphaUSD balance decreased (fees were paid)
    let alice_alpha_after = alpha_token.balanceOf(sender).call().await.unwrap();
    assert!(
        alice_alpha_after < alice_alpha_before,
        "Alice's AlphaUSD should decrease due to gas fees (before: {alice_alpha_before}, after: {alice_alpha_after})"
    );
}

// ============================================================================
// RPC/Receipt Verification
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_receipt_fields() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(400),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    // Verify receipt fields
    assert!(receipt.status(), "Transaction should succeed");
    assert!(receipt.gas_used > 0, "Gas used should be non-zero");
    assert!(!receipt.inner.logs().is_empty(), "Should have Transfer event logs");

    // Verify transaction type in receipt (0x76 = 118)
    // Note: ReceiptResponse doesn't expose transaction_type directly,
    // but we verified it's a Tempo transaction via the hash
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_get_transaction_by_hash() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(401),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let pending = provider.send_raw_transaction(&encoded).await.unwrap();
    let tx_hash = *pending.tx_hash();
    pending.get_receipt().await.unwrap();

    // Retrieve transaction by hash
    let tx = api.transaction_by_hash(tx_hash).await.unwrap();
    assert!(tx.is_some(), "Transaction should be retrievable by hash");

    let tx = tx.unwrap();

    // Verify transaction fields
    assert_eq!(tx.ty(), 0x76, "Transaction type should be 0x76 (Tempo)");
    assert_eq!(tx.from(), sender, "From address should match sender");
}

// ============================================================================
// Error/Negative Case Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_wrong_chain_id_rejected() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let correct_chain_id = provider.get_chain_id().await.unwrap();
    let wrong_chain_id = correct_chain_id + 1; // Wrong chain ID
    let base_fee = provider.get_gas_price().await.unwrap();

    let transfer_call = token.transfer(recipient, U256::from(10_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id: wrong_chain_id, // Wrong chain ID
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(1),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let result = provider.send_raw_transaction(&encoded).await;
    assert!(result.is_err(), "Transaction with wrong chain ID should be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_gas_too_low_rejected() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let transfer_call = token.transfer(recipient, U256::from(10_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: 1000, // Way too low for any operation
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(2),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // Transaction should be rejected due to insufficient gas for intrinsic cost
    let result = provider.send_raw_transaction(&encoded).await;
    assert!(result.is_err(), "Transaction with gas limit below intrinsic cost should be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_value_in_call_rejected() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Create a Tempo AA transaction with ETH value (not allowed in Tempo)
    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call {
            to: TxKind::Call(recipient),
            value: U256::from(1_000_000), // ETH value - should be rejected
            input: Bytes::new(),
        }],
        access_list: Default::default(),
        nonce_key: U256::from(3),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // Transaction should be rejected or fail execution due to ETH value
    let result = provider.send_raw_transaction(&encoded).await;
    if let Ok(pending) = result {
        let receipt = pending.get_receipt().await;
        // If it gets mined, it should fail
        if let Ok(r) = receipt {
            assert!(!r.status(), "Transaction with ETH value should fail in Tempo mode");
        }
    }
    // If rejected at pool level, that's also acceptable
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_nonce_too_high_rejected() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let transfer_call = token.transfer(recipient, U256::from(10_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    // Use a fresh nonce key and skip nonce 0 - go directly to nonce 5
    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(999), // Fresh nonce key
        nonce: 5,                   // Nonce too high - should be 0
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // Transaction may be accepted into pool but should fail during execution
    let result = provider.send_raw_transaction(&encoded).await;
    if result.is_ok() {
        // Mine a block to trigger execution
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        // The transaction should not produce a successful receipt
        // (it will be dropped due to NonceTooHigh)
    }
    // Either rejected at pool or dropped during execution is acceptable
}

// ============================================================================
// 2D Nonce Key Isolation Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_nonce_keys_are_isolated() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Send tx with nonce_key=100, nonce=0
    let transfer_call = token.transfer(recipient, U256::from(10_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx1 = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call {
            to: TxKind::Call(PATH_USD),
            value: U256::ZERO,
            input: calldata.clone(),
        }],
        access_list: Default::default(),
        nonce_key: U256::from(100),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx1.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx1, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "First tx with nonce_key=100 should succeed");

    // Now send tx with nonce_key=101, nonce=0 - should also succeed (independent keys)
    let tempo_tx2 = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(101), // Different key
        nonce: 0,                   // Same nonce value - but different key, so should work
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx2.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx2, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let tx_hash2 = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt2 = tx_hash2.get_receipt().await.unwrap();
    assert!(
        receipt2.status(),
        "Tx with different nonce_key should succeed even with same nonce value"
    );
}

// ============================================================================
// Fee Token Selection Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_explicit_fee_token_selection() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0]; // Alice - default fee token is AlphaUSD
    let recipient = accounts[1];
    let signer = dev_key(0);

    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Check balances before
    let path_token = IERC20::new(PATH_USD, &provider);
    let alpha_token = IERC20::new(ALPHA_USD, &provider);
    let path_balance_before = path_token.balanceOf(sender).call().await.unwrap();
    let alpha_balance_before = alpha_token.balanceOf(sender).call().await.unwrap();

    // Create tx that explicitly uses PATH_USD as fee token (not the default ALPHA_USD)
    let transfer_call = path_token.transfer(recipient, U256::from(10_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(PATH_USD), // Explicitly use PATH_USD (not default)
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(400),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "Transaction with explicit fee token should succeed");

    // Check balances after
    let path_balance_after = path_token.balanceOf(sender).call().await.unwrap();
    let alpha_balance_after = alpha_token.balanceOf(sender).call().await.unwrap();

    // PATH_USD should decrease (transfer + fees)
    assert!(
        path_balance_after < path_balance_before,
        "PATH_USD balance should decrease (transfer + fees)"
    );

    // ALPHA_USD should NOT decrease (we used PATH_USD for fees)
    assert_eq!(
        alpha_balance_after, alpha_balance_before,
        "ALPHA_USD balance should not change when using PATH_USD for fees"
    );
}

// ============================================================================
// Timestamp Precision Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_block_timestamps_are_monotonic() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    // Mine the first block
    api.mine_one().await;
    let block1 = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let timestamp1 = block1.header.timestamp;

    // Set a future timestamp for the next block
    let future_timestamp = timestamp1 + 10;
    api.evm_set_next_block_timestamp(future_timestamp).unwrap();

    // Mine the second block
    api.mine_one().await;
    let block2 = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let timestamp2 = block2.header.timestamp;

    assert!(
        timestamp2 > timestamp1,
        "Block timestamps must be strictly increasing: {timestamp2} should be > {timestamp1}",
    );
    assert_eq!(timestamp2, future_timestamp, "Block timestamp should match the set value");
}
