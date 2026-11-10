// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {TestToken} from "./TestToken.sol";

/// @notice The return leg on a testnet, one function per stage (script/roundtrip.sh runs them and keeps the
///         manifest). Everything a stage needs comes from the end-to-end fixture (`FERRATA_FIXTURE`, by default
///         ../e2e/runs/sepolia/fixture.json): the addresses the proofs bind, the journals and the seals.
///
///         deploy()    the sender's next four creations: RISC Zero's Groth16 verifier, its emergency stop, a test
///                     token, the vault; then both registrations and both deposits
///         submit()    submitCandidate with the candidate's real seal
///         beacon()    recordBeacon, once the candidate's wait has passed
///         finalize()  finalize with the seal that echoes that beacon (proven after it)
///         release()   every release of the fixture
///
///         A testnet configuration: the timers are short and come from the environment; the checks, the verifier
///         and the proofs are the real ones. The sender is the admin; the freeze authority is an address nobody
///         holds a key for unless FERRATA_FREEZE_AUTHORITY names one.
contract RoundTrip is Script {
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;

    string internal json;
    FerrataVault internal vault;

    function setUp() public {
        json = vm.readFile(vm.envOr("FERRATA_FIXTURE", string("../e2e/runs/sepolia/fixture.json")));
        require(block.chainid == vm.parseJsonUint(json, ".chainId"), "the fixture is for another chain id");
        vault = FerrataVault(payable(vm.parseJsonAddress(json, ".vault")));
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        (d[0], d[1], d[2]) = (a, b, c);
    }

    function _unlimited() internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
    }

    function _params(uint256[] memory denoms, uint256[] memory fees, uint256 lezUnit, uint256 unitWeight)
        internal
        view
        returns (FerrataVault.TokenParams memory)
    {
        return FerrataVault.TokenParams({
            denominations: denoms,
            relayFees: fees,
            lezUnit: lezUnit,
            unitWeight: unitWeight,
            depositCap: _unlimited(),
            releaseCap: _unlimited(),
            releaseDelay: vm.envOr("FERRATA_RELEASE_DELAY", uint256(120))
        });
    }

    function _config(IRiscZeroVerifier verifier, address admin)
        internal
        view
        returns (FerrataVault.Deployment memory)
    {
        uint256 delta = vm.envOr("FERRATA_DELTA", uint256(600));
        return FerrataVault.Deployment({
            verifier: verifier,
            checkpointImageId: vm.parseJsonBytes32(json, ".checkpointImageId"),
            releaseImageId: vm.parseJsonBytes32(json, ".releaseImageId"),
            lezConfigId: vm.parseJsonBytes32(json, ".lezConfigId"),
            genesisState: vm.parseJsonBytes32(json, ".genesisState"),
            nMin: uint32(vm.parseJsonUint(json, ".nMin")),
            tMin: uint32(vm.parseJsonUint(json, ".tMin")),
            deltaFloor: delta,
            initialDelta: delta,
            revokeWindow: vm.envOr("FERRATA_REVOKE_WINDOW", uint256(1 days)),
            maxFreeze: vm.envOr("FERRATA_MAX_FREEZE", uint256(30 days)),
            // The deployment check of SPEC-v3 D3: the channel's posting timeframe times the one-second slot.
            turnSeconds: vm.parseJsonUint(json, ".postingTimeframe"),
            minReleaseDelay: vm.envOr("FERRATA_MIN_RELEASE_DELAY", uint256(0)),
            admin: admin,
            freezeAuthority: vm.envOr(
                "FERRATA_FREEZE_AUTHORITY",
                address(uint160(uint256(keccak256("ferrata testnet: freeze authority (no key)"))))
            ),
            globalDepositCap: _unlimited(),
            globalReleaseCap: _unlimited()
        });
    }

    function deploy() external {
        address sender = msg.sender;
        require(sender == vm.parseJsonAddress(json, ".deployer"), "the sender is not the fixture's deployer");
        require(address(vault).code.length == 0, "the vault is already deployed");
        vm.startBroadcast();
        RiscZeroGroth16Verifier groth16 =
            new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        FerrataVault.Deployment memory config = _config(IRiscZeroVerifier(address(0)), sender);
        RiscZeroVerifierEmergencyStop estop = new RiscZeroVerifierEmergencyStop(groth16, config.freezeAuthority);
        TestToken token = new TestToken();
        config.verifier = IRiscZeroVerifier(address(estop));
        FerrataVault created = new FerrataVault(config);
        // The proofs bind these addresses: stop before anything else is sent if they are not the fixture's.
        require(address(groth16) == vm.parseJsonAddress(json, ".groth16Verifier"), "verifier address");
        require(address(estop) == vm.parseJsonAddress(json, ".emergencyStop"), "emergency stop address");
        require(address(token) == vm.parseJsonAddress(json, ".token"), "token address");
        require(address(created) == address(vault), "vault address: the sender's nonce is not the fixture's");

        created.registerToken(
            ETH, _params(_three(0.001 ether, 0.01 ether, 0.1 ether), _three(2e12, 2e13, 2e14), 1e12, 3000)
        );
        created.registerToken(
            address(token), _params(_three(100e6, 1000e6, 10_000e6), _three(250_000, 2_500_000, 25_000_000), 1, 1)
        );
        token.mint(sender, 100e6);
        token.approve(address(created), 100e6);
        created.depositETH{value: 0.001 ether}(0, vm.parseJsonBytes32(json, ".deposits[0].commitment"), 0);
        created.deposit(address(token), 0, vm.parseJsonBytes32(json, ".deposits[1].commitment"), 1);
        vm.stopBroadcast();
        require(created.depositRoot() == vm.parseJsonBytes32(json, ".depositRoot"), "deposit root");
        console2.log("vault", address(created));
    }

    function _checkpoint(string memory key) internal view returns (FerrataVault.Checkpoint memory c) {
        (,,,, c) =
            abi.decode(vm.parseJsonBytes(json, key), (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
    }

    /// @dev When the candidate's wait is over: submittedAt + deltaAtSubmit + (t - 1) * turnSeconds.
    function _waitOver(bytes32 newState) internal view returns (uint256) {
        (,,,,, uint32 t,,, uint256 submittedAt, uint256 deltaAtSubmit,) = vault.candidates(newState);
        require(submittedAt != 0, "no such candidate");
        return submittedAt + deltaAtSubmit + (uint256(t) - 1) * vault.turnSeconds();
    }

    function submit() external {
        FerrataVault.Checkpoint memory c = _checkpoint(".candidate.journal");
        vm.broadcast();
        vault.submitCandidate(c, vm.parseJsonBytes(json, ".candidate.seal"));
    }

    function beacon() external {
        uint256 over = _waitOver(_checkpoint(".candidate.journal").newState);
        require(block.timestamp >= over, "the candidate's wait is not over");
        vm.broadcast();
        vault.recordBeacon();
    }

    function finalize() external {
        FerrataVault.Checkpoint memory c = _checkpoint(".finalize.journal");
        uint256 index = vm.envOr("FERRATA_BEACON_INDEX", vault.beaconCount() - 1);
        (uint256 timestamp, bytes32 nonce) = vault.beacons(index);
        require(nonce == c.echoNonce, "the finalisation proof echoes another beacon");
        require(timestamp >= _waitOver(c.newState), "that beacon was recorded before the wait was over");
        vm.broadcast();
        vault.finalize(index, c, vm.parseJsonBytes(json, ".finalize.seal"));
    }

    function release() external {
        uint256 count = vm.parseJsonUint(json, ".releaseCount");
        for (uint256 i = 0; i < count; i++) {
            string memory at = string.concat(".releases[", vm.toString(i), "]");
            FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
                token: vm.parseJsonAddress(json, string.concat(at, ".token")),
                denomIndex: 0,
                recipient: vm.parseJsonAddress(json, string.concat(at, ".recipient")),
                relayer: vm.parseJsonAddress(json, string.concat(at, ".relayer")),
                relayFee: uint256(vm.parseJsonBytes32(json, string.concat(at, ".relayFee"))),
                burnRoot: vm.parseJsonBytes32(json, string.concat(at, ".burnRoot")),
                nullifier: vm.parseJsonBytes32(json, string.concat(at, ".nullifier"))
            });
            // A release already paid (the stage was interrupted between two) is not sent again.
            if (vault.nullifierUsed(r.nullifier)) continue;
            (bool accepted, uint256 finalizedAt) = vault.burnRootStatus(r.burnRoot);
            require(accepted, "the burn root is not finalised");
            require(block.timestamp >= finalizedAt + vault.releaseDelayFor(r.token, 0), "the release delay is not over");
            vm.broadcast();
            vault.release(r, vm.parseJsonBytes(json, string.concat(at, ".seal")));
        }
    }
}
