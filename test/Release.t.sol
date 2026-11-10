// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {ReentrantRecipient, RejectingRecipient} from "./mocks/MockRecipients.sol";
import {MockERC20, MockReentrantToken, MockTokenBase} from "./mocks/MockTokens.sol";
import {MockRiscZeroVerifier, RejectingVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

contract AcceptBurnRootTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root 1");
    bytes32 internal constant ROOT_2 = keccak256("burn root 2");

    function test_acceptBurnRoot_recordsFirstAcceptance() public {
        vm.warp(1_700_000_000);
        _proveRoot(_rootJournal(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH));

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.BurnRootAccepted(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH);
        vm.prank(relayer); // permissionless
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);

        assertEq(vault.burnRootAcceptedAt(ROOT), 1_700_000_000);
        assertEq(vault.latestLezBlockId(), LEZ_BLOCK);
    }

    function test_acceptBurnRoot_replayedProofDoesNotMoveTimestamp() public {
        _acceptRoot(ROOT);
        uint256 acceptedAt = vault.burnRootAcceptedAt(ROOT);

        vm.warp(block.timestamp + 7 days);
        vm.recordLogs();
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL); // the same public seal, replayed
        assertEq(vm.getRecordedLogs().length, 0);
        assertEq(vault.burnRootAcceptedAt(ROOT), acceptedAt);
    }

    function test_acceptBurnRoot_sameRootAtLaterBlockKeepsTimestamp() public {
        _acceptRoot(ROOT);
        uint256 acceptedAt = vault.burnRootAcceptedAt(ROOT);

        vm.warp(block.timestamp + 1 hours);
        _proveRoot(_rootJournal(ROOT, LEZ_BLOCK + 50, BURN_COUNT, MIN_DEPTH + 50));
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK + 50, BURN_COUNT, MIN_DEPTH + 50, SEAL);

        assertEq(vault.burnRootAcceptedAt(ROOT), acceptedAt);
        assertEq(vault.latestLezBlockId(), LEZ_BLOCK + 50);
    }

    function test_acceptBurnRoot_olderBlockIsAcceptedButDoesNotLowerLatest() public {
        _acceptRoot(ROOT);
        _proveRoot(_rootJournal(ROOT_2, LEZ_BLOCK - 100, 5, MIN_DEPTH));
        vault.acceptBurnRoot(ROOT_2, LEZ_BLOCK - 100, 5, MIN_DEPTH, SEAL);

        assertEq(vault.burnRootAcceptedAt(ROOT_2), block.timestamp);
        assertEq(vault.latestLezBlockId(), LEZ_BLOCK);
    }

    function test_acceptBurnRoot_belowMinDepthReverts() public {
        _proveRoot(_rootJournal(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH - 1));
        vm.expectRevert(FerrataVault.InsufficientLezDepth.selector);
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH - 1, SEAL);
        assertEq(vault.burnRootAcceptedAt(ROOT), 0);
    }

    function test_acceptBurnRoot_depthIncreaseDoesNotAffectAcceptedRoot() public {
        _deposit(alice, ETH, 1, keccak256("c"));
        _acceptRoot(ROOT); // at exactly MIN_DEPTH

        vm.prank(admin);
        vault.setMinLezDepth(MIN_DEPTH * 2);

        // A new root at the old depth is now refused ...
        _proveRoot(_rootJournal(ROOT_2, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH));
        vm.expectRevert(FerrataVault.InsufficientLezDepth.selector);
        vault.acceptBurnRoot(ROOT_2, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);

        // ... but the root accepted under the old depth still releases.
        _release(_request(ETH, 1, recipient, ROOT, keccak256("n")));
        assertEq(recipient.balance, 1 ether);
    }

    function test_acceptBurnRoot_withoutValidProofReverts() public {
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
    }

    function test_acceptBurnRoot_releaseProgramCannotStandInForRootProgram() public {
        // The right journal, proven by the wrong program.
        verifier.accept(
            RELEASE_IMAGE_ID, sha256(_rootJournalBytes(_rootJournal(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH)))
        );
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
    }

    function test_rejectingVerifier_rejectsRootsAndReleases() public {
        RejectingVerifier rejecting = new RejectingVerifier();
        FerrataVault strict = _deployVault(IRiscZeroVerifier(address(rejecting)), LEZ_CONFIG_ID);
        vm.expectRevert(RejectingVerifier.AlwaysRejects.selector);
        strict.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);

        // For the release path: accept a root first, then put the rejecting code at the verifier's address.
        _deposit(alice, ETH, 1, keccak256("c"));
        _acceptRoot(ROOT);
        vm.etch(address(verifier), address(rejecting).code);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("n"));
        vm.expectRevert(RejectingVerifier.AlwaysRejects.selector);
        vault.release(r, SEAL);
        assertFalse(vault.nullifierUsed(r.nullifier));
    }
}

contract ReleaseTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root 1");
    bytes32 internal constant ROOT_2 = keccak256("burn root 2");
    bytes32 internal constant N1 = keccak256("nullifier 1");
    bytes32 internal constant N2 = keccak256("nullifier 2");
    bytes32 internal constant C1 = keccak256("commitment 1");
    bytes32 internal constant C2 = keccak256("commitment 2");

    /// @dev The complete revert data of a blocked re-entry: the guard's selector and nothing else.
    function _reentrantCallError() internal pure returns (bytes memory) {
        return abi.encodeWithSelector(ReentrancyGuardTransient.ReentrancyGuardReentrantCall.selector);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Happy paths
    // ------------------------------------------------------------------------------------------------------------

    function test_release_eth() public {
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        _proveRelease(_releaseJournal(r));

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Release(N1, ETH, recipient, 1 ether, relayer, 0, ROOT);
        vm.prank(relayer);
        vault.release(r, SEAL);

        assertEq(recipient.balance, 1 ether);
        assertEq(relayer.balance, 0);
        assertEq(address(vault).balance, 0);
        assertEq(vault.escrowed(ETH), 0);
        assertTrue(vault.nullifierUsed(N1));
    }

    function test_release_sixDecimalToken() public {
        _assertTokenRelease(address(usdc), 0);
    }

    function test_release_eighteenDecimalToken() public {
        _assertTokenRelease(address(dai), 2);
    }

    function test_release_usdtStyleTokenWithoutReturnValue() public {
        _assertTokenRelease(address(usdt), 1);
    }

    function _assertTokenRelease(address token, uint8 denomIndex) internal {
        uint256 amount = _amount(token, denomIndex);
        _deposit(alice, token, denomIndex, C1);
        _acceptRoot(ROOT);
        _release(_request(token, denomIndex, recipient, ROOT, N1));

        assertEq(MockTokenBase(token).balanceOf(recipient), amount);
        assertEq(MockTokenBase(token).balanceOf(address(vault)), 0);
        assertEq(vault.escrowed(token), 0);
        assertTrue(vault.nullifierUsed(N1));
    }

    function test_release_worksForDelistedToken() public {
        _deposit(alice, address(dai), 0, C1);
        vm.prank(admin);
        vault.setDepositsEnabled(address(dai), false);

        dai.mint(bob, 100e18);
        vm.prank(bob);
        dai.approve(address(vault), 100e18);
        vm.prank(bob);
        vm.expectRevert(FerrataVault.DepositsDisabled.selector);
        vault.deposit(address(dai), 0, C2, 0);

        _acceptRoot(ROOT);
        _release(_request(address(dai), 0, recipient, ROOT, N1));
        assertEq(dai.balanceOf(recipient), 100e18);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Nullifiers, roots, delay
    // ------------------------------------------------------------------------------------------------------------

    function test_release_sameNullifierTwiceReverts() public {
        _deposit(alice, ETH, 1, C1);
        _deposit(bob, ETH, 1, C2);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        _release(r);

        vm.prank(relayer);
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r, SEAL);

        // Even a fresh, valid proof for the same nullifier under another root and to another recipient fails.
        _acceptRoot(ROOT_2);
        FerrataVault.ReleaseRequest memory again = _request(ETH, 1, bob, ROOT_2, N1);
        _proveRelease(_releaseJournal(again));
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(again, SEAL);

        assertEq(vault.escrowed(ETH), 1 ether);
        assertEq(recipient.balance, 1 ether);
        assertEq(bob.balance, 0);
    }

    function test_release_unknownRootReverts() public {
        _deposit(alice, ETH, 1, C1);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        _proveRelease(_releaseJournal(r)); // a valid release proof does not help without the root
        vm.expectRevert(FerrataVault.UnknownBurnRoot.selector);
        vault.release(r, SEAL);
    }

    function test_release_waitsForReleaseDelay() public {
        vm.prank(admin);
        vault.setReleaseDelay(ETH, 1 days);
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        uint256 acceptedAt = vault.burnRootAcceptedAt(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        _proveRelease(_releaseJournal(r));

        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r, SEAL);
        vm.warp(acceptedAt + 1 days - 1);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r, SEAL);

        vm.warp(acceptedAt + 1 days);
        vault.release(r, SEAL);
        assertEq(recipient.balance, 1 ether);
    }

    function test_release_delayIsPerToken() public {
        vm.prank(admin);
        vault.setReleaseDelay(address(dai), 1 days);
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        _release(_request(ETH, 1, recipient, ROOT, N1)); // ETH has no delay
        assertEq(recipient.balance, 1 ether);
    }

    function test_release_delayIsReadAtReleaseTime() public {
        // A delay set after a root was accepted applies to releases against that root (circuit breaker).
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        vm.prank(admin);
        vault.setReleaseDelay(ETH, 1 hours);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r, SEAL);
        vm.warp(block.timestamp + 1 hours);
        vault.release(r, SEAL);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Relay fee
    // ------------------------------------------------------------------------------------------------------------

    function test_release_relayFeeMustBeZeroOrStandard() public {
        uint256 fee = 0.002 ether;
        vm.prank(admin);
        vault.setStandardRelayFee(ETH, 1, fee);
        for (uint256 i = 0; i < 4; i++) {
            _deposit(alice, ETH, 1, C1);
        }
        _acceptRoot(ROOT);

        uint256[3] memory wrongFees = [fee - 1, fee + 1, 0.5 ether];
        for (uint256 i = 0; i < wrongFees.length; i++) {
            FerrataVault.ReleaseRequest memory bad = _request(ETH, 1, recipient, ROOT, N1);
            bad.relayFee = wrongFees[i];
            _proveRelease(_releaseJournal(bad)); // even a valid proof cannot carry a non-standard fee
            vm.expectRevert(FerrataVault.InvalidRelayFee.selector);
            vault.release(bad, SEAL);
        }

        FerrataVault.ReleaseRequest memory standard = _request(ETH, 1, recipient, ROOT, N1);
        standard.relayFee = fee;
        _release(standard);
        _release(_request(ETH, 1, recipient, ROOT, N2)); // zero fee
        assertEq(recipient.balance, 2 ether - fee);
        assertEq(relayer.balance, fee);
    }

    function test_release_nonZeroFeeRejectedWhenNoStandardFeeIsSet() public {
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        r.relayFee = 1;
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.InvalidRelayFee.selector);
        vault.release(r, SEAL);
    }

    function test_release_feeGoesToMsgSenderWhenFeeRecipientIsUnset() public {
        uint256 fee = 0.002 ether;
        vm.prank(admin);
        vault.setStandardRelayFee(ETH, 1, fee);
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        r.relayFee = fee;
        _proveRelease(_releaseJournal(r));

        address anyRelayer = makeAddr("anyRelayer");
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Release(N1, ETH, recipient, 1 ether, anyRelayer, fee, ROOT);
        vm.prank(anyRelayer);
        vault.release(r, SEAL);

        assertEq(anyRelayer.balance, fee);
        assertEq(recipient.balance, 1 ether - fee);
    }

    function test_release_feeGoesToBoundFeeRecipient() public {
        uint256 fee = 0.002 ether;
        address feeTaker = makeAddr("feeTaker");
        vm.prank(admin);
        vault.setStandardRelayFee(ETH, 1, fee);
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        r.relayFee = fee;
        r.feeRecipient = feeTaker;
        _release(r); // submitted by `relayer`, who is not the bound fee recipient

        assertEq(feeTaker.balance, fee);
        assertEq(relayer.balance, 0);
        assertEq(recipient.balance, 1 ether - fee);
    }

    function test_release_erc20FeeSplit() public {
        uint256 fee = 1e6; // 1 USDC on a 1000 USDC note
        vm.prank(admin);
        vault.setStandardRelayFee(address(usdc), 1, fee);
        _deposit(alice, address(usdc), 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(address(usdc), 1, recipient, ROOT, N1);
        r.relayFee = fee;
        _release(r);

        assertEq(usdc.balanceOf(recipient), 1000e6 - fee);
        assertEq(usdc.balanceOf(relayer), fee);
        assertEq(usdc.balanceOf(address(vault)), 0);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Other checks
    // ------------------------------------------------------------------------------------------------------------

    function test_release_zeroRecipientReverts() public {
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, address(0), ROOT, N1);
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.ZeroRecipient.selector);
        vault.release(r, SEAL);
    }

    function test_release_unregisteredTokenReverts() public {
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        _acceptRoot(ROOT);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.release(_request(address(other), 0, recipient, ROOT, N1), SEAL);
    }

    function test_release_invalidDenominationIndexReverts() public {
        _acceptRoot(ROOT);
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.release(_request(ETH, 3, recipient, ROOT, N1), SEAL);
    }

    function test_release_cannotExceedEscrow() public {
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        _proveRelease(_releaseJournal(r));

        vm.expectRevert(FerrataVault.InsufficientEscrow.selector);
        vault.release(r, SEAL); // nothing escrowed

        _deposit(alice, ETH, 0, C1); // 0.1 ETH
        vm.deal(address(vault), address(vault).balance + 5 ether); // forced ETH is not escrow
        vm.expectRevert(FerrataVault.InsufficientEscrow.selector);
        vault.release(r, SEAL);

        _deposit(alice, ETH, 1, C2); // 1.1 ETH escrowed now
        vault.release(r, SEAL);
        assertEq(vault.escrowed(ETH), 0.1 ether);
    }

    function test_release_escrowIsPerToken() public {
        _deposit(alice, address(dai), 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(address(usdc), 1, recipient, ROOT, N1);
        _proveRelease(_releaseJournal(r));
        usdc.mint(address(vault), 1000e6); // a direct transfer is not escrow either
        vm.expectRevert(FerrataVault.InsufficientEscrow.selector);
        vault.release(r, SEAL);
    }

    function test_release_rootProgramCannotStandInForReleaseProgram() public {
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, N1);
        verifier.accept(ROOT_IMAGE_ID, sha256(_releaseJournalBytes(_releaseJournal(r))));
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(r, SEAL);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Hostile recipients and tokens
    // ------------------------------------------------------------------------------------------------------------

    function test_release_recipientRejectingEthOnlyFailsItsOwnRelease() public {
        RejectingRecipient rejecting = new RejectingRecipient();
        _deposit(alice, ETH, 1, C1);
        _deposit(bob, ETH, 1, C2);
        _acceptRoot(ROOT);

        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, address(rejecting), ROOT, N1);
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.EthTransferFailed.selector);
        vault.release(r, SEAL);

        assertFalse(vault.nullifierUsed(N1)); // the entitlement is intact
        assertEq(vault.escrowed(ETH), 2 ether);

        _release(_request(ETH, 1, recipient, ROOT, N2)); // nobody else is blocked
        assertEq(recipient.balance, 1 ether);
    }

    function test_release_reentrantRecipientIsBlocked() public {
        ReentrantRecipient attacker = new ReentrantRecipient();
        _deposit(alice, ETH, 1, C1);
        _deposit(bob, ETH, 1, C2);
        _acceptRoot(ROOT);

        // A second, independently valid release, which the attacker tries to run from inside the first payout.
        FerrataVault.ReleaseRequest memory inner = _request(ETH, 1, address(attacker), ROOT, N2);
        _proveRelease(_releaseJournal(inner));
        attacker.setHook(address(vault), abi.encodeCall(FerrataVault.release, (inner, SEAL)));

        _release(_request(ETH, 1, address(attacker), ROOT, N1));

        assertTrue(attacker.hookCalled());
        assertFalse(attacker.hookSucceeded());
        assertEq(attacker.hookReturnData(), _reentrantCallError());
        assertEq(address(attacker).balance, 1 ether);
        assertTrue(vault.nullifierUsed(N1));
        assertFalse(vault.nullifierUsed(N2));
        assertEq(vault.escrowed(ETH), 1 ether);

        // Outside the payout the second release is fine.
        vault.release(inner, SEAL);
        assertEq(address(attacker).balance, 2 ether);
    }

    function test_release_reentrantRecipientCannotDeposit() public {
        ReentrantRecipient attacker = new ReentrantRecipient();
        _deposit(alice, ETH, 1, C1);
        _acceptRoot(ROOT);
        attacker.setHook(address(vault), abi.encodeCall(FerrataVault.depositETH, (1, C2, 0)));

        _release(_request(ETH, 1, address(attacker), ROOT, N1));

        assertTrue(attacker.hookCalled());
        assertEq(attacker.hookReturnData(), _reentrantCallError());
        assertEq(vault.nextLeafIndex(), 1);
    }

    function test_deposit_reentrantTokenIsBlocked() public {
        MockReentrantToken token = new MockReentrantToken();
        vm.prank(admin);
        vault.registerToken(address(token), _denoms(1e18, 2e18, 3e18), 1e12, 1, _unlimited(), _unlimited());
        token.mint(alice, 1e18);
        vm.prank(alice);
        token.approve(address(vault), 1e18);
        token.setHook(address(vault), abi.encodeCall(FerrataVault.deposit, (address(token), 0, C2, 0)));

        vm.prank(alice);
        vault.deposit(address(token), 0, C1, 0);

        assertTrue(token.hookCalled());
        assertFalse(token.hookSucceeded());
        assertEq(token.hookReturnData(), _reentrantCallError());
        assertEq(vault.nextLeafIndex(), 1);
        assertEq(vault.escrowed(address(token)), 1e18);
    }

    function test_release_reentrantTokenIsBlocked() public {
        MockReentrantToken token = new MockReentrantToken();
        vm.prank(admin);
        vault.registerToken(address(token), _denoms(1e18, 2e18, 3e18), 1e12, 1, _unlimited(), _unlimited());
        _deposit(alice, address(token), 0, C1);
        _deposit(bob, address(token), 0, C2);
        _acceptRoot(ROOT);

        FerrataVault.ReleaseRequest memory inner = _request(address(token), 0, recipient, ROOT, N2);
        _proveRelease(_releaseJournal(inner));
        token.setHook(address(vault), abi.encodeCall(FerrataVault.release, (inner, SEAL)));

        _release(_request(address(token), 0, recipient, ROOT, N1));

        assertTrue(token.hookCalled());
        assertEq(token.hookReturnData(), _reentrantCallError());
        assertEq(token.balanceOf(recipient), 1e18);
        assertFalse(vault.nullifierUsed(N2));
        assertEq(vault.escrowed(address(token)), 1e18);
    }
}
