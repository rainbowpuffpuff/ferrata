// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Vm} from "forge-std/Vm.sol";
import {console2} from "forge-std/console2.sol";
import {BrokenVerifierTest} from "./BrokenVerifier.t.sol";

/// @notice RFP-022 P.7, the Ethereum side: the storage slots each bridge operation adds to the vault, measured with
///         Foundry's state-diff recording (every SSTORE of the call, its slot's value before the call and after it).
///         A slot is "created" when it goes from zero to non-zero and "cleared" when it goes back to zero. The releases
///         and checkpoints use BrokenVerifierTest's accept-anything verifier (the storage they write is the same as with
///         a real proof). Run: forge test --match-contract StateGrowthTest --match-test test_stateGrowth -vv
///         (bench/perf/vault-growth.sh writes the output to lez/results/perf-state-ethereum.txt).
contract StateGrowthTest is BrokenVerifierTest {
    struct Count {
        uint256 created; // in the vault
        uint256 cleared;
        uint256 rewritten;
        uint256 createdElsewhere; // in the token (its balances)
    }

    function _count(Vm.AccountAccess[] memory acc) internal view returns (Count memory c) {
        uint256 n;
        for (uint256 i = 0; i < acc.length; i++) {
            n += acc[i].storageAccesses.length;
        }
        address[] memory accts = new address[](n);
        bytes32[] memory slots = new bytes32[](n);
        bytes32[] memory before = new bytes32[](n);
        bytes32[] memory last = new bytes32[](n);
        uint256 m;
        for (uint256 i = 0; i < acc.length; i++) {
            if (acc[i].reverted) continue;
            for (uint256 j = 0; j < acc[i].storageAccesses.length; j++) {
                Vm.StorageAccess memory s = acc[i].storageAccesses[j];
                if (!s.isWrite || s.reverted) continue;
                uint256 k = 0;
                while (k < m && !(accts[k] == s.account && slots[k] == s.slot)) k++;
                if (k == m) {
                    accts[m] = s.account;
                    slots[m] = s.slot;
                    before[m] = s.previousValue;
                    m++;
                }
                last[k] = s.newValue;
            }
        }
        for (uint256 k = 0; k < m; k++) {
            bool created = before[k] == bytes32(0) && last[k] != bytes32(0);
            if (accts[k] != address(vault)) {
                // The token's own balances; the test contract's bookkeeping is not counted.
                if (created && accts[k] == address(dai)) c.createdElsewhere++;
                continue;
            }
            if (created) c.created++;
            else if (before[k] != bytes32(0) && last[k] == bytes32(0)) c.cleared++;
            else if (before[k] != last[k]) c.rewritten++;
        }
    }

    function _log(string memory what, uint256 n, Count memory c) internal pure {
        console2.log(string.concat("GROWTH\t", what, "\tops / vault slots created / cleared / rewritten / created in the token"));
        console2.log(n, c.created, c.cleared);
        console2.log(c.rewritten, c.createdElsewhere);
    }

    function test_stateGrowth() public {
        address carol = makeAddr("carol");
        address dave = makeAddr("dave");
        // Deposits: the first and a later one by one address, then the first of another.
        vm.startStateDiffRecording();
        _depositN(carol, address(dai), 0, 1);
        _log("deposit, the address's first", 1, _count(vm.stopAndReturnStateDiff()));
        vm.startStateDiffRecording();
        _depositN(carol, address(dai), 0, 1);
        _log("deposit, a later one by the same address", 1, _count(vm.stopAndReturnStateDiff()));
        vm.startStateDiffRecording();
        _depositN(dave, address(dai), 0, 1);
        _log("deposit, another address's first", 1, _count(vm.stopAndReturnStateDiff()));
        // 100 deposits by 25 new addresses, then 100 more by the same addresses.
        address[] memory who = new address[](25);
        for (uint256 i = 0; i < 25; i++) {
            who[i] = makeAddr(string.concat("depositor ", vm.toString(i)));
        }
        vm.startStateDiffRecording();
        for (uint256 i = 0; i < 25; i++) {
            _depositN(who[i], address(dai), 0, 1);
        }
        _log("deposits by 25 new addresses (total)", 25, _count(vm.stopAndReturnStateDiff()));
        vm.startStateDiffRecording();
        for (uint256 i = 0; i < 25; i++) {
            _depositN(who[i], address(dai), 0, 1);
        }
        _log("deposits by the same 25 addresses again (total)", 25, _count(vm.stopAndReturnStateDiff()));
        // A checkpoint (candidate and finalisation), then releases under it.
        vm.startStateDiffRecording();
        uint256 finalizedAt = _forge(ROOT, false, 0);
        _log("checkpoint: submitCandidate + finalisation", 1, _count(vm.stopAndReturnStateDiff()));
        vm.warp(finalizedAt + MIN_DELAY);
        vm.startStateDiffRecording();
        _release(address(dai), 0, ROOT);
        _log("release, the first", 1, _count(vm.stopAndReturnStateDiff()));
        vm.startStateDiffRecording();
        for (uint256 i = 0; i < 25; i++) {
            _release(address(dai), 0, ROOT);
        }
        _log("releases, 25 more (total)", 25, _count(vm.stopAndReturnStateDiff()));
        vm.startStateDiffRecording();
        _forge(keccak256("a second burn root"), false, 0);
        _log("checkpoint, a later one", 1, _count(vm.stopAndReturnStateDiff()));
    }
}
