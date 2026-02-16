//! Contains various tests related to `forge script`.

use crate::constants::TEMPLATE_CONTRACT;
use alloy_hardforks::EthereumHardfork;
use alloy_primitives::{Address, Bytes, address, hex};
use anvil::{NodeConfig, spawn};
use forge_script_sequence::ScriptSequence;
use foundry_test_utils::{
    ScriptOutcome, ScriptTester,
    rpc::{self, next_http_archive_rpc_url},
    snapbox::IntoData,
    util::{OTHER_SOLC_VERSION, SOLC_VERSION},
};
use regex::Regex;
use serde_json::Value;
use std::{env, fs, path::PathBuf};

// Tests that fork cheat codes can be used in script
forgetest_init!(can_use_fork_cheat_codes_in_script, |prj, cmd| {
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
});

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

// Tests that the manually specified gas limit is used when using the --unlocked option
forgetest_async!(
    #[ignore = "tempo skip - uses Ethereum archive RPC which lacks Tempo block fields"]
    can_execute_script_command_with_manual_gas_limit_unlocked,
    |prj, cmd| {
        foundry_test_utils::util::initialize(prj.root());
        let deploy_script = prj.add_source(
            "Foo",
            r#"
import "forge-std/Script.sol";

contract GasWaster {
    function wasteGas(uint256 minGas) public {
        require(gasleft() >= minGas, "Gas left needs to be higher");
    }
}
contract DeployScript is Script {
    function run() external {
        vm.startBroadcast();
        GasWaster gasWaster = new GasWaster();
        gasWaster.wasteGas{gas: 500000}(200000);
    }
}
   "#,
        );

        let deploy_contract = deploy_script.display().to_string() + ":DeployScript";

        let node_config =
            NodeConfig::test().with_eth_rpc_url(Some(rpc::next_http_archive_rpc_url()));
        let (_api, handle) = spawn(node_config).await;
        let dev = handle.dev_accounts().next().unwrap();
        cmd.set_current_dir(prj.root());

        cmd.args([
        "script",
        &deploy_contract,
        "--root",
        prj.root().to_str().unwrap(),
        "--fork-url",
        &handle.http_endpoint(),
        "--sender",
        format!("{dev:?}").as_str(),
        "-vvvvv",
        "--slow",
        "--broadcast",
        "--unlocked",
        "--ignored-error-codes=2018", // `wasteGas` can be restricted to view
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] DeployScript::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [..] → new GasWaster@[..]
    │   └─ ← [Return] 415 bytes of code
    ├─ [..] GasWaster::wasteGas(200000 [2e5])
    │   └─ ← [Stop]
    └─ ← [Stop]


Script ran successfully.

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [..] → new GasWaster@[..]
    └─ ← [Return] 415 bytes of code

  [..] GasWaster::wasteGas(200000 [2e5])
    └─ ← [Stop]


==========================

Chain 1

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================
2026-01-26T13:02:37.928314Z ERROR alloy_provider::blocks: failed to fetch block number=24319174 err=deserialization error: missing field `timestampMillis` at line 1 column 29656
{"hash":"0x9f0d5d78a919f8a78dd450eff8777bc2d9b58359e5a3594b8bd4a2358aeadddc","parentHash":"0x3065c1a1b108c3a5a84d13b89121408b2a3f7cc98f9e7d00b50770c00e4cd30e","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","miner":"0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97","stateRoot":"0x26a0259a762525441ed43977b7cba48957d71ae124cb776baff81b328756a1ff","transactionsRoot":"0x094e9788dde8ad0b34fef34a082dc698a4b82e3fbd069f398d53b6a961500bc8","receiptsRoot":"0x342f18503d2b252cbf111276cee6fc1251da8bb8e6668da1193b5d678315a480","logsBloom":"0xfdefd5f6edffffbfffbbcdffe7cffffffffefeffdeff19ff3fe9ffdc7ffffdffbd7fbdfffffeffff7fffffffd5fe77dfd7fffbffffdf6fffffddbff73fff7fbfff7fffb9ffdffbeff8fbf7fbdfedfbf77def9f7d7fdefd7a5fff7bbbfbffff97df7fffffff7ffbf6fdffffffffcfdfdfeffbabffbd7cdffffbf7cfdfffefff7efffbffef5feff9fffffff7efff9bf77fbefff7fffdd3f3fbefff7dfe7fddff6fffffffefffbbbfbfb3f5fdfaffb75fbff7f7fffdbf77ffdb9ffbfff3fcae7dfbebff7f7fffebfff73fff5fdfbbfeffff7eacbeffd7ffdcffbfff4bfff3ffff7bfdfdffff3bfd7fafb9eff7ebfffbffffbfffeefed7fffdcfbedbf1b777bfe7b5","difficulty":"0x0","number":"0x17314c6","gasLimit":"0x3938700","gasUsed":"0x1d9798a","timestamp":"0x697765df","extraData":"0x546974616e2028746974616e6275696c6465722e78797a29","mixHash":"0x0546bc1f2c9de2cf933258dee81f44852a8307259a7defca84f80e21b26285bd","nonce":"0x0000000000000000","baseFeePerGas":"0x5db6e66","withdrawalsRoot":"0x425131cbe76287c5396f7540eb710189a2d3d3516549e8f67d27992cbace588f","blobGasUsed":"0x20000","excessBlobGas":"0xac77864","parentBeaconBlockRoot":"0x0b1acd0b986cd42d30b89556b5d98749758457ad3b3a54729e0b38b8290737a1","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","size":"0x267e4","uncles":[],"transactions":["0x7d41e7ef1d7cd34a28ea3e62c9754817cb7216cfe3adf5f8ed8a2089b60605cc","0x0970a15f5a41c03fa6e9492511f7920c5dc98e5b90b19eb919b408a6e57bedce","0xe6362e427b6ad446c68c0d0d11754e9abeac094a965b6fd7244aee238f627f85","0x3c9164c923e744ac41f9e5636c9fbc45251c96090ee2acfc10e01b1095da7900","0xdb3561eb97241fe525c927c0ad8914272ebb4df36b34ba748ac2a98b79616ae5","0x2ac20bfb7382178d0f7bc2f4fb540625d14bac444a3da719fc3a3f7720e35130","0x09106d9bde664e0136d02cb36918a506ddb276362dbc0fd00b05286e7f0366bd","0xae4334df5c4869efc3ad97b49b2e87c3a17f876e0eaf0a18901508e6839470dd","0x3f93fd620675a94da8b0c70cfcee7ff99dee228604578897cac2daf7d4232a53","0xa262f0e05019489f2a7c18db6c378d64a49a172cb6dac544781a01a2fe69c577","0x2bdc002b5be3eb1c1aaad86d1744abe6cedaa50cf94e6aa238e62b7646ed88e0","0x43c5749f0cb6bde333787fbb26853a1e0195c9a9d109d899ec79cebe693cccb3","0x0ba6389e1f8731702b5fd2d36e9d887a7abaf96a89a11095fba98fafa83341e0","0x8b715fb232d7f014f4e22d73e9b2e298983c3a81b54130ed18fa47ef9bde4434","0x999e4bdc8ab574d60e3587921c31227696de7737a4190877cd6593510ed5509a","0x760abd67da0df8bf7d1a2a739d544a4e575db11b611ba83b01b91ede330c3223","0x490222faf70949b2c3120db271c8f908b150fe607b5378f98c7cf2f912a5cef9","0x943e708f39ea8793937cb4442118591c537ab4e7b8f6f4ad74bbee0abb9063ea","0x8fc6f4cd53a0f36a77ee7203a2bd86595607f544fa683f012f5a103169d2321c","0x27341d08420c67974aa13649fb50a00ba740e7322e3b65e4b97cde7121f2c633","0xeae29ee97ad57befc35ca3d3720ce8a4d1daba11018570e4674f730c820b1fb2","0xc6702e134531284156e5ee9c367159814ba2fbf5549d07a067a3bf2245855476","0x7638cce10ca37548f05619a960e295435ea76bca9dc3f48e63a9d9df3049681e","0xf580483760c9ab07e58f3419065df977144ed6b2b8ffec66e78cc99b283acfb8","0xfdd15b1c04eaa618af487593b0232cb29fd1abdd2ff038ae0831cd4141ab98d1","0x31056be22890a1ecde014e5dadaa01674d47cbb31473067376f39e9cee73013c","0x7e3abe0305d437807970680448b2c548f37e303114d254b0237cd389e0ccfa7b","0x84449848f8b077a42b99db30cf7db35c2401f4a97f54dacfdac9e98a81df3687","0xf0480f5dcc374efeb194b00ded1df2ae06579c1002744413e22b8d6e6b0c1d0b","0xab56dea89c6135a0caaf4131e20f7f3da292f2cda0e388c621d59a3fd8cbb3b0","0x190fd0421c6047edcb74e1cee495a80c70e0c45114b0f5d069d0905797f7164c","0xd3248d3cde9903aeea7d645f75624a6938caf8a7a1ccbab4335554fdb12bc34f","0xa34271eed705bc2f108057ed78fae6d35879ea57deb3908e5a2bd290dc57728f","0xeef32064f7fe03b911eef37676e2e973405f227f73378519891fa62554800494","0xff01caf1ea9d8f290e2a2484d529d99742917cfdc6276dea211bf55e97cc22ba","0xe065d639e35b22d6a594ac8094ddc47c7942af3d1937aed079f4c0e71532318f","0xb02e0c87e363a4b8e69760afdd20f6e6d7e8ea66db6c35d10bd7379c5d9ff2ca","0x6b9f515e32aa196b78b725881bf24131d023166b68569146d80a49601ae430dd","0x4f27841385f595083a6de617a50ad805e8ca1d516ac15be5a00025b6d7f7effd","0x6f1b2158459e794251d24c89634adfd178522c4234d17210f58520d3078de168","0x927885025cd296f09b94f5707b5abfa979452afa6f2070ce74b6bb55c834c6a8","0x0d87fada9a2e1f30640acefb0f9cf5e3964fc06cca63cd15e8de3249cf066ace","0x60c6c2eec49b0e87655efd7229e5b9813b7c6b4b9bb158c41787b88e516ccff7","0xaff1cf28459bbf09e4dfb508129f5a5880370543d464a2863ef92021b64c5431","0x9494fbfdfa0271e6218a4391f406ebf9324237cdc37c9888176f6b6580706806","0xc6ce66c69820bbf5a0829219bfe34645d0a29c94b72ce0ddd12a1a9d127def86","0x0639ff33278bda997c3df6b536988f7e8f0befd595b282e1bb3919cf6b212789","0x4fe9f41d88219ed0492321880e9372869a30689071e8d8666589e05cde86d5c5","0xe286559f6aa29db82431d2678eb11b88c71899cf1e9d5663d0a03be045b54056","0x748c7cca1d09619ae11aff4a37db819f1887f8dd7a63d30fd982d67752aecfa9","0x557f739b5d9235effdcd4459146c43f616f3919167243a193b77880fa12696d2","0xc0e5486104420a9bfcdf197590840cac65d5dc46a8f1ca32b0bb3cf6c6853df2","0xfbb7bef0fd41e929cb4426a62cba8458e73d34daa4cc0a008dafd90b71406b85","0x48bb6305c8d02afc0f3b7c86461923dc25ac4c315eeb7696a61d67a3fd6831ea","0x6081a9a17cd5d0f713824fb93e586f9113eb0832cb157a3f1a9eb7353ebaa22e","0xebddea9afb47a775be431bc7cc8a1cc10199aadf4a3b41f5228bd8974153d95f","0x9884cbd0d082d925086a5167a757c020174ec382a937979879e193b5d99f79fc","0x359e86af5e856c4227d6f38dc4ea7932baf2afbcb565356b60a135fcd256bc48","0x390dce9ad859d29689b942d395a73988875269571e3559eb2e979aed3838aca9","0xa8774d07ef61e1bf82d7aedfeca92322776863226d116a3105eaf35268ea9a56","0xdd5f44ced54effa4f13eb72ec93bb6fcd8e0a44be4a7b17aa7763049ad35bde6","0x1120f3d004b9e2835cce10df6a9882c43a5213991b5557514f3af723fc47fad0","0x183b55bc0cd0d5031ac8cea51b027de41991b05d0d9484748f9f3079cb09ea37","0xe4c484f8f6e41ccf928ebe153f62afe4e27d877b5e56f9804c8645217029307f","0xfdd63e8d3a681474635fa93c7f5ff33530e3b3860765f99d63a0b54e5ae0fd14","0x2291ce0530023a8eae09147c61ada40b9a4410458f16e3339e5de2e4d66e6e0e","0x51ce82df27efd6256404236f9aa99c2e781cdf74d23db2489fae686548c0d62b","0x52a2f952cbe01c366c0d7e1f21ebe0ab6b172f8d9e4c8a25981cca89ba122a87","0xfbf75fe68b077362976649b7aa52a171005b97e23420d8d0d777d6df4cb80ccc","0x8679d1419a2f30021e0a308900d1530eae171d43c21c735b7b2a0be21ad28549","0x7f13751a4d4c312190d94fea3d7654289eb2b5d73de10323e7692b232ba9c044","0xa20ff1dc98af33f5a6c378b5b313fa720cc0e95edb9705d3b6c0dff283631465","0x0197cf420bc358afb371c76fb29e6560348a5d35643f99412488387367db754d","0xe086fb3ba025d28f8abe0c4b3405f9a1942b9aab12ea24336c3fea7cc43a5292","0x6731f30860a8a0cfb80d7ae1b3bf652823094ecbbea6b6084b8b7520442e21b5","0x49f4200fab15404bae085f8db40f0ffa04333a8447184ff265df13f136e188d6","0x9c69f0b94bf602e5b6f5ad10e66f233ee143b40aaa66673482cc6676e07235c4","0x4fdb5597e76282871a2bfdbbe654868a9defe4ec843e73373f984aae3fe9026a","0x91fd503aeefba14108ccf841f2a9cb9522b4103ce92d13e7ad58efd90c416134","0x6708188d8472cdd12d636702d72fffd44eb50ed3f754d340bb5bc959170907ae","0x562ed0f7edb75821cf99928cbe806581af5897d2a15be58ea9b1da70bc327db0","0xfe810c29a3873b127a086d49fe639a0dcb6bd46e0458e2248549c1a3cf5fd6f7","0x8cb217a99de4ae66e2e72b031e07b8ee86636127f1bc62a101cef558e3eb816a","0xd141d95d69ce0f6ad25c7239f2383e8dd60be0a53bc6a8c262578b23151c5381","0x7e2cef8c6a5ea99830b1b9c07938e8e231b197d81fa8eead656d895c79995476","0x798e80f886d50e811e7dcb4e2196763ff9cbdfbe0fad4d4b199a5b5b4256fab6","0x8a11036badd072da31397fcd3b107e1f0a71d6b6a3182dd4a2986b20381a6155","0xfe1b22aec201de27410e80114407169800ff4a844d1fa1336510fc771ba93679","0x73af0345951a65386da8e0ccdd8888234f76528a29b4f3ada88c501f1f073efc","0xabfb26b80b1d2db7bc7544f4b9ffcec5ea05297217371ada3ba70b6dd0c4a744","0xf54bb3862cacce23933fd428affab95c5311cd4bf5aee9ba120f56f49f47dad6","0x03b4a2081347b97067fce00eb787dc4537b633160b5f58516b7f3bb078175b8a","0x11aa0b9d1e8f194a047c9a5f0f66f41de11360a3bd9a7dae8b2d5519aa901d76","0x18fd0b3518595c28a768ba9263c36d1805eff17151bc1913536d4f139d0a34b6","0x8a63beb2f858032bc73b03de2068d78ec1f8ae8f8523ec3504e9ec216a103bbb","0x92346646dba133158a18a3d9abb12c46d837d1554eb86a3df97ee58f5b338ab6","0xbaec9f1faadb96df50536ddb6d0337092dd6a3b4b5547b1e41aec3c6e6f625c3","0x323535e3c740aa1c3a122c20588e9286e0157f4a00c0763c09016071dd29342a","0x53467fd322a829f0b766a931cb94a35ec1082a05759171362a20cbefe5d89ca3","0x8e9980798aff15401a4a85cb455c9ceefea6cc6459c08db11720633bfb467045","0x3307d7a45f15e6908c3035724bd8923c49b9420b368d65767db24132bb6c2dd0","0x24681c6ceb9b26c71480554c11ddefe6fb10455a3416214f9fe0b466c8bb3abf","0xaa0613a6923017bcb611f35ce60749ffdc561fb3dd91dcc1627267b3615e6b78","0x513d3fc97620b2ff1b85aec36b4291ae9f3909aa000a92a1351106522e224655","0xd597579eed784a94ff55869bf668ba456a8f2dc489851f6b302857b4f8e283bd","0x218af0d66b2bf47eab0c04dddd71eecf637026fc6a2ffff44c70da29a5d6aafa","0x069b59f50e44ca7bf0b914d70ee5ad45de3f4c9dc0154e76d46c0df3d9a80c55","0x210480c7ac76c1f827b9a965fd84fbb4d729fc2e6c58bdf5ff8b4a30e22b72d3","0xe7333f637c6e33e52d66cd7446056f2286f723edb82ace757cd55de2b33b890e","0xace113d7cc832ed95e9cb986a6d45711c25cb20af12ab94199b8ac89c5878e26","0xe4cd231971d7b00a47e0ccb35ee8819c91fbb8a03426d606209089a96b854b77","0xc61b5e5ca3835cc56a6e188c27b6151da0a214c9a5237bf097f0f2b7486354d7","0xce5a89e26aa2e7b48661ef8c68bef5019e806613cf92aba72ce557d063ee27bb","0xa1f168b2bd7d039b46969094f79e8e69b6e7e6d168bc16dfd6c2e1983cafee19","0xa2de84a153219759cd4562ba97c897b0d602952d914fd0bd9c5120a4ec529cbf","0x706ff84b494b3c9633fcab6b110bc0df8ff608da3b1a7cd9dbc7e422b099c56a","0x9f6bb0a48d94c17122b4ece89baef939665a77ddee61febd30fa90078ccda338","0xf2c99ad44479896277e798cd70d647f9bcbda81ae20e2fecb5e8ffbde5acd7e0","0x5b4789f8f8fff2a541e75fdbcd3016eb4ccf223f472450e5c89da511dc05ce94","0xf2440953f5c1def6e3649cf200b94b3d12e6adc41e88fc02f7afc788268d2239","0x0e69da3c294882a31bf158d0bea8668cfceb63cecd068214e50b45e811bc13b6","0x93219c90abfc87f795ae23e9a55eed7ac540446733eb8fd7aaf9afac00b813ae","0x0be552ea31a0fe1acb1c852b666f208d9dd79778d5f5cf1a423ce47acb87d8a8","0xcafeaf706983180e2ecc1bffb7a19fcd28f775426cff49159a73654538cb2ecf","0xc9ba555f7c2baebf8590983e9d879cf60679993a8a92c1425eb438dc1d5dc806","0x38518b6ce4e077cad2e6c4ebb6f387cb6f8f10d8e2db7a465556af7734f669b8","0xc770bee0043bb2b8a7b6b459ffdd8d3113e89fd49ac0d8b956010f93c37b8fbd","0x9d641687aba408f40cc06f0364335415dd4bc5e72ca7d23331f13dd53b41798f","0x9db112cc009ace474b3d7073a2ecce02f422b1c79e8472ef2465bd3e7d25d19e","0xeda40a607f4f8b84f846bf80f2198af0cd0acbbf0a7830df5c6462b44ebfcbf4","0x4518ad71e81d44735b5dd58299ab84d1085d0f9d67261b541d859d1cedf66e6b","0xebbefd65e5d7ccd9c60b67986cf6096d674171c225cc414fda7208cb2498813d","0x0f31c676d8044f83306dc3344a32aa1fcd2bbd08af261fc94029373a76908f01","0xc931f8d23b051865d87e606e00ac554bde89c65b2b157e41d77923707af1f0c0","0x3290e4d6833d1da7f6bd61dd1b227e0b474a361255daa14afb60eae07db074ea","0xaf9d2b15f975a0390ac47849330ae68eb41f45a93fdeaef09fb640784ef104e6","0xf9dc597d83235b7699de8aa356c303d36d1469e905b0068a5bfd8b941754799f","0xec79f122c337edb573640c85c8cc769e29d68ff1d5213b72cf678a93ca6fb1ac","0x2b69b7ffc97ec3fc303d4ad8fe5325e60a9bc02f9fb8f6caa31b38435546a6bb","0xccd3f46133585516bbd89385db74f921572bd646d6e01dbe31392981b1134215","0x3c1b1ae27bff0ad825b54a36835f843efe847be7fd91914073a2394cb54e5336","0xeef5b112a48331f20cf0b1474cc57c31d3af6f76438481919436a2b06374fbfc","0x20b067e5b9dd34d6d2525a1e8cfc5ab20792a37a545b083b277e1b52fdbe58e5","0x1b2335091773a75c90392d82c365044747d2c28289f2216f00fa6d0dc6e16ce4","0x25b01e9533cdf147adf2bef475353022a4ca593491e4f7903ea4ea160f399443","0xb93ec82c73a5423e7a913da52fc98d613a782c74313cc48329189568d4ac0168","0x120229cddd3235a3ea63c751c17ed00602a223e56fb7227a397b70c58fd320e9","0x70dcddb78a2628755e21741f3e0845b912dd01aeacad4284e7a53cb0e8fd98ca","0x578d02154ed4111535bb9db13141edd7103c7153c877826a6283d4b1e4b9af87","0xc4e9821d7122e14c136c5fb66fb128b66bdf76956a04b30cc8ee0463395ed44c","0x4ba42988765cc3f5a62a25ff24c3f4b665bdf0fc2dee8cee84d094f723cc5dc3","0xecbe490c9bc523b2ec932c897a4a0164e6a9aa62778e066149e019519313cc89","0x62aa87ae45609eb59cdd2dbf35199bbdb1e6a5820f0eb1e5072e3eb0b85a53b4","0x07bb68d796b4976099f338d1a9394eb408373a1528d00adce6284336241202cd","0x77c5b18d9d2cc54602202b3d59839ab931da787ac134eb38224c3cf7e19f217c","0x7714593b100883dab9017915be71f59fade1a6c2e6897e938c5a12e1f35f399d","0xb18d395213540cf9c29ad007d2d945bba44e32282ef6df8fb83e7cb5222d74af","0x65edebf13998388aa2016d26f2fd281852550cb16d4d268adaa8e446bdabffd8","0xb8cffc8e673ac05b7c7c50cdf0757fefd90713bc6307f8ceb7ab70502b008cff","0x5e414298245603b35e9d99fb122abc11bdf3e3fe4f6c4a88f8a2600eab5567fb","0x6f7c763530755e7676188ad009dd69e4e7c285e5b2e90a674c644ca8dc3ce2c9","0x4b180944e03ba48295b3e91b8a947201aedc14c3f047ed940d16e33093b1666a","0x69368bed0007caa8d27fe3cc7ae4676cf7100c728a6b77c43794846ccf7ffde1","0x49b78cae1550d56ce3bfc7fdaa131ba6e66675c9fa1fd652aaa70278761f0e2b","0xa6bfb57d6ff76b118e7fd4632bbf0b4b95b88b660c51177ac6b1e7d2db3d8f47","0x4f7cdf18e13ef7823a490673887c49e31cc52986b06d6427cdfab04b195c3517","0xeafe80769078529e647b06a38313955ea8345dc051a045e009bfeb1ddbe5b921","0xe18a94a59d3773e13fb23bb14252b51232ffe0ebda1b10d7ce9eb4d9e33324da","0x169f2fff388130599a9f0c9ba8f70c1b343ba95a5c305be62a0b1e7313b82456","0x42bda55be0fb308679110cb6b171fbb584258f8c53c88f294f417c9afd85d623","0xfd02bf564a7503b252bbd98766aba7d4b014c8b9dbee4931740010576c840ffc","0xe1a7ebfce659dc522fcb211982f6b16185419e70f0743bb1d5211ec2e1996203","0xcf7d9a5b70bc9ccfeb66eaebc24d3c12c0cc2b7bd37b5ce6efbe7271ae60e984","0xca8163546a516aa98b3fa80399477939c769683001446153eabe0574413ef129","0x9a9ba98e594fd1dfe41d42161509ea87dc277a51e7b79438bbaa2cee30e73d5a","0x777091f5d0ff74e8ba771ebcaa5f4349a0db5e43a8488c82ab2d01ddcdffad07","0x9cb7e949be41dc5383cbaf41d721300838e670f6de62710b3757d0e41af356c2","0x3f037c02502ec127c0635bade163da6f643f8bda4c5a82c07cb81043be5cccce","0x6d5f1fd359e9a5ea757f34daf46c54ed152b2cb75baa9083e78fe96f25250a97","0x0c7fb2ad6799aedb4a6b2634c51cf5332c42810d7c697da3bf701dda5d8b01dc","0xfcb7ddcbd71bc67749dc8541ebc57f4acb95b7e2a14d4ec3d5f106e55ed2f349","0x41593a7cfd72eea19c70ac76832ee5e788232946354b772fa8e643e5ed4f06ca","0xa4b2a78fa76d7f1200a2b1d43ea0374d525f2f490d5fbea5397d9e6d53455fc3","0xb4ffacf3d27632e031646636b169d4b658b0e0af9b4777490c5d828a24f75d8a","0xb57426ad7d62d06c17c6abd82f9a19882b51ba863d761edf4313fab4b06ae042","0x420c3a27b989a862086a4faa88e939d8ce3c2994fd63a0601a62c12f3b9df07e","0x1bd66d57e33dc409eeab6e38e7c0e994148aca83184e6aeec0791c041b6c711c","0x6781a6659dfa11bce6c701e6df81d625f2c978db8e1859690b8e91f4d43ab5f4","0x036e13a9d878a0065fce60724c895792973e38f568aad0a744d868cfa1d2748e","0x3ec4f3846c5ebcf744f3077a70b91946f9a3fa9a3bade048d26b2687308d935f","0xae4e9fa4a1115104ef0d87d09d0d823cacc8db02de86bd1cc0ada59ffdcc1af9","0x10d72c60c37fe15cd89e793a22c804a559f3cff5a0feda152f4c95b69327bd53","0xa683f59692c77d781d92f9fda74ea69050c46824e7e8ac8c801e1ea2bf7a97a1","0x517bc61b5d3ac0c6deef0e198bec9498befc55313cdc051830aca1e6238ff98f","0x993a81b0eddef92a1ae9d4322f61040e415fa006f56bb9e04506dfbb041dc9b6","0xc77419268b38e936ab0c29cb8a4aa3412142a83bc6fe5494c1ae88d41e80392d","0xf36887cdabbeb9eb79f913d0721ebc98eb33672a0e8ce887beaed88d9c364e8e","0xbafadbb7e127aa147c59805b1f11808e610646eb26d10268b984f1a3ffc9f2d6","0xbdc871c89b9d10ab9c21bc6e43ef98abddaf83dfe030dbadea19c7b243969147","0x87b7e28708ae415b44ed9702b373c20ef40c8184213849a424b48798f9e8a074","0x2a5f1c8493c7b0f09e4e95f62a5f87a8cec9ab8aaf4fbc78957039e5cd7f4993","0xe9e9d06c0c82a645d285c2c113d923f226999dd28bdd41f18fdab93e0f23aba6","0x67d2f79f0b3e75c41da722caed1adbe07e3b65592d573f178481f6eef73556ba","0x903a50b0da11943ecf7b17c079deca6e9c9520d733fd32e0c350c8150912cad3","0x14a217445eddb9922bd1b0986710873b284e089e73bdd6cdcd9af86160232b20","0x478af3246db58fed099fd2227fcaade4c09e1cb810c8009283c15d259009ac03","0x9d2383446eafccd1bb5ba462e5435c89dda54808124983b87071a45f671b9277","0xb5374527abedd9f1ba6c8bf1d70176eee71c9e260d314545615a2d87d4b7cfde","0x98f898ac2a13944d1216ded16b64e821ff97224e168fba9df75d4f521f7221bd","0x622606a9684f550d852709007e0098eb9b0e9d0698832e1260b44497866add60","0x16b82f1ece3c90b4bb9947ffd2bd98e58bef9dc7071cfd0e40663480ecf0d5ba","0xc3fa31778b001495010d2c24bacabc562099e9bbb2c79940a3ad47333fb11caf","0xb5920b18e4944310228567f5ddba53832a71c7ce15b6b0df6f241259be146d93","0x718c9ded8660e4314d839f48a19afe25eea6413ab262933e1173fc17e5526a15","0xae2ee03963f4463ca7688718fe415f7040c73ca04be74d5e7b78c2b6c5584ca9","0x6d7b1f7d978abe8d4f6b748a74c22f96dc13be9dc35ecf26e38964c0ee852660","0x99f52014f25837c278056a2357a78b71e9cb96c09edea61ddb64b40a85679d2a","0xb58e508e38c7e08cff30f97fe7143f2b4a808ec3aff00002d34e83a67730408d","0x54c7062676072be271f095749c39595e50b25b9cab71dede11d24ae40c288d4b","0x178b9b83b46574e8309fba0aadefdbb4e32ca4995525b1c952805ca28187d645","0xde56698c6123ca87657e58ea9c1bb17fd7b7c09cd6533ba7bd55e0a72be2c627","0x7951ec6bfa64660d989d3767f3f6cb803bfadf4eb6cbff30f28009d4817e5eb1","0xdfdf1b9f2b4302096547fa5e5e3df0ce68e0230881d364d1c56eedd1dab426ed","0x4deb91a1d7dbcbc72db96a5224e11cb85791f382e69153b8b0c0203f662f6310","0x71a8582a5e689feb05531ef09c355e33dcf0a8c5d4e619061a28330d2fe2c01e","0x1a1f143bce6ec64efb5d3a3551c1b0ebd530358be3c63a9ac2f6eb3d3ac73c34","0xcd0db5259e965d0daea128b82192241abf62c0e749ef6a0a43590c7435fc4188","0xd2bca71df2b2613cc65837a8ab091f3f769a4f41bb853877fd2e9e1e29b1ce8d","0x5e9a68f0df25c19548cdd514166137a12a6e5c32019500beafc8c1d10a9768af","0xaf2ea0c8255fd0570a4db50350b6b71a5f7392fb449ecb7bceac784d91233744","0x8dae3c3a789180a354da89627188da77ea0267069d315954f9e5dda8051bd78b","0xc2be5a3430ebab706373bd7c231e34aafcfb6f9864513fb02d014ee7dbc059ef","0x65de4b7456d6fb9890de195ae53a4778cef67d1465a2e509385cfeaf68d8a984","0x1dbb8cbb9b87e01ba3b45c5d7b36574fc4ca8e2f5839216786dd390529807594","0x78ccb4fb63eae9097d6a6d74de4088e696e3056408f6c5b8be8294c5cf8dc58f","0xb81b4c7c82a896d044dd3067b59c7823456c72828e1c8e6c27f336de0e2f7edb","0x0de4b27e302ec25cd1c2df050e7960203ea7ee4c6425b449e3e85e8b00d8abd0","0x6ce100df80c3adce9a2a76382f61d6e90ffe2f0fcede51e86400da9869fba993","0x9106ca60d6c34662245f4023d85efe5ca9434f2b82983e5fdb8484460414e6d5","0x32da36830cff15b0a6150fab54bc66733862e97aeef6ebc07e8b93f5235096cb","0x34a5e2ce9c12e4b828475a89091809ba387de4318d69488c2e7d9e8a905dca18","0xa0d7afca2f8b8ef7242fb17864e2b147de321b3f6942011cc408aebe46670707","0x2b553fed6ba2641ab459755bb91e39165197410b4e2250ce7867aab20ef9a94c","0xffca7c74f717641261d82ddfb02313fa7d8c1f117175fd159dae62f4570f69e5","0x77137b8576c1b223c87061f34a018c50add2850130cff7c2752bcc1712c6d2b2","0xcc608c7db936fa916f498fbda7f6c4ef75ce3f5e188f4db5a4a79019cbf5fce9","0x3171bda80fadc323f6a7cb354f053de99e40f644f33e5a5779236f3db7280e7b","0xa16b5e84289f3bc23c4e3751a27947a452a6e063e938d68753800aa1f4402744","0x4bf51122fdce1c482e0a2908de32e0c0754bf67507271a21d4c5cf4aa1a81b8b","0xc95202d78abc46f55236ca64737189d11f7890a9e3d05dd61a0df267268f5b1e","0x1e164933dc04b1bb6a0df8a48e2b99e313d3ec68d8de20e58e43e9b660536d4a","0x767e0102d3babae58d10a5566c6ad6ecd391c2c1b6b5d388494c36b6ee077b49","0x3644093cb22fa24ca180101eb1a3038fd454dda21b2c05bfd1f1a2170e87b5f3","0xb652e1746c5523a452bfdc9bfec67fa243171cd53b774149cd2de35c90d43222","0xb7e5dd0a8e238ea3089f96097ddba20593cb32265033451ed20c54c2243ad4f0","0xab63aee40987c75f379705a2a012eefb1f1a3518e09cc8d62e0fb2d4e5183485","0x3b40de17acdf364e8a5dd598a0e30c63bcf460d85c96897f7893f25270abead8","0xe578cd7c9399f286a7cc31f7e6867f7d1d2bbe3f28e5a35d7d16ad352abf71be","0x8504de226e9fe1dfbc4432c01840ed53ed7bbcbbf471b7baebd97ef27109cd5b","0xf95f1b74f5109be19a0e3e544038736036ec4225e97e9374d670ae0b35cd477c","0x2e3fb6ce8eb667d0a4071214821459f9bb3cf73efb8c3f4b13de21c576faf3b5","0x8efbad8e7894f6fd67af1ed3f74fb0a0976bfd5b614834e89f28293f470c92a1","0xe372381de32af1ef95381f243053d65cc06f2fe3ba8cb39c0d5f23321331867f","0xa20cb651065d16f9b7c743bacaf373026c6a2e1afc64b7e81774818660650b7c","0x59d44768ad257df0d846b52546effbd47c58296f9071cfb654460022e036aca9","0x25c3833f307e46b034597ff87af75f3c41648770bad06f0a05b566572834aa53","0x044b220fe6fa27b7649ec3eca35c935651f199a70df739a57d0750a4dbeaadc8","0x004b79fe3c016781a09113e7d008c3e80f0551051d392a14dfb4417f210257a1","0x3f672b406a18d073aba4a13c589022d58a2f00006f3cda345379b45a7de3dfb2","0xc1a3bc1847721db80f566e9e171b75595fce51a08110e75618240f6cca1b15e4","0x30d4b468c3a05cf59fae7070b319e4c61593992d3898e6167fe30117d4546822","0x8a4979e91bb4c9abbb52e7504af1751133d57992a80f808f341e68cc9bd8378a","0xaccfec5bd1c83478d2c53a47b7b7d5cbaba56e8fe33465baf0de76f1ee9130b0","0xc6d8989eca7f7a096c58326a193fa4ecc58f68eed069c01be3f62bf904d9a30e","0x0b898539479a8a6407d2bde0b79d3ebbac9eb596d9f5f9249ff8fcf5ad469bb4","0x27a67045675f2e774d57d47c1d9f6af61f16c0d3ba1efa30be2e2463a72b48b3","0xd225221d0347d7c5f790612f0ad02d7b3e567622816d680cc0e566cd25899ffd","0x8c399c94a16cc316b341a62da58c429585239e6369aa7a79bd2c71d6ad235d3f","0xe061a9b2c4a0cfa5d994594c11a99eb0d4860ffc73ac446ccea263eaf4a3586d","0xe2f7751cb22b1f18e0ad8cbe1c2a259b711138e8b3090fe20438c5a9da83e656","0x3d69894ad19926b8687fbf4a4c7a0b74eb442edd78eb6f56b0b25cdbfcfbf2f0","0x60f591be3a84483975025948f1a8a9d15af1203855744ff5a98fa323e1da7773","0xdd06f01162145bcd8f366b76ea00527b9b74bf305ce0e7395b9e67172fcf0b29","0x5af1e0334011082c2afcdd0502fd31788eef8933faa6a561c3d8c6a6d9989347","0x8a22cb3a65e023ea35c574b64192f999a2ef097fb14046c75c9d17414dba675d","0x9b37080c575ddb94dcbd0261b526034fbadcef5720abc8da1f64a781141730b6","0xd12d53f29e8fb68d480f62a43b20ab5fad57b39082c3a821c6617824024dac5d","0x80d308624b37c2a6332465d78e846ef2e940dc8ef29a1021527c13260bd8be94","0x700252da820d60e5af1c53861734061cd4405c512ad3eb5e4b5bb95303725107","0xf857926e23202eb5a24d75a33512a2760fd31865d40dc0b8d5fbdfcb20901143","0x0c4b733ced21a5066f3eadcb9fd9c0ff389f59b3f993314ef3c0c7d0c60ed5c5","0x2c02d6b9831345f4ee4b4ec33627f2cf64d1ba60b9c9bf0b639d4def795cd3c8","0x50f390c4ac77fd30271a07c2219e2b2ec4724013dcee26762271343ddfc993ae","0x61ba277c1fed7fcba19c49d839ae062ad359d4f785da09829d7dd35018f72b36","0xf79cfb1dc76e14d8f431afa728f76440ede6b0ac57119531c2ff28a853024a01","0x728a0beaab4f2296359173a162cf57343e2601b6c5af3e93b390f1aff3abfb18","0x47bc447627edfd4509e6ef5b4a2e042c041e4477d3f3ea207dbbb27be5aa5640","0xeab2984e02691ec7e239e1aa6c092b379f8d5f685f2b777fdf1536de3077b163","0xec52b7b439d0b67c96f235115e4067b44ca24b4b73ccfa907da4bc3c8376b251","0x776dc55bbe77200c3355014beb2703fc8c569c1ed56db283242aba2ffa40fac2","0xb0af613d0b6ebb64322ec1c95ba96c4ccc7d4b78a0c611be5b136d4a70f62a5a","0x64c7636b5db0730d926ad1555da2e4e6c0377d82222c1878ecb3b8e3de2ccc08","0xdb4e1600890dd9725206e393efed5441d5ca0009ac13119158eabd7cf51129d2","0x581ce0e83eb9f22d44055ac81e92284085fe5527e916ae192f87f68f1d2ff4a1","0x2a1426db955739524d789e35b2428956c2234d8fefde63265fa473a89a22f294","0xedbd586122cd1478f7f264670e32c887c81d3c19ebbab9549e9fdf00f332aa50","0x9288628d6dc436b9e81a9d9ad8ea6c903d14d872714aa5c9aca30425501cc67e","0x0d61bf3aad1a74c9844c365cf141e30ab73dc8c4a1b9c063ce166a1b4bb9e455","0xe8327f5da3b93b9468c6c6357c32e52947745eed5de4be0f2d6b99c0138d7b35","0xcf36ec11b056ba7eadcbe4941639af93b2fec92b424683a4fb6078d474c082db","0x2cc7b558f2da27e17bd080a589d9577a90eab11f1ff7516993618c5498a5efb8","0x2415700ce54b629b776d23d77fc82abb66a7361ee2f054bf5058a5823366aafa","0x9741c675787fce10f13e45bedd7244854ba1f663dddf1e0d80ec6a8db7c9498a","0x9765b1c8622b4fc261af4cbb00fad45cb3cb29d487a6a305c2f77953a18171f8","0x8dd13c77bf9f8cb0f65cf12e4fdf60d389b0dab27bf79fc6ecdb306f54aadcbb","0x797858c6ae208a3c52a12f74ecdb7628e943a0581a9fbd6ea568bd8d2a34eec8","0x055a6b61b60773a23e97bc84bc5cdf8a350d420f02f1b190f4313ed0ec94d022","0xd960034968be5e3e987d70de83295ce7761ea6bdd96bed1ab69841e4894ae5ff","0x994ed219f83bc024a892edbe8099e79a34e4b40315b52b4208285ad51197ccfb","0xd7f99880882dcabb3bee38b9143d00537d2d1a071ec248e551e39886a0a85ed6","0x08b86316028144e8c02f40a6bbed4805d92d52384c59b92a3dbf5bfc90f69ee1","0x85b70ef70764c4d98d8509e235a4409b3f7db11a01ec63c54fe627941215f44b","0x6ffdf64e8a90830a7a80d784b9e1f555e1cf4f155847a0b456ee72bc0feecf76","0xefebf9020adc38432eb4c50cf77d00e809ab55e3c8f17214c6aa16eb197e3b57","0xd34b0000fd584b92bdc3e3fb034da8226a9a0acec72a6f60582beb00731a9b01","0xaaa7c7b3e8853c897de214e9852ba6d77635cd2cfc3a042571f06cc355087ed9","0xe3db33d276dab753a0656133d65f3c1a2a8556ed1e641b1a437624c72833d5eb","0xec8ee9a53bba0f72b3138c655fef9c22026cefddb7f6b45ee7309d3dd34e7627","0xae554517e1cc16781e44b0efdd47905bd225aab12ae7f78795cdf3b88f097a71","0xaef4c646fc2c408fe9f90eb709f5156fe595520a2f8e7805c8238d48b7b5ff57","0x1c0e9e80b8c936db2ee0c6ae7777acb2d938cba2f67eec9ca6251a38b3eb42c8","0x9e9b0519e264d5e08f891e9b9d15a3fbcaa50ce85916c6389111cc817f5781af","0xfafe1df9c98d04ef49cf0f5f81a4b67f91ba6ce701a7cac4b861a026f4440fe9","0xb85d2cd0ae01976ce7b07a1ee476af4255969973240f9c36a727fb34cdd11961","0x12d58f461c53b07dc4c36ce8fdff4d97d4cffabd2238c48174d666d9d523d9e8","0x23d6164d313d74bb53a28584fd873c4a5c98c31e1760c6a1ce537e52ba825164","0x3f620b2bc3bba93e2298e9fc2253543fec68918a1de4e568c85662bb53f2487f","0x6a8431ca31edae62c557628dfe4230cfdf6dff86ddacc13fe80ffb17f16769cd","0x23eac6caa8f0ebbf12112b0c451cb6051fda871ddf09976cc2e9e91e8639fd10","0x309362be0e137302120aeb2f4889fcfda0ca724cc019c4fbda615c83b352186d","0x1fabacc3949baeced8566b8f9ae8972b31851f68933e70945b6e4ce57b9477d8","0xfa0bbb1a2f6101ef1a888a8734e81fbc1cb42dc23c28dd7f94ffc9bf632db671","0x842eea46dc0fc1f2498e9f222dc77352417e03ac1b061d1685b291472238f0b5","0x8511be3b5e98140c03555913ba10c757b1feae0c854de22bd21dd39294c1c212","0xfc3c6a5d65c7b040cccd4c349224579f9524653133ee7c8374b43092554f81b5","0x3dc15857dba022f971e0dd39affc97be957ddfd7367724aa9e994a37ad71d4b3","0x181768981e14338b78238ec69b31a17c52c46e9722a73563e4fdacabb2b088af","0x1597cda74aeca0c4dcc820856f8228c860aac4c5e5fe300e9038f586c015ca48","0xdd0e6bef876e802500716afdf24fc6c710d0f51da4464d6b7cb6c7a2dd42376e","0xf1b9a04154aa56547176c692673b5e4c4d584de40fceb006a8e583dbd89d6785","0x094a41829784e1f2886a59a97e47209fc1d0e85855a33f1671488e4950beccfa","0x263e2c388f43745f8b032505c74cb5919d4c9f9c95fee5ce2a260dff3d143f54","0x9699f57553845dd23d5065b65acb566b18451a1dc7cec10acb67dba6e9a2b1c0","0xb8b578027767569ec111e029f44175d147cf0061bedeaca373e810d78c3392de","0x97823aa82f589ead22cec81f6acae1b463f5419427142636179284b3e5471b6f","0x4bbe50017f697ebd340d366f9768746d97d5440db086d74b0218f116ea9847aa","0x51484b733a5a8f67630474333c0dcda04109996dd758b689e727c6b7426253b3","0x1a06e84e14a5c969506365e96cf014b1c24b50998ef5cd2eb8885b26ddb5fb7f","0xbda41788ea59873a71b8e649cfe69bf7ee3e06f277bc912f6b08569dfa072c3c","0xce520f00c95b6e30fcaf4204f094c520e690a066037106a302b312dcf3ff380b","0x45b92304316aabc108975884b100c6454dd986e31ed0b672ea3445ee0fbfd0d8","0xd0c18d0edd2d717f7772d2564c1847c7db15d38159570752edc7f7a304b1e55c","0xe760b03b017d7ed9ac991ca16c50bb2e2745afcd2bafbbb94230439548616c9a","0x08d58821e97ebc827697c89795b31408137a6923072f1086f9cdad9d907bdeed","0xd81016c9662b8c1cdd22ff6f851c2b9dac33365f4a1736d1e97ceae961008814","0x7fc0bb233d27459b107422e0bc3551245505eb1894a6072669346b285831dd50","0x659427dedea2b8dab5daee37c3d6ebf30ca7f4771e1bd1f26b1c6b526613fd45","0x8f9660c4def55eef33e57dc2623fc7dd050a0b3521bb01221871fc0a51d96c37","0xf0c2320a5ca5bc4d6764859a9eebd640d0a655e9bb7fff6c41a5d9bb40e56efa","0xcacaa929f8bd0eac3445f82f07f5ffbbe85ca04eb5a69eb5d40984cb8e5133d5","0xbf94ec939ec7b1cf6f28703a65bd956174889f40b4d7bb697b2645c93d11a73e","0xce9c4bba488cc29ba41bda06968cc2eba4aae9a664bf8536178bbdbe6bef4fb5","0xf4b75a9004f2eb15169f8f51ee43857b35ef5f9ffd85c08ece0250fcdd930577","0xae9a2e0f0757843c8a36efa1132285ede75cb4207f278b4bce34c991af400295","0x826cc893ade8987c88bf2d2b529afbc70f5c688337cc7fc5d22bda57bb02a69f"],"withdrawals":[{"index":"0x6f25f4b","validatorIndex":"0x1e4e48","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102d071"},{"index":"0x6f25f4c","validatorIndex":"0x1e4e49","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x3d3fe77"},{"index":"0x6f25f4d","validatorIndex":"0x1e4e4a","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x101f933"},{"index":"0x6f25f4e","validatorIndex":"0x1e4e4b","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102d0f3"},{"index":"0x6f25f4f","validatorIndex":"0x1e4e4c","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x3b76458"},{"index":"0x6f25f50","validatorIndex":"0x1e4e4d","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102433b"},{"index":"0x6f25f51","validatorIndex":"0x1e4e4e","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1029321"},{"index":"0x6f25f52","validatorIndex":"0x1e4e4f","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1025fdb"},{"index":"0x6f25f53","validatorIndex":"0x1e4e50","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102cff0"},{"index":"0x6f25f54","validatorIndex":"0x1e4e51","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1027851"},{"index":"0x6f25f55","validatorIndex":"0x1e4e52","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x101f7b4"},{"index":"0x6f25f56","validatorIndex":"0x1e4e53","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x10270c0"},{"index":"0x6f25f57","validatorIndex":"0x1e4e54","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1023bf9"},{"index":"0x6f25f58","validatorIndex":"0x1e4e55","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028377"},{"index":"0x6f25f59","validatorIndex":"0x1e4e56","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1024322"},{"index":"0x6f25f5a","validatorIndex":"0x1e4e57","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102c5fc"}]}


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
    }
);

// Tests that the manually specified gas limit is used.
forgetest_async!(
    #[ignore = "tempo skip - uses Ethereum archive RPC which lacks Tempo block fields"]
    can_execute_script_command_with_manual_gas_limit,
    |prj, cmd| {
        foundry_test_utils::util::initialize(prj.root());
        let deploy_script = prj.add_source(
            "Foo",
            r#"
import "forge-std/Script.sol";

contract GasWaster {
    function wasteGas(uint256 minGas) public {
        require(gasleft() >= minGas, "Gas left needs to be higher");
    }
}
contract DeployScript is Script {
    function run() external {
        vm.startBroadcast();
        GasWaster gasWaster = new GasWaster();
        gasWaster.wasteGas{gas: 500000}(200000);
    }
}
   "#,
        );

        let deploy_contract = deploy_script.display().to_string() + ":DeployScript";

        let node_config =
            NodeConfig::test().with_eth_rpc_url(Some(rpc::next_http_archive_rpc_url()));
        let (_api, handle) = spawn(node_config).await;
        let private_key =
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string();
        cmd.set_current_dir(prj.root());

        cmd.args([
        "script",
        &deploy_contract,
        "--root",
        prj.root().to_str().unwrap(),
        "--fork-url",
        &handle.http_endpoint(),
        "-vvvvv",
        "--slow",
        "--broadcast",
        "--private-key",
        &private_key,
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful with warnings:
Warning (2018): Function state mutability can be restricted to view
 [FILE]:7:5:
  |
7 |     function wasteGas(uint256 minGas) public {
  |     ^ (Relevant source part starts here and spans across multiple lines).

Traces:
  [..] DeployScript::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [..] → new GasWaster@[..]
    │   └─ ← [Return] 415 bytes of code
    ├─ [..] GasWaster::wasteGas(200000 [2e5])
    │   └─ ← [Stop]
    └─ ← [Stop]


Script ran successfully.

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [..] → new GasWaster@[..]
    └─ ← [Return] 415 bytes of code

  [..] GasWaster::wasteGas(200000 [2e5])
    └─ ← [Stop]


==========================

Chain 1

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================
2026-01-26T13:02:35.868845Z ERROR alloy_provider::blocks: failed to fetch block number=24319172 err=deserialization error: missing field `timestampMillis` at line 1 column 34200
{"hash":"0x7d91cb46dad30faec499a8fda29b7324960bcc22c44605e84707ad091f1308d9","parentHash":"0xdb4a590f3f9551c3452022277c58eb346161372a5ba4d08864b99e43569c9687","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","miner":"0xdadb0d80178819f2319190d340ce9a924f783711","stateRoot":"0xd5e2c6730336ebfaa9144a22632c1a947bf3300b258a953f20c8bb6af4973e01","transactionsRoot":"0x6aeaca2236e534528c481ec9e44495627e8780bfbf88b95264c271958e14828f","receiptsRoot":"0xc61e40c7adff92b0675ca30de878a2e631d75759bfa8e688fc668de4370be339","logsBloom":"0x3bafdb75ef6f3bfb7cf7ff3fdfdd975f973af7bef57f1d7debe3fbfe7ff6adffbd9ff5feabfdfbeb7f9ffff5c98ef7fe7ffffb7efe8abfec6dfcffefbc3f6bd5ffb7bfb678dbf16bfb37ffaaf7e9d9e7ff7fdbd7ff757fad7e2ebfdeda67feceff7f3f4acfbeae7c3e96dffaaf19fffff6fb6ebe2fd39fdea7fb4bf7fbfe742fef7af7d97b9fcdee5d7f3fe393f76f5e3537feef7ff6bbde8dfffcff72bf3f696ff79c7b72dbf7bdfaf1fff2fd77dedf77bf4bffbcfcf7beb6fd7f6ffdfa7fffffaa7ecef9bfbee51ffbffdffe7e7d2b4f771eff7fdc84fd7cab71e7fb7feaefef776f6ff676f37fdfcff7bd7bd9cfd7fffeeffa6f3fcfcf9fdf36fff9bffcad","difficulty":"0x0","number":"0x17314c4","gasLimit":"0x3938700","gasUsed":"0x1da15fb","timestamp":"0x697765c7","extraData":"0x4275696c6465724e6574202842656176657229","mixHash":"0x29d3e3e306be592927326a034ef9b02976b572bb1bd758f05c9fbda5097cd2d6","nonce":"0x0000000000000000","baseFeePerGas":"0x5a4b85e","withdrawalsRoot":"0xe9d1a41a7567b5d1214ede07ef2c3231008f177652c5df5c00504ccd60344f00","blobGasUsed":"0x20000","excessBlobGas":"0xac22310","parentBeaconBlockRoot":"0xeec3e51c4c0f0684e06c658cbff199348e2cb7dc5cf64c7a403bef5d74b0fe80","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","size":"0x25150","uncles":[],"transactions":["0x7da0332438f5a28d7e98b4775fa7463fd666239fb7a359689cb3e55700bb5a07","0x54a6df485f0a743e814145e7b04f44d938d26175e3b8395d2924545c873cea09","0x9be91ddabf92f96a0c9eab6541045302bda556d14fb6d8d1acc2af1f916bbd9f","0x476088de76699f0e92ca90ce462ebe972df2460f0d38630c72bc408504461322","0x5772c848198753eaecea649e53d8d503eaaa85b66409ca30e7709b83f8b65a11","0xac7eca1144e08a773f9de8f2599ef2a5925768d115e9f36a73f3455d8779fb22","0x909f4c34e3f4486f08b61ee7258dd748f2e878fb7ea636b4ee9b9b5226f6f2d3","0xc4206ca84fdac8a6dabc7e3c39ae6da2070a2f92f96baa247aa10a0f696819e4","0x11e9a12b89492e838dbcec86e911118349bcc8fa550da8a26fc64507d41f03c9","0xeb1f8c5e0eef7425e39cc24220d5aaf669d6e0115d594cbd8501dbcaae0960ce","0x5cb9cbba3e5977cb2a486bffa672de11d0193687b1f09a3cb20e957479e744bd","0xef0ac338b7c4cfb5cce7a815a5387b891b70e050c6209fc226a010081be0f98a","0x28c97f9f82cb4d761b2fa1ab285454976c531465dc59085289b7df576beba711","0x86414a5fb5baa6dd76c5b9e995ee621257d637da34d74cdc5d719f72d84f1d54","0x29a98ac4fa1592447c9c003a60df5990067c00a8051f80007893db994cbe2945","0x88a063956fa69c1f1a5f0fa8a9ab3d70bdde58430f8fa4c12d409c876526c9f4","0x13275151a2ba330d6973848dfaa5a99a87a2456e270e354df07187f6a4eaf91e","0x91dce550dc2605f2692e7819fe2384f59bda0e4aa4ebf25f43dc973e9a2075ea","0x87a61eef31121605f42d3c84c4ddba4f6e44a3c3dcb1a43061484d1fb2897fbe","0xf726460adaccb16898123e1dc377c2cb5123d711029a41a067c90340c03a3049","0x97cea4d20cc8a94e71c4bbaaa3d5ff271e285c18d2a65d2ea094f80b4907d551","0xc872fa2375842fee1618c6ab79290c4cdc4d697cc924ac87c6a3dff236fa1326","0x5447973d97bb5b7dfe4d7137eb24d95254bc7b89a0431da2d58a708039e93ab1","0x68e019042edbb4665502c25ec91b690ef69eca52f27765f49298db0733471128","0x832b2cec5cb1e4719269e809d791f950a83de39e460518dc8d13dbe973b4c902","0x7f6b94cbcc6489225e17f02baf4c38be5cf45f2afc164153a877ec4a7227b0bd","0x04d53438d8b753d4d78ddcfdcd86390146dd5fdfde36ca434e3244d7b92ebaf2","0xd3cf4107d41e0d14e346e9947516723340f19cb434f130947c1ace24441d6789","0x1f18a3e7b37e9cc5751ce3d9e7d2809df30a0e283c43cb60a4cafd02b8656bee","0xaa7e53640e7db774c61f05db43496af6c61c70985435066cd8a2cadcd6692f13","0xf67e9861194786ee4da24e5530a77201ede37bada52c366cccfc5caa7eb138dc","0x18d06583aae6916c511f7ff66a5554bff89a1324bc1947528602edbe50715dbb","0x8504598e9fffd49779a7657d30a01584fb9b3e2a9ea56a10c24fb76708e3a92f","0x256b0609ac5162d1769fb2e5834506a944876756af09ccce6bc9a0d8fdb0d345","0x59f652aa843925b1d7299a6980d2ac26cb13f37430f5e7fba6a6d1079bcbc792","0xc684f63cfee3084f0c93d35d226b46193902d1b86e64c58c30bea47cdb28acbc","0x1b4781b474f3e61488558b2cd9cc1a1d5b82b702decf9babe6e8c34cef56ae05","0xefea2adb3c9e45247f7839ddc20dba7990827771f8ca3c679e0e70255c18c41f","0x68cb007166ea5b195c980e42fde91a8eb1daca5e6363a73abf2c26ca74d618e0","0x381fa1b728f2fbafbb116ab3c50c75b4deec82e07b92b20af3bede02108250df","0x427a0eee435c640d3fe32d72a57e475010a6f90328e312eb99ce3b2e6ecaad74","0x267907e5a7c02c6f3d7c78298f643866d73c12ec2ff1b2ddf062eefdd87c0a9b","0x6ef66417f48ab529b3734ddcba893cef8d6be228abafb0d95600550b018d2056","0x99af39be7b21d1758c4d43b2961f4260d61611a3bc0a9ddf9e0452b554dcdc65","0xbd3943ab6070e8e80dcac624b8a701d6b3d6e2ea9b3b625a39e6e027e9b17649","0x2127d4287e45a78b33d288f025061099a0dda8d12dd6605f92021ab390aa0e75","0xafc3bc1fdc0a8d7b45083a0c1014e2771443f0744a43f6dca0f58ca3e440182f","0xa89afae27ee1ae642ad1394064d9434b102a90dc5e91000cfa58bb98467ebee7","0x46fb5eb1c231f2fa790d5c1bec9b79e35ea27c2c944078324719eab569f2881c","0xf83a7d1d352305d420eb9a49abf920500d74cf44f32f7f9ffbf2f5c6b2b19cb0","0x0072136ac94c4ad59f4a7fdaa657305553f642fd5d13ddf466c6572be4439bc3","0x2d36d5fa147d5797c9cbce53afc1bbe659a42b01c16dbcdffbb6632ad6b83af6","0x801bbec7d9112b42073d9aec4aaa28afac64973fab95abe423f7fa9bbd435ff2","0x4418bbd6d1c5f8185fe314d0b0725a4e132cafbfac737d8e469f81d470ec4296","0x876a7bfafb9e677e74424c2a35bb4e063181cc0f45265e18c86ec67e16774ad1","0xd250b5de166c7364b57984cf1bff23a808aca0b6ba53f8beca4338ea35a2889b","0x355a361783f42435ac257cd59a8b0ad3b434501e1c2159d7ae37d97125e0da58","0x6adb8911fa522dab01042f8566feb59ffd9dcc2fea9ecdf0f2c2c6d34e638268","0x18d31233f5c21163f3ca5f79d2fdd01fe92ac9ebe32e01cee947699da0cfd1ab","0x9658af7a0d84181e6c5744148e0d036e05552be1fb8de1983157ef41e1dcdcca","0x9c9e88f22055461346ab73e7fd07fb4959403850c054beeef28db9d25f9f254d","0xf2c11826e8fe78cb07973001e786e46d3a981eecc625fc7df6b804050bf5bc09","0xa0b013d68f670a941208fe8d6cb571531badf1e7e3810fd3ef759eecfb5cab39","0xc9d027f50dfe9941a58ea4f42e305492fd0ac62d2f22b3171f8023a8844eb1c9","0x7a79706e229386e8c36b0ed84959704cb264f4e55610a175bb387acb732efad4","0xe4d612450b6401f4aad90a00488815fd7ae81c8fe305b117333a2f4ec5d59ad3","0x913f8a18c282a76e11db1548e58ce3c59598785e1a8018444262edb4038db516","0xa67d671fdff2ecd4bafdb42698cd315c815cdc95d05bd4f4f82a0c1c017d2010","0x7971c3855248e2a8e93451cb477ced601a03357efbc06ebb7e404aa53e30f4c0","0x10efccd84550d516f567038541b875130d75f599eb85e317ff8cb0856f2aea49","0xe36e9d2173b855a1d2d039d1c6bc74f28d1b895f86fe6e0b5a559ea1596a8eb9","0xfb9f2c1121e0235099de117b9e49a56eb2e6674bb984f7d2d33bda183fb36c36","0x58cc9bc9c49da1645a271ad691cc4ccd0824f8cbd987beb3543aa35b9dc5c875","0x27f71e069b78886be01248b6364fe70623b9d59808723974869db47a24def2a1","0x5b0efc296daf30584ff0f9f9b29ea3dc0474f6747d39943b27a3d6d540c71022","0x44930d392f84137afb9cb5e9fae4c2c29064df6755f24ce8d1aa9a1f15b86348","0x66ecf0f2fa8fb3e6aaafc6cf43e4553262d57016759e6db8e166fc392a7ef23b","0x5350b29621fb18b5d8302905f6a5acda66c9b5c77877c537c4c8c7b0685d3b3a","0x7764fbafd7c3d06da25a51d2a7dfbab6ef25d346d785cefd765a15ab6ebfc734","0x470f8adb576dab056ed44e5dca68a4c5b5ea41efb919b4d16740ca253d8b9c66","0x75f1804edf61df3329e9fdd2c5f5f27075822258e5f0c7a00e114903bd5d1810","0x2de48e483c6baf3ce36f8a3037ca3336e17ca551ee7922963c3a670943ba019c","0xe6ae3928baeff3a79bade2f85f01201c8b8b3ade41139b6f5160cceaf3895bf8","0x70df6ca8fa5ef026fda28149070269a9a0dd29eaaf8c8d6beb70968754a17941","0x45a935056a01b6eedd0597b1079f9f254ae14193d456d62d93ecbb2edf74b4c3","0x202282a8d7ff6fe344275012a14bef74b2f2cfa9f46109e04f15af9ab0e7db9e","0xae41532418dd6a8d3ec7a570e69ec261746d8830141ebc30790cb9cb5e90ca8f","0xaaa23c7870d5ddfffa260e4fbb4fe69fc14eace85fc64055a92b75c5adebcccb","0x36c6967b73381391f3a06ebfba8855adad1f35e44f00a2bcbb8db902fb970119","0xb3aeb55075a522fe1745afbac31ccb2f3148967fd1f357cc3636920896f9f53f","0x00be9e533f0c2b3e5222e15badd04854003519d49253457a041533fd4b152509","0xb091875dd3e941432ca38483861a46de3d4b0ceb70519958c03e16329571b766","0x313404ae53b97fde722919f1edde745f6758be5587d50cbf5aa9fdcd067a3027","0x3030f0892896f78c8aec183ecb0f710c993f931ed735ceea36c0aef50005b4a1","0x89ace278c0d197fdd271e40dd0a9bdc88bba9212b233159939bf8ec35992a736","0xbec113ccda0cbd521620316e5cdd75deeca7df15d2ac11c9cd6ad0659c24bb26","0xd742a6d72983eee114974a31e7e30d7c97eb243b001c757ae4c830ffbf7da070","0x36ca92bbfd274cbe5f024ba47af8f7e9d97d38be13584d79634c5c014bcb36fc","0xaaabb898a4e8dced5fea9682184cf6e8163219eac686998aee9a7110b9b189df","0x7576ce7c907ecb3ea2bcdc88bd2066ec202f1d07258f7637da7ef1f2391f7d33","0xebe75e3e767fae4fdf3d1d610919c2f128dcdf55a197aab9d5f3a0dfb0bc56bb","0x80b9873c061b7ebf6a7468ea735a224297cfe22ea299854fd8482bac7690853b","0x7de71ea307c193c953f8e334bdc4aca3e0b8afd69269e9bae274fe277c72be0c","0x7cf1a2fceb4621efe94037d341f61bb3c9c0a86f2542b5886e67b4f8dc55233b","0x08950ef0bf43354a86fe6aadd3cc930b329d021e66785a1795fc7e9c6407d396","0x7afed816ad88aa0619bdbb43f1648317026dd101d468e08f1c85a2a5f24f59e2","0xe5b0dc0b521852e5a05ee99e4f19df383e2637c0b78168fe2ca848be35617236","0xd5cacb8f9315ca7b44648744e13a0df2cf240c690b9b8a51bea2ae577653b7a8","0x4b768291e3cf8fb611f83dd18886a4fe630a0bba521e53a350e80d94e0102a98","0x5828141658b6b7fb9e29a2758fa93bfbcb3f1bcd84a512eee0133383ac6094dd","0x52436e26dd3c11a05f03b7ba3793062c081fe11b2671092d4ce6b282ca321f80","0xe985d99674eac2251791e07b5f49073fe570d52500fe08b72178c2e9039b20f3","0xebc77d9a47d430a1a738ca991857329f0dfb6dd39092e922eee42079b2dbfdc6","0x4fcb9a2396df05ac4bd13949544e1100ba628a7472707156247cc0e0e2ff00b5","0x98204094bba370db4561f094687ab940991a0fbd873dc7288f714d881a158a6f","0xc59a6684b7c683bef9dc676e026899ad7e91f689c3fa327517acc5e9613d1a0c","0x61a440ec10afea0c8fcceb0f1b536ff43106686440dbc8e6a46917c06d872c9c","0x7703376737644876c62a4cb6c931717cdff533388eb717396f17f7d9eda98a1c","0x10b7f8f5454a23dcfd2089ef0a51eade14cab59b7e85354a1976bffd30d82343","0xcb108c105c13f00e48f4b2f0e50b5638641ec7b25b59ea36f438cba5b8b97dc2","0xd37f477fe5012d6bcd0e32d9c8b9661f40398c88987b61bea70ed8aec28c20d4","0x04bb7c5e46d518bb0153e7f49e433d217647222682603d9aaa837fc0db09854c","0x1f77a6fec4af4b1ffb6bac293aaf1838280e3133212ac57f4e487fcc07faf20a","0xf56941462c337092cb0a048273648a2447d2d9b65b8967e9adb110b08a4e4686","0x0f349910952f1c610d375dfdd2ed4a939b759df0c6a543b1f26f3924822044da","0x6f8d896f44a595f34d18649d6cbca1bd64adb0d8c09cccb0ca917b6bbe32f2c0","0x92d6c3acff83cf971c1ececc3c375a2096755028274c7155df394187ee729303","0xfc4024e1c637a9b275cea7601b1f2f5999ba5456132e7754320acbf8a8326d10","0x6171bbfa625e515ef0c13ed59c070d137f832f7aaa349b24799c05256c22cc8e","0x07e1cad114c28ebfb24f28a45f22dc11f2344bacca936ee4715c5e17260673cb","0x3bcc906f70c32152677b2be872b4bd32bd42e7626fcf39c6093413c106d3055b","0x0b7127d0b8676c5563e651b3eb6f4dc26c287186e9fc84ef5e40ea929348f936","0xa2a024a7fe1bac82111b895f8d357e41d96fc207749d136382a25fcbc86e7426","0x0c3fe0329b174c46b7375b5db4d60a77fed77627233187667aa427d00986fd71","0xe72842f938d907172ac70fbca81153bc4132f0adb5b4cf970406af21c1fb241c","0x626ba513d82435d6e34594d5b10e528be2921b60df4d82926eff04ecdd312a8d","0x6c2d5a8f17a2943b8c25d300a8deec6eaabbcb06f735355d56467aa8a7ff86d1","0x4126628321f172d0dacfd3c23d201f0d2c990a1b171565d473f210d484a72f09","0xa9c6d3d32c2ee8cfbb9d71c0e65b0950394c0458344d16de1bf170a89367e7b8","0x1c2c098d47002d0c1662ef597c681ea0097f5501b8ee3c0d1122b3c103b1bb90","0x69a671999b11ad40bc3ae847714e7946994045e38da1d7789c8b622bd4865fe4","0x173365b4c83840cc54e69925a748b7698486d2dfaa1e423e45f6cea9bf59bdf5","0x1b29757b3cc9f34316217d47573596b238d7389d343f069fcba459482481fdd9","0x386c35e36d130cd15fe49f315403bde5720df43602c2f84c7c2c46322a6375d5","0xd2905acd97fa5feeb442597b93e62d71ae80cc0259fd04b26bb7253e760388fa","0xd35126b45140211a32fa9f2dfe4986f31d06fea31e37f2d7608f2b5aa7f827b8","0x97fb5c429934245143bcc6b8fe62163550c70af60c01ba3425aa7d16ec56163f","0x3d08ef02ee11bedef0343c086d4b82ee48856f042089c5e2791ae3da592ef627","0x6280d95459932268ed3dd4e39b10580fef4049b563a803f68b3be95eec7d88dd","0x3441067a209cc181b7216ff528f96b6e51bada41ead73ad44e5bc1c1f4c0f93a","0xf941dc557a6bc9cc3321f89f87893e4fbc62c5a64c293d2c79460d8fc2e6d060","0xad21e8e21135ba2ac003824d3837a94ba245ac5b8e210c97c11bd5ce9670a76a","0x2b175a41288bcfb01fc3716abf4e761d45354119f3e5a4d09475b9a5d0f2a606","0xbb3cdf3bd015411645903eec4334d29446ef39cc489467dbd555f8a69f5ee390","0x14e6adec27f7abf0bc145d51c2d109028a5247b526603bae90466ac71b11ba95","0xfe4510fe9a2448e9accc36d6674b61b768f3077e651dda6239da95f0c0811167","0x586963cf264978fcf5c29035a751240ba026c4dcb399a1a97745616815ef8e95","0x209f5f7b665e164fa0934f27c20487dfc58875ef61918331c5bf3ade8e281503","0x5c54c500eb796203b107bd7834391db7e755766fce3ac11d874b944d8d8cd255","0x32bbdc03ea6ebb846b57b46e902fd79654f714833cb7afbb5dfe3b17c63d7eed","0x38d8170a6d8d1172378e40412e984d2e7176c4a7bc9978856715e08c0c23aab8","0x5142cff853827593e9d5a89fdbddbb870b13a067459abc6749c8973862157776","0x9ce8567835f6771bfeac1d208b44a7baffe56618e1f200f778c96234ca7a0905","0x58cf765e7f35766c44a81ca4a6518fff6e92acdc042eba077757742647d34bb2","0xf8ecf6092ca27cb8bf6ee41fa64a6765830d89f17a004df6884c1c8a9e032c51","0x3ffcd30d41054859c4c0e0f7ca8b4dc12ac071f6051d3612ebec2ff114c1ce43","0xc95fa6c7bfa73a56ea53d326fe50edd03227225c9fcaced1f4372521cfd2696f","0x2fd1526b9a9ebe25ab642c5066966a9a1f79ab63150795a4f2c4083eb22816ee","0x3698e5aa70fbe6a1f897b27831f23a5e1342a6a05b1c83692ada3e91cd9c74b1","0x31c6d067089c6e030b5943bb57c0371c4c96733c584075748115d48c814ec299","0x74dacf342b197bdebfd86195e1394087e7f2e21ebfa8ceb84e95672998c93e66","0xfba01a302ffec7c48380359bbaae3e6cc2e2efff2c7c40aec155e48018ff6022","0xb6743d74ae787c087830f5675b803ab918424905d0f8e88bf2202d12095347e3","0x84e47b78c5358cbd6812a39bef6ef985b998c07e04713ca4281fddcbdfbf2a65","0xf57ba30f8eaebcbe986808d0a0e9bb7e9518fccc3f230391a55390c03f39c1e9","0x94c89c89fc2cdf7eb15c08c46a9e9b9e675c44c9eaf07e5e96dfefc7a2a4115c","0x52bfa4d2c0f4eea1cb7015e978923ad49390d6ab95ff8d56c242bab1f5c06089","0x4996e5042e4ff4eb71fd454f251ab29517469232960dae71d886342504bd8591","0x1efbaf0f074bfe02d0fa40a70e9feaac82f457b7d85bc19668dd433047be5848","0x1a9a76c69ef0f5e8c4d154774a27421be990af7aaa7514b7f83c24de9db48613","0x9ce79de5010ae9d4b6617811903d1ca8e7b6e63b19406379ff75811c3db55d9a","0x306704adce0695004f960cac0807ea5487cd0af50c1e68f3ded28d0dbe1beac1","0xc97bede5020ae95dae866dbd59b0321622e8ea22451df6940e2d77e1a8a791bf","0x3d1efce4b7e8dc5322f18d01dbdb643561ef6f1006ffa75830a8f15c981c537d","0x919dd0df9917b209a52edfd6d975d1b05ea4bcb561cd2531960be315a0009a59","0x0502016a3947d62e0625f3e9254e6ef94c1d73a855ed9122ddfdb7b6d5cc6261","0xda97e7279dfaa98d87cb747b170ff38cb10a3f12076e77d6e2c9adb6c302b589","0x3f297ba769bb17cba032fc8f1a04bd30a3fc2fcb27e3d87e892c136851c7d911","0xb80589757878f06e791fd440df8ef98b67bfe5dc5be948e088f7a2c39d750da0","0x053886e7bd9f39f87c541e82944d6ef087dc552c9e215910bed0936f7ad2c7a0","0x748a710ccd2f406aafc639b4ce26ae3c5c4e5b583eadad00cd3a4980c57806f7","0xf1d29128237f3dcbe6d6843d08eb237025a9dfba8ccdc6f33b3d9e6d91c4e46f","0x169f6c1d9d6e95c486e217434aa18923ebca2264cb968c65709d17c09de0cacd","0xc6eb4ad32372e8b02150279a9f3b9c14ad8fd3495f9e403489527d4137725b9f","0x7775e2d7d229a3927e571a71829a21babba4af0903a03246978578c8dd1d3910","0x0c707b170d0fb3b25ff30b4a0f516657659ff74ff55b8fe3824ef7f6af107491","0x00e9adfba2e2e601c05c1f932af627353bc68440fbed91702a15d9271cc97c8f","0x3d43dd6ac008b50cf873c2f8dc83c57295b9b14f58e44f74c89a20f59cb4753e","0xb53aa4be124508b9a31196a50f7288a13603ed54841b10cd5a4419c9aeec1a60","0xda74cafab5cbe1bab449770aa0ec84e11477deb9321af72dcbd47b56b3e9adcd","0x949f7888f72bed86ada87a5e15f8c8fe491e23f6d1dd700c44e15bcf37e599e5","0x9bd80e618045ff55c1570e85e65ad1d4211eab4699a258fd3e6570ac6ff57152","0x942661716de2b58f7dde35ed4ffe38b9f89a8de1048d31241ae00a0928f2a943","0x4080fc450cdeab3c42ef338a936b72315df9da40910d6efc24a2fdb1aa30c240","0xc5ec8a55820cc71e2ae1bc7f15e4324234dc55924192244ccb8477e09c7d49e2","0xf84ac5ae0eb20d6172ed230ed8ffd9c0ea493f4d86185223ff1951e3f2c29ded","0x06bad7425d79b732c0b52f57589865ed1a875be2d6bd58fce22377a0f6218f5a","0x3205e71b3923c4b96c16033895cb1f4849ef5205e1c9c0f38cfd43d09ceeefc0","0xc31d86674ba7862ae31156972f22ff0e2e3dfdbdd3219b88f26f215a6461c8ac","0xb75ff594ddd96f4624d8661789bf3ebbb00a8c485fae86ebcf014e89c576a15c","0x80877fbd26cc3d3aed41592f8381e5e8c437c46a2a38bf33e2c4f298e5967b86","0xccf826b769e88117946897daf847f5a64807af648979391070975253eac5b3a0","0x4dff4976cfa502c30ae2f72ae197e715c7fffda5828709d8f034a8bf1f0697c8","0x0db150105485a4d4614350427bbf1a1b5810f571f0cb73936b15e29d460bd626","0xb0b8d74863327e87c743c29c21f7d3dc9913bff9c1e2501ea9ca8e409bcd285d","0x0e05d64e4b1515c5c3402ea10ab96461ff90a6af4bc94b0a0284b9c3585bde52","0xc0e1509461775da937aedda299ed3ead13aefdfda7bd24e15762c27a5391211a","0x74327d6bc92ab8f0a15b53c09707ff401894ccda485898114eeb3b9ebbca520b","0x4696c86bb1036b631bb11d43110316cdbfdfead51d88bc98b3c6ebdac2105ac0","0x1db18d88ceddec52b5ebfbae479a43709a0edb9d48d6d1e94b47e1d4c8d9f3d7","0xf704a158fb22cacc2f9894975ade99f1477f5f118931aceb58aac40e1cf2a976","0xe42021eb1851a605ae623bbf560a63d0c94afc902569b74caf403d8a3f847add","0xc0d10e511ad4451517a176312ef02425461d4aa1bb905ff096bdd966b20cafb1","0x7a0473cdbf14f2a000fac76755aab3acfedf622b8f2ecf056ac9df370ff3735d","0x4accc3ee5232480e0f672d148d743731debce9d218c7c21ab6c7980278c0d6f3","0x45e3788f4abdeac571a82f678e87787a318d3301f1c9a519651f0f9cb0848506","0x2452d1cdb64572cc007e8f24f74f41e6e338ff57b67b41bb862aa2992a72b877","0x23a5492bb1f638d5c34894a84b940650fb115a2718f31fa6753fd79bc77a427c","0x1523133ac5873fee7a7ac2edd518855bf3015b69d11dadc3dc3c3d5fdcfac740","0x08edc845153ff57511cf6c03748f0e904f29e8627d0e36a592c531ca830f8f1a","0x07c1069af699c3f610d3912c77f55c5aefb7ec8ab41f88591cb5f6ad05284ed6","0xf46cb6bc222c2259ebd738518ecc09c88550166e814d0b24a6fb20f1a8bb1bce","0xbd391da87e041a84754c0175ff8a139993e11d5518443243fbc174abb64039e5","0x0928d74e5b66234eab32e3fc1422b303c101be09ea46880af5c49a786f7ddfcc","0xc45afab4a99a949e48ad64f49706639a3b013c374c93989646df3ab11d8d6722","0xd988f3258990e137b121a33b3db246ba99faf2ee6bef827b7c437bb6e22b823e","0x53c1647c2439687681e60eb400e73ab2e02721732d4256847d171989e17084e1","0xa33e2f6d260a1b6a6fd8a91bd6c70eb0ef7f9719d4c33b138721db6666f7a19c","0xf60b959984120dfb9271af47de797e2448a66999d466236a2f85d000ba39c288","0x379ed1416d2b090636d6e06f9d0eba4eb4d08e7c060469447b2da081474da3d2","0xf09401db2ba16a44f0f00cfe9036f61b3fb1615bfc6793936d16d6fc8a8820ee","0xef26ba7c4744fc5af2756de860c718c77212f4233401a15d593354e0867ac4f4","0x2fb73d97274378a5a7783c08a51fe77465041a212e7ae9e7eba4b17f53f8d97e","0xf9877cf404ab3bb0457bed3d7fadd3b6c8c0bda47c916a2445d80d676559f09f","0x97aebe3a33d1c453f2b6def43193f13d49738802cc2d3232823f04052d764ce4","0x0e0a16c41774fc0eda2ac97ac8e7d36892599a96a918a26ab193f1536b5872e6","0x205eee7fe0746fc30b4a16c4934fd2e4a70ece3899d39c0dafffccb65f274a57","0xcdd1f26a842f2493403ef8bba5d850046e3e7a5fad6449caca21989b5b24b255","0x61dc9baa478b938744847c406d9e40cb11939a6d5276a67763d95c1361b450b3","0xf7bc6de297526dc541ff69d4bd3eefbc9a2f0c3c55be44a132cc5b7fadb0d541","0xee65d3d03d9e530803975d10fdcba1929c80cec143da32641f33b246202136c8","0xed6ea826c0293480942c399862d27e6b10c8c1af78c274bbe8cf47ad6881b29a","0xeb2765b0cc64201feab87095532944641b5ccd25682941f2a1ae3edb8b876620","0xe8f5309b293eeeadc50c9a778e5145ec9e862aabd2e5fcda7dbae4ffb2715e14","0xe5a60964e3a54de168066baef336e64de8c57290a08858bf7b4d3cbbd3691a02","0xe2239ce48aee6811c6398aa422934070811b0204ad807cb9d635de7eafec41a5","0xde114a8d5a29c887d24f08c76e8af2ea940d685a3c372ccf76637a072bdf4fb8","0xc70e61baedb3f3a4016ec0e14d662f0375cd99b8f7e414b9069305bbabd63fcd","0xbc92ba9c405f33e9a787ec58730905829af3bd2fbd188c086b93420d8ee4c98c","0xae1e0e82d03679cc32799584276b557783fc83aba74503ed009bc3ab43638885","0xaa4cec8efe903839e5ccc960c86063fdb79533448d859678a71e9aba141d185b","0x882b903948a78cbed2cd59583a074edd564df3f18b33b74dc3704f94401eb117","0x81c35fe6619951c81728dcf24e785d47f911e016b848f6e18a831232012d2883","0x7db2abeb717c36b69153bfe3d6bf1ea0b6492964c1f59b670bccd05f49338d5c","0x7332ac4b5048d16b55481870d11d0fe5d83999b7d0080944e59d7e1b34ab1936","0x552cf3a195d78ce03f83c651c0d72cae7483cdee876bfaa31e458dbcb5277896","0x4c54d65996e3dfccf2c2ab8a3bb9dbea6e75a2679cfa512a60e9f3a091a9cc66","0x4c13bc04c407860931b16fabc06bc818d776fe50d9fd8ac1e99c8284af321f7c","0x3e6fb22716250baa7bd38c3bfd11c2a66e3f720c1d49ab69a018d6a698bd8ec4","0x3d1d52156787dc87bacd072d5a77681fa969a4e62d63b018172b30d85a5f4644","0x37ee64d7e4c9d26717a03db0a4f4fc5c8468530817c8aed19133a5c522d97946","0x1d207ae5baffd89a2090c09a4f031fd4885e00419d7d08ec535816862705c05d","0x091e65e42fad0aaa635dbe35e849eb744a300f719b83739142f990b37f8a5ace","0x464a94911c3d726ae84c3631afb1953a03e19cdaec6f6cd810c0f8e33d30d468","0x6e3043b39c0986fa707115c24b6b20f4805c01536c8ae0c457c8f0c7d4ab6d5e","0xc68153d72184a2a6b43403caf3d948f2f42d433e61589831608b513f6ef1ceab","0x47ad1587a15c9506626b41fe290bc33013ed7b45e295c818e8e19fb1771b74bc","0x99c35f9b215c8e570de37053d401ab7ca96f388ef0198f8ffc43935258303073","0x89f089579a58a2908e993e8ec003263b502ddba8aa4fef87c675f11ccd756322","0xe71dbf6e082c599ee23f46739f6976070251d380c9d9e9bd547447c0e26715d4","0x5f561796cf161ed6781a75ae2f23eea3218bd0cbcbdd22bc0361ae07e0c7858a","0x701a932fce0cf63690c4ac53b55b96a0b80a19a899427c39247fe9c372107ead","0xacd31a9b1d727880500422fb4eda57ef2a368e1187a1b4442a58abe928b843d6","0xfbdf6bb1974ed8a962259b197554ce65d70f5ae7e61b57d3216fc8d14bc24172","0x217858ff4387e033a9f7cdab885fbc2e4161a5e06acdb4d2a24e63231b14c7d5","0x742fb3c83eb34276cb8b8f9ed2253bbdcf840910500e463994d46e79c39519bb","0xf5fb520667438863503fc9463580c4f7eef0b9645d53ab5550b7cde7cf177429","0xdd58a8d23e438e4dabfffc8661cb45ae9d5f9d61818b445e44b347edba75ba83","0xfe41f152fd1ef3104e270d7cbc13ab20332075cb34e2fa15aad509b87a8ae498","0x857618d524ac9c3ae931bc24d6531d05b72f61628f84ba093d5c0a6c64a28fb0","0x178499e6444b4ea9fdce6263fbc8410a83f338db33a966aaefc66f42ee9f88ad","0x1d787a8c6e57791c60c9cda67bc74088573d02aff35897ab5b2a29ea4281bc06","0x722a63a9ada082e133acabc58838af3023de8050684a51fb57f1a9ed7274fc23","0x7fa7ead20c9226be2ea33c0d61c4efc58ca3766644c1ffe231fcdc14d0f19a0d","0xde1a97cde275c36fb386fdf4f1073c42ef7101046dda967db437ce04deadb23f","0x1abac5b7dd948a9caf655d4b17d8ff32f773230def3c98703539c385733e9ba1","0xc5522f500b62f2922bd2ad98128dd22f7abb3277debfd7ee497eb381652a863b","0xab369cef5926de17f6980f40945e19bdfcf559a26594fca3bf1804a25dca37f1","0xfe87935a356c8d198a2129bf98a809472de962b6feade49055d4521dad19c5fd","0xe325328cb812a26edcc9031b2ae8c2f809461ebab438f09b3a8d08912006d795","0x78b0588de50104d5f99bb1a6a02dfcf132bff0d9ebd84cd29263e59a8f7f2322","0x8584acaee619468fed21aa564a622cc0e95672eaa28e91f81914b7b5651c2401","0x7a3f5855d8a484796809b0836f7b19531be221981394ba48aec87068d043b8a4","0x5bd3bbda7e7c4a1a2e926668a29576950998eb4864c5641e3f4f8aa128524e4e","0x2cb0f5d3bf992ca9c674d9abc0e1600625fd371741b93b720c5a67151f33248b","0x0a1edd6181b6929f2f305b82e38b33cac643c1c56819582949d65c6bb71d08c0","0x445d9ed258881f68b243d43a5a35cbb0e90706bacd4cd134bf4e9e680bc77253","0xd86b917db83bc3e34c34583364a6aff4846cbe6cd88658eea4aed476b9c4efd4","0x5a957ab41af5988a92a4bb18a5b2707b699cb5aeb08110b22af9a238d6f944d2","0xff4d70d3e6e33c3497a6fcc0966c7d8e66999a75464596c1ffe25074a5e08719","0x06c76aed0ae6199aa3d73e3f5ada378b553141a11fb1ba3601247b4a6cf02a88","0x99b53aad4781b47a9021434643e1a61d143c930c9508b1ec7015e129d7aeccf7","0xe8b9d2c763243e0bf00d0e7b1964a90b8638abb7ddc6b62e59058cc65206ef27","0x0772880ec14dc6be5cdde2462e962a1604814ee6e171bb223764d99e247696e3","0x9651dc5859e82fa8311c5954415056bfc9c03258e92da67bbb7f005524feb892","0x987e090cdee42f38535dfff0aad1599eb9b884a674d154adcf59285931bc28c3","0x293790e60d520cb61ad1d736a2e8043e61e10c637ecb52c3c0df26b340f3c6a7","0xff111607a91a2c229d1c94bde901a55e46022c6e9a2f541abeabb3992f45e784","0x675fbbe8739f7cdb88af4b7f2c243a90c883ea6cfc5ce03fbdec71d54c9c6fd9","0x4f253751691a1e0c0540fff14f8207341bd1c105d71d58bcd20e3dffc312b112","0xfbfd363dd62097ed5c33eea8eeccd2da8c8f957bbdd11183e5133278121ec129","0xb2ecfb319f296e2131fd8013921313b91c77e66b43647c51e648760e84339cce","0x9ef07796d8abe5ac9183b240f79ca2034276ff30b8fcde4ed6daf512a6badc24","0x0a911fc548b57ad1a62c54cdc0eaefa7375122a49321e553d88ed1af2357d1d8","0x97e09e44e6cb1adba32a9f9615a65fce3adcc0d342f61a1c44e12fc224b33cfd","0xd4823e1b159ef885c72e810a8d6cfd47fc97ede15d4c0bfdc2b3a1a4859f9dac","0x050c59cac353c92fe2cf27ae4dfa8633f11f6093c82f82ea3a2c5872c3c5de00","0x9c180859a913fd936f662cb69dad5ea809943961befdd0e9788bf0f947aab50e","0xe5325d27f11870aa8b831fc99faaf5b601ca7f0e84bd106cbc542992717dfc18","0xeba55194338912940e3c469a30a7c1692d98055c6544f8eddfba9577000b86bc","0xf78246a7aaff42f040569dd09b8b172562d8223f1c1486f6db50d5224f5d344b","0x2d954241890cd9a60a905ba86973aa46436bc31e066da0b86453e2ea53157418","0x27746023e445650ad895a984a80b5876ba4fb8d8beaa18304458348659b2e47c","0xeafe17d6cc4967ac5b9a300d9dbdb2ab9459de749033dd078b7e3c238d7b8989","0x93ddfce56f18ad7e91ed93cbc969989d46543255c9be96a27c9267461d94e0c1","0xc6bac08a174928f49b73ba4b910b37ff8af3311b84b32b324afff146e8149108","0x598874d39b2c812778d206dee5ff9f506d8c113b977b260f106e06e5ea387b0d","0x76c768884d4e5dc5f7e8d31d6d25d9bc6a3d2c19b4fe9c9100158166e7bc5b9e","0x065242090e5841764aa35fde909217b6d534c537c51cae2a72356fbd3db9f8ab","0xaaa928bd2926727b85a99fc6eb2c1990dc3f72c806e8212156b18d3f88ee406b","0x135508155e034f6946d09b7d16ee93ceb48f7d676e7901fdb317be682cce10bd","0x0b7477d669e7962ce3366ab3552f4a566ac964c2ac935bb4ec8deb8000245249","0x658c575750a89c4392e0c223848d4b64688e6013dbded0223d2e32a0c8d7aba6","0xc1a6cc7c7b8287f8b963211a95fd2432f60df45fa35d6fe9155d1b0361a6328d","0x39c68a15774d1bb0cf88a444b29db734f96a6dac6156c425087354c826a6a15b","0xfe22ea0c837e1a95e42cbf28c9e1fb5015d4019d4989ae6bbb7b94aa81b77af2","0xe7dca93d537d419ddeca9a56e3a20ab0537b48fcd0b018a61e40cd1fdac00f70","0xded415ad1618ca358bdce589e3d380f228ecf3e7f39126a15fa9198ff6cd7d5c","0xd7413d98d1dc6e83dddf80a23b12f886af32008c07cc279d0efe3133b75dc51e","0xca0e052ed2e32c04355eba82cdca6083bf17528c8d13e26383a385dfd1916748","0xb16f807522cf98d5832dc944e90f96e3819617d1e7639961460ce7bb7e54b135","0x9ff798cf96a76b7a80b7d0ec978f08fe61bdd4609a52e8d125b319305c52b314","0x9f8a8ee9a971592aa5e65c0aa56bbbf686ce2a8d1c840006033dcb22b516ad37","0x495cc50b22d0673a0ec01987a3a1419b641e4e936a14b0fe26e1a354fa5d156c","0x1a2c6691a56efe419b9d5aed0796c68de6e142c9fc604291bf456c6269339b43","0x1786cac25fe7479273c533f2b92c1dfd5d0c1a408d0078591c56e9cc43d0a590","0x0a5b9e0e7f5ab99dc0f054b39f44efae043ed7bcfd66b947d2e5151a1b01f552","0xdc597c86a2684c30f39daa31adb8a85957ac0797e0243e07cacc1dd4bab75191","0x742ec8859f9a0ba1df54f3f86ce1e37d15a8fb9d0580a5104f173d5bb2a636a5","0x51f24d57ef0320344d22048ac887b8d4e7441eb0a6e406bfee42f5f7ce5f854c","0x4b723f3617ae38a494b90f7953c504be5aa7f630319de20f4d3ec28156c919c3","0xfddce3743897d4829a6e436514e536964931f933455909fdfc7c68a917f194d3","0x78a44e16da0321bb1664e6a0c0245308f9e4192f24e0472d707d4ed94cf5e79d","0x46b50c2ed1653b3eac213eebe9f14cc18f5769d18649e94d85aaf601039b5c6c","0x1888de4370dc497e6605443759980219b793cdeda3fe1a16250fa6f9d5f16e8b","0x10b4a06abc0496b0151078138d25f039a2fc9a4653145d9438b6b6118ed5b102","0x0ad5580a17356159d9ca5f34ec05e37b76ab92433a2c4af102528c43dccbf3cb","0xc54cec6df649afaea08c2cc0056e1d7ba99700f7349102d8719e4b6c0c7ce134","0x6eea44957a0e056066bd4bec2e42e813add0808d0b2765352a1f4237f7f666ca","0x42d6dd3486354789fb189461f5cf56997c0bf66607128cd03d5d6da8368b4ad8","0xcd89e2bc0fab7eb47fcd3623c22fe69012a1d8581496a0b5e702ebc968a1b00f","0x3746d8574bce2cb56aa93b9a69e3c8a33a568d2ab7f88422839ad2df1a8feaf3","0x16d4a3387f8cb9363a4d302f70a8d951d41d253062914b90f38c9d14d8ec516e","0xc58bf8602998c1c06c6f2aaca5abe02912e8eb8e59202835eeae9150a4886d01","0x6f7d5e02d502316c7abaf1a7a910f923bb23c43dd2b904454c6086e9ee55389f","0x4f04d601d985604e60e4f13f519d683b0691497c7d67ef891a9b8dfd5cc5d297","0xfc9b4bb6e3997e06877cab021fa3b4a513b44b7cc3a14c3e6eb96b5b7c3a257e","0x670ca0158db028cefb207db589d68c1f0499c2a6c3f94ef05f00ef832418bb6e","0x6172364e2c9c1ead5779eaa7491940259d061e5a8a84ed52350b848d55f281ae","0xed6c022f2513a5b645d20336b826ceaebb5895b54f0c65a8f8ca2031f61b5843","0xa9becf1cc3b4280f6385e1f6dc70d93ef9597b36491341a4a93f3e108efca546","0x71c3d389bafd2ddfb501966ab24a72df52f5bba5e3431e592178b5bebeef0572","0xc73f3034f20954d825c7eee4bf48ad8a029e3eb4c91561370b0b7b5fdb031632","0x849c93ee0534de5026d98333d851ed9c1ab8c0533854bed5394471978a456c60","0x0b16d106a41a50d510b191e9eb33529ad64a1f242a69d434a1f4fe255d8777c8","0x2c4eb2cc9befb52d9881d5a713b5c4da9b1ade77dc97ff7d2f7518655dbe014d","0xed3ae3e5d27f20511687a85fd5821176e9763ee5c9c510a47122b54343d5fa30","0xdfa5ce9ff3ff101da79d7f745c5c12a331799be071b8b9beeb1a98f2ecdcf9da","0xd3ad45302ae1162529c722599b7e47266db3a4044dec95124d59dbd4db0ecf39","0xd12232202fcbde04f816d271a16e5eda763197ffee993cb3684b65d9c6495df7","0xb0ca8a637d36c6b9b338af45c35c18f9cd18fefc261fb1b74cc48a8ec1fb721c","0xac5f9e1ef8664db83b1eca729fa2746f0b87097028a4aa25bf88a819b233fad7","0xa05531b31fa409fdce3a22dc9366a14075a411797ffb88b9fe5c5be1afbfef3e","0x4e7199732ef6bb71bbe15b7dbf6e5e5a6f593ba4e4eae6fc0f0d1bf2adfcb415","0x3c9424ed1f19cb577e67b30205f98fe33b87aadb2fe47a475ee233486320c1a8","0x11259449fb7ac58bf41dda4186570d9f6866dc846b3ac072bbb4f4ea945c4671","0x0c96167e8d8e0cbce7fa47e5fb885880df94bf507c10e0339f8991e60f2d990c","0xce7d49fe4beff8a52e819b1b3681d33e162a185b8c1a854aae64b049995fbca1","0xa8573d3a1d3c2f956991bdca21d8f737aa9decf3436de3c6aa66ff72f38bceb3","0x6dd889522b0a37d6c4168ab4cc33ed0c4b2bb778f5234045ff9a817bf868d47e","0x6c33f12f6fee627981b9869c86c92cc9315e39f45ff2ebdfccd227aa9677b70f","0x2428c05646f4f30d45b0725a2e59477cfe5b707047c478da2b647a0575065884","0xf35258d8479acb962e96450531de6f013574db63dba2523a8e90d48b45b34c3b","0x7ffecff14707111271064cfc3006d0bb3ff80edfe01ff15027a52fde3afd5b84","0xf776e9951cffa4052da7296e257f402c2ae7fd2748e90e1a5f9330bb04d9362b","0x1da6547790d47b0cc18f7a88259cb3297b36bd2f3a6f1d449c0be7af63c43209","0x4a3302bf539f65ebb51ff103b7406db1a115aed770de39418d83fd98b21e7674","0xbcb6e994a81f2c1dc7874af988a9a60a9c53c1741dbe4aebdf25131fdb04aea9","0xc19bb615b95e699072a0ae0318e4b81b035972e0e2243f68776edebabc65daeb","0x2afa6b0ab0a36bcaa9d84a7aac10375468ba81346cff9036b9301d8cf9fc090c","0x9edfc090f68c470c6e326ca3a784d51dd4a80ba14ed8b417e38ccf9d4f77a5e5","0xfdc691aa9b60d40f0fa1df9f72995a4f698097f8a4501f680298a476d5a57857","0x2bdf0c6f55965aa7f5efa78c4d86caf9cc5650290532e9a6ec00e3e795789ecb","0x255ee242666e16e5c786e64e2428257d01afe029ddb20576e1be5ce28335976b","0xd12834ea614fd74159672bcf3d594c4db185b46b1cf526bcace9faf1b1c82934","0x44b9956a4866ce96a300e98d37236e062c18e379ed8b1935381bee007e218c1f","0x29fb7be8a028dd03f055f4ed9701cb0054a12363c3860af27ffff219670aee82","0x049a5dd86bdd8e84231942303794dea8538c0f3ddaadbdbcd9af879f74d9df06","0xcff2f4a22944edf6982f27381b9d08d8cb922ec0d605a4f63c8c78d39000472c","0x30a5f65880ccca3116dc65644605f5d19e74335e4fbe6f854cdca008e3e9afa6","0x30a96d9a0ccbde8490fc86c224f458c546b863d4ebb8e99ca47ffed3ea99364a","0xddf7cf6ec9e65c3e23ebcb479b081b5f2e02facacfad040ea596f369a2671d5b","0xb50a78fe0c09cfa4ac5369a8a5a52d9ea81883eb039e7aca26fb0672ccd382fb","0x9365f558d1d0c2d7a25ee95fa57694d8fb61c620039b21711b5247ef6be3a30b","0x6b42cb9a5821845d3ee37d802275e278cd4d93a117bbdb8f8389368ddee361c3","0xe9a69e5939ec73417d307a7b06b352e7fd8f2e7fabf4a019f3e154690e0bedf1","0x3a0c8476e3bf990ee117733d004bdf77f84499a2cc151b23ac608c2fbad7e956","0x443ffedbe263377c8f2daa955153db89802c82acbd31aeeca02eb3dcd373cd41","0xe061ac697f13b3f39cccd75f93bf89c0ed68dc96701aa3778f88bd593ab12a35","0x5fb51c92a8dc7b05a326dd109331718a8d5f94c0c1795ec42532817da00b187c","0x60318cb40d9790e93fc61035e8240a96e6f2f2d9cdbf815c77140dff7f35859d","0xc18a4648ae08fd72709d961d5baacf9b061bdfa62ca77e3fcb1bb532d5d7f49c","0x5f3c48f8c2a558f19635b921941c240fb0623a84335043b15653f7321ab5dacd","0x68aa6e86f9d81b92808078615066fd1db0bd88a90172e0cf211bee2cabef84e6","0x416166bf78a745a8c2bb8f8e033b328f3f73bb437775976fec448272c40ae57a","0xdf06ad8e65dce06b252d3da668a90c3a00c548845fe1c3ce4040ae41e2306cb6","0xfb3d66e266552c9e413821c3081b0f9cf517fdab5e5c8732a669926c464d99bf","0x4bb8589b668f1227a38c75282809ac66c924f89fe7bd0fd410ef44a61c2e2e9e","0xa84bd84b4f90a3b071c532f6d96e9700e3660f5e199b8746915bef55a62720d5","0x1f2f5342c20f5b9098a94c6b5735c8bf32b6fbeececa18b3e60b8090eb7cc080","0x7aed52a344b103f5f1a5d802e54da9c536bb9bc7a1da6e28f5bcca4b38d7f1a5"],"withdrawals":[{"index":"0x6f25f2b","validatorIndex":"0x1e4e28","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1029ae7"},{"index":"0x6f25f2c","validatorIndex":"0x1e4e29","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x101dde8"},{"index":"0x6f25f2d","validatorIndex":"0x1e4e2a","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1021b45"},{"index":"0x6f25f2e","validatorIndex":"0x1e4e2b","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1029bdb"},{"index":"0x6f25f2f","validatorIndex":"0x1e4e2c","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102a465"},{"index":"0x6f25f30","validatorIndex":"0x1e4e2d","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1025f7a"},{"index":"0x6f25f31","validatorIndex":"0x1e4e2e","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028ac2"},{"index":"0x6f25f32","validatorIndex":"0x1e4e2f","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102bec7"},{"index":"0x6f25f33","validatorIndex":"0x1e4e30","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102ad42"},{"index":"0x6f25f34","validatorIndex":"0x1e4e31","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102ce7f"},{"index":"0x6f25f35","validatorIndex":"0x1e4e32","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1023b90"},{"index":"0x6f25f36","validatorIndex":"0x1e4e33","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028160"},{"index":"0x6f25f37","validatorIndex":"0x1e4e34","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028248"},{"index":"0x6f25f38","validatorIndex":"0x1e4e35","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102bd9d"},{"index":"0x6f25f39","validatorIndex":"0x1e4e36","address":"0x2b78035514401ed1592eb691b8673a93edf97470","amount":"0x1019300"},{"index":"0x6f25f3a","validatorIndex":"0x1e4e37","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102b5c7"}]}
2026-01-26T13:02:35.985084Z ERROR alloy_provider::blocks: failed to fetch block number=24319172 err=deserialization error: missing field `timestampMillis` at line 1 column 34200
{"hash":"0x7d91cb46dad30faec499a8fda29b7324960bcc22c44605e84707ad091f1308d9","parentHash":"0xdb4a590f3f9551c3452022277c58eb346161372a5ba4d08864b99e43569c9687","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","miner":"0xdadb0d80178819f2319190d340ce9a924f783711","stateRoot":"0xd5e2c6730336ebfaa9144a22632c1a947bf3300b258a953f20c8bb6af4973e01","transactionsRoot":"0x6aeaca2236e534528c481ec9e44495627e8780bfbf88b95264c271958e14828f","receiptsRoot":"0xc61e40c7adff92b0675ca30de878a2e631d75759bfa8e688fc668de4370be339","logsBloom":"0x3bafdb75ef6f3bfb7cf7ff3fdfdd975f973af7bef57f1d7debe3fbfe7ff6adffbd9ff5feabfdfbeb7f9ffff5c98ef7fe7ffffb7efe8abfec6dfcffefbc3f6bd5ffb7bfb678dbf16bfb37ffaaf7e9d9e7ff7fdbd7ff757fad7e2ebfdeda67feceff7f3f4acfbeae7c3e96dffaaf19fffff6fb6ebe2fd39fdea7fb4bf7fbfe742fef7af7d97b9fcdee5d7f3fe393f76f5e3537feef7ff6bbde8dfffcff72bf3f696ff79c7b72dbf7bdfaf1fff2fd77dedf77bf4bffbcfcf7beb6fd7f6ffdfa7fffffaa7ecef9bfbee51ffbffdffe7e7d2b4f771eff7fdc84fd7cab71e7fb7feaefef776f6ff676f37fdfcff7bd7bd9cfd7fffeeffa6f3fcfcf9fdf36fff9bffcad","difficulty":"0x0","number":"0x17314c4","gasLimit":"0x3938700","gasUsed":"0x1da15fb","timestamp":"0x697765c7","extraData":"0x4275696c6465724e6574202842656176657229","mixHash":"0x29d3e3e306be592927326a034ef9b02976b572bb1bd758f05c9fbda5097cd2d6","nonce":"0x0000000000000000","baseFeePerGas":"0x5a4b85e","withdrawalsRoot":"0xe9d1a41a7567b5d1214ede07ef2c3231008f177652c5df5c00504ccd60344f00","blobGasUsed":"0x20000","excessBlobGas":"0xac22310","parentBeaconBlockRoot":"0xeec3e51c4c0f0684e06c658cbff199348e2cb7dc5cf64c7a403bef5d74b0fe80","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","size":"0x25150","uncles":[],"transactions":["0x7da0332438f5a28d7e98b4775fa7463fd666239fb7a359689cb3e55700bb5a07","0x54a6df485f0a743e814145e7b04f44d938d26175e3b8395d2924545c873cea09","0x9be91ddabf92f96a0c9eab6541045302bda556d14fb6d8d1acc2af1f916bbd9f","0x476088de76699f0e92ca90ce462ebe972df2460f0d38630c72bc408504461322","0x5772c848198753eaecea649e53d8d503eaaa85b66409ca30e7709b83f8b65a11","0xac7eca1144e08a773f9de8f2599ef2a5925768d115e9f36a73f3455d8779fb22","0x909f4c34e3f4486f08b61ee7258dd748f2e878fb7ea636b4ee9b9b5226f6f2d3","0xc4206ca84fdac8a6dabc7e3c39ae6da2070a2f92f96baa247aa10a0f696819e4","0x11e9a12b89492e838dbcec86e911118349bcc8fa550da8a26fc64507d41f03c9","0xeb1f8c5e0eef7425e39cc24220d5aaf669d6e0115d594cbd8501dbcaae0960ce","0x5cb9cbba3e5977cb2a486bffa672de11d0193687b1f09a3cb20e957479e744bd","0xef0ac338b7c4cfb5cce7a815a5387b891b70e050c6209fc226a010081be0f98a","0x28c97f9f82cb4d761b2fa1ab285454976c531465dc59085289b7df576beba711","0x86414a5fb5baa6dd76c5b9e995ee621257d637da34d74cdc5d719f72d84f1d54","0x29a98ac4fa1592447c9c003a60df5990067c00a8051f80007893db994cbe2945","0x88a063956fa69c1f1a5f0fa8a9ab3d70bdde58430f8fa4c12d409c876526c9f4","0x13275151a2ba330d6973848dfaa5a99a87a2456e270e354df07187f6a4eaf91e","0x91dce550dc2605f2692e7819fe2384f59bda0e4aa4ebf25f43dc973e9a2075ea","0x87a61eef31121605f42d3c84c4ddba4f6e44a3c3dcb1a43061484d1fb2897fbe","0xf726460adaccb16898123e1dc377c2cb5123d711029a41a067c90340c03a3049","0x97cea4d20cc8a94e71c4bbaaa3d5ff271e285c18d2a65d2ea094f80b4907d551","0xc872fa2375842fee1618c6ab79290c4cdc4d697cc924ac87c6a3dff236fa1326","0x5447973d97bb5b7dfe4d7137eb24d95254bc7b89a0431da2d58a708039e93ab1","0x68e019042edbb4665502c25ec91b690ef69eca52f27765f49298db0733471128","0x832b2cec5cb1e4719269e809d791f950a83de39e460518dc8d13dbe973b4c902","0x7f6b94cbcc6489225e17f02baf4c38be5cf45f2afc164153a877ec4a7227b0bd","0x04d53438d8b753d4d78ddcfdcd86390146dd5fdfde36ca434e3244d7b92ebaf2","0xd3cf4107d41e0d14e346e9947516723340f19cb434f130947c1ace24441d6789","0x1f18a3e7b37e9cc5751ce3d9e7d2809df30a0e283c43cb60a4cafd02b8656bee","0xaa7e53640e7db774c61f05db43496af6c61c70985435066cd8a2cadcd6692f13","0xf67e9861194786ee4da24e5530a77201ede37bada52c366cccfc5caa7eb138dc","0x18d06583aae6916c511f7ff66a5554bff89a1324bc1947528602edbe50715dbb","0x8504598e9fffd49779a7657d30a01584fb9b3e2a9ea56a10c24fb76708e3a92f","0x256b0609ac5162d1769fb2e5834506a944876756af09ccce6bc9a0d8fdb0d345","0x59f652aa843925b1d7299a6980d2ac26cb13f37430f5e7fba6a6d1079bcbc792","0xc684f63cfee3084f0c93d35d226b46193902d1b86e64c58c30bea47cdb28acbc","0x1b4781b474f3e61488558b2cd9cc1a1d5b82b702decf9babe6e8c34cef56ae05","0xefea2adb3c9e45247f7839ddc20dba7990827771f8ca3c679e0e70255c18c41f","0x68cb007166ea5b195c980e42fde91a8eb1daca5e6363a73abf2c26ca74d618e0","0x381fa1b728f2fbafbb116ab3c50c75b4deec82e07b92b20af3bede02108250df","0x427a0eee435c640d3fe32d72a57e475010a6f90328e312eb99ce3b2e6ecaad74","0x267907e5a7c02c6f3d7c78298f643866d73c12ec2ff1b2ddf062eefdd87c0a9b","0x6ef66417f48ab529b3734ddcba893cef8d6be228abafb0d95600550b018d2056","0x99af39be7b21d1758c4d43b2961f4260d61611a3bc0a9ddf9e0452b554dcdc65","0xbd3943ab6070e8e80dcac624b8a701d6b3d6e2ea9b3b625a39e6e027e9b17649","0x2127d4287e45a78b33d288f025061099a0dda8d12dd6605f92021ab390aa0e75","0xafc3bc1fdc0a8d7b45083a0c1014e2771443f0744a43f6dca0f58ca3e440182f","0xa89afae27ee1ae642ad1394064d9434b102a90dc5e91000cfa58bb98467ebee7","0x46fb5eb1c231f2fa790d5c1bec9b79e35ea27c2c944078324719eab569f2881c","0xf83a7d1d352305d420eb9a49abf920500d74cf44f32f7f9ffbf2f5c6b2b19cb0","0x0072136ac94c4ad59f4a7fdaa657305553f642fd5d13ddf466c6572be4439bc3","0x2d36d5fa147d5797c9cbce53afc1bbe659a42b01c16dbcdffbb6632ad6b83af6","0x801bbec7d9112b42073d9aec4aaa28afac64973fab95abe423f7fa9bbd435ff2","0x4418bbd6d1c5f8185fe314d0b0725a4e132cafbfac737d8e469f81d470ec4296","0x876a7bfafb9e677e74424c2a35bb4e063181cc0f45265e18c86ec67e16774ad1","0xd250b5de166c7364b57984cf1bff23a808aca0b6ba53f8beca4338ea35a2889b","0x355a361783f42435ac257cd59a8b0ad3b434501e1c2159d7ae37d97125e0da58","0x6adb8911fa522dab01042f8566feb59ffd9dcc2fea9ecdf0f2c2c6d34e638268","0x18d31233f5c21163f3ca5f79d2fdd01fe92ac9ebe32e01cee947699da0cfd1ab","0x9658af7a0d84181e6c5744148e0d036e05552be1fb8de1983157ef41e1dcdcca","0x9c9e88f22055461346ab73e7fd07fb4959403850c054beeef28db9d25f9f254d","0xf2c11826e8fe78cb07973001e786e46d3a981eecc625fc7df6b804050bf5bc09","0xa0b013d68f670a941208fe8d6cb571531badf1e7e3810fd3ef759eecfb5cab39","0xc9d027f50dfe9941a58ea4f42e305492fd0ac62d2f22b3171f8023a8844eb1c9","0x7a79706e229386e8c36b0ed84959704cb264f4e55610a175bb387acb732efad4","0xe4d612450b6401f4aad90a00488815fd7ae81c8fe305b117333a2f4ec5d59ad3","0x913f8a18c282a76e11db1548e58ce3c59598785e1a8018444262edb4038db516","0xa67d671fdff2ecd4bafdb42698cd315c815cdc95d05bd4f4f82a0c1c017d2010","0x7971c3855248e2a8e93451cb477ced601a03357efbc06ebb7e404aa53e30f4c0","0x10efccd84550d516f567038541b875130d75f599eb85e317ff8cb0856f2aea49","0xe36e9d2173b855a1d2d039d1c6bc74f28d1b895f86fe6e0b5a559ea1596a8eb9","0xfb9f2c1121e0235099de117b9e49a56eb2e6674bb984f7d2d33bda183fb36c36","0x58cc9bc9c49da1645a271ad691cc4ccd0824f8cbd987beb3543aa35b9dc5c875","0x27f71e069b78886be01248b6364fe70623b9d59808723974869db47a24def2a1","0x5b0efc296daf30584ff0f9f9b29ea3dc0474f6747d39943b27a3d6d540c71022","0x44930d392f84137afb9cb5e9fae4c2c29064df6755f24ce8d1aa9a1f15b86348","0x66ecf0f2fa8fb3e6aaafc6cf43e4553262d57016759e6db8e166fc392a7ef23b","0x5350b29621fb18b5d8302905f6a5acda66c9b5c77877c537c4c8c7b0685d3b3a","0x7764fbafd7c3d06da25a51d2a7dfbab6ef25d346d785cefd765a15ab6ebfc734","0x470f8adb576dab056ed44e5dca68a4c5b5ea41efb919b4d16740ca253d8b9c66","0x75f1804edf61df3329e9fdd2c5f5f27075822258e5f0c7a00e114903bd5d1810","0x2de48e483c6baf3ce36f8a3037ca3336e17ca551ee7922963c3a670943ba019c","0xe6ae3928baeff3a79bade2f85f01201c8b8b3ade41139b6f5160cceaf3895bf8","0x70df6ca8fa5ef026fda28149070269a9a0dd29eaaf8c8d6beb70968754a17941","0x45a935056a01b6eedd0597b1079f9f254ae14193d456d62d93ecbb2edf74b4c3","0x202282a8d7ff6fe344275012a14bef74b2f2cfa9f46109e04f15af9ab0e7db9e","0xae41532418dd6a8d3ec7a570e69ec261746d8830141ebc30790cb9cb5e90ca8f","0xaaa23c7870d5ddfffa260e4fbb4fe69fc14eace85fc64055a92b75c5adebcccb","0x36c6967b73381391f3a06ebfba8855adad1f35e44f00a2bcbb8db902fb970119","0xb3aeb55075a522fe1745afbac31ccb2f3148967fd1f357cc3636920896f9f53f","0x00be9e533f0c2b3e5222e15badd04854003519d49253457a041533fd4b152509","0xb091875dd3e941432ca38483861a46de3d4b0ceb70519958c03e16329571b766","0x313404ae53b97fde722919f1edde745f6758be5587d50cbf5aa9fdcd067a3027","0x3030f0892896f78c8aec183ecb0f710c993f931ed735ceea36c0aef50005b4a1","0x89ace278c0d197fdd271e40dd0a9bdc88bba9212b233159939bf8ec35992a736","0xbec113ccda0cbd521620316e5cdd75deeca7df15d2ac11c9cd6ad0659c24bb26","0xd742a6d72983eee114974a31e7e30d7c97eb243b001c757ae4c830ffbf7da070","0x36ca92bbfd274cbe5f024ba47af8f7e9d97d38be13584d79634c5c014bcb36fc","0xaaabb898a4e8dced5fea9682184cf6e8163219eac686998aee9a7110b9b189df","0x7576ce7c907ecb3ea2bcdc88bd2066ec202f1d07258f7637da7ef1f2391f7d33","0xebe75e3e767fae4fdf3d1d610919c2f128dcdf55a197aab9d5f3a0dfb0bc56bb","0x80b9873c061b7ebf6a7468ea735a224297cfe22ea299854fd8482bac7690853b","0x7de71ea307c193c953f8e334bdc4aca3e0b8afd69269e9bae274fe277c72be0c","0x7cf1a2fceb4621efe94037d341f61bb3c9c0a86f2542b5886e67b4f8dc55233b","0x08950ef0bf43354a86fe6aadd3cc930b329d021e66785a1795fc7e9c6407d396","0x7afed816ad88aa0619bdbb43f1648317026dd101d468e08f1c85a2a5f24f59e2","0xe5b0dc0b521852e5a05ee99e4f19df383e2637c0b78168fe2ca848be35617236","0xd5cacb8f9315ca7b44648744e13a0df2cf240c690b9b8a51bea2ae577653b7a8","0x4b768291e3cf8fb611f83dd18886a4fe630a0bba521e53a350e80d94e0102a98","0x5828141658b6b7fb9e29a2758fa93bfbcb3f1bcd84a512eee0133383ac6094dd","0x52436e26dd3c11a05f03b7ba3793062c081fe11b2671092d4ce6b282ca321f80","0xe985d99674eac2251791e07b5f49073fe570d52500fe08b72178c2e9039b20f3","0xebc77d9a47d430a1a738ca991857329f0dfb6dd39092e922eee42079b2dbfdc6","0x4fcb9a2396df05ac4bd13949544e1100ba628a7472707156247cc0e0e2ff00b5","0x98204094bba370db4561f094687ab940991a0fbd873dc7288f714d881a158a6f","0xc59a6684b7c683bef9dc676e026899ad7e91f689c3fa327517acc5e9613d1a0c","0x61a440ec10afea0c8fcceb0f1b536ff43106686440dbc8e6a46917c06d872c9c","0x7703376737644876c62a4cb6c931717cdff533388eb717396f17f7d9eda98a1c","0x10b7f8f5454a23dcfd2089ef0a51eade14cab59b7e85354a1976bffd30d82343","0xcb108c105c13f00e48f4b2f0e50b5638641ec7b25b59ea36f438cba5b8b97dc2","0xd37f477fe5012d6bcd0e32d9c8b9661f40398c88987b61bea70ed8aec28c20d4","0x04bb7c5e46d518bb0153e7f49e433d217647222682603d9aaa837fc0db09854c","0x1f77a6fec4af4b1ffb6bac293aaf1838280e3133212ac57f4e487fcc07faf20a","0xf56941462c337092cb0a048273648a2447d2d9b65b8967e9adb110b08a4e4686","0x0f349910952f1c610d375dfdd2ed4a939b759df0c6a543b1f26f3924822044da","0x6f8d896f44a595f34d18649d6cbca1bd64adb0d8c09cccb0ca917b6bbe32f2c0","0x92d6c3acff83cf971c1ececc3c375a2096755028274c7155df394187ee729303","0xfc4024e1c637a9b275cea7601b1f2f5999ba5456132e7754320acbf8a8326d10","0x6171bbfa625e515ef0c13ed59c070d137f832f7aaa349b24799c05256c22cc8e","0x07e1cad114c28ebfb24f28a45f22dc11f2344bacca936ee4715c5e17260673cb","0x3bcc906f70c32152677b2be872b4bd32bd42e7626fcf39c6093413c106d3055b","0x0b7127d0b8676c5563e651b3eb6f4dc26c287186e9fc84ef5e40ea929348f936","0xa2a024a7fe1bac82111b895f8d357e41d96fc207749d136382a25fcbc86e7426","0x0c3fe0329b174c46b7375b5db4d60a77fed77627233187667aa427d00986fd71","0xe72842f938d907172ac70fbca81153bc4132f0adb5b4cf970406af21c1fb241c","0x626ba513d82435d6e34594d5b10e528be2921b60df4d82926eff04ecdd312a8d","0x6c2d5a8f17a2943b8c25d300a8deec6eaabbcb06f735355d56467aa8a7ff86d1","0x4126628321f172d0dacfd3c23d201f0d2c990a1b171565d473f210d484a72f09","0xa9c6d3d32c2ee8cfbb9d71c0e65b0950394c0458344d16de1bf170a89367e7b8","0x1c2c098d47002d0c1662ef597c681ea0097f5501b8ee3c0d1122b3c103b1bb90","0x69a671999b11ad40bc3ae847714e7946994045e38da1d7789c8b622bd4865fe4","0x173365b4c83840cc54e69925a748b7698486d2dfaa1e423e45f6cea9bf59bdf5","0x1b29757b3cc9f34316217d47573596b238d7389d343f069fcba459482481fdd9","0x386c35e36d130cd15fe49f315403bde5720df43602c2f84c7c2c46322a6375d5","0xd2905acd97fa5feeb442597b93e62d71ae80cc0259fd04b26bb7253e760388fa","0xd35126b45140211a32fa9f2dfe4986f31d06fea31e37f2d7608f2b5aa7f827b8","0x97fb5c429934245143bcc6b8fe62163550c70af60c01ba3425aa7d16ec56163f","0x3d08ef02ee11bedef0343c086d4b82ee48856f042089c5e2791ae3da592ef627","0x6280d95459932268ed3dd4e39b10580fef4049b563a803f68b3be95eec7d88dd","0x3441067a209cc181b7216ff528f96b6e51bada41ead73ad44e5bc1c1f4c0f93a","0xf941dc557a6bc9cc3321f89f87893e4fbc62c5a64c293d2c79460d8fc2e6d060","0xad21e8e21135ba2ac003824d3837a94ba245ac5b8e210c97c11bd5ce9670a76a","0x2b175a41288bcfb01fc3716abf4e761d45354119f3e5a4d09475b9a5d0f2a606","0xbb3cdf3bd015411645903eec4334d29446ef39cc489467dbd555f8a69f5ee390","0x14e6adec27f7abf0bc145d51c2d109028a5247b526603bae90466ac71b11ba95","0xfe4510fe9a2448e9accc36d6674b61b768f3077e651dda6239da95f0c0811167","0x586963cf264978fcf5c29035a751240ba026c4dcb399a1a97745616815ef8e95","0x209f5f7b665e164fa0934f27c20487dfc58875ef61918331c5bf3ade8e281503","0x5c54c500eb796203b107bd7834391db7e755766fce3ac11d874b944d8d8cd255","0x32bbdc03ea6ebb846b57b46e902fd79654f714833cb7afbb5dfe3b17c63d7eed","0x38d8170a6d8d1172378e40412e984d2e7176c4a7bc9978856715e08c0c23aab8","0x5142cff853827593e9d5a89fdbddbb870b13a067459abc6749c8973862157776","0x9ce8567835f6771bfeac1d208b44a7baffe56618e1f200f778c96234ca7a0905","0x58cf765e7f35766c44a81ca4a6518fff6e92acdc042eba077757742647d34bb2","0xf8ecf6092ca27cb8bf6ee41fa64a6765830d89f17a004df6884c1c8a9e032c51","0x3ffcd30d41054859c4c0e0f7ca8b4dc12ac071f6051d3612ebec2ff114c1ce43","0xc95fa6c7bfa73a56ea53d326fe50edd03227225c9fcaced1f4372521cfd2696f","0x2fd1526b9a9ebe25ab642c5066966a9a1f79ab63150795a4f2c4083eb22816ee","0x3698e5aa70fbe6a1f897b27831f23a5e1342a6a05b1c83692ada3e91cd9c74b1","0x31c6d067089c6e030b5943bb57c0371c4c96733c584075748115d48c814ec299","0x74dacf342b197bdebfd86195e1394087e7f2e21ebfa8ceb84e95672998c93e66","0xfba01a302ffec7c48380359bbaae3e6cc2e2efff2c7c40aec155e48018ff6022","0xb6743d74ae787c087830f5675b803ab918424905d0f8e88bf2202d12095347e3","0x84e47b78c5358cbd6812a39bef6ef985b998c07e04713ca4281fddcbdfbf2a65","0xf57ba30f8eaebcbe986808d0a0e9bb7e9518fccc3f230391a55390c03f39c1e9","0x94c89c89fc2cdf7eb15c08c46a9e9b9e675c44c9eaf07e5e96dfefc7a2a4115c","0x52bfa4d2c0f4eea1cb7015e978923ad49390d6ab95ff8d56c242bab1f5c06089","0x4996e5042e4ff4eb71fd454f251ab29517469232960dae71d886342504bd8591","0x1efbaf0f074bfe02d0fa40a70e9feaac82f457b7d85bc19668dd433047be5848","0x1a9a76c69ef0f5e8c4d154774a27421be990af7aaa7514b7f83c24de9db48613","0x9ce79de5010ae9d4b6617811903d1ca8e7b6e63b19406379ff75811c3db55d9a","0x306704adce0695004f960cac0807ea5487cd0af50c1e68f3ded28d0dbe1beac1","0xc97bede5020ae95dae866dbd59b0321622e8ea22451df6940e2d77e1a8a791bf","0x3d1efce4b7e8dc5322f18d01dbdb643561ef6f1006ffa75830a8f15c981c537d","0x919dd0df9917b209a52edfd6d975d1b05ea4bcb561cd2531960be315a0009a59","0x0502016a3947d62e0625f3e9254e6ef94c1d73a855ed9122ddfdb7b6d5cc6261","0xda97e7279dfaa98d87cb747b170ff38cb10a3f12076e77d6e2c9adb6c302b589","0x3f297ba769bb17cba032fc8f1a04bd30a3fc2fcb27e3d87e892c136851c7d911","0xb80589757878f06e791fd440df8ef98b67bfe5dc5be948e088f7a2c39d750da0","0x053886e7bd9f39f87c541e82944d6ef087dc552c9e215910bed0936f7ad2c7a0","0x748a710ccd2f406aafc639b4ce26ae3c5c4e5b583eadad00cd3a4980c57806f7","0xf1d29128237f3dcbe6d6843d08eb237025a9dfba8ccdc6f33b3d9e6d91c4e46f","0x169f6c1d9d6e95c486e217434aa18923ebca2264cb968c65709d17c09de0cacd","0xc6eb4ad32372e8b02150279a9f3b9c14ad8fd3495f9e403489527d4137725b9f","0x7775e2d7d229a3927e571a71829a21babba4af0903a03246978578c8dd1d3910","0x0c707b170d0fb3b25ff30b4a0f516657659ff74ff55b8fe3824ef7f6af107491","0x00e9adfba2e2e601c05c1f932af627353bc68440fbed91702a15d9271cc97c8f","0x3d43dd6ac008b50cf873c2f8dc83c57295b9b14f58e44f74c89a20f59cb4753e","0xb53aa4be124508b9a31196a50f7288a13603ed54841b10cd5a4419c9aeec1a60","0xda74cafab5cbe1bab449770aa0ec84e11477deb9321af72dcbd47b56b3e9adcd","0x949f7888f72bed86ada87a5e15f8c8fe491e23f6d1dd700c44e15bcf37e599e5","0x9bd80e618045ff55c1570e85e65ad1d4211eab4699a258fd3e6570ac6ff57152","0x942661716de2b58f7dde35ed4ffe38b9f89a8de1048d31241ae00a0928f2a943","0x4080fc450cdeab3c42ef338a936b72315df9da40910d6efc24a2fdb1aa30c240","0xc5ec8a55820cc71e2ae1bc7f15e4324234dc55924192244ccb8477e09c7d49e2","0xf84ac5ae0eb20d6172ed230ed8ffd9c0ea493f4d86185223ff1951e3f2c29ded","0x06bad7425d79b732c0b52f57589865ed1a875be2d6bd58fce22377a0f6218f5a","0x3205e71b3923c4b96c16033895cb1f4849ef5205e1c9c0f38cfd43d09ceeefc0","0xc31d86674ba7862ae31156972f22ff0e2e3dfdbdd3219b88f26f215a6461c8ac","0xb75ff594ddd96f4624d8661789bf3ebbb00a8c485fae86ebcf014e89c576a15c","0x80877fbd26cc3d3aed41592f8381e5e8c437c46a2a38bf33e2c4f298e5967b86","0xccf826b769e88117946897daf847f5a64807af648979391070975253eac5b3a0","0x4dff4976cfa502c30ae2f72ae197e715c7fffda5828709d8f034a8bf1f0697c8","0x0db150105485a4d4614350427bbf1a1b5810f571f0cb73936b15e29d460bd626","0xb0b8d74863327e87c743c29c21f7d3dc9913bff9c1e2501ea9ca8e409bcd285d","0x0e05d64e4b1515c5c3402ea10ab96461ff90a6af4bc94b0a0284b9c3585bde52","0xc0e1509461775da937aedda299ed3ead13aefdfda7bd24e15762c27a5391211a","0x74327d6bc92ab8f0a15b53c09707ff401894ccda485898114eeb3b9ebbca520b","0x4696c86bb1036b631bb11d43110316cdbfdfead51d88bc98b3c6ebdac2105ac0","0x1db18d88ceddec52b5ebfbae479a43709a0edb9d48d6d1e94b47e1d4c8d9f3d7","0xf704a158fb22cacc2f9894975ade99f1477f5f118931aceb58aac40e1cf2a976","0xe42021eb1851a605ae623bbf560a63d0c94afc902569b74caf403d8a3f847add","0xc0d10e511ad4451517a176312ef02425461d4aa1bb905ff096bdd966b20cafb1","0x7a0473cdbf14f2a000fac76755aab3acfedf622b8f2ecf056ac9df370ff3735d","0x4accc3ee5232480e0f672d148d743731debce9d218c7c21ab6c7980278c0d6f3","0x45e3788f4abdeac571a82f678e87787a318d3301f1c9a519651f0f9cb0848506","0x2452d1cdb64572cc007e8f24f74f41e6e338ff57b67b41bb862aa2992a72b877","0x23a5492bb1f638d5c34894a84b940650fb115a2718f31fa6753fd79bc77a427c","0x1523133ac5873fee7a7ac2edd518855bf3015b69d11dadc3dc3c3d5fdcfac740","0x08edc845153ff57511cf6c03748f0e904f29e8627d0e36a592c531ca830f8f1a","0x07c1069af699c3f610d3912c77f55c5aefb7ec8ab41f88591cb5f6ad05284ed6","0xf46cb6bc222c2259ebd738518ecc09c88550166e814d0b24a6fb20f1a8bb1bce","0xbd391da87e041a84754c0175ff8a139993e11d5518443243fbc174abb64039e5","0x0928d74e5b66234eab32e3fc1422b303c101be09ea46880af5c49a786f7ddfcc","0xc45afab4a99a949e48ad64f49706639a3b013c374c93989646df3ab11d8d6722","0xd988f3258990e137b121a33b3db246ba99faf2ee6bef827b7c437bb6e22b823e","0x53c1647c2439687681e60eb400e73ab2e02721732d4256847d171989e17084e1","0xa33e2f6d260a1b6a6fd8a91bd6c70eb0ef7f9719d4c33b138721db6666f7a19c","0xf60b959984120dfb9271af47de797e2448a66999d466236a2f85d000ba39c288","0x379ed1416d2b090636d6e06f9d0eba4eb4d08e7c060469447b2da081474da3d2","0xf09401db2ba16a44f0f00cfe9036f61b3fb1615bfc6793936d16d6fc8a8820ee","0xef26ba7c4744fc5af2756de860c718c77212f4233401a15d593354e0867ac4f4","0x2fb73d97274378a5a7783c08a51fe77465041a212e7ae9e7eba4b17f53f8d97e","0xf9877cf404ab3bb0457bed3d7fadd3b6c8c0bda47c916a2445d80d676559f09f","0x97aebe3a33d1c453f2b6def43193f13d49738802cc2d3232823f04052d764ce4","0x0e0a16c41774fc0eda2ac97ac8e7d36892599a96a918a26ab193f1536b5872e6","0x205eee7fe0746fc30b4a16c4934fd2e4a70ece3899d39c0dafffccb65f274a57","0xcdd1f26a842f2493403ef8bba5d850046e3e7a5fad6449caca21989b5b24b255","0x61dc9baa478b938744847c406d9e40cb11939a6d5276a67763d95c1361b450b3","0xf7bc6de297526dc541ff69d4bd3eefbc9a2f0c3c55be44a132cc5b7fadb0d541","0xee65d3d03d9e530803975d10fdcba1929c80cec143da32641f33b246202136c8","0xed6ea826c0293480942c399862d27e6b10c8c1af78c274bbe8cf47ad6881b29a","0xeb2765b0cc64201feab87095532944641b5ccd25682941f2a1ae3edb8b876620","0xe8f5309b293eeeadc50c9a778e5145ec9e862aabd2e5fcda7dbae4ffb2715e14","0xe5a60964e3a54de168066baef336e64de8c57290a08858bf7b4d3cbbd3691a02","0xe2239ce48aee6811c6398aa422934070811b0204ad807cb9d635de7eafec41a5","0xde114a8d5a29c887d24f08c76e8af2ea940d685a3c372ccf76637a072bdf4fb8","0xc70e61baedb3f3a4016ec0e14d662f0375cd99b8f7e414b9069305bbabd63fcd","0xbc92ba9c405f33e9a787ec58730905829af3bd2fbd188c086b93420d8ee4c98c","0xae1e0e82d03679cc32799584276b557783fc83aba74503ed009bc3ab43638885","0xaa4cec8efe903839e5ccc960c86063fdb79533448d859678a71e9aba141d185b","0x882b903948a78cbed2cd59583a074edd564df3f18b33b74dc3704f94401eb117","0x81c35fe6619951c81728dcf24e785d47f911e016b848f6e18a831232012d2883","0x7db2abeb717c36b69153bfe3d6bf1ea0b6492964c1f59b670bccd05f49338d5c","0x7332ac4b5048d16b55481870d11d0fe5d83999b7d0080944e59d7e1b34ab1936","0x552cf3a195d78ce03f83c651c0d72cae7483cdee876bfaa31e458dbcb5277896","0x4c54d65996e3dfccf2c2ab8a3bb9dbea6e75a2679cfa512a60e9f3a091a9cc66","0x4c13bc04c407860931b16fabc06bc818d776fe50d9fd8ac1e99c8284af321f7c","0x3e6fb22716250baa7bd38c3bfd11c2a66e3f720c1d49ab69a018d6a698bd8ec4","0x3d1d52156787dc87bacd072d5a77681fa969a4e62d63b018172b30d85a5f4644","0x37ee64d7e4c9d26717a03db0a4f4fc5c8468530817c8aed19133a5c522d97946","0x1d207ae5baffd89a2090c09a4f031fd4885e00419d7d08ec535816862705c05d","0x091e65e42fad0aaa635dbe35e849eb744a300f719b83739142f990b37f8a5ace","0x464a94911c3d726ae84c3631afb1953a03e19cdaec6f6cd810c0f8e33d30d468","0x6e3043b39c0986fa707115c24b6b20f4805c01536c8ae0c457c8f0c7d4ab6d5e","0xc68153d72184a2a6b43403caf3d948f2f42d433e61589831608b513f6ef1ceab","0x47ad1587a15c9506626b41fe290bc33013ed7b45e295c818e8e19fb1771b74bc","0x99c35f9b215c8e570de37053d401ab7ca96f388ef0198f8ffc43935258303073","0x89f089579a58a2908e993e8ec003263b502ddba8aa4fef87c675f11ccd756322","0xe71dbf6e082c599ee23f46739f6976070251d380c9d9e9bd547447c0e26715d4","0x5f561796cf161ed6781a75ae2f23eea3218bd0cbcbdd22bc0361ae07e0c7858a","0x701a932fce0cf63690c4ac53b55b96a0b80a19a899427c39247fe9c372107ead","0xacd31a9b1d727880500422fb4eda57ef2a368e1187a1b4442a58abe928b843d6","0xfbdf6bb1974ed8a962259b197554ce65d70f5ae7e61b57d3216fc8d14bc24172","0x217858ff4387e033a9f7cdab885fbc2e4161a5e06acdb4d2a24e63231b14c7d5","0x742fb3c83eb34276cb8b8f9ed2253bbdcf840910500e463994d46e79c39519bb","0xf5fb520667438863503fc9463580c4f7eef0b9645d53ab5550b7cde7cf177429","0xdd58a8d23e438e4dabfffc8661cb45ae9d5f9d61818b445e44b347edba75ba83","0xfe41f152fd1ef3104e270d7cbc13ab20332075cb34e2fa15aad509b87a8ae498","0x857618d524ac9c3ae931bc24d6531d05b72f61628f84ba093d5c0a6c64a28fb0","0x178499e6444b4ea9fdce6263fbc8410a83f338db33a966aaefc66f42ee9f88ad","0x1d787a8c6e57791c60c9cda67bc74088573d02aff35897ab5b2a29ea4281bc06","0x722a63a9ada082e133acabc58838af3023de8050684a51fb57f1a9ed7274fc23","0x7fa7ead20c9226be2ea33c0d61c4efc58ca3766644c1ffe231fcdc14d0f19a0d","0xde1a97cde275c36fb386fdf4f1073c42ef7101046dda967db437ce04deadb23f","0x1abac5b7dd948a9caf655d4b17d8ff32f773230def3c98703539c385733e9ba1","0xc5522f500b62f2922bd2ad98128dd22f7abb3277debfd7ee497eb381652a863b","0xab369cef5926de17f6980f40945e19bdfcf559a26594fca3bf1804a25dca37f1","0xfe87935a356c8d198a2129bf98a809472de962b6feade49055d4521dad19c5fd","0xe325328cb812a26edcc9031b2ae8c2f809461ebab438f09b3a8d08912006d795","0x78b0588de50104d5f99bb1a6a02dfcf132bff0d9ebd84cd29263e59a8f7f2322","0x8584acaee619468fed21aa564a622cc0e95672eaa28e91f81914b7b5651c2401","0x7a3f5855d8a484796809b0836f7b19531be221981394ba48aec87068d043b8a4","0x5bd3bbda7e7c4a1a2e926668a29576950998eb4864c5641e3f4f8aa128524e4e","0x2cb0f5d3bf992ca9c674d9abc0e1600625fd371741b93b720c5a67151f33248b","0x0a1edd6181b6929f2f305b82e38b33cac643c1c56819582949d65c6bb71d08c0","0x445d9ed258881f68b243d43a5a35cbb0e90706bacd4cd134bf4e9e680bc77253","0xd86b917db83bc3e34c34583364a6aff4846cbe6cd88658eea4aed476b9c4efd4","0x5a957ab41af5988a92a4bb18a5b2707b699cb5aeb08110b22af9a238d6f944d2","0xff4d70d3e6e33c3497a6fcc0966c7d8e66999a75464596c1ffe25074a5e08719","0x06c76aed0ae6199aa3d73e3f5ada378b553141a11fb1ba3601247b4a6cf02a88","0x99b53aad4781b47a9021434643e1a61d143c930c9508b1ec7015e129d7aeccf7","0xe8b9d2c763243e0bf00d0e7b1964a90b8638abb7ddc6b62e59058cc65206ef27","0x0772880ec14dc6be5cdde2462e962a1604814ee6e171bb223764d99e247696e3","0x9651dc5859e82fa8311c5954415056bfc9c03258e92da67bbb7f005524feb892","0x987e090cdee42f38535dfff0aad1599eb9b884a674d154adcf59285931bc28c3","0x293790e60d520cb61ad1d736a2e8043e61e10c637ecb52c3c0df26b340f3c6a7","0xff111607a91a2c229d1c94bde901a55e46022c6e9a2f541abeabb3992f45e784","0x675fbbe8739f7cdb88af4b7f2c243a90c883ea6cfc5ce03fbdec71d54c9c6fd9","0x4f253751691a1e0c0540fff14f8207341bd1c105d71d58bcd20e3dffc312b112","0xfbfd363dd62097ed5c33eea8eeccd2da8c8f957bbdd11183e5133278121ec129","0xb2ecfb319f296e2131fd8013921313b91c77e66b43647c51e648760e84339cce","0x9ef07796d8abe5ac9183b240f79ca2034276ff30b8fcde4ed6daf512a6badc24","0x0a911fc548b57ad1a62c54cdc0eaefa7375122a49321e553d88ed1af2357d1d8","0x97e09e44e6cb1adba32a9f9615a65fce3adcc0d342f61a1c44e12fc224b33cfd","0xd4823e1b159ef885c72e810a8d6cfd47fc97ede15d4c0bfdc2b3a1a4859f9dac","0x050c59cac353c92fe2cf27ae4dfa8633f11f6093c82f82ea3a2c5872c3c5de00","0x9c180859a913fd936f662cb69dad5ea809943961befdd0e9788bf0f947aab50e","0xe5325d27f11870aa8b831fc99faaf5b601ca7f0e84bd106cbc542992717dfc18","0xeba55194338912940e3c469a30a7c1692d98055c6544f8eddfba9577000b86bc","0xf78246a7aaff42f040569dd09b8b172562d8223f1c1486f6db50d5224f5d344b","0x2d954241890cd9a60a905ba86973aa46436bc31e066da0b86453e2ea53157418","0x27746023e445650ad895a984a80b5876ba4fb8d8beaa18304458348659b2e47c","0xeafe17d6cc4967ac5b9a300d9dbdb2ab9459de749033dd078b7e3c238d7b8989","0x93ddfce56f18ad7e91ed93cbc969989d46543255c9be96a27c9267461d94e0c1","0xc6bac08a174928f49b73ba4b910b37ff8af3311b84b32b324afff146e8149108","0x598874d39b2c812778d206dee5ff9f506d8c113b977b260f106e06e5ea387b0d","0x76c768884d4e5dc5f7e8d31d6d25d9bc6a3d2c19b4fe9c9100158166e7bc5b9e","0x065242090e5841764aa35fde909217b6d534c537c51cae2a72356fbd3db9f8ab","0xaaa928bd2926727b85a99fc6eb2c1990dc3f72c806e8212156b18d3f88ee406b","0x135508155e034f6946d09b7d16ee93ceb48f7d676e7901fdb317be682cce10bd","0x0b7477d669e7962ce3366ab3552f4a566ac964c2ac935bb4ec8deb8000245249","0x658c575750a89c4392e0c223848d4b64688e6013dbded0223d2e32a0c8d7aba6","0xc1a6cc7c7b8287f8b963211a95fd2432f60df45fa35d6fe9155d1b0361a6328d","0x39c68a15774d1bb0cf88a444b29db734f96a6dac6156c425087354c826a6a15b","0xfe22ea0c837e1a95e42cbf28c9e1fb5015d4019d4989ae6bbb7b94aa81b77af2","0xe7dca93d537d419ddeca9a56e3a20ab0537b48fcd0b018a61e40cd1fdac00f70","0xded415ad1618ca358bdce589e3d380f228ecf3e7f39126a15fa9198ff6cd7d5c","0xd7413d98d1dc6e83dddf80a23b12f886af32008c07cc279d0efe3133b75dc51e","0xca0e052ed2e32c04355eba82cdca6083bf17528c8d13e26383a385dfd1916748","0xb16f807522cf98d5832dc944e90f96e3819617d1e7639961460ce7bb7e54b135","0x9ff798cf96a76b7a80b7d0ec978f08fe61bdd4609a52e8d125b319305c52b314","0x9f8a8ee9a971592aa5e65c0aa56bbbf686ce2a8d1c840006033dcb22b516ad37","0x495cc50b22d0673a0ec01987a3a1419b641e4e936a14b0fe26e1a354fa5d156c","0x1a2c6691a56efe419b9d5aed0796c68de6e142c9fc604291bf456c6269339b43","0x1786cac25fe7479273c533f2b92c1dfd5d0c1a408d0078591c56e9cc43d0a590","0x0a5b9e0e7f5ab99dc0f054b39f44efae043ed7bcfd66b947d2e5151a1b01f552","0xdc597c86a2684c30f39daa31adb8a85957ac0797e0243e07cacc1dd4bab75191","0x742ec8859f9a0ba1df54f3f86ce1e37d15a8fb9d0580a5104f173d5bb2a636a5","0x51f24d57ef0320344d22048ac887b8d4e7441eb0a6e406bfee42f5f7ce5f854c","0x4b723f3617ae38a494b90f7953c504be5aa7f630319de20f4d3ec28156c919c3","0xfddce3743897d4829a6e436514e536964931f933455909fdfc7c68a917f194d3","0x78a44e16da0321bb1664e6a0c0245308f9e4192f24e0472d707d4ed94cf5e79d","0x46b50c2ed1653b3eac213eebe9f14cc18f5769d18649e94d85aaf601039b5c6c","0x1888de4370dc497e6605443759980219b793cdeda3fe1a16250fa6f9d5f16e8b","0x10b4a06abc0496b0151078138d25f039a2fc9a4653145d9438b6b6118ed5b102","0x0ad5580a17356159d9ca5f34ec05e37b76ab92433a2c4af102528c43dccbf3cb","0xc54cec6df649afaea08c2cc0056e1d7ba99700f7349102d8719e4b6c0c7ce134","0x6eea44957a0e056066bd4bec2e42e813add0808d0b2765352a1f4237f7f666ca","0x42d6dd3486354789fb189461f5cf56997c0bf66607128cd03d5d6da8368b4ad8","0xcd89e2bc0fab7eb47fcd3623c22fe69012a1d8581496a0b5e702ebc968a1b00f","0x3746d8574bce2cb56aa93b9a69e3c8a33a568d2ab7f88422839ad2df1a8feaf3","0x16d4a3387f8cb9363a4d302f70a8d951d41d253062914b90f38c9d14d8ec516e","0xc58bf8602998c1c06c6f2aaca5abe02912e8eb8e59202835eeae9150a4886d01","0x6f7d5e02d502316c7abaf1a7a910f923bb23c43dd2b904454c6086e9ee55389f","0x4f04d601d985604e60e4f13f519d683b0691497c7d67ef891a9b8dfd5cc5d297","0xfc9b4bb6e3997e06877cab021fa3b4a513b44b7cc3a14c3e6eb96b5b7c3a257e","0x670ca0158db028cefb207db589d68c1f0499c2a6c3f94ef05f00ef832418bb6e","0x6172364e2c9c1ead5779eaa7491940259d061e5a8a84ed52350b848d55f281ae","0xed6c022f2513a5b645d20336b826ceaebb5895b54f0c65a8f8ca2031f61b5843","0xa9becf1cc3b4280f6385e1f6dc70d93ef9597b36491341a4a93f3e108efca546","0x71c3d389bafd2ddfb501966ab24a72df52f5bba5e3431e592178b5bebeef0572","0xc73f3034f20954d825c7eee4bf48ad8a029e3eb4c91561370b0b7b5fdb031632","0x849c93ee0534de5026d98333d851ed9c1ab8c0533854bed5394471978a456c60","0x0b16d106a41a50d510b191e9eb33529ad64a1f242a69d434a1f4fe255d8777c8","0x2c4eb2cc9befb52d9881d5a713b5c4da9b1ade77dc97ff7d2f7518655dbe014d","0xed3ae3e5d27f20511687a85fd5821176e9763ee5c9c510a47122b54343d5fa30","0xdfa5ce9ff3ff101da79d7f745c5c12a331799be071b8b9beeb1a98f2ecdcf9da","0xd3ad45302ae1162529c722599b7e47266db3a4044dec95124d59dbd4db0ecf39","0xd12232202fcbde04f816d271a16e5eda763197ffee993cb3684b65d9c6495df7","0xb0ca8a637d36c6b9b338af45c35c18f9cd18fefc261fb1b74cc48a8ec1fb721c","0xac5f9e1ef8664db83b1eca729fa2746f0b87097028a4aa25bf88a819b233fad7","0xa05531b31fa409fdce3a22dc9366a14075a411797ffb88b9fe5c5be1afbfef3e","0x4e7199732ef6bb71bbe15b7dbf6e5e5a6f593ba4e4eae6fc0f0d1bf2adfcb415","0x3c9424ed1f19cb577e67b30205f98fe33b87aadb2fe47a475ee233486320c1a8","0x11259449fb7ac58bf41dda4186570d9f6866dc846b3ac072bbb4f4ea945c4671","0x0c96167e8d8e0cbce7fa47e5fb885880df94bf507c10e0339f8991e60f2d990c","0xce7d49fe4beff8a52e819b1b3681d33e162a185b8c1a854aae64b049995fbca1","0xa8573d3a1d3c2f956991bdca21d8f737aa9decf3436de3c6aa66ff72f38bceb3","0x6dd889522b0a37d6c4168ab4cc33ed0c4b2bb778f5234045ff9a817bf868d47e","0x6c33f12f6fee627981b9869c86c92cc9315e39f45ff2ebdfccd227aa9677b70f","0x2428c05646f4f30d45b0725a2e59477cfe5b707047c478da2b647a0575065884","0xf35258d8479acb962e96450531de6f013574db63dba2523a8e90d48b45b34c3b","0x7ffecff14707111271064cfc3006d0bb3ff80edfe01ff15027a52fde3afd5b84","0xf776e9951cffa4052da7296e257f402c2ae7fd2748e90e1a5f9330bb04d9362b","0x1da6547790d47b0cc18f7a88259cb3297b36bd2f3a6f1d449c0be7af63c43209","0x4a3302bf539f65ebb51ff103b7406db1a115aed770de39418d83fd98b21e7674","0xbcb6e994a81f2c1dc7874af988a9a60a9c53c1741dbe4aebdf25131fdb04aea9","0xc19bb615b95e699072a0ae0318e4b81b035972e0e2243f68776edebabc65daeb","0x2afa6b0ab0a36bcaa9d84a7aac10375468ba81346cff9036b9301d8cf9fc090c","0x9edfc090f68c470c6e326ca3a784d51dd4a80ba14ed8b417e38ccf9d4f77a5e5","0xfdc691aa9b60d40f0fa1df9f72995a4f698097f8a4501f680298a476d5a57857","0x2bdf0c6f55965aa7f5efa78c4d86caf9cc5650290532e9a6ec00e3e795789ecb","0x255ee242666e16e5c786e64e2428257d01afe029ddb20576e1be5ce28335976b","0xd12834ea614fd74159672bcf3d594c4db185b46b1cf526bcace9faf1b1c82934","0x44b9956a4866ce96a300e98d37236e062c18e379ed8b1935381bee007e218c1f","0x29fb7be8a028dd03f055f4ed9701cb0054a12363c3860af27ffff219670aee82","0x049a5dd86bdd8e84231942303794dea8538c0f3ddaadbdbcd9af879f74d9df06","0xcff2f4a22944edf6982f27381b9d08d8cb922ec0d605a4f63c8c78d39000472c","0x30a5f65880ccca3116dc65644605f5d19e74335e4fbe6f854cdca008e3e9afa6","0x30a96d9a0ccbde8490fc86c224f458c546b863d4ebb8e99ca47ffed3ea99364a","0xddf7cf6ec9e65c3e23ebcb479b081b5f2e02facacfad040ea596f369a2671d5b","0xb50a78fe0c09cfa4ac5369a8a5a52d9ea81883eb039e7aca26fb0672ccd382fb","0x9365f558d1d0c2d7a25ee95fa57694d8fb61c620039b21711b5247ef6be3a30b","0x6b42cb9a5821845d3ee37d802275e278cd4d93a117bbdb8f8389368ddee361c3","0xe9a69e5939ec73417d307a7b06b352e7fd8f2e7fabf4a019f3e154690e0bedf1","0x3a0c8476e3bf990ee117733d004bdf77f84499a2cc151b23ac608c2fbad7e956","0x443ffedbe263377c8f2daa955153db89802c82acbd31aeeca02eb3dcd373cd41","0xe061ac697f13b3f39cccd75f93bf89c0ed68dc96701aa3778f88bd593ab12a35","0x5fb51c92a8dc7b05a326dd109331718a8d5f94c0c1795ec42532817da00b187c","0x60318cb40d9790e93fc61035e8240a96e6f2f2d9cdbf815c77140dff7f35859d","0xc18a4648ae08fd72709d961d5baacf9b061bdfa62ca77e3fcb1bb532d5d7f49c","0x5f3c48f8c2a558f19635b921941c240fb0623a84335043b15653f7321ab5dacd","0x68aa6e86f9d81b92808078615066fd1db0bd88a90172e0cf211bee2cabef84e6","0x416166bf78a745a8c2bb8f8e033b328f3f73bb437775976fec448272c40ae57a","0xdf06ad8e65dce06b252d3da668a90c3a00c548845fe1c3ce4040ae41e2306cb6","0xfb3d66e266552c9e413821c3081b0f9cf517fdab5e5c8732a669926c464d99bf","0x4bb8589b668f1227a38c75282809ac66c924f89fe7bd0fd410ef44a61c2e2e9e","0xa84bd84b4f90a3b071c532f6d96e9700e3660f5e199b8746915bef55a62720d5","0x1f2f5342c20f5b9098a94c6b5735c8bf32b6fbeececa18b3e60b8090eb7cc080","0x7aed52a344b103f5f1a5d802e54da9c536bb9bc7a1da6e28f5bcca4b38d7f1a5"],"withdrawals":[{"index":"0x6f25f2b","validatorIndex":"0x1e4e28","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1029ae7"},{"index":"0x6f25f2c","validatorIndex":"0x1e4e29","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x101dde8"},{"index":"0x6f25f2d","validatorIndex":"0x1e4e2a","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1021b45"},{"index":"0x6f25f2e","validatorIndex":"0x1e4e2b","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1029bdb"},{"index":"0x6f25f2f","validatorIndex":"0x1e4e2c","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102a465"},{"index":"0x6f25f30","validatorIndex":"0x1e4e2d","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1025f7a"},{"index":"0x6f25f31","validatorIndex":"0x1e4e2e","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028ac2"},{"index":"0x6f25f32","validatorIndex":"0x1e4e2f","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102bec7"},{"index":"0x6f25f33","validatorIndex":"0x1e4e30","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102ad42"},{"index":"0x6f25f34","validatorIndex":"0x1e4e31","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102ce7f"},{"index":"0x6f25f35","validatorIndex":"0x1e4e32","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1023b90"},{"index":"0x6f25f36","validatorIndex":"0x1e4e33","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028160"},{"index":"0x6f25f37","validatorIndex":"0x1e4e34","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x1028248"},{"index":"0x6f25f38","validatorIndex":"0x1e4e35","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102bd9d"},{"index":"0x6f25f39","validatorIndex":"0x1e4e36","address":"0x2b78035514401ed1592eb691b8673a93edf97470","amount":"0x1019300"},{"index":"0x6f25f3a","validatorIndex":"0x1e4e37","address":"0x8e609ac80f4324e499a6efd24f221a2caa868224","amount":"0x102b5c7"}]}


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
    }
);

// Tests that the run command can run functions with arguments
forgetest!(can_execute_script_command_with_args, |prj, cmd| {
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

    cmd.arg("script")
        .arg(script)
        .arg("--sig")
        .arg("run(uint256,uint256)")
        .arg("1")
        .arg("2")
        .assert_success()
        .stdout_eq(str![[r#"
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

forgetest_async!(flaky_can_broadcast_script_skipping_simulation, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    // This example script would fail in on-chain simulation
    let deploy_script = prj.add_source(
        "DeployScript",
        r#"
import "forge-std/Script.sol";

contract HashChecker {
    bytes32 public lastHash;

    function update() public {
        bytes32 newHash = blockhash(block.number - 1);
        require(newHash != lastHash, "Hash didn't change");
        lastHash = newHash;
    }

    function checkLastHash() public view {
        require(lastHash != bytes32(0), "Hash shouldn't be zero");
    }
}

contract DeployScript is Script {
    HashChecker public hashChecker;

    function run() external {
        vm.startBroadcast();
        hashChecker = new HashChecker();
    }
}"#,
    );

    let deploy_contract = deploy_script.display().to_string() + ":DeployScript";

    let node_config = NodeConfig::test().with_eth_rpc_url(Some(rpc::next_http_archive_rpc_url()));
    let (_api, handle) = spawn(node_config).await;
    let private_key =
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string();
    cmd.set_current_dir(prj.root());

    cmd.args([
        "script",
        &deploy_contract,
        "--root",
        prj.root().to_str().unwrap(),
        "--fork-url",
        &handle.http_endpoint(),
        "-vvvvv",
        "--broadcast",
        "--slow",
        "--skip-simulation",
        "--private-key",
        &private_key,
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] DeployScript::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [..] → new HashChecker@[..]
    │   └─ ← [Return] 718 bytes of code
    └─ ← [Stop]


Script ran successfully.

SKIPPING ON CHAIN SIMULATION.


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);

    let run_log = std::fs::read_to_string("broadcast/DeployScript.sol/1/run-latest.json").unwrap();
    let run_object: Value = serde_json::from_str(&run_log).unwrap();
    let contract_address = &run_object["receipts"][0]["contractAddress"]
        .as_str()
        .unwrap()
        .parse::<Address>()
        .unwrap()
        .to_string();

    let run_code = r#"
import "forge-std/Script.sol";
import { HashChecker } from "./DeployScript.sol";

contract RunScript is Script {
    HashChecker public hashChecker;

    function run() external {
        vm.startBroadcast();
        hashChecker = HashChecker(CONTRACT_ADDRESS);
        uint numUpdates = 8;
        vm.roll(block.number - numUpdates);
        for(uint i = 0; i < numUpdates; i++) {
            vm.roll(block.number + 1);
            hashChecker.update();
            hashChecker.checkLastHash();
        }
    }
}"#
    .replace("CONTRACT_ADDRESS", contract_address);

    let run_script = prj.add_source("RunScript", &run_code);
    let run_contract = run_script.display().to_string() + ":RunScript";

    cmd.forge_fuse()
        .args([
            "script",
            &run_contract,
            "--root",
            prj.root().to_str().unwrap(),
            "--fork-url",
            &handle.http_endpoint(),
            "-vvvvv",
            "--broadcast",
            "--slow",
            "--skip-simulation",
            "--gas-estimate-multiplier",
            "200",
            "--private-key",
            &private_key,
        ])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] RunScript::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    ├─ [0] VM::roll([..])
    │   └─ ← [Return]
    ├─ [..] [..]::update()
    │   └─ ← [Stop]
    ├─ [..] [..]::checkLastHash() [staticcall]
    │   └─ ← [Stop]
    └─ ← [Stop]


Script ran successfully.

SKIPPING ON CHAIN SIMULATION.


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
});

forgetest_async!(can_deploy_script_without_lib, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0, 1])
        .await
        .add_sig("BroadcastTestNoLinking", "deployDoesntPanic()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 1), (1, 2)])
        .await;
});

forgetest_async!(can_deploy_script_with_lib, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0, 1])
        .await
        .add_sig("BroadcastTest", "deploy()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 2), (1, 1)])
        .await;
});

forgetest_async!(can_deploy_script_private_key, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_addresses(&[address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906")])
        .await
        .add_sig("BroadcastTest", "deployPrivateKey()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment_addresses(&[(
            address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906"),
            3,
        )])
        .await;
});

forgetest_async!(
    #[ignore = "tempo skip - uses native ETH value transfer which Tempo does not support"]
    can_deploy_unlocked,
    |prj, cmd| {
        let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

        tester
            .sender("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".parse().unwrap())
            .unlocked()
            .add_sig("BroadcastTest", "deployOther()")
            .simulate(ScriptOutcome::OkSimulation)
            .broadcast(ScriptOutcome::OkBroadcast);
    }
);

forgetest_async!(can_deploy_script_remember_key, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_addresses(&[address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906")])
        .await
        .add_sig("BroadcastTest", "deployRememberKey()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment_addresses(&[(
            address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906"),
            2,
        )])
        .await;
});

forgetest_async!(can_deploy_script_remember_key_and_resume, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .add_deployer(0)
        .load_addresses(&[address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906")])
        .await
        .add_sig("BroadcastTest", "deployRememberKeyResume()")
        .simulate(ScriptOutcome::OkSimulation)
        .resume(ScriptOutcome::MissingWallet)
        // load missing wallet
        .load_private_keys(&[0])
        .await
        .run(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment_addresses(&[(
            address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906"),
            1,
        )])
        .await
        .assert_nonce_increment(&[(0, 2)])
        .await;
});

forgetest_async!(can_resume_script, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastTest", "deploy()")
        .simulate(ScriptOutcome::OkSimulation)
        .resume(ScriptOutcome::MissingWallet)
        // load missing wallet
        .load_private_keys(&[1])
        .await
        .run(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 2), (1, 1)])
        .await;
});

forgetest_async!(
    #[ignore = "tempo skip - uses native ETH value transfer which Tempo does not support"]
    can_deploy_broadcast_wrap,
    |prj, cmd| {
        let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

        tester
            .add_deployer(2)
            .load_private_keys(&[0, 1, 2])
            .await
            .add_sig("BroadcastTest", "deployOther()")
            .simulate(ScriptOutcome::OkSimulation)
            .broadcast(ScriptOutcome::OkBroadcast)
            .assert_nonce_increment(&[(0, 4), (1, 4), (2, 1)])
            .await;
    }
);

forgetest_async!(
    #[ignore = "tempo skip - uses native ETH value transfer which Tempo does not support"]
    panic_no_deployer_set,
    |prj, cmd| {
        let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

        tester
            .load_private_keys(&[0, 1])
            .await
            .add_sig("BroadcastTest", "deployOther()")
            .simulate(ScriptOutcome::WarnSpecifyDeployer)
            .broadcast(ScriptOutcome::MissingSender);
    }
);

forgetest_async!(can_deploy_no_arg_broadcast, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .add_deployer(0)
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastTest", "deployNoArgs()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 3)])
        .await;
});

forgetest_async!(
    #[ignore = "tempo skip - create2 duplicate detection behavior differs"]
    can_deploy_with_create2,
    |prj, cmd| {
        let (api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

        // Prepare CREATE2 Deployer
        api.anvil_set_code(
            foundry_evm::constants::DEFAULT_CREATE2_DEPLOYER,
            Bytes::from_static(foundry_evm::constants::DEFAULT_CREATE2_DEPLOYER_RUNTIME_CODE),
        )
        .await
        .unwrap();

        tester
            .add_deployer(0)
            .load_private_keys(&[0])
            .await
            .add_sig("BroadcastTestNoLinking", "deployCreate2()")
            .simulate(ScriptOutcome::OkSimulation)
            .broadcast(ScriptOutcome::OkBroadcast)
            .assert_nonce_increment(&[(0, 2)])
            .await
            // Running again results in error, since we're repeating the salt passed to CREATE2
            .run(ScriptOutcome::ScriptFailed);
    }
);

forgetest_async!(
    #[ignore = "tempo skip - create2 with custom deployer behavior differs"]
    can_deploy_with_custom_create2,
    |prj, cmd| {
        let (api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());
        let create2 = address!("0x0000000000000000000000000000000000b4956c");

        // Prepare CREATE2 Deployer
        api.anvil_set_code(
            create2,
            Bytes::from_static(foundry_evm::constants::DEFAULT_CREATE2_DEPLOYER_RUNTIME_CODE),
        )
        .await
        .unwrap();

        tester
            .add_deployer(0)
            .load_private_keys(&[0])
            .await
            .add_create2_deployer(create2)
            .add_sig("BroadcastTestNoLinking", "deployCreate2(address)")
            .arg(&create2.to_string())
            .simulate(ScriptOutcome::OkSimulation)
            .broadcast(ScriptOutcome::OkBroadcast)
            .assert_nonce_increment(&[(0, 2)])
            .await;
    }
);

forgetest_async!(
    #[ignore = "tempo skip - create2 bytecode matching behavior differs"]
    can_deploy_with_custom_create2_notmatched_bytecode,
    |prj, cmd| {
        let (api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());
        let create2 = address!("0x0000000000000000000000000000000000b4956c");

        // Prepare CREATE2 Deployer
        api.anvil_set_code(
        create2,
        Bytes::from_static(&hex!("7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe03601600081602082378035828234f58015156039578182fd5b8082525050506014600cef")),
    )
    .await
    .unwrap();

        tester
            .add_deployer(0)
            .load_private_keys(&[0])
            .await
            .add_create2_deployer(create2)
            .add_sig("BroadcastTestNoLinking", "deployCreate2()")
            .simulate(ScriptOutcome::ScriptFailed)
            .broadcast(ScriptOutcome::ScriptFailed);
    }
);

forgetest_async!(
    #[ignore = "tempo skip - create2 error detection behavior differs"]
    cannot_deploy_with_nonexist_create2,
    |prj, cmd| {
        let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
        let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());
        let create2 = address!("0x0000000000000000000000000000000000b4956c");

        tester
            .add_deployer(0)
            .load_private_keys(&[0])
            .await
            .add_create2_deployer(create2)
            .add_sig("BroadcastTestNoLinking", "deployCreate2()")
            .simulate(ScriptOutcome::ScriptFailed)
            .broadcast(ScriptOutcome::ScriptFailed);
    }
);

forgetest_async!(can_deploy_and_simulate_25_txes_concurrently, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastTestNoLinking", "deployMany()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 25)])
        .await;
});

forgetest_async!(can_deploy_and_simulate_mixed_broadcast_modes, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastMix", "deployMix()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 15)])
        .await;
});

forgetest_async!(deploy_with_setup, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastTestSetup", "run()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 6)])
        .await;
});

forgetest_async!(fail_broadcast_staticcall, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastTestNoLinking", "errorStaticCall()")
        .simulate(ScriptOutcome::StaticCallNotAllowed);
});

forgetest_async!(check_broadcast_log, |prj, cmd| {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    // Prepare CREATE2 Deployer
    let addr = address!("0x4e59b44847b379578588920ca78fbf26c0b4956c");
    let code = hex::decode("7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe03601600081602082378035828234f58015156039578182fd5b8082525050506014600cf3").expect("Could not decode create2 deployer init_code").into();
    api.anvil_set_code(addr, code).await.unwrap();

    tester
        .load_private_keys(&[0])
        .await
        .add_sig("BroadcastTestSetup", "run()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 6)])
        .await;

    // Uncomment to recreate the broadcast log
    // std::fs::copy(
    //     "broadcast/Broadcast.t.sol/31337/run-latest.json",
    //     PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/broadcast.
    // log. json" ), );

    // Check broadcast logs
    // Ignore timestamp, blockHash, blockNumber, cumulativeGasUsed, effectiveGasPrice,
    // transactionIndex and logIndex values since they can change in between runs
    let re = Regex::new(r#"((timestamp":).[0-9]*)|((blockHash":).*)|((blockNumber":).*)|((cumulativeGasUsed":).*)|((effectiveGasPrice":).*)|((transactionIndex":).*)|((logIndex":).*)"#).unwrap();

    let fixtures_log = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/broadcast.log.json"),
    )
    .unwrap();
    let _fixtures_log = re.replace_all(&fixtures_log, "");

    let run_log =
        std::fs::read_to_string("broadcast/Broadcast.t.sol/31337/run-latest.json").unwrap();
    let _run_log = re.replace_all(&run_log, "");

    // similar_asserts::assert_eq!(fixtures_log, run_log);

    // Uncomment to recreate the sensitive log
    // std::fs::copy(
    //     "cache/Broadcast.t.sol/31337/run-latest.json",
    //     PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    //         .join("../../testdata/fixtures/broadcast.sensitive.log.json"),
    // );

    // Check sensitive logs
    // Ignore port number since it can change in between runs
    let re = Regex::new(r":[0-9]+").unwrap();

    let fixtures_log = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/broadcast.sensitive.log.json"),
    )
    .unwrap();
    let fixtures_log = re.replace_all(&fixtures_log, "");

    let run_log = std::fs::read_to_string("cache/Broadcast.t.sol/31337/run-latest.json").unwrap();
    let run_log = re.replace_all(&run_log, "");

    // Clean up carriage return OS differences
    let re = Regex::new(r"\r\n").unwrap();
    let fixtures_log = re.replace_all(&fixtures_log, "\n");
    let run_log = re.replace_all(&run_log, "\n");

    similar_asserts::assert_eq!(fixtures_log, run_log);
});

forgetest_async!(test_default_sender_balance, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    // Expect the default sender to have uint256.max balance.
    tester
        .add_sig("TestInitialBalance", "runDefaultSender()")
        .simulate(ScriptOutcome::OkSimulation);
});

forgetest_async!(test_custom_sender_balance, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    // Expect the sender to have its starting balance.
    tester
        .add_deployer(0)
        .add_sig("TestInitialBalance", "runCustomSender()")
        .simulate(ScriptOutcome::OkSimulation);
});

#[derive(serde::Deserialize)]
struct Transactions {
    transactions: Vec<Transaction>,
}

#[derive(serde::Deserialize)]
struct Transaction {
    arguments: Vec<String>,
}

// test we output arguments <https://github.com/foundry-rs/foundry/issues/3053>
forgetest_async!(can_execute_script_with_arguments, |prj, cmd| {
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

    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let script = prj.add_script(
                "Counter.s.sol",
                r#"
import "forge-std/Script.sol";

struct Point {
    uint256 x;
    uint256 y;
}

contract A {
    address a;
    uint b;
    int c;
    bytes32 d;
    bool e;
    bytes f;
    Point g;
    string h;

  constructor(address _a, uint _b, int _c, bytes32 _d, bool _e, bytes memory _f, Point memory _g, string memory _h) {
    a = _a;
    b = _b;
    c = _c;
    d = _d;
    e = _e;
    f = _f;
    g = _g;
    h = _h;
  }
}

contract Script0 is Script {
  function run() external {
    vm.broadcast();

    new A(msg.sender, 2 ** 32, -1 * (2 ** 32), keccak256(abi.encode(1)), true, "abcdef", Point(10, 99), "hello");
  }
}
   "#,
            );

    cmd
        .forge_fuse()
        .arg("script")
        .arg(script)
        .args([
            "--tc",
            "Script0",
            "--sender",
            "0x00a329c0648769A73afAc7F9381E08FB43dBEA72",
            "--rpc-url",
            handle.http_endpoint().as_str(),
        ])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
...
Script ran successfully.

## Setting up 1 EVM.

==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================

SIMULATION COMPLETE. To broadcast these transactions, add --broadcast and wallet configuration(s) to the previous command. See forge script --help for more.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);

    let run_latest = foundry_common::fs::json_files(&prj.root().join("broadcast"))
        .find(|path| path.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let transactions: Transactions = serde_json::from_str(&content).unwrap();
    let transactions = transactions.transactions;
    assert_eq!(transactions.len(), 1);
    assert_eq!(
        transactions[0].arguments,
        vec![
            "0x00a329c0648769A73afAc7F9381E08FB43dBEA72".to_string(),
            "4294967296".to_string(),
            "-4294967296".to_string(),
            "0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6".to_string(),
            "true".to_string(),
            "0x616263646566".to_string(),
            "(10, 99)".to_string(),
            "hello".to_string(),
        ]
    );
});

// test we output arguments <https://github.com/foundry-rs/foundry/issues/3053>
forgetest_async!(can_execute_script_with_arguments_nested_deploy, |prj, cmd| {
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

    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let script = prj.add_script(
        "Counter.s.sol",
        r#"
import "forge-std/Script.sol";

contract A {
  address a;
  uint b;
  int c;
  bytes32 d;
  bool e;
  bytes f;
  string g;

  constructor(address _a, uint _b, int _c, bytes32 _d, bool _e, bytes memory _f, string memory _g) {
    a = _a;
    b = _b;
    c = _c;
    d = _d;
    e = _e;
    f = _f;
    g = _g;
  }
}

contract B {
  constructor(address _a, uint _b, int _c, bytes32 _d, bool _e, bytes memory _f, string memory _g) {
    new A(_a, _b, _c, _d, _e, _f, _g);
  }
}

contract Script0 is Script {
  function run() external {
    vm.broadcast();
    new B(msg.sender, 2 ** 32, -1 * (2 ** 32), keccak256(abi.encode(1)), true, "abcdef", "hello");
  }
}
   "#,
    );

    cmd
        .forge_fuse()
        .arg("script")
        .arg(script)
        .args([
            "--tc",
            "Script0",
            "--sender",
            "0x00a329c0648769A73afAc7F9381E08FB43dBEA72",
            "--rpc-url",
            handle.http_endpoint().as_str(),
        ])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
...
Script ran successfully.

## Setting up 1 EVM.

==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================

SIMULATION COMPLETE. To broadcast these transactions, add --broadcast and wallet configuration(s) to the previous command. See forge script --help for more.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);

    let run_latest = foundry_common::fs::json_files(&prj.root().join("broadcast"))
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let transactions: Transactions = serde_json::from_str(&content).unwrap();
    let transactions = transactions.transactions;
    assert_eq!(transactions.len(), 1);
    assert_eq!(
        transactions[0].arguments,
        vec![
            "0x00a329c0648769A73afAc7F9381E08FB43dBEA72".to_string(),
            "4294967296".to_string(),
            "-4294967296".to_string(),
            "0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6".to_string(),
            "true".to_string(),
            "0x616263646566".to_string(),
            "hello".to_string(),
        ]
    );
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

forgetest_async!(assert_can_resume_with_additional_contracts, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .add_deployer(0)
        .add_sig("ScriptAdditionalContracts", "run()")
        .broadcast(ScriptOutcome::MissingWallet)
        .load_private_keys(&[0])
        .await
        .resume(ScriptOutcome::OkBroadcast);
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

// https://github.com/foundry-rs/foundry/issues/7620
forgetest_async!(can_run_zero_base_fee, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract SimpleScript is Script {
    function run() external returns (bool success) {
        vm.startBroadcast();
        (success, ) = address(0).call("");
    }
}
   "#,
    );

    let node_config = NodeConfig::test_tempo().with_base_fee(Some(0));
    let (_api, handle) = spawn(node_config).await;
    let dev = handle.dev_accounts().next().unwrap();

    // Firstly run script with non-zero gas prices to ensure that eth_feeHistory contains non-zero
    // values.
    cmd.args([
        "script",
        "SimpleScript",
        "--fork-url",
        &handle.http_endpoint(),
        "--sender",
        format!("{dev:?}").as_str(),
        "--broadcast",
        "--unlocked",
        "--with-gas-price",
        "2000000",
        "--priority-gas-price",
        "100000",
        "--non-interactive",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
...
Script ran successfully.

== Return ==
success: bool true

## Setting up 1 EVM.

==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]).stderr_eq(str![[r#"
Warning: Script contains a transaction to 0x0000000000000000000000000000000000000000 which does not contain any code.

"#]]);

    // Ensure that we can correctly estimate gas when base fee is zero but priority fee is not.
    cmd.forge_fuse()
        .args([
            "script",
            "SimpleScript",
            "--fork-url",
            &handle.http_endpoint(),
            "--sender",
            format!("{dev:?}").as_str(),
            "--broadcast",
            "--unlocked",
            "--non-interactive",
        ])
        .assert_success()
        .stdout_eq(str![[r#"
No files changed, compilation skipped
...
Script ran successfully.

== Return ==
success: bool true

## Setting up 1 EVM.

==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]).stderr_eq(str![[r#"
Warning: Script contains a transaction to 0x0000000000000000000000000000000000000000 which does not contain any code.

"#]]);
});

// Asserts that the script runs with expected non-output using `--quiet` flag
forgetest_async!(adheres_to_quiet_flag, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract SimpleScript is Script {
    function run() external returns (bool success) {
        vm.startBroadcast();
        (success, ) = address(0).call("");
    }
}
   "#,
    );

    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    cmd.args([
        "script",
        "SimpleScript",
        "--fork-url",
        &handle.http_endpoint(),
        "--sender",
        "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        "--broadcast",
        "--unlocked",
        "--non-interactive",
        "--quiet",
    ])
    .assert_empty_stdout();
});

// Asserts that the script runs with expected non-output using `--quiet` flag
forgetest_async!(adheres_to_json_flag, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract SimpleScript is Script {
    function run() external returns (bool success) {
        vm.startBroadcast();
        (success, ) = address(0).call("");
    }
}
   "#,
    );

    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    cmd.args([
        "script",
        "SimpleScript",
        "--fork-url",
        &handle.http_endpoint(),
        "--sender",
        "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        "--broadcast",
        "--unlocked",
        "--non-interactive",
        "--json",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
{"logs":[],"returns":{"success":{"internal_type":"bool","value":"true"}},"success":true,"raw_logs":[],"traces":[["Deployment",{"arena":[{"parent":null,"children":[],"idx":0,"trace":{"depth":0,"success":true,"caller":"0x1804c8ab1f12e6bbf3894d4083f33e07309d1f38","address":"0x5b73c5498c1e3b4dba84de0f1833c4a029d90519","maybe_precompile":false,"selfdestruct_address":null,"selfdestruct_refund_target":null,"selfdestruct_transferred_value":null,"kind":"CREATE","value":"0x0","data":"[..]","output":"[..]","gas_used":"{...}","gas_limit":"{...}","gas_refund_counter":0,"status":"Return","steps":[],"decoded":{"label":"SimpleScript","return_data":null,"call_data":null}},"logs":[],"ordering":[]}]}],["Execution",{"arena":[{"parent":null,"children":[1,2],"idx":0,"trace":{"depth":0,"success":true,"caller":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266","address":"0x5b73c5498c1e3b4dba84de0f1833c4a029d90519","maybe_precompile":null,"selfdestruct_address":null,"selfdestruct_refund_target":null,"selfdestruct_transferred_value":null,"kind":"CALL","value":"0x0","data":"0xc0406226","output":"0x0000000000000000000000000000000000000000000000000000000000000001","gas_used":"{...}","gas_limit":1073720760,"gas_refund_counter":0,"status":"Return","steps":[],"decoded":{"label":"SimpleScript","return_data":"true","call_data":{"signature":"run()","args":[]}}},"logs":[],"ordering":[{"Call":0},{"Call":1}]},{"parent":0,"children":[],"idx":1,"trace":{"depth":1,"success":true,"caller":"0x5b73c5498c1e3b4dba84de0f1833c4a029d90519","address":"0x7109709ecfa91a80626ff3989d68f67f5b1dd12d","maybe_precompile":null,"selfdestruct_address":null,"selfdestruct_refund_target":null,"selfdestruct_transferred_value":null,"kind":"CALL","value":"0x0","data":"0x7fb5297f","output":"0x","gas_used":"{...}","gas_limit":1056940999,"gas_refund_counter":0,"status":"Return","steps":[],"decoded":{"label":"VM","return_data":null,"call_data":{"signature":"startBroadcast()","args":[]}}},"logs":[],"ordering":[]},{"parent":0,"children":[],"idx":2,"trace":{"depth":1,"success":true,"caller":"0x5b73c5498c1e3b4dba84de0f1833c4a029d90519","address":"0x0000000000000000000000000000000000000000","maybe_precompile":null,"selfdestruct_address":null,"selfdestruct_refund_target":null,"selfdestruct_transferred_value":null,"kind":"CALL","value":"0x0","data":"0x","output":"0x","gas_used":"{...}","gas_limit":1056940650,"gas_refund_counter":0,"status":"Stop","steps":[],"decoded":{"label":null,"return_data":null,"call_data":null}},"logs":[],"ordering":[]}]}]],"gas_used":"{...}","labeled_addresses":{},"returned":"0x0000000000000000000000000000000000000000000000000000000000000001","address":null}
{"chain":31337,"estimated_gas_price":"{...}","estimated_total_gas_used":"{...}","estimated_amount_required":"{...}","token_symbol":"[..]"}
{"chain":"anvil-hardhat","status":"success","tx_hash":"[..]","contract_address":"[..]","block_number":1,"gas_used":"{...}","gas_price":"{...}"}
{"status":"success","transactions":"[..]/broadcast/Foo.sol/31337/run-latest.json","sensitive":"[..]/cache/Foo.sol/31337/run-latest.json"}

"#]].is_jsonlines());
});

// https://github.com/foundry-rs/foundry/pull/7742
forgetest_async!(unlocked_no_sender, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract SimpleScript is Script {
    function run() external returns (bool success) {
        vm.startBroadcast(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266);
        (success, ) = address(0).call("");
    }
}
   "#,
    );

    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    cmd.args([
        "script",
        "SimpleScript",
        "--fork-url",
        &handle.http_endpoint(),
        "--broadcast",
        "--unlocked",
        "--non-interactive",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
...
Script ran successfully.

== Return ==
success: bool true

## Setting up 1 EVM.

==========================

Chain 31337

...

"#]])
    .stderr_eq(str![[r#"
...
"#]]);
});

// https://github.com/foundry-rs/foundry/issues/7833
forgetest_async!(
    #[ignore = "tempo skip - create2 error detection behavior differs"]
    error_no_create2,
    |prj, cmd| {
        let (_api, handle) =
            spawn(NodeConfig::test().with_disable_default_create2_deployer(true)).await;

        foundry_test_utils::util::initialize(prj.root());
        prj.add_script(
            "Foo",
            r#"
import "forge-std/Script.sol";

contract SimpleContract {}

contract SimpleScript is Script {
    function run() external {
        vm.startBroadcast(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266);
        new SimpleContract{salt: bytes32(0)}();
    }
}
   "#,
        );

        cmd.args([
            "script",
            "SimpleScript",
            "--fork-url",
            &handle.http_endpoint(),
            "--broadcast",
            "--unlocked",
        ]);

        cmd.assert_failure().stderr_eq(str![[r#"
Error: script failed: missing CREATE2 deployer: 0x4e59b44847b379578588920cA78FbF26c0B4956C

"#]]);
    }
);

forgetest_async!(can_switch_forks_in_setup, |prj, cmd| {
    let (_api, handle) =
        spawn(NodeConfig::test().with_disable_default_create2_deployer(true)).await;

    foundry_test_utils::util::initialize(prj.root());
    let url = handle.http_endpoint();

    prj.add_script(
        "Foo",
        &r#"
import "forge-std/Script.sol";

contract SimpleScript is Script {
    function setUp() external {
        uint256 initialFork = vm.activeFork();
        vm.createSelectFork("<url>");
        vm.selectFork(initialFork);
    }

    function run() external {
        assert(vm.getNonce(msg.sender) == 0);
    }
}
   "#
        .replace("<url>", &url),
    );

    cmd.args([
        "script",
        "SimpleScript",
        "--fork-url",
        &url,
        "--sender",
        "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful with warnings:
Warning (2018): Function state mutability can be restricted to view
  [FILE]:13:5:
   |
13 |     function run() external {
   |     ^ (Relevant source part starts here and spans across multiple lines).

Script ran successfully.

"#]]);
});

// Asserts that running the same script twice only deploys library once.
forgetest_async!(can_deploy_library_create2, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0, 1])
        .await
        .add_sig("BroadcastTest", "deploy()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 2), (1, 1)])
        .await;

    tester.clear();

    tester
        .load_private_keys(&[0, 1])
        .await
        .add_sig("BroadcastTest", "deploy()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 1), (1, 1)])
        .await;
});

// Asserts that running the same script twice only deploys library once when using different
// senders.
forgetest_async!(can_deploy_library_create2_different_sender, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    let mut tester = ScriptTester::new_broadcast(cmd, &handle.http_endpoint(), prj.root());

    tester
        .load_private_keys(&[0, 1])
        .await
        .add_sig("BroadcastTest", "deploy()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(0, 2), (1, 1)])
        .await;

    tester.clear();

    // Run different script from the same contract (which requires the same library).
    tester
        .load_private_keys(&[2])
        .await
        .add_sig("BroadcastTest", "deployNoArgs()")
        .simulate(ScriptOutcome::OkSimulation)
        .broadcast(ScriptOutcome::OkBroadcast)
        .assert_nonce_increment(&[(2, 2)])
        .await;
});

// <https://github.com/foundry-rs/foundry/issues/8993>
forgetest_async!(
    #[ignore = "tempo skip - broadcastRawTransaction missing from field"]
    test_broadcast_raw_create2_deployer,
    |prj, cmd| {
        let (api, handle) =
            spawn(NodeConfig::test().with_disable_default_create2_deployer(true)).await;

        foundry_test_utils::util::initialize(prj.root());
        prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract SimpleScript is Script {
    function run() external {
        // send funds to create2 factory deployer
        vm.startBroadcast();
        payable(0x3fAB184622Dc19b6109349B94811493BF2a45362).transfer(10000000 gwei);
        // deploy create2 factory
        vm.broadcastRawTransaction(
            hex"f8a58085174876e800830186a08080b853604580600e600039806000f350fe7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe03601600081602082378035828234f58015156039578182fd5b8082525050506014600cf31ba02222222222222222222222222222222222222222222222222222222222222222a02222222222222222222222222222222222222222222222222222222222222222"
        );
    }
}
   "#,
    );

        cmd.args([
            "script",
            "--private-key",
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "--rpc-url",
            &handle.http_endpoint(),
            "--broadcast",
            "--slow",
            "SimpleScript",
        ]);

        cmd.assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.

## Setting up 1 EVM.

==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);

        assert!(
            !api.get_code(
                address!("0x4e59b44847b379578588920cA78FbF26c0B4956C"),
                Default::default()
            )
            .await
            .unwrap()
            .is_empty()
        );
    }
);

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

forgetest_async!(can_simulate_with_default_sender, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Script.s.sol",
        r#"
import "forge-std/Script.sol";
contract A {
    function getValue() external pure returns (uint256) {
        return 100;
    }
}
contract B {
    constructor(A a) {
        require(a.getValue() == 100);
    }
}
contract SimpleScript is Script {
    function run() external {
        vm.startBroadcast();
        A a = new A();
        new B(a);
    }
}
            "#,
    );

    cmd.arg("script").args(["SimpleScript", "--fork-url", &handle.http_endpoint(), "-vvvv"]);
    cmd.assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] SimpleScript::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [..] → new A@0x5b73C5498c1E3b4dbA84de0F1833c4a029d90519
    │   └─ ← [Return] 175 bytes of code
    ├─ [..] → new B@0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496
    │   ├─ [..] A::getValue() [staticcall]
    │   │   └─ ← [Return] 100
    │   └─ ← [Return] 62 bytes of code
    └─ ← [Stop]


Script ran successfully.

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [..] → new A@0x5b73C5498c1E3b4dbA84de0F1833c4a029d90519
    └─ ← [Return] 175 bytes of code

  [..] → new B@0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496
    ├─ [..] A::getValue() [staticcall]
    │   └─ ← [Return] 100
    └─ ← [Return] 62 bytes of code
...
"#]]);
});

forgetest_async!(should_detect_additional_contracts, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    foundry_test_utils::util::initialize(prj.root());
    prj.add_source(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract Simple {}

contract Deployer {
    function deploy() public {
        new Simple();
    }
}

contract ContractScript is Script {
    function run() public {
        vm.startBroadcast();
        Deployer deployer = new Deployer();
        deployer.deploy();
    }
}
   "#,
    );
    cmd.arg("script")
        .args([
            "ContractScript",
            "--private-key",
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "--rpc-url",
            &handle.http_endpoint(),
        ])
        .assert_success();

    let run_latest = foundry_common::fs::json_files(&prj.root().join("broadcast"))
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let sequence: ScriptSequence = foundry_common::fs::read_json_file(&run_latest).unwrap();

    assert_eq!(sequence.transactions.len(), 2);
    assert_eq!(sequence.transactions[1].additional_contracts.len(), 1);
});

// <https://github.com/foundry-rs/foundry/issues/9661>
forgetest_async!(should_set_correct_sender_nonce_via_cli, |prj, cmd| {
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
});

forgetest_async!(dryrun_without_broadcast, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;

    foundry_test_utils::util::initialize(prj.root());
    prj.add_source(
        "Foo",
        r#"
import "forge-std/Script.sol";

contract Called {
    event log_string(string);
    uint256 public x;
    uint256 public y;
    function run(uint256 _x, uint256 _y) external {
        x = _x;
        y = _y;
        emit log_string("script ran");
    }
}

contract DryRunTest is Script {
    function run() external {
        vm.startBroadcast();
        Called called = new Called();
        called.run(123, 456);
    }
}
   "#,
    );

    cmd.arg("script")
        .args([
            "DryRunTest",
            "--private-key",
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "--rpc-url",
            &handle.http_endpoint(),
            "-vvvv",
        ])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] DryRunTest::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [..] → new Called@0x5FbDB2315678afecb367f032d93F642f64180aa3
    │   └─ ← [Return] 567 bytes of code
    ├─ [..] Called::run(123, 456)
    │   ├─ emit log_string(val: "script ran")
    │   └─ ← [Stop]
    └─ ← [Stop]


Script ran successfully.

== Logs ==
  script ran

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [113557] → new Called@0x5FbDB2315678afecb367f032d93F642f64180aa3
    └─ ← [Return] 567 bytes of code

  [46595] Called::run(123, 456)
    ├─ emit log_string(val: "script ran")
    └─ ← [Stop]


==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================

=== Transactions that will be broadcast ===


Chain 31337

### Transaction 1 ###

accessList           []
chainId              31337
gasLimit             [..]
gasPrice             
input                [..]
maxFeePerBlobGas     
maxFeePerGas         
maxPriorityFeePerGas 
nonce                0
to                   
type                 0
value                0

### Transaction 2 ###

accessList           []
chainId              31337
gasLimit             [..]
gasPrice             
input                0x7357f5d2000000000000000000000000000000000000000000000000000000000000007b00000000000000000000000000000000000000000000000000000000000001c8
maxFeePerBlobGas     
maxFeePerGas         
maxPriorityFeePerGas 
nonce                1
to                   0x5FbDB2315678afecb367f032d93F642f64180aa3
type                 0
value                0
contract: Called(0x5FbDB2315678afecb367f032d93F642f64180aa3)
data (decoded): run(uint256,uint256)(
  123,
  456
)


SIMULATION COMPLETE. To broadcast these transactions, add --broadcast and wallet configuration(s) to the previous command. See forge script --help for more.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
});

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

// Tests that script warns if no tx to broadcast.
// <https://github.com/foundry-rs/foundry/issues/10015>
forgetest_async!(warns_if_no_transactions_to_broadcast, |prj, cmd| {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "NoTxScript.s.sol",
        r#"
        import {Script} from "forge-std/Script.sol";

    contract NoTxScript is Script {
        function run() public {
            vm.startBroadcast();
            // No real tx created
            vm.stopBroadcast();
        }
    }
    "#,
    );

    cmd.args([
        "script",
        "--private-key",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        "--rpc-url",
        &handle.http_endpoint(),
        "--broadcast",
        "NoTxScript",
    ])
    .assert_success()
    .stderr_eq(str![
        r#"
Warning: No transactions to broadcast.

"#
    ]);
});

// Tests EIP-7702 broadcast <https://github.com/foundry-rs/foundry/issues/10461>
forgetest_async!(can_broadcast_txes_with_signed_auth, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.initialize_default_contracts();
    prj.add_script(
            "EIP7702Script.s.sol",
            r#"
import "forge-std/Script.sol";
import {Vm} from "forge-std/Vm.sol";
import {Counter} from "../src/Counter.sol";
contract EIP7702Script is Script {
    uint256 constant PRIVATE_KEY = uint256(bytes32(0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80));
    address constant SENDER = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
    function setUp() public {}
    function run() public {
        vm.startBroadcast(PRIVATE_KEY);
        Counter counter = new Counter();
        Counter counter1 = new Counter();
        Counter counter2 = new Counter();
        vm.signAndAttachDelegation(address(counter), PRIVATE_KEY);
        Counter(SENDER).increment();
        Counter(SENDER).increment();
        vm.signAndAttachDelegation(address(counter1), PRIVATE_KEY);
        Counter(SENDER).setNumber(0);
        vm.signAndAttachDelegation(address(counter2), PRIVATE_KEY);
        Counter(SENDER).setNumber(0);
        vm.stopBroadcast();
    }
}
   "#,
        );

    let node_config = NodeConfig::test().with_hardfork(Some(EthereumHardfork::Prague.into()));
    let (_api, handle) = spawn(node_config).await;

    cmd.args([
        "script",
        "script/EIP7702Script.s.sol",
        "--rpc-url",
        &handle.http_endpoint(),
        "-vvvvv",
        "--non-interactive",
        "--slow",
        "--broadcast",
        "--evm-version",
        "prague",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] EIP7702Script::setUp()
    └─ ← [Stop]

  [..] EIP7702Script::run()
    ├─ [0] VM::startBroadcast(<pk>)
    │   └─ ← [Return]
    ├─ [..] → new Counter@0x5FbDB2315678afecb367f032d93F642f64180aa3
    │   └─ ← [Return] 481 bytes of code
    ├─ [..] → new Counter@0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512
    │   └─ ← [Return] 481 bytes of code
    ├─ [..] → new Counter@0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0
    │   └─ ← [Return] 481 bytes of code
    ├─ [0] VM::signAndAttachDelegation(0x5FbDB2315678afecb367f032d93F642f64180aa3, "<pk>")
    │   └─ ← [Return] (0, 0xd4301eb9f82f747137a5f2c3dc3a5c2d253917cf99ecdc0d49f7bb85313c3159, 0x786d354f0bbd456f44116ddd3aa50475e989d72d8396005e5b3a12cede83fb68, 4, 0x5FbDB2315678afecb367f032d93F642f64180aa3)
    ├─ [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::increment()
    │   └─ ← [Stop]
    ├─ [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::increment()
    │   └─ ← [Stop]
    ├─ [0] VM::signAndAttachDelegation(0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512, "<pk>")
    │   └─ ← [Return] (0, 0xaba9128338f7ff036a0d2ecb96d4f4376389005cd565f87aba33b312570af962, 0x69acbe0831fb8ca95338bc4b908dcfebaf7b81b0f770a12c073ceb07b89fbdf3, 7, 0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512)
    ├─ [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::setNumber(0)
    │   └─ ← [Stop]
    ├─ [0] VM::signAndAttachDelegation(0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0, "<pk>")
    │   └─ ← [Return] (1, 0x3a3427b66e589338ce7ea06135650708f9152e93e257b4a5ec6eb86a3e09a2ce, 0x444651c354c89fd3312aafb05948e12c0a16220827a5e467705253ab4d8aa8d3, 9, 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0)
    ├─ [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::setNumber(0)
    │   └─ ← [Stop]
    ├─ [0] VM::stopBroadcast()
    │   └─ ← [Return]
    └─ ← [Stop]


Script ran successfully.

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [..] → new Counter@0x5FbDB2315678afecb367f032d93F642f64180aa3
    └─ ← [Return] 481 bytes of code

  [..] → new Counter@0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512
    └─ ← [Return] 481 bytes of code

  [..] → new Counter@0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0
    └─ ← [Return] 481 bytes of code

  [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::increment()
    └─ ← [Stop]

  [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::increment()
    └─ ← [Stop]

  [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::setNumber(0)
    └─ ← [Stop]

  [..] 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266::setNumber(0)
    └─ ← [Stop]


==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
});

// Tests EIP-7702 with multiple auth <https://github.com/foundry-rs/foundry/issues/10551>
// Alice sends 5 ETH from Bob to Receiver1 and 1 ETH to Receiver2
forgetest_async!(can_broadcast_txes_with_multiple_auth, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_source(
        "BatchCallDelegation.sol",
        r#"
contract BatchCallDelegation {
    event CallExecuted(address indexed to, uint256 indexed value, bytes data, bool success);

    struct Call {
        bytes data;
        address to;
        uint256 value;
    }

    function execute(Call[] calldata calls) external payable {
        for (uint256 i = 0; i < calls.length; i++) {
            Call memory call = calls[i];
            (bool success,) = call.to.call{value: call.value}(call.data);
            require(success, "call reverted");
            emit CallExecuted(call.to, call.value, call.data, success);
        }
    }
}
   "#,
    );

    prj.add_script(
            "BatchCallDelegationScript.s.sol",
            r#"
import {Script, console} from "forge-std/Script.sol";
import {Vm} from "forge-std/Vm.sol";
import {BatchCallDelegation} from "../src/BatchCallDelegation.sol";

contract BatchCallDelegationScript is Script {
    // Alice's address and private key (EOA with no initial contract code).
    address payable ALICE_ADDRESS = payable(0x70997970C51812dc3A010C7d01b50e0d17dc79C8);
    uint256 constant ALICE_PK = 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d;

    // Bob's address and private key (Bob will execute transactions on Alice's behalf).
    address constant BOB_ADDRESS = 0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC;
    uint256 constant BOB_PK = 0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a;

    address constant RECEIVER_1 = 0x14dC79964da2C08b23698B3D3cc7Ca32193d9955;
    address constant RECEIVER_2 = 0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc;

    uint256 constant DEPLOYER_PK = 0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6;

    function run() public {
        BatchCallDelegation.Call[] memory aliceCalls = new BatchCallDelegation.Call[](1);
        aliceCalls[0] = BatchCallDelegation.Call({to: RECEIVER_1, value: 5 ether, data: ""});

        BatchCallDelegation.Call[] memory bobCalls = new BatchCallDelegation.Call[](2);
        bobCalls[0] = BatchCallDelegation.Call({to: RECEIVER_1, value: 5 ether, data: ""});
        bobCalls[1] = BatchCallDelegation.Call({to: RECEIVER_2, value: 1 ether, data: ""});

        vm.startBroadcast(DEPLOYER_PK);
        BatchCallDelegation batcher = new BatchCallDelegation();
        vm.stopBroadcast();

        vm.startBroadcast(ALICE_PK);
        vm.signAndAttachDelegation(address(batcher), ALICE_PK);
        vm.signAndAttachDelegation(address(batcher), BOB_PK);
        vm.signAndAttachDelegation(address(batcher), BOB_PK);

        BatchCallDelegation(BOB_ADDRESS).execute(bobCalls);

        vm.stopBroadcast();
    }
}
   "#,
        );

    let node_config = NodeConfig::test().with_hardfork(Some(EthereumHardfork::Prague.into()));
    let (api, handle) = spawn(node_config).await;

    cmd.args([
        "script",
        "script/BatchCallDelegationScript.s.sol",
        "--rpc-url",
        &handle.http_endpoint(),
        "--non-interactive",
        "--slow",
        "--broadcast",
        "--evm-version",
        "prague",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Script ran successfully.

## Setting up 1 EVM.

==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);

    // Alice nonce should be 2 (tx sender and one auth)
    let alice_acc = api
        .get_account(address!("0x70997970C51812dc3A010C7d01b50e0d17dc79C8"), None)
        .await
        .unwrap();
    assert_eq!(alice_acc.nonce, 2);

    // Bob nonce should be 2 (two auths) and balance reduced by 6 ETH.
    let bob_acc = api
        .get_account(address!("0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"), None)
        .await
        .unwrap();
    assert_eq!(bob_acc.nonce, 2);
    assert_eq!(bob_acc.balance.to_string(), "94000000000000000000");

    // Receiver balances should be updated with 5 ETH and 1 ETH.
    let receiver1 = api
        .get_account(address!("0x14dC79964da2C08b23698B3D3cc7Ca32193d9955"), None)
        .await
        .unwrap();
    assert_eq!(receiver1.nonce, 0);
    assert_eq!(receiver1.balance.to_string(), "105000000000000000000");
    let receiver2 = api
        .get_account(address!("0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc"), None)
        .await
        .unwrap();
    assert_eq!(receiver2.nonce, 0);
    assert_eq!(receiver2.balance.to_string(), "101000000000000000000");
});

// <https://github.com/foundry-rs/foundry/issues/11159>
forgetest_async!(check_broadcast_log_with_additional_contracts, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_source(
        "Counter.sol",
        r#"
contract Counter {
    uint256 public number;

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}
   "#,
    );
    prj.add_source(
        "Factory.sol",
        r#"
import {Counter} from "./Counter.sol";

contract Factory {
    function deployCounter() public returns (Counter) {
        return new Counter();
    }
}
   "#,
    );
    let deploy_script = prj.add_script(
        "Factory.s.sol",
        r#"
import "forge-std/Script.sol";
import {Factory} from "../src/Factory.sol";
import {Counter} from "../src/Counter.sol";

contract FactoryScript is Script {
    Factory public factory;
    Counter public counter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        factory = new Factory();
        counter = factory.deployCounter();

        vm.stopBroadcast();
    }
}
   "#,
    );

    let deploy_contract = deploy_script.display().to_string() + ":FactoryScript";
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    cmd.args([
        "script",
        &deploy_contract,
        "--root",
        prj.root().to_str().unwrap(),
        "--fork-url",
        &handle.http_endpoint(),
        "--slow",
        "--broadcast",
        "--private-key",
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ])
    .assert_success();

    let broadcast_log = prj.root().join("broadcast/Factory.s.sol/31337/run-latest.json");
    let script_sequence: ScriptSequence = serde_json::from_reader(
        fs::File::open(prj.artifacts().join(broadcast_log)).expect("no broadcast log"),
    )
    .expect("no script sequence");

    let counter_contract = script_sequence
        .transactions
        .get(1)
        .expect("no tx")
        .additional_contracts
        .first()
        .expect("no Counter contract");
    assert_eq!(counter_contract.contract_name, Some("Counter".to_string()));
});

// <https://github.com/foundry-rs/foundry/issues/11213>
forgetest_async!(call_to_non_contract_address_does_not_panic, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());

    let endpoint = rpc::next_http_archive_rpc_url();

    prj.add_source(
        "Counter.sol",
        r#"
contract Counter {
    uint256 public number;

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}
   "#,
    );

    let deploy_script = prj.add_script(
        "Counter.s.sol",
        &r#"
import "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";

contract CounterScript is Script {
    Counter public counter;
    function setUp() public {}
    function run() public {
        vm.createSelectFork("<url>");
        vm.startBroadcast();
        counter = new Counter();
        vm.stopBroadcast();

        vm.createSelectFork("<url>");
        vm.startBroadcast();
        counter.increment();
        vm.stopBroadcast();
    }
}
   "#
        .replace("<url>", &endpoint),
    );

    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    cmd.args([
        "script",
        &deploy_script.display().to_string(),
        "--root",
        prj.root().to_str().unwrap(),
        "--fork-url",
        &handle.http_endpoint(),
        "--slow",
        "--broadcast",
        "--private-key",
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] → new CounterScript@[..]
    └─ ← [Return] 2162 bytes of code

  [..] CounterScript::setUp()
    └─ ← [Stop]

  [..] CounterScript::run()
    ├─ [..] VM::createSelectFork("<rpc url>")
    │   └─ ← [Return] 1
    ├─ [..] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [..] → new Counter@[..]
    │   └─ ← [Return] 481 bytes of code
    ├─ [..] VM::stopBroadcast()
    │   └─ ← [Return]
    ├─ [..] VM::createSelectFork("<rpc url>")
    │   └─ ← [Return] 2
    ├─ [..] VM::startBroadcast()
    │   └─ ← [Return]
    └─ ← [Revert] call to non-contract address [..]



"#]])
    .stderr_eq(str![[r#"
Error: script failed: call to non-contract address [..]
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

// <https://github.com/foundry-rs/foundry/issues/11855>
forgetest_async!(can_broadcast_from_deploy_code_cheatcode, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.initialize_default_contracts();
    prj.add_script(
        "Counter.s.sol",
        r#"
import "forge-std/Script.sol";
import {Vm} from "forge-std/Vm.sol";
import {Counter} from "../src/Counter.sol";
contract CounterScript is Script {
    function run() public {
        vm.startBroadcast();
        address addr1 = vm.deployCode("src/Counter.sol:Counter");
        Counter(addr1).increment();
        vm.stopBroadcast();
    }
}
   "#,
    );

    let node_config = NodeConfig::test().with_hardfork(Some(EthereumHardfork::Prague.into()));
    let (_api, handle) = spawn(node_config).await;

    cmd.args([
        "script",
        "script/Counter.s.sol:CounterScript",
        "--rpc-url",
        &handle.http_endpoint(),
        "-vvvv",
        "--broadcast",
        "--private-key",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [..] CounterScript::run()
    ├─ [0] VM::startBroadcast()
    │   └─ ← [Return]
    ├─ [0] VM::deployCode("src/Counter.sol:Counter")
    │   ├─ [..] → new Counter@0x5FbDB2315678afecb367f032d93F642f64180aa3
    │   │   └─ ← [Return] 481 bytes of code
    │   └─ ← [Return] Counter: [0x5FbDB2315678afecb367f032d93F642f64180aa3]
    ├─ [..] Counter::increment()
    │   └─ ← [Stop]
    ├─ [0] VM::stopBroadcast()
    │   └─ ← [Return]
    └─ ← [Stop]


Script ran successfully.

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [..] → new Counter@0x5FbDB2315678afecb367f032d93F642f64180aa3
    └─ ← [Return] 481 bytes of code

  [..] Counter::increment()
    └─ ← [Stop]


==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
});

forgetest_async!(flaky_can_deploy_with_broadcast_in_setup, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Deploy.s.sol",
        r#"
import "forge-std/Script.sol";
import {Vm} from "forge-std/Vm.sol";
contract DeployScript is Script {
    function setUp() public {
        vm.startBroadcast();
    }

    function run() public {
        payable(address(0)).transfer(1 ether);

        vm.stopBroadcast();
    }
}
   "#,
    );

    let node_config = NodeConfig::test().with_hardfork(Some(EthereumHardfork::Prague.into()));
    let (_api, handle) = spawn(node_config).await;

    cmd.args([
        "script",
        "script/Deploy.s.sol:DeployScript",
        "--rpc-url",
        &handle.http_endpoint(),
        "-vvvv",
        "--broadcast",
        "--private-key",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Traces:
  [9882] DeployScript::run()
    ├─ [0] 0x0000000000000000000000000000000000000000::fallback{value: 1000000000000000000}()
    │   └─ ← [Stop]
    ├─ [0] VM::stopBroadcast()
    │   └─ ← [Return]
    └─ ← [Stop]


Script ran successfully.

## Setting up 1 EVM.
==========================
Simulated On-chain Traces:

  [0] 0x0000000000000000000000000000000000000000::fallback{value: 1000000000000000000}()
    └─ ← [Stop]


==========================

Chain 31337

[ESTIMATED_GAS_PRICE]

[ESTIMATED_TOTAL_GAS_USED]

[ESTIMATED_AMOUNT_REQUIRED]

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

[SAVED_TRANSACTIONS]

[SAVED_SENSITIVE_VALUES]


"#]]);
});

// <https://github.com/foundry-rs/foundry/issues/12151>
forgetest_async!(
    #[ignore = "tempo skip - uses native ETH value transfer which Tempo does not support"]
    can_execute_script_with_createx_and_via_ir,
    |prj, cmd| {
        foundry_test_utils::util::initialize(prj.root());
        prj.update_config(|config| {
            config.optimizer = Some(true);
            config.via_ir = true;
        });
        prj.add_script("CreateXScript.s.sol", include_str!("../fixtures/CreateXScript.sol"));

        let (_api, handle) = spawn(NodeConfig::test().with_auto_impersonate(true)).await;
        cmd.cast_fuse()
            .args([
                "send",
                "0xeD456e05CaAb11d66C4c797dD6c1D6f9A7F352b5",
                "--value",
                "1000000000000000000",
                "--from",
                "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
                "--unlocked",
                "--rpc-url",
                &handle.http_endpoint(),
            ])
            .assert_success();
        cmd.cast_fuse()
        .args(["publish", "0xf92f698085174876e800832dc6c08080b92f1660a06040523060805234801561001457600080fd5b50608051612e3e6100d860003960008181610603015281816107050152818161082b015281816108d50152818161127f01528181611375015281816113e00152818161141f015281816114a7015281816115b3015281816117d20152818161183d0152818161187c0152818161190401528181611ac501528181611c7801528181611ce301528181611d2201528181611daa01528181611fe901528181612206015281816122f20152818161244d015281816124a601526125820152612e3e6000f3fe60806040526004361061018a5760003560e01c806381503da1116100d6578063d323826a1161007f578063e96deee411610059578063e96deee414610395578063f5745aba146103a8578063f9664498146103bb57600080fd5b8063d323826a1461034f578063ddda0acb1461036f578063e437252a1461038257600080fd5b80639c36a286116100b05780639c36a28614610316578063a7db93f214610329578063c3fe107b1461033c57600080fd5b806381503da1146102d0578063890c283b146102e357806398e810771461030357600080fd5b80632f990e3f116101385780636cec2536116101125780636cec25361461027d57806374637a7a1461029d5780637f565360146102bd57600080fd5b80632f990e3f1461023757806331a7c8c81461024a57806342d654fc1461025d57600080fd5b806327fe18221161016957806327fe1822146101f15780632852527a1461020457806328ddd0461461021757600080fd5b8062d84acb1461018f57806326307668146101cb57806326a32fc7146101de575b600080fd5b6101a261019d366004612915565b6103ce565b60405173ffffffffffffffffffffffffffffffffffffffff909116815260200160405180910390f35b6101a26101d9366004612994565b6103e6565b6101a26101ec3660046129db565b610452565b6101a26101ff3660046129db565b6104de565b6101a2610212366004612a39565b610539565b34801561022357600080fd5b506101a2610232366004612a90565b6106fe565b6101a2610245366004612aa9565b61072a565b6101a2610258366004612aa9565b6107bb565b34801561026957600080fd5b506101a2610278366004612b1e565b6107c9565b34801561028957600080fd5b506101a2610298366004612a90565b610823565b3480156102a957600080fd5b506101a26102b8366004612b4a565b61084f565b6101a26102cb3660046129db565b611162565b6101a26102de366004612b74565b6111e8565b3480156102ef57600080fd5b506101a26102fe366004612bac565b611276565b6101a2610311366004612bce565b6112a3565b6101a2610324366004612994565b611505565b6101a2610337366004612c49565b6116f1565b6101a261034a366004612aa9565b611964565b34801561035b57600080fd5b506101a261036a366004612cd9565b6119ed565b6101a261037d366004612c49565b611a17565b6101a2610390366004612bce565b611e0c565b6101a26103a3366004612915565b611e95565b6101a26103b6366004612bce565b611ea4565b6101a26103c9366004612b74565b611f2d565b60006103dd8585858533611a17565b95945050505050565b6000806103f2846120db565b90508083516020850134f59150610408826123d3565b604051819073ffffffffffffffffffffffffffffffffffffffff8416907fb8fda7e00c6b06a2b54e58521bc5894fee35f1090e5a3bb6390bfe2b98b497f790600090a35092915050565b60006104d86104d260408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b836103e6565b92915050565b600081516020830134f090506104f3816123d3565b60405173ffffffffffffffffffffffffffffffffffffffff8216907f4db17dd5e4732fb6da34a148104a592783ca119a1e7bb8829eba6cbadef0b51190600090a2919050565b600080610545856120db565b905060008460601b90506040517f3d602d80600a3d3981f3363d3d373d3d3d363d7300000000000000000000000081528160148201527f5af43d82803e903d91602b57fd5bf300000000000000000000000000000000006028820152826037826000f593505073ffffffffffffffffffffffffffffffffffffffff8316610635576040517fc05cee7a00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f00000000000000000000000000000000000000000000000000000000000000001660048201526024015b60405180910390fd5b604051829073ffffffffffffffffffffffffffffffffffffffff8516907fb8fda7e00c6b06a2b54e58521bc5894fee35f1090e5a3bb6390bfe2b98b497f790600090a36000808473ffffffffffffffffffffffffffffffffffffffff1634876040516106a19190612d29565b60006040518083038185875af1925050503d80600081146106de576040519150601f19603f3d011682016040523d82523d6000602084013e6106e3565b606091505b50915091506106f382828961247d565b505050509392505050565b60006104d87f00000000000000000000000000000000000000000000000000000000000000008361084f565b60006107b36107aa60408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b85858533611a17565b949350505050565b60006107b3848484336112a3565b60006040518260005260ff600b53836020527f21c35dbe1b344a2488cf3321d6ce542f8e9f305544ff09e4993a62319a497c1f6040526055600b20601452806040525061d694600052600160345350506017601e20919050565b60006104d8827f00000000000000000000000000000000000000000000000000000000000000006107c9565b600060607f9400000000000000000000000000000000000000000000000000000000000000610887600167ffffffffffffffff612d45565b67ffffffffffffffff16841115610902576040517f3c55ab3b00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b836000036109c7576040517fd60000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f800000000000000000000000000000000000000000000000000000000000000060368201526037015b6040516020818303038152906040529150611152565b607f8411610a60576040517fd60000000000000000000000000000000000000000000000000000000000000060208201527fff0000000000000000000000000000000000000000000000000000000000000080831660218301527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606088901b16602283015260f886901b1660368201526037016109b1565b60ff8411610b1f576040517fd70000000000000000000000000000000000000000000000000000000000000060208201527fff0000000000000000000000000000000000000000000000000000000000000080831660218301527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606088901b1660228301527f8100000000000000000000000000000000000000000000000000000000000000603683015260f886901b1660378201526038016109b1565b61ffff8411610bff576040517fd80000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f820000000000000000000000000000000000000000000000000000000000000060368201527fffff00000000000000000000000000000000000000000000000000000000000060f086901b1660378201526039016109b1565b62ffffff8411610ce0576040517fd90000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f830000000000000000000000000000000000000000000000000000000000000060368201527fffffff000000000000000000000000000000000000000000000000000000000060e886901b166037820152603a016109b1565b63ffffffff8411610dc2576040517fda0000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f840000000000000000000000000000000000000000000000000000000000000060368201527fffffffff0000000000000000000000000000000000000000000000000000000060e086901b166037820152603b016109b1565b64ffffffffff8411610ea5576040517fdb0000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f850000000000000000000000000000000000000000000000000000000000000060368201527fffffffffff00000000000000000000000000000000000000000000000000000060d886901b166037820152603c016109b1565b65ffffffffffff8411610f89576040517fdc0000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f860000000000000000000000000000000000000000000000000000000000000060368201527fffffffffffff000000000000000000000000000000000000000000000000000060d086901b166037820152603d016109b1565b66ffffffffffffff841161106e576040517fdd0000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f870000000000000000000000000000000000000000000000000000000000000060368201527fffffffffffffff0000000000000000000000000000000000000000000000000060c886901b166037820152603e016109b1565b6040517fde0000000000000000000000000000000000000000000000000000000000000060208201527fff00000000000000000000000000000000000000000000000000000000000000821660218201527fffffffffffffffffffffffffffffffffffffffff000000000000000000000000606087901b1660228201527f880000000000000000000000000000000000000000000000000000000000000060368201527fffffffffffffffff00000000000000000000000000000000000000000000000060c086901b166037820152603f0160405160208183030381529060405291505b5080516020909101209392505050565b60006104d86111e260408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b83611505565b600061126f61126860408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b8484610539565b9392505050565b600061126f83837f00000000000000000000000000000000000000000000000000000000000000006119ed565b60008451602086018451f090506112b9816123d3565b60405173ffffffffffffffffffffffffffffffffffffffff8216907f4db17dd5e4732fb6da34a148104a592783ca119a1e7bb8829eba6cbadef0b51190600090a26000808273ffffffffffffffffffffffffffffffffffffffff168560200151876040516113279190612d29565b60006040518083038185875af1925050503d8060008114611364576040519150601f19603f3d011682016040523d82523d6000602084013e611369565b606091505b5091509150816113c9577f0000000000000000000000000000000000000000000000000000000000000000816040517fa57ca23900000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b73ffffffffffffffffffffffffffffffffffffffff7f00000000000000000000000000000000000000000000000000000000000000001631156114fb578373ffffffffffffffffffffffffffffffffffffffff167f000000000000000000000000000000000000000000000000000000000000000073ffffffffffffffffffffffffffffffffffffffff163160405160006040518083038185875af1925050503d8060008114611495576040519150601f19603f3d011682016040523d82523d6000602084013e61149a565b606091505b509092509050816114fb577f0000000000000000000000000000000000000000000000000000000000000000816040517fc2b3f44500000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b5050949350505050565b600080611511846120db565b905060006040518060400160405280601081526020017f67363d3d37363d34f03d5260086018f30000000000000000000000000000000081525090506000828251602084016000f5905073ffffffffffffffffffffffffffffffffffffffff81166115e0576040517fc05cee7a00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b604051839073ffffffffffffffffffffffffffffffffffffffff8316907f2feea65dd4e9f9cbd86b74b7734210c59a1b2981b5b137bd0ee3e208200c906790600090a361162c83610823565b935060008173ffffffffffffffffffffffffffffffffffffffff1634876040516116569190612d29565b60006040518083038185875af1925050503d8060008114611693576040519150601f19603f3d011682016040523d82523d6000602084013e611698565b606091505b505090506116a681866124ff565b60405173ffffffffffffffffffffffffffffffffffffffff8616907f4db17dd5e4732fb6da34a148104a592783ca119a1e7bb8829eba6cbadef0b51190600090a25050505092915050565b6000806116fd876120db565b9050808651602088018651f59150611714826123d3565b604051819073ffffffffffffffffffffffffffffffffffffffff8416907fb8fda7e00c6b06a2b54e58521bc5894fee35f1090e5a3bb6390bfe2b98b497f790600090a36000808373ffffffffffffffffffffffffffffffffffffffff168660200151886040516117849190612d29565b60006040518083038185875af1925050503d80600081146117c1576040519150601f19603f3d011682016040523d82523d6000602084013e6117c6565b606091505b509150915081611826577f0000000000000000000000000000000000000000000000000000000000000000816040517fa57ca23900000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b73ffffffffffffffffffffffffffffffffffffffff7f0000000000000000000000000000000000000000000000000000000000000000163115611958578473ffffffffffffffffffffffffffffffffffffffff167f000000000000000000000000000000000000000000000000000000000000000073ffffffffffffffffffffffffffffffffffffffff163160405160006040518083038185875af1925050503d80600081146118f2576040519150601f19603f3d011682016040523d82523d6000602084013e6118f7565b606091505b50909250905081611958577f0000000000000000000000000000000000000000000000000000000000000000816040517fc2b3f44500000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b50505095945050505050565b60006107b36119e460408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b858585336116f1565b6000604051836040820152846020820152828152600b8101905060ff815360559020949350505050565b600080611a23876120db565b905060006040518060400160405280601081526020017f67363d3d37363d34f03d5260086018f30000000000000000000000000000000081525090506000828251602084016000f5905073ffffffffffffffffffffffffffffffffffffffff8116611af2576040517fc05cee7a00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b604051839073ffffffffffffffffffffffffffffffffffffffff8316907f2feea65dd4e9f9cbd86b74b7734210c59a1b2981b5b137bd0ee3e208200c906790600090a3611b3e83610823565b935060008173ffffffffffffffffffffffffffffffffffffffff1687600001518a604051611b6c9190612d29565b60006040518083038185875af1925050503d8060008114611ba9576040519150601f19603f3d011682016040523d82523d6000602084013e611bae565b606091505b50509050611bbc81866124ff565b60405173ffffffffffffffffffffffffffffffffffffffff8616907f4db17dd5e4732fb6da34a148104a592783ca119a1e7bb8829eba6cbadef0b51190600090a260608573ffffffffffffffffffffffffffffffffffffffff1688602001518a604051611c299190612d29565b60006040518083038185875af1925050503d8060008114611c66576040519150601f19603f3d011682016040523d82523d6000602084013e611c6b565b606091505b50909250905081611ccc577f0000000000000000000000000000000000000000000000000000000000000000816040517fa57ca23900000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b73ffffffffffffffffffffffffffffffffffffffff7f0000000000000000000000000000000000000000000000000000000000000000163115611dfe578673ffffffffffffffffffffffffffffffffffffffff167f000000000000000000000000000000000000000000000000000000000000000073ffffffffffffffffffffffffffffffffffffffff163160405160006040518083038185875af1925050503d8060008114611d98576040519150601f19603f3d011682016040523d82523d6000602084013e611d9d565b606091505b50909250905081611dfe577f0000000000000000000000000000000000000000000000000000000000000000816040517fc2b3f44500000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b505050505095945050505050565b60006103dd611e8c60408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b868686866116f1565b60006103dd85858585336116f1565b60006103dd611f2460408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b86868686611a17565b6000808360601b90506040517f3d602d80600a3d3981f3363d3d373d3d3d363d7300000000000000000000000081528160148201527f5af43d82803e903d91602b57fd5bf3000000000000000000000000000000000060288201526037816000f092505073ffffffffffffffffffffffffffffffffffffffff8216612016576040517fc05cee7a00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b60405173ffffffffffffffffffffffffffffffffffffffff8316907f4db17dd5e4732fb6da34a148104a592783ca119a1e7bb8829eba6cbadef0b51190600090a26000808373ffffffffffffffffffffffffffffffffffffffff1634866040516120809190612d29565b60006040518083038185875af1925050503d80600081146120bd576040519150601f19603f3d011682016040523d82523d6000602084013e6120c2565b606091505b50915091506120d282828861247d565b50505092915050565b60008060006120e9846125b3565b9092509050600082600281111561210257612102612e02565b1480156121205750600081600281111561211e5761211e612e02565b145b1561215e57604080513360208201524691810191909152606081018590526080016040516020818303038152906040528051906020012092506123cc565b600082600281111561217257612172612e02565b1480156121905750600181600281111561218e5761218e612e02565b145b156121b0576121a9338560009182526020526040902090565b92506123cc565b60008260028111156121c4576121c4612e02565b03612233576040517f13b3a2a100000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b600182600281111561224757612247612e02565b1480156122655750600081600281111561226357612263612e02565b145b1561227e576121a9468560009182526020526040902090565b600182600281111561229257612292612e02565b1480156122b0575060028160028111156122ae576122ae612e02565b145b1561231f576040517f13b3a2a100000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b61239a60408051437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe08101406020830152419282019290925260608101919091524260808201524460a08201524660c08201523360e08201526000906101000160405160208183030381529060405280519060200120905090565b84036123a657836123c9565b604080516020810186905201604051602081830303815290604052805190602001205b92505b5050919050565b73ffffffffffffffffffffffffffffffffffffffff8116158061240b575073ffffffffffffffffffffffffffffffffffffffff81163b155b1561247a576040517fc05cee7a00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b50565b82158061249f575073ffffffffffffffffffffffffffffffffffffffff81163b155b156124fa577f0000000000000000000000000000000000000000000000000000000000000000826040517fa57ca23900000000000000000000000000000000000000000000000000000000815260040161062c929190612d94565b505050565b811580612520575073ffffffffffffffffffffffffffffffffffffffff8116155b80612540575073ffffffffffffffffffffffffffffffffffffffff81163b155b156125af576040517fc05cee7a00000000000000000000000000000000000000000000000000000000815273ffffffffffffffffffffffffffffffffffffffff7f000000000000000000000000000000000000000000000000000000000000000016600482015260240161062c565b5050565b600080606083901c3314801561261057508260141a60f81b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff19167f0100000000000000000000000000000000000000000000000000000000000000145b1561262057506000905080915091565b606083901c3314801561265a57507fff00000000000000000000000000000000000000000000000000000000000000601484901a60f81b16155b1561266b5750600090506001915091565b33606084901c036126825750600090506002915091565b606083901c1580156126db57508260141a60f81b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff19167f0100000000000000000000000000000000000000000000000000000000000000145b156126ec5750600190506000915091565b606083901c15801561272557507fff00000000000000000000000000000000000000000000000000000000000000601484901a60f81b16155b1561273557506001905080915091565b606083901c61274a5750600190506002915091565b8260141a60f81b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff19167f0100000000000000000000000000000000000000000000000000000000000000036127a55750600290506000915091565b8260141a60f81b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff19166000036127e15750600290506001915091565b506002905080915091565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052604160045260246000fd5b600082601f83011261282c57600080fd5b813567ffffffffffffffff80821115612847576128476127ec565b604051601f83017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0908116603f0116810190828211818310171561288d5761288d6127ec565b816040528381528660208588010111156128a657600080fd5b836020870160208301376000602085830101528094505050505092915050565b6000604082840312156128d857600080fd5b6040516040810181811067ffffffffffffffff821117156128fb576128fb6127ec565b604052823581526020928301359281019290925250919050565b60008060008060a0858703121561292b57600080fd5b84359350602085013567ffffffffffffffff8082111561294a57600080fd5b6129568883890161281b565b9450604087013591508082111561296c57600080fd5b506129798782880161281b565b92505061298986606087016128c6565b905092959194509250565b600080604083850312156129a757600080fd5b82359150602083013567ffffffffffffffff8111156129c557600080fd5b6129d18582860161281b565b9150509250929050565b6000602082840312156129ed57600080fd5b813567ffffffffffffffff811115612a0457600080fd5b6107b38482850161281b565b803573ffffffffffffffffffffffffffffffffffffffff81168114612a3457600080fd5b919050565b600080600060608486031215612a4e57600080fd5b83359250612a5e60208501612a10565b9150604084013567ffffffffffffffff811115612a7a57600080fd5b612a868682870161281b565b9150509250925092565b600060208284031215612aa257600080fd5b5035919050565b600080600060808486031215612abe57600080fd5b833567ffffffffffffffff80821115612ad657600080fd5b612ae28783880161281b565b94506020860135915080821115612af857600080fd5b50612b058682870161281b565b925050612b1585604086016128c6565b90509250925092565b60008060408385031215612b3157600080fd5b82359150612b4160208401612a10565b90509250929050565b60008060408385031215612b5d57600080fd5b612b6683612a10565b946020939093013593505050565b60008060408385031215612b8757600080fd5b612b9083612a10565b9150602083013567ffffffffffffffff8111156129c557600080fd5b60008060408385031215612bbf57600080fd5b50508035926020909101359150565b60008060008060a08587031215612be457600080fd5b843567ffffffffffffffff80821115612bfc57600080fd5b612c088883890161281b565b95506020870135915080821115612c1e57600080fd5b50612c2b8782880161281b565b935050612c3b86604087016128c6565b915061298960808601612a10565b600080600080600060c08688031215612c6157600080fd5b85359450602086013567ffffffffffffffff80821115612c8057600080fd5b612c8c89838a0161281b565b95506040880135915080821115612ca257600080fd5b50612caf8882890161281b565b935050612cbf87606088016128c6565b9150612ccd60a08701612a10565b90509295509295909350565b600080600060608486031215612cee57600080fd5b8335925060208401359150612b1560408501612a10565b60005b83811015612d20578181015183820152602001612d08565b50506000910152565b60008251612d3b818460208701612d05565b9190910192915050565b67ffffffffffffffff828116828216039080821115612d8d577f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b5092915050565b73ffffffffffffffffffffffffffffffffffffffff831681526040602082015260008251806040840152612dcf816060850160208701612d05565b601f017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe016919091016060019392505050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052602160045260246000fdfea164736f6c6343000817000a1ca005f70bf8a1493291468f36ef23b05eb3a4f1807f6b4022942a4104b7537bfc36a029528c0c29546c81e7d78b0277ef87031541bdc96427b246ecedb6d74cd3ed62", "--rpc-url", &handle.http_endpoint()])
        .assert_success();
        cmd.forge_fuse()
            .args([
                "script",
                "script/CreateXScript.s.sol:CreateXScript",
                "--rpc-url",
                &handle.http_endpoint(),
                "--slow",
                "--sender",
                "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
                "--private-key",
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
                "--broadcast",
            ])
            .assert_success();
    }
);

forgetest_async!(script_can_run_with_live_logs_flag, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_script(
        "Foo.s.sol",
        r#"
import {Script, console} from "forge-std/Script.sol";

contract Foo is Script {
    function setUp() pure public {
        console.log("Setup");
    }

    function run() pure public {
        console.log("Run %d", uint256(1));
    }
}
    "#,
    );

    cmd.forge_fuse()
        .args(["script", "script/Foo.s.sol", "--live-logs"])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Setup
Run 1
Script ran successfully.
[GAS]

"#]]);
});

forgetest_async!(script_can_run_with_live_logs_config, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.update_config(|config| {
        config.live_logs = true;
    });

    prj.add_script(
        "Foo.s.sol",
        r#"
import {Script, console} from "forge-std/Script.sol";

contract Foo is Script {
    function setUp() pure public {
        console.log("Setup");
    }

    function run() pure public {
        console.log("Run %d", uint256(1));
    }
}
    "#,
    );

    cmd.forge_fuse().args(["script", "script/Foo.s.sol"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
Setup
Run 1
Script ran successfully.
[GAS]

"#]]);
});
