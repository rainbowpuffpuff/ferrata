// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";

/// @notice Accepts exactly the (imageId, journalDigest) pairs a test has registered with `accept`. The seal is
///         ignored: what the tests check is that the vault asks about the right image and the right journal.
contract MockRiscZeroVerifier is IRiscZeroVerifier {
    error VerificationFailed();

    mapping(bytes32 imageId => mapping(bytes32 journalDigest => bool)) public accepted;

    function accept(bytes32 imageId, bytes32 journalDigest) external {
        accepted[imageId][journalDigest] = true;
    }

    function verify(bytes calldata, bytes32 imageId, bytes32 journalDigest) external view {
        if (!accepted[imageId][journalDigest]) revert VerificationFailed();
    }
}

/// @notice Rejects every proof.
contract RejectingVerifier is IRiscZeroVerifier {
    error AlwaysRejects();

    function verify(bytes calldata, bytes32, bytes32) external pure {
        revert AlwaysRejects();
    }
}
