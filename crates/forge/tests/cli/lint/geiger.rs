forgetest_init!(call, |prj, cmd| {
    prj.add_test(
        "call.t.sol",
        r#"
        import {Test} from "forge-std/Test.sol";

        contract A is Test {
            function do_ffi() public {
                string[] memory inputs = new string[](1);
                vm.ffi(inputs);
            }
        }
    "#,
    );

    cmd.arg("geiger").assert_failure().stderr_eq(str![[r#"
Warning: `forge geiger` is deprecated, as it is just an alias for `forge lint --only-lint unsafe-cheatcode`

Error: Encountered invalid solc version in test/call.t.sol: No solc version exists that matches the version requirement: =0.8.33

"#]]);
});

forgetest_init!(assignment, |prj, cmd| {
    prj.add_test(
        "assignment.t.sol",
        r#"
        import {Test} from "forge-std/Test.sol";

        contract A is Test {
            function do_ffi() public returns (bytes memory) {
                string[] memory inputs = new string[](1);
                bytes memory stuff = vm.ffi(inputs);
                return stuff;
            }
        }
    "#,
    );

    cmd.arg("geiger").assert_failure().stderr_eq(str![[r#"
Warning: `forge geiger` is deprecated, as it is just an alias for `forge lint --only-lint unsafe-cheatcode`

Error: Encountered invalid solc version in test/assignment.t.sol: No solc version exists that matches the version requirement: =0.8.33

"#]]);
});

forgetest_init!(exit_code, |prj, cmd| {
    prj.add_test(
        "multiple.t.sol",
        r#"
        import {Test} from "forge-std/Test.sol";

        contract A is Test {
            function do_ffi() public {
                string[] memory inputs = new string[](1);
                vm.ffi(inputs);
                vm.ffi(inputs);
                vm.ffi(inputs);
            }
        }
    "#,
    );

    cmd.arg("geiger").assert_failure().stderr_eq(str![[r#"
Warning: `forge geiger` is deprecated, as it is just an alias for `forge lint --only-lint unsafe-cheatcode`

Error: Encountered invalid solc version in test/multiple.t.sol: No solc version exists that matches the version requirement: =0.8.33

"#]]);
});
