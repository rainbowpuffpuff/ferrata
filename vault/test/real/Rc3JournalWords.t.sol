// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// RFP-022 Security #2 and #3, word by word, on the real seals of the LEZ v0.4.0-rc3 runs of ZoneBridgeRc3Multi.t.sol
// (four keys) and ZoneBridgeRc3Rotation.t.sol (a rotation to five): the same vault, deployment and deposits as their
// fixture (ZoneBridgeRc3Fixture), repeated here so that this file runs only its own four tests on each seal set.
// Internal review of 27 November 2026 (docs/security/internal-review-2026-11-27.md).

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {RuleBMultiFixture} from "./RuleBMulti.t.sol";

abstract contract Rc3JournalWordsCases is RuleBMultiFixture {
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


    /// @dev sha256("ferrata/vault/release/v2"): the release journal's domain (FerrataVault.RELEASE_DOMAIN).
    bytes32 internal constant RELEASE_DOMAIN = 0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51;

    function _verifier() internal view returns (IRiscZeroVerifier) {
        return vault.verifier();
    }

    /// @dev `bytes32 w` of `b` XOR 1 in its last byte.
    function _flipWord(bytes memory b, uint256 w) internal pure returns (bytes memory out) {
        out = bytes.concat(b);
        out[32 * w + 31] = out[32 * w + 31] ^ 0x01;
    }

    /// @dev The release journal the vault rebuilds for `r` (FerrataVault._releaseJournalDigest's preimage).
    function _releaseJournal(FerrataVault.ReleaseRequest memory r) internal view returns (bytes memory) {
        uint256 amount = 0.001 ether * 10 ** r.denomIndex;
        return abi.encode(
            RELEASE_DOMAIN,
            block.chainid,
            address(vault),
            vault.lezConfigId(),
            r.burnRoot,
            r.nullifier,
            r.token,
            amount,
            r.recipient,
            r.relayer,
            r.relayFee
        );
    }

    // ------------------------------------------------------------------------------------------------------------
    // Every journal word, at the verifier the vault pins
    // ------------------------------------------------------------------------------------------------------------

    /// @notice The checkpoint journal's 17 words (domain, chain id, vault, configuration, the floors, parent and new
    ///         state, burn root and count, imported leaf count, sunset, echo nonce, tip id and hash, n, t): the real
    ///         seal verifies for the journal as proven and for no journal with one word changed.
    function test_rc3Words_everyCheckpointJournalWordIsBoundByTheRealSeal() public view {
        assertEq(journal.length, 17 * 32, "17 words");
        _verifier().verify(seal, imageId, sha256(journal)); // the control: no revert
        for (uint256 w = 0; w < 17; w++) {
            try _verifier().verify(seal, imageId, sha256(_flipWord(journal, w))) {
                revert(string.concat("checkpoint journal word ", vm.toString(w), " is not bound"));
            } catch (bytes memory reason) {
                // casting to 'bytes4' is safe because a custom error's first four bytes are its selector
                // forge-lint: disable-next-line(unsafe-typecast)
                assertEq(bytes4(reason), VerificationFailed.selector, "refused by the verifier's pairing check");
            }
        }
    }

    /// @notice Each release journal's 11 words (domain, chain id, vault, configuration, burn root, nullifier, token,
    ///         amount, recipient, relayer, relay fee), for both of the run's releases: the real seal verifies for the
    ///         journal the vault rebuilds and for none with one word changed. The vault, the configuration, the
    ///         chain and the burn root are words like the others: a seal for this vault pays at no other.
    function test_rc3Words_everyReleaseJournalWordIsBoundByTheRealSeal() public view {
        for (uint256 i = 0; i < 2; i++) {
            (FerrataVault.ReleaseRequest memory r, bytes memory s) = _releaseAt(i);
            bytes memory j = _releaseJournal(r);
            assertEq(j.length, 11 * 32, "11 words");
            _verifier().verify(s, RELEASE_IMAGE_ID, sha256(j)); // the control: the vault's own journal verifies
            for (uint256 w = 0; w < 11; w++) {
                try _verifier().verify(s, RELEASE_IMAGE_ID, sha256(_flipWord(j, w))) {
                    revert(string.concat("release journal word ", vm.toString(w), " is not bound"));
                } catch (bytes memory reason) {
                    // casting to 'bytes4' is safe because a custom error's first four bytes are its selector
                    // forge-lint: disable-next-line(unsafe-typecast)
                    assertEq(bytes4(reason), VerificationFailed.selector, "refused by the verifier's pairing check");
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // Every caller-supplied checkpoint field, through the vault
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Through `submitCandidate`: the fields the vault checks itself are refused by those checks (parent,
    ///         floors, a key set below them, a nonzero echo nonce under rule (B)); every other field changed alone
    ///         (new state, burn root and count, imported count, sunset, tip id and hash, n and t above the floors)
    ///         is refused by the verifier. The unaltered checkpoint is then taken.
    function test_rc3Words_everyCheckpointFieldChangedAloneIsRefused() public {
        bytes4[13] memory want = [
            VerificationFailed.selector, // 0 newState
            VerificationFailed.selector, // 1 burnRoot
            VerificationFailed.selector, // 2 burnCount
            VerificationFailed.selector, // 3 importedLeafCount
            VerificationFailed.selector, // 4 sunset
            VerificationFailed.selector, // 5 tipId
            VerificationFailed.selector, // 6 tipHash (the LEZ header the checkpoint was cut at)
            VerificationFailed.selector, // 7 n above the floor
            VerificationFailed.selector, // 8 t above the floor
            FerrataVault.ParentNotFinal.selector, // 9 parentState
            FerrataVault.WrongCommitteeFloor.selector, // 10 nMin
            FerrataVault.KeySetBelowFloor.selector, // 11 t below the floor
            FerrataVault.EchoNonceNotZero.selector // 12 echoNonce under rule (B)
        ];
        for (uint256 k = 0; k < want.length; k++) {
            FerrataVault.Checkpoint memory x = _checkpoint();
            if (k == 0) x.newState = keccak256(abi.encode(x.newState));
            if (k == 1) x.burnRoot = keccak256(abi.encode(x.burnRoot));
            if (k == 2) x.burnCount += 1;
            if (k == 3) x.importedLeafCount -= 1;
            if (k == 4) x.sunset = !x.sunset;
            if (k == 5) x.tipId += 1;
            if (k == 6) x.tipHash = keccak256(abi.encode(x.tipHash));
            if (k == 7) x.n += 1;
            if (k == 8) x.t += 1;
            if (k == 9) x.parentState = keccak256(abi.encode(x.parentState));
            if (k == 10) x.nMin += 1;
            if (k == 11) x.t = 2;
            if (k == 12) x.echoNonce = bytes32(uint256(1));
            vm.expectRevert(want[k]);
            vault.submitCandidate{gas: TAMPERED_CALL_GAS}(x, seal);
        }
        vault.submitCandidate(_checkpoint(), seal);
        (,,,, uint48 submittedAt,,,) = vault.candidates(_checkpoint().newState);
        assertEq(submittedAt, block.timestamp, "the unaltered checkpoint is taken");
    }

    /// @notice Another chain: the real checkpoint is refused as a candidate, and, the checkpoint final on its own chain,
    ///         the real release is refused there too (the chain id is read at call time, never cached).
    function test_rc3Words_onAnotherChainTheRealCheckpointAndReleaseAreRefused() public {
        uint256 chain = block.chainid;
        vm.chainId(chain + 1);
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, seal);
        vm.chainId(chain);
        _final();
        vm.warp(block.timestamp + 121);
        (FerrataVault.ReleaseRequest memory r, bytes memory s) = _releaseAt(0);
        vm.chainId(chain + 1);
        vm.expectRevert(VerificationFailed.selector);
        vault.release{gas: TAMPERED_CALL_GAS}(r, s);
        vm.chainId(chain);
        vault.release(r, s);
        assertTrue(vault.nullifierUsed(r.nullifier), "paid on its own chain");
    }
}

/// @notice The four-key run (n = 4, t = 3).
contract Rc3JournalWordsMultiTest is Rc3JournalWordsCases {
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

/// @notice The rotation run (five keys after the rotation, n = 5, t = 4).
contract Rc3JournalWordsRotationTest is Rc3JournalWordsCases {
    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-rc3-rotation.json";
    }

    function _n() internal pure override returns (uint32) {
        return 5;
    }

    function _t() internal pure override returns (uint32) {
        return 4;
    }
}
