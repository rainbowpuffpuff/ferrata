// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {StdInvariant} from "forge-std/StdInvariant.sol";
import {console2} from "forge-std/console2.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {AcceptAllVerifier} from "../mocks/AcceptAllVerifier.sol";
import {VaultTestBase} from "../utils/VaultTestBase.sol";
import {BrokenVerifierHandler} from "./BrokenVerifierHandler.sol";

/// @notice A vault whose verifier accepts every seal: AcceptAllVerifier behind RISC Zero's unmodified emergency stop,
///         whose owner is renounced as at a deployment. Finite release caps on three of four tokens (USDT has none) and globally,
///         release delays from the floor (1 h, DAI) to the revocation window (3 days, ETH), and a checkpoint fee of
///         0.1%, so forged releases credit the attacker as the checkpoints' prover.
abstract contract BrokenVerifierSetup is VaultTestBase {
    uint256 internal constant MIN_DELAY = 1 hours;
    uint256 internal constant GLOBAL_CAP = 300_000e6; // value units (1e-6 USD): 300,000 USD
    uint256 internal constant GLOBAL_REFILL = 10e6; // 10 USD a second, 36,000 an hour

    RiscZeroVerifierEmergencyStop internal brokenStop;
    address[] internal brokenTokens;
    uint256[] internal releaseCaps;
    uint256[] internal releaseRefills;
    uint256[] internal releaseDelays;

    function _mode() internal pure virtual returns (FerrataVault.Mode) {
        return FerrataVault.Mode.A;
    }

    /// @dev Replaces the base suite's vault with one whose verifier accepts every seal, behind the real emergency stop.
    function _deployBroken() internal {
        brokenStop = new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(address(new AcceptAllVerifier())), freezer);
        // As at a deployment, nobody holds the stop's key: only RISC Zero's proof of an exploit trips it.
        vm.prank(freezer);
        brokenStop.renounceOwnership();
        IRiscZeroVerifier v = IRiscZeroVerifier(address(brokenStop));
        FerrataVault.Deployment memory dep =
            _mode() == FerrataVault.Mode.A ? _deployment(v, LEZ_CONFIG_ID) : _deploymentB(v, LEZ_CONFIG_ID);
        dep.minReleaseDelay = MIN_DELAY;
        dep.globalReleaseCap = _cap(GLOBAL_CAP, GLOBAL_REFILL);
        vault = new FerrataVault(dep);

        address[] memory tokens = new address[](4);
        tokens[0] = ETH;
        tokens[1] = address(usdc);
        tokens[2] = address(dai);
        tokens[3] = address(usdt);
        uint256[] memory caps = new uint256[](4);
        uint256[] memory refills = new uint256[](4);
        uint256[] memory delays = new uint256[](4);
        (caps[0], refills[0], delays[0]) = (15 ether, 0.002 ether, REVOKE_WINDOW); // 7.2 ETH an hour; delay = window
        (caps[1], refills[1], delays[1]) = (25_000e6, 2e6, 1 days);
        (caps[2], refills[2], delays[2]) = (40_000e18, 10e18, MIN_DELAY);
        (caps[3], refills[3], delays[3]) = (UNLIMITED, 0, 12 hours); // no token cap: the global cap and escrow only

        vm.startPrank(admin);
        vault.registerToken(
            ETH, _brokenParams(_denoms(0.1 ether, 1 ether, 10 ether), 1e12, 3000, caps[0], refills[0], delays[0])
        );
        vault.registerToken(
            address(usdc), _brokenParams(_denoms(100e6, 1000e6, 10_000e6), 1, 1, caps[1], refills[1], delays[1])
        );
        vault.registerToken(
            address(dai), _brokenParams(_denoms(100e18, 1000e18, 10_000e18), 1e12, 1, caps[2], refills[2], delays[2])
        );
        vault.registerToken(
            address(usdt), _brokenParams(_denoms(100e6, 1000e6, 10_000e6), 1, 1, caps[3], refills[3], delays[3])
        );
        vm.stopPrank();
        (brokenTokens, releaseCaps, releaseRefills, releaseDelays) = (tokens, caps, refills, delays);
    }

    /// @dev Default registration (relay fee 0.2%) with a release cap, a release delay and a checkpoint fee of 0.1%.
    function _brokenParams(
        uint256[] memory denoms,
        uint256 lezUnit,
        uint256 unitWeight,
        uint256 cap,
        uint256 refill,
        uint256 delay
    ) internal view returns (FerrataVault.TokenParams memory p) {
        p = _params(denoms, lezUnit, unitWeight);
        p.releaseCap = _cap(cap, refill);
        p.releaseDelay = delay;
        for (uint256 i = 0; i < denoms.length; i++) {
            p.checkpointFees[i] = denoms[i] / 1000;
        }
    }
}

/// @notice What the vault still guarantees when its proof verifier accepts every seal (docs/trust.md §9, vault/README.md
///         "Under a broken verifier"): the handler forges checkpoints and releases as fast as the vault lets it, while
///         deposits continue and the freeze authority, a watcher and the stop's guardian sometimes react.
contract BrokenVerifierInvariants is BrokenVerifierSetup {
    BrokenVerifierHandler internal handler;

    function setUp() public override {
        super.setUp();
        _deployBroken();
        handler = new BrokenVerifierHandler(
            vault, brokenStop, freezer, watcher, brokenTokens, releaseCaps, releaseRefills, releaseDelays
        );
        // Escrow from the first call: 24 honest deposits, six per token over its three denominations.
        for (uint256 i = 0; i < 24; i++) {
            handler.deposit(i, i, i);
        }

        bytes4[] memory selectors = new bytes4[](13);
        selectors[0] = BrokenVerifierHandler.deposit.selector;
        selectors[1] = BrokenVerifierHandler.forgeCandidate.selector;
        selectors[2] = BrokenVerifierHandler.finalizeForged.selector;
        selectors[3] = BrokenVerifierHandler.forgeRelease.selector;
        selectors[4] = BrokenVerifierHandler.replayNullifier.selector;
        selectors[5] = BrokenVerifierHandler.attackerClaimsOwed.selector;
        selectors[6] = BrokenVerifierHandler.refund.selector;
        selectors[7] = BrokenVerifierHandler.watcherCancels.selector;
        selectors[8] = BrokenVerifierHandler.freezerRevokes.selector;
        selectors[9] = BrokenVerifierHandler.freezerFreezes.selector;
        selectors[10] = BrokenVerifierHandler.freezerRetires.selector;
        selectors[11] = BrokenVerifierHandler.anyoneStops.selector;
        selectors[12] = BrokenVerifierHandler.warp.selector;
        targetContract(address(handler));
        targetSelector(StdInvariant.FuzzSelector({addr: address(handler), selectors: selectors}));
    }

    function _runsFile() internal pure returns (string memory) {
        return _runsFileOf(_mode());
    }

    /// @notice Per-token escrow: what left the vault in a token (releases and refunds) never exceeds what was deposited
    ///         in that token, escrow is exactly the difference, and the balance is escrow plus what is owed.
    function invariant_aTokenNeverPaysMoreThanWasDepositedOfIt() public view {
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            address t = tokens[i];
            uint256 out = handler.released(t) + handler.refundedAmount(t);
            assertLe(out, handler.deposited(t));
            assertEq(vault.escrowed(t), handler.deposited(t) - out);
            assertEq(_balance(t, address(vault)), vault.escrowed(t) + vault.totalOwed(t));
        }
    }

    /// @notice The token buckets: releases and refunds of a capped token in any window [t_i, t_j] total at most
    ///         capacity + refill * (t_j - t_i), and so do all tokens together in value units under the global cap.
    function invariant_payoutsInAnyWindowStayWithinTheCaps() public view {
        assertEq(handler.ghostCapViolations(), 0);
    }

    /// @notice No release pays unless a non-revoked checkpoint finalised its root and the token's release delay has
    ///         passed since (one second more when the delay equals the revocation window): never under a revoked
    ///         checkpoint, a cancelled or pending candidate, or a root nobody finalised.
    function invariant_noReleaseWithoutAMaturedUnrevokedCheckpoint() public view {
        assertEq(handler.ghostReleaseUnaccepted(), 0);
    }

    /// @notice A forged candidate is final no earlier than its wait W after submission; a revocation only inside the
    ///         revocation window.
    function invariant_forgedCheckpointsKeepTheirClocks() public view {
        assertEq(handler.ghostEarlyFinalisations(), 0);
        assertEq(handler.ghostRevocationsOutsideWindow(), 0);
    }

    /// @notice After RISC Zero's emergency stop: no candidate, no finalisation, no release and no refund.
    function invariant_nothingAfterTheEmergencyStop() public view {
        assertEq(handler.ghostAfterStop(), 0);
        if (handler.stopped()) assertTrue(brokenStop.paused());
    }

    /// @notice No release during a global or per-token release freeze (no refund during a global one).
    function invariant_noReleaseDuringAFreeze() public view {
        assertEq(handler.ghostReleaseWhileFrozen(), 0);
    }

    /// @notice A nullifier pays once, even with a seal the verifier accepts.
    function invariant_aNullifierPaysOnce() public view {
        assertEq(handler.ghostDoubleSpends(), 0);
    }

    /// @notice Refunds only after retirement and a sunset checkpoint the revocation window has passed, for leaves at or
    ///         above its count, once each, the leaf's whole escrow, paid to the leaf's depositor; releases pay exactly a
    ///         denomination.
    function invariant_refundsAndAmountsFollowTheRules() public view {
        assertEq(handler.ghostRefundsAgainstRules(), 0);
        assertEq(handler.ghostAmountMismatches(), 0);
    }

    /// @dev After every run: with FERRATA_BROKEN_VERIFIER_RUNS=1 (and the directory made), one line per run in
    ///      ../target/broken-verifier/ (the campaign's totals, tools/broken_verifier_runs.py; not committed); and the run
    ///      summary (shown with -vv for the last run of each campaign).
    function afterInvariant() public {
        address[] memory ts = handler.tokens();
        string memory line = string.concat(
            vm.toString(handler.depositCount()),
            "\t",
            vm.toString(handler.forgedSubmitted()),
            "\t",
            vm.toString(handler.forgedFinalised()),
            "\t",
            vm.toString(handler.forgedSunsets()),
            "\t",
            vm.toString(handler.cancellations()),
            "\t",
            vm.toString(handler.revocations()),
            "\t",
            vm.toString(handler.releaseCount()),
            "\t",
            vm.toString(handler.refundCount()),
            "\t",
            vm.toString(uint256(handler.stopped() ? 1 : 0)),
            "\t",
            vm.toString(handler.maxGlobalCapUseBps())
        );
        for (uint256 i = 0; i < ts.length; i++) {
            line = string.concat(
                line,
                "\t",
                vm.toString(handler.deposited(ts[i])),
                "\t",
                vm.toString(handler.released(ts[i])),
                "\t",
                vm.toString(handler.refundedAmount(ts[i])),
                "\t",
                vm.toString(handler.maxUse(ts[i]))
            );
        }
        if (vm.envOr("FERRATA_BROKEN_VERIFIER_RUNS", false)) vm.writeLine(_runsFile(), line);
        console2.log("honest deposits", handler.depositCount());
        console2.log("forged candidates", handler.forgedSubmitted());
        console2.log("forged checkpoints finalised", handler.forgedFinalised());
        console2.log("  of them sunset", handler.forgedSunsets());
        console2.log("cancellations", handler.cancellations());
        console2.log("revocations", handler.revocations());
        console2.log("forged releases paid", handler.releaseCount());
        console2.log("refunds paid", handler.refundCount());
        console2.log("emergency stop tripped", handler.stopped());
        console2.log("refused: root not accepted", handler.refusals(FerrataVault.BurnRootNotAccepted.selector));
        console2.log("refused: release delay", handler.refusals(FerrataVault.ReleaseDelayNotElapsed.selector));
        console2.log("refused: token cap", handler.refusals(FerrataVault.TokenReleaseCapExceeded.selector));
        console2.log("refused: global cap", handler.refusals(FerrataVault.GlobalReleaseCapExceeded.selector));
        console2.log("refused: escrow", handler.refusals(FerrataVault.InsufficientEscrow.selector));
        console2.log("refused: frozen", handler.refusals(FerrataVault.ReleasesAreFrozen.selector));
        console2.log("refused: nullifier used", handler.refusals(FerrataVault.NullifierAlreadyUsed.selector));
        console2.log("refused: stopped (EnforcedPause)", handler.refusals(bytes4(keccak256("EnforcedPause()"))));
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            console2.log("token", i, "deposited", handler.deposited(tokens[i]));
            console2.log("  taken by forged releases", handler.released(tokens[i]));
            console2.log("  fullest window, bps of the cap bound", handler.maxUse(tokens[i]));
        }
        console2.log("global: fullest window, bps of the bound", handler.maxGlobalCapUseBps());
    }
}

/// @dev Where afterInvariant appends its line: one file per rule.
function _runsFileOf(FerrataVault.Mode mode) pure returns (string memory) {
    return mode == FerrataVault.Mode.A ? "../target/broken-verifier/runs-A.tsv" : "../target/broken-verifier/runs-B.tsv";
}

/// @notice The same attacker and invariants on a rule (B) vault: a forged candidate is final once deltaAtSubmit (1 h)
///         has passed, with no beacon and no second proof.
contract BrokenVerifierInvariantsB is BrokenVerifierInvariants {
    function _mode() internal pure override returns (FerrataVault.Mode) {
        return FerrataVault.Mode.B;
    }
}
