// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {ITIP20} from "tempo-std/interfaces/ITIP20.sol";
import {Mail} from "../src/Mail.sol";

contract MailScript is Script {
    Mail public mail;

    function setUp() public {}

    function run(ITIP20 token) public {
        vm.startBroadcast();

        mail = new Mail(token);

        vm.stopBroadcast();
    }
}
