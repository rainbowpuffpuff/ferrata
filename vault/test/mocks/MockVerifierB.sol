// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IRiscZeroVerifier as IRiscZeroVerifierFull, Receipt} from "risc0/IRiscZeroVerifier.sol";

/// @notice A second proof system, for the migration rehearsal only (RFP-022 Soft Requirement 4: the proof-system
///         components are pluggable). It stands for a new zkVM release or a compressed proof format: it implements
///         the same `IRiscZeroVerifier` interface the vault pins (`MockRiscZeroVerifier` is proof system A), but it
///         requires seals in its own format, and a deployment pairs it with that deployment's own image ids.
///         Deployable behind RISC Zero's unmodified `RiscZeroVerifierEmergencyStop`, like the base one.
contract MockRiscZeroVerifierB is IRiscZeroVerifierFull {
    /// @notice The first four bytes of every system-B seal: its format selector. A seal that does not start with it
    ///         is not a proof of this proof system (a system-A seal among them).
    bytes4 public constant SEAL_FORMAT = bytes4(0xB2D0B2D0);

    error VerificationFailed();
    error WrongSealFormat();

    mapping(bytes32 imageId => mapping(bytes32 journalDigest => bool)) public accepted;
    mapping(bytes32 claimDigest => bool) public acceptedClaims;

    function accept(bytes32 imageId, bytes32 journalDigest) external {
        accepted[imageId][journalDigest] = true;
    }

    /// @dev For `verifyIntegrity`, which the emergency stop's proof-of-exploit path calls.
    function acceptClaim(bytes32 claimDigest) external {
        acceptedClaims[claimDigest] = true;
    }

    function verify(bytes calldata seal, bytes32 imageId, bytes32 journalDigest) external view {
        if (seal.length < 4 || bytes4(seal[:4]) != SEAL_FORMAT) revert WrongSealFormat();
        if (!accepted[imageId][journalDigest]) revert VerificationFailed();
    }

    function verifyIntegrity(Receipt calldata receipt) external view {
        if (!acceptedClaims[receipt.claimDigest]) revert VerificationFailed();
    }
}
