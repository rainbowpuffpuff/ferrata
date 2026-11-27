// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// The rotation run on LEZ v0.4.0-rc3 under RISC Zero 3.1 (e2e/runs/zone-bridge-rc3-rotation/): the zone of
// zone-bridge-rc3-multi (four LEZ v0.4.0-rc3 sequencers on one Logos Blockchain 0.3.5-rc.1 node, the bridge program
// `ferrata_bridge_b1`), where after session A a fifth sequencer's key was staked on LEZ (upstream's `sequencer_service
// stake`) and the zone's committee proposed the channel's new key set (checkpoint/attestor/zone-bridge.sh with
// ZB_ROTATE=1): the ChannelConfigOp Bedrock applied, signed by three of the four keys, is in the checkpoint's input as it
// was inscribed, so the proof (checkpoint_b_0_3_5_31) applies it to the parent's four keys; the journal is four of the
// five new keys' attestations (n = 5, t = 4). The vault's genesis is over the four keys at T0, its floors nMin 4 and
// tMin 3; RISC Zero's Groth16 verifier 3.1.0 behind its emergency stop. The fixture:
// ZB_FIXTURE=zone-bridge-rc3-rotation checkpoint/attestor/zone-bridge.sh fixture. The tests of ZoneBridgeRc3Fixture run
// on it too (the checkpoint, both releases once each, the floors, a flipped seal byte, redirected and replayed releases).

import {FerrataVault} from "../../src/FerrataVault.sol";
import {ZoneBridgeRc3Fixture} from "./ZoneBridgeRc3Multi.t.sol";

contract ZoneBridgeRc3RotationTest is ZoneBridgeRc3Fixture {
    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-rc3-rotation.json";
    }

    function _n() internal pure override returns (uint32) {
        return 5;
    }

    function _t() internal pure override returns (uint32) {
        return 4;
    }

    /// @notice The parent is the four keys of T0; the proof's key set is the committee's five, which keep the four.
    function test_rc3Rotation_theProofAppliedTheCommitteesFiveKeySet() public view {
        assertEq(keys.length, 4);
        assertEq(rotatedKeys.length, 5);
        for (uint256 i = 0; i < keys.length; i++) {
            bool kept;
            for (uint256 j = 0; j < rotatedKeys.length; j++) {
                kept = kept || rotatedKeys[j] == keys[i];
            }
            assertTrue(kept, "each of the four keys is in the new set");
        }
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(c.parentState, vault.genesisState());
        assertEq(c.n, 5);
        assertEq(c.t, 4);
    }

    /// @notice A vault whose genesis is the rotated set does not continue from this proof's parent.
    function test_rc3Rotation_aVaultWhoseGenesisIsTheNewSetRefusesIt() public {
        bytes32[] memory r = new bytes32[](rotatedKeys.length);
        for (uint256 i = 0; i < rotatedKeys.length; i++) {
            r[i] = rotatedKeys[i];
        }
        _deploy(FerrataVault.Mode.B, imageId, _state(r), 4, 3);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }
}
