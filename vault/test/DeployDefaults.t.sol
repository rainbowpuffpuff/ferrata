// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {DeployVault} from "../script/DeployVault.s.sol";
import {FerrataVault} from "../src/FerrataVault.sol";

import {MockERC20} from "./mocks/MockTokens.sol";
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
}

/// @notice RFP-022 Sec.4 ("Caps bound the maximum value at risk in any rolling window; proposals must document
///         recommended defaults"): the deployment script's default is capped. A vault built from exactly what the
///         script would send (its own `Deployment` and registrations, on the default params) refuses the deposit
///         that would pass its token's daily ceiling (100 times the largest denomination, counted on each deposit's
///         whole escrow), refills over a day, and keeps every release cap above an immutable floor; the uncapped
///         deployment of the earlier testnet runs needs FERRATA_UNCAPPED=1.
contract DeployDefaultsTest is VaultTestBase {
    DeployVaultExposed internal script;
    FerrataVault internal deployed;
    MockERC20 internal testToken;

    function _deploy(bool uncapped) internal {
        vm.setEnv("FERRATA_MODE", "B");
        vm.setEnv("FERRATA_UNCAPPED", uncapped ? "true" : "false");
        script = new DeployVaultExposed();
        script.init_(); // the deployer (admin) is this test contract
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
        // The immutable release floor: one 0.1 ETH a day in value units (ETH's largest is worth less than the token's).
        assertEq(deployed.releaseCapFloor(), 300_000_000);
        assertEq(deployed.releaseRefillFloor(), uint256(300_000_000) / 1 days);

        // A deposit counts its whole escrow (0.1 ETH and its 0.2 % relay fee) against the bucket: 99 of 0.1 ETH fill
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
        deployed.setGlobalCap(FerrataVault.Direction.Release, 299_999_999, 1 ether);
        deployed.setTokenCap(ETH, FerrataVault.Direction.Release, 0.1 ether, uint256(10 ether) / 1 days);
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
