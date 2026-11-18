// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {MockTokenBase} from "../mocks/MockTokens.sol";
import {ReferenceMerkle} from "../utils/ReferenceMerkle.sol";

/// @notice The actor of BrokenVerifierInvariants. The vault's verifier accepts every seal (AcceptAllVerifier behind RISC
///         Zero's emergency stop), and this handler is an attacker who forges checkpoints with burn roots of its choice
///         and releases against them with fresh nullifiers, to its own address, at the earliest second the vault
///         allows. Honest depositors keep depositing; the freeze authority, a watcher and the stop's guardian
///         sometimes react (revoke, cancel, freeze, retire, stop). The handler keeps its own model of what the vault
///         must refuse, from its own records and the deployment's parameters, and counts every payout the model says
///         should not have happened, and every window in which payouts exceeded the release caps' bound.
contract BrokenVerifierHandler is Test {
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    bytes internal constant FORGED_SEAL = hex"f0f0f0f0deadbeef";
    uint256 internal constant MAX_WARP = 2 days;

    struct Fin {
        bytes32 burnRoot;
        uint64 submittedAt;
        uint64 wait;
        uint64 finalizedAt;
        bool revoked;
        bool sunset;
        uint64 imported;
    }

    struct Pending {
        FerrataVault.Checkpoint c;
        uint64 submittedAt;
        uint64 wait;
        bool live;
    }

    struct Pay {
        uint64 t;
        uint256 amount;
    }

    struct LeafData {
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
        uint256 escrow;
    }

    struct TokenModel {
        uint256 cap; // release bucket capacity (UNLIMITED: none)
        uint256 refill; // per second
        uint256 delay; // release delay of every checkpoint (no change in this suite)
        uint256 lezUnit;
        uint256 unitWeight;
        uint256[] denoms;
    }

    FerrataVault public immutable vault;
    RiscZeroVerifierEmergencyStop public immutable estop;
    address public immutable freezer;
    address public immutable watcher;
    address public immutable attacker;
    address public immutable attackerRelayer;
    uint256 public immutable revokeWindow;
    uint256 public immutable maxFreeze;
    uint256 public immutable delta;
    uint256 public immutable turnSeconds;
    uint256 public immutable globalCap;
    uint256 public immutable globalRefill;
    bool public immutable modeA;

    address[] internal _tokens;
    address[] internal _depositors;
    mapping(address token => TokenModel) internal _model;

    Fin[] internal _fins;
    Pending[] internal _pending;
    bytes32[] internal _leaves;
    LeafData[] internal _leafData;
    mapping(uint64 leafIndex => bool) internal _refunded;
    mapping(bytes32 nullifier => bool) internal _used;
    bytes32[] internal _usedList;
    mapping(address token => Pay[]) internal _pays;
    Pay[] internal _globalPays;
    uint256 internal _nonce;

    // The model's view of the reactions.
    bool public stopped;
    bool public retiredModel;
    uint256 public releasesFrozenUntilModel;
    mapping(address token => uint256) public tokenReleasesFrozenUntilModel;

    // Totals.
    mapping(address token => uint256) public deposited;
    mapping(address token => uint256) public released;
    mapping(address token => uint256) public refundedAmount;
    mapping(address token => uint256) public maxCapUseBps; // the fullest window, in basis points of its bound
    uint256 public maxGlobalCapUseBps;

    // Violations (every one must stay 0).
    uint256 public ghostReleaseUnaccepted;
    uint256 public ghostEarlyFinalisations;
    uint256 public ghostAfterStop;
    uint256 public ghostReleaseWhileFrozen;
    uint256 public ghostDoubleSpends;
    uint256 public ghostCapViolations;
    uint256 public ghostRefundsAgainstRules;
    uint256 public ghostAmountMismatches;
    uint256 public ghostRevocationsOutsideWindow;

    // Counts for the run summary.
    uint256 public depositCount;
    uint256 public forgedSubmitted;
    uint256 public forgedFinalised;
    uint256 public forgedSunsets;
    uint256 public cancellations;
    uint256 public revocations;
    uint256 public releaseCount;
    uint256 public refundCount;
    uint256 public stops;
    mapping(bytes4 selector => uint256) public refusals;

    constructor(
        FerrataVault vault_,
        RiscZeroVerifierEmergencyStop estop_,
        address freezer_,
        address watcher_,
        address[] memory tokens_,
        uint256[] memory caps,
        uint256[] memory refills,
        uint256[] memory delays
    ) {
        vault = vault_;
        estop = estop_;
        freezer = freezer_;
        watcher = watcher_;
        attacker = makeAddr("attacker");
        attackerRelayer = makeAddr("attacker's relayer");
        revokeWindow = vault_.revokeWindow();
        maxFreeze = vault_.maxFreeze();
        delta = vault_.delta();
        turnSeconds = vault_.turnSeconds();
        (uint256 gCap, uint256 gRefill,) = vault_.globalCap(FerrataVault.Direction.Release);
        globalCap = gCap;
        globalRefill = gRefill;
        modeA = vault_.mode() == FerrataVault.Mode.A;
        for (uint256 i = 0; i < tokens_.length; i++) {
            address t = tokens_[i];
            _tokens.push(t);
            TokenModel storage m = _model[t];
            m.cap = caps[i];
            m.refill = refills[i];
            m.delay = delays[i];
            (,, m.lezUnit, m.unitWeight, m.denoms) = vault_.tokenConfig(t);
        }
        for (uint256 i = 0; i < 4; i++) {
            _depositors.push(makeAddr(string(abi.encodePacked("depositor ", bytes1(uint8(48 + i))))));
        }
    }

    // ---- honest deposits ------------------------------------------------------------------------------------------

    function deposit(uint256 actorSeed, uint256 tokenSeed, uint256 denomSeed) external {
        address actor = _depositors[actorSeed % _depositors.length];
        address token = _tokens[tokenSeed % _tokens.length];
        // forge-lint: disable-next-line(unsafe-typecast)
        uint8 d = uint8(denomSeed % _model[token].denoms.length); // at most 8 denominations
        (uint256 mintFee, uint256 relayFee,,) = vault.fees(token, 0, d); // no fee change in this suite
        uint256 amount = _model[token].denoms[d];
        uint256 total = amount + mintFee + relayFee;
        uint64 index = vault.depositCount(actor);
        uint64 leafIndex = vault.nextLeafIndex();
        bytes32 commitment = keccak256(abi.encode("honest deposit", leafIndex));
        uint256 before = _balanceOf(token, address(vault));
        bool ok;
        if (token == ETH) {
            vm.deal(actor, total);
            vm.prank(actor);
            try vault.depositETH{value: total}(d, commitment, index) {
                ok = true;
            } catch {}
        } else {
            MockTokenBase(token).mint(actor, total);
            vm.prank(actor);
            MockTokenBase(token).approve(address(vault), total);
            vm.prank(actor);
            try vault.deposit(token, d, commitment, index) {
                ok = true;
            } catch {}
        }
        if (!ok) return; // retired, or a sunset checkpoint is final: deposits have ended
        uint256 got = _balanceOf(token, address(vault)) - before;
        deposited[token] += got;
        depositCount++;
        _leaves.push(ReferenceMerkle.leafHash(leafIndex, actor, token, amount, commitment));
        _leafData.push(LeafData(actor, token, d, commitment, got));
    }

    // ---- the attacker: forged checkpoints --------------------------------------------------------------------------

    /// @dev A forged candidate on `finalState` with a burn root of the attacker's choice (one of three it reuses, or a
    ///      fresh one), any imported leaf count the vault's monotonicity checks allow, sometimes a sunset; the seal is
    ///      garbage and the verifier accepts it.
    function forgeCandidate(uint256 rootSeed, uint256 importSeed, uint256 flags) external {
        bool finalSunset = vault.finalSunset();
        uint64 lo = vault.finalImportedLeafCount();
        uint64 imported = finalSunset ? lo : uint64(bound(importSeed, lo, vault.nextLeafIndex()));
        // forge-lint: disable-next-line(unsafe-typecast)
        uint32 t = vault.tMin() + uint32((flags >> 8) % 3);
        _nonce++;
        FerrataVault.Checkpoint memory c = FerrataVault.Checkpoint({
            nMin: vault.nMin(),
            tMin: vault.tMin(),
            parentState: vault.finalState(),
            newState: keccak256(abi.encode("forged state", _nonce)),
            burnRoot: _root(rootSeed),
            burnCount: uint64(_nonce),
            importedLeafCount: imported,
            sunset: finalSunset || flags % 5 == 2,
            echoNonce: bytes32(0),
            tipId: uint64(_nonce),
            tipHash: keccak256(abi.encode("forged tip", _nonce)),
            n: t + 1,
            t: t
        });
        vm.prank(attacker);
        try vault.submitCandidate(c, FORGED_SEAL) {
            if (stopped) ghostAfterStop++;
            Pending storage p = _pending.push();
            p.c = c;
            p.submittedAt = uint64(block.timestamp);
            p.wait = uint64(delta + (uint256(t) - 1) * turnSeconds);
            p.live = true;
            forgedSubmitted++;
        } catch (bytes memory e) {
            _refused(e);
        }
    }

    /// @dev The attacker finalises a forged candidate at the first second its wait allows: under rule (A) with a
    ///      beacon recorded then and a garbage "second proof" echoing it, under rule (B) with the candidate's journal.
    function finalizeForged(uint256 pick) external {
        if (_pending.length == 0) return;
        Pending storage p = _pending[pick % _pending.length];
        if (!p.live) return;
        uint256 due = uint256(p.submittedAt) + p.wait;
        if (block.timestamp < due) vm.warp(due);
        vm.roll(block.number + 1);
        FerrataVault.Checkpoint memory c = p.c;
        bool ok;
        if (modeA) {
            uint256 bi = vault.recordBeacon();
            (, bytes32 nonce) = vault.beacons(bi);
            c.echoNonce = nonce;
            vm.prank(attacker);
            try vault.finalize(bi, c, FORGED_SEAL) {
                ok = true;
            } catch (bytes memory e) {
                _refused(e);
            }
        } else {
            vm.prank(attacker);
            try vault.finalizeAttested(c) {
                ok = true;
            } catch (bytes memory e) {
                _refused(e);
            }
        }
        if (!ok) return;
        if (stopped) ghostAfterStop++;
        if (block.timestamp < due) ghostEarlyFinalisations++;
        _fins.push(
            Fin({
                burnRoot: c.burnRoot,
                submittedAt: p.submittedAt,
                wait: p.wait,
                finalizedAt: uint64(block.timestamp),
                revoked: false,
                sunset: c.sunset,
                imported: c.importedLeafCount
            })
        );
        forgedFinalised++;
        if (c.sunset) forgedSunsets++;
        _killPending(); // finalState moved: every other candidate is stale
    }

    // ---- the attacker: forged releases -----------------------------------------------------------------------------

    /// @dev A release of one denomination to the attacker with a fresh nullifier and a garbage seal, against the root
    ///      of a forged checkpoint (three times in four; three times in four waiting for its release delay) or a root
    ///      that was never finalised; with the registered relay fee to the attacker's relayer or none.
    function forgeRelease(uint256 tokenSeed, uint256 denomSeed, uint256 rootPick, uint256 flags) external {
        address token = _tokens[tokenSeed % _tokens.length];
        // forge-lint: disable-next-line(unsafe-typecast)
        uint8 d = uint8(denomSeed % _model[token].denoms.length);
        bytes32 root;
        if (_fins.length != 0 && flags % 4 != 2) {
            Fin storage f = _fins[rootPick % _fins.length];
            root = f.burnRoot;
            if ((flags >> 2) % 4 != 3) {
                uint256 opens = _opensAt(f, token);
                if (block.timestamp < opens) vm.warp(opens);
                vm.roll(block.number + 1);
            }
        } else {
            root = _root(rootPick);
        }
        _nonce++;
        _tryRelease(_request(token, d, root, keccak256(abi.encode("forged nullifier", _nonce)), (flags >> 3) % 2 == 1));
    }

    /// @dev The attacker replays a nullifier the vault has already paid, against a matured root.
    function replayNullifier(uint256 pick, uint256 rootPick, uint256 tokenSeed) external {
        if (_usedList.length == 0 || _fins.length == 0) return;
        address token = _tokens[tokenSeed % _tokens.length];
        _tryRelease(
            _request(token, 0, _fins[rootPick % _fins.length].burnRoot, _usedList[pick % _usedList.length], false)
        );
    }

    function attackerClaimsOwed(uint256 tokenSeed) external {
        address token = _tokens[tokenSeed % _tokens.length];
        if (vault.owed(attacker, token) == 0) return;
        vm.prank(attacker);
        vault.claimOwed(token);
    }

    // ---- refunds (permissionless; paid to the leaf's depositor) -------------------------------------------------------

    function refund(uint256 pick) external {
        if (_leaves.length == 0) return;
        uint64 li = uint64(pick % _leaves.length);
        LeafData memory leaf = _leafData[li];
        bytes32[] memory siblings = ReferenceMerkle.path(_leaves, li);
        bytes32[32] memory path;
        for (uint256 i = 0; i < 32; i++) {
            path[i] = siblings[i];
        }
        uint256 escrowBefore = vault.escrowed(leaf.token);
        uint256 paidBefore = _balanceOf(leaf.token, leaf.depositor) + vault.owed(leaf.depositor, leaf.token);
        vm.prank(attacker);
        try vault.refund(li, leaf.depositor, leaf.token, leaf.denomIndex, leaf.commitment, path) {}
        catch (bytes memory e) {
            _refused(e);
            return;
        }
        uint256 amount = escrowBefore - vault.escrowed(leaf.token);
        uint256 paid = _balanceOf(leaf.token, leaf.depositor) + vault.owed(leaf.depositor, leaf.token) - paidBefore;
        if (amount != leaf.escrow || paid != amount) ghostAmountMismatches++;
        (bool set, uint64 count, uint64 at) = _sunsetModel();
        if (
            !retiredModel || stopped || block.timestamp < releasesFrozenUntilModel || !set || li < count
                || block.timestamp < uint256(at) + revokeWindow + 1 || _refunded[li]
        ) ghostRefundsAgainstRules++;
        _refunded[li] = true;
        refundedAmount[leaf.token] += amount;
        refundCount++;
        _recordPayout(leaf.token, amount);
    }

    // ---- reactions -------------------------------------------------------------------------------------------------

    /// @dev One time in four: a watcher cancels a pending forged candidate.
    function watcherCancels(uint256 pick) external {
        if (_pending.length == 0 || pick % 4 != 1) return;
        Pending storage p = _pending[(pick / 4) % _pending.length];
        if (!p.live) return;
        vm.prank(watcher);
        try vault.cancelCandidate(p.c.newState) {
            p.live = false;
            cancellations++;
        } catch {}
    }

    /// @dev One time in two: the freeze authority revokes from a forged checkpoint (refused once its window is over).
    function freezerRevokes(uint256 pick) external {
        if (_fins.length == 0 || pick % 2 != 1) return;
        uint256 k = (pick / 2) % _fins.length;
        vm.prank(freezer);
        try vault.revokeFrom(k) {
            if (_fins[k].revoked || block.timestamp > uint256(_fins[k].finalizedAt) + revokeWindow) {
                ghostRevocationsOutsideWindow++;
            }
            for (uint256 i = k; i < _fins.length; i++) {
                _fins[i].revoked = true;
            }
            revocations++;
            _killPending(); // the revocation epoch moved
        } catch {}
    }

    /// @dev One time in sixteen each: freeze or unfreeze all releases, or one token's.
    function freezerFreezes(uint256 seed, uint256 tokenSeed) external {
        uint256 action = seed % 16; // 3, 5, 7, 9: the fuzzer's favourite values (0, 1, max) never act
        if (action != 3 && action != 5 && action != 7 && action != 9) return;
        action = (action - 3) / 2;
        bool frozen = action % 2 == 0;
        uint256 until = frozen ? block.timestamp + maxFreeze : 0;
        vm.prank(freezer);
        if (action < 2) {
            vault.setReleasesFrozen(frozen);
            releasesFrozenUntilModel = until;
        } else {
            address token = _tokens[tokenSeed % _tokens.length];
            vault.setTokenReleasesFrozen(token, frozen);
            tokenReleasesFrozenUntilModel[token] = until;
        }
    }

    /// @dev One time in eight: the freeze authority retires the vault (deposits end; refunds can open).
    function freezerRetires(uint256 seed) external {
        if (seed % 8 != 3 || retiredModel) return;
        vm.prank(freezer);
        vault.retire();
        retiredModel = true;
    }

    /// @dev One time in 64: the guardian trips RISC Zero's emergency stop in front of the verifier (forever).
    function guardianStops(uint256 seed) external {
        if (seed % 64 != 17 || stopped) return;
        vm.prank(freezer);
        estop.estop();
        stopped = true;
        stops++;
    }

    function warp(uint256 seconds_) external {
        vm.warp(block.timestamp + bound(seconds_, 1, MAX_WARP));
        vm.roll(block.number + 1);
    }

    // ---- views for the invariants ----------------------------------------------------------------------------------

    function tokens() external view returns (address[] memory) {
        return _tokens;
    }

    function finalisedCount() external view returns (uint256) {
        return _fins.length;
    }

    function maxUse(address token) external view returns (uint256) {
        return maxCapUseBps[token];
    }

    // ---- internals -------------------------------------------------------------------------------------------------

    function _request(address token, uint8 d, bytes32 root, bytes32 nullifier, bool withRelay)
        internal
        view
        returns (FerrataVault.ReleaseRequest memory r)
    {
        (, uint256 relayFee,,) = vault.fees(token, 0, d);
        r = FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: d,
            recipient: attacker,
            relayer: withRelay ? attackerRelayer : address(0),
            relayFee: withRelay ? relayFee : 0,
            burnRoot: root,
            nullifier: nullifier
        });
    }

    function _tryRelease(FerrataVault.ReleaseRequest memory r) internal {
        uint256 escrowBefore = vault.escrowed(r.token);
        vm.prank(attacker);
        try vault.release(r, FORGED_SEAL) {}
        catch (bytes memory e) {
            _refused(e);
            return;
        }
        uint256 amount = escrowBefore - vault.escrowed(r.token);
        if (amount != _model[r.token].denoms[r.denomIndex]) ghostAmountMismatches++;
        if (!_matured(r.burnRoot, r.token)) ghostReleaseUnaccepted++;
        if (stopped) ghostAfterStop++;
        if (block.timestamp < releasesFrozenUntilModel || block.timestamp < tokenReleasesFrozenUntilModel[r.token]) {
            ghostReleaseWhileFrozen++;
        }
        if (_used[r.nullifier]) ghostDoubleSpends++;
        _used[r.nullifier] = true;
        _usedList.push(r.nullifier);
        released[r.token] += amount;
        releaseCount++;
        _recordPayout(r.token, amount);
    }

    /// @dev Some non-revoked forged checkpoint finalised `root` and its release delay for `token` has passed: the delay
    ///      in force, plus one second when it equals the revocation window (D11).
    function _matured(bytes32 root, address token) internal view returns (bool) {
        for (uint256 k = 0; k < _fins.length; k++) {
            Fin storage f = _fins[k];
            if (f.burnRoot == root && !f.revoked && block.timestamp >= _opensAt(f, token)) return true;
        }
        return false;
    }

    function _opensAt(Fin storage f, address token) internal view returns (uint256) {
        uint256 delay = _model[token].delay;
        return uint256(f.finalizedAt) + delay + (delay == revokeWindow ? 1 : 0);
    }

    /// @dev The refund boundary: the first non-revoked sunset checkpoint's imported count and finalisation time.
    function _sunsetModel() internal view returns (bool set, uint64 count, uint64 at) {
        for (uint256 k = 0; k < _fins.length; k++) {
            if (!_fins[k].revoked && _fins[k].sunset) return (true, _fins[k].imported, _fins[k].finalizedAt);
        }
    }

    /// @dev Records a payout and checks every window that ends with it against the token bucket's bound: what left in
    ///      [t_i, t_j] is at most capacity + refill * (t_j - t_i), per capped token (token units) and for all tokens
    ///      together (the global bucket, value units).
    function _recordPayout(address token, uint256 amount) internal {
        TokenModel storage m = _model[token];
        uint64 t = uint64(block.timestamp);
        if (m.cap != UNLIMITED) {
            _pays[token].push(Pay(t, amount));
            uint256 use = _checkWindows(_pays[token], m.cap, m.refill);
            if (use > maxCapUseBps[token]) maxCapUseBps[token] = use;
        }
        if (globalCap != UNLIMITED) {
            _globalPays.push(Pay(t, amount * m.unitWeight / m.lezUnit));
            uint256 use = _checkWindows(_globalPays, globalCap, globalRefill);
            if (use > maxGlobalCapUseBps) maxGlobalCapUseBps = use;
        }
    }

    function _checkWindows(Pay[] storage ps, uint256 cap, uint256 refill) internal returns (uint256 maxBps) {
        uint256 j = ps.length - 1;
        uint256 sum;
        for (uint256 i = j + 1; i > 0; i--) {
            sum += ps[i - 1].amount;
            uint256 limit = cap + refill * (ps[j].t - ps[i - 1].t);
            if (sum > limit) ghostCapViolations++;
            uint256 bps = sum * 10_000 / limit;
            if (bps > maxBps) maxBps = bps;
        }
    }

    function _killPending() internal {
        for (uint256 i = 0; i < _pending.length; i++) {
            _pending[i].live = false;
        }
    }

    /// @dev Three burn roots the attacker reuses (so one root can be finalised by several checkpoints, some revoked),
    ///      or a fresh one.
    function _root(uint256 seed) internal view returns (bytes32) {
        uint256 k = seed % 4;
        if (k < 3) return keccak256(abi.encode("forged burn root", k));
        return keccak256(abi.encode("fresh forged burn root", _nonce, seed));
    }

    function _refused(bytes memory e) internal {
        bytes4 sel;
        if (e.length >= 4) sel = bytes4(e);
        refusals[sel]++;
    }

    function _balanceOf(address token, address who) internal view returns (uint256) {
        return token == ETH ? who.balance : MockTokenBase(token).balanceOf(who);
    }
}
