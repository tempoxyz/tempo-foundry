use super::*;
use alloy_primitives::U256;
use foundry_cli::utils::load_dotenv;
use foundry_test_utils::TestCommand;

forgetest_async!(tip20, |_prj, cmd| {
    load_dotenv();
    let rpc = std::env::var("TEMPO_RPC").expect("TEMPO_RPC must be informed");

    // Anvil PK and addresses
    const PK: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const ADDR1: &str = "0x5FbDB2315678afecb367f032d93F642f64180aa3";
    const ADDR2: &str = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";

    // Tempo-related constants and values
    const ALPHA: &str = "0x20c0000000000000000000000000000000000001";
    let drip = U256::from(1e12);

    let get_balance = |cmd: &mut TestCommand, token, addr| -> U256 {
        // Fetch token balance on the given address
        let output = cmd
            .cast_fuse()
            .args(["erc20", "balance", token, addr, "--rpc-url", &rpc])
            .assert_success()
            .get_output()
            .stdout_lossy();

        // Parse the balance from the output (format: "1000000000000 [1e12]")
        output.split_whitespace().next().unwrap().parse().unwrap()
    };

    let init_balance = get_balance(&mut cmd, ALPHA, ADDR1);
    assert!(init_balance > U256::ZERO);

    // Fund the address from the faucet
    cmd.cast_fuse().args(["rpc", "tempo_fundAddress", ADDR1, "--rpc-url", &rpc]).assert_success();

    let post_balance = get_balance(&mut cmd, ALPHA, ADDR1);
    assert_eq!(post_balance, init_balance + drip);

    // Test ERC20 transfer from ADDR1 to ADDR2
    let addr1_balance_before = post_balance;
    let addr2_balance_before = get_balance(&mut cmd, ALPHA, ADDR2);
    cmd.cast_fuse()
        .args([
            "erc20",
            "transfer",
            ALPHA,
            ADDR2,
            &drip.to_string(),
            "--private-key",
            PK,
            "--rpc-url",
            &rpc,
        ])
        .assert_success();

    // Verify balance changes
    let addr1_balance_after = get_balance(&mut cmd, ALPHA, ADDR1);
    let addr2_balance_after = get_balance(&mut cmd, ALPHA, ADDR2);
    assert_eq!(addr1_balance_after, addr1_balance_before - drip);
    assert_eq!(addr2_balance_after, addr2_balance_before + drip);
});
