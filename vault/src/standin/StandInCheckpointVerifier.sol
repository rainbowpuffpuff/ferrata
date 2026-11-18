// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

import {IEmergencyStop} from "../interfaces/IEmergencyStop.sol";
import {IRiscZeroVerifier} from "../interfaces/IRiscZeroVerifier.sol";

/// @title StandInCheckpointVerifier: a TESTNET STAND-IN for Ferrata's checkpoint proof. Not part of the design.
/// @notice Read this before trusting a vault that names this contract as its `verifier`.
///
///         What it is. Ferrata's vault accepts a LEZ burn root only with a zero-knowledge proof of its checkpoint
///         program (rule (B): `checkpoint_b`, which checks LEZ v0.4 blocks, their inscriptions on Logos Blockchain and
///         the signatures of `t` of the zone's sequencers over the final tip). On a zone whose blocks that program
///         cannot read and whose sequencer runs no attestor (the public LEZ testnet on LEZ v0.3.1, November 2026) no
///         such proof can exist. This contract takes the place of that one proof and of nothing else: for the single
///         image id `STAND_IN_IMAGE_ID` it accepts, instead of a proof, signatures by `threshold` of a fixed set of
///         ECDSA keys over the journal digest the vault computed. Every other image id is passed, unchanged, to RISC
///         Zero's own verifier behind RISC Zero's emergency stop (`riscZero`), so a release from such a vault still
///         needs a real zero-knowledge proof of the unchanged `release` program.
///
///         What it does not establish. The signers are keys held by whoever deployed this contract, not the zone's
///         sequencers. A burn root accepted through this contract is therefore the signers' word, and nothing in it is
///         checked against LEZ blocks or Logos Blockchain: `threshold` of the keys can make the vault accept any burn
///         root and so pay out its whole escrow. It is a bridge with a trusted committee for the outbound root, which
///         is exactly what Ferrata's design exists to avoid. It is not in the proposal's design, not audited, and
///         never for production: the constructor refuses every chain but anvil, Sepolia, Hoodi and Holesky.
///
///         What stays as designed in a vault that uses it: the vault's bytecode and all its rules (the candidate's
///         public wait, finalisation, the release delay, nullifiers, caps, freezes, revocation), the deposits, and
///         every release proof.
/// @dev The stand-in id is a constant, the SHA-256 of a label: it is no program's image id, so the vault's
///      `checkpointImageId()` shows at once that it is not `checkpoint_b`, and this contract can never be configured
///      to answer for a real program (in particular not for `release`). There is no owner, no setter and no upgrade:
///      the key set and the threshold are fixed at construction.
contract StandInCheckpointVerifier is IRiscZeroVerifier, IEmergencyStop {
    /// @notice sha256("ferrata/stand-in/checkpoint-attestation/v1"): the only image id answered here with signatures.
    bytes32 public constant STAND_IN_IMAGE_ID = 0x66ff80f9020fb4926743819c158cc5f797ee1a19777e27a472147bbaf6e2dc38;

    /// @notice keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
    bytes32 internal constant DOMAIN_TYPEHASH = 0x8b73c3c69bb8fe3d512ecc4cf759cc79239f7b179b0ffacaa9a75d522b39400f;
    /// @notice keccak256("Ferrata stand-in checkpoint attestation (testnet only)")
    bytes32 internal constant NAME_HASH = 0xdf8694736c90c81e7f92778f7d15b56f82ede23aa6aa4985220f29e5d99506c7;
    /// @notice keccak256("1")
    bytes32 internal constant VERSION_HASH = 0xc89efdaa54c0f20c7adf612882df0950f5a951637e0307cdcb4c672f298b8bc6;
    /// @notice keccak256("CheckpointAttestation(bytes32 imageId,bytes32 journalDigest)")
    bytes32 internal constant ATTESTATION_TYPEHASH = 0x8048d964f510a124a4c82c2c036e50a4c497813ecd07336cee7dbe8dfdcdf99c;

    uint256 internal constant SIGNATURE_LENGTH = 65;
    uint256 public constant MAX_SIGNERS = 16;

    /// @notice RISC Zero's `RiscZeroVerifierEmergencyStop` around its Groth16 verifier: every image id but the stand-in
    ///         one is verified there, and `paused()` is its stop.
    IRiscZeroVerifier public immutable riscZero;
    /// @notice How many distinct keys of the set must sign.
    uint256 public immutable threshold;

    address[] internal _signers;
    mapping(address => bool) public isSigner;

    /// @notice Emitted once, at construction: what this stand-in forwards to and whose word it takes.
    event StandInDeployed(address indexed riscZero, bytes32 standInImageId, uint256 threshold, address[] signers);

    error NotATestnet();
    error RiscZeroHasNoCode();
    error RiscZeroNotAStop();
    error InvalidSignerSet();
    error InvalidThreshold();
    error Stopped();
    error SealLength();
    error BelowThreshold();
    error InvalidSignature();
    error NotASigner();
    error SignersNotAscending();

    /// @param riscZero_ RISC Zero's emergency stop (a deployed, untripped `RiscZeroVerifierEmergencyStop`).
    /// @param signers_ The key set: 1 to 16 non-zero addresses in strictly ascending order.
    /// @param threshold_ 1 to `signers_.length`.
    constructor(IRiscZeroVerifier riscZero_, address[] memory signers_, uint256 threshold_) {
        uint256 id = block.chainid;
        if (id != 31_337 && id != 11_155_111 && id != 560_048 && id != 17_000) revert NotATestnet();
        if (address(riscZero_).code.length == 0) revert RiscZeroHasNoCode();
        try IEmergencyStop(address(riscZero_)).paused() returns (bool stopped) {
            if (stopped) revert RiscZeroNotAStop();
        } catch {
            revert RiscZeroNotAStop();
        }
        uint256 n = signers_.length;
        if (n == 0 || n > MAX_SIGNERS) revert InvalidSignerSet();
        address previous = address(0);
        for (uint256 i = 0; i < n; i++) {
            // Strictly ascending from address(0): no zero address, no duplicate.
            if (signers_[i] <= previous) revert InvalidSignerSet();
            previous = signers_[i];
            isSigner[previous] = true;
        }
        if (threshold_ == 0 || threshold_ > n) revert InvalidThreshold();
        riscZero = riscZero_;
        threshold = threshold_;
        _signers = signers_;
        emit StandInDeployed(address(riscZero_), STAND_IN_IMAGE_ID, threshold_, signers_);
    }

    /// @notice The stand-in key set, ascending.
    function signers() external view returns (address[] memory) {
        return _signers;
    }

    /// @notice RISC Zero's emergency stop, as the vault reads it. Once it is tripped nothing verifies here either.
    function paused() public view returns (bool) {
        return IEmergencyStop(address(riscZero)).paused();
    }

    /// @notice The EIP-712 domain separator on the chain this call runs on (never cached: a signature made for one
    ///         chain id is not valid on another).
    function domainSeparator() public view returns (bytes32) {
        // EIP-712's own expressions, kept readable rather than hand-optimised (as the vault's beacon nonce).
        // forge-lint: disable-next-line(asm-keccak256)
        return keccak256(abi.encode(DOMAIN_TYPEHASH, NAME_HASH, VERSION_HASH, block.chainid, address(this)));
    }

    /// @notice What a stand-in key signs for the journal digest `journalDigest`: the EIP-712 digest of
    ///         `CheckpointAttestation(STAND_IN_IMAGE_ID, journalDigest)` under this contract's domain (this chain id
    ///         and this address). The journal digest is the vault's own, which binds the vault's chain id, its
    ///         address and its `lezConfigId`.
    function attestationDigest(bytes32 journalDigest) public view returns (bytes32) {
        // forge-lint: disable-next-line(asm-keccak256)
        bytes32 structHash = keccak256(abi.encode(ATTESTATION_TYPEHASH, STAND_IN_IMAGE_ID, journalDigest));
        // forge-lint: disable-next-line(asm-keccak256)
        return keccak256(abi.encodePacked(hex"1901", domainSeparator(), structHash));
    }

    /// @inheritdoc IRiscZeroVerifier
    /// @dev `imageId == STAND_IN_IMAGE_ID`: `seal` is k concatenated 65-byte signatures (r, s, v), threshold <= k <=
    ///      the size of the set, each by a key of the set, in strictly ascending order of signer address (so no key
    ///      counts twice), each over `attestationDigest(journalDigest)`; reverts otherwise, and after RISC Zero's stop.
    ///      Any other image id: RISC Zero's verifier decides, behind its stop.
    function verify(bytes calldata seal, bytes32 imageId, bytes32 journalDigest) external view {
        if (imageId != STAND_IN_IMAGE_ID) {
            riscZero.verify(seal, imageId, journalDigest);
            return;
        }
        // Nothing is accepted after RISC Zero's stop, as for every proof behind it.
        if (paused()) revert Stopped();
        uint256 count = seal.length / SIGNATURE_LENGTH;
        if (seal.length == 0 || seal.length % SIGNATURE_LENGTH != 0 || count > _signers.length) revert SealLength();
        if (count < threshold) revert BelowThreshold();
        bytes32 digest = attestationDigest(journalDigest);
        address previous = address(0);
        for (uint256 i = 0; i < count; i++) {
            uint256 at = i * SIGNATURE_LENGTH;
            bytes32 r = bytes32(seal[at:at + 32]);
            bytes32 s = bytes32(seal[at + 32:at + 64]);
            uint8 v = uint8(seal[at + 64]);
            // OpenZeppelin's recovery: refuses s in the upper half of the curve order and v other than 27 or 28.
            (address signer, ECDSA.RecoverError err,) = ECDSA.tryRecover(digest, v, r, s);
            if (err != ECDSA.RecoverError.NoError) revert InvalidSignature();
            if (!isSigner[signer]) revert NotASigner();
            if (signer <= previous) revert SignersNotAscending();
            previous = signer;
        }
    }
}
