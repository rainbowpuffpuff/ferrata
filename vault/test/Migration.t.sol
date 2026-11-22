// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console2} from "forge-std/console2.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull} from "risc0/IRiscZeroVerifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {MockRiscZeroVerifierB} from "./mocks/MockVerifierB.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @title The migration rehearsal (RFP-022 Security #8, Security #9, Soft Requirement 4)
/// @notice One configuration change on the vault, in order, as `vault/MIGRATION.md` tells it:
///         v1 with proof system A (step 1), the in-flight state when a bug is announced (step 2), the stop
///         (step 3), in-flight burns honoured (step 4), in-flight deposits honoured (step 5), vault v2 with proof
///         system B (step 6), users moving (step 7). The Security #9 branch - a bug in the verifier itself, where
///         the emergency stop replaces the freeze and everything that still can move escrow - is
///         `test/MigrationFailsafe.t.sol`.
///
///         RFP-022 Security #8 (its last sentences): "... bridge accepts as a valid proof after deployment.
///         Change requires deploying a new version and migrating to it. Proposals must document the migration
///         mechanism and how in-flight deposits and burns are honoured across it."
///         RFP-022 Security #9: a failsafe for funds at risk once a verification bug is found, which must not be a
///         backdoor; the design's answer is documented in `vault/MIGRATION.md` and rehearsed in MigrationFailsafe.
///         RFP-022 Soft Requirement 4: "Design the proof-system components as pluggable, so that future zkVM
///         improvements, proof compression, or hardware acceleration can be adopted without restructuring the vault
///         or the bridge program (redeployment and migration would be necessary, per Security #8)."
///
///         What the rehearsal is not: the LEZ side of the migration (`Sunset`, the successor configuration's `Init`
///         and import of v2's tree, the claim against it) runs in the LEZ harness (`lez/`), not here; this file
///         rehearses the vault side only. A burn on LEZ is simulated as the existing tests simulate it: a burn root
///         a checkpoint proves, and a nullifier - the vault never sees the LEZ transaction itself.
contract MigrationTest is VaultTestBase {
    // ---- proof system B and v2's configuration: everything the vault pins is different, the vault code is not ----

    bytes32 internal constant CHECKPOINT_IMAGE_ID_B = keccak256("test: checkpoint program image id, proof system B");
    bytes32 internal constant RELEASE_IMAGE_ID_B = keccak256("test: release program image id, proof system B");
    /// @notice v2 pairs with a successor LEZ configuration (deployer-guide.md §6): a new `lezConfigId`.
    bytes32 internal constant LEZ_CONFIG_ID_B = keccak256("test: the successor configuration's LEZ id (v2)");
    /// @notice A system-B seal: it starts with MockRiscZeroVerifierB's four-byte format selector.
    bytes internal constant SEAL_B = hex"B2D0B2D0DEADBEEF";

    // ---- the rehearsal's roots and nullifiers ----

    bytes32 internal constant R0 = keccak256("v1 burn root 0 (contains b2, released before the announcement)");
    bytes32 internal constant R1 = keccak256("v1 burn root 1 (contains b1, finalised, not yet released)");
    bytes32 internal constant RB = keccak256("v2 burn root, proven under proof system B");
    bytes32 internal constant RS = keccak256("v1 burn root at the sunset (no burns left under it)");
    bytes32 internal constant R3 = keccak256("v1 burn root 3 (the holder of d1's balance burns on LEZ)");

    bytes32 internal constant N_B1 = keccak256("burn b1's nullifier");
    bytes32 internal constant N_B2 = keccak256("burn b2's nullifier");
    bytes32 internal constant N_A3 = keccak256("the holder's burn, after the migration window");

    address internal carol = makeAddr("carol"); // a settled depositor of v1: imported and released long since
    address internal dave = makeAddr("dave"); // imported; dave's burn is b1, in flight at the announcement
    address internal burnRecipient = makeAddr("burnRecipient"); // the recipient b1's burn names
    address internal eve = makeAddr("eve"); // named by no proof and no leaf: receives nothing

    /// @notice The release delay the admin gives every root finalised as checkpoint 1 or later of v1.
    uint256 internal constant DELAY = 1 hours;

    uint64 internal d1Leaf; // alice's deposit, imported by LEZ
    uint64 internal d2Leaf; // bob's deposit, never imported
    /// @notice When v1 finalised checkpoint 1, b1's burn root.
    uint256 internal r1FinalizedAt;

    /// v1's deposit leaves, for refund paths (ReferenceMerkle shares no code with the vault's tree).
    struct Leaf {
        address who;
        uint8 denomIndex;
        bytes32 commitment;
    }
    Leaf[] internal leafData;
    bytes32[] internal leaves;

    // v2: the same `FerrataVault` code, freshly deployed with proof system B.
    FerrataVault internal v2;
    MockRiscZeroVerifierB internal verifierB;
    RiscZeroVerifierEmergencyStop internal estopB;

    // ------------------------------------------------------------------------------------------------------------
    // The scenario
    // ------------------------------------------------------------------------------------------------------------

    /// @dev A deposit into v1, kept in `leafData`/`leaves` so refund paths can be built later.
    function _dep(address who, uint8 denomIndex, bytes32 commitment) internal returns (uint64 leafIndex) {
        leafIndex = _deposit(who, ETH, denomIndex, commitment);
        leafData.push(Leaf({who: who, denomIndex: denomIndex, commitment: commitment}));
        leaves.push(_leafHash(leafIndex, who, ETH, _amount(ETH, denomIndex), commitment));
    }

    /// @dev Builds the path first: the SHA-256 precompile call must not be counted against `vm.expectRevert`.
    function _path(uint64 i) internal view returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    /// @dev Step 2: the in-flight state at the moment a verification bug is announced. Four positions:
    ///      d1 (imported), d2 (never imported), b1 (finalised, unreleased), b2 (released), plus settled history
    ///      (d0a, d0b) so the released and pending burns have escrow behind them.
    function _setupInflight() internal {
        // settled history: two imported 1-ETH deposits beside the rehearsal's own
        uint64 d0a = _dep(carol, 1, keccak256("d0a: carol's settled deposit (its burn is b2)"));
        uint64 d0b = _dep(dave, 1, keccak256("d0b: dave's deposit (dave's burn is b1)"));
        // the rehearsal's deposits
        d1Leaf = _dep(alice, 2, keccak256("d1: imported by LEZ; its holder still holds the wrapped tokens"));
        d2Leaf = _dep(bob, 0, keccak256("d2: never imported; refundable after the sunset"));
        assertEq(uint256(d0a), 0);
        assertEq(uint256(d0b), 1);
        assertEq(uint256(d1Leaf), 2);
        assertEq(uint256(d2Leaf), 3);

        // checkpoint 0 imports the first three leaves (d0a, d0b, d1); its burn root carries b2
        FerrataVault.Checkpoint memory c0 = _nextCheckpoint(vault, R0, 3, false);
        _submit(c0);
        _finalizeAfterDelta(c0); // checkpoint 0
        assertEq(_checkpointRecord(vault, 0).importedLeafCount, 3);

        // b2: released already - v1's settled return leg, before the announcement
        _release(_request(ETH, 1, carol, R0, N_B2));
        assertEq(carol.balance, 1 ether);

        // an hour between a root's finalisation and releases against it, for roots finalised from here on
        vm.prank(admin);
        vault.setReleaseDelay(ETH, DELAY);

        // checkpoint 1: b1's burn root, finalised, the burn not yet released
        FerrataVault.Checkpoint memory c1 = _nextCheckpoint(vault, R1, 3, false);
        _submit(c1);
        _finalizeAfterDelta(c1); // checkpoint 1
        (, uint256 finalizedAt) = vault.burnRootStatus(R1);
        r1FinalizedAt = finalizedAt;
    }

    /// @dev Step 3, the mechanism that applies when the bug is NOT in v1's verifier: the freeze authority stops
    ///      new deposits at once, then retires the vault. (A bug in the verifier itself takes the emergency stop
    ///      instead - EmergencyStop.t.sol, and MigrationFailsafe.t.sol here - and after that nothing more releases
    ///      or refunds, so there is no b1-on-v1 left to honour.)
    function _freezeThenRetire() internal {
        vm.prank(freezer);
        vault.setDepositsFrozen(true);
        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());
    }

    /// @dev Steps 4-5: the in-flight burn is released on v1 (after its delay, to the recipient the burn names),
    ///      then the sunset is finalised and the revocation window passes, so refunds open.
    function _honourInflightThenSunset() internal {
        // b1's root wait: its release delay (1 h) is up
        if (block.timestamp < r1FinalizedAt + DELAY) vm.warp(r1FinalizedAt + DELAY);
        _release(_request(ETH, 1, burnRecipient, R1, N_B1));
        assertEq(burnRecipient.balance, 1 ether);

        // the sunset checkpoint: the refund boundary is its imported leaf count (3 = d1 imported, d2 not)
        FerrataVault.Checkpoint memory cs = _nextCheckpoint(vault, RS, 3, true);
        _submit(cs);
        _finalizeAfterDelta(cs); // checkpoint 2, the sunset
        assertEq(vault.sunsetLeafCount(), 3);
        uint256 opensAt = vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1;
        if (block.timestamp < opensAt) vm.warp(opensAt); // refunds open (v3 D5, D11)
    }

    function _refund(uint64 i) internal {
        Leaf memory l = leafData[i];
        bytes32[32] memory p = _path(i); // built before any expectRevert that wraps this call
        vault.refund(i, l.who, ETH, l.denomIndex, l.commitment, p);
    }

    /// @dev Refund a leaf naming someone other than its depositor: the leaf hash will not match the path.
    function _refundToEve(uint64 i) internal {
        Leaf memory l = leafData[i];
        bytes32[32] memory p = _path(i);
        vault.refund(i, eve, ETH, l.denomIndex, l.commitment, p);
    }

    function _expectRefundRevert(uint64 i, bytes4 expectedError) internal {
        Leaf memory l = leafData[i];
        bytes32[32] memory p = _path(i);
        vm.expectRevert(expectedError);
        vault.refund(i, l.who, ETH, l.denomIndex, l.commitment, p);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 1: vault v1, proof system A
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Step 1: vault v1 is deployed as the tests deploy it, with proof system A - the mock verifier the
    ///         existing tests use for the checkpoint and release journals, behind RISC Zero's emergency stop - and
    ///         the image ids v1's proofs are made under. ETH and a test token are registered.
    function test_migration_step1_v1RunsProofSystemA_ethAndATestTokenRegistered() public {
        assertEq(address(vault.verifier()), address(estop), "v1 pins the emergency stop");
        assertEq(address(estop.verifier()), address(verifier), "around proof system A");
        assertFalse(estop.paused());
        assertEq(vault.checkpointImageId(), CHECKPOINT_IMAGE_ID);
        assertEq(vault.releaseImageId(), RELEASE_IMAGE_ID);
        assertEq(vault.lezConfigId(), LEZ_CONFIG_ID);
        assertEq(vault.storedLezConfigId(), LEZ_CONFIG_ID);
        assertEq(uint8(vault.mode()), uint8(FerrataVault.Mode.A));

        (bool ethRegistered,,,,) = vault.tokenConfig(ETH);
        assertTrue(ethRegistered, "ETH is registered");
        (bool usdcRegistered,,,, uint256[] memory denoms) = vault.tokenConfig(address(usdc));
        assertTrue(usdcRegistered, "the test token is registered");
        assertEq(denoms.length, 3);
        assertFalse(vault.retired());
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 2: the in-flight state at the announcement
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Step 2: at the moment a bug is announced, four positions are in flight:
    ///         - d1 (leaf 2): imported by LEZ - a finalised checkpoint counts it in its `importedLeafCount`, so it
    ///           is claimable on LEZ (its holder burns there) and will not be refundable on Ethereum;
    ///         - d2 (leaf 3): deposited, never imported - nothing on LEZ claims it, so it is a refund candidate;
    ///         - b1: a burn under a finalised checkpoint's root, released only after the announcement;
    ///         - b2: a burn released already.
    function test_migration_step2_inFlightStateAtTheAnnouncement() public {
        _setupInflight();

        // d1 was imported: the checkpoint's count covers it
        assertGt(_checkpointRecord(vault, 0).importedLeafCount, uint256(d1Leaf), "d1 is counted");
        assertGt(vault.finalImportedLeafCount(), uint256(d1Leaf));
        // d2 was not: it sits at the boundary the sunset will set
        assertEq(uint256(d2Leaf), vault.finalImportedLeafCount());
        assertLt(uint256(d2Leaf), vault.nextLeafIndex());

        // b1: finalised, not released
        (bool accepted,) = vault.burnRootStatus(R1);
        assertTrue(accepted);
        assertFalse(vault.nullifierUsed(N_B1));
        assertEq(burnRecipient.balance, 0);

        // b2: released already
        assertTrue(vault.nullifierUsed(N_B2));
        assertEq(carol.balance, 1 ether);

        // the ETH escrow: d0a + d0b + d1 + d2, less b2's paid denomination
        assertEq(vault.escrowed(ETH), 1.002 ether + 1.002 ether + 10.02 ether + 0.1002 ether - 1 ether);
        assertEq(address(vault).balance, vault.escrowed(ETH) + vault.totalOwed(ETH));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 3: stop new activity
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Step 3: stop new activity. For a LEZ change or any planned migration - v1's verifier and Ferrata's
    ///         guests sound - the freeze authority freezes deposits at once and then retires the vault, and new
    ///         deposits revert. When the bug IS in the verifier, the emergency stop applies instead
    ///         (EmergencyStop.t.sol; MigrationFailsafe.t.sol rehearses it): after it nothing more is released or
    ///         refunded (D13). When it is in one of Ferrata's own guests, releases and checkpoints are frozen too,
    ///         and wrong checkpoints revoked (MigrationFailsafe.t.sol, test_guestBug_*; MIGRATION.md section 4). So
    ///         the freeze-and-retire path below is the one in which v1 honours every in-flight burn.
    function test_migration_step3_freezeThenRetireStopsNewDepositsForever() public {
        _setupInflight();

        // the freeze first: an immediate, reversible stop of deposits
        uint256 at = block.timestamp;
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.DepositsFrozenSet(true, at + MAX_FREEZE);
        vm.prank(freezer);
        vault.setDepositsFrozen(true);
        assertTrue(_isFrozen(vault.depositsFrozenUntil()));

        vm.deal(alice, 1.002 ether);
        vm.startPrank(alice);
        usdc.mint(alice, _depositTotal(vault, address(usdc), 0));
        usdc.approve(address(vault), _depositTotal(vault, address(usdc), 0));
        vm.expectRevert(FerrataVault.DepositsAreFrozen.selector);
        vault.depositETH{value: 1.002 ether}(1, keccak256("no deposit while frozen"), 0);
        vm.expectRevert(FerrataVault.DepositsAreFrozen.selector);
        vault.deposit(address(usdc), 0, keccak256("no token deposit either"), 0);
        vm.stopPrank();

        // then the one-way retirement: deposits stop forever
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Retired();
        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());

        // lifting the freeze does not lift retirement
        vm.prank(freezer);
        vault.setDepositsFrozen(false);
        assertFalse(_isFrozen(vault.depositsFrozenUntil()));
        vm.startPrank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: 1.002 ether}(1, keccak256("retired means retired"), 0);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.deposit(address(usdc), 0, keccak256("as does a retired token deposit"), 0);
        vm.stopPrank();
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.AlreadyRetired.selector);
        vault.retire();

        // retirement touched nothing else: the same escrow, the same checkpoints, b1 still releasable
        (bool accepted,) = vault.burnRootStatus(R1);
        assertTrue(accepted);
        assertEq(
            vault.escrowed(ETH), 1.002 ether + 1.002 ether + 10.02 ether + 0.1002 ether - 1 ether, "escrow unchanged"
        );
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 4: in-flight burns are honoured
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Step 4: b1 is released on v1 to its own recipient, after its root's delay, exactly as an ordinary
    ///         release - nothing about the migration changed the release path. A release that names another
    ///         address is refused: the release proof binds the recipient, so nobody redirects it.
    function test_migration_step4_inFlightBurnsAreHonouredOnV1() public {
        _setupInflight();
        _freezeThenRetire();

        // the delay has not passed: no release from the root b1 sits under (the vault refuses before any proof)
        vm.warp(r1FinalizedAt + DELAY - 1);
        FerrataVault.ReleaseRequest memory tooEarly = _request(ETH, 1, burnRecipient, R1, N_B1);
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(tooEarly, SEAL);
        assertFalse(vault.nullifierUsed(N_B1));

        vm.warp(r1FinalizedAt + DELAY);

        // naming another recipient is refused: v1's verifier was never shown a proof of that journal, and the
        // release proof binds the recipient, so nobody redirects it
        bytes32 otherNullifier = keccak256("a nullifier for the wrong-recipient release");
        FerrataVault.ReleaseRequest memory wrongRecipient = _request(ETH, 1, eve, R1, otherNullifier);
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(wrongRecipient, SEAL);
        assertFalse(vault.nullifierUsed(otherNullifier));
        assertEq(eve.balance, 0);

        // after its delay: the release to its own recipient, exactly as an ordinary release (Release.t.sol's
        // pattern: prove, expect the event, call)
        FerrataVault.ReleaseRequest memory r1 = _request(ETH, 1, burnRecipient, R1, N_B1);
        _proveRelease(_releaseJournal(r1));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Release(N_B1, ETH, burnRecipient, 1 ether, relayer, 0, R1);
        vm.prank(relayer);
        vault.release(r1, SEAL);
        assertEq(burnRecipient.balance, 1 ether);
        assertTrue(vault.nullifierUsed(N_B1));

        // escrow down by the paid denomination; fees were zero in schedule 0
        assertEq(vault.escrowed(ETH), 1.002 ether + 1.002 ether + 10.02 ether + 0.1002 ether - 2 ether);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 5: in-flight deposits are honoured
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Step 5: after a sunset checkpoint is final and the revocation window has passed, d2 is refunded to
    ///         its depositor - a permissionless call that pays the depositor named in the leaf, and nobody else.
    ///         d1 is not refundable: LEZ imported it, so its holder claims or burns there instead. Refunding a
    ///         leaf twice, or under another depositor's name, is refused.
    function test_migration_step5_inFlightDepositsAreHonouredAfterTheSunset() public {
        _setupInflight();
        _freezeThenRetire();

        // no refund before a finalised sunset checkpoint
        _expectRefundRevert(d2Leaf, FerrataVault.NotRefundable.selector);

        // the sunset checkpoint: the boundary is its imported leaf count (3)
        FerrataVault.Checkpoint memory cs = _nextCheckpoint(vault, RS, 3, true);
        _submit(cs);
        _finalizeAfterDelta(cs); // checkpoint 2, the sunset
        assertEq(vault.sunsetLeafCount(), 3);
        assertEq(vault.sunsetSetAt(), 2);
        assertTrue(vault.sunsetLeafCountSet());
        assertTrue(vault.finalSunset());

        // its revocation window has not passed: refunds wait, so a forged sunset can still be revoked
        _expectRefundRevert(d2Leaf, FerrataVault.RefundNotYetOpen.selector);

        // d1 was imported: counted in a finalised checkpoint's leaf count, so it is not refundable here
        _expectRefundRevert(d1Leaf, FerrataVault.NotRefundable.selector);

        // the window passes; refunds open
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);

        // naming anyone else as the depositor is refused: the leaf binds its depositor, and the path (built for
        // the real leaf) will not match a leaf forged with another name; nothing has been refunded yet
        bytes32[32] memory p = _path(d2Leaf);
        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(d2Leaf, eve, ETH, leafData[d2Leaf].denomIndex, leafData[d2Leaf].commitment, p);
        assertEq(eve.balance, 0);

        // d2 goes back to bob, the depositor named in the leaf; the caller (anyone) receives nothing
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Refund(d2Leaf, bob, ETH, _amount(ETH, 0) + _relayFee(ETH, 0));
        vm.prank(relayer);
        _refund(d2Leaf);
        assertEq(bob.balance, _amount(ETH, 0) + _relayFee(ETH, 0));
        assertEq(relayer.balance, 0);
        assertTrue(vault.refunded(d2Leaf));

        // twice is refused
        _expectRefundRevert(d2Leaf, FerrataVault.AlreadyRefunded.selector);
        // and d1 is still not refundable: its holder burns on LEZ (step 7)
        _expectRefundRevert(d1Leaf, FerrataVault.NotRefundable.selector);

        // escrow down by d2's whole escrow (denomination plus its relay fee)
        assertEq(vault.escrowed(ETH), 1.002 ether + 1.002 ether + 10.02 ether - 1 ether);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 6: vault v2 with proof system B - and neither vault accepts the other's proofs
    // ------------------------------------------------------------------------------------------------------------

    /// @dev Deploys v2: the same `FerrataVault` contract, unchanged, with another verifier implementation of the
    ///      same `IRiscZeroVerifier` interface behind the same `RiscZeroVerifierEmergencyStop` wrapper, and other
    ///      image ids and a successor `lezConfigId`. As at a deployment, the stop's guardian key is renounced.
    function _deployV2() internal {
        verifierB = new MockRiscZeroVerifierB();
        estopB = new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(address(verifierB)), freezer);
        vm.prank(freezer);
        estopB.renounceOwnership(); // proposal B5, D13: nobody holds the stop's guardian key
        FerrataVault.Deployment memory d = _deployment(IRiscZeroVerifier(address(estopB)), LEZ_CONFIG_ID_B);
        d.checkpointImageId = CHECKPOINT_IMAGE_ID_B;
        d.releaseImageId = RELEASE_IMAGE_ID_B;
        v2 = new FerrataVault(d);
        _registerDefaultTokens(v2);
    }

    function _proveCheckpointB(FerrataVault v, FerrataVault.Checkpoint memory c) internal {
        verifierB.accept(CHECKPOINT_IMAGE_ID_B, sha256(_checkpointJournalBytes(_checkpointJournal(v, LEZ_CONFIG_ID_B, c))));
    }

    function _submitB(FerrataVault v, FerrataVault.Checkpoint memory c) internal {
        _proveCheckpointB(v, c);
        v.submitCandidate(c, SEAL_B);
    }

    /// @dev Rule (A) finalisation on a system-B vault: the wait, a beacon, a second proof that echoes it.
    function _finalizeB(FerrataVault v, FerrataVault.Checkpoint memory c) internal returns (uint256 index) {
        (uint256 submittedAt, uint256 wait) = _waitOf(v, c.newState);
        if (block.timestamp < submittedAt + wait) vm.warp(submittedAt + wait);
        vm.roll(block.number + 1);
        uint256 beaconIndex = v.recordBeacon();
        (, bytes32 nonce) = v.beacons(beaconIndex);
        c.echoNonce = nonce;
        _proveCheckpointB(v, c);
        v.finalize(beaconIndex, c, SEAL_B);
        return v.checkpointCount() - 1;
    }

    function _proveReleaseB(FerrataVault v, uint256 amount, FerrataVault.ReleaseRequest memory r) internal {
        ReleaseJournal memory j = ReleaseJournal({
            chainId: block.chainid,
            vault: address(v),
            lezConfigId: LEZ_CONFIG_ID_B,
            burnRoot: r.burnRoot,
            nullifier: r.nullifier,
            token: r.token,
            amount: amount,
            recipient: r.recipient,
            relayer: r.relayer,
            relayFee: r.relayFee
        });
        verifierB.accept(RELEASE_IMAGE_ID_B, sha256(_releaseJournalBytes(j)));
    }

    /// @notice Step 6: vault v2 is the same `FerrataVault` code - not an upgrade, a new deployment - running proof
    ///         system B (Soft Requirement 4: the verifier is a constructor argument, nothing in the vault names
    ///         one). It deposits, checkpoints and releases under system B. A v1 seal is refused by v2 (its seals do
    ///         not carry system B's format), and a v2 seal is refused by v1: v1 pins its own verifier and image
    ///         ids, and no proof v2 produced is a statement v1's verifier ever accepted.
    function test_migration_step6_v2RunsProofSystemBNeitherVaultAcceptsTheOther() public {
        _deployV2();

        // v2 pins proof system B and its own image ids, against a successor configuration
        assertEq(address(v2.verifier()), address(estopB));
        assertEq(address(estopB.verifier()), address(verifierB));
        assertFalse(estopB.paused());
        assertEq(v2.checkpointImageId(), CHECKPOINT_IMAGE_ID_B);
        assertEq(v2.releaseImageId(), RELEASE_IMAGE_ID_B);
        assertEq(v2.lezConfigId(), LEZ_CONFIG_ID_B);
        assertEq(uint8(v2.mode()), uint8(FerrataVault.Mode.A)); // everything but the proof system is as v1's

        // a deposit on v2 (deposits take no proof), a checkpoint and a release, all under system B
        _depositInto(v2, bob, ETH, 1, keccak256("d3: bob deposits into v2"));
        assertEq(v2.nextLeafIndex(), 1);
        assertEq(v2.escrowed(ETH), 1.002 ether);

        FerrataVault.Checkpoint memory cB = _nextCheckpoint(v2, RB, 1, false);
        _submitB(v2, cB);
        _finalizeB(v2, cB); // v2 checkpoint 0, proven under CHECKPOINT_IMAGE_ID_B
        (bool accepted,) = v2.burnRootStatus(RB);
        assertTrue(accepted);
        assertEq(v2.finalState(), cB.newState);

        FerrataVault.ReleaseRequest memory rB = _request(ETH, 1, recipient, RB, keccak256("v2 release nullifier"));
        _proveReleaseB(v2, 1 ether, rB);
        // a v1 (system A) seal is refused by v2: the wrong format, even for this very journal
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifierB.WrongSealFormat.selector);
        v2.release(rB, SEAL);
        assertFalse(v2.nullifierUsed(keccak256("v2 release nullifier")));
        // the system-B seal releases
        vm.prank(relayer);
        v2.release(rB, SEAL_B);
        assertEq(recipient.balance, 1 ether);
        assertEq(v2.escrowed(ETH), 0.002 ether); // the deposit's relay fee, for its claim on LEZ

        // a v2 seal is refused by v1: fund v1 like a vault with an accepted root, then offer v2's proof
        _deposit(alice, ETH, 1, keccak256("v1 escrow for the cross-check"));
        bytes32 rootX = keccak256("v1 root for the cross-check");
        _acceptRoot(rootX);
        FerrataVault.ReleaseRequest memory rOnV1 =
            _request(ETH, 1, recipient, rootX, keccak256("v2 release nullifier"));
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(rOnV1, SEAL_B);
        // and even if proof system A's verifier were handed v2's own image id, v1 asks only for image A:
        verifier.accept(RELEASE_IMAGE_ID_B, sha256(_releaseJournalBytes(_releaseJournal(rOnV1))));
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(rOnV1, SEAL_B);
        assertEq(recipient.balance, 1 ether, "only v2's release paid");
    }

    /// @notice Step 6, the fixity half: neither vault has any function that changes its verifier or image ids.
    ///         The compiled artifact's ABI is scanned - every function that names the verifier, an image id or the
    ///         LEZ configuration is read-only (a setter would have to appear here) - calling a setter anyway hits
    ///         the fallback, and every admin, freeze-authority, watcher and permissionless action the tests can
    ///         perform leaves the four getters with exactly the values deployment gave them.
    function test_migration_step6_noFunctionChangesVerifierOrImageIds() public {
        _deployV2();

        // the artifact lists the vault's external functions; none writes the proof system
        _artifactHasNoProofSystemSetter();

        bytes4 notFound = FerrataVault.FunctionNotFound.selector;

        // (bool ok, bytes memory ret) = address(vault).call(abi.encodeWithSignature("setVerifier(address)", address(verifierB)));
        (bool ok0, bytes memory ret0) = address(vault).call(abi.encodeWithSignature("setVerifier(address)", address(verifierB)));
        _assertNoSetter(ok0, ret0, "setVerifier", notFound);
        (bool ok1, bytes memory ret1) =
            address(vault).call(abi.encodeWithSignature("setCheckpointImageId(bytes32)", bytes32(uint256(1))));
        _assertNoSetter(ok1, ret1, "setCheckpointImageId", notFound);
        (bool ok2, bytes memory ret2) =
            address(vault).call(abi.encodeWithSignature("setReleaseImageId(bytes32)", bytes32(uint256(1))));
        _assertNoSetter(ok2, ret2, "setReleaseImageId", notFound);
        (bool ok3, bytes memory ret3) = address(vault).call(abi.encodeWithSignature("setLezConfigId(bytes32)", bytes32(uint256(1))));
        _assertNoSetter(ok3, ret3, "setLezConfigId", notFound);

        // a finalised checkpoint the battery can revoke inside its window, and a candidate it can cancel
        _deposit(alice, ETH, 1, keccak256("c"));
        uint256 escrow = vault.escrowed(ETH);
        FerrataVault.Checkpoint memory c0 = _nextCheckpoint(vault, keccak256("battery root 0"), 1, false);
        _submit(c0);
        _finalizeAfterDelta(c0); // checkpoint 0, inside its revocation window

        bytes32 v1Before = _proofSystemFingerprint(vault);
        bytes32 v2Before = _proofSystemFingerprint(v2);

        // watchers: reject-only
        vm.prank(watcher);
        vault.pauseCheckpoints();
        assertTrue(_isFrozen(vault.checkpointsFrozenUntil()));
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
        FerrataVault.Checkpoint memory cand = _nextCheckpoint(vault, keccak256("battery candidate"), 1, false);
        _submit(cand);
        vm.prank(watcher);
        vault.cancelCandidate(cand.newState);
        assertEq(v1Before, _proofSystemFingerprint(vault), "watchers move nothing");

        // the freeze authority: freezes, cancellation, revocation, retirement
        vm.startPrank(freezer);
        vault.setDepositsFrozen(true);
        vault.setReleasesFrozen(true);
        vault.setTokenDepositsFrozen(ETH, true);
        vault.setTokenReleasesFrozen(ETH, true);
        vault.setDepositsFrozen(false);
        vault.setReleasesFrozen(false);
        vault.setTokenDepositsFrozen(ETH, false);
        vault.setTokenReleasesFrozen(ETH, false);
        vault.revokeFrom(0); // inside the revocation window; restores the pre-checkpoint state
        vault.retire();
        vm.stopPrank();
        assertEq(v1Before, _proofSystemFingerprint(vault), "the freeze authority moves nothing");

        // the admin: registry, fees, caps, delays, delta, and the two role changes
        MockERC20 batteryToken = new MockERC20("Battery Token", "BAT", 18);
        vm.startPrank(admin);
        vault.registerToken(address(batteryToken), _params(_denoms(1 ether, 2 ether, 4 ether), 1e12, 1));
        uint256[] memory zeros = new uint256[](3);
        vault.announceFees(ETH, zeros, zeros, zeros);
        vault.setDepositsEnabled(ETH, false);
        vault.setDepositsEnabled(ETH, true);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vault.setGlobalCap(FerrataVault.Direction.Release, UNLIMITED, 0);
        vault.setReleaseDelay(ETH, 1 hours);
        vault.raiseDelta(DELTA + 1 hours);
        vm.stopPrank();
        vm.warp(block.timestamp + 7 days);
        vault.applyFees(ETH); // permissionless; the announced schedule is due
        assertEq(v1Before, _proofSystemFingerprint(vault), "the admin moves nothing");

        // the role changes, through their timelocks
        address newFreezer = makeAddr("battery freeze authority");
        vm.prank(admin);
        vault.proposeRoles([watcher, address(0), address(0)], newFreezer);
        vm.warp(block.timestamp + 14 days);
        vault.applyRoles(); // permissionless
        assertEq(vault.freezeAuthority(), newFreezer);
        address newAdmin = makeAddr("battery admin");
        vm.prank(admin);
        vault.transferAdmin(newAdmin);
        vm.prank(newAdmin);
        vault.acceptAdmin();
        assertEq(vault.admin(), newAdmin);
        assertEq(v1Before, _proofSystemFingerprint(vault), "role changes move nothing");
        assertEq(v2Before, _proofSystemFingerprint(v2));

        // the deposits the retired vault now refuses
        vm.deal(alice, 1.002 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: 1.002 ether}(1, keccak256("retired"), 1);

        // and still: nothing about what either vault accepts as a proof has changed
        assertEq(v1Before, _proofSystemFingerprint(vault));
        assertEq(v2Before, _proofSystemFingerprint(v2));
        assertEq(escrow, vault.escrowed(ETH), "nothing moved: the battery only changed settings");
    }

    function _assertNoSetter(bool ok, bytes memory returndata, string memory name, bytes4 expected) internal view {
        assertFalse(ok, name);
        assertEq(bytes4(returndata), expected, name);
    }

    /// @dev The four getters that define what the vault accepts as a valid proof, hashed for one comparison.
    function _proofSystemFingerprint(FerrataVault v) internal view returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                uint160(address(v.verifier())), v.checkpointImageId(), v.releaseImageId(), v.lezConfigId()
            )
        );
    }

    /// @dev Reads `out/FerrataVault.sol/FerrataVault.json` (the compiled artifact), lists every external function
    ///      of the vault, and checks none can write the proof system: a function whose name names the verifier, an
    ///      image id or the LEZ configuration is read-only on the ABI (its stateMutability is `view`), and there is
    ///      no setter for any of the four. The entries are read by index until the ABI array ends.
    function _artifactHasNoProofSystemSetter() internal {
        string memory artifactPath = "out/FerrataVault.sol/FerrataVault.json";
        string memory artifact = vm.readFile(artifactPath);
        uint256 entries;
        uint256 functions;
        bool hasVerifierGetter;
        bool hasCheckpointImageIdGetter;
        bool hasReleaseImageIdGetter;
        for (uint256 i = 0; i < 1000; i++) {
            string memory at = string.concat(".abi[", vm.toString(i), "]");
            string memory entryType;
            try vm.parseJsonString(artifact, string.concat(at, ".type")) returns (string memory s) {
                entryType = s;
            } catch {
                break; // past the end of the ABI array: every entry has been read
            }
            entries += 1;
            if (!_eqStr(entryType, "function")) continue;
            string memory name = vm.parseJsonString(artifact, string.concat(at, ".name"));
            string memory mutability = vm.parseJsonString(artifact, string.concat(at, ".stateMutability"));
            functions += 1;
            console2.log(name, mutability);

            string memory lower = _lowerStr(name);
            bool namesProofSystem = _containsStr(lower, "verifier") || _containsStr(lower, "imageid")
                || _containsStr(lower, "lezconfigid");
            // every ABI function that names the proof system may only read it; a setter would fail here
            if (namesProofSystem) assertTrue(_eqStr(mutability, "view"), name);
            if (_eqStr(name, "verifier")) hasVerifierGetter = true;
            if (_eqStr(name, "checkpointImageId")) hasCheckpointImageIdGetter = true;
            if (_eqStr(name, "releaseImageId")) hasReleaseImageIdGetter = true;
        }
        assertGt(entries, 100, "the artifact's ABI was found and read");
        assertTrue(hasVerifierGetter, "the verifier getter is on the ABI");
        assertTrue(hasCheckpointImageIdGetter, "the checkpointImageId getter is on the ABI");
        assertTrue(hasReleaseImageIdGetter, "the releaseImageId getter is on the ABI");
        assertGt(functions, 60, "the vault's external functions were all seen");
    }

    function _eqStr(string memory a, string memory b) internal pure returns (bool) {
        return keccak256(bytes(a)) == keccak256(bytes(b));
    }

    /// @dev A copy in lowercase: `bytes(s)` aliases the string's own memory, so the copy is made first.
    function _lowerStr(string memory s) internal pure returns (string memory) {
        bytes memory src = bytes(s);
        bytes memory b = new bytes(src.length);
        for (uint256 i = 0; i < src.length; i++) {
            b[i] = (src[i] >= "A" && src[i] <= "Z") ? bytes1(uint8(src[i]) + 32) : src[i];
        }
        return string(b);
    }

    function _containsStr(string memory haystack, string memory needle) internal pure returns (bool) {
        bytes memory h = bytes(haystack);
        bytes memory n = bytes(needle);
        if (n.length == 0 || h.length < n.length) return false;
        for (uint256 i = 0; i + n.length <= h.length; i++) {
            uint256 j = 0;
            while (j < n.length && h[i + j] == n[j]) {
                j += 1;
            }
            if (j == n.length) return true;
        }
        return false;
    }

    // ------------------------------------------------------------------------------------------------------------
    // Step 7: users move
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Step 7: the holder of d1's LEZ balance burns it on LEZ (simulated as the existing tests do: a burn
    ///         is a leaf under a burn root a checkpoint proves - the vault sees only root, count and nullifier),
    ///         releases on v1 with proof system A - to the recipient the burn names, after the root's delay - and
    ///         deposits the same denomination into v2. What is not rehearsed here: the LEZ side's `Sunset` and the
    ///         new LEZ configuration (v2's `Init` pairing, the import of its tree, the claim against it) run in the
    ///         LEZ harness (`lez/`), not in this vault test.
    function test_migration_step7_usersMove_burnOnLezReleaseOnV1DepositIntoV2() public {
        // the whole rehearsal so far, compressed: steps 2-5
        _setupInflight();
        _freezeThenRetire();
        _honourInflightThenSunset();

        // the refund the sunset opened (bob's, step 5's test in full detail)
        vm.prank(relayer);
        _refund(d2Leaf);
        assertEq(bob.balance, _amount(ETH, 0) + _relayFee(ETH, 0));

        // the holder of d1's balance burns it on LEZ: a burn root under a finalised checkpoint, after the sunset
        FerrataVault.Checkpoint memory c3 = _nextCheckpoint(vault, R3, 3, true);
        _submit(c3);
        _finalizeAfterDelta(c3); // checkpoint 3: after a sunset, every checkpoint carries its count (v3 D4)
        (, uint256 r3FinalizedAt) = vault.burnRootStatus(R3);
        assertTrue(r3FinalizedAt > vault.sunsetFinalizedAt());

        // she releases on v1, to the recipient her burn names (herself), after the root's release delay
        vm.warp(r3FinalizedAt + DELAY);
        FerrataVault.ReleaseRequest memory r3 = _request(ETH, 2, alice, R3, N_A3);
        _proveRelease(_releaseJournal(r3));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Release(N_A3, ETH, alice, 10 ether, relayer, 0, R3);
        vm.prank(relayer);
        vault.release(r3, SEAL);
        assertEq(alice.balance, 10 ether);

        // and deposits the same denomination into v2, which runs proof system B (deposits take no proof)
        _deployV2();
        _depositInto(v2, alice, ETH, 2, keccak256("d5: alice re-deposits into v2"));
        assertEq(v2.nextLeafIndex(), 1);
        assertEq(v2.escrowed(ETH), 10.02 ether);
        assertEq(v2.depositCount(alice), 1);

        // v1's final state: every leaf either released, refunded or claimable-in-principle on LEZ; only the relay
        // fees of the imported, already-burned deposits stay, for claims on LEZ that never came
        assertEq(vault.escrowed(ETH), 0.002 ether + 0.002 ether + 0.02 ether);
        assertEq(address(vault).balance, vault.escrowed(ETH) + vault.totalOwed(ETH));
        // d2's refund closed its position; the tree keeps all four leaves, retired and unmoving
        assertEq(vault.nextLeafIndex(), 4);
        assertTrue(vault.retired());
        assertTrue(vault.finalSunset());
    }
}
