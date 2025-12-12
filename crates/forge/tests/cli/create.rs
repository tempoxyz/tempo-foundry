//! Contains various tests for checking the `forge create` subcommand

use crate::utils::{self, EnvExternalities};
use alloy_primitives::Address;
use foundry_compilers::artifacts::remappings::Remapping;
use foundry_test_utils::{
    forgetest,
    util::{OutputExt, TestCommand, TestProject},
};
use std::str::FromStr;

/// This will insert _dummy_ contract that uses a library
///
/// **NOTE** This is intended to be linked against a random address and won't actually work. The
/// purpose of this is _only_ to make sure we can deploy contracts linked against addresses.
///
/// This will create a library `remapping/MyLib.sol:MyLib`
///
/// returns the contract argument for the create command
fn setup_with_simple_remapping(prj: &TestProject) -> String {
    // explicitly set remapping and libraries
    prj.update_config(|config| {
        config.remappings = vec![Remapping::from_str("remapping/=lib/remapping/").unwrap().into()];
        config.libraries = vec![format!("remapping/MyLib.sol:MyLib:{:?}", Address::random())];
    });

    prj.add_source(
        "LinkTest",
        r#"
import "remapping/MyLib.sol";
contract LinkTest {
    function foo() public returns (uint256) {
        return MyLib.foobar(1);
    }
}
"#,
    );

    prj.add_lib(
        "remapping/MyLib",
        r"
library MyLib {
    function foobar(uint256 a) public view returns (uint256) {
    	return a * 100;
    }
}
",
    );

    "src/LinkTest.sol:LinkTest".to_string()
}

fn setup_oracle(prj: &TestProject) -> String {
    prj.update_config(|c| {
        c.libraries = vec![format!(
            "./src/libraries/ChainlinkTWAP.sol:ChainlinkTWAP:{:?}",
            Address::random()
        )];
    });

    prj.add_source(
        "Contract",
        r#"
import {ChainlinkTWAP} from "./libraries/ChainlinkTWAP.sol";
contract Contract {
    function getPrice() public view returns (int latest) {
        latest = ChainlinkTWAP.getLatestPrice(0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE);
    }
}
"#,
    );

    prj.add_source(
        "libraries/ChainlinkTWAP",
        r"
library ChainlinkTWAP {
   function getLatestPrice(address base) public view returns (int256) {
        return 0;
   }
}
",
    );

    "src/Contract.sol:Contract".to_string()
}

/// configures the `TestProject` with the given closure and calls the `forge create` command
fn create_on_chain<F>(info: Option<EnvExternalities>, prj: TestProject, mut cmd: TestCommand, f: F)
where
    F: FnOnce(&TestProject) -> String,
{
    if let Some(info) = info {
        let contract_path = f(&prj);

        let output = cmd
            .arg("create")
            .args(info.create_args())
            .arg(contract_path)
            .assert_success()
            .get_output()
            .stdout_lossy();
        let _address = utils::parse_deployed_address(output.as_str())
            .unwrap_or_else(|| panic!("Failed to parse deployer {output}"));
    }
}

// tests `forge` create on goerli if correct env vars are set
forgetest!(can_create_simple_on_goerli, |prj, cmd| {
    create_on_chain(EnvExternalities::goerli(), prj, cmd, setup_with_simple_remapping);
});

// tests `forge` create on goerli if correct env vars are set
forgetest!(can_create_oracle_on_goerli, |prj, cmd| {
    create_on_chain(EnvExternalities::goerli(), prj, cmd, setup_oracle);
});

// tests `forge` create on amoy if correct env vars are set
forgetest!(can_create_oracle_on_amoy, |prj, cmd| {
    create_on_chain(EnvExternalities::amoy(), prj, cmd, setup_oracle);
});
