// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IRiscZeroVerifier as IRiscZeroVerifierFull, Receipt} from "risc0/IRiscZeroVerifier.sol";

/// @notice A broken proof system, for tests only: it accepts every seal for every image id and every journal. The
///         broken-verifier invariant suite puts it behind RISC Zero's unmodified RiscZeroVerifierEmergencyStop, the
///         wrapper a deployment uses, so the model is "the Groth16 verifier (or the proof system under it) is
///         unsound, the emergency stop in front of it still works".
contract AcceptAllVerifier is IRiscZeroVerifierFull {
    function verify(bytes calldata, bytes32, bytes32) external pure {}

    function verifyIntegrity(Receipt calldata) external pure {}
}
