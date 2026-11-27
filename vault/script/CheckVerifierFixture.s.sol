// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull, Receipt} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";

/// @notice Answers what a reader of RISC Zero's 3.1.0 verifier asks (SELECTOR, VERSION) and accepts every seal.
contract FakeGroth16 is IRiscZeroVerifierFull {
    bytes4 public constant SELECTOR = 0xe71e6620;
    string public constant VERSION = "3.1.0";

    function verify(bytes calldata, bytes32, bytes32) external pure {}
    function verifyIntegrity(Receipt calldata) external pure {}
}

/// @notice Answers what a reader of RISC Zero's emergency stop asks (verifier, owner, paused) and accepts every seal.
contract FakeStop is IRiscZeroVerifier {
    address public immutable verifier;
    address public constant owner = address(0);
    bool public constant paused = false;

    constructor(address v) {
        verifier = v;
    }

    function verify(bytes calldata, bytes32, bytes32) external pure {}
}

/// @title The wirings script/test-check-verifier-local.sh holds script/check-verifier.sh to (anvil only)
/// @notice One vault behind RISC Zero's Groth16 verifier 3.1.0 and its emergency stop with the owner renounced, as
///         script/DeployVault.s.sol deploys it; and five that differ in one thing: a fake verifier that answers
///         SELECTOR() and VERSION() like 3.1.0's, a fake stop that answers like RISC Zero's, a stop that keeps its
///         owner, a stop tripped after the vault was deployed. Each vault is printed as `CHECKVERIFIER NAME ADDRESS`.
contract CheckVerifierFixture is Script {
    address internal deployer;
    uint256 internal n;

    function run() external {
        require(block.chainid == 31_337, "CheckVerifierFixture: anvil only");
        deployer = msg.sender;
        vm.startBroadcast();
        address g = address(new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID));
        _print("ok", _vault(_stop(g, true)));
        _print("fake-verifier", _vault(_stop(address(new FakeGroth16()), true)));
        _print("fake-stop", _vault(address(new FakeStop(g))));
        _print("owned-stop", _vault(_stop(g, false)));
        address tripped = _stop(g, false);
        address v = _vault(tripped);
        RiscZeroVerifierEmergencyStop(tripped).estop();
        RiscZeroVerifierEmergencyStop(tripped).renounceOwnership();
        _print("tripped-stop", v);
        vm.stopBroadcast();
    }

    function _stop(address verifier, bool renounce) internal returns (address s) {
        s = address(new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(verifier), deployer));
        if (renounce) RiscZeroVerifierEmergencyStop(s).renounceOwnership();
    }

    function _vault(address stop) internal returns (address) {
        n++;
        FerrataVault.CapParams memory unlimited = FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        FerrataVault.Deployment memory d = FerrataVault.Deployment({
            verifier: IRiscZeroVerifier(stop),
            checkpointImageId: keccak256("fixture: checkpoint image"),
            releaseImageId: keccak256("fixture: release image"),
            lezConfigId: keccak256(abi.encode("fixture: configuration", n)),
            genesisState: keccak256("fixture: genesis"),
            nMin: 3,
            tMin: 2,
            deltaFloor: 1 hours,
            initialDelta: 1 hours,
            revokeWindow: 1 days,
            maxFreeze: 30 days,
            turnSeconds: 0,
            minReleaseDelay: 0,
            admin: deployer,
            freezeAuthority: address(0xF00D),
            globalDepositCap: unlimited,
            globalReleaseCap: unlimited,
            mode: FerrataVault.Mode.B,
            watchers: [address(0), address(0), address(0)],
            releaseCapFloor: FerrataVault.CapParams({capacity: 0, refillPerSecond: 0}),
            feeRecipient: deployer
        });
        return address(new FerrataVault(d));
    }

    function _print(string memory name, address vault) internal pure {
        console2.log(string.concat("CHECKVERIFIER ", name, " ", vm.toString(vault)));
    }
}
