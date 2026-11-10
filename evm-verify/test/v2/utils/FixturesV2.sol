// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";

/// @notice Loads test/fixtures/v2/*.json, written by `host fixtures` from real RISC Zero Groth16 receipts of the
///         v2 release guest (artifacts/<case>/receipt-groth16.bin) and from the v2 scenario's checkpoint states.
abstract contract FixturesV2 is Test {
    struct Release {
        uint256 chainId;
        address deployer;
        address groth16Verifier; // expected CREATE address, deployer nonce 0
        address emergencyStop; // nonce 1
        address testToken; // nonce 2
        address vault; // nonce 3
        bytes32 lezConfigId;
        bytes32 releaseImageId;
        bytes32 burnRoot;
        bytes32 nullifier;
        address token;
        uint256 amount;
        address recipient;
        address relayer;
        uint256 relayFee;
        bytes journal;
        bytes32 journalDigest;
        bytes seal; // 4-byte selector || 256-byte Groth16 proof
    }

    struct CheckpointFixture {
        uint256 chainId;
        address vault;
        bytes32 lezConfigId;
        uint32 nMin;
        uint32 tMin;
        bytes32 genesisState;
        bytes32 newState;
        bytes32 burnRoot;
        uint64 burnCount;
        uint64 importedLeafCount;
        bool sunset;
    }

    function _load(string memory name) internal view returns (Release memory f) {
        string memory json = vm.readFile(string.concat("test/fixtures/v2/release-", name, ".json"));
        f.chainId = vm.parseJsonUint(json, ".chainId");
        f.deployer = vm.parseJsonAddress(json, ".deployer");
        f.groth16Verifier = vm.parseJsonAddress(json, ".groth16Verifier");
        f.emergencyStop = vm.parseJsonAddress(json, ".emergencyStop");
        f.testToken = vm.parseJsonAddress(json, ".testToken");
        f.vault = vm.parseJsonAddress(json, ".vault");
        f.lezConfigId = vm.parseJsonBytes32(json, ".lezConfigId");
        f.releaseImageId = vm.parseJsonBytes32(json, ".releaseImageId");
        f.burnRoot = vm.parseJsonBytes32(json, ".burnRoot");
        f.nullifier = vm.parseJsonBytes32(json, ".nullifier");
        f.token = vm.parseJsonAddress(json, ".token");
        f.amount = uint256(vm.parseJsonBytes32(json, ".amount"));
        f.recipient = vm.parseJsonAddress(json, ".recipient");
        f.relayer = vm.parseJsonAddress(json, ".relayer");
        f.relayFee = uint256(vm.parseJsonBytes32(json, ".relayFee"));
        f.journal = vm.parseJsonBytes(json, ".journal");
        f.journalDigest = vm.parseJsonBytes32(json, ".journalDigest");
        f.seal = vm.parseJsonBytes(json, ".seal");
        assertEq(sha256(f.journal), f.journalDigest, "fixture journal digest");
    }

    function _loadCheckpoint() internal view returns (CheckpointFixture memory c) {
        string memory json = vm.readFile("test/fixtures/v2/checkpoint.json");
        c.chainId = vm.parseJsonUint(json, ".chainId");
        c.vault = vm.parseJsonAddress(json, ".vault");
        c.lezConfigId = vm.parseJsonBytes32(json, ".lezConfigId");
        c.nMin = uint32(vm.parseJsonUint(json, ".nMin"));
        c.tMin = uint32(vm.parseJsonUint(json, ".tMin"));
        c.genesisState = vm.parseJsonBytes32(json, ".genesisState");
        c.newState = vm.parseJsonBytes32(json, ".newState");
        c.burnRoot = vm.parseJsonBytes32(json, ".burnRoot");
        c.burnCount = uint64(vm.parseJsonUint(json, ".burnCount"));
        c.importedLeafCount = uint64(vm.parseJsonUint(json, ".importedLeafCount"));
        c.sunset = vm.parseJsonBool(json, ".sunset");
    }

    /// @dev EIP-2028 calldata cost: 16 gas per non-zero byte, 4 per zero byte.
    function _calldataGas(bytes memory data) internal pure returns (uint256 g) {
        for (uint256 i = 0; i < data.length; i++) {
            g += data[i] == 0 ? 4 : 16;
        }
    }
}
