// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// The bridge program on a zone of four LEZ sequencers (checkpoint/attestor/zone-bridge.sh, results in
// checkpoint/attestor/RESULTS.md §8, records in e2e/runs/zone-bridge-1/): the final programs of docs/image-ids.md;
// crown-driver's sessions A and B on the zone (deploy, pairing, registration, imports, a public and a private claim,
// a public and a private burn); an attestor beside each sequencer; the rule (B) checkpoint over the zone's real blocks
// with three of the four attestations (n = 4, t = 3), proven with checkpoint_b and wrapped to Groth16; both burns'
// releases proven with the release program. The vault is v5 in mode (B) with nMin 4 and tMin 3, genesis = the zone's
// state at T0 over its four keys, RISC Zero's real Groth16 verifier behind its emergency stop, ETH registered as
// script/DeployVault.s.sol registers it, at the address and chain id the journal names (the run's local anvil:
// Sepolia's chain id, the address CREATE(deployer, 3)). The fixture: zone-bridge.sh fixture.

import {console2} from "forge-std/Test.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {RuleBMultiFixture} from "./RuleBMulti.t.sol";

abstract contract ZoneBridgeFixture is RuleBMultiFixture {
    /// @dev docs/image-ids.md, RISC Zero 3.1 (the fixtures were proven again under 3.1: vault/test/real/README.md).
    bytes32 internal constant CHECKPOINT_B_IMAGE_ID = 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0;
    bytes32 internal constant RELEASE_IMAGE_ID = 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd;

    /// @dev The key set the journal is over (after the proof's rotations) and its threshold.
    function _n() internal pure virtual returns (uint32);
    function _t() internal pure virtual returns (uint32);

    function _releaseImageId() internal pure override returns (bytes32) {
        return RELEASE_IMAGE_ID;
    }

    function setUp() public {
        _load();
        require(imageId == CHECKPOINT_B_IMAGE_ID, "the fixture's checkpoint image is not docs/image-ids.md's");
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 4, 3);
        _registerAndDeposit();
    }

    /// @dev ETH as DeployVault registers it (0.001 / 0.01 / 0.1 ETH; relay fee 0.2 %, checkpoint fee 0.1 %; lezUnit
    ///      1e12; no caps; release delay 120 s) and the run's two deposits of 0.001 ETH plus the relay fee (their
    ///      commitments are the run's, from crown.json): the checkpoint says LEZ imported two leaves.
    function _registerAndDeposit() internal {
        uint256[] memory d = new uint256[](3);
        (d[0], d[1], d[2]) = (0.001 ether, 0.01 ether, 0.1 ether);
        uint256[] memory relay = new uint256[](3);
        (relay[0], relay[1], relay[2]) = (2e12, 2e13, 2e14);
        uint256[] memory fees = new uint256[](3);
        (fees[0], fees[1], fees[2]) = (1e12, 1e13, 1e14);
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        vm.prank(admin);
        vault.registerToken(
            address(0),
            FerrataVault.TokenParams({
                denominations: d,
                relayFees: relay,
                lezUnit: 1e12,
                unitWeight: 3000,
                depositCap: unlimited,
                releaseCap: unlimited,
                releaseDelay: 120,
                lezDefinition: bytes32(0),
                decimals: 18,
                checkpointFees: fees
            })
        );
        string memory json = vm.readFile(_path());
        address depositor = vm.parseJsonAddress(json, ".depositor");
        vm.deal(depositor, 1 ether);
        for (uint64 j = 0; j < 2; j++) {
            bytes32 commitment = vm.parseJsonBytes32(json, string.concat(".commitments[", vm.toString(j), "]"));
            vm.prank(depositor);
            vault.depositETH{value: 0.001 ether + 2e12}(0, commitment, j);
        }
        assertEq(vault.nextLeafIndex(), 2);
    }

    function test_zoneBridge_theJournalIsTOfNOverTheZonesKeys() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(keys.length, 4, "the vault's genesis is over the zone's four keys");
        assertEq(c.parentState, vault.genesisState());
        assertEq(c.n, _n());
        assertEq(c.t, _t());
        assertEq(c.nMin, 4);
        assertEq(c.tMin, 3);
        assertEq(c.tipId, tipId);
        assertEq(c.tipHash, tipHash);
        assertEq(c.burnCount, 2, "the zone's public and private burn");
        assertEq(c.importedLeafCount, 2, "both deposits imported on the zone");
        assertEq(c.echoNonce, bytes32(0), "rule (B): no beacon");
    }

    /// @notice The return leg on the zone: the checkpoint final after the hour (not before), its burn root accepted,
    ///         then both burns released to the recipients the LEZ wallet bound, each with its real release seal; a
    ///         second release of the same burn is refused.
    function test_zoneBridge_checkpointThenBothReleases() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.prank(makeAddr("anyone"));
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        vm.warp(at + WAIT - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(at + WAIT);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        (bool accepted,) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted, "the burn root is accepted");
        vm.warp(block.timestamp + 121);
        string memory json = vm.readFile(_path());
        uint256 count = vm.parseJsonUint(json, ".releaseCount");
        assertEq(count, 2);
        for (uint256 i = 0; i < count; i++) {
            string memory p = string.concat(".releases[", vm.toString(i), "]");
            FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
                token: vm.parseJsonAddress(json, string.concat(p, ".token")),
                denomIndex: uint8(vm.parseJsonUint(json, string.concat(p, ".denomIndex"))),
                recipient: vm.parseJsonAddress(json, string.concat(p, ".recipient")),
                relayer: vm.parseJsonAddress(json, string.concat(p, ".relayer")),
                relayFee: vm.parseJsonUint(json, string.concat(p, ".relayFee")),
                burnRoot: vm.parseJsonBytes32(json, string.concat(p, ".burnRoot")),
                nullifier: vm.parseJsonBytes32(json, string.concat(p, ".nullifier"))
            });
            bytes memory releaseSeal = vm.parseJsonBytes(json, string.concat(p, ".seal"));
            assertEq(r.burnRoot, c.burnRoot);
            uint256 before = r.recipient.balance;
            vault.release(r, releaseSeal);
            uint256 paid = r.recipient.balance - before;
            assertEq(paid, vm.parseJsonUint(json, string.concat(p, ".paidOnAnvil")), "as the run's anvil paid");
            assertTrue(vault.nullifierUsed(r.nullifier));
            vm.expectRevert();
            vault.release(r, releaseSeal);
        }
    }

    function test_zoneBridge_aFlippedSealByteOrFewerSignaturesAreRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory bad = bytes.concat(seal);
        bad[bad.length - 1] = bad[bad.length - 1] ^ 0x01;
        vm.expectRevert();
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
        FerrataVault.Checkpoint memory x = _checkpoint();
        x.t = 2;
        vm.expectRevert(FerrataVault.KeySetBelowFloor.selector);
        vault.submitCandidate(x, seal);
    }

    function test_gas_zoneBridge() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        console2.log("zone-bridge, 2 burns: submitCandidate", vm.lastCallGas().gasTotalUsed);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        console2.log("zone-bridge, 2 burns: finalizeAttested", vm.lastCallGas().gasTotalUsed);
    }
}

/// @notice zone-bridge-1: the four sequencers' keys throughout; three of the four attestations (n = 4, t = 3).
contract ZoneBridgeTest is ZoneBridgeFixture {
    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge.json";
    }

    function _n() internal pure override returns (uint32) {
        return 4;
    }

    function _t() internal pure override returns (uint32) {
        return 3;
    }
}

/// @notice zone-bridge-3: zone-bridge-1's run again (22 November) with each attestor checking every tip against its own
/// sequencer's LEZ node and its last round taken while the zone runs (checkpoint/attestor/RESULTS.md §10.1); the four
/// sequencers' keys throughout; three of the four attestations (n = 4, t = 3).
contract ZoneBridgeLezCheckedTest is ZoneBridgeFixture {
    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-3.json";
    }

    function _n() internal pure override returns (uint32) {
        return 4;
    }

    function _t() internal pure override returns (uint32) {
        return 3;
    }
}
