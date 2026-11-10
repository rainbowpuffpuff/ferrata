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
            (,,,, uint64 imported, bool sunset, bool revoked,,, uint256 finalizedAt) = vault.checkpoints(i);
            if (sunset && !revoked) return (true, imported, finalizedAt + vault.revokeWindow() + 1); // v3 D11
        }
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
///         and refund replays, direct donations, time, freezes and admin changes, and keeps ghost accounting for the
///         invariants. Calls the vault rejects simply revert the handler call, which also discards the ghost updates.
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

    FerrataVault internal immutable vault;
    MockRiscZeroVerifier internal immutable verifier;
    address internal immutable admin;
    /// The freeze authority: changes when an admin proposal that names one is applied (v4, D13).
    address public freezer;
    address internal immutable relayer;
    HandlerCodec internal immutable _codec;

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
        address[] memory tokens_
    ) {
        vault = vault_;
        verifier = verifier_;
        admin = admin_;
        freezer = freezer_;
        relayer = makeAddr("handler relayer");
        _codec = new HandlerCodec();
        _tokens = tokens_;
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

    /// @notice Every address the vault could owe: recipients, the relayer, depositors (refunds), the handler.
    function payees() external view returns (address[] memory list) {
        list = new address[](_recipients.length + _actors.length + 2);
        uint256 n;
        for (uint256 i = 0; i < _recipients.length; i++) {
            list[n++] = _recipients[i];
        }
        for (uint256 i = 0; i < _actors.length; i++) {
            list[n++] = _actors[i];
        }
        list[n++] = relayer;
        list[n++] = address(this);
    }

    // ---- deposits -------------------------------------------------------------------------------------------------

    function deposit(uint256 actorSeed, uint256 tokenSeed, uint256 denomSeed, bytes32 commitment) external {
        address actor = _actors[actorSeed % _actors.length];
        address token = _tokens[tokenSeed % _tokens.length];
        uint256[] memory denoms = vault.denominations(token);
        // forge-lint: disable-next-line(unsafe-typecast)
        uint8 denomIndex = uint8(denomSeed % denoms.length); // < 8: a token has at most 8 denominations
        uint256 amount = denoms[denomIndex];
        if (commitment == bytes32(0)) commitment = bytes32(uint256(1));
        uint64 index = vault.depositCount(actor);
        uint64 leafIndex = vault.nextLeafIndex();

        if (token == ETH) {
            vm.deal(actor, amount);
            vm.prank(actor);
            vault.depositETH{value: amount}(denomIndex, commitment, index);
        } else {
            MockTokenBase(token).mint(actor, amount);
            vm.prank(actor);
            MockTokenBase(token).approve(address(vault), amount);
            vm.prank(actor);
            vault.deposit(token, denomIndex, commitment, index);
        }

        ghostDeposited[token] += amount;
        ghostDepositCount++;
        _leaves.push(ReferenceMerkle.leafHash(leafIndex, actor, token, amount, commitment));
        _leafData.push(LeafData(actor, token, denomIndex, commitment));
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
        uint256 amount = vault.denominations(l.token)[l.denomIndex];
        bool allowed = found && leafIndex >= boundary && block.timestamp >= opensAt && _retired
            && block.timestamp >= _releasesFrozenUntil && _releaseCapsAllow(l.token, amount); // v4 (D13): caps
        bool payable_ = allowed && !_leafRefunded[leafIndex]
            && ghostDeposited[l.token] - ghostReleased[l.token] - ghostRefunded[l.token] >= amount;
        bytes32[32] memory path = _codec.path(_leaves, leafIndex);
        try vault.refund(leafIndex, l.depositor, l.token, l.denomIndex, l.commitment, path) {
            if (!allowed) ghostRefundsAgainstTheRules++;
        } catch {
            if (payable_) {
                ghostRefundsWronglyRefused++;
                return;
            }
            revert RefundRefusedAsExpected();
        }
        ghostRefunded[l.token] += amount;
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
            ghostRefunded[l.token] += vault.denominations(l.token)[l.denomIndex];
        } catch {}
    }

    // ---- releases and owed claims ---------------------------------------------------------------------------------

    function release(uint256 tokenSeed, uint256 denomSeed, uint256 rootSeed, uint256 recipientSeed, bool withFee)
        external
    {
        vm.assume(_acceptedRoots.length != 0);
        address token = _tokens[tokenSeed % _tokens.length];
        uint8 denomIndex = _affordableDenomination(token, denomSeed);
        FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: denomIndex,
            recipient: _recipients[recipientSeed % _recipients.length],
            relayer: (recipientSeed >> 128) % 2 == 0 ? address(0) : _recipients[(recipientSeed >> 64) % _recipients.length],
            relayFee: withFee ? vault.relayFees(token)[denomIndex] : 0,
            burnRoot: _acceptedRoots[rootSeed % _acceptedRoots.length],
            nullifier: keccak256(abi.encode("nullifier", _nonce++))
        });
        uint256 amount = vault.denominations(token)[denomIndex];
        verifier.accept(vault.releaseImageId(), _codec.releaseDigest(vault, r, amount));

        vm.prank(relayer);
        vault.release(r, "");

        ghostReleased[token] += amount;
        ghostReleaseCount++;
        _usedNullifiers.push(r.nullifier);
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
        uint256 amount = vault.denominations(token)[denomIndex];
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
        uint256 largest = vault.denominations(token)[2];
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
        uint256[] memory denoms = vault.denominations(token);
        uint256 escrow = vault.escrowed(token);
        // forge-lint: disable-next-line(unsafe-typecast)
        uint8 start = uint8(seed % denoms.length); // < 8: a token has at most 8 denominations
        for (uint8 k = 0; k < denoms.length; k++) {
            uint8 i = uint8((start + k) % denoms.length);
            if (denoms[i] <= escrow) return i;
        }
        return start;
    }
}
