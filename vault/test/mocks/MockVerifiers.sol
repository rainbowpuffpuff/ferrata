// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IRiscZeroVerifier as IRiscZeroVerifierFull, Receipt} from "risc0/IRiscZeroVerifier.sol";

/// @notice Accepts exactly the (imageId, journalDigest) pairs a test has registered with `accept`. The seal is
///         ignored: what the tests check is that the vault asks about the right image and the right journal.
///         Implements RISC Zero's full verifier interface, so it can sit behind RiscZeroVerifierEmergencyStop.
contract MockRiscZeroVerifier is IRiscZeroVerifierFull {
    error VerificationFailed();

    mapping(bytes32 imageId => mapping(bytes32 journalDigest => bool)) public accepted;
    mapping(bytes32 claimDigest => bool) public acceptedClaims;

    function accept(bytes32 imageId, bytes32 journalDigest) external {
        accepted[imageId][journalDigest] = true;
    }

    /// @dev For `verifyIntegrity`, which the emergency stop's proof-of-exploit path calls.
    function acceptClaim(bytes32 claimDigest) external {
        acceptedClaims[claimDigest] = true;
    }

    function verify(bytes calldata, bytes32 imageId, bytes32 journalDigest) external view {
        if (!accepted[imageId][journalDigest]) revert VerificationFailed();
    }

    function verifyIntegrity(Receipt calldata receipt) external view {
        if (!acceptedClaims[receipt.claimDigest]) revert VerificationFailed();
    }
}

/// @notice Rejects every proof.
contract RejectingVerifier is IRiscZeroVerifierFull {
    error AlwaysRejects();

    function verify(bytes calldata, bytes32, bytes32) external pure {
        revert AlwaysRejects();
    }

    function verifyIntegrity(Receipt calldata) external pure {
        revert AlwaysRejects();
    }
}
