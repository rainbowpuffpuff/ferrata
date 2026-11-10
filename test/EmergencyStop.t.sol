// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Ownable} from "openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";
import {Receipt as RiscZeroReceipt} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice The vault's verifier is RISC Zero's RiscZeroVerifierEmergencyStop (risc0-ethereum v3.0.0, unmodified)
///         around the base verifier, with the freeze authority as guardian. Stopping it is one-way.
contract EmergencyStopTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");

    function test_estop_guardianStopsProofDependentCalls() public {
        _deposit(alice, ETH, 1, keccak256("c"));
        _acceptRoot(ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("n"));
        _proveRelease(_releaseJournal(r));
        bytes32 laterRoot = keccak256("later root");
        _proveRoot(_rootJournal(laterRoot, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH));

        vm.prank(freezer);
        estop.estop();
        assertTrue(estop.paused());

        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r, SEAL);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.acceptBurnRoot(laterRoot, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
        assertFalse(vault.nullifierUsed(r.nullifier));

        _deposit(bob, ETH, 1, keccak256("d")); // no proof involved
    }

    function test_estop_onlyGuardianOrAProofOfExploit() public {
        vm.prank(admin);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, admin));
        estop.estop();

        vm.expectRevert(RiscZeroVerifierEmergencyStop.InvalidProofOfExploit.selector);
        estop.estop(RiscZeroReceipt({seal: SEAL, claimDigest: bytes32(uint256(1))}));

        // Anyone holding a verifying receipt for the all-zero claim digest can stop it.
        verifier.acceptClaim(bytes32(0));
        vm.prank(alice);
        estop.estop(RiscZeroReceipt({seal: SEAL, claimDigest: bytes32(0)}));
        assertTrue(estop.paused());
    }
}
