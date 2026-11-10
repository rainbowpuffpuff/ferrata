// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";

/// @notice SPEC-v2 journals: the Rust encodings (ferrata-bridge-core `ReleaseJournal::encode` and
///         `CheckpointJournal::encode`, written to vectors/release-journal-v2.json and
///         vectors/checkpoint-v2.json) equal Solidity's `abi.encode` of the same fields, byte for byte.
///         The comparison with the v2 vault's own digest functions is in VaultV2.t.sol.
contract JournalEqualityV2Test is Test {
    function _key(string memory list, uint256 i, string memory field) internal pure returns (string memory) {
        return string.concat(".", list, "[", vm.toString(i), "].", field);
    }

    function test_releaseJournal_rustEqualsAbiEncode() public view {
        string memory json = vm.readFile("../vectors/release-journal-v2.json");
        uint256 n = vm.parseJsonUint(json, ".count");
        assertEq(n, 6);
        for (uint256 i = 0; i < n; i++) {
            bytes memory solidity = _releaseAbi(json, i);
            assertEq(solidity.length, 352);
            assertEq(solidity, vm.parseJsonBytes(json, _key("cases", i, "journal")), "release journal bytes");
            assertEq(sha256(solidity), vm.parseJsonBytes32(json, _key("cases", i, "digest")), "release journal digest");
        }
    }

    /// abi.encode(RELEASE_DOMAIN, evmChainId, vault, lezConfigId, burnRoot, NOut, token, amount, recipient,
    /// relayer, relayFee), in two halves (eleven arguments overflow the legacy code generator's stack).
    function _releaseAbi(string memory json, uint256 i) internal pure returns (bytes memory) {
        bytes memory head = abi.encode(
            sha256("ferrata/vault/release/v2"),
            uint256(vm.parseJsonBytes32(json, _key("cases", i, "chainId"))),
            vm.parseJsonAddress(json, _key("cases", i, "vault")),
            vm.parseJsonBytes32(json, _key("cases", i, "lezConfigId")),
            vm.parseJsonBytes32(json, _key("cases", i, "burnRoot")),
            vm.parseJsonBytes32(json, _key("cases", i, "nullifier"))
        );
        bytes memory tail = abi.encode(
            vm.parseJsonAddress(json, _key("cases", i, "token")),
            uint256(vm.parseJsonBytes32(json, _key("cases", i, "amount"))),
            vm.parseJsonAddress(json, _key("cases", i, "recipient")),
            vm.parseJsonAddress(json, _key("cases", i, "relayer")),
            uint256(vm.parseJsonBytes32(json, _key("cases", i, "relayFee")))
        );
        return bytes.concat(head, tail);
    }

    function test_checkpointJournal_rustEqualsAbiEncode() public view {
        string memory json = vm.readFile("../vectors/checkpoint-v2.json");
        uint256 n = vm.parseJsonUint(json, ".count");
        assertEq(n, 3);
        for (uint256 i = 0; i < n; i++) {
            bytes memory solidity = _checkpointAbi(json, i);
            assertEq(solidity.length, 416);
            assertEq(solidity, vm.parseJsonBytes(json, _key("journals", i, "journal")), "checkpoint journal bytes");
            assertEq(sha256(solidity), vm.parseJsonBytes32(json, _key("journals", i, "digest")), "checkpoint journal digest");
        }
    }

    /// abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot,
    /// burnCount, importedLeafCount, sunset, echoNonce), with the types SPEC-v2 implies.
    function _checkpointAbi(string memory json, uint256 i) internal pure returns (bytes memory) {
        bytes memory head = abi.encode(
            sha256("ferrata/vault/checkpoint/v2"),
            uint256(vm.parseJsonBytes32(json, _key("journals", i, "chainId"))),
            vm.parseJsonAddress(json, _key("journals", i, "vault")),
            vm.parseJsonBytes32(json, _key("journals", i, "lezConfigId")),
            uint32(vm.parseJsonUint(json, _key("journals", i, "nMin"))),
            uint32(vm.parseJsonUint(json, _key("journals", i, "tMin")))
        );
        bytes memory tail = abi.encode(
            vm.parseJsonBytes32(json, _key("journals", i, "parentState")),
            vm.parseJsonBytes32(json, _key("journals", i, "newState")),
            vm.parseJsonBytes32(json, _key("journals", i, "burnRoot")),
            uint64(vm.parseUint(vm.parseJsonString(json, _key("journals", i, "burnCount")))),
            uint64(vm.parseUint(vm.parseJsonString(json, _key("journals", i, "importedLeafCount")))),
            vm.parseJsonBool(json, _key("journals", i, "sunset")),
            vm.parseJsonBytes32(json, _key("journals", i, "echoNonce"))
        );
        return bytes.concat(head, tail);
    }
}
