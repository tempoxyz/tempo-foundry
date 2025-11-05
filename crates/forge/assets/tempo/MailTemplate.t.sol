// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {ITIP20} from "tempo-std/interfaces/ITIP20.sol";
import {ITIP20RolesAuth} from "tempo-std/interfaces/ITIP20RolesAuth.sol";
import {ITIP20Factory} from "tempo-std/interfaces/ITIP20Factory.sol";
import {StdPrecompiles} from "tempo-std/StdPrecompiles.sol";
import {Mail} from "../src/Mail.sol";

contract MailTest is Test {
    ITIP20Factory internal constant TIP20_FACTORY = ITIP20Factory(StdPrecompiles.TIP20_FACTORY_ADDRESS);

    ITIP20 public token;
    Mail public mail;

    address public constant ALICE = address(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266);
    address public constant BOB = address(0x70997970C51812dc3A010C7d01b50e0d17dc79C8);

    function setUp() public {
        token = ITIP20(
            TIP20_FACTORY.createToken(
                "testUSD", "tUSD", "USD", ITIP20(StdPrecompiles.LINKING_USD_ADDRESS), address(this)
            )
        );
        ITIP20RolesAuth(address(token)).grantRole(keccak256("ISSUER_ROLE"), ALICE);
        token.mint(ALICE, 100000 * 10 ** token.decimals());
        mail = new Mail(token);
    }

    function test_SendMail() public {
        Mail.Attachment memory attachment =
            Mail.Attachment({amount: 100 * 10 ** token.decimals(), memo: "Invoice #1234"});

        vm.startPrank(ALICE);
        token.approve(address(mail), attachment.amount);

        vm.prank(ALICE);
        mail.sendMail(BOB, "Hello, this is a unit test mail.", attachment);

        assertEq(token.balanceOf(BOB), attachment.amount);
    }

    function testFuzz_SendMail(uint256 amount, string memory message, bytes32 memo) public {
        amount = bound(amount, 1, 100000 * 10 ** token.decimals());

        Mail.Attachment memory attachment = Mail.Attachment({amount: amount, memo: memo});

        vm.prank(ALICE);
        token.approve(address(mail), attachment.amount);

        vm.prank(ALICE);
        mail.sendMail(BOB, message, attachment);

        assertEq(token.balanceOf(BOB), attachment.amount);
    }
}
