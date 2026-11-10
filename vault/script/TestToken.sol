// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice A 6-decimal ERC-20 with open minting, for the return-leg test and the testnet round trip. It stands in
///         for a registered token; nothing about it is part of the vault.
contract TestToken is ERC20 {
    constructor() ERC20("Ferrata Test USD", "TUSD") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
