// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice 6-decimal test token ("TUSD") with open minting.
contract TestToken is ERC20 {
    constructor() ERC20("Test USD", "TUSD") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

interface IOwedClaimer {
    function claimOwed(address token) external;
}

/// @notice A release recipient that rejects ETH until it is told to accept (so the vault's pull-payment
///         fallback is exercised), then collects what it is owed. Installed with vm.etch at the address the
///         proof binds, so its storage starts with `accepting == false`.
contract RejectingRecipient {
    bool public accepting;

    receive() external payable {
        require(accepting, "RejectingRecipient: not accepting");
    }

    function setAccepting(bool a) external {
        accepting = a;
    }

    function collect(address vault, address token) external {
        IOwedClaimer(vault).claimOwed(token);
    }
}
