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

/// @notice Drives the vault through random sequences of deposits, checkpoint candidates, finalisations, revocations,
///         retirement, refunds, releases (some to payees that cannot receive), owed claims, nullifier and refund
///         replays, direct donations, time, freezes and admin changes, and keeps ghost accounting for the invariants.
///         Calls the vault rejects simply revert the handler call, which also discards the ghost updates.
contract VaultHandler is CommonBase, StdCheats, StdUtils {
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;

    struct LeafData {
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }

    FerrataVault public immutable vault;
    MockRiscZeroVerifier public immutable verifier;
    address public immutable admin;
    address public immutable freezer;
    address public immutable relayer;

    address[] internal _tokens;
    address[] internal _actors;
    address[] internal _recipients; // EOAs and two contracts that accept ETH only when open
    ToggleRecipient[] internal _toggles;

    mapping(address token => uint256) public ghostDeposited;
    mapping(address token => uint256) public ghostReleased;
    mapping(address token => uint256) public ghostRefunded;
    uint256 public ghostDepositCount;
    uint256 public ghostReleaseCount;
    uint256 public ghostRefundCount;
    uint256 public ghostFinalizeCount;
    uint256 public ghostRevokeCount;
    uint256 public ghostDoubleSpends;
    uint256 public ghostDoubleRefunds;
    uint256 public ghostReplaysStoppedByNullifier;
    mapping(bytes32 state => bool) public ghostWentThroughFinalize;

    bytes32[] internal _leaves;
    LeafData[] internal _leafData;
    bytes32[] internal _acceptedRoots;
    mapping(bytes32 root => bool) internal _knownRoot;
    bytes32[] internal _usedNullifiers;
    uint64[] internal _refundedLeaves;
    FerrataVault.Checkpoint internal _pending;
    bool internal _hasPending;
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

    /// @dev A candidate on top of `finalState`; occasionally a sunset checkpoint with some imported leaf count.
    function submitCandidate(uint256 rootSeed, uint64 importSeed, bool sunsetCoin) external {
        _checkpointNonce += 1;
        uint64 leafCount = vault.nextLeafIndex();
        FerrataVault.Checkpoint memory c = FerrataVault.Checkpoint({
            nMin: vault.nMin(),
            tMin: vault.tMin(),
            parentState: vault.finalState(),
            newState: keccak256(abi.encode("handler state", _checkpointNonce)),
            burnRoot: keccak256(abi.encode("burn root", rootSeed % 6)), // a small set, so roots repeat
            burnCount: _checkpointNonce,
            importedLeafCount: importSeed % (leafCount + 1), // leafCount <= 2**32, so no overflow
            sunset: sunsetCoin && importSeed % 4 == 0,
            echoNonce: bytes32(0)
        });
        verifier.accept(vault.checkpointImageId(), _checkpointDigest(c));
        vault.submitCandidate(c, "");
        _pending = c;
        _hasPending = true;
    }

    /// @dev Finalises the last submitted candidate: waits out its delta, records a beacon and echoes it.
    function finalizePending(uint256 extraWait) external {
        if (!_hasPending) return;
        FerrataVault.Checkpoint memory c = _pending;
        (,,,,, uint256 submittedAt, uint256 deltaAtSubmit,) = vault.candidates(c.newState);
        uint256 readyAt = submittedAt + deltaAtSubmit + bound(extraWait, 0, 1 hours);
        if (block.timestamp < readyAt) vm.warp(readyAt);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, c.echoNonce) = vault.beacons(beaconIndex);
        verifier.accept(vault.checkpointImageId(), _checkpointDigest(c));
        vault.finalize(beaconIndex, c, "");

        _hasPending = false;
        ghostFinalizeCount++;
        ghostWentThroughFinalize[c.newState] = true;
        if (!_knownRoot[c.burnRoot]) {
            _knownRoot[c.burnRoot] = true;
            _acceptedRoots.push(c.burnRoot);
        }
    }

    function revoke(uint256 depthSeed) external {
        uint256 count = vault.checkpointCount();
        if (count == 0) return;
        uint256 index = count - 1 - (depthSeed % (count < 3 ? count : 3));
        vm.prank(freezer);
        vault.revokeFrom(index);
        ghostRevokeCount++;
    }

    /// @dev Retirement ends deposits for good, so it is rare and comes only after some activity.
    function retire(uint256 seed) external {
        if (vault.retired() || ghostDepositCount < 10 || seed % 8 != 0) return;
        vm.prank(freezer);
        vault.retire();
    }

    // ---- refunds ----------------------------------------------------------------------------------------------------

    function refund(uint64 leafSeed) external {
        uint64 leafCount = vault.nextLeafIndex();
        if (leafCount == 0) return;
        uint64 leafIndex = leafSeed % leafCount;
        LeafData memory l = _leafData[leafIndex];
        bytes32[32] memory path = _path(leafIndex);
        uint256 amount = vault.denominations(l.token)[l.denomIndex];
        vault.refund(leafIndex, l.depositor, l.token, l.denomIndex, l.commitment, path);
        ghostRefunded[l.token] += amount;
        ghostRefundCount++;
        _refundedLeaves.push(leafIndex);
    }

    /// @dev Tries to refund an already refunded leaf again.
    function replayRefund(uint256 which) external {
        if (_refundedLeaves.length == 0) return;
        uint64 leafIndex = _refundedLeaves[which % _refundedLeaves.length];
        LeafData memory l = _leafData[leafIndex];
        bytes32[32] memory path = _path(leafIndex);
        try vault.refund(leafIndex, l.depositor, l.token, l.denomIndex, l.commitment, path) {
            ghostDoubleRefunds++;
            ghostRefunded[l.token] += vault.denominations(l.token)[l.denomIndex];
        } catch {}
    }

    // ---- releases and owed claims ---------------------------------------------------------------------------------

    function release(uint256 tokenSeed, uint256 denomSeed, uint256 rootSeed, uint256 recipientSeed, bool withFee)
        external
    {
        if (_acceptedRoots.length == 0) return;
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
        verifier.accept(vault.releaseImageId(), _releaseDigest(r, amount));

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
            recipient: makeAddr(string.concat("replay recipient ", vm.toString(recipientSeed % 1000))),
            relayer: address(0),
            relayFee: 0,
            burnRoot: _acceptedRoots[recipientSeed % _acceptedRoots.length],
            nullifier: _usedNullifiers[which % _usedNullifiers.length]
        });
        uint256 amount = vault.denominations(token)[denomIndex];
        verifier.accept(vault.releaseImageId(), _releaseDigest(r, amount));

        vm.prank(relayer);
        try vault.release(r, "") {
            ghostDoubleSpends++;
            ghostReleased[token] += amount; // keep escrow accounting exact so only the double-spend check fires
        } catch (bytes memory err) {
            // Replays can also fail earlier (freeze, delay, revoked root); count the ones the nullifier check stopped.
            bytes memory nullifierUsed = abi.encodeWithSelector(FerrataVault.NullifierAlreadyUsed.selector);
            if (keccak256(err) == keccak256(nullifierUsed)) ghostReplaysStoppedByNullifier++;
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
        amount = bound(amount, 0, 1e24);
        if (token == ETH) {
            vm.deal(address(vault), address(vault).balance + amount);
        } else {
            MockTokenBase(token).mint(address(vault), amount);
        }
    }

    function warp(uint256 secondsForward) external {
        vm.warp(block.timestamp + bound(secondsForward, 0, 3 days));
        vm.roll(block.number + 1);
    }

    function setFreeze(uint256 which, uint256 tokenSeed, uint256 valueSeed) external {
        bool frozen = valueSeed % 4 == 0; // unfreezing is three times as likely, so activity continues
        address token = _tokens[tokenSeed % _tokens.length];
        vm.startPrank(freezer);
        uint256 kind = which % 5;
        if (kind == 0) vault.setDepositsFrozen(frozen);
        else if (kind == 1) vault.setReleasesFrozen(frozen);
        else if (kind == 2) vault.setTokenDepositsFrozen(token, frozen);
        else if (kind == 3) vault.setTokenReleasesFrozen(token, frozen);
        else vault.setCandidatesFrozen(frozen);
        vm.stopPrank();
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

        else capacity = bound(capacitySeed, largest, largest * 20);
        uint256 refill = bound(refillSeed, 0, largest / 60);
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
        vault.setReleaseDelay(token, bound(delay, 0, 2 hours));
    }

    // ---- helpers ----------------------------------------------------------------------------------------------------

    function _path(uint64 leafIndex) internal view returns (bytes32[32] memory path) {
        bytes32[] memory siblings = ReferenceMerkle.path(_leaves, leafIndex);
        for (uint256 h = 0; h < 32; h++) {
            path[h] = siblings[h];
        }
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

    function _checkpointDigest(FerrataVault.Checkpoint memory c) internal view returns (bytes32) {
        bytes memory head = bytes.concat(
            sha256(bytes("ferrata/vault/checkpoint/v2")),
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
        return sha256(bytes.concat(head, body));
    }

    function _releaseDigest(FerrataVault.ReleaseRequest memory r, uint256 amount) internal view returns (bytes32) {
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
}
