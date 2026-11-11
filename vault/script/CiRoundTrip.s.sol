// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "../test/mocks/MockVerifiers.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

/// @notice The vault of CI's round trip (cli/tests/roundtrip_b.rs, e2e/ci/README.md), on a local anvil only: rule (B),
///         the vault tests' MockRiscZeroVerifier behind the emergency stop (the test accepts each journal a guest
///         produced in RISC Zero dev mode, as the vault's own tests accept theirs), ETH registered with the LEZ harness's
///         ETH entry. From the environment: FERRATA_CID (the harness configuration's id for this vault's address, the
///         deployer's fourth contract: the linked FerrataRegistry library, the mock, its emergency stop, the vault),
///         FERRATA_GENESIS_STATE (the zone's genesis checkpoint state), FERRATA_CHECKPOINT_IMAGE_ID (the rule (B)
///         checkpoint guest's) and FERRATA_RELEASE_IMAGE_ID (the release guest's).
contract CiRoundTrip is Script {
    function run() external {
        bytes32 cid = vm.envBytes32("FERRATA_CID");
        bytes32 genesis = vm.envBytes32("FERRATA_GENESIS_STATE");
        bytes32 checkpointImage = vm.envBytes32("FERRATA_CHECKPOINT_IMAGE_ID");
        bytes32 releaseImage = vm.envBytes32("FERRATA_RELEASE_IMAGE_ID");
        address admin = msg.sender;
        vm.startBroadcast();
        MockRiscZeroVerifier mock = new MockRiscZeroVerifier();
        address freezeAuthority = address(uint160(uint256(keccak256("ferrata ci round trip: freeze authority (no key)"))));
        RiscZeroVerifierEmergencyStop verifier = new RiscZeroVerifierEmergencyStop(mock, freezeAuthority);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        FerrataVault vault = new FerrataVault(
            FerrataVault.Deployment({
                verifier: IRiscZeroVerifier(address(verifier)),
                checkpointImageId: checkpointImage,
                releaseImageId: releaseImage,
                lezConfigId: cid,
                genesisState: genesis,
                nMin: 3,
                tMin: 2,
                deltaFloor: 600,
                initialDelta: 600,
                revokeWindow: 1 days,
                maxFreeze: 30 days,
                turnSeconds: 0,
                minReleaseDelay: 0,
                admin: admin,
                freezeAuthority: freezeAuthority,
                globalDepositCap: unlimited,
                globalReleaseCap: unlimited,
                mode: FerrataVault.Mode.B,
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
