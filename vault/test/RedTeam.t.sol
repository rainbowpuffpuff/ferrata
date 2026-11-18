// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console} from "forge-std/console.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockFeeOnTransferToken, MockTokenBase} from "./mocks/MockTokens.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice The blind red-team vectors of area "vault" (`vectors/redteam-2026-11-18/vectors_bridge.py`, list VAULT;
///         written from the specification alone before any implementation was read), one test per vector, on the
///         suite's own fixture (B-VLT: the v5 vault behind RISC Zero's emergency stop, the mock base verifier, ETH and
///         three tokens registered, rule (A)). A vector expecting a refusal passes if the call reverts at all
///         (`vm.expectRevert()`: the specification names no error); before that, the same call is made once as a
///         probe whose effects are rolled back, only to record which error it reverted with. Each test logs one
///         `REDTEAM|<id>|<outcome>|<error>|<note>` line per decisive call, which the results file is made from.
contract RedTeamTest is VaultTestBase {
    // ---------------------------------------------------------------------------------------------------------------
    // Recording
    // ---------------------------------------------------------------------------------------------------------------

    function _record(string memory id, string memory outcome, string memory error_, string memory note) internal pure {
        console.log(string.concat("REDTEAM|", id, "|", outcome, "|", error_, "|", note));
    }

    /// @dev The name of the custom error (or `Error(string)` / `Panic(uint256)`) in revert data.
    function _errorName(bytes memory ret) internal pure returns (string memory) {
        if (ret.length < 4) return ret.length == 0 ? "revert without data" : "short revert data";
        bytes4 s = bytes4(ret);
        if (s == 0x08c379a0) {
            bytes memory reason = new bytes(ret.length - 4);
            for (uint256 i = 0; i < reason.length; i++) {
                reason[i] = ret[i + 4];
            }
            return string.concat("Error(", abi.decode(reason, (string)), ")");
        }
        if (s == 0x4e487b71) return "Panic(uint256)";
        if (s == FerrataVault.DirectEthNotAccepted.selector) return "DirectEthNotAccepted()";
        if (s == FerrataVault.FunctionNotFound.selector) return "FunctionNotFound()";
        if (s == FerrataVault.WrongDepositIndex.selector) return "WrongDepositIndex()";
        if (s == FerrataVault.VaultRetired.selector) return "VaultRetired()";
        if (s == FerrataVault.TransferAmountMismatch.selector) return "TransferAmountMismatch()";
        if (s == FerrataVault.InvalidDenominationIndex.selector) return "InvalidDenominationIndex()";
        if (s == FerrataVault.WrongEthAmount.selector) return "WrongEthAmount()";
        if (s == FerrataVault.ZeroCommitment.selector) return "ZeroCommitment()";
        if (s == FerrataVault.TokenNotRegistered.selector) return "TokenNotRegistered()";
        if (s == FerrataVault.NullifierAlreadyUsed.selector) return "NullifierAlreadyUsed()";
        if (s == FerrataVault.InvalidRelayFee.selector) return "InvalidRelayFee()";
        if (s == FerrataVault.ReleaseDelayNotElapsed.selector) return "ReleaseDelayNotElapsed()";
        if (s == FerrataVault.BurnRootNotAccepted.selector) return "BurnRootNotAccepted()";
        if (s == FerrataVault.InsufficientEscrow.selector) return "InsufficientEscrow()";
        if (s == FerrataVault.ParentNotFinal.selector) return "ParentNotFinal()";
        if (s == FerrataVault.CandidateAlreadySubmitted.selector) return "CandidateAlreadySubmitted()";
        if (s == FerrataVault.EchoNonceNotZero.selector) return "EchoNonceNotZero()";
        if (s == FerrataVault.NotRefundable.selector) return "NotRefundable()";
        if (s == FerrataVault.NotRetired.selector) return "NotRetired()";
        if (s == FerrataVault.RefundNotYetOpen.selector) return "RefundNotYetOpen()";
        if (s == FerrataVault.InvalidMerklePath.selector) return "InvalidMerklePath()";
        if (s == FerrataVault.CheckpointAlreadyRevoked.selector) return "CheckpointAlreadyRevoked()";
        if (s == FerrataVault.RevokeWindowPassed.selector) return "RevokeWindowPassed()";
        if (s == FerrataVault.UnknownCheckpoint.selector) return "UnknownCheckpoint()";
        if (s == FerrataVault.DeltaCanOnlyRise.selector) return "DeltaCanOnlyRise()";
        if (s == FerrataVault.DeltaTooLong.selector) return "DeltaTooLong()";
        if (s == FerrataVault.NotAdmin.selector) return "NotAdmin()";
        if (s == FerrataVault.NotFreezeAuthority.selector) return "NotFreezeAuthority()";
        if (s == MockRiscZeroVerifier.VerificationFailed.selector) return "VerificationFailed() (mock verifier)";
        if (s == MockTokenBase.InsufficientBalance.selector) return "InsufficientBalance() (token)";
        if (s == MockTokenBase.InsufficientAllowance.selector) return "InsufficientAllowance() (token)";
        return string.concat("selector ", vm.toString(abi.encodePacked(s)));
    }

    /// @dev The vector's decisive call, `data` with `value` from `from` to the vault: a probe whose effects are rolled
    ///      back records the error it reverts with (or that it was accepted), then the verdict, any revert.
    function _expectRefusal(string memory id, address from, uint256 value, bytes memory data, string memory note)
        internal
    {
        uint256 snapshot = vm.snapshotState();
        vm.prank(from);
        (bool ok, bytes memory ret) = address(vault).call{value: value}(data);
        vm.revertToState(snapshot);
        _record(id, ok ? "accepted" : "refused", ok ? "" : _errorName(ret), note);

        vm.prank(from);
        vm.expectRevert();
        (bool revertedAsExpected,) = address(vault).call{value: value}(data);
        assertTrue(revertedAsExpected, "expectRevert: the call did not revert");
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Fixture pieces
    // ---------------------------------------------------------------------------------------------------------------

    /// @dev A release of `denomIndex` of `token` under an accepted root, proven (the mock verifier accepts its journal).
    function _provenRelease(address token, uint8 denomIndex, bytes32 root, bytes32 nullifier)
        internal
        returns (FerrataVault.ReleaseRequest memory r)
    {
        r = _request(token, denomIndex, recipient, root, nullifier);
        _proveRelease(_releaseJournal(r));
    }

    // Refund fixture (as test/Refund.t.sol builds it): the leaves of the deposit tree, to make Merkle paths.
    struct Leaf {
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }

    Leaf[] internal leafData;
    bytes32[] internal leaves;

    function _dep(address who, address token, uint8 denomIndex, bytes32 commitment) internal returns (uint64 i) {
        i = _deposit(who, token, denomIndex, commitment);
        leafData.push(Leaf(who, token, denomIndex, commitment));
        leaves.push(_leafHash(i, who, token, _amount(token, denomIndex), commitment));
    }

    function _path(uint64 i) internal view returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    function _refundCall(uint64 i) internal view returns (bytes memory) {
        Leaf memory l = leafData[i];
        return abi.encodeCall(FerrataVault.refund, (i, l.depositor, l.token, l.denomIndex, l.commitment, _path(i)));
    }

    /// @dev Finalises a sunset checkpoint with `importedLeafCount` (rule (A): candidate, wait, beacon, second proof).
    function _sunset(uint64 importedLeafCount) internal returns (uint256 index) {
        FerrataVault.Checkpoint memory c =
            _nextCheckpoint(vault, keccak256("redteam root at sunset"), importedLeafCount, true);
        _submit(c);
        return _finalizeAfterDelta(c);
    }

    // ---------------------------------------------------------------------------------------------------------------
    // The vectors
    // ---------------------------------------------------------------------------------------------------------------

    /// VLT-01: plain ETH (empty calldata, 1 ether) sent to the vault. Expected: refused (bridge.md §5.1).
    function test_VLT_01_plainEthSentToTheVault() public {
        vm.deal(alice, 1 ether);
        _expectRefusal("VLT-01", alice, 1 ether, "", "1 ether, empty calldata, from an EOA");
        assertEq(address(vault).balance, 0);
    }

    /// VLT-02: deposit(token, 0, C, j + 1) with j = depositCount[msg.sender]. Expected: refused (bridge.md §5.1).
    function test_VLT_02_depositWithIndexNotTheDepositCount() public {
        uint256 total = _depositTotal(vault, address(usdc), 0);
        usdc.mint(alice, total);
        vm.prank(alice);
        usdc.approve(address(vault), total);
        uint64 j = vault.depositCount(alice);
        _expectRefusal(
            "VLT-02",
            alice,
            0,
            abi.encodeCall(FerrataVault.deposit, (address(usdc), 0, keccak256("redteam C"), j + 1)),
            "USDC denomination 0 funded and approved; index j + 1"
        );
        assertEq(vault.depositCount(alice), j);
    }

    /// VLT-03: depositETH(0, C, j) after retire(). Expected: refused (SPEC-v2 §3.4).
    function test_VLT_03_depositAfterRetire() public {
        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());
        uint256 total = _depositTotal(vault, ETH, 0);
        vm.deal(alice, total);
        uint64 j = vault.depositCount(alice);
        _expectRefusal(
            "VLT-03",
            alice,
            total,
            abi.encodeCall(FerrataVault.depositETH, (0, keccak256("redteam C"), j)),
            "retired by the freeze authority; exact value, correct index"
        );
    }

    /// VLT-04: a registered fee-on-transfer token (the suite's MockFeeOnTransferToken: 1% burnt on every transfer),
    ///         deposit of one denomination. Expected: refused (bridge.md §5.1).
    function test_VLT_04_feeOnTransferTokenDeposit() public {
        MockFeeOnTransferToken fot = new MockFeeOnTransferToken();
        vm.prank(admin);
        vault.registerToken(address(fot), _params(_denoms(1e18, 10e18, 100e18), 1e12, 1));
        uint256 total = _depositTotal(vault, address(fot), 0);
        fot.mint(alice, total);
        vm.prank(alice);
        fot.approve(address(vault), total);
        _expectRefusal(
            "VLT-04",
            alice,
            0,
            abi.encodeCall(FerrataVault.deposit, (address(fot), 0, keccak256("redteam C"), 0)),
            "fee-on-transfer token registered by the admin; denomination 0 funded and approved"
        );
        assertEq(vault.escrowed(address(fot)), 0);
    }

    /// VLT-05 (control): a registered USDT-like token whose transfer/transferFrom return nothing (the suite's
    ///         MockUSDT, registered by the fixture): a deposit, and later a release. Expected: accepted (F.17).
    function test_VLT_05_nonReturningTokenDepositAndRelease() public {
        uint256 amount = _amount(address(usdt), 1);
        uint256 total = _depositTotal(vault, address(usdt), 1);
        uint256 vaultBefore = usdt.balanceOf(address(vault));
        _deposit(alice, address(usdt), 1, keccak256("redteam usdt C"));
        assertEq(usdt.balanceOf(address(vault)) - vaultBefore, total, "deposit escrowed the total");
        assertEq(vault.escrowed(address(usdt)), total);
        assertEq(vault.depositCount(alice), 1);

        bytes32 root = keccak256("redteam usdt burn root");
        _acceptRoot(root);
        FerrataVault.ReleaseRequest memory r =
            _request(address(usdt), 1, recipient, root, keccak256("redteam usdt NOut"));
        _release(r);
        assertEq(usdt.balanceOf(recipient), amount, "release paid the denomination");
        assertTrue(vault.nullifierUsed(r.nullifier));
        assertEq(vault.escrowed(address(usdt)), total - amount);
        _record(
            "VLT-05",
            "accepted",
            "",
            string.concat(
                "deposit escrowed ",
                vm.toString(total),
                " (denomination ",
                vm.toString(amount),
                " + relay fee); release paid the recipient ",
                vm.toString(usdt.balanceOf(recipient)),
                "; NOut used"
            )
        );
    }

    /// VLT-06: deposit(token, 8, C, j): the denomination index out of range (three registered). Expected: refused.
    function test_VLT_06_denominationIndexOutOfRange() public {
        usdc.mint(alice, 1_000_000e6);
        vm.prank(alice);
        usdc.approve(address(vault), type(uint256).max);
        _expectRefusal(
            "VLT-06",
            alice,
            0,
            abi.encodeCall(FerrataVault.deposit, (address(usdc), 8, keccak256("redteam C"), 0)),
            "USDC (3 denominations) funded and approved; denomIndex 8"
        );
    }

    /// VLT-07: a finalised burn root and a valid release; the same release again, with the same seal and with a
    ///         re-randomised seal. Expected: refused (SPEC-v2 §5.3: replay keyed on NOut only).
    function test_VLT_07_releaseReplayedSameNOut() public {
        _deposit(alice, ETH, 1, keccak256("redteam C 0"));
        _deposit(alice, ETH, 1, keccak256("redteam C 1")); // escrow for a second payout, were the replay accepted
        bytes32 root = keccak256("redteam burn root");
        _acceptRoot(root);
        FerrataVault.ReleaseRequest memory r = _provenRelease(ETH, 1, root, keccak256("redteam NOut"));
        vm.prank(relayer);
        vault.release(r, SEAL);
        assertEq(recipient.balance, 1 ether);

        _expectRefusal("VLT-07", relayer, 0, abi.encodeCall(FerrataVault.release, (r, SEAL)), "same release, same seal");
        _expectRefusal(
            "VLT-07",
            relayer,
            0,
            abi.encodeCall(FerrataVault.release, (r, bytes(hex"5eed0f5eed0f5eed0f"))),
            "same release, re-randomised seal"
        );
        assertEq(recipient.balance, 1 ether);
    }

    /// VLT-08: a release with relayFee = the registered fee + 1 (otherwise valid and proven). Expected: refused.
    function test_VLT_08_relayFeeNeitherZeroNorRegistered() public {
        _deposit(alice, ETH, 1, keccak256("redteam C"));
        bytes32 root = keccak256("redteam burn root");
        _acceptRoot(root);
        uint256 registered = _relayFee(ETH, 1);
        assertGt(registered, 0);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, root, keccak256("redteam NOut"));
        r.relayer = relayer;
        r.relayFee = registered + 1;
        _proveRelease(_releaseJournal(r));
        _expectRefusal(
            "VLT-08",
            relayer,
            0,
            abi.encodeCall(FerrataVault.release, (r, SEAL)),
            string.concat("relayFee ", vm.toString(registered + 1), " = registered + 1; journal proven")
        );
    }

    /// VLT-09: a release one second before finalizedAt + releaseDelay (delay 1 h, set before the root's checkpoint).
    ///         Expected: refused (SPEC-v2 §5.3).
    function test_VLT_09_releaseBeforeTheReleaseDelay() public {
        uint256 delay = 1 hours;
        vm.prank(admin);
        vault.setReleaseDelay(ETH, delay);
        _deposit(alice, ETH, 1, keccak256("redteam C"));
        bytes32 root = keccak256("redteam burn root");
        _acceptRoot(root);
        uint256 finalizedAt = _finalizedAt(root);
        FerrataVault.ReleaseRequest memory r = _provenRelease(ETH, 1, root, keccak256("redteam NOut"));
        vm.warp(finalizedAt + delay - 1);
        _expectRefusal(
            "VLT-09",
            relayer,
            0,
            abi.encodeCall(FerrataVault.release, (r, SEAL)),
            "releaseDelay 1 h; at finalizedAt + 1 h - 1 s; journal proven"
        );
    }

    /// VLT-10: revokeFrom(index of the root), then a release under that root. Expected: refused (SPEC-v2 §4.4).
    function test_VLT_10_releaseAgainstARevokedRoot() public {
        _deposit(alice, ETH, 1, keccak256("redteam C"));
        bytes32 root = keccak256("redteam burn root");
        uint256 index = _acceptRoot(root);
        vm.prank(freezer);
        vault.revokeFrom(index);
        FerrataVault.ReleaseRequest memory r = _provenRelease(ETH, 1, root, keccak256("redteam NOut"));
        _expectRefusal(
            "VLT-10",
            relayer,
            0,
            abi.encodeCall(FerrataVault.release, (r, SEAL)),
            "root's checkpoint revoked inside the window; journal proven"
        );
    }

    /// VLT-11: submitCandidate with parentState = a random value (its journal proven). Expected: refused (§10.1).
    function test_VLT_11_candidateWhoseParentIsNotFinalState() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(keccak256("redteam burn root"));
        c.parentState = keccak256("redteam random parent state");
        assertTrue(c.parentState != vault.finalState());
        _proveCheckpoint(_checkpointJournal(c));
        _expectRefusal(
            "VLT-11",
            relayer,
            0,
            abi.encodeCall(FerrataVault.submitCandidate, (c, SEAL)),
            "parentState random; journal proven"
        );
    }

    /// VLT-12: a pending candidate, then submitCandidate with the same newState. Expected: refused (SPEC-v3 D13).
    function test_VLT_12_candidateAlreadyPending() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(keccak256("redteam burn root"));
        _submit(c);
        vm.warp(block.timestamp + 1 minutes);
        _expectRefusal(
            "VLT-12",
            relayer,
            0,
            abi.encodeCall(FerrataVault.submitCandidate, (c, SEAL)),
            "the same candidate (same newState, proven journal) one minute later"
        );
    }

    /// VLT-13: retired, a finalised sunset checkpoint with importedLeafCount k = 2 past its revoke window;
    ///         refund(leafIndex k - 1, ...) with the leaf's true data and path. Expected: refused (bridge.md §16).
    function test_VLT_13_refundBelowTheSunsetBoundary() public {
        _dep(alice, ETH, 1, keccak256("redteam c0"));
        _dep(bob, ETH, 0, keccak256("redteam c1"));
        _dep(alice, ETH, 0, keccak256("redteam c2"));
        _dep(bob, ETH, 1, keccak256("redteam c3"));
        vm.prank(freezer);
        vault.retire();
        uint64 k = 2;
        _sunset(k);
        assertEq(vault.sunsetLeafCount(), k);
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);
        bytes memory call_ = _refundCall(k - 1);
        _expectRefusal("VLT-13", relayer, 0, call_, "k = 2, refund(1) after the revoke window; true leaf and path");
    }

    /// VLT-14: a finalised sunset checkpoint (k = 1), not retired; refund(leafIndex >= k, ...) once its revoke window
    ///         has passed. Expected: refused (SPEC-v3 D5).
    function test_VLT_14_refundBeforeRetirement() public {
        _dep(alice, ETH, 1, keccak256("redteam c0"));
        _dep(bob, ETH, 0, keccak256("redteam c1"));
        uint64 k = 1;
        _sunset(k);
        assertFalse(vault.retired());
        vm.warp(vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1);
        bytes memory call_ = _refundCall(k);
        _expectRefusal("VLT-14", relayer, 0, call_, "k = 1, refund(1), not retired, after the revoke window");
    }

    /// VLT-15: revokeFrom(i), then revokeFrom(i) again (inside the window). Expected: refused (SPEC-v3 D5).
    function test_VLT_15_revokeAnAlreadyRevokedIndex() public {
        uint256 i = _acceptRoot(keccak256("redteam burn root"));
        vm.prank(freezer);
        vault.revokeFrom(i);
        _expectRefusal(
            "VLT-15", freezer, 0, abi.encodeCall(FerrataVault.revokeFrom, (i)), "by the freeze authority, same index"
        );
    }

    /// VLT-16: rule (A); the admin sets delta below its current value (the vault's only delta setter, raiseDelta,
    ///         with current - 1 s, still above the floor). Expected: refused (SPEC-v2 §7: raise only).
    function test_VLT_16_adminLowersDelta() public {
        assertTrue(vault.mode() == FerrataVault.Mode.A);
        uint256 current = vault.delta();
        assertGt(current - 1, DELTA_FLOOR);
        _expectRefusal(
            "VLT-16",
            admin,
            0,
            abi.encodeCall(FerrataVault.raiseDelta, (current - 1)),
            string.concat("raiseDelta(", vm.toString(current - 1), ") by the admin; current ", vm.toString(current))
        );
        assertEq(vault.delta(), current);
    }
}
