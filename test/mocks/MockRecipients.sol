// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

/// @notice Refuses ETH.
contract RejectingRecipient {
    error NoEthPlease();

    receive() external payable {
        revert NoEthPlease();
    }
}

/// @notice On receiving ETH, makes one arbitrary call (meant to re-enter the vault), records how it ended, and
///         accepts the ETH regardless.
contract ReentrantRecipient {
    address public hookTarget;
    bytes public hookData;
    bool public hookCalled;
    bool public hookSucceeded;
    bytes public hookReturnData;

    function setHook(address target, bytes calldata data) external {
        hookTarget = target;
        hookData = data;
    }

    receive() external payable {
        if (hookTarget == address(0)) return;
        address target = hookTarget;
        hookTarget = address(0); // one attempt per arming
        hookCalled = true;
        (hookSucceeded, hookReturnData) = target.call(hookData);
    }
}

interface IFerrataVaultClaims {
    function claimOwed(address token) external;
    function depositETH(uint8 denomIndex, bytes32 commitment, uint64 index) external payable;
}

/// @notice A contract that accepts ETH only while `open`, and can deposit into and claim from the vault itself.
contract ToggleRecipient {
    error Closed();

    bool public open;

    function setOpen(bool open_) external {
        open = open_;
    }

    function claim(address vault, address token) external {
        IFerrataVaultClaims(vault).claimOwed(token);
    }

    function depositETH(address vault, uint8 denomIndex, bytes32 commitment, uint64 index) external payable {
        IFerrataVaultClaims(vault).depositETH{value: msg.value}(denomIndex, commitment, index);
    }

    receive() external payable {
        if (!open) revert Closed();
    }
}

/// @notice Refuses ETH by reverting with a 600 kB buffer (about 743k gas of memory, which it can afford inside a
///         1M-gas transaction), so a sender that copied the return data would run out of gas doing so.
contract RevertBombRecipient {
    receive() external payable {
        assembly {
            revert(0, 600000)
        }
    }
}

/// @notice Refuses ETH by burning every unit of gas it is given.
contract GasBurnerRecipient {
    uint256 public sink;

    receive() external payable {
        while (true) {
            sink += 1;
        }
    }
}
