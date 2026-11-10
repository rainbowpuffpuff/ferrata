// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console} from "forge-std/Test.sol";
import {Ownable} from "openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";

import {Receipt as RiscZeroReceipt, VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier, SelectorMismatch} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FixturesV2} from "./utils/FixturesV2.sol";

/// @notice SPEC-v2 §6: the real RiscZeroGroth16Verifier (risc0-ethereum v3.0.0 ControlID) behind RISC Zero's
///         RiscZeroVerifierEmergencyStop (unmodified), guarded by the freeze authority. It verifies the v2
///         release receipts made on this machine, rejects everything else, and once stopped accepts nothing.
contract Groth16VerifyV2Test is FixturesV2 {
    RiscZeroGroth16Verifier internal groth16;
    RiscZeroVerifierEmergencyStop internal estop;
    address internal guardian = makeAddr("freeze authority (estop guardian)");
    Release internal eth;
    Release internal erc20;
    Release internal ethpull;

    function setUp() public {
        groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        estop = new RiscZeroVerifierEmergencyStop(groth16, guardian);
        eth = _load("eth");
        erc20 = _load("erc20");
        ethpull = _load("ethpull");
    }

    function test_selector_isDerivedFromControlId_andMatchesEverySeal() public view {
        assertEq(groth16.SELECTOR(), bytes4(0x73c457ba));
        assertEq(bytes4(eth.seal), groth16.SELECTOR());
        assertEq(bytes4(erc20.seal), groth16.SELECTOR());
        assertEq(bytes4(ethpull.seal), groth16.SELECTOR());
        assertEq(eth.seal.length, 260);
    }

    /// Gas of `verify` alone, through the stop and directly (see RESULTS.md):
    ///  - snapshotGasLastCall: gas used inside the called contract's frame;
    ///  - gasleft() delta: that frame plus the caller-side CALL cost (cold account, ABI-encoding the seal).
    function test_gas_verifyAlone() public {
        for (uint256 k = 0; k < 3; k++) {
            Release memory f = k == 0 ? eth : (k == 1 ? erc20 : ethpull);
            uint256 before = gasleft();
            estop.verify(f.seal, f.releaseImageId, f.journalDigest);
            uint256 delta = before - gasleft();
            uint256 frame = vm.snapshotGasLastCall(string.concat("verify-via-estop-", vm.toString(k)));
            bytes memory data = abi.encodeCall(estop.verify, (f.seal, f.releaseImageId, f.journalDigest));
            console.log("verify via EmergencyStop, case", k);
            console.log("  snapshotGasLastCall (estop frame, incl. Groth16):", frame);
            console.log("  gasleft() delta (frame + CALL)                  :", delta);
            console.log("  as its own tx, estimate 21000 + calldata + frame:", 21_000 + _calldataGas(data) + frame);
        }
    }

    function test_gas_verifyAlone_rawGroth16() public {
        Release memory f = eth; // copy out of storage before measuring
        uint256 before = gasleft();
        groth16.verify(f.seal, f.releaseImageId, f.journalDigest);
        uint256 delta = before - gasleft();
        uint256 frame = vm.snapshotGasLastCall("verify-raw-groth16");
        console.log("verify on RiscZeroGroth16Verifier directly");
        console.log("  snapshotGasLastCall:", frame);
        console.log("  gasleft() delta    :", delta);
    }

    // ---- negative ---------------------------------------------------------------------------------------------

    function test_reject_sealForADifferentJournal() public {
        vm.expectRevert(VerificationFailed.selector);
        estop.verify(eth.seal, eth.releaseImageId, erc20.journalDigest);
        vm.expectRevert(VerificationFailed.selector);
        estop.verify(erc20.seal, erc20.releaseImageId, ethpull.journalDigest);
    }

    /// Each of the eight proof words with its last bit flipped. An invalid curve point makes the BN254
    /// precompiles consume all the gas they get, so each attempt runs under a 2M cap.
    function test_reject_tamperedSeal_everyProofWord() public view {
        for (uint256 w = 0; w < 8; w++) {
            bytes memory seal = bytes.concat(eth.seal);
            seal[4 + 32 * w + 31] ^= 0x01;
            (bool ok, bytes memory ret) = address(estop).staticcall{gas: 2_000_000}(
                abi.encodeCall(estop.verify, (seal, eth.releaseImageId, eth.journalDigest))
            );
            assertFalse(ok, string.concat("tampered word ", vm.toString(w), " accepted"));
            assertTrue(ret.length == 0 || bytes4(ret) == VerificationFailed.selector);
        }
    }

    function test_reject_tamperedSelector() public {
        bytes memory seal = bytes.concat(eth.seal);
        seal[0] ^= 0x01;
        vm.expectRevert(abi.encodeWithSelector(SelectorMismatch.selector, bytes4(seal), bytes4(0x73c457ba)));
        estop.verify(seal, eth.releaseImageId, eth.journalDigest);
    }

    function test_reject_rightSeal_oneJournalWordChanged() public {
        for (uint256 w = 0; w < 11; w++) {
            bytes memory journal = bytes.concat(eth.journal);
            journal[32 * w + 31] ^= 0x01;
            bytes32 digest = sha256(journal);
            vm.expectRevert(VerificationFailed.selector);
            estop.verify(eth.seal, eth.releaseImageId, digest);
        }
    }

    function test_reject_wrongImageId() public {
        vm.expectRevert(VerificationFailed.selector);
        estop.verify(eth.seal, keccak256("another program"), eth.journalDigest);
    }

    // ---- the emergency stop (spec §6) -------------------------------------------------------------------------

    function test_stop_byGuardian_blocksEveryProof_forever() public {
        estop.verify(eth.seal, eth.releaseImageId, eth.journalDigest); // accepted before
        vm.prank(guardian);
        estop.estop();
        assertTrue(estop.paused());
        vm.expectRevert(Pausable.EnforcedPause.selector);
        estop.verify(eth.seal, eth.releaseImageId, eth.journalDigest);
        // There is no unpause function; the contract's whole ABI is estop, estop(receipt), verify,
        // verifyIntegrity, the ownership functions and `paused`. Transferring ownership does not unpause.
        vm.prank(guardian);
        estop.transferOwnership(makeAddr("new guardian"));
        vm.expectRevert(Pausable.EnforcedPause.selector);
        estop.verify(erc20.seal, erc20.releaseImageId, erc20.journalDigest);
    }

    function test_stop_onlyGuardian_orAProofOfExploit() public {
        vm.prank(makeAddr("someone"));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, makeAddr("someone")));
        estop.estop();
        // The circuit breaker needs a valid proof for the all-zero claim digest; a real seal for another claim
        // does not verify against it, and a non-zero claim digest is refused outright.
        vm.expectRevert(VerificationFailed.selector);
        estop.estop(RiscZeroReceipt({seal: eth.seal, claimDigest: bytes32(0)}));
        vm.expectRevert(RiscZeroVerifierEmergencyStop.InvalidProofOfExploit.selector);
        estop.estop(RiscZeroReceipt({seal: eth.seal, claimDigest: keccak256("not zero")}));
        assertFalse(estop.paused());
    }
}
