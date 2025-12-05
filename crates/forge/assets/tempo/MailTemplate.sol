// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {ITIP20} from "tempo-std/interfaces/ITIP20.sol";

contract SimpleMail {
    event SimpleMailSent(address indexed from, address indexed to, string message);

    mapping(address => uint256) public balances;

    function sendMail(address to, string memory message, uint256 amount) external {
        balances[to] += amount;

        emit SimpleMailSent(msg.sender, to, message);
    }
}

contract Mail {
    event MailSent(address indexed from, address indexed to, string message, Attachment attachment);

    struct Attachment {
        uint256 amount;
        bytes32 memo;
    }

    ITIP20 public token;

    constructor(ITIP20 token_) {
        token = token_;
    }

    function sendMail(address to, string memory message, Attachment memory attachment) external {
        token.transferFromWithMemo(msg.sender, to, attachment.amount, attachment.memo);

        emit MailSent(msg.sender, to, message, attachment);
    }
}
