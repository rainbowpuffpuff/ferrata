// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test, console2} from "forge-std/Test.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";

/// @notice The return leg over REAL LEZ blocks: a rule (B) checkpoint over blocks 8..=202 of the lab's chain
///         (lez/lab: LEZ v0.4.0-rc1's own sequencer with one Logos Blockchain validator, leader proofs in dev mode),
///         the blocks exactly as the sequencer inscribed them on its node, the tip attested final by a key that the
///         lab sequencer's channel key certified. The devnet's channel holds one key, so n = 1 and t = 1. Proven on
///         one NVIDIA L4 (gpu-prover/results/realblocks-l4/), wrapped to Groth16, verified by RISC Zero's unmodified
///         Groth16 verifier behind its unmodified emergency stop (checkpoint/RESULTS.md, "Real LEZ blocks").
abstract contract RealBlocksFixture is Test {
    /// checkpoint_b's image id: the same program as RuleBReal.t.sol's proof (checkpoint/RESULTS.md §4).
    bytes32 internal constant CHECKPOINT_B_IMAGE_ID = 0xd359754364c10357103359276867f4fe58842c42cbb30a3ada5b241de6aa0d64;
    /// Rule (A)'s checkpoint image id, built from the same sources.
    bytes32 internal constant CHECKPOINT_A_IMAGE_ID = 0xe0b86641a069bf1ed2def38c8c14b846e7fe55626e518defc3cc7f71146d61b9;
    uint256 internal constant WAIT = 1 hours; // D13's default reaction window
    /// Gas for a call with a tampered seal (a malformed point makes the pairing precompile take all it is given).
    uint256 internal constant TAMPERED_CALL_GAS = 2_000_000;

    bytes internal journal;
    bytes internal seal;
    /// The lab sequencer's channel key (a throwaway devnet key) and the attestation key it certified.
    bytes32 internal labKey;
    bytes32 internal attestationKey;
    /// The channel as the lab's node reports it, and the parent tip (the lab's block 7).
    uint32 internal postingTimeframe;
    bytes32 internal configTipHash;
    uint64 internal parentTipId;
    bytes32 internal parentTipHash;
    uint64 internal tipId;
    bytes32 internal tipHash;

    FerrataVault internal vault;
    address internal admin = makeAddr("real blocks admin");
    address internal freezer = makeAddr("real blocks freeze authority");
    address internal watcher = makeAddr("real blocks watcher");

    function _load() internal {
        string memory json = vm.readFile("test/real/fixtures/real-blocks.json");
        journal = vm.parseJsonBytes(json, ".journal");
        seal = vm.parseJsonBytes(json, ".seal");
        require(vm.parseJsonBytes32(json, ".imageId") == CHECKPOINT_B_IMAGE_ID, "image id");
        labKey = vm.parseJsonBytes32(json, ".labChannelKey");
        attestationKey = vm.parseJsonBytes32(json, ".attestationKey");
        postingTimeframe = uint32(vm.parseJsonUint(json, ".postingTimeframe"));
        configTipHash = vm.parseJsonBytes32(json, ".configTipHash");
        parentTipId = uint64(vm.parseJsonUint(json, ".parentTipId"));
        parentTipHash = vm.parseJsonBytes32(json, ".parentTipHash");
        tipId = uint64(vm.parseJsonUint(json, ".tipId"));
        tipHash = vm.parseJsonBytes32(json, ".tipHash");
    }

    function _checkpoint() internal view returns (FerrataVault.Checkpoint memory c) {
        (,,,, c) = abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
    }

    /// @dev `S` of a state at the parent tip under `keys`, with no burns, no imports and no sunset, as
    ///      checkpoint-core's state_digest and attestor-core's key_set_hash define it (integers big-endian).
    function _state(bytes32[] memory keys) internal view returns (bytes32) {
        bytes memory packed;
        for (uint256 i = 0; i < keys.length; i++) {
            packed = bytes.concat(packed, keys[i]);
        }
        bytes32 keySetHash =
            sha256(abi.encodePacked("ferrata/keyset/v2", uint32(keys.length), postingTimeframe, packed));
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

    function _keys(bytes32 a) internal pure returns (bytes32[] memory k) {
        k = new bytes32[](1);
        k[0] = a;
    }

    /// @dev The release image the vault pins (none is proven in the real-blocks run).
    function _releaseImageId() internal view virtual returns (bytes32) {
        return keccak256("ferrata test: no release in this run");
    }

    /// @dev RISC Zero's verifier and stop, then a vault of `mode` at the address the journal binds, on its chain.
    function _deploy(FerrataVault.Mode mode, bytes32 imageId, bytes32 genesisState, uint32 nMin_, uint32 tMin_)
        internal
    {
        (, uint256 chainId, address at, bytes32 lezConfigId,) =
            abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
        vm.chainId(chainId);
        RiscZeroGroth16Verifier groth16 =
            new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        RiscZeroVerifierEmergencyStop estop = new RiscZeroVerifierEmergencyStop(groth16, freezer);
        FerrataVault.Deployment memory d = FerrataVault.Deployment({
            verifier: IRiscZeroVerifier(address(estop)),
            checkpointImageId: imageId,
            releaseImageId: _releaseImageId(),
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

/// @notice A rule (B) vault constructed with the lab channel's key set (its genesis state is the proof's parent).
contract RealBlocksTest is RealBlocksFixture {
    function setUp() public {
        _load();
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 1, 1);
    }

    /// @notice The state the vault starts from, recomputed here from the lab channel's one key, its posting timeframe
    ///         and configuration tip as the lab's node reports them and the lab's block 7, is the parent the real
    ///         proof starts from; the proof names the lab's block 202 as its tip, a key set of n = 1, t = 1, no burns.
    function test_realBlocks_theProvenParentIsTheLabChannelsKeySet() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(c.parentState, vault.genesisState());
        assertEq(c.tipId, tipId);
        assertEq(c.tipHash, tipHash);
        assertEq(c.n, 1);
        assertEq(c.t, 1);
        assertEq(c.nMin, 1);
        assertEq(c.tMin, 1);
        assertEq(c.burnCount, 0, "no bridge program runs in the lab");
        assertEq(c.echoNonce, bytes32(0));
    }

    function test_realBlocks_candidateThenFinalAfterTheWait() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.prank(makeAddr("anyone"));
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        (bytes32 digest,,,,,,,) = vault.candidates(c.newState);
        assertEq(digest, sha256(journal), "the vault keeps the digest the real seal was verified against");
        assertEq(vault.finalState(), c.parentState, "nothing final before the wait");

        vm.warp(at + WAIT);
        vm.prank(makeAddr("finaliser"));
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        (bool accepted, uint256 finalizedAt) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
        assertEq(finalizedAt, at + WAIT);
    }

    /// @notice Finalisation at once, and one second before the wait ends, is refused; the candidate stays pending.
    function test_realBlocks_earlyFinalisationIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(at + WAIT - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.parentState);
        vm.warp(at + WAIT);
        vault.finalizeAttested(c); // control
        assertEq(vault.finalState(), c.newState);
    }

    /// @notice One flipped byte in the selector or in any of the proof's eight words.
    function test_realBlocks_aFlippedByteInTheSealIsRefused() public {
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

    /// @notice The real seal does not carry another tip, another state or another key-set size.
    function test_realBlocks_anotherJournalUnderTheRealSealIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        for (uint256 field = 0; field < 4; field++) {
            FerrataVault.Checkpoint memory x = abi.decode(abi.encode(c), (FerrataVault.Checkpoint));
            if (field == 0) x.tipHash = keccak256("another tip");
            if (field == 1) x.tipId += 1;
            if (field == 2) x.newState = keccak256("another state");
            if (field == 3) x.n = 2;
            vm.expectRevert(VerificationFailed.selector);
            vault.submitCandidate(x, seal);
        }
    }
}

/// @notice Vaults constructed otherwise than with the lab channel's key set and the devnet's floors.
contract RealBlocksWrongVaultTest is RealBlocksFixture {
    function setUp() public {
        _load();
    }

    /// @notice A vault whose genesis state names another key at the same tip (the attestation key in place of the
    ///         channel key): the proof starts from the lab channel's key set, so it is refused.
    function test_realBlocks_aVaultOfAnotherKeyRefusesIt() public {
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(_keys(attestationKey)), 1, 1);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    /// @notice A vault whose genesis state names a larger key set holding the channel key (n = 2): refused.
    function test_realBlocks_aVaultOfALargerKeySetRefusesIt() public {
        bytes32[] memory both = new bytes32[](2);
        both[0] = labKey;
        both[1] = attestationKey;
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(both), 1, 1);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    /// @notice A vault whose floors ask for a larger committee than the devnet has (nMin = 3, tMin = 2).
    function test_realBlocks_aVaultWithLargerFloorsRefusesIt() public {
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 3, 2);
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    /// @notice A rule (A) vault asks for rule (A)'s program, so the proof of checkpoint_b is refused.
    function test_realBlocks_aRuleAVaultRefusesIt() public {
        _deploy(FerrataVault.Mode.A, CHECKPOINT_A_IMAGE_ID, _state(_keys(labKey)), 1, 1);
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }
}

/// @notice Gas with the real verifier and the real seal, one transaction per step
///         (`forge test --isolate --match-contract RealBlocksGas -vv`).
contract RealBlocksGas is RealBlocksFixture {
    function setUp() public {
        _load();
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 1, 1);
    }

    function test_gas_realBlocks_ruleB() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        console2.log("real blocks: submitCandidate, real checkpoint_b seal", vm.lastCallGas().gasTotalUsed);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        console2.log("real blocks: finalizeAttested after the wait", vm.lastCallGas().gasTotalUsed);
    }
}
