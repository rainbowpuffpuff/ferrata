// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test, console2} from "forge-std/Test.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";

/// @notice Rule (B) with no mock on the Ethereum side: the real rule (B) checkpoint proof (guest checkpoint_b over
///         cp-empty-360's blocks, three certified attestations, n = 4, t = 3; proven on one NVIDIA L4, record in
///         gpu-prover/results/ruleb-l4b/), wrapped to Groth16, verified by RISC Zero's unmodified Groth16 verifier
///         behind its unmodified emergency stop. The journal binds chain id 1 and a vault address, so the rule (B)
///         vault is constructed at that address (its creation code run there, as CREATE would).
abstract contract RuleBRealFixture is Test {
    /// checkpoint_b's image id (checkpoint/RESULTS.md §4); the fixture's must equal it.
    bytes32 internal constant CHECKPOINT_B_IMAGE_ID = 0xd359754364c10357103359276867f4fe58842c42cbb30a3ada5b241de6aa0d64;
    /// Rule (A)'s checkpoint image id, built from the same sources.
    bytes32 internal constant CHECKPOINT_A_IMAGE_ID = 0xe0b86641a069bf1ed2def38c8c14b846e7fe55626e518defc3cc7f71146d61b9;
    uint256 internal constant WAIT = 1 hours; // D13's default reaction window
    /// Gas for a call with a tampered seal (the real verification of a good one takes about 0.25M).
    uint256 internal constant TAMPERED_CALL_GAS = 2_000_000;

    bytes internal journal;
    bytes internal seal;
    FerrataVault internal vault;
    RiscZeroVerifierEmergencyStop internal estop;
    address internal admin = makeAddr("rule B admin");
    address internal freezer = makeAddr("rule B freeze authority");
    address internal watcher = makeAddr("rule B watcher");

    function _load() internal {
        string memory json = vm.readFile("test/real/fixtures/rule-b.json");
        journal = vm.parseJsonBytes(json, ".journal");
        seal = vm.parseJsonBytes(json, ".seal");
        require(vm.parseJsonBytes32(json, ".imageId") == CHECKPOINT_B_IMAGE_ID, "image id");
    }

    function _checkpoint() internal view returns (FerrataVault.Checkpoint memory c) {
        (,,,, c) = abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
    }

    /// @dev RISC Zero's verifier and stop, then the vault of `mode` at the journal's address on its chain.
    function _deploy(FerrataVault.Mode mode, bytes32 checkpointImageId) internal {
        (, uint256 chainId, address at, bytes32 lezConfigId, FerrataVault.Checkpoint memory c) =
            abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
        vm.chainId(chainId);
        RiscZeroGroth16Verifier groth16 =
            new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        estop = new RiscZeroVerifierEmergencyStop(groth16, freezer);
        FerrataVault.Deployment memory d = FerrataVault.Deployment({
            verifier: IRiscZeroVerifier(address(estop)),
            checkpointImageId: checkpointImageId,
            releaseImageId: keccak256("ferrata test: no release in this run"),
            lezConfigId: lezConfigId,
            genesisState: c.parentState,
            nMin: c.nMin,
            tMin: c.tMin,
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

contract RuleBRealTest is RuleBRealFixture {
    function setUp() public {
        vm.skip(true, "rule-b.json is a record of an earlier checkpoint_b under RISC Zero 3.0.5; its input is not recorded, so it was not proven again under 3.1 (test/real/README.md)");
        _load();
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID);
    }

    function test_ruleB_realProof_candidateThenFinalAfterTheWait() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(c.echoNonce, bytes32(0));
        assertEq(c.parentState, vault.genesisState());
        vm.prank(makeAddr("anyone"));
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        (bytes32 digest,,,,,,,) = vault.candidates(c.newState);
        assertEq(digest, sha256(journal), "the vault keeps the digest the real seal was verified against");
        assertEq(vault.beaconCount(), 0);

        vm.warp(at + WAIT - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(at + WAIT);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        (bool accepted, uint256 finalizedAt) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
        assertEq(finalizedAt, at + WAIT);
    }

    /// @notice One flipped byte in the selector or in any of the proof's eight words. A malformed curve point makes
    ///         the pairing precompile consume all the gas it is given, so each tampered call gets a bounded budget,
    ///         as in ReturnLeg.t.sol.
    function test_ruleB_realProof_aFlippedByteInTheSealIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory bad = bytes.concat(seal);
        bad[0] = bad[0] ^ 0x01;
        vm.expectRevert();
        vault.submitCandidate(c, bad);
        for (uint256 word = 0; word < 8; word++) {
            bad = bytes.concat(seal);
            bad[4 + 32 * word + 31] = bad[4 + 32 * word + 31] ^ 0x01;
            vm.expectRevert();
            vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
        }
        vault.submitCandidate(c, seal); // control
        (bytes32 digest,,,,,,,) = vault.candidates(c.newState);
        assertEq(digest, sha256(journal));
    }

    function test_ruleB_realProof_eachJournalFieldIsBound() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        for (uint256 field = 0; field < 12; field++) {
            FerrataVault.Checkpoint memory x = abi.decode(abi.encode(c), (FerrataVault.Checkpoint));
            if (field == 0) x.newState = keccak256("another state");
            if (field == 1) x.burnRoot = keccak256("another root");
            if (field == 2) x.burnCount = 1;
            if (field == 3) x.sunset = true;
            if (field == 4) x.tipId += 1;
            if (field == 5) x.tipHash = keccak256("another tip");
            if (field == 6) x.n = 5;
            if (field == 7) x.t = 4;
            if (field == 8) vm.chainId(2);
            if (field == 9) x.echoNonce = keccak256("a nonce"); // refused before the proof: candidates echo 0
            if (field == 10) x.nMin = 4; // refused before the proof: the deployed floors
            if (field == 11) x.tMin = 3;
            if (field == 9) vm.expectRevert(FerrataVault.EchoNonceNotZero.selector);
            else if (field >= 10) vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
            else vm.expectRevert(VerificationFailed.selector);
            vault.submitCandidate(x, seal);
            vm.chainId(1);
        }
        vault.submitCandidate(c, seal); // control
    }

    /// @notice A watcher cancels the real candidate within its wait; nothing is accepted.
    function test_ruleB_realProof_aWatcherCancels() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        vm.prank(watcher);
        vault.cancelCandidate(c.newState);
        vm.warp(block.timestamp + WAIT);
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.parentState);
    }
}

/// @notice The real rule (B) proof on a rule (A) vault at the same address: the vault asks for rule (A)'s program,
///         so the proof of checkpoint_b is refused.
contract RuleBRealOnRuleAVaultTest is RuleBRealFixture {
    function setUp() public {
        vm.skip(true, "rule-b.json is a record of an earlier checkpoint_b under RISC Zero 3.0.5; its input is not recorded, so it was not proven again under 3.1 (test/real/README.md)");
        _load();
        _deploy(FerrataVault.Mode.A, CHECKPOINT_A_IMAGE_ID);
    }

    function test_ruleB_realProof_refusedByARuleAVault() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate(c, seal);
    }
}

/// @notice Gas of rule (B) with the real verifier and the real seal, one transaction per step
///         (`forge test --isolate --match-contract RuleBRealGas -vv`).
contract RuleBRealGas is RuleBRealFixture {
    function setUp() public {
        vm.skip(true, "rule-b.json is a record of an earlier checkpoint_b under RISC Zero 3.0.5; its input is not recorded, so it was not proven again under 3.1 (test/real/README.md)");
        _load();
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID);
    }

    function _log(string memory step) internal view {
        console2.log(step, vm.lastCallGas().gasTotalUsed);
    }

    function test_gas_ruleB_withTheRealVerifier() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        _log("rule (B): submitCandidate, real checkpoint_b seal (no beacon)");
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        _log("rule (B): finalizeAttested after the wait (no proof)");
    }
}
