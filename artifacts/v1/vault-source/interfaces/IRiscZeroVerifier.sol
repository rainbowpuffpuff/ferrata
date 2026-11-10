// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

/// @notice Minimal vendored subset of RISC Zero's verifier interface.
/// @dev `verify` returns nothing and reverts if the seal is not a valid proof that the
///      program `imageId` produced a journal whose SHA-256 digest is `journalDigest`.
interface IRiscZeroVerifier {
    function verify(bytes calldata seal, bytes32 imageId, bytes32 journalDigest) external view;
}
