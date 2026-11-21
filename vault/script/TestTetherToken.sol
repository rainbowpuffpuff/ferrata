// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

/// @notice A 6-decimal token with Tether's (USDT's) quirks, for the web depositor's test: `approve`, `transfer` and
///         `transferFrom` return nothing, and `approve` refuses to change a non-zero allowance into another non-zero
///         one (mainnet TetherToken: `require(!((_value != 0) && (allowed[msg.sender][_spender] != 0)))`). Open
///         minting. It stands in for a registered token; nothing about it is part of the vault.
contract TestTetherToken {
    string public constant name = "Ferrata Test Tether";
    string public constant symbol = "TUSDT";
    uint8 public constant decimals = 6;
    uint256 public totalSupply;
    mapping(address account => uint256) public balanceOf;
    mapping(address owner => mapping(address spender => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    function mint(address to, uint256 amount) external {
        totalSupply += amount;
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    function approve(address spender, uint256 value) external {
        require(!((value != 0) && (allowance[msg.sender][spender] != 0)), "approve: reset to zero first");
        allowance[msg.sender][spender] = value;
        emit Approval(msg.sender, spender, value);
    }

    function transfer(address to, uint256 value) external {
        _move(msg.sender, to, value);
    }

    function transferFrom(address from, address to, uint256 value) external {
        uint256 current = allowance[from][msg.sender];
        require(current >= value, "allowance");
        if (current != type(uint256).max) allowance[from][msg.sender] = current - value;
        _move(from, to, value);
    }

    function _move(address from, address to, uint256 value) internal {
        require(balanceOf[from] >= value, "balance");
        balanceOf[from] -= value;
        balanceOf[to] += value;
        emit Transfer(from, to, value);
    }
}
