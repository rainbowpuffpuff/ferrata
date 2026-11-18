// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {BrokenVerifierSetup} from "./invariant/BrokenVerifierInvariants.t.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";

/// @notice The attack the broken-verifier invariant suite fuzzes, step by step with its numbers: the verifier accepts
///         every seal, so anyone can finalise a checkpoint with a burn root of their choice and release against it.
///         What stands in the way is the wait, the release delay, the caps, the per-token escrow, the revocation
///         window, the freezes and the emergency stop (vault/README.md "Under a broken verifier").
contract BrokenVerifierTest is BrokenVerifierSetup {
    bytes internal constant JUNK = hex"6a756e6b";
    bytes32 internal constant ROOT = keccak256("a burn root nobody burned under");

    address internal attacker = makeAddr("attacker");
    uint256 internal forged;
    uint256 internal nullifiers;
    bytes32[] internal leaves;

    function setUp() public override {
        super.setUp();
        _deployBroken();
    }

    /// @dev A forged checkpoint with a garbage seal, finalised at the first second its wait allows (and refused one
    ///      second before). Returns when it was finalised.
    function _forge(bytes32 root, bool sunset, uint64 imported) internal returns (uint256 finalizedAt) {
        forged++;
        FerrataVault.Checkpoint memory c = FerrataVault.Checkpoint({
            nMin: N_MIN,
            tMin: T_MIN,
            parentState: vault.finalState(),
            newState: keccak256(abi.encode("forged state", forged)),
            burnRoot: root,
            burnCount: uint64(forged),
            importedLeafCount: imported,
            sunset: sunset,
            echoNonce: bytes32(0),
            tipId: uint64(forged),
            tipHash: keccak256(abi.encode("forged tip", forged)),
            n: N_MIN,
            t: T_MIN
        });
        vm.prank(attacker);
        vault.submitCandidate(c, JUNK);
        uint256 wait = vault.delta() + (uint256(T_MIN) - 1) * vault.turnSeconds();
        vm.warp(block.timestamp + wait - 1);
        vm.roll(block.number + 1);
        if (_mode() == FerrataVault.Mode.A) {
            uint256 early = vault.recordBeacon();
            (, bytes32 earlyNonce) = vault.beacons(early);
            c.echoNonce = earlyNonce;
            vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
            vault.finalize(early, c, JUNK);
            vm.warp(block.timestamp + 1);
            vm.roll(block.number + 1);
            uint256 beacon = vault.recordBeacon();
            (, bytes32 nonce) = vault.beacons(beacon);
            c.echoNonce = nonce;
            vm.prank(attacker);
            vault.finalize(beacon, c, JUNK);
        } else {
            vm.expectRevert(FerrataVault.WaitNotOver.selector);
            vault.finalizeAttested(c);
            vm.warp(block.timestamp + 1);
            vm.prank(attacker);
            vault.finalizeAttested(c);
        }
        return block.timestamp;
    }

    function _forgedRequest(address token, uint8 denomIndex, bytes32 root)
        internal
        returns (FerrataVault.ReleaseRequest memory)
    {
        nullifiers++;
        return FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: denomIndex,
            recipient: attacker,
            relayer: address(0),
            relayFee: 0,
            burnRoot: root,
            nullifier: keccak256(abi.encode("forged nullifier", nullifiers))
        });
    }

    function _release(address token, uint8 denomIndex, bytes32 root) internal {
        FerrataVault.ReleaseRequest memory r = _forgedRequest(token, denomIndex, root);
        vm.prank(attacker);
        vault.release(r, JUNK);
    }

    function _releaseRefused(address token, uint8 denomIndex, bytes32 root, bytes4 reason) internal {
        FerrataVault.ReleaseRequest memory r = _forgedRequest(token, denomIndex, root);
        vm.prank(attacker);
        vm.expectRevert(reason);
        vault.release(r, JUNK);
    }

    function _depositN(address from, address token, uint8 denomIndex, uint256 n) internal {
        for (uint256 i = 0; i < n; i++) {
            bytes32 c = keccak256(abi.encode("honest commitment", leaves.length));
            uint64 leafIndex = _depositInto(vault, from, token, denomIndex, c);
            leaves.push(ReferenceMerkle.leafHash(leafIndex, from, token, _denomsOf(vault, token)[denomIndex], c));
        }
    }

    /// @notice DAI (cap 40,000 + 10 a second, delay 1 h): nothing before the delay; then the bucket, 40,000; 1,000 s
    ///         later the refill, 10,000; after the revocation nothing, however long the bucket refills.
    function test_forgedReleasesStopAtTheCapAndAtTheRevocation() public {
        _depositN(alice, address(dai), 2, 10); // 100,000 DAI escrowed (and the relay fees)
        uint256 escrow0 = vault.escrowed(address(dai));
        uint256 finalizedAt = _forge(ROOT, false, 0);

        _releaseRefused(address(dai), 2, ROOT, FerrataVault.ReleaseDelayNotElapsed.selector);
        vm.warp(finalizedAt + MIN_DELAY);
        for (uint256 i = 0; i < 4; i++) {
            _release(address(dai), 2, ROOT);
        }
        _releaseRefused(address(dai), 2, ROOT, FerrataVault.TokenReleaseCapExceeded.selector);
        vm.warp(block.timestamp + 1000);
        _release(address(dai), 2, ROOT);
        _releaseRefused(address(dai), 2, ROOT, FerrataVault.TokenReleaseCapExceeded.selector);
        assertEq(escrow0 - vault.escrowed(address(dai)), 50_000e18);
        // The attacker holds it all: the denominations less the checkpoint fee, which it is owed as the "prover".
        assertEq(dai.balanceOf(attacker) + vault.owed(attacker, address(dai)), 50_000e18);

        vm.prank(freezer);
        vault.revokeFrom(0);
        vm.warp(block.timestamp + 1 days);
        _releaseRefused(address(dai), 2, ROOT, FerrataVault.BurnRootNotAccepted.selector);
        assertEq(escrow0 - vault.escrowed(address(dai)), 50_000e18);
    }

    /// @notice ETH (delay = the revocation window, 3 days): in the last second a revocation is possible no release
    ///         is, so a freeze authority that acts within the window always acts first.
    function test_aDelayEqualToTheWindowMeansTheRevocationComesFirst() public {
        _depositN(alice, ETH, 2, 3);
        uint256 finalizedAt = _forge(ROOT, false, 0);
        vm.warp(finalizedAt + REVOKE_WINDOW);
        _releaseRefused(ETH, 2, ROOT, FerrataVault.ReleaseDelayNotElapsed.selector);
        vm.prank(freezer);
        vault.revokeFrom(0);
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        _releaseRefused(ETH, 2, ROOT, FerrataVault.BurnRootNotAccepted.selector);
    }

    /// @notice ETH, not revoked: from the second after the window it pays, up to the 15 ETH bucket.
    function test_anUnrevokedForgeryPaysAfterTheWindowUpToTheCap() public {
        _depositN(alice, ETH, 2, 3); // 30 ETH escrowed (and fees)
        uint256 finalizedAt = _forge(ROOT, false, 0);
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        _release(ETH, 2, ROOT); // 10 ETH
        _releaseRefused(ETH, 2, ROOT, FerrataVault.TokenReleaseCapExceeded.selector);
        for (uint256 i = 0; i < 5; i++) {
            _release(ETH, 1, ROOT); // 5 x 1 ETH: 15 ETH, the bucket
        }
        _releaseRefused(ETH, 0, ROOT, FerrataVault.TokenReleaseCapExceeded.selector);
        assertEq(attacker.balance + vault.owed(attacker, ETH), 15 ether);
    }

    /// @notice USDT has no token cap: the global bucket (300,000 USD of value units) is what stops it.
    function test_aTokenWithoutACapIsBoundedByTheGlobalCap() public {
        _depositN(alice, address(usdt), 2, 35); // 350,000 USDT
        uint256 finalizedAt = _forge(ROOT, false, 0);
        vm.warp(finalizedAt + 12 hours);
        for (uint256 i = 0; i < 30; i++) {
            _release(address(usdt), 2, ROOT);
        }
        _releaseRefused(address(usdt), 2, ROOT, FerrataVault.GlobalReleaseCapExceeded.selector);
        // ... and, the global bucket being shared, no other token pays either until it refills.
        _depositN(alice, address(dai), 0, 1);
        _releaseRefused(address(dai), 0, ROOT, FerrataVault.GlobalReleaseCapExceeded.selector);
    }

    /// @notice Escrow is per token: a forged release of USDC can only take USDC, however much DAI the vault holds.
    function test_escrowIsPerToken() public {
        _depositN(alice, address(dai), 2, 3);
        _depositN(alice, address(usdc), 0, 1); // 100 USDC (and the relay fee)
        uint256 finalizedAt = _forge(ROOT, false, 0);
        vm.warp(finalizedAt + 1 days);
        _releaseRefused(address(usdc), 1, ROOT, FerrataVault.InsufficientEscrow.selector); // 1,000 USDC: not there
        _release(address(usdc), 0, ROOT);
        _releaseRefused(address(usdc), 0, ROOT, FerrataVault.InsufficientEscrow.selector);
    }

    /// @notice A forged sunset checkpoint (nothing imported) makes every leaf refundable once the vault is retired and
    ///         the window has passed, but a refund pays the leaf's depositor, never the caller, and draws on the
    ///         release bucket.
    function test_aForgedSunsetRefundsDepositorsNotTheAttacker() public {
        _depositN(alice, address(dai), 1, 1); // leaf 0
        _depositN(bob, address(dai), 1, 1); // leaf 1
        uint256 escrowLeaf0 = vault.escrowed(address(dai)) / 2;
        uint256 finalizedAt = _forge(ROOT, true, 0);
        vm.prank(freezer);
        vault.retire();
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, 0);
        bytes32[32] memory path;
        for (uint256 i = 0; i < 32; i++) {
            path[i] = siblings[i];
        }
        bytes32 c0 = keccak256(abi.encode("honest commitment", uint256(0)));
        vm.prank(attacker);
        vm.expectRevert(FerrataVault.RefundNotYetOpen.selector);
        vault.refund(0, alice, address(dai), 1, c0, path);
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        uint256 before = dai.balanceOf(alice);
        vm.prank(attacker);
        vault.refund(0, alice, address(dai), 1, c0, path);
        assertEq(dai.balanceOf(alice) - before, escrowLeaf0);
        assertEq(dai.balanceOf(attacker), 0);
        (,, uint256 available) = vault.tokenCap(address(dai), FerrataVault.Direction.Release);
        assertEq(available, 40_000e18 - escrowLeaf0);
    }

    /// @notice After RISC Zero's emergency stop nothing proven is accepted: no release, no candidate, no refund.
    function test_afterTheEmergencyStopNothingPays() public {
        _depositN(alice, address(dai), 2, 2);
        uint256 finalizedAt = _forge(ROOT, false, 0);
        vm.warp(finalizedAt + MIN_DELAY);
        _release(address(dai), 2, ROOT);
        vm.prank(freezer);
        brokenStop.estop();
        _releaseRefused(address(dai), 2, ROOT, bytes4(keccak256("EnforcedPause()")));
        FerrataVault.Checkpoint memory c;
        c.nMin = N_MIN;
        c.tMin = T_MIN;
        c.parentState = vault.finalState();
        c.newState = keccak256("after the stop");
        c.n = N_MIN;
        c.t = T_MIN;
        vm.prank(attacker);
        vm.expectRevert(bytes4(keccak256("EnforcedPause()")));
        vault.submitCandidate(c, JUNK);
    }
}

/// @notice The same steps on a rule (B) vault (the wait is deltaAtSubmit, 1 h; no beacon).
contract BrokenVerifierBTest is BrokenVerifierTest {
    function _mode() internal pure override returns (FerrataVault.Mode) {
        return FerrataVault.Mode.B;
    }
}
