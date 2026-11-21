// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "../test/mocks/MockVerifiers.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {TestToken} from "./TestToken.sol";
import {TestTetherToken} from "./TestTetherToken.sol";

/// @notice The vault of the web depositor page's test (web/depositor/test/run.sh), on a local anvil only: the same
///         deployment as the CLI's end-to-end vault (script/CliLocal.s.sol — the vault tests' MockRiscZeroVerifier
///         behind the emergency stop, ETH registered with three denominations and relay fees), plus a TestToken
///         (script/TestToken.sol) also registered with denominations and fees, and test tokens minted to anvil's
///         account 1, the account the headless browser's injected wallet answers for. Nothing here is claimed on
///         LEZ, so the configuration id is a fixed label rather than the harness's `FERRATA_CID`; pass
///         FERRATA_CID to use one. WEB_DEPOSITOR_ACCOUNT overrides the mint recipient (anvil's default account 1).
contract WebDepositorLocal is Script {
    function run() external {
        bytes32 cid = vm.envOr("FERRATA_CID", keccak256("ferrata web depositor test: lez config id"));
        address admin = msg.sender;
        address depositor = vm.envOr(
            "WEB_DEPOSITOR_ACCOUNT",
            address(0x70997970C51812dc3A010C7d01b50e0d17dc79C8)
        ); // anvil's account 1
        vm.startBroadcast();
        MockRiscZeroVerifier mock = new MockRiscZeroVerifier();
        address freezeAuthority = address(uint160(uint256(keccak256("ferrata web depositor test: freeze authority (no key)"))));
        RiscZeroVerifierEmergencyStop verifier = new RiscZeroVerifierEmergencyStop(mock, freezeAuthority);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        FerrataVault vault = new FerrataVault(
            FerrataVault.Deployment({
                verifier: IRiscZeroVerifier(address(verifier)),
                checkpointImageId: keccak256("ferrata web depositor test: checkpoint image (mock)"),
                releaseImageId: keccak256("ferrata web depositor test: release image (mock)"),
                lezConfigId: cid,
                genesisState: keccak256("ferrata web depositor test: genesis state"),
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
        // Native ETH, exactly as script/CliLocal.s.sol registers it.
        {
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
        }
        // A 6-decimal ERC-20 with its own denominations and fees. Registration sets fee schedule 0 with no mint
        // fee and the given relay fees (the vault only allows mint fees through announceFees' 7-day notice).
        TestToken token = new TestToken();
        uint256[] memory denominations = new uint256[](3);
        (denominations[0], denominations[1], denominations[2]) = (1_000_000, 10_000_000, 100_000_000);
        uint256[] memory relayFees = new uint256[](3);
        (relayFees[0], relayFees[1], relayFees[2]) = (10_000, 100_000, 1_000_000);
        uint256[] memory checkpointFees = new uint256[](3);
        (checkpointFees[0], checkpointFees[1], checkpointFees[2]) = (0, 100_000, 1_000_000);
        vault.registerToken(
            address(token),
            FerrataVault.TokenParams({
                denominations: denominations,
                relayFees: relayFees,
                lezUnit: 1,
                unitWeight: 1,
                depositCap: unlimited,
                releaseCap: unlimited,
                releaseDelay: 120,
                lezDefinition: bytes32(0),
                decimals: 6,
                checkpointFees: checkpointFees
            })
        );
        token.mint(depositor, 1_000_000_000); // 1,000 TUSD
        // A Tether-style token (approve returns nothing and refuses to change a non-zero allowance into another), so
        // that the page's allowance reset is exercised; the same denominations and fees.
        TestTetherToken tether = new TestTetherToken();
        vault.registerToken(
            address(tether),
            FerrataVault.TokenParams({
                denominations: denominations,
                relayFees: relayFees,
                lezUnit: 1,
                unitWeight: 1,
                depositCap: unlimited,
                releaseCap: unlimited,
                releaseDelay: 120,
                lezDefinition: bytes32(0),
                decimals: 6,
                checkpointFees: checkpointFees
            })
        );
        tether.mint(depositor, 1_000_000_000); // 1,000 TUSDT
        vm.stopBroadcast();
        console2.log("FERRATA_WEB_VAULT", address(vault));
        console2.log("FERRATA_WEB_TOKEN", address(token));
        console2.log("FERRATA_WEB_TETHER", address(tether));
        console2.log("FERRATA_WEB_DEPOSITOR", depositor);
    }
}
