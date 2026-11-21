// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script, console} from "forge-std/Script.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "../test/mocks/MockVerifiers.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {TestToken} from "./TestToken.sol";

/// @notice The vault of `modules/scripts/bridge-send-stack.sh` (ferrata_bridge_module's Ethereum sends through Logos
///         Core's send stack), on a local anvil only: the vault tests' MockRiscZeroVerifier behind RISC Zero's
///         emergency stop, mode (B), the deployer as admin and `FERRATA_FREEZE_AUTHORITY` as freeze authority (both
///         anvil accounts whose keys the run imports into Logos Core's keystore_module), ETH registered as
///         CiRoundTrip registers it (0.1, 1, 10 ETH), and an ERC-20 test token (6 decimals) deployed but NOT
///         registered: the module's admin registers it through the send stack.
contract LogosSendStack is Script {
    function run() external {
        address admin = msg.sender;
        address freezeAuthority = vm.envAddress("FERRATA_FREEZE_AUTHORITY");
        vm.startBroadcast();
        MockRiscZeroVerifier mock = new MockRiscZeroVerifier();
        RiscZeroVerifierEmergencyStop verifier = new RiscZeroVerifierEmergencyStop(mock, freezeAuthority);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        FerrataVault vault = new FerrataVault(
            FerrataVault.Deployment({
                verifier: IRiscZeroVerifier(address(verifier)),
                checkpointImageId: keccak256("logos send stack: checkpoint image (mock verifier)"),
                releaseImageId: keccak256("logos send stack: release image (mock verifier)"),
                lezConfigId: keccak256("logos send stack: configuration"),
                genesisState: keccak256("logos send stack: genesis state"),
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
        TestToken token = new TestToken();
        vm.stopBroadcast();
        console.log("vault", address(vault));
        console.log("mock", address(mock));
        console.log("token", address(token));
    }
}
