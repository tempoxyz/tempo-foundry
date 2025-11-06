// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {ITIP20} from "tempo-std/interfaces/ITIP20.sol";
import {ITIP20RolesAuth} from "tempo-std/interfaces/ITIP20RolesAuth.sol";
import {ITIP20Factory} from "tempo-std/interfaces/ITIP20Factory.sol";
import {StdPrecompiles} from "tempo-std/StdPrecompiles.sol";
import {Mail} from "../src/Mail.sol";

contract MailScript is Script {
    ITIP20Factory internal constant TIP20_FACTORY = ITIP20Factory(StdPrecompiles.TIP20_FACTORY_ADDRESS);
    ITIP20 internal constant LINKING_USD = ITIP20(StdPrecompiles.LINKING_USD_ADDRESS);

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        ITIP20 token = ITIP20(TIP20_FACTORY.createToken("testUSD", "tUSD", "USD", LINKING_USD, msg.sender));
        ITIP20RolesAuth(address(token)).grantRole(keccak256("ISSUER_ROLE"), msg.sender);
        token.mint(msg.sender, 1_000_000 * 10 ** token.decimals());

        new Mail(token);

        vm.stopBroadcast();
    }
}
