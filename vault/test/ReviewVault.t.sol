// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Tests of the agent review of 12 November 2026 (docs/REVIEW-vault.md). They pin what the vault does where
///         nothing pinned it or where a comment says otherwise; none changes the vault.
contract ReviewVaultTest is VaultTestBase {
    /// @dev Storage slot 37: `freezeAuthority` in the low 20 bytes and `retired` at byte offset 20 (FerrataVault v5's
    ///      layout). The LEZ bridge program's keyless `Sunset` attests this slot and reads byte 11 of its big-endian
    ///      word (lez/bridge-wire: `SLOT_RETIRED`, `RETIRED_BYTE`); the vault's own tests read only slots 0-2.
    uint256 internal constant SLOT_RETIRED = 37;
    uint256 internal constant RETIRED_BYTE = 11;

    function _slot37() internal view returns (bytes32) {
        return vm.load(address(vault), bytes32(SLOT_RETIRED));
    }

    function test_review_slot37_holdsTheFreezeAuthorityAndTheRetiredByteTheLezSideReads() public {
        bytes32 before = _slot37();
        assertEq(before, bytes32(uint256(uint160(freezer))), "slot 37 is the freeze authority, retired = false");
        assertEq(uint8(before[RETIRED_BYTE]), 0, "byte 11 is 0 before retirement");

        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());

        bytes32 later = _slot37();
        assertEq(later, bytes32(uint256(uint160(freezer)) | (uint256(1) << 160)), "retired packed at offset 20");
        assertEq(uint8(later[RETIRED_BYTE]), 1, "byte 11 of the big-endian word is the retired flag");
    }

    /// @dev A keyless retirement writes the same byte (anyone, from `retireOpensAt()` on).
    function test_review_slot37_keylessRetirementSetsTheSameByte() public {
        vm.warp(vault.retireOpensAt());
        vm.prank(alice);
        vault.retire();
        assertEq(uint8(_slot37()[RETIRED_BYTE]), 1);
        assertEq(address(uint160(uint256(_slot37()))), freezer, "the freeze authority is untouched");
    }

    /// @dev FerrataVault.sol:232-234 and README ("Constructor bounds") bound `revokeFrom`'s loop by
    ///      `revokeWindow / deltaFloor + 1` because "two finalisations are at least `deltaFloor` apart". The vault
    ///      alone does not guarantee that: a no-op checkpoint (`newState == parentState`) does not move the epoch, so
    ///      it and a state-changing candidate submitted beside it finalise in the same block. The pinned checkpoint
    ///      programs never produce a no-op (the state commits to the tip and a checkpoint needs a new block), so the
    ///      bound holds for them; this test shows it is theirs, not the vault's.
    function test_review_twoFinalisationsShareABlock_soTheRevokeBoundRestsOnTheProgram() public {
        FerrataVault.Deployment memory d = _deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.revokeWindow = 2 hours; // revokeWindow / deltaFloor = 2: "at most 3 checkpoints marked"
        FerrataVault v = new FerrataVault(d);

        uint256 firstFinalisedAt;
        for (uint256 round = 0; round < 3; round++) {
            FerrataVault.Checkpoint memory noOp = _nextCheckpoint(v, keccak256(abi.encode("root", round)), 0, false);
            noOp.newState = noOp.parentState;
            FerrataVault.Checkpoint memory step = _nextCheckpoint(v, keccak256(abi.encode("root", round)), 0, false);
            _submit(v, LEZ_CONFIG_ID, noOp);
            _submit(v, LEZ_CONFIG_ID, step);
            uint256 a = _finalizeAfterDelta(v, LEZ_CONFIG_ID, noOp);
            uint256 b = _finalizeAfterDelta(v, LEZ_CONFIG_ID, step);
            assertEq(b, a + 1);
            (,,,,,,,,, uint64 atA,) = v.checkpoints(a);
            (,,,,,,,,, uint64 atB,) = v.checkpoints(b);
            assertEq(atA, atB, "two finalisations in one block");
            if (round == 0) firstFinalisedAt = atA;
        }
        assertEq(v.checkpointCount(), 6);
        assertEq(block.timestamp, firstFinalisedAt + d.revokeWindow, "the last second the first one is revocable");

        vm.prank(freezer);
        v.revokeFrom(0);
        for (uint256 i = 0; i < 6; i++) {
            (,,,,,, bool revoked,,,,) = v.checkpoints(i);
            assertTrue(revoked);
        }
        assertGt(v.checkpointCount(), d.revokeWindow / d.deltaFloor + 1, "more than the documented bound");
    }
}
