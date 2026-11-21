// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";
import {Receipt as RiscZeroReceipt} from "risc0/IRiscZeroVerifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {ToggleRecipient} from "./mocks/MockRecipients.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @title The Security #9 failsafe, rehearsed (RFP-022 Security #9; SPEC-v3 D8, D13)
/// @notice RFP-022 Security #9 asks for a failsafe for funds at risk once a verification bug is found, which must
///         not be a backdoor. The design's answer is that there IS no escrow-moving failsafe, and this file shows
///         exactly what that means on the vault (each test cites what it proves; `vault/MIGRATION.md` §5 collects
///         them):
///
///         - the only failsafe is the STOP (`bridge.md` §15): when the bug is in the proof system itself, anyone
///           holding a verifying receipt for the all-zero claim digest - RISC Zero's proof-of-exploit path, exactly
///           what `EmergencyStop.t.sol` shows - trips the verifier's emergency stop, and it is one-way;
///         - after the stop, in-flight burns are never released and in-flight deposits never refunded, even when a
///           sunset checkpoint was finalised first: D8's known limit, as built under D13 ("after a stop nothing more
///         is released or refunded"); the only value that still moves is an `owed` balance, to its own payee;
///         - no admin, freeze authority, watcher or anyone else can move escrow to any address: every role's every
///         function either refuses or moves nothing, and the value-moving functions of the vault (release, refund,
///         claimOwed) pay only the party a proof or a leaf names, only to the address it names, and only what was
///         already escrowed or owed - a recovery needing someone to choose new verification logic would be the
///         backdoor Security #8 forbids;
///         - for the migration rehearsal itself (Migration.t.sol step 3): a bug in the VERIFIER takes this stop
///         instead of the freeze-and-retire path, and after it there is no b1-on-v1 left to honour;
///         - a bug in one of Ferrata's own guests cannot trip the stop (RISC Zero is sound: there is no proof of
///         exploit to give it): the answer is the release freeze, the checkpoint freeze and `revokeFrom` (D13,
///         `docs/trust.md` section 6), each lasting `maxFreeze` unless renewed. The `test_guestBug_*` tests show
///         what that does to the in-flight positions (MIGRATION.md section 4).
contract MigrationFailsafeTest is VaultTestBase {
    bytes32 internal constant ROOT0 = keccak256("failsafe burn root 0 (the released burn)");
    bytes32 internal constant ROOT1 = keccak256("failsafe burn root 1 (the burn in flight at the stop)");
    bytes32 internal constant ROOT_NONE = keccak256("failsafe root of a candidate never finalised");
    bytes32 internal constant ROOT_FORGED = keccak256("failsafe burn root a buggy checkpoint guest let through");
    /// @notice A junk seal: after the stop no proof is checked at all, and before it none was made.
    bytes internal constant JUNK = hex"6a756e6b";

    address internal eve = makeAddr("eve"); // named by no proof and no leaf: receives nothing
    address internal carol = makeAddr("carol"); // a depositor whose burn was released before the stop
    address internal dave = makeAddr("dave"); // a depositor whose burn is in flight at the stop

    struct Leaf {
        address who;
        uint8 denomIndex;
        bytes32 commitment;
    }
    Leaf[] internal leafData;
    bytes32[] internal leaves;

    function _dep(address who, uint8 denomIndex, bytes32 commitment) internal returns (uint64 leafIndex) {
        leafIndex = _deposit(who, ETH, denomIndex, commitment);
        leafData.push(Leaf({who: who, denomIndex: denomIndex, commitment: commitment}));
        leaves.push(_leafHash(leafIndex, who, ETH, _denomsOf(vault, ETH)[denomIndex], commitment));
    }

    function _path(uint64 i) internal view returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    /// @dev The stop, tripped by the proof of exploit: a verifying receipt for the all-zero claim digest, which a
    ///      sound proof system can never produce. This is the proven-unsoundness path, and it is open to anyone.
    function _stopByProofOfExploit() internal {
        verifier.acceptClaim(bytes32(0)); // stands in for the receipt RISC Zero would publish
        vm.prank(eve);
        estop.estop(RiscZeroReceipt({seal: JUNK, claimDigest: bytes32(0)}));
        assertTrue(estop.paused(), "stopped");
    }

    // ------------------------------------------------------------------------------------------------------------
    // What a tripped stop leaves working
    // ------------------------------------------------------------------------------------------------------------

    /// @notice After the stop, every in-flight position is stranded except `owed` balances (D8):
    ///         - a burn under a finalised root is never released;
    ///         - no checkpoint proof is accepted any more;
    ///         - a deposit is never refunded;
    ///         - what a payout failed to push before the stop is still claimable by its own payee, only to itself.
    function test_failsafe_stopStrandsInflightPositionsExceptOwedBalances() public {
        _dep(alice, 1, keccak256("a deposit behind the released burn"));
        _dep(bob, 0, keccak256("a deposit never imported"));

        _acceptRoot(ROOT0);
        // a release before the stop whose recipient cannot receive: the vault credits `owed` (spec §5.4)
        ToggleRecipient payee = new ToggleRecipient();
        _release(_request(ETH, 1, address(payee), ROOT0, keccak256("the failed-payout nullifier")));
        assertEq(vault.owed(address(payee), ETH), 1 ether);
        assertEq(vault.nullifierUsed(keccak256("the failed-payout nullifier")), true);

        // the burn in flight: finalised, never released
        _acceptRoot(ROOT1);
        (bool accepted,) = vault.burnRootStatus(ROOT1);
        assertTrue(accepted);
        uint256 escrowAtStop = vault.escrowed(ETH);

        _stopByProofOfExploit();
        assertTrue(estop.paused());

        // an in-flight burn is never released: the vault asks the verifier, which is stopped (the 0.1-ETH
        // denomination is within escrow, so it is the stop that refuses, not the escrow check before it)
        FerrataVault.ReleaseRequest memory r1 = _request(ETH, 0, eve, ROOT1, keccak256("b1's nullifier"));
        vm.prank(relayer);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r1, JUNK);
        assertEq(eve.balance, 0);

        // no checkpoint proof is accepted any more
        FerrataVault.Checkpoint memory fresh = _nextCheckpoint(keccak256("a root never proven"));
        _proveCheckpoint(_checkpointJournal(fresh));
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.submitCandidate(fresh, JUNK);

        // the runbook (README, "Known limits", D8's second bullet): freeze and retire, so nothing new enters
        vm.startPrank(freezer);
        vault.setDepositsFrozen(true);
        vault.retire();
        vm.stopPrank();
        assertTrue(vault.retired());

        // a deposit is never refunded, even retired, even with a real leaf and a real path
        bytes32[32] memory refused = _path(1); // built first: the precompile must not take the expectRevert
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(1, bob, ETH, leafData[1].denomIndex, leafData[1].commitment, refused);
        assertFalse(vault.refunded(1));

        // the one thing that still pays: the payee's own owed balance, to itself, claimed by itself
        vm.prank(eve);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH); // nobody else is owed anything
        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        assertEq(address(payee).balance, 1 ether);

        // accounting: everything but the claimed `owed` stays in the vault, exactly as D8 says
        assertEq(vault.escrowed(ETH), escrowAtStop, "escrow did not move");
        assertEq(address(vault).balance, vault.escrowed(ETH) + vault.totalOwed(ETH));
        // bob's whole escrow (0.1 ETH + its relay fee) and the released-and-claimed denomination's relay fee
        assertEq(address(vault).balance, 0.1 ether + 0.1 ether / 500 + 1 ether / 500);
    }

    /// @notice D8's "unless a sunset checkpoint was finalised first", as built: it does not help. With the vault
    ///         retired, a sunset checkpoint finalised and its revocation window passed, one refund succeeds - the
    ///         same refund a second later, after the stop, is refused: D13 ("after a stop nothing more is released
    ///         or refunded") is what the code does, and the deposited escrow stays in the vault for good.
    function test_failsafe_sunsetBeforeTheStopDoesNotOpenRefundsAfterIt() public {
        _dep(alice, 1, keccak256("imported before the sunset"));
        uint64 bobFirst = _dep(bob, 0, keccak256("never imported; refunded before the stop"));
        uint64 bobSecond = _dep(bob, 0, keccak256("never imported; stranded by the stop"));

        vm.prank(freezer);
        vault.retire();
        FerrataVault.Checkpoint memory sunset = _nextCheckpoint(vault, ROOT0, 1, true);
        _submit(sunset);
        _finalizeAfterDelta(sunset); // the sunset checkpoint, finalised before the stop
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1); // refunds open (v3 D5, D11)

        // control: the same refund the test performs after the stop, here before it, pays
        vm.prank(relayer);
        vault.refund(bobFirst, bob, ETH, leafData[bobFirst].denomIndex, leafData[bobFirst].commitment, _path(bobFirst));
        assertEq(bob.balance, 0.1 ether + 0.1 ether / 500);
        assertTrue(vault.refunded(bobFirst));

        _stopByProofOfExploit();

        // the second leaf is in every other way refundable: retired vault, finalised sunset, window passed, real
        // path - and the stop is the only thing that refuses it
        bytes32[32] memory refused = _path(bobSecond); // built first: the precompile must not take the expectRevert
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(bobSecond, bob, ETH, leafData[bobSecond].denomIndex, leafData[bobSecond].commitment, refused);
        assertFalse(vault.refunded(bobSecond));

        // so escrow that would have been refundable is in the vault for good
        assertEq(bob.balance, 0.1 ether + 0.1 ether / 500, "no second refund paid");
        assertEq(vault.escrowed(ETH), 1.002 ether + 0.1 ether + 0.1 ether / 500);
    }

    // ------------------------------------------------------------------------------------------------------------
    // No role can move escrow to any address
    // ------------------------------------------------------------------------------------------------------------

    /// @dev Everything of value the vault and the actors hold, hashed for one comparison (USDC is the token that
    ///      is not ETH; `eve` stands for every address that is neither the admin, the freeze authority, a watcher,
    ///      a depositor nor a payee).
    function _valueState() internal view returns (bytes32) {
        // three hashes, each small enough for one abi.encode, then one over the three
        bytes32 vaultSide = keccak256(
            abi.encode(
                address(vault).balance,
                usdc.balanceOf(address(vault)),
                vault.escrowed(ETH),
                vault.escrowed(address(usdc)),
                vault.totalOwed(ETH),
                vault.totalOwed(address(usdc))
            )
        );
        bytes32 ethSide = keccak256(
            abi.encode(eve.balance, admin.balance, freezer.balance, watcher.balance, alice.balance, bob.balance)
        );
        bytes32 tokenSide = keccak256(
            abi.encode(
                usdc.balanceOf(eve),
                usdc.balanceOf(admin),
                usdc.balanceOf(freezer),
                usdc.balanceOf(watcher),
                usdc.balanceOf(alice),
                usdc.balanceOf(bob)
            )
        );
        return keccak256(abi.encode(vaultSide, ethSide, tokenSide));
    }

    /// @notice After the stop, each role's every function that could move value is tried - and either refuses or
    ///         moves nothing. The vault's value-moving functions are `deposit`/`depositETH` (which move value IN),
    ///         `release`, `refund` and `claimOwed`; the last three pay only a party a proof or a leaf names, to the
    ///         address it names. Every administrative function - the admin's, the freeze authority's, the
    ///         watchers', the permissionless ones - is exercised here and moves nothing.
    function test_failsafe_noRoleCanMoveEscrowToAnyAddressAfterTheStop() public {
        ToggleRecipient payee = _stoppedRetiredWorld();

        // the payee claims what is owed to it, to itself; after this, nothing of value can change hands
        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        assertEq(address(payee).balance, 1 ether, "the payee's own owed is the one thing that pays");

        vm.deal(eve, 1 ether); // armed before the snapshot: the ether is eve's own, the vault never takes it
        bytes32 before = _valueState();
        _everyPayoutRefuses();
        assertEq(_valueState(), before, "no value moved while refusing");
        _everyAdminFunctionMovesNothing();
        assertEq(_valueState(), before, "no admin function moves value");
        _everyRejectOnlyFunctionMovesNothing();
        assertEq(_valueState(), before, "no freeze-authority or watcher function moves value");
        _roleChangesMoveNothing();
        assertEq(_valueState(), before, "no role change moves value");
        _noRawSendReachesTheVault();
        assertEq(_valueState(), before, "still: no value moved");
    }

    /// @dev The world the roles act on: ETH and USDC escrow, an `owed` balance, a finalised root that pays nothing
    ///      yet, a pending candidate, the verifier stopped by a proof of exploit, one more deposit that entered
    ///      after the stop (the D8 known limit), and the runbook's freeze and retire.
    function _stoppedRetiredWorld() internal returns (ToggleRecipient payee) {
        _dep(alice, 1, keccak256("eth escrow"));
        _dep(bob, 0, keccak256("eth escrow, refund candidate"));
        _deposit(bob, address(usdc), 1, keccak256("usdc escrow"));

        _acceptRoot(ROOT0);
        // an owed balance exists, so claimOwed is not vacuous
        payee = new ToggleRecipient();
        _release(_request(ETH, 1, address(payee), ROOT0, keccak256("the owed nullifier")));
        assertEq(vault.owed(address(payee), ETH), 1 ether);

        // a finalised root that pays nothing yet (the burn in flight), and a pending candidate to cancel
        _acceptRoot(ROOT1);
        FerrataVault.Checkpoint memory pending = _nextCheckpoint(keccak256("the pending candidate"));
        _submit(pending);
        pendingState = pending.newState;

        _stopByProofOfExploit();

        // the known limit (D8's second bullet): deposits still enter a stopped vault's escrow...
        _deposit(alice, ETH, 0, keccak256("escrow that enters after the stop"));
        // ...so the runbook retires it at once, which stops deposits forever
        vm.startPrank(freezer);
        vault.setDepositsFrozen(true);
        vault.retire();
        vm.stopPrank();
        vm.deal(alice, 0.1002 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: 0.1002 ether}(0, keccak256("no more"), 2);
    }

    /// @dev The pending candidate `_everyRejectOnlyFunctionMovesNothing` cancels.
    bytes32 internal pendingState;

    /// @dev release, refund and claimOwed, tried by every role and by an unprivileged address. The release asks
    ///      for the 0.1-ETH denomination, which is within the escrow, so it is the stop that refuses.
    function _everyPayoutRefuses() internal {
        FerrataVault.ReleaseRequest memory r1 = _request(ETH, 0, eve, ROOT1, keccak256("eve's nullifier"));
        vm.prank(eve);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r1, JUNK);
        vm.prank(admin);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r1, JUNK);
        vm.prank(freezer);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r1, JUNK);
        vm.prank(watcher);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r1, JUNK);
        assertEq(eve.balance, 1 ether, "the refused sends came straight back to eve");

        Leaf memory leaf = leafData[1];
        bytes32[32] memory p = _path(1); // real leaf, real path: the stop is the only thing that refuses
        vm.prank(eve);
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(1, leaf.who, ETH, leaf.denomIndex, leaf.commitment, p);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(1, leaf.who, ETH, leaf.denomIndex, leaf.commitment, p);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(1, leaf.who, ETH, leaf.denomIndex, leaf.commitment, p);

        // claimOwed: only a payee's own balance, claimed by the payee
        vm.startPrank(eve);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(address(usdc));
        vm.stopPrank();
        vm.prank(admin);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
    }

    function _everyAdminFunctionMovesNothing() internal {
        MockERC20 token = new MockERC20("Failsafe Token", "FST", 18);
        vm.startPrank(admin);
        vault.registerToken(address(token), _params(_denoms(1 ether, 2 ether, 4 ether), 1e12, 1));
        uint256[] memory zeros = new uint256[](3);
        vault.announceFees(ETH, zeros, zeros, zeros);
        vault.setDepositsEnabled(ETH, false);
        vault.setDepositsEnabled(ETH, true);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vault.setGlobalCap(FerrataVault.Direction.Release, UNLIMITED, 0);
        vault.setReleaseDelay(ETH, 1 hours);
        vault.raiseDelta(5 hours);
        vm.stopPrank();
    }

    function _everyRejectOnlyFunctionMovesNothing() internal {
        vm.startPrank(freezer);
        vault.setDepositsFrozen(false); // lifting the freeze lifts only the freeze
        vault.setReleasesFrozen(true);
        vault.setReleasesFrozen(false);
        vault.setCheckpointsFrozen(true);
        vault.setCheckpointsFrozen(false);
        vault.setTokenDepositsFrozen(ETH, true);
        vault.setTokenDepositsFrozen(ETH, false);
        vault.setTokenReleasesFrozen(ETH, true);
        vault.setTokenReleasesFrozen(ETH, false);
        vault.cancelCandidate(pendingState);
        vault.revokeFrom(0); // inside the revocation window; a revocation restores states, it pays nothing
        vm.stopPrank();
        vm.prank(watcher);
        vault.pauseCheckpoints();
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
    }

    function _roleChangesMoveNothing() internal {
        vault.applyFees(ETH); // nothing announced is due yet
        vm.warp(block.timestamp + 7 days);
        vault.applyFees(ETH); // the announced schedule takes effect; it moves no value
        address newFreezer = makeAddr("the replacement freeze authority");
        vm.prank(admin);
        vault.proposeRoles([watcher, address(0), address(0)], newFreezer);
        vm.warp(block.timestamp + 14 days);
        vault.applyRoles(); // permissionless, past its timelock
        assertEq(vault.freezeAuthority(), newFreezer);
        address newAdmin = makeAddr("the replacement admin");
        vm.prank(admin);
        vault.transferAdmin(newAdmin);
        vm.prank(newAdmin);
        vault.acceptAdmin();
        assertEq(vault.admin(), newAdmin);
        vm.prank(newFreezer);
        vm.expectRevert(FerrataVault.AlreadyRetired.selector); // the vault is retired already
        vault.retire();
    }

    function _noRawSendReachesTheVault() internal {
        (bool ok,) = address(vault).call{value: 1 ether}("");
        assertFalse(ok, "no receive");
        (ok,) = address(vault).call(abi.encodeWithSignature("sweep(address)", eve));
        assertFalse(ok, "no fallback sweep");
    }

    // ------------------------------------------------------------------------------------------------------------
    // Which stop applies when (Migration.t.sol step 3's other case)
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Migration.t.sol's rehearsal stops new activity with the freeze authority's freeze and `retire`,
    ///         which leaves v1's verifier running, so an in-flight burn can still be released on v1. When the bug
    ///         is in the proof system itself, the emergency stop applies instead (exactly what EmergencyStop.t.sol
    ///         shows) - and after it, nothing more is released or refunded: b1's recipient is never paid on v1 and
    ///         the unimported deposit is never refunded. That is the whole difference between the two mechanisms,
    ///         and it is why `vault/MIGRATION.md` answers Security #9 with a document rather than a recovery
    ///         function.
    function test_failsafe_theVerifierBugCase_theEmergencyStopAppliesAndB1Strands() public {
        // v1's in-flight state, as the rehearsal built it: two imported deposits (one burned and released, one
        // burned and pending) and one deposit LEZ never imported
        _dep(carol, 1, keccak256("behind b2, the released burn"));
        _dep(dave, 1, keccak256("behind b1, the burn in flight"));
        uint64 stranded = _dep(bob, 0, keccak256("never imported"));

        _acceptRoot(ROOT0);
        _release(_request(ETH, 1, carol, ROOT0, keccak256("b2's nullifier"))); // b2, released already
        _acceptRoot(ROOT1); // b1's root, finalised, not released

        uint256 escrowAtStop = vault.escrowed(ETH);

        // the announcement says the bug is in the VERIFIER: the emergency stop applies
        _stopByProofOfExploit();

        // the runbook (README, "Known limits", D8's second bullet): freeze deposits and retire
        vm.startPrank(freezer);
        vault.setDepositsFrozen(true);
        vault.retire();
        vm.stopPrank();

        // b1, which the rehearsal releases on v1 when the verifier is sound, is never released here
        FerrataVault.ReleaseRequest memory r1 = _request(ETH, 1, eve, ROOT1, keccak256("b1's nullifier"));
        vm.prank(relayer);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r1, JUNK);
        assertEq(eve.balance, 0);

        // and the unimported deposit is never refunded either: no checkpoint can finalise after the stop, so even
        // a sunset cannot be set up, and the stop's own check refuses refunds besides (its own tests above)
        bytes32[32] memory refused = _path(stranded); // built first: the precompile must not take the expectRevert
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(stranded, bob, ETH, leafData[stranded].denomIndex, leafData[stranded].commitment, refused);

        // nothing moved, nobody can move anything, and the escrow stays in the vault for good
        assertEq(vault.escrowed(ETH), escrowAtStop);
        assertEq(address(vault).balance, vault.escrowed(ETH) + vault.totalOwed(ETH));
        assertTrue(vault.retired() && estop.paused());
    }

    // ------------------------------------------------------------------------------------------------------------
    // A bug in one of Ferrata's own guests: the freezes and revokeFrom, not the stop (D13; MIGRATION.md section 4)
    // ------------------------------------------------------------------------------------------------------------

    /// @dev The positions of the guest-bug tests: d1 imported and burned on LEZ (b1, its root finalised by an honest
    ///      checkpoint), d2 never imported; the vault retired and sunset, the sunset's revocation window over, so
    ///      that b1's release and d2's refund would both pay now. Returns d2's leaf index.
    function _guestBugWorld() internal returns (uint64 d2) {
        _dep(alice, 1, keccak256("d1: imported; burned on LEZ as b1"));
        d2 = _dep(bob, 0, keccak256("d2: never imported"));
        _acceptRoot(ROOT1); // b1's burn root, finalised by an honest checkpoint
        vm.prank(freezer);
        vault.retire();
        FerrataVault.Checkpoint memory sunset = _nextCheckpoint(vault, ROOT0, 1, true);
        _submit(sunset);
        _finalizeAfterDelta(sunset);
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);
    }

    /// @notice A bug in the RELEASE guest (a release nobody burned for would verify). The stop cannot be tripped (no
    ///         proof of exploit exists for a sound RISC Zero), so the freeze authority freezes releases, which stops
    ///         every release and every refund, and checkpoints. Each freeze lasts `maxFreeze` (30 days) and is
    ///         renewed by setting it again: renewed, it holds; left to lapse, the forged release pays. So the runbook
    ///         renews it until v2 is live, and while it holds b1 is not released and d2 not refunded.
    function test_guestBug_releaseFreezeHoldsReleasesAndRefundsOnlyWhileRenewed() public {
        uint64 d2 = _guestBugWorld();
        vm.startPrank(freezer);
        vault.setReleasesFrozen(true);
        vault.setCheckpointsFrozen(true);
        vm.stopPrank();

        // what the buggy guest lets anyone prove: a release of b1's root to an address no burn names
        FerrataVault.ReleaseRequest memory forged = _request(ETH, 1, eve, ROOT1, keccak256("a forged nullifier"));
        _proveRelease(_releaseJournal(forged));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forged, SEAL);
        // b1's own release waits too
        FerrataVault.ReleaseRequest memory b1 = _request(ETH, 1, alice, ROOT1, keccak256("b1's nullifier"));
        _proveRelease(_releaseJournal(b1));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(b1, SEAL);
        // and so does d2's refund, which the global release freeze stops
        bytes32[32] memory p2 = _path(d2);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.refund(d2, bob, ETH, leafData[d2].denomIndex, leafData[d2].commitment, p2);
        // no new checkpoint either
        FerrataVault.Checkpoint memory next = _nextCheckpoint(keccak256("a root proven after the announcement"));
        _proveCheckpoint(_checkpointJournal(next));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(next, SEAL);

        // renewed one second before it would lapse, the freeze holds another 30 days
        vm.warp(block.timestamp + MAX_FREEZE - 1);
        vm.prank(freezer);
        vault.setReleasesFrozen(true);
        vm.warp(block.timestamp + MAX_FREEZE - 1);
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forged, SEAL);
        assertEq(eve.balance, 0);

        // left to lapse, it lets the forged release through: why the runbook renews it until v2 is live
        vm.warp(block.timestamp + 2);
        vm.prank(relayer);
        vault.release(forged, SEAL);
        assertGt(eve.balance, 0, "a lapsed release freeze reopens the forged path");
    }

    /// @notice Still a RELEASE-guest bug: refunds do not run the release guest (a refund is a Merkle path to the
    ///         frozen deposit root, checked by the vault itself), and the per-token release freeze stops releases
    ///         but not refunds. Freezing every token's releases instead of releases globally keeps the forged path
    ///         shut and lets d2 be refunded to its depositor; b1, which only the buggy guest can prove, stays on v1.
    function test_guestBug_tokenReleaseFreezesKeepReleasesShutWhileD2IsRefunded() public {
        uint64 d2 = _guestBugWorld();
        vm.startPrank(freezer);
        vault.setTokenReleasesFrozen(ETH, true); // every registered token's, in a real deployment
        vault.setCheckpointsFrozen(true);
        vm.stopPrank();

        FerrataVault.ReleaseRequest memory forged = _request(ETH, 1, eve, ROOT1, keccak256("a forged nullifier"));
        _proveRelease(_releaseJournal(forged));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(forged, SEAL);

        uint256 bobBefore = bob.balance;
        vault.refund(d2, bob, ETH, leafData[d2].denomIndex, leafData[d2].commitment, _path(d2));
        assertEq(bob.balance - bobBefore, 0.1 ether + 0.1 ether / 500, "d2 refunded to its depositor, whole escrow");
        assertTrue(vault.refunded(d2));

        FerrataVault.ReleaseRequest memory b1 = _request(ETH, 1, alice, ROOT1, keccak256("b1's nullifier"));
        _proveRelease(_releaseJournal(b1));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(b1, SEAL);
        assertEq(eve.balance, 0);
    }

    /// @notice Still a RELEASE-guest bug, found before the sunset checkpoint was final. With checkpoints frozen the
    ///         sunset checkpoint can be neither submitted nor finalised, so d2 is not refundable. The checkpoint guest
    ///         is sound in this case, so the freeze authority lifts the checkpoint freeze for it: the sunset
    ///         checkpoint goes through its wait (in which a watcher could cancel it) and is finalised, checkpoints
    ///         are frozen again, and once its revocation window has passed d2 is refunded to its depositor. Every
    ///         registered token's release freeze, set in place of the global one, keeps the forged path and b1 shut
    ///         throughout, the sunset checkpoint's own root included.
    function test_guestBug_releaseGuest_sunsetNotYetFinal_checkpointFreezeLiftedForIt() public {
        _dep(alice, 1, keccak256("d1: imported; burned on LEZ as b1"));
        uint64 d2 = _dep(bob, 0, keccak256("d2: never imported"));
        _acceptRoot(ROOT1); // b1's burn root, finalised by an honest checkpoint

        // the bug is announced: releases and checkpoints freeze at once, then deposits, and the vault is retired;
        // then every registered token's releases are frozen in place of the global release freeze (these first)
        vm.startPrank(freezer);
        vault.setReleasesFrozen(true);
        vault.setCheckpointsFrozen(true);
        vault.setDepositsFrozen(true);
        vault.retire();
        vault.setTokenReleasesFrozen(ETH, true);
        vault.setTokenReleasesFrozen(address(usdc), true);
        vault.setTokenReleasesFrozen(address(dai), true);
        vault.setTokenReleasesFrozen(address(usdt), true);
        vault.setReleasesFrozen(false);
        vm.stopPrank();

        FerrataVault.ReleaseRequest memory forged = _request(ETH, 1, eve, ROOT1, keccak256("a forged nullifier"));
        _proveRelease(_releaseJournal(forged));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(forged, SEAL);
        FerrataVault.ReleaseRequest memory forgedUsdc =
            _request(address(usdc), 0, eve, ROOT1, keccak256("a forged nullifier, another token"));
        _proveRelease(_releaseJournal(forgedUsdc));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(forgedUsdc, SEAL);

        // no sunset checkpoint yet: d2 is not refundable, and the sunset checkpoint cannot be submitted
        bytes32[32] memory p2 = _path(d2); // built first: the precompile must not take the expectRevert
        vm.expectRevert(FerrataVault.NotRefundable.selector);
        vault.refund(d2, bob, ETH, leafData[d2].denomIndex, leafData[d2].commitment, p2);
        FerrataVault.Checkpoint memory sunset = _nextCheckpoint(vault, ROOT0, 1, true);
        _proveCheckpoint(_checkpointJournal(sunset));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(sunset, SEAL);

        // the checkpoint guest is sound: the checkpoint freeze is lifted, the sunset checkpoint submitted, and
        // finalised after its wait; then checkpoints are frozen again
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
        vault.submitCandidate(sunset, SEAL);
        _finalizeAfterDelta(sunset);
        assertTrue(vault.sunsetLeafCountSet(), "the sunset checkpoint is final");
        assertEq(uint256(vault.sunsetLeafCount()), 1);
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);

        // d2 waits for the sunset checkpoint's revocation window, then is refunded to its depositor, whole escrow
        vm.expectRevert(FerrataVault.RefundNotYetOpen.selector);
        vault.refund(d2, bob, ETH, leafData[d2].denomIndex, leafData[d2].commitment, p2);
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);
        uint256 bobBefore = bob.balance;
        vault.refund(d2, bob, ETH, leafData[d2].denomIndex, leafData[d2].commitment, p2);
        assertEq(bob.balance - bobBefore, 0.1 ether + 0.1 ether / 500, "d2 refunded to its depositor, whole escrow");
        assertTrue(vault.refunded(d2));

        // and throughout no release could pay: not the forged one, not b1, not one on the sunset checkpoint's root
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(forged, SEAL);
        FerrataVault.ReleaseRequest memory b1 = _request(ETH, 1, alice, ROOT1, keccak256("b1's nullifier"));
        _proveRelease(_releaseJournal(b1));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(b1, SEAL);
        FerrataVault.ReleaseRequest memory onSunset = _request(ETH, 0, eve, ROOT0, keccak256("on the sunset's root"));
        _proveRelease(_releaseJournal(onSunset));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.TokenReleasesAreFrozen.selector);
        vault.release(onSunset, SEAL);
        assertEq(eve.balance, 0);
    }

    /// @notice A bug in the CHECKPOINT guest (a checkpoint carrying a burn root nobody burned into would verify,
    ///         and finalises). The freeze authority freezes checkpoints and releases and, within the revocation
    ///         window, revokes the wrong checkpoint and every later one. The forged root then pays nothing; b1's root,
    ///         finalised by an honest checkpoint before it, still does once releases are unfrozen; checkpoints stay
    ///         frozen (renewed) until v2 is live, since the guest that would make the next one is the buggy one.
    function test_guestBug_checkpointGuest_revokeTakesBackTheForgedRootAndB1StillReleases() public {
        _dep(alice, 1, keccak256("d1: imported; burned on LEZ as b1"));
        _dep(bob, 1, keccak256("another deposit, so escrow could cover a forged release"));
        _acceptRoot(ROOT1); // b1's root, honest
        uint256 forgedIndex = _acceptRoot(ROOT_FORGED); // what the buggy checkpoint guest let finalise

        vm.startPrank(freezer);
        vault.setCheckpointsFrozen(true);
        vault.setReleasesFrozen(true);
        vault.revokeFrom(forgedIndex);
        vm.stopPrank();
        (bool forgedAccepted,) = vault.burnRootStatus(ROOT_FORGED);
        assertFalse(forgedAccepted, "the forged root no longer counts");

        // the honest checkpoints are checked against the chain; then releases reopen, checkpoints stay frozen
        vm.prank(freezer);
        vault.setReleasesFrozen(false);
        FerrataVault.ReleaseRequest memory forged = _request(ETH, 1, eve, ROOT_FORGED, keccak256("a forged nullifier"));
        _proveRelease(_releaseJournal(forged));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(forged, SEAL);
        assertEq(eve.balance, 0);

        uint256 aliceBefore = alice.balance;
        _release(_request(ETH, 1, alice, ROOT1, keccak256("b1's nullifier")));
        assertGt(alice.balance, aliceBefore, "b1 released to the recipient its burn names");

        FerrataVault.Checkpoint memory next = _nextCheckpoint(keccak256("a root after the announcement"));
        _proveCheckpoint(_checkpointJournal(next));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(next, SEAL);
    }
}
