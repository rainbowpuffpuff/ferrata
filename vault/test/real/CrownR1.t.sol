// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {RealBlocksFixture} from "./RealBlocks.t.sol";

/// @notice Rehearsal R1 of the crown run's proofs (e2e/crown/README.md, stages 7 and 9): the CROWN build's checkpoint_b
///         (its image id at the crown build path; sources with the ImportRoot decoding fix) proven on one NVIDIA L4 over
///         the lab's real LEZ blocks 8..=202, with a STAND-IN Sepolia configuration (chain id 11155111, the crown plan's
///         predicted vault address, labelled bridge fields), wrapped to Groth16; and the crown build's release program on
///         a STAND-IN synthetic burn, proven on the build box's CPU and wrapped. The vault is v5 in mode (B) with RISC
///         Zero's unmodified Groth16 verifier behind its emergency stop.
abstract contract CrownProofsFixture is RealBlocksFixture {
    bytes32 internal constant CROWN_CHECKPOINT_B_IMAGE_ID =
        0x7e51065ffd0a8004fbdd0452871c4a77da7b59e6499365f43d61363fd4307754;
    bytes32 internal constant CROWN_RELEASE_IMAGE_ID =
        0x20595dc54bbd80cf487e4666286db9aab7a048e2e0d7646b42ec6d3c90ca2625;

    bytes internal releaseJournal;
    bytes internal releaseSeal;
    uint256 internal releaseCount;

    function _fixture() internal pure virtual returns (string memory);

    function _releaseImageId() internal pure override returns (bytes32) {
        return CROWN_RELEASE_IMAGE_ID;
    }

    function setUp() public {
        vm.skip(true, "crown-r1/crown-r4 are records of the crown programs of 10-11 Nov 2026 under RISC Zero 3.0.5; their inputs are not recorded, so they were not proven again under 3.1 (test/real/README.md)");
        string memory json = vm.readFile(_fixture());
        require(vm.parseJsonBytes32(json, ".imageId") == CROWN_CHECKPOINT_B_IMAGE_ID, "image id");
        require(vm.parseJsonBytes32(json, ".releaseImageId") == CROWN_RELEASE_IMAGE_ID, "release image id");
        journal = vm.parseJsonBytes(json, ".journal");
        seal = vm.parseJsonBytes(json, ".seal");
        labKey = vm.parseJsonBytes32(json, ".labChannelKey");
        postingTimeframe = uint32(vm.parseJsonUint(json, ".postingTimeframe"));
        configTipHash = vm.parseJsonBytes32(json, ".configTipHash");
        parentTipId = uint64(vm.parseJsonUint(json, ".parentTipId"));
        parentTipHash = vm.parseJsonBytes32(json, ".parentTipHash");
        tipId = uint64(vm.parseJsonUint(json, ".tipId"));
        tipHash = vm.parseJsonBytes32(json, ".tipHash");
        releaseJournal = vm.parseJsonBytes(json, ".releaseJournal");
        releaseSeal = vm.parseJsonBytes(json, ".releaseSeal");
        releaseCount = vm.parseJsonUint(json, ".releaseCount");
        _deploy(FerrataVault.Mode.B, CROWN_CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 1, 1);
        _deposits();
    }

    /// @dev The deposits the run made before the checkpoint (none by default).
    function _deposits() internal virtual {}

    /// @notice The vault's genesis (recomputed from the lab channel's key and block 7) is the proof's parent; the proof
    ///         is bound to Sepolia's chain id and the vault address the journal names.
    function test_crown_theProvenParentIsTheVaultsGenesis() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(c.parentState, vault.genesisState());
        assertEq(block.chainid, 11_155_111);
        assertEq(c.tipId, tipId);
        assertEq(c.tipHash, tipHash);
        assertEq(c.n, 1);
        assertEq(c.t, 1);
    }

    function test_crown_candidateThenFinalAfterTheWait() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.prank(makeAddr("anyone"));
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(at + WAIT);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        (bool accepted,) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
    }

    function test_crown_aFlippedSealByteIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory bad = bytes.concat(seal);
        bad[100] ^= 0x01;
        vm.expectRevert();
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
    }

    /// @notice The seal is bound to the crown image: a vault pinning the real-blocks proof's image id refuses it.
    function test_crown_aVaultOfTheEarlierImageRefusesIt() public {
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 1, 1);
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate(c, seal);
    }

    /// @notice The release seal verifies for the crown release image and its journal, and for nothing else.
    function test_crown_theReleaseSealVerifiesForTheCrownReleaseImage() public {
        RiscZeroGroth16Verifier groth16 =
            new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        bytes32 digest = sha256(releaseJournal); // before expectRevert: the precompile call would be the "next call"
        groth16.verify(releaseSeal, CROWN_RELEASE_IMAGE_ID, digest);
        vm.expectRevert(VerificationFailed.selector);
        groth16.verify(releaseSeal, CROWN_CHECKPOINT_B_IMAGE_ID, digest);
    }
}

/// @notice R1: the lab's real blocks 8..=202, stand-in configuration, synthetic release (checkpoint/realblocks/out-crown-r1).
contract CrownR1Test is CrownProofsFixture {
    function _fixture() internal pure override returns (string memory) {
        return "test/real/fixtures/crown-r1.json";
    }
}

/// @notice R4: the CROWN devnet's own blocks 18..=92 of the LEZ part's STAND-IN rehearsal session (the attestation and
///         merged bridge programs at the crown path; bridge Init, Register ETH, ImportRoot n = 1 and 2, a public and a
///         private claim, a public and a private burn, the private ones with real proofs; the Ethereum side an anvil fork
///         with a stand-in beacon committee): the checkpoint counts both burns and the imported leaf count 2; the
///         releases of both burns are proven from the LEZ wallet's own notes.
contract CrownR4Test is CrownProofsFixture {
    function _fixture() internal pure override returns (string memory) {
        return "test/real/fixtures/crown-r4.json";
    }

    /// @dev ETH registered (0.001 / 0.01 / 0.1 ETH, lezUnit 1e12, no fees, no caps) and the run's two 0.001 ETH deposits:
    ///      the checkpoint says LEZ imported two leaves, and the vault refuses an imported count above its own
    ///      (ImportedLeafCountTooHigh) -- as it did here before the deposits were added.
    function _deposits() internal override {
        uint256[] memory d = new uint256[](3);
        (d[0], d[1], d[2]) = (0.001 ether, 0.01 ether, 0.1 ether);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        vm.prank(admin);
        vault.registerToken(
            address(0),
            FerrataVault.TokenParams({
                denominations: d,
                relayFees: new uint256[](3),
                lezUnit: 1e12,
                unitWeight: 1,
                depositCap: unlimited,
                releaseCap: unlimited,
                releaseDelay: 0,
                lezDefinition: bytes32(0),
                decimals: 18,
                checkpointFees: new uint256[](3)
            })
        );
        address depositor = makeAddr("crown depositor");
        vm.deal(depositor, 1 ether);
        for (uint64 j = 0; j < 2; j++) {
            vm.prank(depositor);
            vault.depositETH{value: 0.001 ether}(0, keccak256(abi.encode("crown R4 deposit", j)), j);
        }
        assertEq(vault.nextLeafIndex(), 2);
    }

    /// @notice The whole return leg with real proofs: the checkpoint final after the hour, then both burns (the public
    ///         and the private one) released to the recipient the LEZ wallet bound in COut, each with its real seal; a
    ///         second release of the same burn is refused.
    function test_crownR4_bothBurnsAreReleasedWithTheirRealSeals() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        string memory json = vm.readFile(_fixture());
        for (uint256 i = 0; i < releaseCount; i++) {
            string memory at = string.concat(".releases[", vm.toString(i), "]");
            FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
                token: vm.parseJsonAddress(json, string.concat(at, ".token")),
                denomIndex: uint8(vm.parseJsonUint(json, string.concat(at, ".denomIndex"))),
                recipient: vm.parseJsonAddress(json, string.concat(at, ".recipient")),
                relayer: vm.parseJsonAddress(json, string.concat(at, ".relayer")),
                relayFee: vm.parseJsonUint(json, string.concat(at, ".relayFee")),
                burnRoot: vm.parseJsonBytes32(json, string.concat(at, ".burnRoot")),
                nullifier: vm.parseJsonBytes32(json, string.concat(at, ".nullifier"))
            });
            bytes memory releaseSealI = vm.parseJsonBytes(json, string.concat(at, ".seal"));
            assertEq(r.burnRoot, c.burnRoot);
            uint256 before = r.recipient.balance;
            vault.release(r, releaseSealI);
            assertEq(r.recipient.balance - before, 0.001 ether, "no fees registered here: the whole denomination");
            assertTrue(vault.nullifierUsed(r.nullifier));
            vm.expectRevert();
            vault.release(r, releaseSealI);
        }
        assertEq(address(vault).balance, 0, "both deposits released");
    }

    function test_crownR4_theCheckpointCountsBothBurnsAndBothImports() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(c.burnCount, 2);
        assertEq(c.importedLeafCount, 2);
        assertEq(c.burnCount, releaseCount);
    }
}
