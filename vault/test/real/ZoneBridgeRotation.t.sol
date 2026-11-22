// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// zone-bridge-2 (checkpoint/attestor/zone-bridge.sh with ZB_ROTATE=1, results in checkpoint/attestor/RESULTS.md §9,
// records in e2e/runs/zone-bridge-2/): the zone of zone-bridge-1, where after session A a fifth sequencer's key was
// staked on LEZ (upstream's `sequencer_service stake`) and the zone's committee proposed the channel's new key set:
// the ChannelConfigOp Bedrock applied, signed by three of the four keys, is in the checkpoint's input as it was
// inscribed, so the proof applies it to the parent's four keys; the journal is four of the five new keys' attestations
// (n = 5, t = 4). The vault's genesis is over the four keys at T0, its floors nMin 4 and tMin 3.

import {FerrataVault} from "../../src/FerrataVault.sol";
import {ZoneBridgeFixture} from "./ZoneBridge.t.sol";

contract ZoneBridgeRotationTest is ZoneBridgeFixture {
    function _path() internal pure virtual override returns (string memory) {
        return "test/real/fixtures/zone-bridge-rotation.json";
    }

    function _n() internal pure override returns (uint32) {
        return 5;
    }

    function _t() internal pure override returns (uint32) {
        return 4;
    }

    /// @notice The parent is the four keys of T0; the proof's key set is the committee's five, which keep the four.
    function test_zoneBridgeRotation_theProofAppliedTheCommitteesFiveKeySet() public view {
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
    }

    /// @notice A vault whose genesis is the rotated set does not continue from this proof's parent.
    function test_zoneBridgeRotation_aVaultWhoseGenesisIsTheNewSetRefusesIt() public {
        bytes32[] memory r = new bytes32[](rotatedKeys.length);
        for (uint256 i = 0; i < rotatedKeys.length; i++) {
            r[i] = rotatedKeys[i];
        }
        _deploy(FerrataVault.Mode.B, imageId, _state(r), 4, 3);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }
}

/// @notice zone-bridge-4: the rotation run again (22 November) with each attestor checking every tip against its own
/// sequencer's LEZ node and the last round taken while the zone runs (checkpoint/attestor/RESULTS.md §10.1).
contract ZoneBridgeRotationLezCheckedTest is ZoneBridgeRotationTest {
    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-4.json";
    }
}
