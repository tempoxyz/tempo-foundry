//! Contains various tests related to `forge script`.

use crate::constants::TEMPLATE_CONTRACT;
use foundry_test_utils::{
    ScriptOutcome, ScriptTester,
    rpc::next_http_archive_rpc_url,
    util::{OTHER_SOLC_VERSION, SOLC_VERSION},
};
use std::fs;

// Tests that fork cheat codes can be used in script
forgetest_init!(
    #[ignore]
    can_use_fork_cheat_codes_in_script,
    |prj, cmd| {
        let script = prj.add_source(
            "Foo",
            r#"
import "forge-std/Script.sol";

contract ContractScript is Script {
    function setUp() public {}

    function run() public {
        uint256 fork = vm.activeFork();
        vm.rollFork(11469702);
    }
}
   "#,
        );

        let rpc = foundry_test_utils::rpc::next_http_rpc_endpoint();

        cmd.arg("script").arg(script).args(["--fork-url", rpc.as_str(), "-vvvvv"]).assert_success();
    }
);

// Tests that the `run` command works correctly
forgetest!(can_execute_script_command2, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
contract Demo {
    event log_string(string);
    function run() external {
        emit log_string("script ran");
    }
}
   "#,
    );

    cmd.arg("script").arg(script).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Logs ==
  script ran

"#]]);
});

// Tests that the `run` command works correctly when path *and* script name is specified
forgetest!(can_execute_script_command_fqn, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
contract Demo {
    event log_string(string);
    function run() external {
        emit log_string("script ran");
    }
}
   "#,
    );

    cmd.arg("script").arg(format!("{}:Demo", script.display())).assert_success().stdout_eq(str![[
        r#"
...
Script ran successfully.
[GAS]

== Logs ==
  script ran
...
"#
    ]]);
});

// Tests that the run command can run arbitrary functions
forgetest!(can_execute_script_command_with_sig, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
contract Demo {
    event log_string(string);
    function myFunction() external {
        emit log_string("script ran");
    }
}
   "#,
    );

    cmd.arg("script").arg(script).arg("--sig").arg("myFunction()").assert_success().stdout_eq(
        str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Logs ==
  script ran

"#]],
    );
});

static FAILING_SCRIPT: &str = r#"
import "forge-std/Script.sol";

contract FailingScript is Script {
    function run() external {
        revert("failed");
    }
}
"#;

// Tests that execution throws upon encountering a revert in the script.
forgetest_async!(assert_exit_code_error_on_failure_script, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    let script = prj.add_source("FailingScript", FAILING_SCRIPT);

    // set up command
    cmd.arg("script").arg(script);

    // run command and assert error exit code
    cmd.assert_failure().stderr_eq(str![[r#"
Error: script failed: failed

"#]]);
});

// Tests that execution throws upon encountering a revert in the script with --json option.
// <https://github.com/foundry-rs/foundry/issues/2508>
forgetest_async!(assert_exit_code_error_on_failure_script_with_json, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    let script = prj.add_source("FailingScript", FAILING_SCRIPT);

    // set up command
    cmd.arg("script").arg(script).arg("--json");

    // run command and assert error exit code
    cmd.assert_failure().stderr_eq(str![[r#"
Error: script failed: failed

"#]]);
});

// Tests that the run command can run functions with arguments without specifying the signature
// <https://github.com/foundry-rs/foundry/issues/11240>
forgetest!(can_execute_script_command_with_args_no_sig, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
contract Demo {
    event log_string(string);
    event log_uint(uint);
    function run(uint256 a, uint256 b) external {
        emit log_string("script ran");
        emit log_uint(a);
        emit log_uint(b);
    }
}
   "#,
    );

    cmd.arg("script").arg(script).arg("1").arg("2").assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Logs ==
  script ran
  1
  2

"#]]);
});

// Tests that the run command can run functions with return values
forgetest!(can_execute_script_command_with_returned, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
contract Demo {
    event log_string(string);
    function run() external returns (uint256 result, uint8) {
        emit log_string("script ran");
        return (255, 3);
    }
}"#,
    );

    cmd.arg("script").arg(script).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Return ==
result: uint256 255
1: uint8 3

== Logs ==
  script ran

"#]]);
});

// checks that skipping build
forgetest_init!(can_execute_script_and_skip_contracts, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
contract Demo {
    event log_string(string);
    function run() external returns (uint256 result, uint8) {
        emit log_string("script ran");
        return (255, 3);
    }
}"#,
    );
    cmd.arg("script")
        .arg(script)
        .args(["--skip", "tests", "--skip", TEMPLATE_CONTRACT])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Return ==
result: uint256 255
1: uint8 3

== Logs ==
  script ran

"#]]);
});

forgetest_async!(can_run_script_with_empty_setup, |prj, cmd| {
    let mut tester = ScriptTester::new_broadcast_without_endpoint(cmd, prj.root());

    tester.add_sig("BroadcastEmptySetUp", "run()").simulate(ScriptOutcome::OkNoEndpoint);
});

forgetest_async!(does_script_override_correctly, |prj, cmd| {
    let mut tester = ScriptTester::new_broadcast_without_endpoint(cmd, prj.root());

    tester.add_sig("CheckOverrides", "run()").simulate(ScriptOutcome::OkNoEndpoint);
});

forgetest_async!(assert_tx_origin_is_not_overwritten, |prj, cmd| {
    cmd.args(["init", "--force"])
        .arg(prj.root())
        .assert_success()
        .stdout_eq(str![[r#"
Initializing [..]...
Installing forge-std in [..] (url: https://github.com/foundry-rs/forge-std, tag: None)
    Installed forge-std[..]
    Initialized forge project

"#]])
        .stderr_eq(str![[r#"
Warning: Target directory is not empty, but `--force` was specified
...

"#]]);

    let script = prj.add_script(
        "ScriptTxOrigin.s.sol",
        r#"
import { Script } from "forge-std/Script.sol";

contract ScriptTxOrigin is Script {
    function run() public {
        uint256 pk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        vm.startBroadcast(pk); // 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266

        ContractA contractA = new ContractA();
        ContractB contractB = new ContractB();

        contractA.test(address(contractB));
        contractB.method(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266);

        require(tx.origin == 0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38);
        vm.stopBroadcast();
    }
}

contract ContractA {
    function test(address _contractB) public {
        require(msg.sender == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "sender 1");
        require(tx.origin == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "origin 1");
        ContractB contractB = ContractB(_contractB);
        ContractC contractC = new ContractC();
        require(msg.sender == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "sender 2");
        require(tx.origin == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "origin 2");
        contractB.method(address(this));
        contractC.method(address(this));
        require(msg.sender == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "sender 3");
        require(tx.origin == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "origin 3");
    }
}

contract ContractB {
    function method(address sender) public view {
        require(msg.sender == sender, "sender");
        require(tx.origin == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "origin");
    }
}

contract ContractC {
    function method(address sender) public view {
        require(msg.sender == sender, "sender");
        require(tx.origin == 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266, "origin");
    }
}
   "#,
    );

    cmd.forge_fuse()
        .arg("script")
        .arg(script)
        .args(["--tc", "ScriptTxOrigin"])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

If you wish to simulate on-chain transactions pass a RPC URL.

"#]]);
});

forgetest_async!(assert_can_create_multiple_contracts_with_correct_nonce, |prj, cmd| {
    cmd.args(["init", "--force"])
        .arg(prj.root())
        .assert_success()
        .stdout_eq(str![[r#"
Initializing [..]...
Installing forge-std in [..] (url: https://github.com/foundry-rs/forge-std, tag: None)
    Installed forge-std[..]
    Initialized forge project

"#]])
        .stderr_eq(str![[r#"
Warning: Target directory is not empty, but `--force` was specified
...

"#]]);

    let script = prj.add_script(
        "ScriptTxOrigin.s.sol",
        r#"
import {Script, console} from "forge-std/Script.sol";

contract Contract {
  constructor() {
    console.log(tx.origin);
  }
}

contract SubContract {
  constructor() {
    console.log(tx.origin);
  }
}

contract BadContract {
  constructor() {
    new SubContract();
    console.log(tx.origin);
  }
}
contract NestedCreate is Script {
  function run() public {
    address sender = address(uint160(uint(keccak256("woops"))));

    vm.broadcast(sender);
    new BadContract();

    vm.broadcast(sender);
    new Contract();
  }
}
   "#,
    );

    cmd.forge_fuse()
        .arg("script")
        .arg(script)
        .args(["--tc", "NestedCreate"])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Logs ==
  0x159E2f2F1C094625A2c6c8bF59526d91454c2F3c
  0x159E2f2F1C094625A2c6c8bF59526d91454c2F3c
  0x159E2f2F1C094625A2c6c8bF59526d91454c2F3c

If you wish to simulate on-chain transactions pass a RPC URL.

"#]]);
});

forgetest_async!(assert_can_detect_target_contract_with_interfaces, |prj, cmd| {
    let script = prj.add_script(
        "ScriptWithInterface.s.sol",
        r#"
contract Script {
  function run() external {}
}

interface Interface {}
            "#,
    );

    cmd.arg("script").arg(script).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

"#]]);
});

forgetest_async!(assert_can_detect_unlinked_target_with_libraries, |prj, cmd| {
    let script = prj.add_script(
        "ScriptWithExtLib.s.sol",
        r#"
library Lib {
    function f() public {}
}

contract Script {
    function run() external {
        Lib.f();
    }
}
            "#,
    );

    cmd.arg("script").arg(script).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

If you wish to simulate on-chain transactions pass a RPC URL.

"#]]);
});

forgetest_async!(can_detect_contract_when_multiple_versions, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());

    prj.add_script(
        "A.sol",
        &format!(
            r#"
pragma solidity {SOLC_VERSION};
import "./B.sol";

contract ScriptA {{}}
"#
        ),
    );

    prj.add_script(
        "B.sol",
        &format!(
            r#"
pragma solidity >={OTHER_SOLC_VERSION} <={SOLC_VERSION};
import 'forge-std/Script.sol';

contract ScriptB is Script {{
    function run() external {{
        vm.broadcast();
        address(0).call("");
    }}
}}
"#
        ),
    );

    prj.add_script(
        "C.sol",
        &format!(
            r#"
pragma solidity {OTHER_SOLC_VERSION};
import "./B.sol";

contract ScriptC {{}}
"#
        ),
    );

    let mut tester = ScriptTester::new(cmd, None, prj.root(), "script/B.sol");
    tester.cmd.forge_fuse().args(["script", "script/B.sol"]);
    tester.simulate(ScriptOutcome::OkNoEndpoint);
});

forgetest_async!(can_sign_with_script_wallet_single, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());

    let mut tester = ScriptTester::new_broadcast_without_endpoint(cmd, prj.root());
    tester
        .add_sig("ScriptSign", "run()")
        .load_private_keys(&[0])
        .await
        .simulate(ScriptOutcome::OkNoEndpoint);
});

forgetest_async!(can_sign_with_script_wallet_multiple, |prj, cmd| {
    let mut tester = ScriptTester::new_broadcast_without_endpoint(cmd, prj.root());
    let acc = tester.accounts_pub[0].to_checksum(None);
    tester
        .add_sig("ScriptSign", "run(address)")
        .arg(&acc)
        .load_private_keys(&[0, 1, 2])
        .await
        .simulate(ScriptOutcome::OkRun);
});

forgetest_async!(fails_with_function_name_and_overloads, |prj, cmd| {
    let script = prj.add_script(
        "Script.s.sol",
        r#"
contract Script {
    function run() external {}

    function run(address,uint256) external {}
}
            "#,
    );

    cmd.arg("script").args([&script.to_string_lossy(), "--sig", "run"]);
    cmd.assert_failure().stderr_eq(str![[r#"
Error: Multiple functions with the same name `run` found in the ABI

"#]]);
});

forgetest_async!(can_decode_custom_errors, |prj, cmd| {
    cmd.args(["init", "--force"])
        .arg(prj.root())
        .assert_success()
        .stdout_eq(str![[r#"
Initializing [..]...
Installing forge-std in [..] (url: https://github.com/foundry-rs/forge-std, tag: None)
    Installed forge-std[..]
    Initialized forge project

"#]])
        .stderr_eq(str![[r#"
Warning: Target directory is not empty, but `--force` was specified
...

"#]]);

    let script = prj.add_script(
        "CustomErrorScript.s.sol",
        r#"
import { Script } from "forge-std/Script.sol";

contract ContractWithCustomError {
    error CustomError();

    constructor() {
        revert CustomError();
    }
}

contract CustomErrorScript is Script {
    ContractWithCustomError test;

    function run() public {
        test = new ContractWithCustomError();
    }
}
"#,
    );

    cmd.forge_fuse().arg("script").arg(script).args(["--tc", "CustomErrorScript"]);
    cmd.assert_failure().stderr_eq(str![[r#"
Error: script failed: CustomError()

"#]]);
});

forgetest_init!(can_get_script_wallets, |prj, cmd| {
    let script = prj.add_source(
        "Foo",
        r#"
import "forge-std/Script.sol";

interface Vm {
    function getWallets() external view returns (address[] memory wallets);
}

contract WalletScript is Script {
    function run() public view {
        address[] memory wallets = Vm(address(vm)).getWallets();
        console.log(wallets[0]);
    }
}"#,
    );
    cmd.arg("script")
        .arg(script)
        .args([
            "--private-key",
            "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
            "-v",
        ])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Logs ==
  0xa0Ee7A142d267C1f36714E4a8F75612F20a79720

"#]]);
});

forgetest_init!(can_remember_keys, |prj, cmd| {
    let script = prj
        .add_source(
            "Foo",
            r#"
import "forge-std/Script.sol";

interface Vm {
    function rememberKeys(string calldata mnemonic, string calldata derivationPath, uint32 count) external returns (address[] memory keyAddrs);
}

contract WalletScript is Script {
    function run() public {
        string memory mnemonic = "test test test test test test test test test test test junk";
        string memory derivationPath = "m/44'/60'/0'/0/";
        address[] memory wallets = Vm(address(vm)).rememberKeys(mnemonic, derivationPath, 3);
        for (uint256 i = 0; i < wallets.length; i++) {
            console.log(wallets[i]);
        }
    }
}"#,
        );
    cmd.arg("script").arg(script).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.
[GAS]

== Logs ==
  0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
  0x70997970C51812dc3A010C7d01b50e0d17dc79C8
  0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC

"#]]);
});

// <https://github.com/foundry-rs/foundry/issues/9661>
forgetest_async!(
    #[ignore = "tempo skip"]
    should_set_correct_sender_nonce_via_cli,
    |prj, cmd| {
        foundry_test_utils::util::initialize(prj.root());
        prj.add_script(
            "MyScript.s.sol",
            r#"
        import {Script, console} from "forge-std/Script.sol";

    contract MyScript is Script {
        function run() public view {
            console.log("sender nonce", vm.getNonce(msg.sender));
        }
    }
    "#,
        );

        let rpc_url = next_http_archive_rpc_url();

        let fork_bn = 21614115;

        cmd.forge_fuse()
            .args([
                "script",
                "MyScript",
                "--sender",
                "0x4838B106FCe9647Bdf1E7877BF73cE8B0BAD5f97",
                "--fork-block-number",
                &fork_bn.to_string(),
                "--rpc-url",
                &rpc_url,
            ])
            .assert_success()
            .stdout_eq(str![[r#"[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
...
== Logs ==
  sender nonce 1124703[..]"#]]);
    }
);

// Tests warn when artifact source file no longer exists.
// <https://github.com/foundry-rs/foundry/issues/9068>
forgetest_init!(should_warn_if_artifact_source_no_longer_exists, |prj, cmd| {
    prj.initialize_default_contracts();
    cmd.args(["script", "script/Counter.s.sol"]).assert_success().stdout_eq(str![[r#"
...
Script ran successfully.
...

"#]]);
    fs::rename(
        prj.paths().scripts.join("Counter.s.sol"),
        prj.paths().scripts.join("Counter1.s.sol"),
    )
    .unwrap();
    cmd.forge_fuse().args(["script", "script/Counter1.s.sol"]).assert_success().stderr_eq(str![[r#"
...
Warning: Detected artifacts built from source files that no longer exist. Run `forge clean` to make sure builds are in sync with project files.
 - [..]script/Counter.s.sol
...

"#]])
        .stdout_eq(str![[r#"
...
Script ran successfully.
...

"#]]);
});

// Tests that script reverts if it uses `address(this)`.
forgetest_init!(should_revert_on_address_opcode, |prj, cmd| {
    prj.add_script(
        "ScriptWithAddress.s.sol",
        r#"
        import {Script, console} from "forge-std/Script.sol";

    contract ScriptWithAddress is Script {
        function run() public view {
            console.log("script address", address(this));
        }
    }
    "#,
    );

    cmd.arg("script").arg("ScriptWithAddress").assert_failure().stderr_eq(str![[r#"
Error: script failed: Usage of `address(this)` detected in script contract. Script contracts are ephemeral and their addresses should not be relied upon.

"#]]);

    // Disable script protection.
    prj.update_config(|config| {
        config.script_execution_protection = false;
    });
    cmd.assert_success().stdout_eq(str![[r#"
...
Script ran successfully.
...

"#]]);
});

// Test that --verify without --broadcast fails with a clear error message
forgetest!(verify_without_broadcast_fails, |prj, cmd| {
    let script = prj.add_source(
        "Counter",
        r#"
import "forge-std/Script.sol";

contract CounterScript is Script {
    function run() external {
        // Simple script that does nothing
    }
}
   "#,
    );

    cmd.args([
        "script",
        script.to_str().unwrap(),
        "--verify",
        "--rpc-url",
        "https://sepolia.infura.io/v3/test",
    ])
    .assert_failure()
    .stderr_eq(str![[r#"
error: the following required arguments were not provided:
  --broadcast

Usage: [..] script --broadcast --verify --fork-url <URL> <PATH> [ARGS]...

For more information, try '--help'.

"#]]);
});
