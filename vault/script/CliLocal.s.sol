// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "../test/mocks/MockVerifiers.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

/// @notice The vault of the ferrata CLI's end-to-end test (cli/tests/e2e.rs), on a local anvil only: the vault tests'
///         MockRiscZeroVerifier (the test stands in for the checkpoint prover and the Groth16 wrapper), and ETH
///         registered with the LEZ harness's ETH entry (lez/harness/src/bridge.rs `eth_entry`), so that the harness's
///         bridge program attests this vault's own storage. `FERRATA_CID` is the harness configuration's id for this
///         vault's address (the deployer's fourth contract: the linked FerrataRegistry library, the mock, its emergency stop, the vault).
contract CliLocal is Script {
    function run() external {
        bytes32 cid = vm.envBytes32("FERRATA_CID");
        address admin = msg.sender;
        vm.startBroadcast();
        MockRiscZeroVerifier mock = new MockRiscZeroVerifier();
        address freezeAuthority = address(uint160(uint256(keccak256("ferrata cli e2e: freeze authority (no key)"))));
        RiscZeroVerifierEmergencyStop verifier = new RiscZeroVerifierEmergencyStop(mock, freezeAuthority);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        FerrataVault vault = new FerrataVault(
            FerrataVault.Deployment({
                verifier: IRiscZeroVerifier(address(verifier)),
                checkpointImageId: keccak256("ferrata cli e2e: checkpoint image (mock)"),
                releaseImageId: keccak256("ferrata cli e2e: release image (mock)"),
                lezConfigId: cid,
                genesisState: keccak256("ferrata cli e2e: genesis state"),
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
            address(0),
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
        vm.stopBroadcast();
    }
}
