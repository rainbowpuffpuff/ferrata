// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../../src/FerrataVault.sol";
import {VaultHalmos} from "./VaultHalmos.t.sol";

/// Concrete replays under `forge test` of what Halmos reported (docs/reviews/security/tools-2026-11-11.md §3).
/// Kept outside the suite so its count stays as docs/EVIDENCE.md row 12 gives it; to run:
///   cp docs/reviews/security/replay/HalmosReplay.t.sol vault/test/halmos/ && cd vault &&
///   forge test --match-contract HalmosReplay -vv
/// test_replay_*: the counterexamples to check_after_stop_only_owed_leaves under the generic storage layout (an
/// admin's registerToken after the stop): the call succeeds and no balance, escrow, owed amount or checkpoint changes.
/// test_release_pays_concretely: the release check_reach_release asserts impossible does pay (expected to fail with
/// "assertion failed"), as it must for check_release_needs_proof_root_and_fresh_nullifier not to be vacuous.
contract HalmosReplay is VaultHalmos {
    function _replay(uint8 decimals, uint256 refill) internal {
        vm.prank(freezer);
        estop.estop();
        Books memory s0 = _books();
        uint256 count0 = vault.checkpointCount();
        uint256[] memory d = new uint256[](1);
        d[0] = 0x64000000000000000000000000000000;
        uint256[] memory z = new uint256[](1);
        FerrataVault.TokenParams memory p = FerrataVault.TokenParams({
            denominations: d,
            relayFees: z,
            lezUnit: 0x64,
            unitWeight: 0x40,
            depositCap: FerrataVault.CapParams(0, 0),
            releaseCap: FerrataVault.CapParams(1 << 255, refill),
            releaseDelay: 0,
            lezDefinition: bytes32(0),
            decimals: decimals,
            checkpointFees: z
        });
        vm.prank(admin);
        vault.registerToken(address(verifier), p);
        Books memory s1 = _books();
        assertEq(s1.ethBalance, s0.ethBalance);
        assertEq(s1.usdcBalance, s0.usdcBalance);
        assertEq(s1.ethEscrow, s0.ethEscrow);
        assertEq(s1.usdcEscrow, s0.usdcEscrow);
        assertEq(s1.ethOwed, s0.ethOwed);
        assertEq(s1.usdcOwed, s0.usdcOwed);
        assertEq(vault.checkpointCount(), count0);
        (bool registered,,,,) = vault.tokenConfig(address(verifier));
        assertTrue(registered);
    }

    function test_replay_decimals_32() public {
        _replay(0x20, 1 << 255);
    }

    function test_replay_decimals_128() public {
        _replay(0x80, 1 << 249);
    }

    function test_release_pays_concretely() public {
        check_reach_release(false, 0, address(0xBEEF), keccak256("n"));
    }
}
