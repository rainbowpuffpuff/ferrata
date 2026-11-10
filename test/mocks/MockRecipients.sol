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
