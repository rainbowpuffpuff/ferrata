// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {Script} from "forge-std/Script.sol";

/// @notice Accepts every seal and reports an emergency stop that is never tripped. For the watchers'
///         rehearsal on a local anvil chain only (`monitor/tests/anvil-e2e.sh`): `monitor` never looks
///         at a proof, and its rehearsal needs candidates with chosen journals, honest and wrong.
///         Never deploy it on a public network.
contract AcceptAllStop is IRiscZeroVerifier {
    function verify(bytes calldata, bytes32, bytes32) external pure {}

    function paused() external pure returns (bool) {
        return false;
    }
}

/// @notice A FerrataVault v5 with the rehearsal's roles: one watcher (the monitor's key), a freeze authority (a
///         second monitor's key, for the revocation scenario), a 1-day revocation window, n >= 3, t >= 2. Rule (B)
///         with the default wait of 1 h; with FIXTURE_MODE_A=true, rule (A) with turnSeconds 900 and delta at its
///         default floor of 3 h 01 min (v3 D4). Roles come from the environment (FIXTURE_*).
contract MonitorFixture is Script {
    function run() external returns (FerrataVault vault) {
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        bool ruleA = vm.envOr("FIXTURE_MODE_A", false);
        uint256 delta = ruleA ? 3 hours + 1 minutes : 1 hours;
        vm.startBroadcast();
        AcceptAllStop verifier = new AcceptAllStop();
        FerrataVault.Deployment memory d = FerrataVault.Deployment({
            verifier: IRiscZeroVerifier(address(verifier)),
            checkpointImageId: keccak256("monitor rehearsal: checkpoint image id"),
            releaseImageId: keccak256("monitor rehearsal: release image id"),
            lezConfigId: keccak256("monitor rehearsal: LEZ configuration id"),
            genesisState: vm.envBytes32("FIXTURE_GENESIS"),
            nMin: 3,
            tMin: 2,
            deltaFloor: delta,
            initialDelta: delta,
            revokeWindow: 1 days,
            maxFreeze: 30 days,
            turnSeconds: ruleA ? 900 : 0,
            minReleaseDelay: 0,
            admin: vm.envAddress("FIXTURE_ADMIN"),
            freezeAuthority: vm.envAddress("FIXTURE_FREEZER"),
            globalDepositCap: unlimited,
            globalReleaseCap: unlimited,
            mode: ruleA ? FerrataVault.Mode.A : FerrataVault.Mode.B,
            watchers: [vm.envAddress("FIXTURE_WATCHER"), address(0), address(0)],
            releaseCapFloor: FerrataVault.CapParams({capacity: 0, refillPerSecond: 0}),
            feeRecipient: vm.envAddress("FIXTURE_FEES")
        });
        vault = new FerrataVault(d);
        vm.stopBroadcast();
    }
}
