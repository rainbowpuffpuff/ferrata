// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

/// @notice The one function of RISC Zero's `RiscZeroVerifierEmergencyStop` the vault reads (OpenZeppelin `Pausable`).
/// @dev True once the stop has been tripped; it is never undone.
interface IEmergencyStop {
    function paused() external view returns (bool);
}
