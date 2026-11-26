// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// RFP-022 F.7 on LEZ v0.4.0-rc3 with more than one sequencer key: the return leg over a zone of four LEZ v0.4.0-rc3
// sequencers on one Logos Blockchain 0.3.5-rc.1 node (upstream's four-sequencer devnet, the turn rotating among the
// four keys), the bridge program of docs/image-ids.md (`ferrata_bridge_b1`) deployed on it by crown-driver (sessions A
// and B: a public and a private claim, a public and a private burn), one ferrata_attestor beside each sequencer, three of
// the four attestations in the rule (B) checkpoint (n = 4, t = 3), proven under RISC Zero 3.1 with checkpoint_b_0_3_5
// and wrapped to Groth16; both burns' releases proven with release_31. The vault is v5 in mode (B) with floors nMin 4,
// tMin 3, genesis = the zone's state at T0 over its four keys, behind RISC Zero's Groth16 verifier 3.1.0 and its
// emergency stop, at the address and chain id the journal names. Records: e2e/runs/zone-bridge-rc3-multi/; the fixture:
// ZB_FIXTURE=zone-bridge-rc3-multi checkpoint/attestor/zone-bridge.sh fixture.

import {console2} from "forge-std/Test.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {RuleBMultiFixture} from "./RuleBMulti.t.sol";

abstract contract ZoneBridgeRc3Fixture is RuleBMultiFixture {
    /// @dev docs/image-ids.md, RISC Zero 3.1: checkpoint_b_0_3_5_31, checkpoint_b_31 (the d480efc6 layout), release_31.
    bytes32 internal constant CHECKPOINT_B_0_3_5_IMAGE_ID =
        0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04;
    bytes32 internal constant CHECKPOINT_B_IMAGE_ID = 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0;
    bytes32 internal constant RELEASE_IMAGE_ID = 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd;
    /// @dev The selector of RISC Zero's Groth16 verifier 3.1.0 (a seal's first four bytes).
    bytes4 internal constant SELECTOR_31 = 0xe71e6620;

    /// @dev The key set the journal is over (after the proof's rotations) and its threshold.
    function _n() internal pure virtual returns (uint32);
    function _t() internal pure virtual returns (uint32);

    function _releaseImageId() internal pure override returns (bytes32) {
        return RELEASE_IMAGE_ID;
    }

    function setUp() public {
        _load();
        require(imageId == CHECKPOINT_B_0_3_5_IMAGE_ID, "the fixture's checkpoint image is not checkpoint_b_0_3_5_31");
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

    /// @dev The fixture's release `i` and its seal.
    function _releaseAt(uint256 i) internal view returns (FerrataVault.ReleaseRequest memory r, bytes memory s) {
        string memory json = vm.readFile(_path());
        string memory p = string.concat(".releases[", vm.toString(i), "]");
        r = FerrataVault.ReleaseRequest({
            token: vm.parseJsonAddress(json, string.concat(p, ".token")),
            denomIndex: uint8(vm.parseJsonUint(json, string.concat(p, ".denomIndex"))),
            recipient: vm.parseJsonAddress(json, string.concat(p, ".recipient")),
            relayer: vm.parseJsonAddress(json, string.concat(p, ".relayer")),
            relayFee: vm.parseJsonUint(json, string.concat(p, ".relayFee")),
            burnRoot: vm.parseJsonBytes32(json, string.concat(p, ".burnRoot")),
            nullifier: vm.parseJsonBytes32(json, string.concat(p, ".nullifier"))
        });
        s = vm.parseJsonBytes(json, string.concat(p, ".seal"));
    }

    function _final() internal returns (FerrataVault.Checkpoint memory c) {
        c = _checkpoint();
        vault.submitCandidate(c, seal);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
    }

    /// @notice The journal: t of n over the zone's keys (four at T0, the vault's genesis; the floors 4 and 3), the two
    ///         burns and the two imported deposits of the run; every seal is RISC Zero 3.1's.
    function test_rc3Multi_theJournalIsTOfNOverTheZonesKeys() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(keys.length, 4, "the vault's genesis is over the zone's four keys at T0");
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
        // casting to 'bytes4' is safe because a seal's first four bytes are its selector (RiscZeroGroth16Verifier)
        // forge-lint: disable-next-line(unsafe-typecast)
        assertEq(bytes4(seal), SELECTOR_31, "the checkpoint's seal is RISC Zero 3.1's");
        for (uint256 i = 0; i < 2; i++) {
            (, bytes memory s) = _releaseAt(i);
            // casting to 'bytes4' is safe because a seal's first four bytes are its selector (RiscZeroGroth16Verifier)
            // forge-lint: disable-next-line(unsafe-typecast)
            assertEq(bytes4(s), SELECTOR_31, "a release's seal is RISC Zero 3.1's");
        }
    }

    /// @notice The checkpoint final after the hour (not before), its burn root accepted, both burns released with
    ///         their real release seals to the recipients their burns name, each exactly once.
    function test_rc3Multi_checkpointThenBothReleasesEachOnce() public {
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
        assertEq(vm.parseJsonUint(json, ".releaseCount"), 2);
        for (uint256 i = 0; i < 2; i++) {
            (FerrataVault.ReleaseRequest memory r, bytes memory s) = _releaseAt(i);
            assertEq(r.burnRoot, c.burnRoot);
            uint256 before = r.recipient.balance;
            vault.release(r, s);
            uint256 paid = vm.parseJsonUint(json, string.concat(".releases[", vm.toString(i), "].paidOnAnvil"));
            assertEq(r.recipient.balance - before, paid, "paid as on the run's anvil");
            assertTrue(vault.nullifierUsed(r.nullifier));
            vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
            vault.release(r, s);
        }
    }

    /// @notice Fewer signatures than the floor are not enough: the journal claiming t = 2 is refused by the floors; a
    ///         vault deployed with other floors refuses this journal (they are bound in it); a vault whose genesis is
    ///         another key set (the first three keys) refuses it at the parent.
    function test_rc3Multi_belowTheFloorOtherFloorsOrAnotherKeySetAreRefused() public {
        FerrataVault.Checkpoint memory x = _checkpoint();
        x.t = 2;
        vm.expectRevert(FerrataVault.KeySetBelowFloor.selector);
        vault.submitCandidate(x, seal);
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 3, 2);
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(_checkpoint(), seal);
        _deploy(FerrataVault.Mode.B, imageId, _state(_first(3)), 4, 3);
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(_checkpoint(), seal);
    }

    /// @notice A flipped byte in the checkpoint's seal, or the same seal under the d480efc6 layout's checkpoint_b (what
    ///         the November vaults pin), is refused by RISC Zero's verifier.
    function test_rc3Multi_aFlippedSealByteOrAnotherImageIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory bad = bytes.concat(seal);
        bad[bad.length - 1] = bad[bad.length - 1] ^ 0x01;
        vm.expectRevert();
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(keys), 4, 3);
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, seal);
    }

    /// @notice RFP-022 Sec.3 on this run's real seals: the recipient, the relayer, the relay fee or the nullifier
    ///         changed alone is refused by the verifier; the request as proven is paid once and its replay refused.
    function test_rc3Multi_aRedirectedRepricedOrReplayedReleaseIsRefused() public {
        _final();
        vm.warp(block.timestamp + 121);
        address stranger = makeAddr("a submitter's own address");
        for (uint256 k = 0; k < 4; k++) {
            (FerrataVault.ReleaseRequest memory x, bytes memory s) = _releaseAt(0);
            if (k == 0) x.recipient = stranger;
            if (k == 1) x.relayer = stranger;
            // the other relay fee the vault allows (0 or the registered 2e12): allowed, but not what was proven
            if (k == 2) x.relayFee = x.relayFee == 0 ? 2e12 : 0;
            if (k == 3) x.nullifier = keccak256(abi.encode(x.nullifier));
            vm.expectRevert(VerificationFailed.selector);
            vault.release(x, s);
        }
        (FerrataVault.ReleaseRequest memory r, bytes memory releaseSeal) = _releaseAt(0);
        vault.release(r, releaseSeal);
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r, releaseSeal);
    }

    /// @notice A replayed checkpoint: once final, the same journal and seal are not taken as a candidate again (its
    ///         parent is no longer the vault's final state), and the burn root stays accepted once.
    function test_rc3Multi_theFinalisedCheckpointIsNotTakenAgain() public {
        FerrataVault.Checkpoint memory c = _final();
        uint256 count = vault.checkpointCount();
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(c, seal);
        assertEq(vault.checkpointCount(), count);
        assertEq(vault.finalState(), c.newState);
    }

    function test_gas_rc3Multi() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        console2.log(_path(), "submitCandidate", vm.lastCallGas().gasTotalUsed);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        console2.log(_path(), "finalizeAttested", vm.lastCallGas().gasTotalUsed);
        vm.warp(block.timestamp + 121);
        for (uint256 i = 0; i < 2; i++) {
            (FerrataVault.ReleaseRequest memory r, bytes memory s) = _releaseAt(i);
            vault.release(r, s);
            console2.log(_path(), "release", vm.lastCallGas().gasTotalUsed);
        }
    }
}

/// @notice Four LEZ v0.4.0-rc3 sequencers' keys throughout; three of the four attestations (n = 4, t = 3).
contract ZoneBridgeRc3MultiTest is ZoneBridgeRc3Fixture {
    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-rc3-multi.json";
    }

    function _n() internal pure override returns (uint32) {
        return 4;
    }

    function _t() internal pure override returns (uint32) {
        return 3;
    }
}
