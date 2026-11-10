// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console2} from "forge-std/console2.sol";
import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {SelectorMismatch} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {ReturnLegFixture} from "./ReturnLegFixture.sol";

/// @notice The return leg (LEZ burn to Ethereum release) end to end with no mock on the Ethereum side: the v3 vault
///         behind RISC Zero's emergency stop and real Groth16 verifier, a real proof of the checkpoint program for
///         the candidate, a second one that echoes the beacon for the finalisation, and real proofs of the release
///         program. What is stand-in is on the LEZ side: the blocks and inscriptions the checkpoint proofs are about
///         were built by checkpoint/fidelity with test sequencer keys (e2e/README.md).
contract ReturnLegTest is ReturnLegFixture {
    /// A tampered Groth16 point makes the BN254 precompiles consume all the gas they are given, so calls with a
    /// tampered seal get a gas limit.
    uint256 internal constant TAMPERED_CALL_GAS = 3_000_000;

    function setUp() public {
        _loadFixture();
        _deploy();
        _deposit();
    }

    // ---- the fixture and the vault agree before any proof is looked at -----------------------------------------

    function test_theDeploymentIsTheOneTheProofsBind() public view {
        assertEq(block.chainid, chainId);
        assertEq(address(vault.verifier()), address(estop));
        assertEq(address(estop.verifier()), address(groth16));
        assertEq(estop.owner(), freezer, "the stop's guardian is the freeze authority (SPEC-v2 section 6)");
        assertEq(groth16.SELECTOR(), bytes4(0x73c457ba), "the selector of risc0 3.0.x Groth16 receipts");
        assertEq(vault.checkpointImageId(), checkpointImageId);
        assertEq(vault.releaseImageId(), releaseImageId);
        assertEq(vm.load(address(vault), bytes32(uint256(2))), lezConfigId, "slot 2 holds lezConfigId");
        assertEq(vault.finalState(), genesisState);
        assertEq(vault.turnSeconds(), 900, "the channel's posting timeframe, 900 one-second slots");
        // The deposit tree the wallet computed with the bridge core is the vault's.
        assertEq(vault.depositRoot(), depositRoot, "deposit root: bridge core and vault");
        assertEq(vault.nextLeafIndex(), 2);
        assertEq(vm.load(address(vault), bytes32(0)), depositRoot, "slot 0 holds the deposit root");
    }

    /// The checkpoint program's journals are, byte for byte, the vault's 17-word encoding of its own arguments.
    function test_checkpointJournals_areTheVaultsEncoding() public view {
        FerrataVault.Checkpoint memory c = _checkpoint(candidate.journal);
        assertEq(candidate.journal.length, 17 * 32);
        assertEq(
            candidate.journal, abi.encode(vault.CHECKPOINT_DOMAIN(), block.chainid, address(vault), lezConfigId, c)
        );
        assertEq(c.parentState, genesisState);
        assertEq(c.echoNonce, bytes32(0));
        assertEq(c.importedLeafCount, vault.nextLeafIndex(), "the zone imported both deposits");
        assertEq(c.burnCount, 5);
        assertEq(uint256(c.n), 4);
        assertEq(uint256(c.t), 3);

        FerrataVault.Checkpoint memory e = _checkpoint(finalization.journal);
        assertEq(
            finalization.journal, abi.encode(vault.CHECKPOINT_DOMAIN(), block.chainid, address(vault), lezConfigId, e)
        );
        assertEq(e.echoNonce, beaconNonce);
        assertEq(
            beaconNonce,
            keccak256(abi.encode(uint256(beaconPrevrandao), beaconBlockNumber, address(vault))),
            "the echoed nonce is the vault's beacon expression for the fixture's block"
        );
        e.echoNonce = bytes32(0);
        assertEq(keccak256(abi.encode(e)), keccak256(abi.encode(c)), "the two journals differ only in the echo");
    }

    /// The release program's journals are the vault's 11-word encoding, against the checkpoint's burn root.
    function test_releaseJournals_areTheVaultsEncoding() public view {
        FerrataVault.Checkpoint memory c = _checkpoint(candidate.journal);
        for (uint256 i = 0; i < releases.length; i++) {
            ReleaseCase memory r = releases[i];
            assertEq(r.burnRoot, c.burnRoot, "the release is against the checkpoint's burn root");
            assertEq(r.amount, vault.denominations(r.token)[0]);
            assertEq(
                r.journal,
                abi.encode(
                    vault.RELEASE_DOMAIN(),
                    block.chainid,
                    address(vault),
                    lezConfigId,
                    r.burnRoot,
                    r.nullifier,
                    r.token,
                    r.amount,
                    r.recipient,
                    r.relayer,
                    r.relayFee
                ),
                r.name
            );
        }
    }

    // ---- the run ---------------------------------------------------------------------------------------------

    function test_returnLeg_endToEnd() public {
        FerrataVault.Checkpoint memory c = _checkpoint(candidate.journal);

        // Candidate: the real checkpoint seal, through the stop, into the Groth16 verifier.
        uint256 submittedAt = _submit();
        (bytes32 parent, bytes32 root,,,,,,, uint256 at,,) = vault.candidates(c.newState);
        assertEq(parent, genesisState);
        assertEq(root, c.burnRoot);
        assertEq(at, submittedAt);
        assertEq(vault.finalState(), genesisState, "a candidate changes nothing yet");

        // The wait W = delta + (t - 1) * turnSeconds, then the beacon of the fixture's block.
        assertEq(_wait(), 3 hours + 1 minutes + 2 * 900);
        uint256 beaconIndex = _recordBeaconAt(submittedAt + _wait());
        (uint256 beaconTime, bytes32 nonce) = vault.beacons(beaconIndex);
        assertEq(beaconTime, submittedAt + _wait());
        assertEq(nonce, beaconNonce, "the vault's beacon is the one the finalisation proof echoes");

        // Finalisation: the second real checkpoint seal.
        _finalize(beaconIndex);
        assertEq(vault.finalState(), c.newState);
        assertEq(vault.finalImportedLeafCount(), 2);
        (bool accepted, uint256 finalizedAt) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
        assertEq(finalizedAt, block.timestamp);

        // Releases: after the token's delay, each with its real release seal, to recipients that never held
        // anything.
        vm.warp(finalizedAt + RELEASE_DELAY);
        for (uint256 i = 0; i < releases.length; i++) {
            _releaseAndCheck(releases[i]);
        }
        assertEq(vault.escrowed(ETH), 0);
        assertEq(vault.escrowed(address(token)), 0);
        assertEq(address(vault).balance, 0);
        assertEq(token.balanceOf(address(vault)), 0);
    }

    function _balance(address asset, address who) internal view returns (uint256) {
        return asset == ETH ? who.balance : token.balanceOf(who);
    }

    function _releaseAndCheck(ReleaseCase memory r) internal {
        assertEq(_balance(r.token, r.recipient), 0, "a fresh recipient");
        uint256 relayerBefore = _balance(r.token, r.relayer);
        vm.prank(submitter);
        vault.release(_request(r), r.seal);
        assertEq(_balance(r.token, r.recipient), r.amount - r.relayFee, r.name);
        if (r.relayFee != 0) assertEq(_balance(r.token, r.relayer) - relayerBefore, r.relayFee, r.name);
        assertTrue(vault.nullifierUsed(r.nullifier));
    }

    // ---- negative cases, all against the real verifier -----------------------------------------------------------

    /// One flipped byte in the candidate's seal: in the selector, and in each of the eight proof words.
    function test_negative_candidateSeal_oneByteFlipped() public {
        FerrataVault.Checkpoint memory c = _checkpoint(candidate.journal);
        bytes memory bad = _flip(candidate.seal, 0);
        vm.expectRevert(abi.encodeWithSelector(SelectorMismatch.selector, bytes4(bad), groth16.SELECTOR()));
        vault.submitCandidate(c, bad);
        for (uint256 word = 0; word < 8; word++) {
            bad = _flip(candidate.seal, 4 + 32 * word + 31);
            vm.expectRevert();
            vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
        }
        _submit(); // control: the untouched seal is accepted
    }

    /// The finalisation's seal with one byte flipped, at the moment everything else is in place.
    function test_negative_finalizeSeal_oneByteFlipped() public {
        uint256 submittedAt = _submit();
        uint256 beaconIndex = _recordBeaconAt(submittedAt + _wait());
        FerrataVault.Checkpoint memory e = _checkpoint(finalization.journal);
        for (uint256 word = 0; word < 8; word++) {
            bytes memory bad = _flip(finalization.seal, 4 + 32 * word + 31);
            vm.expectRevert();
            vault.finalize{gas: TAMPERED_CALL_GAS}(beaconIndex, e, bad);
        }
        // The candidate's seal is a valid proof of another journal (echo 0): refused as a finalisation.
        vm.expectRevert(VerificationFailed.selector);
        vault.finalize(beaconIndex, e, candidate.seal);
        _finalize(beaconIndex);
    }

    function test_negative_releaseSeal_oneByteFlipped() public {
        _acceptRoot();
        for (uint256 i = 0; i < releases.length; i++) {
            ReleaseCase memory r = releases[i];
            for (uint256 word = 0; word < 8; word++) {
                bytes memory bad = _flip(r.seal, 4 + 32 * word + 31);
                vm.expectRevert();
                vault.release{gas: TAMPERED_CALL_GAS}(_request(r), bad);
            }
            // Another valid seal (the other release's, or the checkpoint's) proves another statement.
            vm.expectRevert(VerificationFailed.selector);
            vault.release(_request(r), releases[(i + 1) % releases.length].seal);
            vm.expectRevert(VerificationFailed.selector);
            vault.release(_request(r), finalization.seal);
            assertFalse(vault.nullifierUsed(r.nullifier));
            _releaseAndCheck(r);
        }
    }

    /// The candidate's real seal with one journal field changed. Fields the vault checks itself are refused by
    /// the vault; every other field reaches the verifier, which refuses the proof.
    function test_negative_candidate_oneJournalFieldChanged() public {
        FerrataVault.Checkpoint memory c;
        for (uint256 field = 0; field < 9; field++) {
            c = _checkpoint(candidate.journal);
            if (field == 0) c.newState = keccak256("another state");
            if (field == 1) c.burnRoot = keccak256("another burn root");
            if (field == 2) c.burnCount += 1;
            if (field == 3) c.importedLeafCount += 1;
            if (field == 4) c.sunset = true;
            if (field == 5) c.tipId += 1;
            if (field == 6) c.tipHash = keccak256("another tip");
            if (field == 7) c.n += 1;
            if (field == 8) c.t += 1;
            vm.expectRevert(VerificationFailed.selector);
            vault.submitCandidate(c, candidate.seal);
        }
        c = _checkpoint(candidate.journal);
        c.parentState = keccak256("another parent");
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(c, candidate.seal);
        c = _checkpoint(candidate.journal);
        c.echoNonce = beaconNonce;
        vm.expectRevert(FerrataVault.EchoNonceNotZero.selector);
        vault.submitCandidate(c, candidate.seal);
        c = _checkpoint(candidate.journal);
        c.nMin += 1;
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(c, candidate.seal);
        // On another chain the same seal proves nothing.
        vm.chainId(1);
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate(_checkpoint(candidate.journal), candidate.seal);
        vm.chainId(chainId);
        _submit();
    }

    /// The finalisation's real seal with one journal field changed. `finalize` compares five fields with the
    /// candidate; the tip, n and t are not compared (README), so only the proof stands behind them.
    function test_negative_finalize_oneJournalFieldChanged() public {
        uint256 submittedAt = _submit();
        uint256 beaconIndex = _recordBeaconAt(submittedAt + _wait());
        FerrataVault.Checkpoint memory e;
        for (uint256 field = 0; field < 4; field++) {
            e = _checkpoint(finalization.journal);
            if (field == 0) e.tipId += 1;
            if (field == 1) e.tipHash = keccak256("another tip");
            if (field == 2) e.n += 1;
            if (field == 3) e.t += 1;
            vm.expectRevert(VerificationFailed.selector);
            vault.finalize(beaconIndex, e, finalization.seal);
        }
        e = _checkpoint(finalization.journal);
        e.burnRoot = keccak256("another burn root");
        vm.expectRevert(FerrataVault.CandidateMismatch.selector);
        vault.finalize(beaconIndex, e, finalization.seal);
        e = _checkpoint(finalization.journal);
        e.echoNonce = keccak256("another nonce");
        vm.expectRevert(FerrataVault.EchoNonceMismatch.selector);
        vault.finalize(beaconIndex, e, finalization.seal);
        _finalize(beaconIndex);
    }

    /// The release's real seal with one request field changed. A change of denomination alone reaches the verifier
    /// only where the fee stays valid for it (the ETH release, fee 0); with a registered fee the vault itself
    /// refuses first, because fees are per denomination.
    function test_negative_release_oneJournalFieldChanged() public {
        _acceptRoot();
        // Escrow for the larger denomination, so that only the proof can object to it.
        vm.deal(depositor, 1 ether);
        vm.prank(depositor);
        vault.depositETH{value: 0.01 ether}(1, keccak256("someone's commitment"), 2);
        for (uint256 i = 0; i < releases.length; i++) {
            ReleaseCase memory r = releases[i];
            FerrataVault.ReleaseRequest memory q;
            for (uint256 field = 0; field < 5; field++) {
                q = _request(r);
                if (field == 0) q.recipient = makeAddr("thief");
                if (field == 1) q.relayer = makeAddr("another relayer");
                if (field == 2) q.relayFee = r.relayFee == 0 ? vault.relayFees(r.token)[0] : 0;
                if (field == 3) q.nullifier = keccak256("another nullifier");
                if (field == 4) q.denomIndex = 1;
                bool feeStillValid = field != 4 || r.relayFee == 0;
                vm.expectRevert(feeStillValid ? VerificationFailed.selector : FerrataVault.InvalidRelayFee.selector);
                vault.release(q, r.seal);
            }
            assertFalse(vault.nullifierUsed(r.nullifier));
        }
    }

    function test_negative_releaseReplayed() public {
        _acceptRoot();
        for (uint256 i = 0; i < releases.length; i++) {
            _releaseAndCheck(releases[i]);
            vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
            vault.release(_request(releases[i]), releases[i].seal);
        }
    }

    /// A valid release proof against a root that is not finalised: before any checkpoint, while the root is only
    /// a candidate, and after finalisation until the release delay has passed.
    function test_negative_releaseAgainstARootNotFinalised() public {
        ReleaseCase memory r = releases[0];
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(_request(r), r.seal);
        uint256 submittedAt = _submit();
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(_request(r), r.seal);
        uint256 beaconIndex = _recordBeaconAt(submittedAt + _wait());
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(_request(r), r.seal);
        _finalize(beaconIndex);
        vm.warp(block.timestamp + RELEASE_DELAY - 1);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(_request(r), r.seal);
        vm.warp(block.timestamp + 1);
        _releaseAndCheck(r);
    }

    /// Finalising early with the real finalisation seal: with the beacon recorded at submission, and with the
    /// fixture's own beacon block (so the echoed nonce is right) placed one second before the wait is over.
    function test_negative_finalizingEarly() public {
        uint256 submittedAt = _submit();
        FerrataVault.Checkpoint memory e = _checkpoint(finalization.journal);
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(0, e, finalization.seal);

        uint256 snapshot = vm.snapshotState();
        uint256 early = _recordBeaconAt(submittedAt + _wait() - 1);
        (, bytes32 nonce) = vault.beacons(early);
        assertEq(nonce, e.echoNonce, "only the time is wrong");
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, e, finalization.seal);
        vm.revertToState(snapshot);

        _finalize(_recordBeaconAt(submittedAt + _wait()));
    }

    /// After the guardian stops the verifier, a real proof no longer releases.
    function test_negative_afterTheStop_aRealProofIsRefused() public {
        _acceptRoot();
        vm.prank(freezer);
        estop.estop();
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(_request(releases[0]), releases[0].seal);
    }
}

/// @notice Whole-transaction gas of every step with the real verifier, for GAS.md. Run with
///             forge test --isolate --match-contract ReturnLegGas -vv
///         so that each top-level call or creation is its own transaction (21000, calldata, cold access).
contract ReturnLegGas is ReturnLegFixture {
    function setUp() public {
        _loadFixture();
    }

    function _log(string memory step) internal view {
        console2.log(step, vm.lastCallGas().gasTotalUsed);
    }

    function test_gas_everyStepWithTheRealVerifier() public {
        _deployVerifier();
        _log("deploy RiscZeroGroth16Verifier");
        _deployStop();
        _log("deploy RiscZeroVerifierEmergencyStop");
        _deployToken();
        _log("deploy TestToken");
        _deployVault();
        _log("deploy FerrataVault");
        _registerEth();
        _log("registerToken ETH (3 denominations)");
        _registerToken();
        _log("registerToken ERC-20 (3 denominations)");
        _assertAddresses();
        _depositEth();
        _log("depositETH, first deposit of the tree and of the address");
        _fundToken();
        _depositToken();
        _log("deposit ERC-20, second deposit of the address");

        uint256 submittedAt = _submit();
        _log("submitCandidate, real checkpoint seal (records a beacon)");
        uint256 beaconIndex = _recordBeaconAt(submittedAt + _wait());
        _log("recordBeacon");
        _finalize(beaconIndex);
        _log("finalize, real checkpoint seal echoing the beacon");
        vm.warp(block.timestamp + RELEASE_DELAY);
        for (uint256 i = 0; i < releases.length; i++) {
            ReleaseCase memory r = releases[i];
            vm.prank(submitter);
            vault.release(_request(r), r.seal);
            _log(string.concat("release ", r.name, ", real release seal"));
        }
    }
}
