// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Vm} from "forge-std/Vm.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Ethereum-side checks for Privacy Preservation #3 and #5, and Functionality #2.
///         The vault never receives LEZ-side data, so the first two tests pass by construction. They exist to fail
///         if a later change adds an argument, event field or storage write that carries such data: every storage
///         slot a deposit or release writes is listed explicitly, and the one event each emits is checked field by
///         field. Mapping slots are computed from the layout in README.md (`forge inspect FerrataVault storageLayout`).
contract PrivacyTest is VaultTestBase {
    // LEZ-side values of one user. A real client derives them from the wallet seed; here they are constants.
    bytes32 internal constant LEZ_DESTINATION = keccak256("LEZ destination account");
    bytes32 internal constant BLINDING = keccak256("blinding for (depositor, index), derived from the seed");
    bytes32 internal constant LEZ_BURN_ACCOUNT = keccak256("LEZ account the burn came from");
    bytes32 internal constant BURN_SECRET = keccak256("burn secret, derived from the seed");

    uint256 internal constant ESCROWED_SLOT = 44;
    uint256 internal constant DEPOSIT_COUNT_SLOT = 45;
    uint256 internal constant NULLIFIER_USED_SLOT = 47;

    function _contains(bytes memory haystack, bytes32 needle) internal pure returns (bool) {
        if (haystack.length < 32) return false;
        for (uint256 offset = 0; offset + 32 <= haystack.length; offset++) {
            bytes32 window;
            for (uint256 k = 0; k < 32; k++) {
                window |= bytes32(haystack[offset + k]) >> (8 * k);
            }
            if (window == needle) return true;
        }
        return false;
    }

    function _assertAbsent(
        bytes32 secret,
        bytes memory callData,
        Vm.Log[] memory logs,
        Vm.AccountAccess[] memory accesses
    ) internal pure {
        assertFalse(_contains(callData, secret), "calldata");
        for (uint256 i = 0; i < logs.length; i++) {
            for (uint256 t = 0; t < logs[i].topics.length; t++) {
                assertTrue(logs[i].topics[t] != secret, "event topic");
            }
            assertFalse(_contains(logs[i].data, secret), "event data");
        }
        for (uint256 i = 0; i < accesses.length; i++) {
            for (uint256 s = 0; s < accesses[i].storageAccesses.length; s++) {
                Vm.StorageAccess memory a = accesses[i].storageAccesses[s];
                if (a.isWrite) assertTrue(a.newValue != secret, "storage write");
            }
        }
    }

    /// @dev Distinct storage slots of `target` written in `accesses`, in first-write order.
    function _writtenSlots(Vm.AccountAccess[] memory accesses, address target)
        internal
        pure
        returns (bytes32[] memory)
    {
        bytes32[] memory slots = new bytes32[](64);
        uint256 n;
        for (uint256 i = 0; i < accesses.length; i++) {
            for (uint256 s = 0; s < accesses[i].storageAccesses.length; s++) {
                Vm.StorageAccess memory a = accesses[i].storageAccesses[s];
                if (!a.isWrite || a.account != target) continue;
                bool seen;
                for (uint256 k = 0; k < n; k++) {
                    if (slots[k] == a.slot) seen = true;
                }
                if (!seen) slots[n++] = a.slot;
            }
        }
        bytes32[] memory out = new bytes32[](n);
        for (uint256 k = 0; k < n; k++) {
            out[k] = slots[k];
        }
        return out;
    }

    function _assertSameSet(bytes32[] memory actual, bytes32[] memory expected) internal pure {
        assertEq(actual.length, expected.length, "number of written slots");
        for (uint256 i = 0; i < expected.length; i++) {
            bool found;
            for (uint256 k = 0; k < actual.length; k++) {
                if (actual[k] == expected[i]) found = true;
            }
            assertTrue(found, "expected slot not written");
        }
    }

    function _mappingSlot(bytes32 key, uint256 baseSlot) internal pure returns (bytes32) {
        return keccak256(abi.encode(key, baseSlot));
    }

    function test_privacy_depositRevealsNothingBeyondTheCommitment() public {
        bytes32 commitment = sha256(bytes.concat("example commitment scheme", LEZ_DESTINATION, BLINDING));
        bytes memory callData = abi.encodeCall(FerrataVault.depositETH, (1, commitment, 0));
        vm.deal(alice, 1 ether);

        vm.recordLogs();
        vm.startStateDiffRecording();
        vm.prank(alice);
        (bool ok,) = address(vault).call{value: 1 ether}(callData);
        Vm.AccountAccess[] memory accesses = vm.stopAndReturnStateDiff();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertTrue(ok);

        _assertAbsent(LEZ_DESTINATION, callData, logs, accesses);
        _assertAbsent(BLINDING, callData, logs, accesses);

        // One event, carrying exactly: depositor, token (topics); index, leafIndex, amount, commitment (data).
        assertEq(logs.length, 1);
        assertEq(logs[0].topics.length, 3);
        assertEq(logs[0].topics[0], FerrataVault.Deposit.selector);
        assertEq(logs[0].topics[1], bytes32(uint256(uint160(alice))));
        assertEq(logs[0].topics[2], bytes32(0)); // ETH
        assertEq(logs[0].data, abi.encode(uint64(0), uint64(0), uint256(1 ether), commitment));

        // Storage writes (all caps UNLIMITED here): root, leaf count, one branch node, the depositor's counter, and
        // the escrow total. The branch node for leaf 0 is the leaf itself.
        bytes32[] memory expected = new bytes32[](5);
        expected[0] = bytes32(uint256(0));
        expected[1] = bytes32(uint256(1));
        expected[2] = bytes32(uint256(3)); // _branch[0]
        expected[3] = _mappingSlot(bytes32(uint256(uint160(alice))), DEPOSIT_COUNT_SLOT);
        expected[4] = _mappingSlot(bytes32(0), ESCROWED_SLOT);
        _assertSameSet(_writtenSlots(accesses, address(vault)), expected);
    }

    function test_privacy_releaseRevealsNoLezAccount() public {
        _deposit(bob, ETH, 1, keccak256("someone's commitment"));
        bytes32 burnRoot = keccak256("burn root");
        _acceptRoot(burnRoot);

        // The nullifier is a one-way function of the burn secret (the real derivation is the LEZ side's).
        bytes32 nullifier = sha256(bytes.concat("example nullifier scheme", BURN_SECRET));
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, burnRoot, nullifier);
        _proveRelease(_releaseJournal(r));
        bytes memory callData = abi.encodeCall(FerrataVault.release, (r, SEAL));

        vm.recordLogs();
        vm.startStateDiffRecording();
        vm.prank(relayer);
        (bool ok,) = address(vault).call(callData);
        Vm.AccountAccess[] memory accesses = vm.stopAndReturnStateDiff();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertTrue(ok);

        _assertAbsent(LEZ_BURN_ACCOUNT, callData, logs, accesses);
        _assertAbsent(BURN_SECRET, callData, logs, accesses);

        assertEq(logs.length, 1);
        assertEq(logs[0].topics[0], FerrataVault.Release.selector);
        assertEq(logs[0].data, abi.encode(uint256(1 ether), relayer, uint256(0), burnRoot));

        bytes32[] memory expected = new bytes32[](2);
        expected[0] = _mappingSlot(nullifier, NULLIFIER_USED_SLOT);
        expected[1] = _mappingSlot(bytes32(0), ESCROWED_SLOT);
        _assertSameSet(_writtenSlots(accesses, address(vault)), expected);
    }

    /// @notice Privacy #5: failures carry no data beyond a 4-byte selector, so a rejected claim, a repeated claim and
    ///         a cap rejection cannot echo anything that identifies which deposit or burn was involved.
    function test_privacy_failuresCarryOnlyASelector() public {
        _deposit(alice, ETH, 1, keccak256("c"));
        bytes32 root = keccak256("burn root");
        _acceptRoot(root);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, root, keccak256("n"));
        _release(r);

        bytes[] memory calls = new bytes[](5);
        calls[0] = abi.encodeCall(FerrataVault.release, (r, SEAL)); // repeated claim
        FerrataVault.ReleaseRequest memory unknownRoot = _request(ETH, 1, recipient, keccak256("x"), keccak256("m"));
        calls[1] = abi.encodeCall(FerrataVault.release, (unknownRoot, SEAL)); // rejected claim
        FerrataVault.ReleaseRequest memory badFee = _request(ETH, 1, recipient, root, keccak256("m"));
        badFee.relayFee = 1;
        calls[2] = abi.encodeCall(FerrataVault.release, (badFee, SEAL));
        calls[3] = abi.encodeCall(FerrataVault.depositETH, (1, keccak256("c"), 7)); // wrong index
        calls[4] = abi.encodeCall(FerrataVault.depositETH, (1, bytes32(0), 1)); // zero commitment

        for (uint256 i = 0; i < calls.length; i++) {
            (bool ok, bytes memory ret) = address(vault).call(calls[i]);
            assertFalse(ok);
            assertEq(ret.length, 4);
        }

        vm.prank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 0, 0);
        FerrataVault.ReleaseRequest memory capped = _request(ETH, 1, recipient, root, keccak256("m"));
        _proveRelease(_releaseJournal(capped));
        _deposit(alice, ETH, 1, keccak256("c2"));
        (bool ok2, bytes memory ret2) = address(vault).call(abi.encodeCall(FerrataVault.release, (capped, SEAL)));
        assertFalse(ok2);
        assertEq(ret2, abi.encodeWithSelector(FerrataVault.TokenReleaseCapExceeded.selector));
    }
}
