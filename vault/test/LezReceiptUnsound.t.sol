// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {ToggleRecipient} from "./mocks/MockRecipients.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @title LEZ's own check of private-transaction receipts unsound: what each in-flight position does on Ethereum
///        (RFP-022 Security #9; docs/security/lez-receipt-unsound.md)
/// @notice The premise: LEZ accepts a forged private-transaction receipt (LEZ v0.4.0-rc1 and rc3 verify them with
///         risc0-zkvm 3.0.5, in the range of RISC Zero's advisory GHSA-g6qg-9j8p-7j87; not tested, for Logos to
///         confirm; docs/trust.md section 5). A forged receipt can write a burn record that no holder paid for and mint
///         wrapped tokens without a deposit. The vault's own proofs stay sound here: every checkpoint and release
///         below is an honest proof (the mock verifier accepts exactly the journals proven) about a LEZ state that
///         itself holds forged records. So the forgery is upstream of every check the vault makes: the checkpoint is
///         true to LEZ's state, and nothing on Ethereum tells a forged burn from a paid one. What the tests show is
///         what the containment can and cannot do, position by position, which is why the RFP's documentation
///         alternative is taken for this case (no failsafe; docs/security/lez-receipt-unsound.md).
contract LezReceiptUnsoundTest is VaultTestBase {
    uint8 internal constant ONE = 1; // ETH denomination index 1: 1 ether, relay fee 0.002 ether escrowed with it

    bytes32 internal constant FORGED_0 = keccak256("forged burn 0: a record no holder paid for");
    bytes32 internal constant FORGED_1 = keccak256("forged burn 1");
    bytes32 internal constant HONEST_0 = keccak256("honest burn 0");
    bytes32 internal constant HONEST_1 = keccak256("honest burn 1");

    address internal attacker = makeAddr("attacker");

    bytes32[] internal leaves;

    struct Dep {
        address who;
        bytes32 commitment;
        uint64 leafIndex;
    }

    /// @dev An honest ETH deposit of 1 ether, its leaf remembered for refund paths.
    function _dep(FerrataVault v, address who, uint256 i) internal returns (Dep memory d) {
        d.who = who;
        d.commitment = keccak256(abi.encode("commitment", who, i));
        d.leafIndex = _depositInto(v, who, ETH, ONE, d.commitment);
        leaves.push(_leafHash(d.leafIndex, who, ETH, 1 ether, d.commitment));
    }

    function _path(uint64 i) internal view returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    function _releaseExpecting(FerrataVault.ReleaseRequest memory r, bytes memory revertData) internal {
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        if (revertData.length != 0) vm.expectRevert(revertData);
        vault.release(r, SEAL);
    }

    /// @dev Proves and sends `r`, expecting exactly the `Release` event an honest release emits.
    function _releaseEmitting(FerrataVault.ReleaseRequest memory r) internal {
        _proveRelease(_releaseJournal(r));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Release(r.nullifier, r.token, r.recipient, 1 ether, relayer, 0, r.burnRoot);
        vm.prank(relayer);
        vault.release(r, SEAL);
    }

    function _refundsOpen() internal {
        vm.warp(uint256(_checkpointRecord(vault, vault.sunsetSetAt()).finalizedAt) + REVOKE_WINDOW + 1);
    }

    function _sunset(uint64 importedLeafCount) internal returns (uint256 index) {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, keccak256("root at sunset"), importedLeafCount, true);
        _submit(c);
        index = _finalizeAfterDelta(c);
    }

    function _refund(Dep memory d) internal {
        _refundExpecting(d, "");
    }

    /// @dev The path is computed first: computing it calls the SHA-256 precompile, an external call that would
    ///      otherwise take the place of the refund as the call an expected revert applies to.
    function _refundExpecting(Dep memory d, bytes memory revertData) internal {
        bytes32[32] memory p = _path(d.leafIndex);
        if (revertData.length != 0) vm.expectRevert(revertData);
        vault.refund(d.leafIndex, d.who, ETH, ONE, d.commitment, p);
    }

    // ------------------------------------------------------------------------------------------------------------
    // 1. A forged burn in an honest checkpoint is paid like a real one, and revocation only delays it
    // ------------------------------------------------------------------------------------------------------------

    /// @notice The forged burn's release is the same transaction, the same checks and the same event as an honest
    ///         one: the vault has no field that could tell them apart.
    function test_lezUnsound_aForgedBurnInAnHonestCheckpointIsPaidLikeARealOne() public {
        _dep(vault, alice, 0);
        _dep(vault, bob, 1);
        bytes32 root = keccak256("burn root holding HONEST_0 and FORGED_0");
        _acceptRoot(root);

        uint256 before = attacker.balance;
        _releaseEmitting(_request(ETH, ONE, attacker, root, FORGED_0));
        assertEq(attacker.balance - before, 1 ether, "the forged burn is paid in full");
        before = alice.balance;
        _releaseEmitting(_request(ETH, ONE, alice, root, HONEST_0));
        assertEq(alice.balance - before, 1 ether, "and so is the honest one");
    }

    /// @notice `revokeFrom` removes a root, not a record of LEZ's state: the next honest checkpoint carries the forged
    ///         burn again (LEZ's burn tree only grows), and its release goes through against the new root.
    function test_lezUnsound_revocationOnlyDelaysAForgedBurnThatLezStateKeeps() public {
        _dep(vault, alice, 0);
        bytes32 root1 = keccak256("root 1: FORGED_0 at index 0");
        uint256 index = _acceptRoot(root1);

        vm.prank(freezer);
        vault.revokeFrom(index);
        _releaseExpecting(
            _request(ETH, ONE, attacker, root1, FORGED_0),
            abi.encodeWithSelector(FerrataVault.BurnRootNotAccepted.selector)
        );

        // LEZ's state still holds the record: the next checkpoint's root contains it, and it is released there.
        bytes32 root2 = keccak256("root 2: FORGED_0 at index 0, one more burn");
        _acceptRoot(root2);
        uint256 before = attacker.balance;
        _releaseExpecting(_request(ETH, ONE, attacker, root2, FORGED_0), "");
        assertEq(attacker.balance - before, 1 ether, "paid against the next root");
        assertTrue(vault.nullifierUsed(FORGED_0));
    }

    // ------------------------------------------------------------------------------------------------------------
    // 2. What bounds it: the release caps per window (a rate, not a total)
    // ------------------------------------------------------------------------------------------------------------

    /// @notice With ETH's release bucket at 2 ether a day, forged releases drain at most 2 ether a day: the third in
    ///         a day is refused, and a day later two more go through. The caps bound the rate, not the loss.
    ///         (Without a cap the first day would have paid all five: the escrow is the only other bound.)
    function test_lezUnsound_releaseCapsBoundTheLossPerWindowNotInTotal() public {
        for (uint256 i = 0; i < 5; i++) {
            _dep(vault, alice, i);
        }
        vm.prank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 2 ether, uint256(2 ether) / 1 days);
        bytes32 root = keccak256("root with five forged burns");
        _acceptRoot(root);

        for (uint256 i = 0; i < 2; i++) {
            _releaseExpecting(_request(ETH, ONE, attacker, root, keccak256(abi.encode("forged", i))), "");
        }
        _releaseExpecting(
            _request(ETH, ONE, attacker, root, keccak256(abi.encode("forged", uint256(2)))),
            abi.encodeWithSelector(FerrataVault.TokenReleaseCapExceeded.selector)
        );
        vm.warp(block.timestamp + 1 days + 1 minutes); // the refill rounds down: a minute more fills the bucket
        for (uint256 i = 2; i < 4; i++) {
            _releaseExpecting(_request(ETH, ONE, attacker, root, keccak256(abi.encode("forged", i))), "");
        }
        assertEq(attacker.balance, 4 ether, "four forged releases in two days");
    }

    // ------------------------------------------------------------------------------------------------------------
    // 3. The containment runbook, position by position
    // ------------------------------------------------------------------------------------------------------------

    /// @notice The runbook for this case (docs/security/lez-receipt-unsound.md section 3): the token's releases frozen
    ///         (per token, so refunds stay open), the vault retired, LEZ's `Sunset` carried by an honest sunset
    ///         checkpoint, then checkpoints frozen; each freeze renewed. Position by position:
    ///         - a deposit LEZ never imported: refunded whole to its depositor once the window has passed;
    ///         - an honest burn under an accepted root, and a forged one: both held, nullifiers unused;
    ///         - a burn not yet in a final checkpoint: no checkpoint can carry it;
    ///         - an `owed` balance: claimed by its payee, freezes or not;
    ///         and a freeze renewed before it lapses still holds 30 days later.
    function test_lezUnsound_theRunbookHoldsEveryPositionWhileRenewed() public {
        _dep(vault, alice, 0); // leaf 0: imported and claimed on LEZ; its holder burned (HONEST_0)
        _dep(vault, bob, 1); // leaf 1: imported and claimed
        Dep memory unimported = _dep(vault, bob, 2); // leaf 2: never imported on LEZ
        _dep(vault, alice, 3); // leaf 3: an earlier honest release to a payee that could not receive

        bytes32 root = keccak256("root: HONEST_0, HONEST_1, FORGED_0, and the payee's earlier burn");
        _acceptRoot(root);
        ToggleRecipient payee = new ToggleRecipient(); // closed: the payout becomes `owed`
        _releaseExpecting(_request(ETH, ONE, address(payee), root, keccak256("an earlier burn")), "");
        assertEq(vault.owed(address(payee), ETH), 1 ether);

        // Detection. The freeze authority's runbook.
        vm.startPrank(freezer);
        vault.setTokenReleasesFrozen(ETH, true);
        vault.setDepositsFrozen(true);
        vault.retire();
        vm.stopPrank();
        _sunset(2); // LEZ imported leaves 0 and 1 before its Sunset: the honest count
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);

        // A burn not yet in a final checkpoint: no candidate can be submitted.
        FerrataVault.Checkpoint memory later = _nextCheckpoint(vault, keccak256("a later root"), 2, true);
        _proveCheckpoint(_checkpointJournal(vault, LEZ_CONFIG_ID, later));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(later, SEAL);

        // Honest and forged burns under the accepted root: both held.
        _releaseExpecting(
            _request(ETH, ONE, alice, root, HONEST_0),
            abi.encodeWithSelector(FerrataVault.TokenReleasesAreFrozen.selector)
        );
        _releaseExpecting(
            _request(ETH, ONE, attacker, root, FORGED_0),
            abi.encodeWithSelector(FerrataVault.TokenReleasesAreFrozen.selector)
        );

        // The owed balance: claimed by its payee under the freezes.
        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        assertEq(address(payee).balance, 1 ether, "owed balance claimed");

        // The unimported deposit: refunded whole (denomination and relay fee) after the window, per-token freeze on.
        _refundsOpen();
        uint256 bobBefore = bob.balance;
        _refund(unimported);
        assertEq(bob.balance - bobBefore, 1 ether + _relayFee(ETH, ONE), "refunded whole");

        // Every freeze renewed on day 29 still holds on day 31: releases, checkpoints and deposits; the burns pending.
        vm.warp(block.timestamp + 26 days);
        vm.startPrank(freezer);
        vault.setTokenReleasesFrozen(ETH, true);
        vault.setCheckpointsFrozen(true);
        vault.setDepositsFrozen(true);
        vm.stopPrank();
        vm.warp(block.timestamp + 2 days);
        _releaseExpecting(
            _request(ETH, ONE, alice, root, HONEST_0),
            abi.encodeWithSelector(FerrataVault.TokenReleasesAreFrozen.selector)
        );
        FerrataVault.Checkpoint memory day31 = _nextCheckpoint(vault, keccak256("a root on day 31"), 2, true);
        _proveCheckpoint(_checkpointJournal(vault, LEZ_CONFIG_ID, day31));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(day31, SEAL);
        assertGt(vault.depositsFrozenUntil(), block.timestamp, "the deposit freeze, renewed, still holds");
        assertFalse(vault.nullifierUsed(HONEST_0), "the honest burn is still pending");
        assertFalse(vault.nullifierUsed(FORGED_0), "and so is the forged one");
    }

    // ------------------------------------------------------------------------------------------------------------
    // 4. Without the freeze: a race for one token's escrow, and the shortfall falls on the last
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Three deposits escrow 3.006 ether; LEZ's state holds two honest and two forged burns. Released in
    ///         arrival order, the forged ones are paid like the others and the last honest burn finds 0.006 ether:
    ///         refused (`InsufficientEscrow`), its holder's wrapped token burned on LEZ. A lapsed freeze is this.
    function test_lezUnsound_withoutTheFreezeTheShortfallFallsOnTheLastRedeemer() public {
        for (uint256 i = 0; i < 3; i++) {
            _dep(vault, alice, i);
        }
        bytes32 root = keccak256("root: FORGED_0, FORGED_1, HONEST_0, HONEST_1");
        _acceptRoot(root);
        vm.prank(freezer);
        vault.setTokenReleasesFrozen(ETH, true);
        vm.warp(block.timestamp + MAX_FREEZE); // not renewed: lapses

        _releaseExpecting(_request(ETH, ONE, attacker, root, FORGED_0), "");
        _releaseExpecting(_request(ETH, ONE, attacker, root, FORGED_1), "");
        _releaseExpecting(_request(ETH, ONE, alice, root, HONEST_0), "");
        assertEq(vault.escrowed(ETH), 3 * _relayFee(ETH, ONE), "only the three relay fees remain");
        _releaseExpecting(
            _request(ETH, ONE, bob, root, HONEST_1), abi.encodeWithSelector(FerrataVault.InsufficientEscrow.selector)
        );
        assertFalse(vault.nullifierUsed(HONEST_1), "the last honest burn is unpaid");
    }

    /// @notice Refunds draw on the same escrow: forged releases made before the incident was seen leave a deposit
    ///         that LEZ never imported unrefundable (`InsufficientEscrow`), although nothing about it was forged.
    function test_lezUnsound_forgedReleasesBeforeDetectionCanLeaveAnUnimportedDepositUnrefundable() public {
        _dep(vault, alice, 0); // imported on LEZ
        Dep memory unimported = _dep(vault, bob, 1);
        bytes32 root = keccak256("root: FORGED_0, FORGED_1");
        _acceptRoot(root);
        _releaseExpecting(_request(ETH, ONE, attacker, root, FORGED_0), "");
        _releaseExpecting(_request(ETH, ONE, attacker, root, FORGED_1), "");

        vm.prank(freezer);
        vault.retire();
        _sunset(1);
        _refundsOpen();
        _refundExpecting(unimported, abi.encodeWithSelector(FerrataVault.InsufficientEscrow.selector));
    }

    // ------------------------------------------------------------------------------------------------------------
    // 5. A forged import count (the attestation program's records are within the forger's reach too)
    // ------------------------------------------------------------------------------------------------------------

    /// @notice A LEZ state whose imported leaf count is above the vault's tree cannot be finalised
    ///         (`ImportedLeafCountTooHigh`): every later burn waits, until enough real deposits make the count
    ///         possible (rule (B) here; rule (A) checks the same at finalisation).
    function test_lezUnsound_aForgedImportCountAboveTheTreeHaltsCheckpointsUntilDepositsCatchUp() public {
        FerrataVault vB = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        _registerDefaultTokens(vB);
        _depositInto(vB, alice, ETH, ONE, keccak256("leaf 0"));
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vB, keccak256("root over a forged import"), 3, false);
        _submit(vB, LEZ_CONFIG_ID, c);
        (uint256 submittedAt, uint256 wait) = _waitOf(vB, c.newState);
        vm.warp(submittedAt + wait);
        vm.expectRevert(FerrataVault.ImportedLeafCountTooHigh.selector);
        vB.finalizeAttested(c);

        _depositInto(vB, bob, ETH, ONE, keccak256("leaf 1"));
        _depositInto(vB, bob, ETH, ONE, keccak256("leaf 2"));
        vB.finalizeAttested(c);
        assertEq(vB.finalImportedLeafCount(), 3, "accepted once the tree holds three leaves");
    }

    /// @notice After retirement nothing catches a forged count up (review round 1): a LEZ state whose imported count is
    ///         above the retired vault's tree is never finalised, the sunset checkpoint included, so the refund boundary
    ///         is never set and no deposit is ever refunded; every burn not yet in a final checkpoint waits for good. (A
    ///         count above 2**32, the tree's size, could never be caught up even without retirement.)
    function test_lezUnsound_afterRetirementAForgedCountAboveTheTreeEndsCheckpointsAndRefunds() public {
        Dep memory unimported = _dep(vault, bob, 0);
        vm.prank(freezer);
        vault.retire();
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, keccak256("root at sunset"), 2, true);
        _submit(c);
        (uint256 submittedAt, uint256 wait) = _waitOf(vault, c.newState);
        vm.warp(submittedAt + wait);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(vault, LEZ_CONFIG_ID, c));
        vm.expectRevert(FerrataVault.ImportedLeafCountTooHigh.selector);
        vault.finalize(beaconIndex, c, SEAL);

        // No deposit can raise the tree to the count: the vault is retired.
        uint256 total = _depositTotal(vault, ETH, ONE);
        uint64 index = vault.depositCount(alice);
        vm.deal(alice, total);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: total}(ONE, keccak256("a deposit to catch the count up"), index);

        // A year later the same: no boundary, no refund.
        vm.warp(block.timestamp + 365 days);
        vm.expectRevert(FerrataVault.ImportedLeafCountTooHigh.selector);
        vault.finalize(beaconIndex, c, SEAL);
        assertFalse(vault.sunsetLeafCountSet(), "the refund boundary is never set");
        _refundExpecting(unimported, abi.encodeWithSelector(FerrataVault.NotRefundable.selector));
    }

    /// @notice A forged count at or below the tree is accepted, and at a sunset it sets the refund boundary: a deposit
    ///         LEZ never really imported, below the forged count, is not refundable on Ethereum. Whether it can still
    ///         be claimed on LEZ depends on an honest import covering it before `Sunset` (LEZ side; not tested here).
    function test_lezUnsound_aForgedImportCountAtSunsetLeavesADepositBelowItUnrefundable() public {
        _dep(vault, alice, 0); // honestly imported
        Dep memory notImported = _dep(vault, bob, 1); // LEZ never imported its real root
        vm.prank(freezer);
        vault.retire();
        _sunset(2); // the forged count: 2, although LEZ imported one real leaf
        _refundsOpen();
        _refundExpecting(notImported, abi.encodeWithSelector(FerrataVault.NotRefundable.selector));
    }

}
