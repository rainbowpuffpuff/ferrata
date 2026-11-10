// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {CommonBase} from "forge-std/Base.sol";
import {StdCheats} from "forge-std/StdCheats.sol";
import {StdUtils} from "forge-std/StdUtils.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {ToggleRecipient} from "../mocks/MockRecipients.sol";
import {MockTokenBase} from "../mocks/MockTokens.sol";
import {MockRiscZeroVerifier} from "../mocks/MockVerifiers.sol";
import {ReferenceMerkle} from "../utils/ReferenceMerkle.sol";

/// @notice Journal digests, rebuilt word by word from the spec rather than with the vault's code, and Merkle paths.
///         A contract of its own, so that the handler stays under the EIP-170 size limit.
contract HandlerCodec {
    /// @dev The 17-word checkpoint journal of v3 D3.
    function checkpointDigest(FerrataVault vault, FerrataVault.Checkpoint calldata c) external view returns (bytes32) {
        bytes memory head = bytes.concat(
            sha256(bytes("ferrata/vault/checkpoint/v3")),
            bytes32(block.chainid),
            bytes32(uint256(uint160(address(vault)))),
            vault.lezConfigId(),
            bytes32(uint256(c.nMin)),
            bytes32(uint256(c.tMin))
        );
        bytes memory body = bytes.concat(
            c.parentState,
            c.newState,
            c.burnRoot,
            bytes32(uint256(c.burnCount)),
            bytes32(uint256(c.importedLeafCount)),
            bytes32(uint256(c.sunset ? 1 : 0)),
            c.echoNonce
        );
        bytes memory tail =
            bytes.concat(bytes32(uint256(c.tipId)), c.tipHash, bytes32(uint256(c.n)), bytes32(uint256(c.t)));
        return sha256(bytes.concat(head, body, tail));
    }

    /// @dev The 11-word release journal of spec §5.2.
    function releaseDigest(FerrataVault vault, FerrataVault.ReleaseRequest calldata r, uint256 amount)
        external
        view
        returns (bytes32)
    {
        bytes memory context = bytes.concat(
            sha256(bytes("ferrata/vault/release/v2")),
            bytes32(block.chainid),
            bytes32(uint256(uint160(address(vault)))),
            vault.lezConfigId(),
            r.burnRoot,
            r.nullifier
        );
        bytes memory payout = bytes.concat(
            bytes32(uint256(uint160(r.token))),
            bytes32(amount),
            bytes32(uint256(uint160(r.recipient))),
            bytes32(uint256(uint160(r.relayer))),
            bytes32(r.relayFee)
        );
        return sha256(bytes.concat(context, payout));
    }

    /// @dev The first valid sunset checkpoint's imported leaf count, and when refunds open: the second after its last
    ///      revocable one.
    function refundBoundary(FerrataVault vault) external view returns (bool found, uint64 boundary, uint256 opensAt) {
        uint256 count = vault.checkpointCount();
        for (uint256 i = 0; i < count; i++) {
            (,,,, uint64 imported, bool sunset, bool revoked,,, uint64 finalizedAt,) = vault.checkpoints(i);
            if (sunset && !revoked) return (true, imported, finalizedAt + vault.revokeWindow() + 1); // v3 D11
        }
    }

    /// @dev The imported leaf count of the newest valid checkpoint that can no longer be revoked: every later valid
    ///      count, so every refund boundary, is at least this, so a leaf below it can never be refunded and LEZ may
    ///      mint it (the model's claims stay below it).
    function settledImported(FerrataVault vault) external view returns (uint64) {
        uint256 count = vault.checkpointCount();
        for (uint256 i = count; i > 0; i--) {
            (,,,, uint64 imported,, bool revoked,,, uint64 finalizedAt,) = vault.checkpoints(i - 1);
            if (!revoked && block.timestamp > finalizedAt + vault.revokeWindow()) return imported;
        }
        return 0;
    }

    /// @dev The checkpoint fee's payee for releases against `burnRoot`: the finaliser of its checkpoint.
    function finalizerOf(FerrataVault vault, bytes32 burnRoot) external view returns (address finalizer) {
        uint256 entry = vault.burnRootCheckpoint(burnRoot);
        if (entry != 0) (,,,,,,,,,, finalizer) = vault.checkpoints(entry - 1);
    }

    /// @dev bridge-wire `fee_slot`: sha256("ferrata/vault/fees/v1" || token || index:u64).
    function feeSlot(address token, uint256 index) external pure returns (bytes32) {
        return sha256(bytes.concat(bytes("ferrata/vault/fees/v1"), bytes20(token), bytes8(uint64(index))));
    }

    /// @dev bridge-wire `FeeSchedule::hash`, built byte by byte from its definition.
    function scheduleHash(
        address token,
        uint256 index,
        uint64[3] calldata times, // fromLeaf, announcedAt, effectiveAt
        uint256[] calldata mintFees,
        uint256[] calldata burnFees,
        uint256[] calldata checkpointFees
    ) external pure returns (bytes32) {
        bytes memory m = bytes.concat(
            bytes("ferrata/fees/v1"),
            bytes20(token),
            bytes8(uint64(index)),
            bytes8(times[0]),
            bytes8(times[1]),
            bytes8(times[2]),
            bytes1(uint8(mintFees.length))
        );
        for (uint256 i = 0; i < mintFees.length; i++) {
            m = bytes.concat(m, bytes32(mintFees[i]));
        }
        for (uint256 i = 0; i < burnFees.length; i++) {
            m = bytes.concat(m, bytes32(burnFees[i]));
        }
        for (uint256 i = 0; i < checkpointFees.length; i++) {
            m = bytes.concat(m, bytes32(checkpointFees[i]));
        }
        return sha256(m);
    }

    function path(bytes32[] calldata leaves, uint64 leafIndex) external pure returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, leafIndex);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }
}

/// @notice Drives the vault through random sequences of deposits, checkpoint candidates (four committee sizes),
///         finalisations, cancellations, attempts to finalise checkpoints that break monotonicity or were cancelled,
///         revocations, retirement, refunds, releases (some to payees that cannot receive), owed claims, nullifier
///         and refund replays, direct donations, time, freezes and admin changes, fee announcements (v5, D13), and
///         keeps ghost accounting for the invariants, including a model of the LEZ side: claims mint what the attested
///         fee schedule of the leaf says, burns take one denomination out of the wrapped supply, releases pay burns. Calls the vault rejects simply revert the handler call, which also discards the ghost updates.
///         Calls that have nothing to do are discarded (vm.assume), so in the run table "calls - reverts - discards"
///         of finalizePending, cancelPending, finalizeRejected, revoke, refund and release counts exactly the
///         finalisations, cancellations, refusals observed, revocations, refunds and releases.
contract VaultHandler is CommonBase, StdCheats, StdUtils {
    /// The vault refused a rejected candidate for another reason than the expected one (e.g. a stale parent).
    error RefusedForAnotherReason();
    /// The vault refused a refund the handler's records do not allow either.
    error RefundRefusedAsExpected();

    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;

    struct LeafData {
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }

    /// A fee schedule as this handler expects it (v5, D13); `effectiveAt` 0 in `_pending` means none announced.
    struct Schedule {
        uint64 fromLeaf;
        uint64 announcedAt;
        uint64 effectiveAt;
        uint256[] mintFees;
        uint256[] burnFees;
        uint256[] checkpointFees;
    }

    /// A burn on LEZ (modelled) waiting for its release.
    struct Burn {
        address token;
        uint8 denomIndex;
        bytes32 nullifier;
    }

    FerrataVault internal immutable vault;
    MockRiscZeroVerifier internal immutable verifier;
    address internal immutable admin;
    /// The freeze authority: changes when an admin proposal that names one is applied (v4, D13).
    address public freezer;
    address internal immutable relayer;
    address internal immutable feeRecipient;
    HandlerCodec internal immutable _codec;

    // ---- v5 (D13): the fee model and the LEZ supply model
    mapping(address token => Schedule[]) internal _schedules; // in force, by index
    mapping(address token => Schedule) internal _pending;
    mapping(uint64 leafIndex => uint256) internal _escrowOf; // what each deposit escrowed (measured)
    mapping(uint64 leafIndex => bool) internal _claimed;
    Burn[] internal _pendingBurns;
    /// Wrapped supply on LEZ, the escrow of deposits neither claimed nor refunded, burns not yet released.
    mapping(address token => uint256) public ghostSupply;
    mapping(address token => uint256) public ghostUnminted;
    mapping(address token => uint256) public ghostUnreleased;
    /// A deposit, release or refund that moved other amounts than the fee model says.
    uint256 public ghostFeeMismatches;
    /// A published schedule hash that is not the model's preimage, or a slot published (or not) against the model.
    uint256 public ghostPublishedMismatches;
    /// A refund of a leaf the model claimed on LEZ (a double payout).
    uint256 public ghostRefundsOfClaimedLeaves;
    uint256 internal _claimCount;
    uint256 internal _burnCount;
    uint256 internal _feeChanges;

    address[] internal _tokens;
    address[] internal _actors;
    address[] internal _recipients; // EOAs and two contracts that accept ETH only when open
    ToggleRecipient[] internal _toggles;

    mapping(address token => uint256) public ghostDeposited;
    mapping(address token => uint256) public ghostReleased;
    mapping(address token => uint256) public ghostRefunded;
    mapping(address token => uint256) public ghostDonated;
    uint256 public ghostDepositCount;
    uint256 public ghostReleaseCount;
    uint256 public ghostDoubleSpends;
    uint256 public ghostDoubleRefunds;
    /// Finalisations of candidates that break monotonicity (v3 D4) or were cancelled, accepted although they must
    /// not be.
    uint256 public ghostRejectedFinalisations;
    /// Refunds paid although the handler's own records say they were not allowed (v3 D5), and refunds refused
    /// although those records say they were allowed and payable.
    uint256 public ghostRefundsAgainstTheRules;
    uint256 public ghostRefundsWronglyRefused;
    /// Candidates submitted or finalised while the handler's records say checkpoints were frozen (v3 D4).
    uint256 public ghostCheckpointCallsDuringFreeze;
    mapping(bytes32 state => bool) public ghostWentThroughFinalize;

    /// Counts for the run summary only (see `summary`).
    uint256 internal _refundCount;
    uint256 internal _finalizeCount;
    uint256 internal _sunsetFinalizeCount;
    uint256 internal _cancelCount;
    uint256 internal _monotonicityRefusals;
    uint256 internal _cancelledRefusals;
    uint256 internal _revokeCount;
    uint256 internal _replaysStoppedByNullifier;

    bool internal _retired;
    uint256 internal _releasesFrozenUntil;
    uint256 internal _checkpointsFrozenUntil;

    // v4 (D13) roles, as this handler expects them to be.
    address[3] internal _watcherModel;
    address[4] internal _watcherPool;
    address[2] internal _freezerPool;
    address[3] internal _proposedWatchers;
    address internal _proposedFreezer;
    uint256 internal _proposedAt; // 0: nothing proposed
    uint256 public ghostRoleTimelockViolations; // a role change applied before its 14 days, or refused when due
    uint256 public ghostEarlyFinalisations; // a finalisation before the candidate's wait
    uint256 internal _watcherPauses;
    uint256 internal _roleChanges;

    bytes32[] internal _leaves;
    LeafData[] internal _leafData;
    bytes32[] internal _acceptedRoots;
    mapping(bytes32 root => bool) internal _knownRoot;
    bytes32[] internal _usedNullifiers;
    uint64[] internal _refundedLeaves;
    mapping(uint64 leafIndex => bool) internal _leafRefunded;

    /// Two candidate slots: the pending one, which respects monotonicity, and one the vault must never finalise
    /// (it breaks monotonicity, or it was cancelled). `_pendingSlot` says which slot is which.
    FerrataVault.Checkpoint[2] internal _slot;
    bool[2] internal _filled;
    uint256[2] internal _readyAt; // submission time + W, computed here
    uint256 internal _pendingSlot;
    bool internal _rejectedWasCancelled;
    uint256 internal _nonce;
    uint64 internal _checkpointNonce;

    constructor(
        FerrataVault vault_,
        MockRiscZeroVerifier verifier_,
        address admin_,
        address freezer_,
        address[] memory tokens_,
        uint64 registeredAt
    ) {
        vault = vault_;
        verifier = verifier_;
        admin = admin_;
        freezer = freezer_;
        relayer = makeAddr("handler relayer");
        feeRecipient = vault_.feeRecipient();
        _codec = new HandlerCodec();
        _tokens = tokens_;
        // Fee schedule 0 of each token: registration's (no mint or burn fee), in force from leaf 0.
        for (uint256 t = 0; t < tokens_.length; t++) {
            (,,,, uint256[] memory denoms) = vault_.tokenConfig(tokens_[t]);
            Schedule storage s0 = _schedules[tokens_[t]].push();
            (s0.fromLeaf, s0.announcedAt, s0.effectiveAt) = (0, registeredAt, registeredAt);
            for (uint8 i = 0; i < denoms.length; i++) {
                (,,, uint256 checkpointFee) = vault_.fees(tokens_[t], 0, i);
                s0.mintFees.push(0);
                s0.burnFees.push(0);
                s0.checkpointFees.push(checkpointFee);
            }
        }
        for (uint256 i = 0; i < 3; i++) {
            _actors.push(makeAddr(string.concat("depositor ", vm.toString(i))));
            _recipients.push(makeAddr(string.concat("recipient ", vm.toString(i))));
        }
        for (uint256 i = 0; i < 2; i++) {
            ToggleRecipient toggle = new ToggleRecipient();
            _toggles.push(toggle);
            _recipients.push(address(toggle));
        }
        for (uint256 i = 0; i < 3; i++) {
            _watcherModel[i] = vault_.watchers(i);
        }
        _watcherPool = [_watcherModel[0], makeAddr("watcher pool 1"), makeAddr("watcher pool 2"), makeAddr("pool 3")];
        _freezerPool = [makeAddr("freezer pool 0"), makeAddr("freezer pool 1")];
    }

    function watcherModel(uint256 i) external view returns (address) {
        return _watcherModel[i];
    }

    function checkpointsFrozenUntilModel() external view returns (uint256) {
        return _checkpointsFrozenUntil;
    }

    /// @notice This run's watcher pauses and applied role changes.
    function roleSummary() external view returns (uint256, uint256) {
        return (_watcherPauses, _roleChanges);
    }

    // ---- views for the invariant contract -------------------------------------------------------------------------

    function tokens() external view returns (address[] memory) {
        return _tokens;
    }

    function leaves() external view returns (bytes32[] memory) {
        return _leaves;
    }

    function usedNullifiers() external view returns (bytes32[] memory) {
        return _usedNullifiers;
    }

    function refundedLeaves() external view returns (uint64[] memory) {
        return _refundedLeaves;
    }

    /// @notice This run's refunds, finalisations, sunset finalisations, cancellations, refusals for monotonicity,
    ///         refusals of cancelled candidates, revocations, and replays stopped by the nullifier check.
    function summary() external view returns (uint256[8] memory) {
        return [
            _refundCount,
            _finalizeCount,
            _sunsetFinalizeCount,
            _cancelCount,
            _monotonicityRefusals,
            _cancelledRefusals,
            _revokeCount,
            _replaysStoppedByNullifier
        ];
    }

    /// @notice Every address the vault could owe: recipients, the relayer, depositors (refunds), the handler (the
    ///         finaliser, credited checkpoint fees) and the fee recipient (burn fees).
    function payees() external view returns (address[] memory list) {
        list = new address[](_recipients.length + _actors.length + 3);
        uint256 n;
        for (uint256 i = 0; i < _recipients.length; i++) {
            list[n++] = _recipients[i];
        }
        for (uint256 i = 0; i < _actors.length; i++) {
            list[n++] = _actors[i];
        }
        list[n++] = relayer;
        list[n++] = address(this);
        list[n++] = feeRecipient;
    }

    /// @notice This run's claims and burns (modelled on LEZ) and fee changes put in force.
    function feeSummary() external view returns (uint256, uint256, uint256) {
        return (_claimCount, _burnCount, _feeChanges);
    }

    // ---- deposits -------------------------------------------------------------------------------------------------

    function deposit(uint256 actorSeed, uint256 tokenSeed, uint256 denomSeed, bytes32 commitment) external {
        address actor = _actors[actorSeed % _actors.length];
        address token = _tokens[tokenSeed % _tokens.length];
        // forge-lint: disable-next-line(unsafe-typecast)
        uint8 denomIndex = uint8(denomSeed % _denoms(token).length); // < 8: a token has at most 8 denominations
        if (commitment == bytes32(0)) commitment = bytes32(uint256(1));
        uint64 leafIndex = vault.nextLeafIndex();
        // The model's fees: a due change is in force first (D13), then the denomination, mint and relay fee.
        _applyDue(token);
        uint256 total = _denoms(token)[denomIndex] + _current(token).mintFees[denomIndex] + _relayFee(token, denomIndex);
        uint256 escrowed = _callDeposit(actor, token, denomIndex, commitment, total);
        if (escrowed == type(uint256).max) return; // refused for a different amount: counted
        if (escrowed != total) ghostFeeMismatches++;
        _escrowOf[leafIndex] = escrowed;
        ghostDeposited[token] += escrowed;
        ghostUnminted[token] += escrowed;
        ghostDepositCount++;
        _leaves.push(ReferenceMerkle.leafHash(leafIndex, actor, token, _denoms(token)[denomIndex], commitment));
        _leafData.push(LeafData(actor, token, denomIndex, commitment));
    }

    /// @dev Funds `actor` with `total`, approves it for a token, deposits, and returns what the vault's balance
    ///      gained. A refusal of the vault's own (freeze, retirement, caps, delisting) discards the call; any other
    ///      (a different amount expected: a wrong ETH value or a short allowance) is a fee mismatch, and returns
    ///      type(uint256).max.
    function _callDeposit(address actor, address token, uint8 denomIndex, bytes32 commitment, uint256 total)
        internal
        returns (uint256)
    {
        uint64 index = vault.depositCount(actor);
        uint256 before = _balanceOf(token, address(vault));
        bytes memory err;
        bool ok;
        if (token == ETH) {
            vm.deal(actor, total);
            vm.prank(actor);
            try vault.depositETH{value: total}(denomIndex, commitment, index) {
                ok = true;
            } catch (bytes memory e) {
                err = e;
            }
        } else {
            MockTokenBase(token).mint(actor, total);
            vm.prank(actor);
            MockTokenBase(token).approve(address(vault), total);
            vm.prank(actor);
            try vault.deposit(token, denomIndex, commitment, index) {
                ok = true;
            } catch (bytes memory e) {
                err = e;
            }
        }
        if (ok) return _balanceOf(token, address(vault)) - before;
        if (_isDepositRefusal(err)) revert RefusedForAnotherReason();
        ghostFeeMismatches++;
        return type(uint256).max;
    }

    // ---- checkpoints ----------------------------------------------------------------------------------------------

    /// @dev A candidate on top of `finalState` that respects monotonicity (v3 D4), with one of four committees;
    ///      occasionally a sunset checkpoint.
    function submitCandidate(uint256 rootSeed, uint64 importSeed, bool sunsetCoin, uint256 committeeSeed) external {
        (uint64 imported, bool sunset) = _monotoneCountAndSunset(importSeed, sunsetCoin);
        _submitInto(_pendingSlot, _candidate(rootSeed, imported, sunset, committeeSeed));
    }

    /// @dev Finalises the pending candidate: waits out its W = delta + (t - 1) * turnSeconds (plus up to an hour),
    ///      records a beacon and echoes it.
    function finalizePending(uint256 extraWait) external {
        uint256 slot = _pendingSlot;
        vm.assume(_filled[slot]);
        (FerrataVault.Checkpoint memory c, uint256 beaconIndex) = _readyToFinalize(slot, extraWait);
        bool frozen = block.timestamp < _checkpointsFrozenUntil;
        if (vault.mode() == FerrataVault.Mode.B) vault.finalizeAttested(c);
        else vault.finalize(beaconIndex, c, "");
        if (frozen) ghostCheckpointCallsDuringFreeze++;

        _filled[slot] = false;
        _finalizeCount++;
        if (c.sunset) _sunsetFinalizeCount++;
        ghostWentThroughFinalize[c.newState] = true;
        if (!_knownRoot[c.burnRoot]) {
            _knownRoot[c.burnRoot] = true;
            _acceptedRoots.push(c.burnRoot);
        }
    }

    /// @dev A candidate that breaks monotonicity (v3 D4) against the current state: a lower imported leaf count, or
    ///      after a sunset checkpoint a non-sunset one or another count. Submission does not check monotonicity.
    function submitBreakingCandidate(uint256 rootSeed, uint64 seed, uint256 committeeSeed) external {
        uint64 floor = vault.finalImportedLeafCount();
        bool sunsetNow = vault.finalSunset();
        vm.assume(floor != 0 || sunsetNow); // from a count of 0 without a sunset nothing can break it
        uint64 imported;
        bool sunset;
        if (!sunsetNow) {
            (imported, sunset) = (seed % floor, seed % 3 == 0); // lower
        } else if (seed % 3 == 0) {
            (imported, sunset) = (floor, false); // the sunset flag dropped
        } else if (seed % 3 == 1 || floor == 0) {
            (imported, sunset) = (floor + 1 + seed % 4, true); // more imports after the sunset
        } else {
            (imported, sunset) = (seed % floor, true); // fewer
        }
        _submitInto(1 - _pendingSlot, _candidate(rootSeed, imported, sunset, committeeSeed));
        _rejectedWasCancelled = false;
    }

    /// @dev The freeze authority cancels the pending candidate (reject-only), in one call out of four so that most
    ///      candidates still finalise. It becomes the candidate the vault must never finalise, which finalizeRejected
    ///      tries anyway after its wait.
    function cancelPending(uint256 seed) external {
        uint256 slot = _pendingSlot;
        vm.assume(_filled[slot] && seed % 4 == 0);
        bytes32 newState = _slot[slot].newState;
        // v4 (D13): a watcher or the freeze authority.
        address canceller = (seed >> 8) % 2 == 0 ? freezer : _watcherModel[(seed >> 16) % 3];
        vm.assume(canceller != address(0));
        vm.prank(canceller);
        vault.cancelCandidate(newState);
        _cancelCount++;
        _pendingSlot = 1 - slot;
        _filled[1 - slot] = false;
        _rejectedWasCancelled = true;
    }

    /// @dev Tries to finalise the rejected candidate after its full wait, with a valid proof. The vault must refuse it
    ///      (for monotonicity, or as unknown once cancelled): that refusal returns normally and is counted; any other
    ///      failure (stale parent, freeze) reverts.
    function finalizeRejected(uint256 extraWait) external {
        uint256 slot = 1 - _pendingSlot;
        vm.assume(_filled[slot]);
        (FerrataVault.Checkpoint memory c, uint256 beaconIndex) = _readyToFinalize(slot, extraWait);
        try this.finalizeAs(c, beaconIndex) {
            ghostRejectedFinalisations++;
        } catch (bytes memory err) {
            // Only the selector is wanted; a shorter reason pads with zeros and matches no error.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 reason = bytes4(err);
            if (_rejectedWasCancelled) {
                if (reason != FerrataVault.UnknownCandidate.selector) revert RefusedForAnotherReason();
                _cancelledRefusals++;
            } else {
                if (
                    reason != FerrataVault.ImportedLeafCountDecreased.selector
                        && reason != FerrataVault.SunsetIsFinal.selector
                ) revert RefusedForAnotherReason();
                _monotonicityRefusals++;
            }
            _filled[slot] = false;
        }
    }

    function revoke(uint256 depthSeed) external {
        uint256 count = vault.checkpointCount();
        vm.assume(count != 0);
        uint256 index = count - 1 - (depthSeed % (count < 3 ? count : 3));
        vm.prank(freezer);
        vault.revokeFrom(index);
        _revokeCount++;
    }

    /// @dev Retirement ends deposits for good, so it comes only after a few deposits. Refunds need it (v3 D5).
    function retire(uint256 seed) external {
        vm.assume(!_retired && ghostDepositCount >= 3 && seed % 3 == 0);
        vm.prank(freezer);
        vault.retire();
        _retired = true;
    }

    // ---- refunds ----------------------------------------------------------------------------------------------------

    /// @dev Prefers leaves at or above the refund boundary and can wait for refunds to open. Whether the vault should
    ///      pay is judged first from the handler's own records: a refund paid against them is counted, and so is one
    ///      refused although they allow it, the leaf was never refunded and its token's escrow covers it.
    function refund(uint64 leafSeed, bool waitForOpening) external {
        uint64 leafCount = vault.nextLeafIndex();
        vm.assume(leafCount != 0);
        uint64 leafIndex = leafSeed % leafCount;
        (bool found, uint64 boundary, uint256 opensAt) = _codec.refundBoundary(vault);
        if (found) {
            if (boundary < leafCount && leafSeed % 4 != 0) leafIndex = boundary + leafSeed % (leafCount - boundary);
            if (waitForOpening && block.timestamp < opensAt) vm.warp(opensAt);
        }
        LeafData memory l = _leafData[leafIndex];
        uint256 amount = _escrowOf[leafIndex]; // v5: a refund returns the leaf's whole escrow, fees included
        bool allowed = found && leafIndex >= boundary && block.timestamp >= opensAt && _retired
            && block.timestamp >= _releasesFrozenUntil && _releaseCapsAllow(l.token, amount); // v4 (D13): caps
        bool payable_ = allowed && !_leafRefunded[leafIndex]
            && ghostDeposited[l.token] - ghostReleased[l.token] - ghostRefunded[l.token] >= amount;
        bytes32[32] memory path = _codec.path(_leaves, leafIndex);
        uint256 before = _valueOf(l.token, l.depositor);
        try vault.refund(leafIndex, l.depositor, l.token, l.denomIndex, l.commitment, path) {
            if (!allowed) ghostRefundsAgainstTheRules++;
            if (_valueOf(l.token, l.depositor) - before != amount) ghostFeeMismatches++;
        } catch {
            if (payable_) {
                ghostRefundsWronglyRefused++;
                return;
            }
            revert RefundRefusedAsExpected();
        }
        ghostRefunded[l.token] += amount;
        if (_claimed[leafIndex]) ghostRefundsOfClaimedLeaves++;
        else ghostUnminted[l.token] -= amount;
        _refundCount++;
        _refundedLeaves.push(leafIndex);
        _leafRefunded[leafIndex] = true;
    }

    /// @dev Tries to refund an already refunded leaf again.
    function replayRefund(uint256 which) external {
        if (_refundedLeaves.length == 0) return;
        uint64 leafIndex = _refundedLeaves[which % _refundedLeaves.length];
        LeafData memory l = _leafData[leafIndex];
        bytes32[32] memory path = _codec.path(_leaves, leafIndex);
        try vault.refund(leafIndex, l.depositor, l.token, l.denomIndex, l.commitment, path) {
            ghostDoubleRefunds++;
            ghostRefunded[l.token] += _escrowOf[leafIndex];
        } catch {}
    }

    // ---- releases and owed claims ---------------------------------------------------------------------------------

    /// @dev Releases a burn of the LEZ model against an accepted root, and checks every payout against the fee model:
    ///      the recipient gets the denomination less the relay fee it names and the burn and checkpoint fees in force,
    ///      the fee recipient is credited the burn fee and the root's finaliser the checkpoint fee (D13).
    function release(uint256 burnSeed, uint256 rootSeed, uint256 recipientSeed, bool withFee) external {
        vm.assume(_acceptedRoots.length != 0 && _pendingBurns.length != 0);
        uint256 which = burnSeed % _pendingBurns.length;
        Burn memory b = _pendingBurns[which];
        FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
            token: b.token,
            denomIndex: b.denomIndex,
            recipient: _recipients[recipientSeed % _recipients.length],
            relayer: (recipientSeed >> 128) % 2 == 0 ? address(0) : _recipients[(recipientSeed >> 64) % _recipients.length],
            relayFee: withFee ? _relayFee(b.token, b.denomIndex) : 0,
            burnRoot: _acceptedRoots[rootSeed % _acceptedRoots.length],
            nullifier: b.nullifier
        });
        uint256 amount = _denoms(b.token)[b.denomIndex];
        verifier.accept(vault.releaseImageId(), _codec.releaseDigest(vault, r, amount));
        _applyDue(b.token);
        uint256[3] memory expected = _expectedReleasePayouts(r, amount); // recipient, fee recipient, finaliser
        address finalizer = _codec.finalizerOf(vault, r.burnRoot);
        uint256[3] memory before =
            [_valueOf(b.token, r.recipient), vault.owed(feeRecipient, b.token), vault.owed(finalizer, b.token)];

        vm.prank(relayer);
        vault.release(r, "");

        if (
            _valueOf(b.token, r.recipient) - before[0] != expected[0]
                || vault.owed(feeRecipient, b.token) - before[1] != expected[1]
                || vault.owed(finalizer, b.token) - before[2] != expected[2]
        ) ghostFeeMismatches++;
        ghostReleased[b.token] += amount;
        ghostUnreleased[b.token] -= amount;
        ghostReleaseCount++;
        _usedNullifiers.push(r.nullifier);
        _pendingBurns[which] = _pendingBurns[_pendingBurns.length - 1];
        _pendingBurns.pop();
    }

    /// @dev LEZ model: claims a deposit below the settled import count (so never refundable), minting what LEZ mints:
    ///      the denomination, the mint fee of the schedule of its leaf index and the relay fee. Checks first that the
    ///      vault publishes that schedule's hash at its slot and that the next slot is published only if the model has
    ///      that schedule in force (what LEZ's Register and ImportFees attest).
    function claim(uint256 leafSeed) external {
        uint64 settled = _codec.settledImported(vault);
        vm.assume(settled != 0);
        uint64 leafIndex = uint64(leafSeed % settled);
        vm.assume(!_claimed[leafIndex] && !_leafRefunded[leafIndex]);
        LeafData memory l = _leafData[leafIndex];
        Schedule[] storage list = _schedules[l.token];
        uint256 index = list.length - 1;
        while (list[index].fromLeaf > leafIndex) index--;
        Schedule storage s = list[index];
        bytes32 published = vm.load(address(vault), _codec.feeSlot(l.token, index));
        bytes32 next = vm.load(address(vault), _codec.feeSlot(l.token, index + 1));
        if (published != _scheduleHash(l.token, index, s) || (next != bytes32(0)) != (index + 1 < list.length)) {
            ghostPublishedMismatches++;
        }
        uint256 minted = _denoms(l.token)[l.denomIndex] + s.mintFees[l.denomIndex] + _relayFee(l.token, l.denomIndex);
        ghostSupply[l.token] += minted;
        ghostUnminted[l.token] -= _escrowOf[leafIndex];
        _claimed[leafIndex] = true;
        _claimCount++;
    }

    /// @dev LEZ model: a holder burns one denomination of the wrapped supply (the first token and denomination from
    ///      the seeds that the supply covers).
    function burn(uint256 tokenSeed, uint256 denomSeed) external {
        for (uint256 k = 0; k < _tokens.length; k++) {
            address token = _tokens[(tokenSeed + k) % _tokens.length];
            uint256[] memory denoms = _denoms(token);
            for (uint256 j = 0; j < denoms.length; j++) {
                // forge-lint: disable-next-line(unsafe-typecast)
                uint8 denomIndex = uint8((denomSeed + j) % denoms.length);
                if (ghostSupply[token] < denoms[denomIndex]) continue;
                ghostSupply[token] -= denoms[denomIndex];
                ghostUnreleased[token] += denoms[denomIndex];
                _pendingBurns.push(Burn(token, denomIndex, keccak256(abi.encode("burn", _nonce++))));
                _burnCount++;
                return;
            }
        }
        vm.assume(false); // nothing to burn yet
    }

    /// @dev The admin announces a fee schedule: random fees up to the 1% ceiling (mint fees in whole LEZ units), or,
    ///      one time in five, the fees in force, which withdraws a pending announcement.
    function announceFees(uint256 tokenSeed, uint256 seed) public {
        address token = _tokens[tokenSeed % _tokens.length];
        uint256[] memory denoms = _denoms(token);
        (,, uint256 lezUnit,,) = vault.tokenConfig(token);
        _applyDue(token);
        Schedule storage current = _current(token);
        uint256[][3] memory f;
        bool withdraw = seed % 5 == 0;
        for (uint256 j = 0; j < 3; j++) {
            f[j] = new uint256[](denoms.length);
        }
        for (uint256 i = 0; i < denoms.length; i++) {
            uint256 ceiling = denoms[i] / 100;
            f[0][i] = withdraw
                ? current.mintFees[i]
                : _bound(uint256(keccak256(abi.encode(seed, i, 0))), 0, ceiling / lezUnit) * lezUnit;
            f[1][i] = withdraw ? current.burnFees[i] : _bound(uint256(keccak256(abi.encode(seed, i, 1))), 0, ceiling);
            f[2][i] =
                withdraw ? current.checkpointFees[i] : _bound(uint256(keccak256(abi.encode(seed, i, 2))), 0, ceiling);
        }
        vm.prank(admin);
        vault.announceFees(token, f[0], f[1], f[2]);
        Schedule storage p = _pending[token];
        (p.fromLeaf, p.announcedAt, p.effectiveAt) = (0, uint64(block.timestamp), uint64(block.timestamp + 7 days));
        p.mintFees = f[0];
        p.burnFees = f[1];
        p.checkpointFees = f[2];
        if (withdraw) p.effectiveAt = 0;
    }

    /// @dev Anyone puts a due fee change in force.
    function applyFees(uint256 tokenSeed) external {
        address token = _tokens[tokenSeed % _tokens.length];
        _applyDue(token);
        vault.applyFees(token);
    }

    /// @dev Tries to spend an already used nullifier with a fresh, otherwise valid proof (new recipient).
    function replayNullifier(uint256 which, uint256 recipientSeed) external {
        if (_usedNullifiers.length == 0 || _acceptedRoots.length == 0) return;
        address token = _tokens[recipientSeed % _tokens.length];
        uint8 denomIndex = _affordableDenomination(token, recipientSeed >> 8);
        FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: denomIndex,
            recipient: address(uint160(uint256(keccak256(abi.encode("replay recipient", recipientSeed % 1000))))),
            relayer: address(0),
            relayFee: 0,
            burnRoot: _acceptedRoots[recipientSeed % _acceptedRoots.length],
            nullifier: _usedNullifiers[which % _usedNullifiers.length]
        });
        uint256 amount = _denoms(token)[denomIndex];
        verifier.accept(vault.releaseImageId(), _codec.releaseDigest(vault, r, amount));

        vm.prank(relayer);
        try vault.release(r, "") {
            ghostDoubleSpends++;
            ghostReleased[token] += amount; // keep escrow accounting exact so only the double-spend check fires
        } catch (bytes memory err) {
            // Replays can also fail earlier (freeze, delay, revoked root); count the ones the nullifier check stopped.
            // forge-lint: disable-next-line(unsafe-typecast)
            if (bytes4(err) == FerrataVault.NullifierAlreadyUsed.selector) _replaysStoppedByNullifier++;
        }
    }

    /// @dev Opens or closes one of the payee contracts, and lets it claim what it is owed.
    function togglePayee(uint256 which, bool open, uint256 tokenSeed) external {
        ToggleRecipient toggle = _toggles[which % _toggles.length];
        toggle.setOpen(open);
        address token = _tokens[tokenSeed % _tokens.length];
        if (open && vault.owed(address(toggle), token) != 0) toggle.claim(address(vault), token);
    }

    // ---- environment and roles --------------------------------------------------------------------------------------

    /// @dev Tokens or ETH arriving outside `deposit` (direct transfer, forced ETH) are not escrow.
    function donate(uint256 tokenSeed, uint256 amount) external {
        address token = _tokens[tokenSeed % _tokens.length];
        amount = _bound(amount, 0, 1e24);
        if (token == ETH) {
            // As a real donation arrives: a contract created with the ETH self-destructs to the vault, skipping its
            // reverting receive(). (v4: setting the vault's balance with vm.deal was lost once a later call made the
            // vault DELEGATECALL its library; forge 1.3.6, seen in the trace of a failing run.)
            vm.deal(address(this), address(this).balance + amount);
            bytes memory initcode = abi.encodePacked(hex"73", address(vault), hex"ff"); // PUSH20 vault; SELFDESTRUCT
            address created;
            assembly ("memory-safe") {
                created := create(amount, add(initcode, 32), mload(initcode))
            }
            require(created != address(0), "donation failed");
        } else {
            MockTokenBase(token).mint(address(vault), amount);
        }
        ghostDonated[token] += amount;
    }

    function warp(uint256 secondsForward) external {
        vm.warp(block.timestamp + _bound(secondsForward, 0, 3 days));
        vm.roll(block.number + 1);
    }

    function setFreeze(uint256 which, uint256 tokenSeed, uint256 valueSeed) external {
        bool frozen = valueSeed % 4 == 0; // unfreezing is three times as likely, so activity continues
        address token = _tokens[tokenSeed % _tokens.length];
        uint256 until = frozen ? block.timestamp + vault.maxFreeze() : 0; // read before the prank
        vm.startPrank(freezer);
        uint256 kind = which % 5;
        if (kind == 0) vault.setDepositsFrozen(frozen);
        else if (kind == 1) vault.setReleasesFrozen(frozen);
        else if (kind == 2) vault.setTokenDepositsFrozen(token, frozen);
        else if (kind == 3) vault.setTokenReleasesFrozen(token, frozen);
        else vault.setCheckpointsFrozen(frozen);
        vm.stopPrank();
        if (kind == 1) _releasesFrozenUntil = until;
        if (kind == 4) _checkpointsFrozenUntil = until;
    }

    function setCap(uint256 tokenSeed, bool isDeposit, bool isGlobal, uint256 capacitySeed, uint256 refillSeed)
        external
    {
        address token = _tokens[tokenSeed % _tokens.length];
        uint256 largest = _denoms(token)[2];
        uint256 capacity;
        uint256 choice = capacitySeed % 8;
        if (choice == 0) capacity = UNLIMITED;
        else if (choice == 1) capacity = 0; // closed, rarely

        else capacity = _bound(capacitySeed, largest, largest * 20);
        uint256 refill = _bound(refillSeed, 0, largest / 60);
        FerrataVault.Direction direction = isDeposit ? FerrataVault.Direction.Deposit : FerrataVault.Direction.Release;

        if (isGlobal) {
            // Value units: scale a token-unit capacity by that token's weight per lezUnit (all small numbers here).
            // Read before the prank: any call, even a view, would consume it.
            (,, uint256 lezUnit, uint256 weight,) = vault.tokenConfig(token);
            uint256 scaledCapacity = capacity == UNLIMITED ? UNLIMITED : capacity * weight / lezUnit;
            vm.prank(admin);
            vault.setGlobalCap(direction, scaledCapacity, refill * weight / lezUnit);
        } else {
            vm.prank(admin);
            vault.setTokenCap(token, direction, capacity, refill);
        }
    }

    function setReleaseDelay(uint256 tokenSeed, uint256 delay) external {
        address token = _tokens[tokenSeed % _tokens.length];
        vm.prank(admin);
        vault.setReleaseDelay(token, _bound(delay, 0, 2 hours));
    }

    // ---- v4: rule (B), early finalisation, roles (D13) ---------------------------------------------------------------

    /// @dev Finalises under the vault's rule; external so that a refusal can be caught.
    function finalizeAs(FerrataVault.Checkpoint memory c, uint256 beaconIndex) external {
        require(msg.sender == address(this), "internal");
        if (vault.mode() == FerrataVault.Mode.B) vault.finalizeAttested(c);
        else vault.finalize(beaconIndex, c, "");
    }

    /// @dev Tries to finalise the pending candidate before its wait is over, with a valid proof under (A) (a beacon
    ///      recorded now); the vault must refuse. A success is counted as a violation.
    function finalizeEarly(uint256 seed) external {
        uint256 slot = _pendingSlot;
        vm.assume(_filled[slot] && block.timestamp < _readyAt[slot] && seed % 2 == 0);
        FerrataVault.Checkpoint memory c = _slot[slot];
        uint256 beaconIndex;
        if (vault.mode() == FerrataVault.Mode.A) {
            vm.roll(block.number + 1);
            beaconIndex = vault.recordBeacon();
            (, c.echoNonce) = vault.beacons(beaconIndex);
            verifier.accept(vault.checkpointImageId(), _codec.checkpointDigest(vault, c));
        }
        try this.finalizeAs(c, beaconIndex) {
            ghostEarlyFinalisations++;
        } catch {}
    }

    /// @dev A watcher pauses checkpoints until 12 h from now, never shortening a longer freeze.
    function watcherPause(uint256 seed) external {
        address w = _watcherModel[seed % 3];
        vm.assume(w != address(0) && (seed >> 8) % 4 == 0);
        vm.prank(w);
        vault.pauseCheckpoints();
        uint256 until = block.timestamp + 12 hours;
        if (until > _checkpointsFrozenUntil) _checkpointsFrozenUntil = until;
        _watcherPauses++;
    }

    /// @dev The admin proposes a watcher set from a small pool and, one time in three, a new freeze authority.
    function proposeRoles(uint256 seed) external {
        address[3] memory ws;
        for (uint256 i = 0; i < 3; i++) {
            uint256 pick = (seed >> (8 * i)) % 5;
            ws[i] = pick == 4 ? address(0) : _watcherPool[pick];
        }
        address newFreezer = seed % 3 == 0 ? _freezerPool[(seed >> 32) % 2] : address(0);
        vm.prank(admin);
        vault.proposeRoles(ws, newFreezer);
        _proposedWatchers = ws;
        _proposedFreezer = newFreezer;
        _proposedAt = block.timestamp;
    }

    /// @dev Applies the proposal, one time in four without waiting for its 14 days: an early success, or a refusal
    ///      once due, is counted.
    function applyRoles(uint256 waitSeed) external {
        vm.assume(_proposedAt != 0);
        if (waitSeed % 4 != 0 && block.timestamp < _proposedAt + 14 days) vm.warp(_proposedAt + 14 days);
        bool early = block.timestamp < _proposedAt + 14 days;
        try vault.applyRoles() {
            if (early) ghostRoleTimelockViolations++;
        } catch {
            if (!early) ghostRoleTimelockViolations++;
            return;
        }
        _watcherModel = _proposedWatchers;
        if (_proposedFreezer != address(0)) freezer = _proposedFreezer;
        _proposedAt = 0;
        _roleChanges++;
    }

    // ---- helpers ----------------------------------------------------------------------------------------------------

    /// @dev Whether the token's and the global release buckets have room for `amount` now (refunds draw on them).
    function _releaseCapsAllow(address token, uint256 amount) internal view returns (bool) {
        (,, uint256 tokenAvailable) = vault.tokenCap(token, FerrataVault.Direction.Release);
        (,, uint256 globalAvailable) = vault.globalCap(FerrataVault.Direction.Release);
        (,, uint256 lezUnit, uint256 unitWeight,) = vault.tokenConfig(token);
        return tokenAvailable >= amount && globalAvailable >= amount * unitWeight / lezUnit;
    }

    /// @dev Respects monotonicity (v3 D4): the count lies between the last finalised one and the leaf count, and
    ///      after a sunset checkpoint it stays fixed and every checkpoint is a sunset one. Before retirement one
    ///      candidate in eight is a sunset checkpoint, after it three in four, as in the migration procedure (§4.5).
    function _monotoneCountAndSunset(uint64 seed, bool sunsetCoin) internal view returns (uint64, bool) {
        uint64 leafCount = vault.nextLeafIndex(); // <= 2**32, so no overflow below
        uint64 floor = vault.finalImportedLeafCount(); // <= leafCount: no count is ever drawn above the leaf count
        if (vault.finalSunset()) return (floor, true);
        bool sunset = _retired ? (seed >> 32) % 4 != 0 : sunsetCoin && seed % 4 == 0;
        return (floor + seed % (leafCount - floor + 1), sunset);
    }

    /// @dev A candidate on `finalState` with a fresh state, tip and burn count, and its proof registered.
    function _candidate(uint256 rootSeed, uint64 imported, bool sunset, uint256 committeeSeed)
        internal
        returns (FerrataVault.Checkpoint memory c)
    {
        _checkpointNonce += 1;
        (uint32 n, uint32 t) = _committee(committeeSeed);
        c = FerrataVault.Checkpoint({
            nMin: vault.nMin(),
            tMin: vault.tMin(),
            parentState: vault.finalState(),
            newState: keccak256(abi.encode("handler state", _checkpointNonce)),
            burnRoot: keccak256(abi.encode("burn root", rootSeed % 6)), // a small set, so roots repeat
            burnCount: _checkpointNonce,
            importedLeafCount: imported,
            sunset: sunset,
            echoNonce: bytes32(0),
            tipId: 1000 + _checkpointNonce,
            tipHash: keccak256(abi.encode("handler tip", _checkpointNonce)),
            n: n,
            t: t
        });
        verifier.accept(vault.checkpointImageId(), _codec.checkpointDigest(vault, c));
    }

    /// @dev Key-set sizes n with LEZ's threshold t(n) = max(1, min(ceil(2n/3), n - 1)), all at or above the floors.
    function _committee(uint256 seed) internal pure returns (uint32 n, uint32 t) {
        uint256 k = seed % 4;
        if (k == 0) return (3, 2);
        if (k == 1) return (4, 3);
        if (k == 2) return (7, 5);
        return (10, 7);
    }

    /// @dev Submits `c` and keeps it in `slot`, with the time its wait W = delta + (t - 1) * turnSeconds ends.
    function _submitInto(uint256 slot, FerrataVault.Checkpoint memory c) internal {
        uint256 readyAt = block.timestamp + vault.delta() + (uint256(c.t) - 1) * vault.turnSeconds();
        bool frozen = block.timestamp < _checkpointsFrozenUntil;
        vault.submitCandidate(c, "");
        if (frozen) ghostCheckpointCallsDuringFreeze++;
        _slot[slot] = c;
        _filled[slot] = true;
        _readyAt[slot] = readyAt;
    }

    /// @dev Waits until the slot's candidate may finalise (plus up to an hour), records a beacon in a new block and
    ///      registers the proof of the finalisation journal echoing it.
    function _readyToFinalize(uint256 slot, uint256 extraWait)
        internal
        returns (FerrataVault.Checkpoint memory c, uint256 beaconIndex)
    {
        c = _slot[slot];
        uint256 readyAt = _readyAt[slot] + _bound(extraWait, 0, 1 hours);
        if (block.timestamp < readyAt) vm.warp(readyAt);
        vm.roll(block.number + 1);
        if (vault.mode() == FerrataVault.Mode.B) return (c, 0); // rule (B): the candidate's own journal, no beacon
        beaconIndex = vault.recordBeacon();
        (, c.echoNonce) = vault.beacons(beaconIndex);
        verifier.accept(vault.checkpointImageId(), _codec.checkpointDigest(vault, c));
    }

    /// @dev Prefers a denomination the escrow can cover, so that releases often get past the escrow check.
    function _affordableDenomination(address token, uint256 seed) internal view returns (uint8) {
        uint256[] memory denoms = _denoms(token);
        uint256 escrow = vault.escrowed(token);
        // forge-lint: disable-next-line(unsafe-typecast)
        uint8 start = uint8(seed % denoms.length); // < 8: a token has at most 8 denominations
        for (uint8 k = 0; k < denoms.length; k++) {
            uint8 i = uint8((start + k) % denoms.length);
            if (denoms[i] <= escrow) return i;
        }
        return start;
    }

    // ---- v5 helpers -------------------------------------------------------------------------------------------------

    function _denoms(address token) internal view returns (uint256[] memory denoms) {
        (,,,, denoms) = vault.tokenConfig(token);
    }

    function _relayFee(address token, uint8 denomIndex) internal view returns (uint256 relayFee) {
        (, relayFee,,) = vault.fees(token, 0, denomIndex);
    }

    function _current(address token) internal view returns (Schedule storage) {
        return _schedules[token][_schedules[token].length - 1];
    }

    /// @dev The model's side of a due fee change: in force from the current leaf count, at this block's time.
    function _applyDue(address token) internal {
        Schedule storage p = _pending[token];
        if (p.effectiveAt == 0 || block.timestamp < p.effectiveAt) return;
        Schedule storage s = _schedules[token].push();
        (s.fromLeaf, s.announcedAt, s.effectiveAt) = (vault.nextLeafIndex(), p.announcedAt, uint64(block.timestamp));
        s.mintFees = p.mintFees;
        s.burnFees = p.burnFees;
        s.checkpointFees = p.checkpointFees;
        p.effectiveAt = 0;
        _feeChanges++;
    }

    function _scheduleHash(address token, uint256 index, Schedule storage s) internal view returns (bytes32) {
        return _codec.scheduleHash(
            token, index, [s.fromLeaf, s.announcedAt, s.effectiveAt], s.mintFees, s.burnFees, s.checkpointFees
        );
    }

    /// @dev What a release pays the recipient (less the relay fee unless the recipient also relays) and credits the
    ///      fee recipient and the finaliser, under the schedule in force.
    function _expectedReleasePayouts(FerrataVault.ReleaseRequest memory r, uint256 amount)
        internal
        view
        returns (uint256[3] memory e)
    {
        Schedule storage s = _current(r.token);
        uint256 burnFee = s.burnFees[r.denomIndex];
        uint256 checkpointFee = s.checkpointFees[r.denomIndex];
        address feePayee = r.relayer == address(0) ? relayer : r.relayer;
        e[0] = amount - burnFee - checkpointFee - (feePayee == r.recipient ? 0 : r.relayFee);
        e[1] = burnFee;
        e[2] = checkpointFee;
    }

    /// @dev Balance plus what the vault owes: a payment either arrives or is credited.
    function _valueOf(address token, address who) internal view returns (uint256) {
        return _balanceOf(token, who) + vault.owed(who, token);
    }

    function _balanceOf(address token, address who) internal view returns (uint256) {
        return token == ETH ? who.balance : MockTokenBase(token).balanceOf(who);
    }

    function _isDepositRefusal(bytes memory err) internal pure returns (bool) {
        bytes4 e = bytes4(err);
        return e == FerrataVault.VaultRetired.selector || e == FerrataVault.DepositsAreFrozen.selector
            || e == FerrataVault.TokenDepositsAreFrozen.selector || e == FerrataVault.DepositsDisabled.selector
            || e == FerrataVault.TokenDepositCapExceeded.selector || e == FerrataVault.GlobalDepositCapExceeded.selector;
    }
}
