// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";

/// @notice Loads test/fixtures/release-<case>.json, written by `host fixtures` from a real RISC Zero
///         Groth16 receipt of the release guest (artifacts/<case>/receipt-groth16.bin).
abstract contract Fixtures is Test {
    struct Release {
        uint256 chainId;
        address deployer;
        address verifier; // expected CREATE address of the Groth16 verifier (deployer nonce 0)
        address testToken; // expected CREATE address of the test ERC-20 (deployer nonce 1)
        address vault; // expected CREATE address of the vault (deployer nonce 2)
        address token; // token of this release (address(0) = ETH)
        bytes32 lezConfigId;
        bytes32 releaseImageId;
        bytes32 burnRoot;
        bytes32 nullifier;
        uint256 amount;
        address recipient;
        address feeRecipient;
        uint256 relayFee;
        bytes journal;
        bytes32 journalDigest;
        bytes seal; // 4-byte selector || 256-byte Groth16 proof
    }

    function _load(string memory name) internal view returns (Release memory f) {
        string memory json = vm.readFile(string.concat("test/fixtures/release-", name, ".json"));
        f.chainId = vm.parseJsonUint(json, ".chainId");
        f.deployer = vm.parseJsonAddress(json, ".deployer");
        f.verifier = vm.parseJsonAddress(json, ".verifier");
        f.testToken = vm.parseJsonAddress(json, ".testToken");
        f.vault = vm.parseJsonAddress(json, ".vault");
        f.token = vm.parseJsonAddress(json, ".token");
        f.lezConfigId = vm.parseJsonBytes32(json, ".lezConfigId");
        f.releaseImageId = vm.parseJsonBytes32(json, ".releaseImageId");
        f.burnRoot = vm.parseJsonBytes32(json, ".burnRoot");
        f.nullifier = vm.parseJsonBytes32(json, ".nullifier");
        f.amount = uint256(vm.parseJsonBytes32(json, ".amount"));
        f.recipient = vm.parseJsonAddress(json, ".recipient");
        f.feeRecipient = vm.parseJsonAddress(json, ".feeRecipient");
        f.relayFee = uint256(vm.parseJsonBytes32(json, ".relayFee"));
        f.journal = vm.parseJsonBytes(json, ".journal");
        f.journalDigest = vm.parseJsonBytes32(json, ".journalDigest");
        f.seal = vm.parseJsonBytes(json, ".seal");
        assertEq(sha256(f.journal), f.journalDigest, "fixture journal digest");
    }

    /// @dev EIP-2028 calldata cost: 16 gas per non-zero byte, 4 per zero byte.
    function _calldataGas(bytes memory data) internal pure returns (uint256 g) {
        for (uint256 i = 0; i < data.length; i++) {
            g += data[i] == 0 ? 4 : 16;
        }
    }
}
