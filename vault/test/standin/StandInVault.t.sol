// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {StandInCheckpointVerifier} from "../../src/standin/StandInCheckpointVerifier.sol";
import {StandInBase} from "./StandInBase.sol";

/// @notice The testnet arrangement end to end with the unchanged vault (src/standin/README.md): a rule (B) vault whose
///         verifier is the stand-in, at the address, chain id and LEZ configuration that the REAL release proofs of the
///         live run crown-live-3 bind (e2e/runs/crown-live-3/crown.json: the vault 0x04eE…e44c on Sepolia). The burn
///         root those two proofs are about is accepted here on the stand-in keys' signatures instead of crown-live-3's
///         checkpoint proof; both releases are then paid with their real Groth16 seals through RISC Zero's verifier
///         (3.1.0: the two releases proven again under RISC Zero 3.1 for the same journals,
///         test/real/fixtures/crown-live-3-releases.json; the run's own seals are of risc0 3.0.5).
///         What is real: the vault, the deposits, the release proofs. What is a stand-in: the burn root's attestation.
contract StandInVaultTest is StandInBase {
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    uint256 internal constant DELTA = 120; // the testnet arrangement's wait
    uint256 internal constant RELEASE_DELAY = 120; // crown-live-3's
    uint256 internal constant TAMPERED_CALL_GAS = 3_000_000;
    // crown-live-3 (e2e/runs/crown-live-3/identity.json): the deployer, the nonce of the vault's creation, its id.
    address internal constant DEPLOYER = 0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07;
    uint64 internal constant VAULT_NONCE = 25;
    address internal constant CROWN_VAULT = 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c;
    bytes32 internal constant CROWN_CONFIG_ID = 0x10837afeaf6402e9d4bc2c4a13039e70f88762f13336d9c222d2f6d3b6a95553;
    bytes32 internal constant GENESIS = keccak256("test: the stand-in vault's genesis state");

    FerrataVault internal vault;
    string internal json;
    bytes32 internal burnRoot;

    function setUp() public {
        _setUpStandIn();
        json = vm.readFile("../e2e/runs/crown-live-3/crown.json");
        burnRoot = vm.parseJsonBytes32(json, ".releases[0].burnRoot");

        vm.deal(DEPLOYER, 1 ether);
        vm.setNonce(DEPLOYER, VAULT_NONCE);
        FerrataVault.Deployment memory d = _deployment();
        vm.prank(DEPLOYER);
        vault = new FerrataVault(d);
        assertEq(address(vault), CROWN_VAULT, "the address crown-live-3's release proofs bind");

        // ETH as crown-live-3 registered it (script/DeployVault.s.sol _ethParams), and its two deposits.
        vm.prank(DEPLOYER);
        vault.registerToken(ETH, _ethParams());
        for (uint64 j = 0; j < 2; j++) {
            bytes32 commitment =
                vm.parseJsonBytes32(json, string.concat(".deposits[", vm.toString(uint256(j)), "].commitment"));
            vm.prank(DEPLOYER);
            vault.depositETH{value: 0.001 ether + 2e12}(0, commitment, j);
        }
    }

    function _deployment() internal returns (FerrataVault.Deployment memory d) {
        d.verifier = IRiscZeroVerifier(address(standIn));
        d.checkpointImageId = standInId;
        d.releaseImageId = RELEASE_IMAGE_ID;
        d.lezConfigId = CROWN_CONFIG_ID;
        d.genesisState = GENESIS;
        d.nMin = 3;
        d.tMin = 2;
        d.deltaFloor = DELTA;
        d.initialDelta = DELTA;
        d.revokeWindow = 1 days;
        d.maxFreeze = 30 days;
        d.turnSeconds = 0;
        d.minReleaseDelay = 0;
        d.admin = DEPLOYER;
        d.freezeAuthority = makeAddr("freeze authority");
        d.globalDepositCap = FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
        d.globalReleaseCap = FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
        d.mode = FerrataVault.Mode.B;
        d.releaseCapFloor = FerrataVault.CapParams({capacity: 0, refillPerSecond: 0});
        d.feeRecipient = DEPLOYER;
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory out) {
        out = new uint256[](3);
        (out[0], out[1], out[2]) = (a, b, c);
    }

    function _ethParams() internal pure returns (FerrataVault.TokenParams memory) {
        return FerrataVault.TokenParams({
            denominations: _three(0.001 ether, 0.01 ether, 0.1 ether),
            relayFees: _three(2e12, 2e13, 2e14),
            lezUnit: 1e12,
            unitWeight: 3000,
            depositCap: FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0}),
            releaseCap: FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0}),
            releaseDelay: RELEASE_DELAY,
            lezDefinition: bytes32(0),
            decimals: 18,
            checkpointFees: _three(1e12, 1e13, 1e14)
        });
    }

    /// @dev A checkpoint that carries crown-live-3's burn root (two burns), built on the vault's genesis.
    function _checkpoint(bytes32 root) internal pure returns (FerrataVault.Checkpoint memory c) {
        c.nMin = 3;
        c.tMin = 2;
        c.parentState = GENESIS;
        c.newState = keccak256(abi.encode("test: the state after two burns", root));
        c.burnRoot = root;
        c.burnCount = 2;
        c.importedLeafCount = 2;
        c.tipId = 83_001;
        c.tipHash = keccak256("test: a LEZ tip hash");
        c.n = 3;
        c.t = 2;
    }

    /// @dev The vault's journal digest, from the written layout (17 words).
    function _journalDigest(FerrataVault.Checkpoint memory c) internal view returns (bytes32) {
        return sha256(
            abi.encode(sha256(bytes("ferrata/vault/checkpoint/v3")), block.chainid, address(vault), CROWN_CONFIG_ID, c)
        );
    }

    function _release(uint256 i) internal view returns (FerrataVault.ReleaseRequest memory r, bytes memory seal) {
        string memory at = string.concat(".releases[", vm.toString(i), "]");
        r = FerrataVault.ReleaseRequest({
            token: vm.parseJsonAddress(json, string.concat(at, ".token")),
            denomIndex: uint8(vm.parseJsonUint(json, string.concat(at, ".denomIndex"))),
            recipient: vm.parseJsonAddress(json, string.concat(at, ".recipient")),
            relayer: vm.parseJsonAddress(json, string.concat(at, ".relayer")),
            relayFee: vm.parseJsonUint(json, string.concat(at, ".relayFee")),
            burnRoot: vm.parseJsonBytes32(json, string.concat(at, ".burnRoot")),
            nullifier: vm.parseJsonBytes32(json, string.concat(at, ".nullifier"))
        });
        // The run's release proven again under RISC Zero 3.1, for the same journal (its own seal is of risc0 3.0.5).
        seal = vm.parseJsonBytes(vm.readFile("test/real/fixtures/crown-live-3-releases.json"), string.concat(at, ".seal"));
    }

    function _finalise() internal returns (FerrataVault.Checkpoint memory c) {
        c = _checkpoint(burnRoot);
        vault.submitCandidate(c, _seal(_journalDigest(c)));
        vm.warp(block.timestamp + DELTA);
        vault.finalizeAttested(c);
    }

    // ---- the deployment says what it is ----------------------------------------------------------------------------

    function test_theVaultNamesTheStandIn_notACheckpointProgram() public view {
        assertEq(address(vault.verifier()), address(standIn));
        assertEq(vault.checkpointImageId(), standInId);
        assertTrue(vault.checkpointImageId() != CHECKPOINT_B_IMAGE_ID);
        assertEq(vault.releaseImageId(), RELEASE_IMAGE_ID, "the unchanged release program");
        assertEq(uint256(vault.mode()), uint256(FerrataVault.Mode.B));
        assertEq(uint256(vault.nMin()), standIn.signers().length);
        assertEq(uint256(vault.tMin()), standIn.threshold());
        assertEq(vault.nextLeafIndex(), 2);
    }

    // ---- the round: a signed root, the wait, two real release proofs ----------------------------------------------

    function test_aSignedRootIsFinalAfterTheWait_andBothRealReleaseProofsArePaid() public {
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        bytes32 digest = _journalDigest(c);
        bytes memory candidateSeal = _seal(digest);
        address anyone = makeAddr("anyone");
        vm.prank(anyone);
        vault.submitCandidate(c, candidateSeal);
        (bytes32 stored,,,, uint48 submittedAt,,,) = vault.candidates(c.newState);
        assertEq(stored, digest, "the vault's own digest of the journal is what the keys signed");
        (bool accepted,) = vault.burnRootStatus(burnRoot);
        assertFalse(accepted, "a candidate accepts nothing yet");

        vm.warp(uint256(submittedAt) + DELTA - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(uint256(submittedAt) + DELTA);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        uint256 finalizedAt;
        (accepted, finalizedAt) = vault.burnRootStatus(burnRoot);
        assertTrue(accepted);

        (FerrataVault.ReleaseRequest memory r0, bytes memory seal0) = _release(0);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r0, seal0);
        vm.warp(finalizedAt + RELEASE_DELAY);
        for (uint256 i = 0; i < 2; i++) {
            (FerrataVault.ReleaseRequest memory r, bytes memory seal) = _release(i);
            assertEq(r.burnRoot, burnRoot);
            uint256 before = r.recipient.balance;
            vault.release(r, seal);
            // The denomination less the checkpoint fee (0.1 %), which is credited to the candidate's sender.
            assertEq(r.recipient.balance - before, 0.001 ether - 1e12);
            assertTrue(vault.nullifierUsed(r.nullifier));
            // A replay of the same release is refused.
            vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
            vault.release(r, seal);
        }
        assertEq(vault.owed(anyone, ETH), 2e12, "the checkpoint fees of both releases");
    }

    // ---- refusals -------------------------------------------------------------------------------------------------

    function test_aRootSignedBelowThresholdIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        bytes32 d = _digest(block.chainid, address(standIn), standInId, _journalDigest(c));
        bytes memory one = _sig(keys[0], d);
        vm.expectRevert(StandInCheckpointVerifier.BelowThreshold.selector);
        vault.submitCandidate(c, one);
        bytes memory twice = bytes.concat(one, one);
        vm.expectRevert(StandInCheckpointVerifier.SignersNotAscending.selector);
        vault.submitCandidate(c, twice);
        bytes memory withOutsider = bytes.concat(one, _sig(outsiderKey, d));
        vm.expectRevert();
        vault.submitCandidate(c, withOutsider);
        (bool accepted,) = vault.burnRootStatus(burnRoot);
        assertFalse(accepted);
    }

    /// @notice A release under a root no threshold signed: the real proof is valid, the root is not accepted.
    function test_aReleaseUnderAnUnsignedRootIsRefused() public {
        (FerrataVault.ReleaseRequest memory r, bytes memory seal) = _release(0);
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(r, seal);
        // Another root is final; the proof's root still is not.
        FerrataVault.Checkpoint memory c = _checkpoint(keccak256("test: another burn root"));
        vault.submitCandidate(c, _seal(_journalDigest(c)));
        vm.warp(block.timestamp + DELTA);
        vault.finalizeAttested(c);
        vm.warp(block.timestamp + RELEASE_DELAY);
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(r, seal);
        // And the real proof does not carry over to the signed root.
        r.burnRoot = c.burnRoot;
        vm.expectRevert(VerificationFailed.selector);
        vault.release(r, seal);
    }

    /// @notice The signed journal is the vault's: the same fields signed for another vault, chain or configuration
    ///         (each is a word of the journal digest) are refused here.
    function test_aRootSignedForAnotherVaultChainOrConfigurationIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        bytes32 domain = sha256(bytes("ferrata/vault/checkpoint/v3"));
        bytes32[3] memory others = [
            sha256(abi.encode(domain, block.chainid, makeAddr("another vault"), CROWN_CONFIG_ID, c)),
            sha256(abi.encode(domain, uint256(17_000), address(vault), CROWN_CONFIG_ID, c)),
            sha256(abi.encode(domain, block.chainid, address(vault), keccak256("another configuration"), c))
        ];
        for (uint256 i = 0; i < others.length; i++) {
            bytes memory seal = _seal(others[i]);
            vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
            vault.submitCandidate(c, seal);
        }
    }

    /// @notice What the keys signed is one journal: a candidate with any other field is refused.
    function test_aSignedJournalWithAChangedFieldIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        bytes memory seal = _seal(_journalDigest(c));
        c.burnRoot = keccak256("test: a root nobody signed");
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        vault.submitCandidate(c, seal);
        c = _checkpoint(burnRoot);
        c.burnCount = 3;
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        vault.submitCandidate(c, seal);
        c = _checkpoint(burnRoot);
        c.tipId += 1;
        vm.expectRevert(StandInCheckpointVerifier.NotASigner.selector);
        vault.submitCandidate(c, seal);
    }

    /// @notice crown-live-3's real checkpoint_b proof is not accepted by this vault: its checkpoint id is the stand-in's.
    function test_aRealCheckpointProofIsNotWhatThisVaultTakes() public {
        (,,,, FerrataVault.Checkpoint memory real) = abi.decode(
            vm.parseJsonBytes(json, ".candidate.journal"), (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint)
        );
        bytes memory seal = vm.parseJsonBytes(json, ".candidate.seal");
        // Its floors (1 of 1) are not this vault's; with this vault's floors the seal is no attestation.
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(real, seal);
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        vm.expectRevert(StandInCheckpointVerifier.SealLength.selector);
        vault.submitCandidate(c, seal);
    }

    /// @notice The keys cannot pay themselves without a release proof: signatures sent as a release seal reach RISC
    ///         Zero's verifier and fail there. (They can sign a false root, which is what the stand-in does not prevent.)
    function test_signaturesAreNotAcceptedAsAReleaseProof() public {
        _finalise();
        vm.warp(block.timestamp + RELEASE_DELAY);
        (FerrataVault.ReleaseRequest memory r,) = _release(0);
        bytes32 releaseDigest = sha256(
            abi.encode(
                sha256(bytes("ferrata/vault/release/v2")),
                block.chainid,
                address(vault),
                CROWN_CONFIG_ID,
                r.burnRoot,
                r.nullifier,
                r.token,
                uint256(0.001 ether),
                r.recipient,
                r.relayer,
                r.relayFee
            )
        );
        bytes memory seal = _seal(releaseDigest);
        vm.expectRevert();
        vault.release{gas: TAMPERED_CALL_GAS}(r, seal);
        assertFalse(vault.nullifierUsed(r.nullifier));
    }

    function test_afterRiscZerosStop_aSignedCandidateIsNotTaken() public {
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        bytes memory seal = _seal(_journalDigest(c));
        vm.prank(guardian);
        estop.estop();
        vm.expectRevert(StandInCheckpointVerifier.Stopped.selector);
        vault.submitCandidate(c, seal);
    }

    function test_afterRiscZerosStop_aSubmittedCandidateIsNotFinalised() public {
        FerrataVault.Checkpoint memory c = _checkpoint(burnRoot);
        bytes memory seal = _seal(_journalDigest(c));
        vault.submitCandidate(c, seal);
        vm.warp(block.timestamp + DELTA);
        vm.prank(guardian);
        estop.estop();
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.finalizeAttested(c);
    }
}
