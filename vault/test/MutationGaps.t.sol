// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {Vm} from "forge-std/Vm.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Tests written for the mutants of the Gambit run of 18 November 2026 that the suite let survive and that are
///         not equivalent (vault/MUTATION.md lists each mutant, its classification and the test that kills it).
contract MutationGapsTest is VaultTestBase {
    bytes32 internal constant ROOT_A = keccak256("mutation gaps: burn root A");
    bytes32 internal constant ROOT_B = keccak256("mutation gaps: burn root B");

    function _accepted(bytes32 root) internal view returns (bool accepted) {
        (accepted,) = vault.burnRootStatus(root);
    }

    /// @dev vault-430: the keyless-retirement clock starts at the deployment's own time. setUp deploys at timestamp 1,
    ///      where `keylessFrom = 1` cannot be told apart from `block.timestamp`.
    function test_theKeylessClockStartsAtTheDeploymentTime() public {
        vm.warp(1_000_000);
        FerrataVault v = _deployVault(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        assertEq(v.keylessFrom(), 1_000_000);
        assertEq(v.retireOpensAt(), 1_000_000 + DELTA + (uint256(T_MIN) - 1) * TURN_SECONDS + 30 days);
    }

    /// @dev vault-719: a burn root whose checkpoint was revoked is accepted again when a later checkpoint finalises it,
    ///      also when the revoked checkpoint is not checkpoint 0 (the mutant reads checkpoint 0's flag instead).
    function test_aRootRevokedAtALaterCheckpointIsAcceptedAgainWhenReproven() public {
        _acceptRoot(ROOT_A); // checkpoint 0, never revoked
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT_B);
        _submit(c);
        assertEq(_finalizeAfterDelta(c), 1);
        assertEq(vault.burnRootCheckpoint(ROOT_B), 2);

        vm.prank(freezer);
        vault.revokeFrom(1);
        assertFalse(_accepted(ROOT_B));
        assertTrue(_accepted(ROOT_A));

        c.echoNonce = bytes32(0);
        _submit(c); // the same state and root, proven again on the restored parent
        assertEq(_finalizeAfterDelta(c), 2);
        assertEq(vault.burnRootCheckpoint(ROOT_B), 3);
        assertTrue(_accepted(ROOT_B));
    }

    /// @dev vault-794: a release without a relay fee pays the fee payee nothing, and so makes no transfer to it (the
    ///      mutant transfers 0).
    function test_aReleaseWithoutARelayFeeMakesNoTransferToTheFeePayee() public {
        _deposit(alice, address(usdc), 0, keccak256("mutation gaps: usdc deposit"));
        _acceptRoot(ROOT_A);
        FerrataVault.ReleaseRequest memory r =
            _request(address(usdc), 0, recipient, ROOT_A, keccak256("mutation gaps: nullifier"));
        vm.expectCall(address(usdc), abi.encodeCall(IERC20.transfer, (relayer, 0)), 0);
        _release(r);
        assertGt(usdc.balanceOf(recipient), 0);
    }

    /// @dev vault-1061, vault-1063: revoking the sunset checkpoint clears `sunsetSetAt` with the boundary it set. The
    ///      sunset checkpoint is checkpoint 1 here, so a stale index (1061) or a 1 (1063) differs from the cleared 0.
    function test_revokingTheSunsetCheckpointClearsItsIndex() public {
        _fillTree(8);
        _acceptRoot(ROOT_A); // checkpoint 0, not a sunset one
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT_B, 4, true);
        _submit(c);
        assertEq(_finalizeAfterDelta(c), 1); // checkpoint 1: the sunset at count 4
        assertTrue(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetSetAt(), 1);

        vm.prank(freezer);
        vault.revokeFrom(1);
        assertFalse(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 0);
        assertEq(vault.sunsetSetAt(), 0);
        assertEq(vault.sunsetFinalizedAt(), 0);
    }

    /// @dev vault-1367, vault-1373: a second beacon request in a block returns that block's beacon and records none.
    ///      The suite asked twice in a block only when one beacon existed (index - 1 == index % 1 == 0) or in block 1
    ///      (the block the mutant writes as the last one).
    function test_aSecondBeaconRequestInABlockReturnsThatBlocksBeacon() public {
        vm.roll(10);
        uint256 first = vault.recordBeacon();
        vm.roll(11);
        uint256 second = vault.recordBeacon();
        assertEq(second, first + 1);
        assertEq(vault.recordBeacon(), second);
        vm.roll(12);
        assertEq(vault.recordBeacon(), second + 1);
    }

    /// @dev registry-484: applying a proposal that keeps the freeze authority (address(0)) emits no
    ///      `FreezeAuthorityTransferred` (the mutant emits one naming address(0)); the authority stays.
    function test_applyingAProposalThatKeepsTheFreezeAuthorityEmitsNoTransfer() public {
        vm.prank(admin);
        vault.proposeRoles([makeAddr("mutation gaps: watcher"), address(0), address(0)], address(0));
        vm.warp(block.timestamp + 14 days);
        vm.recordLogs();
        vault.applyRoles();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        bool watchersSet;
        for (uint256 i = 0; i < logs.length; i++) {
            assertTrue(logs[i].topics[0] != FerrataVault.FreezeAuthorityTransferred.selector, "a transfer to nobody");
            if (logs[i].topics[0] == FerrataVault.WatchersSet.selector) watchersSet = true;
        }
        assertTrue(watchersSet);
        assertEq(vault.freezeAuthority(), freezer);
    }

    /// @dev registry-728, registry-731: a cap update restarts the refill clock. Right after it the bucket holds what it
    ///      held, settled under the old rate, and it refills at the new rate from the update on, not from the last
    ///      consumption (728 keeps the old `lastUpdate`, 731 writes 1). The suite changed caps without a time gap.
    function test_aCapUpdateRestartsTheRefillClock() public {
        FerrataVault.Direction d = FerrataVault.Direction.Deposit;
        vm.warp(1000);
        vm.prank(admin);
        vault.setTokenCap(address(dai), d, 1000e18, 0);
        _deposit(alice, address(dai), 0, keccak256("mutation gaps: dai deposit"));
        (,, uint256 left) = vault.tokenCap(address(dai), d);
        assertLt(left, 1000e18);

        vm.warp(2000);
        vm.prank(admin);
        vault.setTokenCap(address(dai), d, 1000e18, 1e15); // a refill rate from now on
        (,, uint256 afterUpdate) = vault.tokenCap(address(dai), d);
        assertEq(afterUpdate, left); // the old rate (0) earned nothing in the 1000 s before
        vm.warp(2010);
        (,, uint256 later) = vault.tokenCap(address(dai), d);
        assertEq(later, left + 10 * 1e15);
    }
}
