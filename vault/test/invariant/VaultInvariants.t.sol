// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {StdInvariant} from "forge-std/StdInvariant.sol";
import {console2} from "forge-std/console2.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {ReferenceMerkle} from "../utils/ReferenceMerkle.sol";
import {VaultTestBase} from "../utils/VaultTestBase.sol";
import {VaultHandler} from "./VaultHandler.sol";

contract VaultInvariants is VaultTestBase {
    /// The fields of a finalised checkpoint the invariants need.
    struct History {
        bytes32 parentState;
        bytes32 newState;
        uint64 importedLeafCount;
        bool sunset;
        bool revoked;
        uint64 parentImportedLeafCount;
        bool parentSunset;
        uint64 finalizedAt;
        address finalizer;
    }

    VaultHandler internal handler;

    /// @dev The vault under test: the base rule (A) vault; VaultInvariantsB swaps in a rule (B) one.
    function _useVault() internal virtual {}

    function setUp() public override {
        super.setUp();
        _useVault();

        vm.startPrank(admin);
        // Finite caps, so the rate limiter takes part (the handler also changes them).
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, 40 ether, 0.01 ether);
        vault.setTokenCap(address(dai), FerrataVault.Direction.Release, 40_000e18, 10e18);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, 500_000e6, 100e6);
        vault.setGlobalCap(FerrataVault.Direction.Release, 500_000e6, 100e6);
        vm.stopPrank();

        address[] memory tokens = new address[](4);
        tokens[0] = ETH;
        tokens[1] = address(usdc);
        tokens[2] = address(dai);
        tokens[3] = address(usdt);
        handler = new VaultHandler(vault, verifier, admin, freezer, tokens, uint64(block.timestamp));
        // v5 (D13): every token starts with a fee change announced and due, so fees take part from the first call.
        for (uint256 i = 0; i < tokens.length; i++) {
            handler.announceFees(i, 7 + 10 * i);
        }
        vm.warp(block.timestamp + FEE_NOTICE);

        bytes4[] memory selectors = new bytes4[](26);
        selectors[0] = VaultHandler.deposit.selector;
        selectors[1] = VaultHandler.submitCandidate.selector;
        selectors[2] = VaultHandler.finalizePending.selector;
        selectors[3] = VaultHandler.revoke.selector;
        selectors[4] = VaultHandler.retire.selector;
        selectors[5] = VaultHandler.refund.selector;
        selectors[6] = VaultHandler.replayRefund.selector;
        selectors[7] = VaultHandler.release.selector;
        selectors[8] = VaultHandler.replayNullifier.selector;
        selectors[9] = VaultHandler.togglePayee.selector;
        selectors[10] = VaultHandler.donate.selector;
        selectors[11] = VaultHandler.warp.selector;
        selectors[12] = VaultHandler.setFreeze.selector;
        selectors[13] = VaultHandler.setCap.selector;
        selectors[14] = VaultHandler.setReleaseDelay.selector;
        selectors[15] = VaultHandler.submitBreakingCandidate.selector;
        selectors[16] = VaultHandler.finalizeRejected.selector;
        selectors[17] = VaultHandler.cancelPending.selector;
        selectors[18] = VaultHandler.finalizeEarly.selector;
        selectors[19] = VaultHandler.watcherPause.selector;
        selectors[20] = VaultHandler.proposeRoles.selector;
        selectors[21] = VaultHandler.applyRoles.selector;
        selectors[22] = VaultHandler.claim.selector;
        selectors[23] = VaultHandler.burn.selector;
        selectors[24] = VaultHandler.announceFees.selector;
        selectors[25] = VaultHandler.applyFees.selector;
        targetContract(address(handler));
        targetSelector(StdInvariant.FuzzSelector({addr: address(handler), selectors: selectors}));
    }

    function invariant_escrowEqualsDepositsMinusReleasesMinusRefunds() public view {
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            address t = tokens[i];
            assertEq(vault.escrowed(t), handler.ghostDeposited(t) - handler.ghostReleased(t) - handler.ghostRefunded(t));
        }
    }

    /// @notice D13 / proposal §B4 (Security #6): the wrapped supply on LEZ equals escrow less unminted deposits and
    ///         unreleased burns. The supply is the LEZ model's: claims mint what the attested fee schedule of the leaf
    ///         and the registered relay fee say, never what the vault says it escrowed; burns take one denomination.
    function invariant_supplyIsEscrowLessUnmintedDepositsAndUnreleasedBurns() public view {
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            address t = tokens[i];
            assertEq(handler.ghostSupply(t), vault.escrowed(t) - handler.ghostUnminted(t) - handler.ghostUnreleased(t));
        }
    }

    /// @notice D13: every deposit escrowed, every release paid and credited, every refund returned exactly what the
    ///         fee model says; every published schedule hash is the model's preimage; no claimed deposit was refunded.
    function invariant_feesFollowTheAnnouncedSchedules() public view {
        assertEq(handler.ghostFeeMismatches(), 0);
        assertEq(handler.ghostPublishedMismatches(), 0);
        assertEq(handler.ghostRefundsOfClaimedLeaves(), 0);
    }

    /// @dev An equality, not `balance >= escrowed + totalOwed`: a one-sided bound stays green when value is
    ///      stranded in the vault (paid out short, a fee neither paid nor credited), and the vault has no rescue path.
    function invariant_balanceIsEscrowPlusOwedPlusDonations() public view {
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            address t = tokens[i];
            assertEq(_balance(t, address(vault)), vault.escrowed(t) + vault.totalOwed(t) + handler.ghostDonated(t));
        }
    }

    function invariant_totalOwedIsTheSumOverPayees() public view {
        address[] memory tokens = handler.tokens();
        address[] memory payees = handler.payees();
        for (uint256 i = 0; i < tokens.length; i++) {
            uint256 sum;
            for (uint256 k = 0; k < payees.length; k++) {
                sum += vault.owed(payees[k], tokens[i]);
            }
            assertEq(vault.totalOwed(tokens[i]), sum);
        }
    }

    function invariant_usedNullifierIsNeverPaidAgain() public view {
        assertEq(handler.ghostDoubleSpends(), 0);
        bytes32[] memory used = handler.usedNullifiers();
        for (uint256 i = 0; i < used.length; i++) {
            assertTrue(vault.nullifierUsed(used[i]));
        }
        assertEq(used.length, handler.ghostReleaseCount());
    }

    function invariant_refundedLeafIsNeverPaidAgain() public view {
        assertEq(handler.ghostDoubleRefunds(), 0);
        uint64[] memory refunded = handler.refundedLeaves();
        for (uint256 i = 0; i < refunded.length; i++) {
            assertTrue(vault.refunded(refunded[i]));
        }
    }

    /// @notice finalState is genesis or a state that went through finalize, and it is the newest state of the valid
    ///         (non-revoked) history: the last valid checkpoint's newState, or genesis if there is none.
    function invariant_finalStateIsGenesisOrFinalised() public view {
        bytes32 state = vault.finalState();
        assertTrue(state == GENESIS_STATE || handler.ghostWentThroughFinalize(state));

        bytes32 expected = GENESIS_STATE;
        uint256 count = vault.checkpointCount();
        for (uint256 i = count; i > 0; i--) {
            (, bytes32 newState,,,,, bool revoked,,,,) = vault.checkpoints(i - 1);
            if (!revoked) {
                expected = newState;
                break;
            }
        }
        assertEq(state, expected);
    }

    /// @notice v3 D4: the valid (non-revoked) checkpoints form one chain from genesis; along it the imported leaf count
    ///         never decreases, and after the first sunset checkpoint every one is a sunset checkpoint with the same
    ///         count. Each records its parent's count and flag, and the vault's current ones are the chain's last.
    function invariant_validHistoryIsAMonotoneChain() public view {
        bytes32 state = GENESIS_STATE;
        uint64 imported = 0;
        bool sunset = false;
        uint256 count = vault.checkpointCount();
        for (uint256 i = 0; i < count; i++) {
            History memory h = _history(i);
            if (h.revoked) continue;
            assertEq(h.parentState, state);
            assertEq(h.parentImportedLeafCount, imported);
            assertEq(h.parentSunset, sunset);
            assertGe(h.importedLeafCount, imported);
            if (sunset) {
                assertTrue(h.sunset);
                assertEq(h.importedLeafCount, imported);
            }
            state = h.newState;
            imported = h.importedLeafCount;
            sunset = h.sunset;
        }
        assertEq(vault.finalState(), state);
        assertEq(vault.finalImportedLeafCount(), imported);
        assertEq(vault.finalSunset(), sunset);
    }

    /// @notice v3 D5: the refund boundary, and the time refunds open from, are those of the first valid sunset
    ///         checkpoint; without one there is no boundary.
    function invariant_sunsetBoundaryIsTheFirstValidSunsetCheckpoint() public view {
        uint256 count = vault.checkpointCount();
        for (uint256 i = 0; i < count; i++) {
            History memory h = _history(i);
            if (h.revoked || !h.sunset) continue;
            assertTrue(vault.sunsetLeafCountSet());
            assertEq(vault.sunsetLeafCount(), h.importedLeafCount);
            assertEq(vault.sunsetSetAt(), i);
            assertEq(vault.sunsetFinalizedAt(), h.finalizedAt);
            return;
        }
        assertFalse(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 0);
        assertEq(vault.sunsetFinalizedAt(), 0);
    }

    /// @notice v3 D4/D5, judged from the handler's own records: no candidate that breaks monotonicity or was
    ///         cancelled was finalised; no candidate was submitted or finalised under a checkpoint freeze; and a refund
    ///         was paid exactly when the vault was retired, releases were not globally frozen, the leaf was at or above
    ///         the first valid sunset checkpoint's count and that checkpoint was at least revokeWindow old (for a leaf
    ///         not yet refunded whose escrow suffices).
    function invariant_checkpointAndRefundRulesHold() public view {
        assertEq(handler.ghostRejectedFinalisations(), 0);
        assertEq(handler.ghostCheckpointCallsDuringFreeze(), 0);
        assertEq(handler.ghostRefundsAgainstTheRules(), 0);
        assertEq(handler.ghostRefundsWronglyRefused(), 0);
    }

    function invariant_leafCountAndRootMatchTheReference() public view {
        assertEq(vault.nextLeafIndex(), handler.ghostDepositCount());
        assertEq(vm.load(address(vault), bytes32(uint256(1))), bytes32(handler.ghostDepositCount()));
        bytes32 expected = ReferenceMerkle.root(handler.leaves());
        assertEq(vault.depositRoot(), expected);
        assertEq(vm.load(address(vault), bytes32(uint256(0))), expected);
        assertEq(vm.load(address(vault), bytes32(uint256(2))), LEZ_CONFIG_ID);
    }

    /// @notice v4 (D13): the freeze authority and the watchers change only through a proposal applied after its 14
    ///         days, and the checkpoint pause is the model's: the freeze authority's freezes, and watcher pauses that
    ///         reach 12 h ahead and never shorten a longer one.
    function invariant_rolesChangeOnlyThroughTheTimelock() public view {
        assertEq(handler.ghostRoleTimelockViolations(), 0);
        assertEq(vault.freezeAuthority(), handler.freezer());
        for (uint256 i = 0; i < 3; i++) {
            assertEq(vault.watchers(i), handler.watcherModel(i));
        }
        assertEq(vault.checkpointsFrozenUntil(), handler.checkpointsFrozenUntilModel());
    }

    /// @notice v4: no candidate is finalised before its wait; under rule (B) no beacon is ever recorded.
    function invariant_nothingFinalisedBeforeItsWait() public view {
        assertEq(handler.ghostEarlyFinalisations(), 0);
        if (vault.mode() == FerrataVault.Mode.B) assertEq(vault.beaconCount(), 0);
    }

    /// @dev Run summary, shown with -vv for the last run of each campaign. The run table counts the whole campaign.
    function afterInvariant() public view {
        console2.log("deposits", handler.ghostDepositCount());
        console2.log("releases", handler.ghostReleaseCount());
        uint256[8] memory counts = handler.summary();
        string[8] memory names = [
            "refunds",
            "finalisations",
            "sunset finalisations",
            "cancellations",
            "refused for monotonicity",
            "refused as cancelled",
            "revocations",
            "replays stopped by nullifier check"
        ];
        for (uint256 i = 0; i < counts.length; i++) {
            console2.log(names[i], counts[i]);
        }
        (uint256 claims, uint256 burns, uint256 feeChanges) = handler.feeSummary();
        console2.log("LEZ claims", claims);
        console2.log("LEZ burns", burns);
        console2.log("fee changes in force", feeChanges);
        (uint256 pauses, uint256 roleChanges) = handler.roleSummary();
        console2.log("watcher pauses", pauses);
        console2.log("role changes", roleChanges);
    }

    function _history(uint256 i) internal view returns (History memory h) {
        FerrataVault.FinalizedCheckpoint memory c = _checkpointRecord(vault, i);
        h = History(
            c.parentState,
            c.newState,
            c.importedLeafCount,
            c.sunset,
            c.revoked,
            c.parentImportedLeafCount,
            c.parentSunset,
            c.finalizedAt,
            c.finalizer
        );
    }
}
