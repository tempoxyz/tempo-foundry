// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "utils/Test.sol";

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

contract ExecuteTransactionTest is Test {
    // PathUSD address on Tempo (precompile)
    address constant PATH_USD = 0x20C0000000000000000000000000000000000000;

    function test_execute_erc20_transfer() public {
        // Alice is the sender (derived from private key 0xa316311a767126072c26570baddf4e16ad797e636bb9374427ace6eebeaad43f)
        address alice = 0x4d6004af73ca7CE5F5879079D8E2d6aFa6316646;
        // Bob is the recipient
        address bob = 0x70CF146aB98ffD5dE24e75dd7423F16181Da8E13;

        uint256 transferAmount = 100;

        // Setup: transfer PathUSD from test contract to alice
        IERC20(PATH_USD).transfer(alice, 1000);

        assertEq(IERC20(PATH_USD).balanceOf(alice), 1000, "alice should have 1000 PathUSD");
        uint256 bobInitialBalance = IERC20(PATH_USD).balanceOf(bob);

        /*
        Signed legacy transaction - ERC20 transfer:
        - from: 0x4d6004af73ca7CE5F5879079D8E2d6aFa6316646 (alice)
        - to: 0x20C0000000000000000000000000000000000000 (PathUSD)
        - value: 0
        - data: transfer(0x70CF146aB98ffD5dE24e75dd7423F16181Da8E13, 100)
        - gas: 100000
        - gas_price: 100
        - nonce: 0
        - chain_id: 1
        */
        vm.executeTransaction(
            hex"f8a58064830186a09420c000000000000000000000000000000000000080b844a9059cbb00000000000000000000000070cf146ab98ffd5de24e75dd7423f16181da8e13000000000000000000000000000000000000000000000000000000000000006426a06243865def3a0d39f45c4ec94396fc3c9203bfa4bf4cc6cf5eff5eb176129b11a064b96ef451ff7fc39a8082b84b69bb55742973d0b4fedf5d83b6db8b2699c46e"
        );

        // Assertion: bob received the tokens, alice's balance decreased
        assertEq(
            IERC20(PATH_USD).balanceOf(bob), bobInitialBalance + transferAmount, "bob should have received PathUSD"
        );
        assertEq(IERC20(PATH_USD).balanceOf(alice), 1000 - transferAmount, "alice balance should decrease");
    }

    function test_state_inheritance() public {
        address alice = 0x4d6004af73ca7CE5F5879079D8E2d6aFa6316646;
        address bob = 0x70CF146aB98ffD5dE24e75dd7423F16181Da8E13;

        uint256 transferAmount = 100;

        // Pre-fund bob with some PathUSD balance to verify state inheritance
        IERC20(PATH_USD).transfer(bob, 500);
        uint256 bobInitialBalance = IERC20(PATH_USD).balanceOf(bob);

        // Fund alice
        IERC20(PATH_USD).transfer(alice, 1000);

        assertEq(IERC20(PATH_USD).balanceOf(alice), 1000);
        assertEq(IERC20(PATH_USD).balanceOf(bob), bobInitialBalance);

        // Execute the same transaction
        vm.executeTransaction(
            hex"f8a58064830186a09420c000000000000000000000000000000000000080b844a9059cbb00000000000000000000000070cf146ab98ffd5de24e75dd7423f16181da8e13000000000000000000000000000000000000000000000000000000000000006426a06243865def3a0d39f45c4ec94396fc3c9203bfa4bf4cc6cf5eff5eb176129b11a064b96ef451ff7fc39a8082b84b69bb55742973d0b4fedf5d83b6db8b2699c46e"
        );

        // Bob's balance should be initial + received
        assertEq(
            IERC20(PATH_USD).balanceOf(bob),
            bobInitialBalance + transferAmount,
            "bob should have initial + received amount"
        );
        assertEq(IERC20(PATH_USD).balanceOf(alice), 1000 - transferAmount, "alice balance should decrease");
    }

    function test_revert_invalid_rlp() public {
        vm._expectCheatcodeRevert("failed to decode RLP-encoded transaction: unexpected string");
        vm.executeTransaction(hex"0102");
    }
}
