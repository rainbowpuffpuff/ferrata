// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {DeployVault} from "../script/DeployVault.s.sol";
import {FerrataVault} from "../src/FerrataVault.sol";

import {MockERC20} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice What script/DeployVault.s.sol would deploy, without sending anything: its own `_deployment` and token
///         parameters, exposed.
contract DeployVaultExposed is DeployVault {
    function init_() external {
        _init();
    }

    function deployment_(address stop) external view returns (FerrataVault.Deployment memory) {
        return _deployment(stop);
    }

    function ethParams_() external view returns (FerrataVault.TokenParams memory) {
        return _ethParams();
    }

    function tokenParams_() external view returns (FerrataVault.TokenParams memory) {
        return _tokenParams();
    }

    function capsAdmitTheFloor_(FerrataVault.TokenParams[2] memory ps, FerrataVault.CapParams memory floor)
        external
        pure
        returns (bool)
    {
        return _capsAdmitTheFloor(ps, floor);
    }

    function floor_() external view returns (FerrataVault.CapParams memory) {
        return _daily(_releaseFloorDaily());
    }

    /// @dev What `_init` read from the environment, set by the tests instead (vm.setEnv is process-wide and tests run
    ///      in parallel).
    function set_(FerrataVault.Mode m, bool uncapped_, uint256 deltaGiven_) external {
        mode = m;
        if (m == FerrataVault.Mode.A) modeName = "A";
        else modeName = "B";
        uncapped = uncapped_;
        deltaGiven = deltaGiven_;
    }
}

/// @notice RFP-022 Sec.4 ("Caps bound the maximum value at risk in any rolling window; proposals must document
///         recommended defaults"): the deployment script's default is capped. A vault built from exactly what the
///         script would send (its own `Deployment` and registrations, on the default params) refuses the deposit
///         that would pass its token's daily ceiling (100 times the largest denomination, counted on each deposit's
///         whole payment), refills over a day, and keeps every release cap above an immutable floor; the uncapped
///         deployment of the earlier testnet runs needs FERRATA_UNCAPPED=1. RFP-022 F.12: the wait Δ, the outbound
///         finality depth, is the design's for the rule by default (3 h 01 min under (A), 1 h under (B)); FERRATA_DELTA
///         sets another for a test run.
contract DeployDefaultsTest is VaultTestBase {
    DeployVaultExposed internal script;
    FerrataVault internal deployed;
    MockERC20 internal testToken;

    function _deploy(bool uncapped) internal {
        _deploy(FerrataVault.Mode.B, uncapped, 0);
    }

    /// @dev The script as `_init` leaves it, with the rule, the caps and FERRATA_DELTA (0: not given) set directly.
    function _script(FerrataVault.Mode m, bool uncapped, uint256 deltaGiven) internal {
        vm.setEnv("FERRATA_MODE", "B"); // _init requires it; every test sets the same value
        script = new DeployVaultExposed();
        script.init_(); // the deployer (admin) is this test contract
        script.set_(m, uncapped, deltaGiven);
    }

    function _deploy(FerrataVault.Mode m, bool uncapped, uint256 deltaGiven) internal {
        _script(m, uncapped, deltaGiven);
        deployed = new FerrataVault(script.deployment_(address(estop)));
        testToken = new MockERC20("Test", "TT", 6);
        deployed.registerToken(ETH, script.ethParams_());
        deployed.registerToken(address(testToken), script.tokenParams_());
    }

    function test_deploymentDefault_isCappedAndRefusesAboveTheCeiling() public {
        _deploy(false);
        // Per token and day: 100 times the largest denomination; refilled over a day.
        (uint256 capacity, uint256 refill,) = deployed.tokenCap(ETH, FerrataVault.Direction.Deposit);
        assertEq(capacity, 10 ether, "ETH: 100 x 0.1 ETH a day");
        assertEq(refill, (uint256(10 ether) + 1 days - 1) / 1 days);
        (capacity,,) = deployed.tokenCap(ETH, FerrataVault.Direction.Release);
        assertEq(capacity, 10 ether, "the same ceiling for releases");
        (capacity,,) = deployed.tokenCap(address(testToken), FerrataVault.Direction.Deposit);
        assertEq(capacity, 1_000_000e6, "test token: 100 x 10,000 a day");
        // Globally: the two ceilings' values (ETH: 10 ETH / 1e12 x 3000; the token: 1e12 / 1 x 1).
        (capacity,,) = deployed.globalCap(FerrataVault.Direction.Deposit);
        assertEq(capacity, 30_000_000_000 + 1_000_000_000_000);
        (capacity,,) = deployed.globalCap(FerrataVault.Direction.Release);
        assertEq(capacity, 30_000_000_000 + 1_000_000_000_000);
        // The immutable release floor: the largest single exit a day in value units, a refund of a 10,000-token leaf
        // (10,000 and its 25-token relay fee; ETH's, 0.1 ETH and 0.0002 ETH, is worth 300,600,000).
        assertEq(deployed.releaseCapFloor(), 10_025_000_000);
        assertEq(
            deployed.releaseRefillFloor(), (uint256(10_025_000_000) + 1 days - 1) / 1 days, "rounded up: whole in a day"
        );

        // A deposit counts its whole payment (0.1 ETH and its 0.2 % relay fee) against the bucket: 99 of 0.1 ETH fill
        // the day; the 100th is refused; a day later the bucket is whole again.
        address depositor = address(0xD5);
        uint256 total = _depositTotal(deployed, ETH, 2);
        uint256 fits = uint256(10 ether) / total;
        assertEq(fits, 99, "0.1002 ETH each");
        for (uint256 i = 0; i < fits; i++) {
            _depositInto(deployed, depositor, ETH, 2, bytes32(i + 1));
        }
        vm.deal(depositor, total);
        uint64 next = deployed.depositCount(depositor);
        vm.prank(depositor);
        vm.expectRevert(FerrataVault.TokenDepositCapExceeded.selector);
        deployed.depositETH{value: total}(2, bytes32(fits + 1), next);
        vm.warp(block.timestamp + 1 days);
        (,, uint256 available) = deployed.tokenCap(ETH, FerrataVault.Direction.Deposit);
        assertEq(available, 10 ether, "refilled over a day");
        _depositInto(deployed, depositor, ETH, 2, bytes32(fits + 1));

        // No admin can cut a release cap below the floor; above it, it may.
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        deployed.setTokenCap(ETH, FerrataVault.Direction.Release, 0, 0);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        deployed.setGlobalCap(FerrataVault.Direction.Release, 10_024_999_999, 1 ether);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        deployed.setTokenCap(ETH, FerrataVault.Direction.Release, 0.1 ether, uint256(10 ether) / 1 days);
        deployed.setTokenCap(ETH, FerrataVault.Direction.Release, 4 ether, uint256(4 ether) / 1 days);
    }

    /// @notice Internal review of 27 November 2026, S-2: at the lowest release caps the floor allows, every exit still
    ///         passes. The floor exists so that no admin can throttle releases below it (the script's header); it must
    ///         therefore admit the largest single exit, a refund of the largest leaf (its denomination and relay fee)
    ///         being the largest. The default was one largest denomination of the token whose largest is worth least
    ///         (0.1 ETH, 300,000,000 value units): caps the floor allowed then left a 10,000-token release
    ///         (10,000,000,000 value units) and a 0.1 ETH refund (300,600,000) refused for good.
    function test_S2_atTheLowestCapsTheFloorAllowsEveryExitStillPasses() public {
        _deploy(false);
        uint256 floor = deployed.releaseCapFloor();
        uint256 refillFloor = deployed.releaseRefillFloor();
        // The admin sets the global and both tokens' release buckets as low as the floor allows.
        deployed.setGlobalCap(FerrataVault.Direction.Release, floor, refillFloor);
        address[2] memory tokens = [ETH, address(testToken)];
        for (uint256 i = 0; i < 2; i++) {
            (,, uint256 lezUnit, uint256 weight, uint256[] memory denoms) = deployed.tokenConfig(tokens[i]);
            uint256 capacity = (floor * lezUnit + weight - 1) / weight;
            uint256 refill = (refillFloor * lezUnit + weight - 1) / weight;
            deployed.setTokenCap(tokens[i], FerrataVault.Direction.Release, capacity, refill);
            // The largest exit of the token: a refund of its largest leaf, denomination and relay fee.
            uint256 largest = denoms[denoms.length - 1] + _relayFeesOf(deployed, tokens[i])[denoms.length - 1];
            (,, uint256 tokenAvailable) = deployed.tokenCap(tokens[i], FerrataVault.Direction.Release);
            assertLe(largest, tokenAvailable, "the token's lowest release bucket admits its largest exit");
            assertLe(largest * weight / lezUnit, floor, "the global floor admits the token's largest exit");
        }

        // And as transactions: the largest test-token denomination, deposited, burned and released under those caps.
        _depositInto(deployed, alice, address(testToken), 2, keccak256("the largest leaf"));
        bytes32 root = keccak256("a burn root with one 10,000-token burn");
        FerrataVault.Checkpoint memory c = _nextCheckpoint(deployed, root, 1, false);
        verifier.accept(
            deployed.checkpointImageId(),
            sha256(_checkpointJournalBytes(_checkpointJournal(deployed, deployed.lezConfigId(), c)))
        );
        deployed.submitCandidate(c, SEAL);
        vm.warp(block.timestamp + deployed.delta());
        deployed.finalizeAttested(c);
        vm.warp(block.timestamp + 121);
        FerrataVault.ReleaseRequest memory r = _request(address(testToken), 2, bob, root, keccak256("the burn's NOut"));
        verifier.accept(
            deployed.releaseImageId(),
            sha256(
                _releaseJournalBytes(
                    ReleaseJournal({
                        chainId: block.chainid,
                        vault: address(deployed),
                        lezConfigId: deployed.lezConfigId(),
                        burnRoot: root,
                        nullifier: r.nullifier,
                        token: r.token,
                        amount: 10_000e6,
                        recipient: bob,
                        relayer: address(0),
                        relayFee: 0
                    })
                )
            )
        );
        deployed.release(r, SEAL);
        assertTrue(deployed.nullifierUsed(r.nullifier), "released under the lowest caps the floor allows");

        // And the largest refund, a 0.1 ETH leaf LEZ never imported (its denomination and relay fee), under the same caps.
        _refundUnderTheLowestCaps(root);
    }

    /// @dev Deposits a 0.1 ETH leaf, retires `deployed`, finalises a sunset checkpoint that counts only the first leaf
    ///      imported, and refunds the new leaf once the window has passed.
    function _refundUnderTheLowestCaps(bytes32 root) internal {
        bytes32[] memory leaves = new bytes32[](2);
        leaves[0] = _leafHash(0, alice, address(testToken), 10_000e6, keccak256("the largest leaf"));
        bytes32 commitment = keccak256("a 0.1 ETH leaf never imported");
        uint64 leafIndex = _depositInto(deployed, bob, ETH, 2, commitment);
        assertEq(leafIndex, 1);
        leaves[1] = _leafHash(1, bob, ETH, 0.1 ether, commitment);
        assertEq(ReferenceMerkle.root(leaves), deployed.depositRoot(), "the reference tree is the vault's");
        vm.prank(deployed.freezeAuthority());
        deployed.retire();
        FerrataVault.Checkpoint memory c = _nextCheckpoint(deployed, root, 1, true);
        verifier.accept(
            deployed.checkpointImageId(),
            sha256(_checkpointJournalBytes(_checkpointJournal(deployed, deployed.lezConfigId(), c)))
        );
        deployed.submitCandidate(c, SEAL);
        vm.warp(block.timestamp + deployed.delta());
        deployed.finalizeAttested(c);
        vm.warp(block.timestamp + deployed.revokeWindow() + 1);
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, 1);
        bytes32[32] memory path;
        for (uint256 h = 0; h < 32; h++) {
            path[h] = siblings[h];
        }
        uint256 before = bob.balance;
        deployed.refund(1, bob, ETH, 2, commitment, path);
        assertEq(bob.balance - before, 0.1 ether + _relayFeesOf(deployed, ETH)[2], "refunded whole under the lowest caps");
    }

    /// @notice The script's pre-flight (final review round): the default ceilings admit the default floor, and a params
    ///         file whose ETH ceiling is below it (1 ETH a day against the 10,000-token exit's 10,025,000,000 value
    ///         units, about 3.34 ETH) is refused before the vault, whose floor is immutable, exists.
    function test_S2_aCeilingBelowTheFloorIsRefusedBeforeTheVault() public {
        _script(FerrataVault.Mode.B, false, 0);
        FerrataVault.CapParams memory floor = script.floor_();
        FerrataVault.TokenParams[2] memory ps = [script.ethParams_(), script.tokenParams_()];
        assertTrue(script.capsAdmitTheFloor_(ps, floor), "the defaults admit the floor");
        ps[0].releaseCap = FerrataVault.CapParams({capacity: 1 ether, refillPerSecond: uint256(1 ether) / 1 days});
        assertFalse(script.capsAdmitTheFloor_(ps, floor), "1 ETH a day is below the floor");
        ps[0].releaseCap = FerrataVault.CapParams({capacity: 4 ether, refillPerSecond: uint256(4 ether) / 1 days});
        assertTrue(script.capsAdmitTheFloor_(ps, floor), "4 ETH a day admits it");
        // The same verdict as the vault's registration: 1 ETH a day is refused there with CapBelowFloor.
        deployed = new FerrataVault(script.deployment_(address(estop)));
        FerrataVault.TokenParams memory low = script.ethParams_();
        low.releaseCap = FerrataVault.CapParams({capacity: 1 ether, refillPerSecond: uint256(1 ether) / 1 days});
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        deployed.registerToken(ETH, low);
    }

    /// @notice F.12: by default the vault waits the design's Δ for its rule, as floor and initial value.
    function test_deploymentDefault_waitIsTheDesignsForTheRule() public {
        _deploy(FerrataVault.Mode.B, false, 0);
        assertEq(deployed.deltaFloor(), 1 hours, "rule (B): 1 h");
        assertEq(deployed.delta(), 1 hours);
        _deploy(FerrataVault.Mode.A, false, 0);
        assertEq(uint8(deployed.mode()), uint8(FerrataVault.Mode.A));
        assertEq(deployed.deltaFloor(), 3 hours + 1 minutes, "rule (A): 3 h 01 min");
        assertEq(deployed.delta(), 3 hours + 1 minutes);
        // The admin raises it for later candidates, never lowers it (Checkpoint.t.sol: a raise does not touch a
        // submitted candidate).
        vm.expectRevert(FerrataVault.DeltaCanOnlyRise.selector);
        deployed.raiseDelta(3 hours);
        deployed.raiseDelta(4 hours);
        assertEq(deployed.delta(), 4 hours);
    }

    /// @notice FERRATA_DELTA sets another wait for a test run (the testnet runs: 600 s, the crown runs: 3,600 s).
    function test_delta_otherOnlyWhenGiven() public {
        _deploy(FerrataVault.Mode.B, false, 600);
        assertEq(deployed.deltaFloor(), 600);
        assertEq(deployed.delta(), 600);
        _deploy(FerrataVault.Mode.A, false, 3600);
        assertEq(deployed.deltaFloor(), 3600);
    }

    function test_uncapped_onlyWhenAskedFor() public {
        _deploy(true);
        (uint256 capacity,,) = deployed.tokenCap(ETH, FerrataVault.Direction.Deposit);
        assertEq(capacity, type(uint256).max);
        (capacity,,) = deployed.globalCap(FerrataVault.Direction.Release);
        assertEq(capacity, type(uint256).max);
        assertEq(deployed.releaseCapFloor(), 0);
    }
}
