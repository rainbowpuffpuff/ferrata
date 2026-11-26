// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {BrokenVerifierSetup} from "./invariant/BrokenVerifierInvariants.t.sol";
import {ToggleRecipient} from "./mocks/MockRecipients.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";

/// @title A broken verifier with no published proof of exploit, so the stop cannot be tripped (RFP-022 Security #9;
///        ADR 0033, `vault/MIGRATION.md` section 4's fourth case and section 5's last paragraph)
/// @notice The premise of every test here: the vault's verifier is unsound - the stand-in accepts every seal, so a
///         forged checkpoint or a forged release verifies exactly as an honest proof does - but RISC Zero has
///         published no proof of exploit, so `estop(receipt)` has no receipt to feed it and the emergency stop stays
///         untripped (`brokenStop.paused()` is asserted false throughout). That is the case of RISC Zero's advisory
///         GHSA-mm6c-35cm-g4x2 on the four Sepolia vaults. What the tests show is what the runbook of ADR 0033 holds,
///         and why no escrow-moving exit exists in it:
///
///         - the freeze authority's freezes (releases, globally, which holds refunds too, checkpoints, deposits)
///           hold a forged release against an accepted root and the finalisation of a forged candidate submitted
///           before the freeze, while the freezes are renewed; a lapse reopens both forged paths;
///         - the admin's zeroed caps (the only containment the Sepolia vaults had, their freeze authority holding no
///           key) refuse the forged release, a refund and new deposits;
///         - no role has a function that moves escrow to an address of its choice;
///         - the refund boundary is set by a finalised checkpoint, so under a broken verifier a forged sunset can
///           make a leaf LEZ imported and claimed refundable on Ethereum - the double spend that rules refunds out
///           as the exit, and why the runbook uses the global release freeze, not the per-token freezes the
///           guest-bug runbook uses (`MigrationFailsafe.t.sol`);
///         - an `owed` balance stays claimable by its own payee: the one thing that still moves.
contract FailsafeBrokenVerifierTest is BrokenVerifierSetup {
    bytes internal constant JUNK = hex"6a756e6b";
    bytes32 internal constant ROOT0 = keccak256("an honest burn root, finalised before the advisory");
    bytes32 internal constant ROOT1 = keccak256("a forged burn root, candidate when the advisory lands");

    address internal attacker = makeAddr("attacker"); // forges every checkpoint and release below
    uint256 internal forged; // makes every forged checkpoint's state, tip and burn count distinct

    struct Leaf {
        address who;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }
    Leaf[] internal leafData;
    bytes32[] internal leaves;

    function setUp() public override {
        super.setUp();
        _deployBroken();
    }

    // ------------------------------------------------------------------------------------------------------------
    // The world of the tests: honest deposits (their leaves tracked, so refund paths are real), forged checkpoints
    // ------------------------------------------------------------------------------------------------------------

    /// @dev An honest deposit, its leaf remembered for the refund paths.
    function _dep(address who, address token, uint8 denomIndex, bytes32 commitment) internal returns (uint64) {
        uint64 leafIndex = _depositInto(vault, who, token, denomIndex, commitment);
        leafData.push(Leaf({who: who, token: token, denomIndex: denomIndex, commitment: commitment}));
        leaves.push(_leafHash(leafIndex, who, token, _denomsOf(vault, token)[denomIndex], commitment));
        return leafIndex;
    }

    function _pathOf(uint64 i) internal view returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    /// @dev A forged checkpoint (a state, tip and burn root nobody proved) with a garbage seal.
    function _forgedStruct(bytes32 burnRoot, bool sunset, uint64 importedLeafCount)
        internal
        returns (FerrataVault.Checkpoint memory c)
    {
        forged++;
        c = FerrataVault.Checkpoint({
            nMin: N_MIN,
            tMin: T_MIN,
            parentState: vault.finalState(),
            newState: keccak256(abi.encode("forged state", forged)),
            burnRoot: burnRoot,
            burnCount: uint64(forged),
            importedLeafCount: importedLeafCount,
            sunset: sunset,
            echoNonce: bytes32(0),
            tipId: uint64(forged),
            tipHash: keccak256(abi.encode("forged tip", forged)),
            n: N_MIN,
            t: T_MIN
        });
    }

    /// @dev As above, submitted with a garbage seal the broken verifier accepts.
    function _forged(bytes32 burnRoot, bool sunset, uint64 importedLeafCount)
        internal
        returns (FerrataVault.Checkpoint memory c)
    {
        c = _forgedStruct(burnRoot, sunset, importedLeafCount);
        vm.prank(attacker);
        vault.submitCandidate(c, JUNK);
    }

    /// @dev Finalises forged candidate `c` at the first second its wait allows, under either rule, with a garbage
    ///      seal. Rule (A) needs a beacon recorded after the wait and its nonce echoed.
    function _forgeFinalize(FerrataVault.Checkpoint memory c) internal returns (uint256 finalizedAt) {
        (uint256 submittedAt, uint256 wait) = _waitOf(vault, c.newState);
        if (block.timestamp < submittedAt + wait) vm.warp(submittedAt + wait);
        if (vault.mode() == FerrataVault.Mode.A) {
            vm.roll(block.number + 1);
            uint256 beaconIndex = vault.recordBeacon();
            (, bytes32 nonce) = vault.beacons(beaconIndex);
            c.echoNonce = nonce;
            vm.prank(attacker);
            vault.finalize(beaconIndex, c, JUNK);
        } else {
            vm.prank(attacker);
            vault.finalizeAttested(c);
        }
        return block.timestamp;
    }

    /// @dev The same finalisation, expected to refuse with `reason` (the beacon is recorded before the expectation,
    ///      so it is the finalisation that must take it).
    function _forgeFinalizeRefused(FerrataVault.Checkpoint memory c, bytes4 reason) internal {
        (uint256 submittedAt, uint256 wait) = _waitOf(vault, c.newState);
        if (block.timestamp < submittedAt + wait) vm.warp(submittedAt + wait);
        if (vault.mode() == FerrataVault.Mode.A) {
            vm.roll(block.number + 1);
            uint256 beaconIndex = vault.recordBeacon();
            (, bytes32 nonce) = vault.beacons(beaconIndex);
            c.echoNonce = nonce;
            vm.prank(attacker);
            vm.expectRevert(reason);
            vault.finalize(beaconIndex, c, JUNK);
        } else {
            vm.prank(attacker);
            vm.expectRevert(reason);
            vault.finalizeAttested(c);
        }
    }

    /// @dev The runbook of ADR 0033, applied by the freeze authority: releases frozen globally (which holds refunds
    ///      too), checkpoints and deposits frozen, then the vault retired. The admin's caps are not set here.
    function _runbook() internal {
        vm.startPrank(freezer);
        vault.setReleasesFrozen(true);
        vault.setCheckpointsFrozen(true);
        vault.setDepositsFrozen(true);
        vault.retire();
        vm.stopPrank();
    }

    // ------------------------------------------------------------------------------------------------------------
    // The runbook's freezes
    // ------------------------------------------------------------------------------------------------------------

    /// @notice The freeze authority's freezes hold both forged paths while they are renewed, and a lapse reopens
    ///         them. A forged release against an already accepted root (whose release delay has passed) pays nobody
    ///         while the release freeze holds, whoever sends it, and b1 - the honest burn under the same root -
    ///         waits with it; the forged candidate submitted before the freeze cannot be finalised once its wait is
    ///         over, and no new one can be submitted; a deposit LEZ never imported is not refunded, the global
    ///         release freeze holding refunds too; no new deposit enters the retired vault. Each freeze lasts
    ///         `maxFreeze` (30 days) and is renewed by being set again; renewed one second before it would lapse,
    ///         both hold another 30 days. Left to lapse, both reopen: the forged release pays and the forged
    ///         checkpoint finalises, which is why the runbook renews each freeze until the vault is replaced. The
    ///         stop never trips: no proof of exploit exists.
    function test_brokenNoStop_theRunbookHoldsTheForgedCheckpointAndReleaseWhileRenewed() public {
        _dep(alice, ETH, 2, keccak256("escrow behind b1, never imported"));
        _dep(alice, ETH, 2, keccak256("escrow behind b1, never imported either"));
        _dep(alice, ETH, 2, keccak256("escrow the forged release would take"));
        _acceptRoot(ROOT0); // b1's root, finalised by an honest checkpoint before the advisory
        vm.warp(_finalizedAt(ROOT0) + REVOKE_WINDOW + 1); // ETH's release delay (the window) has passed

        // the forger's candidate, already submitted when the advisory lands: its wait runs during the freeze
        FerrataVault.Checkpoint memory pending = _forged(ROOT1, false, vault.finalImportedLeafCount());
        assertFalse(brokenStop.paused(), "no proof of exploit: the stop does not trip");

        _runbook();
        assertTrue(vault.retired());

        // a forged release against the accepted root pays nobody while the freeze holds, whoever sends it
        FerrataVault.ReleaseRequest memory forgedRelease =
            _request(ETH, 2, attacker, ROOT0, keccak256("a forged nullifier"));
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        // b1, the honest burn whose delay has passed, waits with it
        FerrataVault.ReleaseRequest memory b1 = _request(ETH, 2, alice, ROOT0, keccak256("b1's nullifier"));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(b1, JUNK);

        // the forged candidate cannot be finalised once its wait is over, and no new one can be submitted
        _forgeFinalizeRefused(pending, FerrataVault.CheckpointsAreFrozen.selector);
        FerrataVault.Checkpoint memory fresh = _forgedStruct(keccak256("a root proven after the advisory"), false, 0);
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(fresh, JUNK);

        // d2, a deposit LEZ never imported: not refunded - the global release freeze holds refunds too (the runbook
        // does not use the per-token release freezes of the guest-bug case, which would let refunds through; see
        // test_brokenNoStop_aForgedSunsetMakesAnImportedLeafRefundable for why)
        bytes32[32] memory path0 = _pathOf(0);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.refund(0, leafData[0].who, ETH, leafData[0].denomIndex, leafData[0].commitment, path0);

        // no new deposit enters the retired vault
        vm.deal(alice, 10.02 ether);
        uint64 aliceNext = vault.depositCount(alice);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: 10.02 ether}(2, keccak256("no more"), aliceNext);

        // renewal: each freeze is set again one second before it would lapse, and holds another 30 days
        vm.warp(block.timestamp + MAX_FREEZE - 1);
        vm.startPrank(freezer);
        vault.setReleasesFrozen(true);
        vault.setCheckpointsFrozen(true);
        vm.stopPrank();
        vm.warp(block.timestamp + MAX_FREEZE - 1);
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        _forgeFinalizeRefused(pending, FerrataVault.CheckpointsAreFrozen.selector);
        assertEq(attacker.balance, 0, "nothing paid while the freezes held");

        // left to lapse, both reopen: the forged release pays, and the forged checkpoint finalises
        vm.warp(block.timestamp + 2);
        vm.prank(attacker);
        vault.release(forgedRelease, JUNK);
        assertGt(attacker.balance, 0, "a lapsed release freeze reopens the forged path");
        _forgeFinalize(pending);
        (bool accepted,) = vault.burnRootStatus(ROOT1);
        assertTrue(accepted, "a lapsed checkpoint freeze lets the forged checkpoint finalise");
        assertFalse(brokenStop.paused(), "still no proof of exploit: the stop never tripped");
    }

    /// @notice The admin's step of the runbook - the only containment the four Sepolia vaults had, their freeze
    ///         authority holding no key: with the release caps set to zero, a forged release against an accepted root
    ///         whose delay has passed is refused, and so is a refund once a leaf has been made refundable; with the
    ///         deposit caps zero, no new deposit enters. The stop is not tripped throughout.
    function test_brokenNoStop_zeroedCapsRefuseForgedReleasesRefundsAndNewDeposits() public {
        _dep(alice, ETH, 2, keccak256("escrow the forged release would take"));
        _dep(alice, ETH, 2, keccak256("escrow a refund would return"));
        _dep(alice, ETH, 2, keccak256("escrow a refund would return"));
        uint256 finalizedAt = _forgeFinalize(_forged(ROOT0, false, 0)); // the forger's root, already final
        vm.warp(finalizedAt + REVOKE_WINDOW + 1); // ETH's release delay (the window) has passed

        // control: before the caps are set, the forged release pays
        vm.prank(attacker);
        vault.release(_request(ETH, 2, attacker, ROOT0, keccak256("the control nullifier")), JUNK);
        uint256 taken = attacker.balance;
        assertGt(taken, 0);

        // the admin zeroes the release caps (per token and globally; this test bed's immutable floor is zero) and
        // the deposit caps
        vm.startPrank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 0, 0);
        vault.setGlobalCap(FerrataVault.Direction.Release, 0, 0);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, 0, 0);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, 0, 0);
        vm.stopPrank();

        vm.prank(attacker);
        vm.expectRevert(FerrataVault.TokenReleaseCapExceeded.selector);
        vault.release(_request(ETH, 2, attacker, ROOT0, keccak256("a forged nullifier")), JUNK);

        // no new deposit enters either
        vm.deal(bob, 10.02 ether);
        vm.prank(bob);
        vm.expectRevert(FerrataVault.TokenDepositCapExceeded.selector);
        vault.depositETH{value: 10.02 ether}(2, keccak256("no more"), 0);

        // and a refund, once a leaf is refundable, is refused by the same caps: a forged sunset makes leaf 1
        // refundable (retired vault, revocation window passed), and the zeroed release bucket stops it
        vm.prank(freezer);
        vault.retire();
        _forgeFinalize(_forged(keccak256("a forged sunset root"), true, 1));
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);
        bytes32[32] memory path1 = _pathOf(1);
        vm.expectRevert(FerrataVault.TokenReleaseCapExceeded.selector);
        vault.refund(1, leafData[1].who, ETH, leafData[1].denomIndex, leafData[1].commitment, path1);
        assertFalse(vault.refunded(1));

        assertEq(attacker.balance, taken, "nothing more left the vault after the caps were zeroed");
        assertFalse(brokenStop.paused(), "the stop never tripped");
    }

    // ------------------------------------------------------------------------------------------------------------
    // Why refunds are not the exit: the boundary itself is set by a proof
    // ------------------------------------------------------------------------------------------------------------

    /// @notice A refund needs no proof (the vault checks the leaf's Merkle path itself), but the refund boundary -
    ///         `sunsetLeafCount`, the first leaf LEZ never imported - is set by a finalised checkpoint, and a
    ///         checkpoint's imported leaf count is proven by the broken verifier. A forged sunset carrying the count
    ///         of the last finalised checkpoint, a count LEZ has since passed, is monotone and finalises; after its
    ///         revocation window it makes a leaf LEZ *did* import and whose wrapped token was claimed refundable on
    ///         Ethereum: the depositor is paid a second time, and the loss lands on whoever holds the wrapped token.
    ///         Inside the window the freeze authority can still revoke it, which clears the boundary - the only
    ///         guard, and a race the runbook does not rely on.
    function test_brokenNoStop_aForgedSunsetMakesAnImportedLeafRefundable() public {
        // leaf 0: imported on LEZ when the last honest checkpoint was cut, and claimed there. leaf 1: imported on
        // LEZ since, and claimed: the wrapped token exists, but no finalised checkpoint carries a count above 1.
        _dep(alice, ETH, 1, keccak256("leaf 0, imported and claimed on LEZ"));
        _dep(bob, ETH, 1, keccak256("leaf 1, imported since the last checkpoint, claimed on LEZ"));
        FerrataVault.Checkpoint memory honest = _nextCheckpoint(vault, ROOT0, 1, false); // carries count 1
        _submit(honest);
        _finalizeAfterDelta(honest);
        assertEq(vault.finalImportedLeafCount(), 1);
        vm.prank(freezer);
        vault.retire();

        // the forged sunset carries the count the last checkpoint carried, not the count LEZ has imported since
        _forgeFinalize(_forged(ROOT1, true, 1));
        assertTrue(vault.sunsetLeafCountSet(), "the forged sunset is final");
        assertEq(uint256(vault.sunsetLeafCount()), 1, "and its boundary is the stale count");

        // inside the window the freeze authority can take it back, which clears the boundary: no refund then
        uint256 sunsetIndex = vault.sunsetSetAt();
        vm.prank(freezer);
        vault.revokeFrom(sunsetIndex);
        assertFalse(vault.sunsetLeafCountSet());
        bytes32[32] memory path1 = _pathOf(1);
        vm.expectRevert(FerrataVault.NotRefundable.selector);
        vault.refund(1, leafData[1].who, ETH, leafData[1].denomIndex, leafData[1].commitment, path1);

        // a second forged sunset, left alone for its revocation window: leaf 1, imported and claimed on LEZ, is
        // refundable on Ethereum and pays its depositor a second time
        _forgeFinalize(_forged(keccak256("a second forged sunset"), true, 1));
        assertEq(uint256(vault.sunsetLeafCount()), 1);
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);
        uint256 bobBefore = bob.balance;
        vm.prank(attacker); // permissionless: anyone can send the depositor's refund
        vault.refund(1, bob, ETH, leafData[1].denomIndex, leafData[1].commitment, path1);
        assertEq(bob.balance - bobBefore, 1 ether + 1 ether / 500, "the imported leaf's whole escrow, a second time");
        assertTrue(vault.refunded(1));

        // leaf 0 is below the boundary: not refundable, and not paid twice
        bytes32[32] memory path0 = _pathOf(0);
        vm.expectRevert(FerrataVault.NotRefundable.selector);
        vault.refund(0, alice, ETH, leafData[0].denomIndex, leafData[0].commitment, path0);
        assertFalse(brokenStop.paused(), "the stop never tripped: no proof of exploit");
    }

    // ------------------------------------------------------------------------------------------------------------
    // No role can move escrow to an address of its choice
    // ------------------------------------------------------------------------------------------------------------

    /// @dev Everything of value the vault and the actors hold, hashed for one comparison.
    function _valueState() internal view returns (bytes32) {
        bytes32 vaultSide =
            keccak256(abi.encode(address(vault).balance, dai.balanceOf(address(vault)), vault.escrowed(ETH)));
        bytes32 ethSide = keccak256(
            abi.encode(
                attacker.balance,
                admin.balance,
                freezer.balance,
                watcher.balance,
                alice.balance,
                bob.balance,
                relayer.balance,
                vault.escrowed(address(dai)),
                vault.totalOwed(ETH),
                vault.totalOwed(address(dai))
            )
        );
        bytes32 tokenSide = keccak256(
            abi.encode(
                dai.balanceOf(attacker),
                dai.balanceOf(admin),
                dai.balanceOf(freezer),
                dai.balanceOf(watcher),
                dai.balanceOf(alice),
                dai.balanceOf(bob)
            )
        );
        return keccak256(abi.encode(vaultSide, ethSide, tokenSide));
    }

    /// @notice Under the runbook, with the stop untripped, no role has a function that moves escrow to an address of
    ///         its choice: the forged release and the forged checkpoint are refused for every caller, a refund is
    ///         held by the global release freeze, `claimOwed` pays a payee only its own balance and only to itself,
    ///         every admin function moves nothing, and a raw send is refused. What the freeze authority can do is
    ///         lift a freeze, which reopens the permissionless forged path for whoever forges first - a lapse, not
    ///         a redirect to an address of its choice, and why the runbook renews the freezes and never lifts them
    ///         (test_brokenNoStop_theRunbookHoldsTheForgedCheckpointAndReleaseWhileRenewed shows what a lapse pays).
    function test_brokenNoStop_noRoleCanMoveEscrowToAnAddressOfItsChoice() public {
        _dep(alice, ETH, 1, keccak256("eth escrow behind b1"));
        _dep(bob, ETH, 0, keccak256("eth escrow, never imported"));
        _dep(bob, address(dai), 1, keccak256("dai escrow"));
        _acceptRoot(ROOT0);
        vm.warp(_finalizedAt(ROOT0) + REVOKE_WINDOW + 1);

        // an owed balance exists, so claimOwed is not vacuous, and a candidate to cancel
        ToggleRecipient payee = new ToggleRecipient(); // cannot receive while closed
        vm.prank(attacker);
        vault.release(_request(ETH, 1, address(payee), ROOT0, keccak256("the owed nullifier")), JUNK);
        assertEq(vault.owed(address(payee), ETH), 1 ether - 1 ether / 1000); // less the checkpoint fee (0.1%)
        FerrataVault.Checkpoint memory pending = _forged(keccak256("the pending candidate"), false, 0);

        _runbook();
        vm.startPrank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 0, 0);
        vault.setGlobalCap(FerrataVault.Direction.Release, 0, 0);
        vm.stopPrank();
        assertFalse(brokenStop.paused(), "the stop never tripped: no proof of exploit");

        vm.deal(attacker, 1 ether); // armed before the snapshot: the ether is the attacker's own
        bytes32 before = _valueState();

        // the forged release, sent by every role and by the attacker, refuses (the freeze first; the caps behind it)
        FerrataVault.ReleaseRequest memory forgedRelease =
            _request(ETH, 1, attacker, ROOT0, keccak256("a forged nullifier"));
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        vm.prank(watcher);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(forgedRelease, JUNK);
        assertEq(attacker.balance, 1 ether, "the refused sends came straight back");

        // a forged checkpoint cannot even be submitted
        FerrataVault.Checkpoint memory fresh = _forgedStruct(keccak256("a forged root"), false, 0);
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(fresh, JUNK);

        // a refund is held by the same global release freeze, whoever's leaf and whoever sends it
        bytes32[32] memory path1 = _pathOf(1);
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.refund(1, leafData[1].who, ETH, leafData[1].denomIndex, leafData[1].commitment, path1);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.refund(1, leafData[1].who, ETH, leafData[1].denomIndex, leafData[1].commitment, path1);

        // claimOwed: only a payee's own balance, claimed by the payee
        vm.startPrank(attacker);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(address(dai));
        vm.stopPrank();
        vm.prank(admin);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);

        // every admin function moves nothing
        MockERC20 token = new MockERC20("Failsafe Token", "FST", 18);
        FerrataVault.TokenParams memory params = _params(_denoms(1 ether, 2 ether, 4 ether), 1e12, 1);
        params.releaseDelay = MIN_DELAY; // this vault's floor (the default registrations have no delay setting)
        vm.startPrank(admin);
        vault.registerToken(address(token), params);
        uint256[] memory zeros = new uint256[](3);
        vault.announceFees(ETH, zeros, zeros, zeros);
        vault.setDepositsEnabled(ETH, false);
        vault.setDepositsEnabled(ETH, true);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vault.setGlobalCap(FerrataVault.Direction.Release, UNLIMITED, 0);
        vault.setReleaseDelay(ETH, 1 hours);
        vault.raiseDelta(5 hours);
        vm.stopPrank();

        // the freeze authority's reject-only powers move nothing either
        vm.startPrank(freezer);
        vault.setTokenReleasesFrozen(ETH, true);
        vault.setTokenReleasesFrozen(ETH, false);
        vault.cancelCandidate(pending.newState);
        vm.stopPrank();
        vm.prank(watcher);
        vault.pauseCheckpoints();
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);

        // raw sends are refused
        (bool ok,) = address(vault).call{value: 1 ether}("");
        assertFalse(ok, "no receive");
        (ok,) = address(vault).call(abi.encodeWithSignature("sweep(address)", attacker));
        assertFalse(ok, "no fallback sweep");

        assertEq(_valueState(), before, "no value moved while refusing");
        assertEq(vault.owed(address(payee), ETH), 1 ether - 1 ether / 1000, "the owed balance is still the payee's own");
    }

    // ------------------------------------------------------------------------------------------------------------
    // The one thing that still moves
    // ------------------------------------------------------------------------------------------------------------

    /// @notice An `owed` balance, credited before the freezes, is still claimable by its own payee, to itself, while
    ///         every freeze of the runbook stands, the vault is retired and the stop is untripped. (After a stop it
    ///         still moves: `MigrationFailsafe.t.sol`,
    ///         `test_failsafe_stopStrandsInflightPositionsExceptOwedBalances`.)
    function test_brokenNoStop_owedBalancesAreStillClaimableByTheirPayee() public {
        _dep(alice, ETH, 1, keccak256("escrow behind the owed balance"));
        _acceptRoot(ROOT0);
        vm.warp(_finalizedAt(ROOT0) + REVOKE_WINDOW + 1);
        ToggleRecipient payee = new ToggleRecipient(); // cannot receive while closed
        vm.prank(attacker);
        vault.release(_request(ETH, 1, address(payee), ROOT0, keccak256("the owed nullifier")), JUNK);
        assertEq(vault.owed(address(payee), ETH), 1 ether - 1 ether / 1000); // less the checkpoint fee (0.1%)

        _runbook();
        assertFalse(brokenStop.paused(), "the stop never tripped: no proof of exploit");

        // nobody else can claim the payee's balance, and the payee claims it once it can receive
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        assertEq(address(payee).balance, 1 ether - 1 ether / 1000, "the payee's own owed, to itself");
        // what stays owed is the release's checkpoint fee, credited to the forged checkpoint's prover
        assertEq(vault.totalOwed(ETH), 1 ether / 1000);
    }
}

/// @notice The same runbook and the same forged paths on a rule (B) vault (the rule the live vaults run): the wait is
///         deltaAtSubmit (1 h), with no beacon and no second proof.
contract FailsafeBrokenVerifierBTest is FailsafeBrokenVerifierTest {
    function _mode() internal pure override returns (FerrataVault.Mode) {
        return FerrataVault.Mode.B;
    }
}
