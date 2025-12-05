// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {ITIP20} from "tempo-std/interfaces/ITIP20.sol";
import {ITIP20RolesAuth} from "tempo-std/interfaces/ITIP20RolesAuth.sol";
import {StdPrecompiles} from "tempo-std/StdPrecompiles.sol";
import {SimpleMail, Mail} from "../src/Mail.sol";

contract MailScript is Script {
    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        ITIP20 token = ITIP20(
            StdPrecompiles.TIP20_FACTORY.createToken("testUSD", "tUSD", "USD", StdPrecompiles.PATH_USD, msg.sender)
        );
        ITIP20RolesAuth(address(token)).grantRole(token.ISSUER_ROLE(), msg.sender);
        token.mint(msg.sender, 1_000_000 * 10 ** token.decimals());

        new Mail(token);

        vm.stopBroadcast();
    }

    function sendSimpleMail() public {
        vm.startBroadcast();

        SimpleMail mail = new SimpleMail();
        mail.sendMail(makeAddr("alice"), "Hello Alice, this is a simple unit test mail.", 1000);

        vm.stopBroadcast();
    }
}
