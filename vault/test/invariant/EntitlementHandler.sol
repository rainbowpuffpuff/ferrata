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

/// @notice Independent journal encoder: release/checkpoint preimages follow the wire specification, not vault code.
contract EntitlementCodec {
    bytes32 internal constant RELEASE_DOMAIN = sha256(bytes("ferrata/vault/release/v2"));
    bytes32 internal constant CHECKPOINT_DOMAIN = sha256(bytes("ferrata/vault/checkpoint/v3"));

    function releaseDigest(FerrataVault vault_, FerrataVault.ReleaseRequest calldata r, uint256 amount)
        external
        view
        returns (bytes32)
    {
        bytes memory context = bytes.concat(
            RELEASE_DOMAIN,
            bytes32(block.chainid),
            bytes32(uint256(uint160(address(vault_)))),
            vault_.lezConfigId(),
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

    function checkpointDigest(FerrataVault vault_, FerrataVault.Checkpoint calldata c) external view returns (bytes32) {
        bytes memory head = bytes.concat(
            CHECKPOINT_DOMAIN,
            bytes32(block.chainid),
            bytes32(uint256(uint160(address(vault_)))),
            vault_.lezConfigId(),
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
            bytes32(0)
        );
        bytes memory tail =
            bytes.concat(bytes32(uint256(c.tipId)), c.tipHash, bytes32(uint256(c.n)), bytes32(uint256(c.t)));
        return sha256(bytes.concat(head, body, tail));
    }
}

/// @dev An address with code can hold the same privileged roles as an EOA under a distinct identity.
contract EntitlementRole {}

/// @notice Drives later activity while retaining a bounded model of every entitlement it has not already collected.
///         A new ETH release is always backed by a fresh deposit of its denomination. Refund candidates use DAI, so
///         releases can never consume the escrow backing those leaves. Expected reverts on speculative deposits are
///         caught; actions with nothing to do return instead of discarding a fuzz step.
contract EntitlementHandler is CommonBase, StdCheats, StdUtils {
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    uint256 internal constant MAX_ENTITLEMENTS = 8;

    struct Leaf {
        uint64 index;
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }

    struct OwedKey {
        address payee;
        address token;
    }

    FerrataVault public immutable vault;
    MockRiscZeroVerifier public immutable verifier;
    EntitlementCodec public immutable codec;
    address public immutable token;
    ToggleRecipient public immutable toggle;
    address public immutable relayer;
    address public immutable feeRecipient;

    address public currentAdmin;
    address[3] internal _adminPool;
    address[3] internal _freezerPool;
    address[3] internal _actors;
    address[3] internal _recipients;

    FerrataVault.ReleaseRequest[] internal _releases;
    OwedKey[] internal _owedKeys;
    Leaf[] internal _refundCandidates;
    mapping(bytes32 => bool) internal _owedKnown;
    bytes32[] internal _leaves;
    bytes32[] internal _acceptedRoots;

    bool internal _candidateFilled;
    FerrataVault.Checkpoint internal _candidate;
    uint256 internal _candidateReadyAt;
    uint256 internal _nonce;
    uint256 internal _createdReleases;
    uint256 internal _collectedReleases;
    uint256 internal _otherClaims;
    uint256 internal _collectedRefunds;

    constructor(FerrataVault vault_, MockRiscZeroVerifier verifier_, address dai_, address admin_) {
        vault = vault_;
        verifier = verifier_;
        codec = new EntitlementCodec();
        token = dai_;
        currentAdmin = admin_;
        relayer = makeAddr("entitlement relayer");
        feeRecipient = vault_.feeRecipient();
        toggle = new ToggleRecipient();
        for (uint256 i = 0; i < 3; i++) {
            _actors[i] = makeAddr(string.concat("entitlement actor ", vm.toString(i)));
        }
        _recipients = [address(toggle), _actors[1], _actors[2]];
        _adminPool = [admin_, address(new EntitlementRole()), makeAddr("third entitlement admin")];
        _freezerPool = [vault_.freezeAuthority(), address(new EntitlementRole()), makeAddr("third entitlement freezer")];
        _trackOwed(feeRecipient, ETH);
        _trackOwed(feeRecipient, token);
        _trackOwed(relayer, ETH);
        _trackOwed(address(this), ETH);
        for (uint256 i = 0; i < _recipients.length; i++) {
            _trackOwed(_recipients[i], ETH);
        }
    }

    receive() external payable {} // checkpoint fees credited to this handler can also be claimed to itself

    /// @dev Called by setUp before targeting: two outstanding releases, one outstanding ETH owed claim and two DAI
    ///      leaves that become refundable after retirement and a sunset checkpoint.
    function initialize() external {
        for (uint256 i = 0; i < 3; i++) {
            require(_deposit(_actors[0], ETH, 1, keccak256(abi.encode("seed ETH", i))), "seed ETH deposit");
        }
        for (uint256 i = 0; i < 2; i++) {
            require(_deposit(_actors[1], token, 0, keccak256(abi.encode("seed DAI", i))), "seed DAI deposit");
        }
        _submit(keccak256("seed burn root"), false);
        _finalize();
        vm.warp(block.timestamp + vault.minReleaseDelay());
        for (uint256 i = 0; i < 3; i++) {
            _addRelease(i);
        }
        _releaseAt(0); // the closed ToggleRecipient gets an owed credit; two burns remain outstanding
    }

    // -------------------------------------------------------------------------------------------------------------
    // Model views used by the snapshot checks
    // -------------------------------------------------------------------------------------------------------------

    function releaseCount() external view returns (uint256) {
        return _releases.length;
    }

    function releaseAtView(uint256 i) external view returns (FerrataVault.ReleaseRequest memory) {
        return _releases[i];
    }

    function owedKeyCount() external view returns (uint256) {
        return _owedKeys.length;
    }

    function owedKeyAt(uint256 i) external view returns (address payee, address token_) {
        return (_owedKeys[i].payee, _owedKeys[i].token);
    }

    function refundCandidateCount() external view returns (uint256) {
        return _refundCandidates.length;
    }

    function refundCandidateAt(uint256 i) external view returns (Leaf memory) {
        return _refundCandidates[i];
    }

    function leaves() external view returns (bytes32[] memory) {
        return _leaves;
    }

    function summary() external view returns (uint256, uint256, uint256, uint256) {
        return (_createdReleases, _collectedReleases, _otherClaims, _collectedRefunds);
    }

    // -------------------------------------------------------------------------------------------------------------
    // Anyone: deposits, honest checkpoints, other releases, claims, refunds and time
    // -------------------------------------------------------------------------------------------------------------

    function deposit(uint256 actorSeed, uint256 tokenSeed, bytes32 commitment) external {
        if (vault.retired() || vault.finalSunset()) return;
        address who = _actors[_bound(actorSeed, 0, _actors.length - 1)];
        address token_ = _bound(tokenSeed, 0, 1) == 0 ? ETH : token;
        uint8 denomIndex = token_ == ETH ? 1 : 0;
        if (commitment == bytes32(0)) commitment = keccak256(abi.encode("nonzero", actorSeed));
        _deposit(who, token_, denomIndex, commitment);
    }

    function submitCandidate(uint256 rootSeed) external {
        if (block.timestamp < vault.checkpointsFrozenUntil()) return;
        bytes32 root = keccak256(abi.encode("later honest burn root", _bound(rootSeed, 0, 7), _nonce));
        _submit(root, vault.retired());
    }

    function finalizeCandidate(uint256 extraWait) external {
        if (!_candidateFilled) return;
        uint256 wait = _bound(extraWait, 0, 3 days);
        uint256 at = _candidateReadyAt + wait;
        if (block.timestamp < at) vm.warp(at);
        if (block.timestamp < vault.checkpointsFrozenUntil()) return;
        _finalize();
    }

    function cancelCandidate() external {
        if (!_candidateFilled) return;
        vm.prank(vault.freezeAuthority());
        vault.cancelCandidate(_candidate.newState);
        _candidateFilled = false;
    }

    function pauseCheckpoints(uint256 watcherSeed) external {
        address watcher = vault.watchers(_bound(watcherSeed, 0, 2));
        if (watcher == address(0)) return;
        vm.prank(watcher);
        vault.pauseCheckpoints();
    }

    function retire() external {
        if (vault.retired()) return;
        vm.prank(vault.freezeAuthority());
        vault.retire();
    }

    function createRelease(uint256 recipientSeed, bool withRelayFee) external {
        if (_releases.length >= MAX_ENTITLEMENTS || _acceptedRoots.length == 0) return;
        if (vault.retired() || vault.finalSunset()) return;
        if (!_deposit(_actors[0], ETH, 1, keccak256(abi.encode("release backing", _nonce)))) return;
        _addRelease(_bound(recipientSeed, 0, _recipients.length - 1));
        _releases[_releases.length - 1].relayFee = withRelayFee ? _relayFee(ETH, 1) : 0;
    }

    function releaseOutstanding(uint256 which) external {
        if (_releases.length == 0) return;
        _settleClock(ETH);
        _releaseAt(_bound(which, 0, _releases.length - 1));
    }

    function claimOutstanding(uint256 which) external {
        if (_owedKeys.length == 0) return;
        OwedKey memory key = _owedKeys[_bound(which, 0, _owedKeys.length - 1)];
        if (vault.owed(key.payee, key.token) == 0) return;
        if (key.payee == address(toggle)) toggle.setOpen(true);
        vm.prank(key.payee);
        vault.claimOwed(key.token);
        _otherClaims++;
    }

    function refundOutstanding(uint256 which) external {
        uint256 eligible = _eligibleRefundCount();
        if (eligible == 0) return;
        uint256 picked = _bound(which, 0, eligible - 1);
        for (uint256 i = 0; i < _refundCandidates.length; i++) {
            Leaf memory leaf = _refundCandidates[i];
            if (!_eligible(leaf)) continue;
            if (picked != 0) {
                picked--;
                continue;
            }
            _settleClock(leaf.token);
            bytes32[32] memory path = _path(leaf.index);
            vault.refund(leaf.index, leaf.depositor, leaf.token, leaf.denomIndex, leaf.commitment, path);
            _collectedRefunds++;
            _refundCandidates[i] = _refundCandidates[_refundCandidates.length - 1];
            _refundCandidates.pop();
            return;
        }
    }

    function warp(uint256 secondsForward) external {
        vm.warp(block.timestamp + _bound(secondsForward, 0, 3 days));
        vm.roll(block.number + 1);
    }

    // -------------------------------------------------------------------------------------------------------------
    // Admin controls. Every delta is put in force through another call; no entitlement is cancelled by these actions.
    // -------------------------------------------------------------------------------------------------------------

    function setTokenCap(uint256 tokenSeed, bool depositDirection, uint256 capacitySeed, uint256 refillSeed) external {
        address token_ = _bound(tokenSeed, 0, 1) == 0 ? ETH : token;
        FerrataVault.Direction direction =
            depositDirection ? FerrataVault.Direction.Deposit : FerrataVault.Direction.Release;
        uint256 floor = depositDirection ? 0 : _tokenFloor(token_, vault.releaseCapFloor());
        uint256 refillFloor = depositDirection ? 0 : _tokenFloor(token_, vault.releaseRefillFloor());
        uint256 capacity = _bound(capacitySeed, floor, floor == 0 ? 100_000 ether : floor * 8);
        uint256 refill = _bound(refillSeed, refillFloor, refillFloor == 0 ? 1 ether : refillFloor * 100);
        vm.prank(currentAdmin);
        vault.setTokenCap(token_, direction, capacity, refill);
    }

    function setGlobalCap(bool depositDirection, uint256 capacitySeed, uint256 refillSeed) external {
        FerrataVault.Direction direction =
            depositDirection ? FerrataVault.Direction.Deposit : FerrataVault.Direction.Release;
        uint256 floor = depositDirection ? 0 : vault.releaseCapFloor();
        uint256 refillFloor = depositDirection ? 0 : vault.releaseRefillFloor();
        uint256 capacity = _bound(capacitySeed, floor, floor == 0 ? 1_000_000 ether : floor * 100);
        uint256 refill = _bound(refillSeed, refillFloor, refillFloor == 0 ? 1 ether : refillFloor * 1000);
        vm.prank(currentAdmin);
        vault.setGlobalCap(direction, capacity, refill);
    }

    function setDepositsEnabled(uint256 tokenSeed, bool enabled) external {
        address token_ = _bound(tokenSeed, 0, 1) == 0 ? ETH : token;
        vm.prank(currentAdmin);
        vault.setDepositsEnabled(token_, enabled);
    }

    function setReleaseDelay(uint256 tokenSeed, uint256 delay) external {
        address token_ = _bound(tokenSeed, 0, 1) == 0 ? ETH : token;
        delay = _bound(delay, vault.minReleaseDelay(), vault.revokeWindow());
        vm.prank(currentAdmin);
        vault.setReleaseDelay(token_, delay);
    }

    function raiseDelta(uint256 newDelta) external {
        newDelta = _bound(newDelta, vault.delta(), 30 days);
        vm.prank(currentAdmin);
        vault.raiseDelta(newDelta);
    }

    function announceFees(uint256 tokenSeed, uint256 seed) external {
        address token_ = _bound(tokenSeed, 0, 1) == 0 ? ETH : token;
        (,, uint256 lezUnit,, uint256[] memory denoms) = vault.tokenConfig(token_);
        uint256[] memory mint = new uint256[](denoms.length);
        uint256[] memory burn = new uint256[](denoms.length);
        uint256[] memory checkpoint = new uint256[](denoms.length);
        for (uint256 i = 0; i < denoms.length; i++) {
            uint256 ceiling = denoms[i] / 100;
            mint[i] = _bound(uint256(keccak256(abi.encode(seed, i, "mint"))), 0, ceiling / lezUnit) * lezUnit;
            burn[i] = _bound(uint256(keccak256(abi.encode(seed, i, "burn"))), 0, ceiling);
            checkpoint[i] = _bound(uint256(keccak256(abi.encode(seed, i, "checkpoint"))), 0, ceiling);
        }
        vm.prank(currentAdmin);
        vault.announceFees(token_, mint, burn, checkpoint);
    }

    function applyFees(uint256 tokenSeed) external {
        vault.applyFees(_bound(tokenSeed, 0, 1) == 0 ? ETH : token);
    }

    function proposeAndApplyRoles(uint256 seed) external {
        address newFreezer = _freezerPool[_bound(seed, 0, _freezerPool.length - 1)];
        if (newFreezer == currentAdmin) {
            newFreezer = _freezerPool[1] == currentAdmin ? _freezerPool[2] : _freezerPool[1];
        }
        address[3] memory watchers = [
            address(new EntitlementRole()),
            _actors[_bound(seed >> 8, 0, _actors.length - 1)],
            address(0)
        ];
        vm.prank(currentAdmin);
        vault.proposeRoles(watchers, newFreezer);
        vm.warp(block.timestamp + 14 days);
        vault.applyRoles();
    }

    function transferAndAcceptAdmin(uint256 seed) external {
        uint256 pick = _bound(seed, 0, _adminPool.length - 1);
        address next = _adminPool[pick];
        if (next == currentAdmin || next == vault.freezeAuthority()) {
            next = _adminPool[(pick + 1) % _adminPool.length];
        }
        if (next == currentAdmin || next == vault.freezeAuthority()) return;
        vm.prank(currentAdmin);
        vault.transferAdmin(next);
        vm.prank(next);
        vault.acceptAdmin();
        currentAdmin = next;
    }

    // -------------------------------------------------------------------------------------------------------------
    // Every freeze can be switched on or off by whichever EOA or role contract currently has authority.
    // -------------------------------------------------------------------------------------------------------------

    function setFreeze(uint256 kind, uint256 tokenSeed, bool frozen) external {
        address token_ = _bound(tokenSeed, 0, 1) == 0 ? ETH : token;
        vm.startPrank(vault.freezeAuthority());
        kind = _bound(kind, 0, 4);
        if (kind == 0) vault.setDepositsFrozen(frozen);
        else if (kind == 1) vault.setReleasesFrozen(frozen);
        else if (kind == 2) vault.setCheckpointsFrozen(frozen);
        else if (kind == 3) vault.setTokenDepositsFrozen(token_, frozen);
        else vault.setTokenReleasesFrozen(token_, frozen);
        vm.stopPrank();
    }

    // -------------------------------------------------------------------------------------------------------------
    // Internal model and action helpers
    // -------------------------------------------------------------------------------------------------------------

    function _deposit(address who, address token_, uint8 denomIndex, bytes32 commitment) internal returns (bool) {
        uint256 total = _paymentTotal(token_, denomIndex);
        uint64 index = vault.depositCount(who);
        uint64 leafIndex = vault.nextLeafIndex();
        if (token_ == ETH) {
            vm.deal(who, who.balance + total);
            vm.prank(who);
            try vault.depositETH{value: total}(denomIndex, commitment, index) {} catch { return false; }
        } else {
            MockTokenBase(token_).mint(who, total);
            vm.prank(who);
            MockTokenBase(token_).approve(address(vault), total);
            vm.prank(who);
            try vault.deposit(token_, denomIndex, commitment, index) {} catch { return false; }
        }
        _trackOwed(feeRecipient, token_);
        if (token_ == token && _refundCandidates.length < MAX_ENTITLEMENTS) {
            _refundCandidates.push(Leaf(leafIndex, who, token_, denomIndex, commitment));
        }
        _leaves.push(ReferenceMerkle.leafHash(leafIndex, who, token_, _denomination(token_, denomIndex), commitment));
        return true;
    }

    function _paymentTotal(address token_, uint8 denomIndex) internal view returns (uint256) {
        (uint64 current, uint64 pendingAt) = vault.feeState(token_);
        uint256 schedule = pendingAt != 0 && block.timestamp >= pendingAt ? uint256(current) + 1 : current;
        (uint256 mintFee, uint256 relayFee,,) = vault.fees(token_, schedule, denomIndex);
        return _denomination(token_, denomIndex) + mintFee + relayFee;
    }

    function _denomination(address token_, uint8 denomIndex) internal view returns (uint256) {
        (,,,, uint256[] memory denoms) = vault.tokenConfig(token_);
        return denoms[denomIndex];
    }

    function _submit(bytes32 burnRoot, bool sunset) internal {
        _nonce++;
        uint64 imported = vault.finalImportedLeafCount();
        sunset = sunset || vault.finalSunset();
        FerrataVault.Checkpoint memory c = FerrataVault.Checkpoint({
            nMin: vault.nMin(),
            tMin: vault.tMin(),
            parentState: vault.finalState(),
            newState: keccak256(abi.encode("entitlement state", _nonce)),
            burnRoot: burnRoot,
            burnCount: uint64(_nonce),
            importedLeafCount: imported,
            sunset: sunset,
            echoNonce: bytes32(0),
            tipId: uint64(1000 + _nonce),
            tipHash: keccak256(abi.encode("entitlement tip", _nonce)),
            n: 3,
            t: 2
        });
        verifier.accept(vault.checkpointImageId(), codec.checkpointDigest(vault, c));
        vault.submitCandidate(c, "entitlement checkpoint proof");
        _candidate = c;
        _candidateReadyAt = block.timestamp + vault.delta();
        _candidateFilled = true;
    }

    function _finalize() internal {
        if (block.timestamp < _candidateReadyAt) vm.warp(_candidateReadyAt);
        vault.finalizeAttested(_candidate);
        _acceptedRoots.push(_candidate.burnRoot);
        _candidateFilled = false;
    }

    function _addRelease(uint256 recipientPick) internal {
        bytes32 root = _acceptedRoots[_acceptedRoots.length - 1];
        toggle.setOpen(false);
        FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
            token: ETH,
            denomIndex: 1,
            recipient: _recipients[recipientPick],
            relayer: relayer,
            relayFee: _relayFee(ETH, 1),
            burnRoot: root,
            nullifier: keccak256(abi.encode("entitlement nullifier", _createdReleases))
        });
        _releases.push(r);
        _createdReleases++;
    }

    function _releaseAt(uint256 which) internal {
        FerrataVault.ReleaseRequest memory r = _releases[which];
        (,,,, uint256[] memory denoms) = vault.tokenConfig(r.token);
        verifier.accept(vault.releaseImageId(), codec.releaseDigest(vault, r, denoms[r.denomIndex]));
        vm.prank(relayer);
        vault.release(r, "entitlement release proof");
        _releases[which] = _releases[_releases.length - 1];
        _releases.pop();
        _collectedReleases++;
    }

    function _settleClock(address token_) internal {
        uint256 wait = vault.maxFreeze() + vault.revokeWindow() + 1;
        wait = _max(wait, _fullRefillWait(token_));
        (uint256 globalCapacity, uint256 globalRefill,) = vault.globalCap(FerrataVault.Direction.Release);
        if (globalCapacity != UNLIMITED && globalRefill != 0) wait = _max(wait, globalCapacity / globalRefill + 1);
        vm.warp(block.timestamp + wait);
    }

    function _fullRefillWait(address token_) internal view returns (uint256) {
        (uint256 capacity, uint256 refill,) = vault.tokenCap(token_, FerrataVault.Direction.Release);
        if (capacity == UNLIMITED || refill == 0) return 0;
        return capacity / refill + 1;
    }

    /// @dev The release floor is in value units; registration converts each token by weight / lezUnit. Round up.
    function _tokenFloor(address token_, uint256 valueFloor) internal view returns (uint256) {
        (,, uint256 lezUnit, uint256 weight,) = vault.tokenConfig(token_);
        uint256 numerator = valueFloor * lezUnit;
        return numerator / weight + (numerator % weight == 0 ? 0 : 1);
    }

    function _max(uint256 a, uint256 b) internal pure returns (uint256) {
        return a > b ? a : b;
    }

    function _relayFee(address token_, uint8 denomIndex) internal view returns (uint256 relayFee) {
        (, relayFee,,) = vault.fees(token_, 0, denomIndex);
    }

    function _trackOwed(address payee, address token_) internal {
        bytes32 key = keccak256(abi.encode(payee, token_));
        if (_owedKnown[key] || _owedKeys.length >= MAX_ENTITLEMENTS) return;
        _owedKnown[key] = true;
        _owedKeys.push(OwedKey(payee, token_));
    }

    function _eligible(Leaf memory leaf) internal view returns (bool) {
        return vault.retired() && vault.sunsetLeafCountSet() && !vault.refunded(leaf.index)
            && leaf.index >= vault.sunsetLeafCount();
    }

    function _eligibleRefundCount() internal view returns (uint256 count) {
        for (uint256 i = 0; i < _refundCandidates.length; i++) {
            if (_eligible(_refundCandidates[i])) count++;
        }
    }

    function _path(uint64 leafIndex) internal view returns (bytes32[32] memory path) {
        bytes32[] memory siblings = ReferenceMerkle.path(_leaves, leafIndex);
        for (uint256 i = 0; i < 32; i++) {
            path[i] = siblings[i];
        }
    }
}
