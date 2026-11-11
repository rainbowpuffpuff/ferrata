// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// Rule (B) with t of n: a real checkpoint_b proof over the lab's real LEZ blocks 186..=202 (T = 202)
// under a key set of three (the lab sequencer's key, the real producer of T, and two seeded stand-in
// sequencer keys), with two attestations collected by ferrata_attestor (n = 3, t = 2; the third
// sequencer silent): checkpoint/attestor/devnet.sh, results in checkpoint/attestor/runs/devnet-n3/.
// The vault is the v5 FerrataVault in mode (B) with nMin = 3, tMin = 2, behind RISC Zero's real
// Groth16 verifier and emergency stop, in Foundry's local EVM.

import {Test, console2} from "forge-std/Test.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";

abstract contract RuleBMultiFixture is Test {
    uint256 internal constant WAIT = 1 hours;
    uint256 internal constant TAMPERED_CALL_GAS = 2_000_000;

    bytes internal journal;
    bytes internal seal;
    bytes32 internal imageId;
    bytes32[] internal keys;
    uint32 internal postingTimeframe;
    bytes32 internal configTipHash;
    uint64 internal parentTipId;
    bytes32 internal parentTipHash;
    uint64 internal tipId;
    bytes32 internal tipHash;

    FerrataVault internal vault;
    address internal admin = makeAddr("multi admin");
    address internal freezer = makeAddr("multi freeze authority");
    address internal watcher = makeAddr("multi watcher");

    function _load() internal {
        string memory json = vm.readFile("test/real/fixtures/rule-b-multi.json");
        journal = vm.parseJsonBytes(json, ".journal");
        seal = vm.parseJsonBytes(json, ".seal");
        imageId = vm.parseJsonBytes32(json, ".imageId");
        bytes32[] memory k = vm.parseJsonBytes32Array(json, ".keys");
        for (uint256 i = 0; i < k.length; i++) {
            keys.push(k[i]);
        }
        postingTimeframe = uint32(vm.parseJsonUint(json, ".postingTimeframe"));
        configTipHash = vm.parseJsonBytes32(json, ".configTipHash");
        parentTipId = uint64(vm.parseJsonUint(json, ".parentTipId"));
        parentTipHash = vm.parseJsonBytes32(json, ".parentTipHash");
        tipId = uint64(vm.parseJsonUint(json, ".tipId"));
        tipHash = vm.parseJsonBytes32(json, ".tipHash");
        require(sha256(journal) == vm.parseJsonBytes32(json, ".journalSha256"), "journal digest");
    }

    function _checkpoint() internal view returns (FerrataVault.Checkpoint memory c) {
        (,,,, c) = abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
    }

    function _state(bytes32[] memory ks) internal view returns (bytes32) {
        bytes memory packed;
        for (uint256 i = 0; i < ks.length; i++) {
            packed = bytes.concat(packed, ks[i]);
        }
        bytes32 keySetHash = sha256(abi.encodePacked("ferrata/keyset/v2", uint32(ks.length), postingTimeframe, packed));
        bytes32 emptyFrontier = sha256(abi.encodePacked("ferrata/burn-frontier/v1", new bytes(32 * 32)));
        return sha256(
            abi.encodePacked(
                "ferrata/lez-state/v1",
                parentTipId,
                parentTipHash,
                keySetHash,
                configTipHash,
                uint64(0),
                emptyFrontier,
                uint64(0),
                uint8(0)
            )
        );
    }

    function _first(uint256 count) internal view returns (bytes32[] memory k) {
        k = new bytes32[](count);
        for (uint256 i = 0; i < count; i++) {
            k[i] = keys[i];
        }
    }

    function _deploy(FerrataVault.Mode mode, bytes32 id, bytes32 genesisState, uint32 nMin_, uint32 tMin_) internal {
        (, uint256 chainId, address at, bytes32 lezConfigId,) =
            abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
        vm.chainId(chainId);
        RiscZeroGroth16Verifier groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        RiscZeroVerifierEmergencyStop estop = new RiscZeroVerifierEmergencyStop(groth16, freezer);
        FerrataVault.Deployment memory d = FerrataVault.Deployment({
            verifier: IRiscZeroVerifier(address(estop)),
            checkpointImageId: id,
            releaseImageId: keccak256("ferrata test: no release in this run"),
            lezConfigId: lezConfigId,
            genesisState: genesisState,
            nMin: nMin_,
            tMin: tMin_,
            deltaFloor: WAIT,
            initialDelta: WAIT,
            revokeWindow: 7 days,
            maxFreeze: 30 days,
            turnSeconds: mode == FerrataVault.Mode.B ? 0 : 900,
            minReleaseDelay: 0,
            admin: admin,
            freezeAuthority: freezer,
            globalDepositCap: FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0}),
            globalReleaseCap: FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0}),
            mode: mode,
            watchers: [watcher, address(0), address(0)],
            releaseCapFloor: FerrataVault.CapParams({capacity: 0, refillPerSecond: 0}),
            feeRecipient: address(0xFEE)
        });
        bytes memory initcode = abi.encodePacked(type(FerrataVault).creationCode, abi.encode(d));
        vm.etch(at, initcode);
        (bool ok, bytes memory runtime) = at.call("");
        require(ok, "constructor reverted");
        vm.etch(at, runtime);
        vault = FerrataVault(payable(at));
    }
}

contract RuleBMultiTest is RuleBMultiFixture {
    function setUp() public {
        _load();
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 3, 2);
    }

    function test_ruleBMulti_theJournalIsTwoOfThree() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(keys.length, 3);
        assertEq(c.parentState, vault.genesisState());
        assertEq(c.n, 3);
        assertEq(c.t, 2);
        assertEq(c.nMin, 3);
        assertEq(c.tMin, 2);
        assertEq(c.tipId, tipId);
        assertEq(c.tipHash, tipHash);
        assertEq(c.burnCount, 0, "no bridge program runs in the lab");
        assertEq(c.echoNonce, bytes32(0), "rule (B): no beacon");
    }

    function test_ruleBMulti_candidateThenFinalAfterTheWait() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.prank(makeAddr("anyone"));
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        (bytes32 digest,,,,,,,) = vault.candidates(c.newState);
        assertEq(digest, sha256(journal));
        assertEq(vault.finalState(), c.parentState, "nothing final before the wait");
        vm.warp(at + WAIT - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(at + WAIT);
        vm.prank(makeAddr("finaliser"));
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        (bool accepted,) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
    }

    /// The journal's committee words are proven: a journal claiming fewer signatures or another
    /// committee under the real seal is refused.
    function test_ruleBMulti_anotherCommitteeUnderTheRealSealIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        for (uint256 field = 0; field < 5; field++) {
            FerrataVault.Checkpoint memory x = abi.decode(abi.encode(c), (FerrataVault.Checkpoint));
            if (field == 0) x.t = 1;
            if (field == 1) x.t = 3;
            if (field == 2) x.n = 4;
            if (field == 3) x.tipHash = keccak256("another tip");
            if (field == 4) x.newState = keccak256("another state");
            vm.expectRevert(VerificationFailed.selector);
            vault.submitCandidate(x, seal);
        }
        vault.submitCandidate(c, seal); // control
    }

    function test_ruleBMulti_aFlippedByteInTheSealIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        for (uint256 word = 0; word < 8; word++) {
            bytes memory bad = bytes.concat(seal);
            bad[4 + 32 * word + 31] = bad[4 + 32 * word + 31] ^ 0x01;
            vm.expectRevert();
            vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
        }
        vault.submitCandidate(c, seal); // control
    }
}

contract RuleBMultiWrongVaultTest is RuleBMultiFixture {
    function setUp() public {
        _load();
    }

    function test_ruleBMulti_aOneKeyVaultRefusesIt() public {
        _deploy(FerrataVault.Mode.B, imageId, _state(_first(1)), 1, 1);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    function test_ruleBMulti_aVaultOfTwoKeysRefusesIt() public {
        _deploy(FerrataVault.Mode.B, imageId, _state(_first(2)), 2, 1);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    function test_ruleBMulti_aVaultWithOtherFloorsRefusesIt() public {
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 1, 1);
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(_checkpoint(), seal);
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 4, 3);
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    function test_ruleBMulti_aVaultPinningAnotherImageRefusesIt() public {
        _deploy(FerrataVault.Mode.B, keccak256("another checkpoint_b image"), _state(keys), 3, 2);
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }
}

contract RuleBMultiGas is RuleBMultiFixture {
    function setUp() public {
        _load();
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 3, 2);
    }

    function test_gas_ruleBMulti() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        console2.log("rule (B) n=3 t=2: submitCandidate, real checkpoint_b seal", vm.lastCallGas().gasTotalUsed);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        console2.log("rule (B) n=3 t=2: finalizeAttested after the wait", vm.lastCallGas().gasTotalUsed);
    }
}
