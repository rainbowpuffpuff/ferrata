// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {StandInCheckpointVerifier} from "../../src/standin/StandInCheckpointVerifier.sol";

/// @notice Shared by the stand-in's tests: RISC Zero's unmodified Groth16 verifier behind RISC Zero's unmodified
///         emergency stop, and a stand-in in front of it with three test keys and threshold two, on Sepolia's chain id.
///         The EIP-712 digest is rebuilt here from its written definition, not with the contract's own function.
abstract contract StandInBase is Test {
    uint256 internal constant SEPOLIA = 11_155_111;
    /// The final programs' ids under RISC Zero 3.1 (docs/image-ids.md).
    bytes32 internal constant RELEASE_IMAGE_ID = 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd;
    bytes32 internal constant CHECKPOINT_B_IMAGE_ID = 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0;
    bytes32 internal constant CHECKPOINT_A_IMAGE_ID = 0x7c93425e9b6fedbd0d79092b3a0b6c164b9a6ee250b9e2e89b26a7fadc4497c1;
    uint256 internal constant SECP256K1_N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141;

    RiscZeroGroth16Verifier internal groth16;
    RiscZeroVerifierEmergencyStop internal estop;
    StandInCheckpointVerifier internal standIn;
    /// `standIn.STAND_IN_IMAGE_ID()`, read once (a call between vm.expectRevert or vm.prank and its target would be
    /// taken for the target).
    bytes32 internal standInId;
    address internal guardian = makeAddr("guardian of RISC Zero's emergency stop");

    /// The stand-in keys, ascending by address.
    uint256[] internal keys;
    address[] internal signerSet;
    uint256 internal outsiderKey;

    function _setUpStandIn() internal {
        vm.chainId(SEPOLIA);
        groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        estop = new RiscZeroVerifierEmergencyStop(groth16, guardian);
        _makeKeys(3);
        (, outsiderKey) = makeAddrAndKey("a key outside the stand-in set");
        standIn = new StandInCheckpointVerifier(IRiscZeroVerifier(address(estop)), signerSet, 2);
        standInId = standIn.STAND_IN_IMAGE_ID();
    }

    function _makeKeys(uint256 n) internal {
        delete keys;
        delete signerSet;
        for (uint256 i = 0; i < n; i++) {
            (address a, uint256 k) = makeAddrAndKey(string.concat("stand-in signer ", vm.toString(i)));
            keys.push(k);
            signerSet.push(a);
        }
        // Insertion sort by address, keys along.
        for (uint256 i = 1; i < n; i++) {
            for (uint256 j = i; j > 0 && signerSet[j - 1] > signerSet[j]; j--) {
                (signerSet[j - 1], signerSet[j]) = (signerSet[j], signerSet[j - 1]);
                (keys[j - 1], keys[j]) = (keys[j], keys[j - 1]);
            }
        }
    }

    /// @dev EIP-712 from its definition: the domain (name, version, chain id, verifying contract) and the struct
    ///      CheckpointAttestation(bytes32 imageId, bytes32 journalDigest).
    function _digest(uint256 chainId, address verifier, bytes32 imageId, bytes32 journalDigest)
        internal
        pure
        returns (bytes32)
    {
        bytes32 domain = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256("Ferrata stand-in checkpoint attestation (testnet only)"),
                keccak256("1"),
                chainId,
                verifier
            )
        );
        bytes32 structHash = keccak256(
            abi.encode(
                keccak256("CheckpointAttestation(bytes32 imageId,bytes32 journalDigest)"), imageId, journalDigest
            )
        );
        return keccak256(abi.encodePacked("\x19\x01", domain, structHash));
    }

    function _sig(uint256 key, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, digest);
        return abi.encodePacked(r, s, v);
    }

    /// @dev The seal a threshold of the set makes for `journalDigest`: signers 0 and 1 (ascending).
    function _seal(bytes32 journalDigest) internal view returns (bytes memory) {
        bytes32 d = _digest(block.chainid, address(standIn), standInId, journalDigest);
        return bytes.concat(_sig(keys[0], d), _sig(keys[1], d));
    }
}
