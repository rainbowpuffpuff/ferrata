// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// The return leg over a zone on Logos Blockchain 0.3.5-rc.1 (LEZ v0.4.0-rc3): one rc3 sequencer_service and its
// 0.3.5-rc.1 node (lez/live/hooks/return-leg.sh; records in e2e/runs/zone-bridge-rc3/, results in
// lez/results/v0.4.0-rc3/return-leg/); the final LEZ programs of docs/image-ids.md; crown-driver's sessions A and B
// (a public and a private claim, a public and a private burn); one attestor beside the sequencer; the rule (B)
// checkpoint over the zone's real blocks, proven with checkpoint-core's second build (checkpoint_b_0_3_5, which reads
// the transactions' fork digest) and wrapped to Groth16; both burns' releases proven with the release program. The
// vault is v5 in mode (B), nMin 1 and tMin 1 (one sequencer), genesis = the zone's state at T0 over its one key, RISC
// Zero's real Groth16 verifier behind its emergency stop, at the address and chain id the journal names. The fixture:
// ZB_FIXTURE=zone-bridge-rc3 checkpoint/attestor/zone-bridge.sh fixture.

import {console2} from "forge-std/Test.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {RuleBMultiFixture} from "./RuleBMulti.t.sol";

contract ZoneBridgeRc3Test is RuleBMultiFixture {
    /// @dev docs/image-ids.md, "For zones on Logos Blockchain 0.3.5-rc.1; not deployed on Sepolia".
    bytes32 internal constant CHECKPOINT_B_0_3_5_IMAGE_ID =
        0xafbf402314e9498f3854f02d533e2a355d4692ca2b64b913bcb0232200c63bcf;
    /// @dev The deployed checkpoint_b (the d480efc6 layout), which the vault on Sepolia pins.
    bytes32 internal constant CHECKPOINT_B_IMAGE_ID = 0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e;
    bytes32 internal constant RELEASE_IMAGE_ID = 0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9;

    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-rc3.json";
    }

    function _releaseImageId() internal pure override returns (bytes32) {
        return RELEASE_IMAGE_ID;
    }

    function setUp() public {
        _load();
        require(imageId == CHECKPOINT_B_0_3_5_IMAGE_ID, "the fixture's checkpoint image is not checkpoint_b_0_3_5");
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 1, 1);
        _registerAndDeposit();
    }

    /// @dev ETH as DeployVault registers it and the run's two deposits (as ZoneBridge.t.sol).
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

    function test_rc3_theJournalIsOneOfOneOverTheZonesKey() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(keys.length, 1, "the vault's genesis is over the zone's one key");
        assertEq(c.parentState, vault.genesisState());
        assertEq(c.n, 1);
        assertEq(c.t, 1);
        assertEq(c.nMin, 1);
        assertEq(c.tMin, 1);
        assertEq(c.tipId, tipId);
        assertEq(c.tipHash, tipHash);
        assertEq(c.burnCount, 2, "the zone's public and private burn");
        assertEq(c.importedLeafCount, 2, "both deposits imported on the zone");
        assertEq(c.echoNonce, bytes32(0), "rule (B): no beacon");
    }

    /// @notice The checkpoint final after the hour (not before), its burn root accepted, both burns released with
    ///         their real release seals; a second release of the same burn is refused.
    function test_rc3_checkpointThenBothReleases() public {
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
            assertEq(r.recipient.balance - before, vm.parseJsonUint(json, string.concat(p, ".paidOnAnvil")));
            assertTrue(vault.nullifierUsed(r.nullifier));
            vm.expectRevert();
            vault.release(r, releaseSeal);
        }
    }

    /// @notice The same vault, at the same address with the same genesis, configuration and floors, but pinning the
    ///         deployed checkpoint_b (the d480efc6 layout, as the vault on Sepolia does): RISC Zero's verifier refuses
    ///         this checkpoint's seal, which is a proof of checkpoint_b_0_3_5. Nothing else differs, and the vault
    ///         checks the parent, the committee and the candidate before the seal, so the image id is what refuses it.
    function test_rc3_aVaultPinningTheDeployedCheckpointBRefusesThisCheckpoint() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(keys), 1, 1);
        assertEq(vault.checkpointImageId(), CHECKPOINT_B_IMAGE_ID);
        vm.expectRevert();
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, seal);
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_0_3_5_IMAGE_ID, _state(keys), 1, 1);
        vault.submitCandidate(c, seal);
    }

    function test_rc3_aFlippedSealByteIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory bad = bytes.concat(seal);
        bad[bad.length - 1] = bad[bad.length - 1] ^ 0x01;
        vm.expectRevert();
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
    }

    function test_gas_rc3() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vault.submitCandidate(c, seal);
        console2.log("rc3 return leg, 2 burns: submitCandidate", vm.lastCallGas().gasTotalUsed);
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        console2.log("rc3 return leg, 2 burns: finalizeAttested", vm.lastCallGas().gasTotalUsed);
    }
}
