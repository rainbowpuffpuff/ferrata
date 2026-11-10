// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {CommonBase} from "forge-std/Base.sol";
import {StdCheats} from "forge-std/StdCheats.sol";
import {StdUtils} from "forge-std/StdUtils.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {MockTokenBase} from "../mocks/MockTokens.sol";
import {MockRiscZeroVerifier} from "../mocks/MockVerifiers.sol";
import {ReferenceMerkle} from "../utils/ReferenceMerkle.sol";

/// @notice Drives the vault through random sequences of deposits, root acceptances, releases, nullifier replays,
///         direct donations, time, freezes and admin changes, and keeps ghost accounting for the invariants.
///         Calls the vault rejects simply revert the handler call, which also discards the ghost updates.
contract VaultHandler is CommonBase, StdCheats, StdUtils {
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;

    FerrataVault public immutable vault;
    MockRiscZeroVerifier public immutable verifier;
    address public immutable admin;
    address public immutable freezer;
    address public immutable relayer;

    address[] internal _tokens;
    address[] internal _actors;
    address[] internal _recipients;

    mapping(address token => uint256) public ghostDeposited;
    mapping(address token => uint256) public ghostReleased;
    uint256 public ghostDepositCount;
    uint256 public ghostReleaseCount;
    uint256 public ghostDoubleSpends;
    uint256 public ghostReplaysStoppedByNullifier;
    bytes32[] internal _leaves;
    bytes32[] internal _acceptedRoots;
    mapping(bytes32 root => bool) internal _isAcceptedRoot;
    bytes32[] internal _usedNullifiers;
    uint256 internal _nonce;

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

    // ---- actions --------------------------------------------------------------------------------------------------

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
    }

    function acceptRoot(uint256 rootSeed, uint64 lezBlockId) external {
        bytes32 root = keccak256(abi.encode("burn root", rootSeed % 8)); // a small set, so re-acceptance happens
        uint32 depth = vault.minLezDepth();
        verifier.accept(vault.rootImageId(), _rootDigest(root, lezBlockId, 1, depth));
        vault.acceptBurnRoot(root, lezBlockId, 1, depth, "");
        if (!_isAcceptedRoot[root]) {
            _isAcceptedRoot[root] = true;
            _acceptedRoots.push(root);
        }
    }

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
            feeRecipient: (recipientSeed >> 128) % 2 == 0 ? address(0) : _recipients[(recipientSeed >> 64) % 3],
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
            feeRecipient: address(0),
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
            // Replays can also fail earlier (freeze, delay); count the ones the nullifier check itself stopped.
            bytes memory nullifierUsed = abi.encodeWithSelector(FerrataVault.NullifierAlreadyUsed.selector);
            if (keccak256(err) == keccak256(nullifierUsed)) ghostReplaysStoppedByNullifier++;
        }
    }

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
    }

    function setFreeze(uint256 which, uint256 tokenSeed, uint256 valueSeed) external {
        bool frozen = valueSeed % 4 == 0; // unfreezing is three times as likely, so activity continues
        address token = _tokens[tokenSeed % _tokens.length];
        vm.startPrank(freezer);
        uint256 kind = which % 4;
        if (kind == 0) vault.setDepositsFrozen(frozen);
        else if (kind == 1) vault.setReleasesFrozen(frozen);
        else if (kind == 2) vault.setTokenDepositsFrozen(token, frozen);
        else vault.setTokenReleasesFrozen(token, frozen);
        vm.stopPrank();
    }

    function setCap(uint256 tokenSeed, bool isDeposit, bool isGlobal, uint256 capacitySeed, uint256 refillSeed)
        external
    {
        address token = _tokens[tokenSeed % _tokens.length];
        uint256 largest = vault.denominations(token)[2];
        uint256 capacity;
        uint256 choice = capacitySeed % 4;
        if (choice == 0) capacity = UNLIMITED;
        else if (choice == 1) capacity = 0;
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

    // ---- helpers --------------------------------------------------------------------------------------------------

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

    function _rootDigest(bytes32 root, uint64 lezBlockId, uint64 burnCount, uint32 depth)
        internal
        view
        returns (bytes32)
    {
        return sha256(
            bytes.concat(
                sha256(bytes("ferrata/vault/burn-root/v1")),
                bytes32(block.chainid),
                bytes32(uint256(uint160(address(vault)))),
                vault.lezConfigId(),
                bytes32(uint256(lezBlockId)),
                root,
                bytes32(uint256(burnCount)),
                bytes32(uint256(depth))
            )
        );
    }

    function _releaseDigest(FerrataVault.ReleaseRequest memory r, uint256 amount) internal view returns (bytes32) {
        bytes memory context = bytes.concat(
            sha256(bytes("ferrata/vault/release/v1")),
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
            bytes32(uint256(uint160(r.feeRecipient))),
            bytes32(r.relayFee)
        );
        return sha256(bytes.concat(context, payout));
    }
}
