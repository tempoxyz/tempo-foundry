//! Core test functionality tests

use foundry_test_utils::str;

forgetest_init!(failing_test_after_failed_setup, |prj, cmd| {
    prj.add_test(
        "FailingTestAfterFailedSetup.t.sol",
        r#"
import "forge-std/Test.sol";

contract FailingTestAfterFailedSetupTest is Test {
    function setUp() public {
        assertTrue(false);
    }

    function testAssertSuccess() public {
        assertTrue(true);
    }

    function testAssertFailure() public {
        assertTrue(false);
    }
}
"#,
    );

    cmd.arg("test").assert_failure().stdout_eq(str![""]);
});

forgetest_init!(legacy_assertions, |prj, cmd| {
    prj.add_test(
        "LegacyAssertions.t.sol",
        r#"
import "forge-std/Test.sol";

contract NoAssertionsRevertTest is Test {
    function testMultipleAssertFailures() public {
        vm.assertEq(uint256(1), uint256(2));
        vm.assertLt(uint256(5), uint256(4));
    }
}

/// forge-config: default.legacy_assertions = true
contract LegacyAssertionsTest {
    bool public failed;

    function testFlagNotSetSuccess() public {}

    function testFlagSetFailure() public {
        failed = true;
    }
}
"#,
    );

    cmd.args(["test", "-j1"]).assert_failure().stdout_eq(str![""]);
});

forgetest_init!(payment_failure, |prj, cmd| {
    prj.add_test(
        "PaymentFailure.t.sol",
        r#"
import "forge-std/Test.sol";

contract Payable {
    function pay() public payable {}
}

contract PaymentFailureTest is Test {
    function testCantPay() public {
        Payable target = new Payable();
        vm.prank(address(1));
        target.pay{value: 1}();
    }
}
"#,
    );

    cmd.arg("test").assert_failure().stdout_eq(str![""]);
});
