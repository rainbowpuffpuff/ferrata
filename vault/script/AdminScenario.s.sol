// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "../test/mocks/MockVerifiers.sol";
import {Script, console} from "forge-std/Script.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

/// @notice The admin view's local scenario (admin/README.md), on a local anvil only: a vault in states the live
///         Sepolia vault has never shown, for the admin view to read. The vault tests' MockRiscZeroVerifier behind
///         the emergency stop (no proof is verified here); rule (A); ETH registered as for the CLI's end-to-end test;
///         two deposits; a fee schedule announced (in force only after the 7-day notice); a release delay changed; a
///         global deposit cap; a role proposal naming three watchers and a new freeze authority; and, by the freeze
///         authority, releases frozen and ETH deposits frozen. Keys: `ADMIN_KEY` (anvil's published test account 0) and
///         `FREEZE_KEY` (any key: the admin funds it); nothing here may reach a public network.
contract AdminScenario is Script {
    function run() external {
        uint256 adminKey = vm.envUint("ADMIN_KEY");
        uint256 freezeKey = vm.envUint("FREEZE_KEY");
        address admin = vm.addr(adminKey);
        address freezeAuthority = vm.addr(freezeKey);
        address eth = address(0);

        vm.startBroadcast(adminKey);
        MockRiscZeroVerifier mock = new MockRiscZeroVerifier();
        RiscZeroVerifierEmergencyStop verifier = new RiscZeroVerifierEmergencyStop(mock, admin);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        FerrataVault vault = new FerrataVault(
            FerrataVault.Deployment({
                verifier: IRiscZeroVerifier(address(verifier)),
                checkpointImageId: keccak256("ferrata admin scenario: checkpoint image (mock)"),
                releaseImageId: keccak256("ferrata admin scenario: release image (mock)"),
                lezConfigId: keccak256("ferrata admin scenario: configuration"),
                genesisState: keccak256("ferrata admin scenario: genesis state"),
                nMin: 3,
                tMin: 2,
                deltaFloor: 600,
                initialDelta: 600,
                revokeWindow: 1 days,
                maxFreeze: 30 days,
                turnSeconds: 2,
                minReleaseDelay: 0,
                admin: admin,
                freezeAuthority: freezeAuthority,
                globalDepositCap: unlimited,
                globalReleaseCap: unlimited,
                mode: FerrataVault.Mode.A,
                watchers: [address(0), address(0), address(0)],
                releaseCapFloor: FerrataVault.CapParams({capacity: 0, refillPerSecond: 0}),
                feeRecipient: admin
            })
        );
        uint256[] memory denominations = new uint256[](3);
        (denominations[0], denominations[1], denominations[2]) = (0.1 ether, 1 ether, 10 ether);
        uint256[] memory relayFees = new uint256[](3);
        (relayFees[0], relayFees[1], relayFees[2]) = (100_000 gwei, 1_000_000 gwei, 10_000_000 gwei);
        vault.registerToken(
            eth,
            FerrataVault.TokenParams({
                denominations: denominations,
                relayFees: relayFees,
                lezUnit: 1 gwei,
                unitWeight: 1,
                depositCap: unlimited,
                releaseCap: unlimited,
                releaseDelay: 120,
                lezDefinition: bytes32(0),
                decimals: 18,
                checkpointFees: new uint256[](3)
            })
        );
        // Two deposits (the denomination plus its relay fee; schedule 0 has no mint fee).
        vault.depositETH{value: 0.1 ether + 100_000 gwei}(0, keccak256("admin scenario: commitment 0"), 0);
        vault.depositETH{value: 1 ether + 1_000_000 gwei}(1, keccak256("admin scenario: commitment 1"), 1);
        // Fee schedule 1: 0.5% mint and burn fees, announced now, in force after the notice.
        uint256[] memory fees = new uint256[](3);
        (fees[0], fees[1], fees[2]) = (0.0005 ether, 0.005 ether, 0.05 ether);
        vault.announceFees(eth, fees, fees, new uint256[](3));
        vault.setReleaseDelay(eth, 600);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, 100 ether, 0.01 ether);
        vault.proposeRoles(
            [
                address(uint160(uint256(keccak256("admin scenario: watcher 1")))),
                address(uint160(uint256(keccak256("admin scenario: watcher 2")))),
                address(uint160(uint256(keccak256("admin scenario: watcher 3"))))
            ],
            address(uint160(uint256(keccak256("admin scenario: next freeze authority"))))
        );
        // The freeze authority's gas.
        payable(freezeAuthority).transfer(1 ether);
        vm.stopBroadcast();

        vm.startBroadcast(freezeKey);
        vault.setReleasesFrozen(true);
        vault.setTokenDepositsFrozen(eth, true);
        vm.stopBroadcast();

        console.log("vault", address(vault));
    }
}
