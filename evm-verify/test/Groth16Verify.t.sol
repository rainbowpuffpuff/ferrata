// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console} from "forge-std/Test.sol";

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier, SelectorMismatch} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {Fixtures} from "./utils/Fixtures.sol";

/// @notice The real RiscZeroGroth16Verifier from risc0-ethereum v3.0.0, constructed with that tag's
///         ControlID (the parameters risc0-zkvm 3.0.5 proves against), verifies the release receipts
///         produced on this machine, and rejects everything else.
contract Groth16VerifyTest is Fixtures {
    RiscZeroGroth16Verifier internal verifier;
    Release internal eth;
    Release internal erc20;

    function setUp() public {
        verifier = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        eth = _load("eth");
        erc20 = _load("erc20");
    }

    function test_selector_isDerivedFromControlId_andMatchesTheSeal() public view {
        assertEq(verifier.SELECTOR(), bytes4(0x73c457ba));
        assertEq(bytes4(eth.seal), verifier.SELECTOR());
        assertEq(bytes4(erc20.seal), verifier.SELECTOR());
        assertEq(eth.seal.length, 260);
    }

    /// Gas of `verify` alone, two ways (see RESULTS.md):
    ///  - snapshotGasLastCall: gas used inside the verifier's call frame;
    ///  - gasleft() delta in this contract around the call: the frame plus the caller-side CALL costs
    ///    (cold account access, ABI encoding of the 260-byte seal into memory).
    /// Neither includes a transaction's 21,000 base cost or its calldata cost, logged separately.
    function test_gas_verifyAlone_eth() public {
        _measureVerify(eth, "verify-eth");
    }

    function test_gas_verifyAlone_erc20() public {
        _measureVerify(erc20, "verify-erc20");
    }

    function _measureVerify(Release memory f, string memory name) internal {
        uint256 before = gasleft();
        verifier.verify(f.seal, f.releaseImageId, f.journalDigest);
        uint256 delta = before - gasleft();
        uint256 frame = vm.snapshotGasLastCall(name);
        bytes memory data = abi.encodeCall(verifier.verify, (f.seal, f.releaseImageId, f.journalDigest));
        console.log(name);
        console.log("  snapshotGasLastCall (callee frame):", frame);
        console.log("  gasleft() delta (frame + CALL)     :", delta);
        console.log("  calldata bytes / EIP-2028 gas      :", data.length, _calldataGas(data));
        console.log("  as its own tx, estimate 21000+calldata+frame:", 21_000 + _calldataGas(data) + frame);
    }

    // ---- negative: everything here must revert -----------------------------------------------------------

    function test_reject_sealForADifferentJournal() public {
        vm.expectRevert(VerificationFailed.selector);
        verifier.verify(eth.seal, eth.releaseImageId, erc20.journalDigest);
        vm.expectRevert(VerificationFailed.selector);
        verifier.verify(erc20.seal, erc20.releaseImageId, eth.journalDigest);
    }

    /// Flips the last bit of each of the eight 32-byte words of the proof (a, b, c). A flipped coordinate
    /// is usually no longer a curve point; the BN254 precompiles then fail and consume all the gas they
    /// were given, so each attempt gets a 2M cap (a valid verify needs about 0.23M) and the failure mode
    /// (VerificationFailed or out of gas, i.e. empty revert data) is logged.
    function test_reject_tamperedSeal_everyProofWord() public view {
        for (uint256 w = 0; w < 8; w++) {
            bytes memory seal = bytes.concat(eth.seal);
            seal[4 + 32 * w + 31] ^= 0x01;
            (bool ok, bytes memory ret) = address(verifier).staticcall{gas: 2_000_000}(
                abi.encodeCall(verifier.verify, (seal, eth.releaseImageId, eth.journalDigest))
            );
            assertFalse(ok, string.concat("tampered word ", vm.toString(w), " accepted"));
            if (ret.length == 4) {
                assertEq(bytes4(ret), VerificationFailed.selector);
                console.log("tampered proof word", w, ": reverted VerificationFailed()");
            } else {
                assertEq(ret.length, 0);
                console.log("tampered proof word", w, ": reverted out of gas (precompile rejected the point)");
            }
        }
    }

    function test_reject_tamperedSelector() public {
        bytes memory seal = bytes.concat(eth.seal);
        seal[0] ^= 0x01;
        vm.expectRevert(abi.encodeWithSelector(SelectorMismatch.selector, bytes4(seal), bytes4(0x73c457ba)));
        verifier.verify(seal, eth.releaseImageId, eth.journalDigest);
    }

    function test_reject_rightSeal_oneJournalFieldChanged() public {
        // Each of the eleven words of the journal, one at a time.
        for (uint256 w = 0; w < 11; w++) {
            bytes memory journal = bytes.concat(eth.journal);
            journal[32 * w + 31] ^= 0x01;
            bytes32 digest = sha256(journal); // before expectRevert: sha256 is itself a (precompile) call
            vm.expectRevert(VerificationFailed.selector);
            verifier.verify(eth.seal, eth.releaseImageId, digest);
        }
    }

    function test_reject_wrongImageId() public {
        vm.expectRevert(VerificationFailed.selector);
        verifier.verify(eth.seal, keccak256("another program"), eth.journalDigest);
    }

    function test_reject_truncatedSeal() public {
        bytes memory seal = new bytes(259);
        for (uint256 i = 0; i < 259; i++) {
            seal[i] = eth.seal[i];
        }
        vm.expectRevert();
        verifier.verify(seal, eth.releaseImageId, eth.journalDigest);
    }
}
