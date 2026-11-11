// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Every field of the release journal is bound: a proof whose journal differs from the vault's
///         reconstruction in exactly one field is rejected. Each test also runs a control in which the same
///         request succeeds once its own journal is proven, so the rejection can only come from the mismatch.
contract ReleaseJournalBindingTest is VaultTestBase {
    bytes32 internal constant ROOT_A = keccak256("burn root A");
    bytes32 internal constant ROOT_B = keccak256("burn root B");
    bytes32 internal constant NULLIFIER = keccak256("nullifier");
    uint256 internal constant FEE = 0.002 ether; // the relay fee registered for 1 ETH (0.2%)
    address internal boundRelayer = makeAddr("boundRelayer");

    function setUp() public override {
        super.setUp();
        _deposit(alice, ETH, 1, keccak256("c1"));
        _deposit(alice, ETH, 1, keccak256("c2"));
        _acceptRoot(ROOT_A);
        _acceptRoot(ROOT_B);
    }

    /// @dev 1 ETH to `recipient`, standard fee bound to `boundRelayer`, under ROOT_A.
    function _honestRequest() internal view returns (FerrataVault.ReleaseRequest memory) {
        return FerrataVault.ReleaseRequest({
            token: ETH,
            denomIndex: 1,
            recipient: recipient,
            relayer: boundRelayer,
            relayFee: FEE,
            burnRoot: ROOT_A,
            nullifier: NULLIFIER
        });
    }

    function _honestJournal() internal view returns (ReleaseJournal memory) {
        return _releaseJournal(_honestRequest());
    }

    function _assertMismatchRejected(ReleaseJournal memory proven) internal {
        FerrataVault.ReleaseRequest memory r = _honestRequest();
        _proveRelease(proven);
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(r, SEAL);
        assertFalse(vault.nullifierUsed(NULLIFIER));

        _proveRelease(_releaseJournal(r)); // control
        vm.prank(relayer);
        vault.release(r, SEAL);
        assertTrue(vault.nullifierUsed(NULLIFIER));
    }

    // ---- one test per journal field -----------------------------------------------------------------------------

    function test_releaseJournal_bindsChainId() public {
        ReleaseJournal memory j = _honestJournal();
        j.chainId = block.chainid + 1;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsVault() public {
        ReleaseJournal memory j = _honestJournal();
        j.vault = makeAddr("another vault");
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsLezConfigId() public {
        ReleaseJournal memory j = _honestJournal();
        j.lezConfigId = keccak256("another LEZ configuration");
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsBurnRoot() public {
        ReleaseJournal memory j = _honestJournal();
        j.burnRoot = ROOT_B;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsNullifier() public {
        ReleaseJournal memory j = _honestJournal();
        j.nullifier = keccak256("another nullifier");
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsToken() public {
        ReleaseJournal memory j = _honestJournal();
        j.token = address(dai);
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsAmount() public {
        ReleaseJournal memory j = _honestJournal();
        j.amount = 0.1 ether;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsRecipient() public {
        ReleaseJournal memory j = _honestJournal();
        j.recipient = bob;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsRelayer() public {
        ReleaseJournal memory j = _honestJournal();
        j.relayer = address(0);
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsRelayFee() public {
        ReleaseJournal memory j = _honestJournal();
        j.relayFee = 0;
        _assertMismatchRejected(j);
    }

    // ---- the same property as concrete attacks (Security #2, #3) --------------------------------------------------

    function test_attack_relayerCannotRedirectFunds() public {
        _proveRelease(_honestJournal()); // the user's proof
        FerrataVault.ReleaseRequest memory forged = _honestRequest();
        forged.recipient = relayer;
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(forged, SEAL);
    }

    function test_attack_relayerCannotAddAFee() public {
        FerrataVault.ReleaseRequest memory honest = _honestRequest();
        honest.relayFee = 0;
        honest.relayer = address(0);
        _proveRelease(_releaseJournal(honest)); // the user chose to pay no fee
        FerrataVault.ReleaseRequest memory forged = _honestRequest();
        forged.relayer = address(0);
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(forged, SEAL);
    }

    function test_attack_relayerCannotRedirectTheFee() public {
        _proveRelease(_honestJournal()); // fee bound to boundRelayer
        FerrataVault.ReleaseRequest memory forged = _honestRequest();
        forged.relayer = relayer;
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(forged, SEAL);
    }

    function test_attack_proofCannotBeReplayedOnAnotherVault() public {
        FerrataVault other = _deployVault(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        _registerDefaultTokens(other);
        _depositInto(other, alice, ETH, 1, keccak256("c3"));
        _acceptRootOn(other, LEZ_CONFIG_ID, ROOT_A);

        _proveRelease(_honestJournal()); // valid for `vault` only
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        other.release(_honestRequest(), SEAL);

        vm.prank(relayer);
        vault.release(_honestRequest(), SEAL);
    }

    function test_attack_proofCannotBeReplayedOnAnotherChain() public {
        _proveRelease(_honestJournal());
        uint256 home = block.chainid;
        vm.chainId(home + 1); // e.g. a fork that kept the state but changed the chain id
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(_honestRequest(), SEAL);

        vm.chainId(home);
        vm.prank(relayer);
        vault.release(_honestRequest(), SEAL);
    }

    function test_attack_proofCannotBeReplayedAcrossLezConfigurations() public {
        FerrataVault other = _deployVault(IRiscZeroVerifier(address(estop)), keccak256("another LEZ configuration"));
        _registerDefaultTokens(other);
        _depositInto(other, alice, ETH, 1, keccak256("c3"));
        _acceptRootOn(other, keccak256("another LEZ configuration"), ROOT_A);

        _proveRelease(_honestJournal());
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        other.release(_honestRequest(), SEAL);
    }
}

/// @notice Every field of the checkpoint journal (SPEC-v3 D3: 17 words) is bound, both when a candidate is submitted and
///         when it is finalised: a proof whose journal differs from the vault's reconstruction in exactly one field
///         is rejected, and the same call then succeeds once its own journal is proven (control).
contract CheckpointJournalBindingTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");

    function setUp() public override {
        super.setUp();
        _fillTree(3); // v4 (D11): the 3 imported leaves of the test checkpoints exist
    }

    /// @dev Changes exactly one field of `j`, numbered in journal order after the domain word (16 fields).
    function _mutate(CheckpointJournal memory j, uint256 field) internal view {
        if (field == 0) j.chainId = block.chainid + 1;
        else if (field == 1) j.vault = address(0xbeef);
        else if (field == 2) j.lezConfigId = keccak256("another LEZ configuration");
        else if (field == 3) j.nMin = N_MIN + 1;
        else if (field == 4) j.tMin = T_MIN + 1;
        else if (field == 5) j.parentState = keccak256("another parent state");
        else if (field == 6) j.newState = keccak256("another new state");
        else if (field == 7) j.burnRoot = keccak256("another burn root");
        else if (field == 8) j.burnCount = j.burnCount + 1;
        else if (field == 9) j.importedLeafCount = j.importedLeafCount + 1;
        else if (field == 10) j.sunset = !j.sunset;
        else if (field == 11) j.echoNonce = keccak256("another echo nonce");
        else if (field == 12) j.tipId = j.tipId + 1;
        else if (field == 13) j.tipHash = keccak256("another tip hash");
        else if (field == 14) j.n = j.n + 1;
        else j.t = j.t + 1;
    }

    function _checkSubmit(uint256 field) internal {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 3, false);
        CheckpointJournal memory proven = _checkpointJournal(c);
        _mutate(proven, field);
        _proveCheckpoint(proven);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.submitCandidate(c, SEAL);

        _proveCheckpoint(_checkpointJournal(c)); // control
        vault.submitCandidate(c, SEAL);
        (,,,, uint48 submittedAt,,,) = vault.candidates(c.newState);
        assertEq(submittedAt, block.timestamp);
    }

    function _checkFinalize(uint256 field) internal {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 3, false);
        _submit(c);
        vm.warp(block.timestamp + DELTA + (uint256(T) - 1) * TURN_SECONDS);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        c.echoNonce = nonce;

        CheckpointJournal memory proven = _checkpointJournal(c);
        _mutate(proven, field);
        _proveCheckpoint(proven);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.finalize(beaconIndex, c, SEAL);

        _proveCheckpoint(_checkpointJournal(c)); // control
        vault.finalize(beaconIndex, c, SEAL);
        assertEq(vault.finalState(), c.newState);
    }

    function test_checkpointJournal_submit_bindsChainId() public {
        _checkSubmit(0);
    }

    function test_checkpointJournal_submit_bindsVault() public {
        _checkSubmit(1);
    }

    function test_checkpointJournal_submit_bindsLezConfigId() public {
        _checkSubmit(2);
    }

    function test_checkpointJournal_submit_bindsNMin() public {
        _checkSubmit(3);
    }

    function test_checkpointJournal_submit_bindsTMin() public {
        _checkSubmit(4);
    }

    function test_checkpointJournal_submit_bindsParentState() public {
        _checkSubmit(5);
    }

    function test_checkpointJournal_submit_bindsNewState() public {
        _checkSubmit(6);
    }

    function test_checkpointJournal_submit_bindsBurnRoot() public {
        _checkSubmit(7);
    }

    function test_checkpointJournal_submit_bindsBurnCount() public {
        _checkSubmit(8);
    }

    function test_checkpointJournal_submit_bindsImportedLeafCount() public {
        _checkSubmit(9);
    }

    function test_checkpointJournal_submit_bindsSunset() public {
        _checkSubmit(10);
    }

    function test_checkpointJournal_submit_bindsEchoNonce() public {
        _checkSubmit(11);
    }

    function test_checkpointJournal_submit_bindsTipId() public {
        _checkSubmit(12);
    }

    function test_checkpointJournal_submit_bindsTipHash() public {
        _checkSubmit(13);
    }

    function test_checkpointJournal_submit_bindsN() public {
        _checkSubmit(14);
    }

    function test_checkpointJournal_submit_bindsT() public {
        _checkSubmit(15);
    }

    function test_checkpointJournal_finalize_bindsChainId() public {
        _checkFinalize(0);
    }

    function test_checkpointJournal_finalize_bindsVault() public {
        _checkFinalize(1);
    }

    function test_checkpointJournal_finalize_bindsLezConfigId() public {
        _checkFinalize(2);
    }

    function test_checkpointJournal_finalize_bindsNMin() public {
        _checkFinalize(3);
    }

    function test_checkpointJournal_finalize_bindsTMin() public {
        _checkFinalize(4);
    }

    function test_checkpointJournal_finalize_bindsParentState() public {
        _checkFinalize(5);
    }

    function test_checkpointJournal_finalize_bindsNewState() public {
        _checkFinalize(6);
    }

    function test_checkpointJournal_finalize_bindsBurnRoot() public {
        _checkFinalize(7);
    }

    function test_checkpointJournal_finalize_bindsBurnCount() public {
        _checkFinalize(8);
    }

    function test_checkpointJournal_finalize_bindsImportedLeafCount() public {
        _checkFinalize(9);
    }

    function test_checkpointJournal_finalize_bindsSunset() public {
        _checkFinalize(10);
    }

    function test_checkpointJournal_finalize_bindsEchoNonce() public {
        _checkFinalize(11);
    }

    function test_checkpointJournal_finalize_bindsTipId() public {
        _checkFinalize(12);
    }

    function test_checkpointJournal_finalize_bindsTipHash() public {
        _checkFinalize(13);
    }

    function test_checkpointJournal_finalize_bindsN() public {
        _checkFinalize(14);
    }

    function test_checkpointJournal_finalize_bindsT() public {
        _checkFinalize(15);
    }
}
