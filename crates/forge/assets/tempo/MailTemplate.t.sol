// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {ITIP20} from "tempo-std/interfaces/ITIP20.sol";
import {ITIP20Factory} from "tempo-std/interfaces/ITIP20Factory.sol";
import {StdPrecompiles} from "tempo-std/StdPrecompiles.sol";
import {Mail} from "../src/Mail.sol";

// TODO: replace this when available
interface ITIP20RolesAuth is ITIP20 {
    error Unauthorized();

    event RoleMembershipUpdated(bytes32 indexed role, address indexed account, address indexed sender, bool hasRole);

    event RoleAdminUpdated(bytes32 indexed role, bytes32 indexed newAdminRole, address indexed sender);

    function grantRole(bytes32 role, address account) external;

    function revokeRole(bytes32 role, address account) external;

    function renounceRole(bytes32 role) external;

    function setRoleAdmin(bytes32 role, bytes32 adminRole) external;
}

contract MailTest is Test {
    ITIP20Factory internal constant TIP20_FACTORY = ITIP20Factory(StdPrecompiles.TIP20_FACTORY_ADDRESS);

    ITIP20RolesAuth public token;
    Mail public mail;

    address public constant ALICE = address(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266);
    address public constant BOB = address(0x70997970C51812dc3A010C7d01b50e0d17dc79C8);

    function setUp() public {
        token = ITIP20RolesAuth(
            TIP20_FACTORY.createToken("testUSD", "tUSD", "USD", StdPrecompiles.LINKING_USD_ADDRESS, address(this))
        );
        token.grantRole(keccak256("MINTER_ROLE"), address(this));
        token.mint(ALICE, 100000 * 10 ** token.decimals());
        mail = new Mail(token);
    }

    function test_SendMail() public {
        Mail.Attachment memory attachment =
            Mail.Attachment({amount: 100 * 10 ** token.decimals(), memo: "Invoice #1234"});

        vm.prank(ALICE);
        token.approve(address(mail), attachment.amount);

        vm.prank(ALICE);
        mail.sendMail(BOB, "Hello, this is a unit test mail.", attachment);

        assertEq(token.balanceOf(BOB), attachment.amount);
    }

    function testFuzz_SendMail(uint256 amount, bytes32 memo) public {
        amount = bound(amount, 1, 10000 * 10 ** token.decimals());

        Mail.Attachment memory attachment = Mail.Attachment({amount: amount, memo: memo});

        vm.prank(ALICE);
        token.approve(address(mail), attachment.amount);

        vm.prank(ALICE);
        mail.sendMail(BOB, "Hello, this is a fuzzed test mail.", attachment);

        assertEq(token.balanceOf(BOB), attachment.amount);
    }
}
