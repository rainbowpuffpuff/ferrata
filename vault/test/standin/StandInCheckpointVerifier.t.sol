// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";

import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {StandInCheckpointVerifier} from "../../src/standin/StandInCheckpointVerifier.sol";
import {StandInBase} from "./StandInBase.sol";

/// @notice The stand-in verifier alone (src/standin/README.md): what it accepts for its one image id, what it refuses,
///         and that every other image id goes to RISC Zero's real verifier, checked with a real release seal of the
///         live run crown-live-3 (e2e/runs/crown-live-3/crown.json).
contract StandInCheckpointVerifierTest is StandInBase {
    bytes32 internal constant JOURNAL_DIGEST = keccak256("test: a checkpoint journal digest");
    /// A tampered Groth16 point makes the BN254 precompiles consume all the gas they are given.
    uint256 internal constant TAMPERED_CALL_GAS = 3_000_000;

    // crown-live-3's first release: its real Groth16 seal and the journal the vault built for it.
    bytes internal releaseSeal;
    bytes32 internal releaseDigest;
    bytes internal checkpointBSeal;

    function setUp() public {
        _setUpStandIn();
        string memory json = vm.readFile("../e2e/runs/crown-live-3/crown.json");
        releaseSeal = vm.parseJsonBytes(json, ".releases[0].seal");
        checkpointBSeal = vm.parseJsonBytes(json, ".candidate.seal");
        releaseDigest = sha256(
            abi.encode(
                sha256(bytes("ferrata/vault/release/v2")),
                vm.parseJsonUint(json, ".chainId"),
                vm.parseJsonAddress(json, ".vault"),
                bytes32(0x10837afeaf6402e9d4bc2c4a13039e70f88762f13336d9c222d2f6d3b6a95553), // its lezConfigId
                vm.parseJsonBytes32(json, ".releases[0].burnRoot"),
                vm.parseJsonBytes32(json, ".releases[0].nullifier"),
                vm.parseJsonAddress(json, ".releases[0].token"),
                uint256(0.001 ether),
                vm.parseJsonAddress(json, ".releases[0].recipient"),
                vm.parseJsonAddress(json, ".releases[0].relayer"),
                vm.parseJsonUint(json, ".releases[0].relayFee")
            )
        );
    }

    // ---- what it is ---------------------------------------------------------------------------------------------

    function test_theStandInIdIsALabelsHash_andNoProgramsId() public view {
        assertEq(standInId, sha256(bytes("ferrata/stand-in/checkpoint-attestation/v1")));
        assertTrue(standInId != RELEASE_IMAGE_ID && standInId != CHECKPOINT_B_IMAGE_ID);
        assertTrue(standInId != CHECKPOINT_A_IMAGE_ID);
    }

    function test_itsConfigurationIsWhatTheConstructorGot_andFixed() public view {
        assertEq(address(standIn.riscZero()), address(estop));
        assertEq(standIn.threshold(), 2);
        address[] memory s = standIn.signers();
        assertEq(s.length, 3);
        for (uint256 i = 0; i < 3; i++) {
            assertEq(s[i], signerSet[i]);
            assertTrue(standIn.isSigner(s[i]));
        }
        assertFalse(standIn.isSigner(vm.addr(outsiderKey)));
        assertFalse(standIn.paused());
    }

    function test_theSignedDigestIsEip712OfChainVerifierImageAndJournal() public view {
        assertEq(
            standIn.attestationDigest(JOURNAL_DIGEST), _digest(SEPOLIA, address(standIn), standInId, JOURNAL_DIGEST)
        );
    }

    // ---- the stand-in id: a threshold of the set, and nothing less ------------------------------------------------

    function test_acceptsAThreshold() public view {
        standIn.verify(_seal(JOURNAL_DIGEST), standInId, JOURNAL_DIGEST);
    }

    function test_acceptsEveryPairAndAllThree() public view {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        standIn.verify(bytes.concat(_sig(keys[0], d), _sig(keys[2], d)), standInId, JOURNAL_DIGEST);
        standIn.verify(bytes.concat(_sig(keys[1], d), _sig(keys[2], d)), standInId, JOURNAL_DIGEST);
        standIn.verify(bytes.concat(_sig(keys[0], d), _sig(keys[1], d), _sig(keys[2], d)), standInId, JOURNAL_DIGEST);
    }

    function test_refusesBelowThreshold() public {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        for (uint256 i = 0; i < 3; i++) {
            bytes memory one = _sig(keys[i], d);
            vm.expectRevert(StandInCheckpointVerifier.BelowThreshold.selector);
            standIn.verify(one, standInId, JOURNAL_DIGEST);
        }
    }

    function test_refusesAnEmptyOrMisalignedSeal_andMoreSignaturesThanKeys() public {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        vm.expectRevert(StandInCheckpointVerifier.SealLength.selector);
        standIn.verify("", standInId, JOURNAL_DIGEST);
        bytes memory seal = _seal(JOURNAL_DIGEST);
        bytes memory longer = bytes.concat(seal, hex"00");
        vm.expectRevert(StandInCheckpointVerifier.SealLength.selector);
        standIn.verify(longer, standInId, JOURNAL_DIGEST);
        bytes memory four = bytes.concat(seal, _sig(keys[2], d), _sig(outsiderKey, d));
        vm.expectRevert(StandInCheckpointVerifier.SealLength.selector);
        standIn.verify(four, standInId, JOURNAL_DIGEST);
    }

    function test_refusesAWrongSigner() public {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        address outsider = vm.addr(outsiderKey);
        // In either position: the order check comes after the membership check.
        bytes memory seal = outsider > signerSet[0]
            ? bytes.concat(_sig(keys[0], d), _sig(outsiderKey, d))
            : bytes.concat(_sig(outsiderKey, d), _sig(keys[0], d));
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
        // Two outsiders are not a threshold either.
        (, uint256 other) = makeAddrAndKey("another outsider");
        seal = bytes.concat(_sig(outsiderKey, d), _sig(other, d));
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
    }

    function test_refusesADuplicatedSigner() public {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        for (uint256 i = 0; i < 3; i++) {
            bytes memory twice = bytes.concat(_sig(keys[i], d), _sig(keys[i], d));
            vm.expectRevert(StandInCheckpointVerifier.SignersNotAscending.selector);
            standIn.verify(twice, standInId, JOURNAL_DIGEST);
        }
        // One key twice next to a second key is still one key twice.
        bytes memory three = bytes.concat(_sig(keys[0], d), _sig(keys[1], d), _sig(keys[1], d));
        vm.expectRevert(StandInCheckpointVerifier.SignersNotAscending.selector);
        standIn.verify(three, standInId, JOURNAL_DIGEST);
    }

    function test_refusesSignersOutOfOrder() public {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        bytes memory swapped = bytes.concat(_sig(keys[1], d), _sig(keys[0], d));
        vm.expectRevert(StandInCheckpointVerifier.SignersNotAscending.selector);
        standIn.verify(swapped, standInId, JOURNAL_DIGEST);
    }

    /// @notice The same signer cannot be counted twice through the other encoding of its signature (s' = n - s with
    ///         the other v), and v must be 27 or 28.
    function test_refusesAMalleatedSignature() public {
        bytes32 d = standIn.attestationDigest(JOURNAL_DIGEST);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(keys[1], d);
        bytes memory flipped = abi.encodePacked(r, bytes32(SECP256K1_N - uint256(s)), v == 27 ? uint8(28) : uint8(27));
        assertEq(ecrecover(d, v == 27 ? 28 : 27, r, bytes32(SECP256K1_N - uint256(s))), signerSet[1], "same signer");
        bytes memory seal = bytes.concat(_sig(keys[0], d), flipped);
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignature.selector);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
        bytes memory rawV = bytes.concat(_sig(keys[0], d), abi.encodePacked(r, s, v - 27));
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignature.selector);
        standIn.verify(rawV, standInId, JOURNAL_DIGEST);
    }

    function test_refusesASignatureForAnotherJournal() public {
        bytes memory seal = _seal(JOURNAL_DIGEST);
        bytes32 other = keccak256("test: another journal digest");
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(seal, standInId, other);
    }

    /// @notice A signature over another image id (here: the release program's) says nothing about a checkpoint.
    function test_refusesASignatureForAnotherImage() public {
        bytes32 d = _digest(SEPOLIA, address(standIn), RELEASE_IMAGE_ID, JOURNAL_DIGEST);
        bytes memory seal = bytes.concat(_sig(keys[0], d), _sig(keys[1], d));
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
    }

    /// @notice A second stand-in with the same keys (another vault's verifier) does not accept the first one's seal.
    function test_refusesASignatureForAnotherVerifier() public {
        StandInCheckpointVerifier second =
            new StandInCheckpointVerifier(IRiscZeroVerifier(address(estop)), signerSet, 2);
        bytes memory seal = _seal(JOURNAL_DIGEST);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        second.verify(seal, standInId, JOURNAL_DIGEST);
    }

    /// @notice The same contract at the same address on another chain id refuses a seal made for Sepolia, and a seal
    ///         made for that chain is refused on Sepolia.
    function test_refusesAReplayAcrossChains() public {
        bytes memory sepoliaSeal = _seal(JOURNAL_DIGEST);
        standIn.verify(sepoliaSeal, standInId, JOURNAL_DIGEST);
        vm.chainId(17_000);
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(sepoliaSeal, standInId, JOURNAL_DIGEST);
        bytes memory holeskySeal = _seal(JOURNAL_DIGEST);
        standIn.verify(holeskySeal, standInId, JOURNAL_DIGEST);
        vm.chainId(SEPOLIA);
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(holeskySeal, standInId, JOURNAL_DIGEST);
    }

    /// @notice A plain signature of the journal digest (no domain: not bound to chain, verifier or image) is refused.
    function test_refusesAnUndomainedSignature() public {
        bytes memory seal = bytes.concat(_sig(keys[0], JOURNAL_DIGEST), _sig(keys[1], JOURNAL_DIGEST));
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
    }

    function testFuzz_randomSealsAreRefused(bytes32 a, bytes32 b, bytes32 c, bytes32 e, uint8 v1, uint8 v2) public {
        bytes memory seal = abi.encodePacked(a, b, v1, c, e, v2);
        vm.expectRevert();
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
    }

    function testFuzz_anyJournalAThresholdSignedIsAccepted_andNoOther(bytes32 journal, bytes32 other) public {
        vm.assume(journal != other);
        bytes memory seal = _seal(journal);
        standIn.verify(seal, standInId, journal);
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        standIn.verify(seal, standInId, other);
    }

    // ---- every other image id: RISC Zero's real verifier decides ---------------------------------------------------

    function test_forwardsTheReleaseId_aRealReleaseSealVerifies() public view {
        standIn.verify(releaseSeal, RELEASE_IMAGE_ID, releaseDigest);
        // The same answer as RISC Zero's contracts give without the stand-in in front.
        estop.verify(releaseSeal, RELEASE_IMAGE_ID, releaseDigest);
        groth16.verify(releaseSeal, RELEASE_IMAGE_ID, releaseDigest);
    }

    function test_forwardsTheReleaseId_aRealSealFailsForAnotherJournalOrImage() public {
        bytes32 other = sha256(bytes("another release journal"));
        vm.expectRevert(VerificationFailed.selector);
        standIn.verify(releaseSeal, RELEASE_IMAGE_ID, other);
        vm.expectRevert(VerificationFailed.selector);
        standIn.verify(releaseSeal, CHECKPOINT_B_IMAGE_ID, releaseDigest);
    }

    function test_forwardsTheReleaseId_aTamperedSealFails() public {
        bytes memory bad = bytes.concat(releaseSeal);
        bad[100] ^= 0x01;
        vm.expectRevert();
        standIn.verify{gas: TAMPERED_CALL_GAS}(bad, RELEASE_IMAGE_ID, releaseDigest);
    }

    /// @notice The stand-in keys cannot stand in for a release proof: their signatures over a release journal, sent
    ///         for the release id, reach RISC Zero's verifier, which refuses them.
    function test_signaturesAreNotAReleaseProof() public {
        bytes32 d = _digest(SEPOLIA, address(standIn), RELEASE_IMAGE_ID, releaseDigest);
        bytes memory seal = bytes.concat(_sig(keys[0], d), _sig(keys[1], d), _sig(keys[2], d));
        vm.expectRevert();
        standIn.verify{gas: TAMPERED_CALL_GAS}(seal, RELEASE_IMAGE_ID, releaseDigest);
        seal = _seal(releaseDigest);
        vm.expectRevert();
        standIn.verify{gas: TAMPERED_CALL_GAS}(seal, RELEASE_IMAGE_ID, releaseDigest);
    }

    /// @notice A real proof is not an attestation: crown-live-3's checkpoint_b seal and its release seal, sent for the
    ///         stand-in id, are refused (260 bytes read as four signatures: more than the set has keys).
    function test_aRealSealIsNotAnAttestation() public {
        vm.expectRevert(StandInCheckpointVerifier.SealLength.selector);
        standIn.verify(checkpointBSeal, standInId, JOURNAL_DIGEST);
        vm.expectRevert(StandInCheckpointVerifier.SealLength.selector);
        standIn.verify(releaseSeal, standInId, releaseDigest);
    }

    /// @notice The real checkpoint programs' ids are not answered with signatures: they go to RISC Zero's verifier.
    function test_theRealCheckpointIdsAreForwarded_notSigned() public {
        bytes memory seal = _seal(JOURNAL_DIGEST);
        vm.expectRevert();
        standIn.verify{gas: TAMPERED_CALL_GAS}(seal, CHECKPOINT_B_IMAGE_ID, JOURNAL_DIGEST);
        vm.expectRevert();
        standIn.verify{gas: TAMPERED_CALL_GAS}(seal, CHECKPOINT_A_IMAGE_ID, JOURNAL_DIGEST);
    }

    function testFuzz_anyOtherImageIdIsForwarded(bytes32 imageId) public {
        vm.assume(imageId != standInId);
        bytes memory seal = _seal(JOURNAL_DIGEST);
        vm.expectRevert();
        standIn.verify{gas: TAMPERED_CALL_GAS}(seal, imageId, JOURNAL_DIGEST);
        if (imageId != RELEASE_IMAGE_ID) {
            vm.expectRevert(VerificationFailed.selector);
            standIn.verify(releaseSeal, imageId, releaseDigest);
        }
    }

    // ---- RISC Zero's emergency stop --------------------------------------------------------------------------------

    function test_afterRiscZerosStop_nothingVerifies() public {
        bytes memory seal = _seal(JOURNAL_DIGEST);
        vm.prank(guardian);
        estop.estop();
        assertTrue(standIn.paused());
        vm.expectRevert(StandInCheckpointVerifier.Stopped.selector);
        standIn.verify(seal, standInId, JOURNAL_DIGEST);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        standIn.verify(releaseSeal, RELEASE_IMAGE_ID, releaseDigest);
    }

    // ---- the constructor -------------------------------------------------------------------------------------------

    function test_constructor_refusesMainnetAndUnknownChains() public {
        uint256[3] memory refused = [uint256(1), 10, 8453];
        for (uint256 i = 0; i < refused.length; i++) {
            vm.chainId(refused[i]);
            vm.expectRevert(StandInCheckpointVerifier.NotATestnet.selector);
            new StandInCheckpointVerifier(IRiscZeroVerifier(address(estop)), signerSet, 2);
        }
        uint256[4] memory allowed = [uint256(31_337), SEPOLIA, 560_048, 17_000];
        for (uint256 i = 0; i < allowed.length; i++) {
            vm.chainId(allowed[i]);
            new StandInCheckpointVerifier(IRiscZeroVerifier(address(estop)), signerSet, 2);
        }
    }

    function test_constructor_refusesAVerifierThatIsNotAnUntrippedStop() public {
        address noCode = makeAddr("no code");
        vm.expectRevert(StandInCheckpointVerifier.RiscZeroHasNoCode.selector);
        new StandInCheckpointVerifier(IRiscZeroVerifier(noCode), signerSet, 2);
        // The bare Groth16 verifier has no stop.
        vm.expectRevert(StandInCheckpointVerifier.RiscZeroNotAStop.selector);
        new StandInCheckpointVerifier(IRiscZeroVerifier(address(groth16)), signerSet, 2);
        vm.prank(guardian);
        estop.estop();
        vm.expectRevert(StandInCheckpointVerifier.RiscZeroNotAStop.selector);
        new StandInCheckpointVerifier(IRiscZeroVerifier(address(estop)), signerSet, 2);
    }

    function test_constructor_refusesABadSignerSet() public {
        IRiscZeroVerifier rz = IRiscZeroVerifier(address(estop));
        address[] memory none = new address[](0);
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignerSet.selector);
        new StandInCheckpointVerifier(rz, none, 1);

        address[] memory unsorted = new address[](2);
        (unsorted[0], unsorted[1]) = (signerSet[1], signerSet[0]);
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignerSet.selector);
        new StandInCheckpointVerifier(rz, unsorted, 1);

        address[] memory twice = new address[](2);
        (twice[0], twice[1]) = (signerSet[0], signerSet[0]);
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignerSet.selector);
        new StandInCheckpointVerifier(rz, twice, 1);

        address[] memory zero = new address[](2);
        (zero[0], zero[1]) = (address(0), signerSet[0]);
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignerSet.selector);
        new StandInCheckpointVerifier(rz, zero, 1);

        address[] memory many = new address[](17);
        for (uint160 i = 0; i < 17; i++) {
            many[i] = address(i + 1);
        }
        vm.expectRevert(StandInCheckpointVerifier.InvalidSignerSet.selector);
        new StandInCheckpointVerifier(rz, many, 1);
    }

    function test_constructor_refusesABadThreshold() public {
        IRiscZeroVerifier rz = IRiscZeroVerifier(address(estop));
        vm.expectRevert(StandInCheckpointVerifier.InvalidThreshold.selector);
        new StandInCheckpointVerifier(rz, signerSet, 0);
        vm.expectRevert(StandInCheckpointVerifier.InvalidThreshold.selector);
        new StandInCheckpointVerifier(rz, signerSet, 4);
        new StandInCheckpointVerifier(rz, signerSet, 1);
        new StandInCheckpointVerifier(rz, signerSet, 3);
    }
}
