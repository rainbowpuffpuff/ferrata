// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {MockERC20, MockUSDT} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Cross-language test vectors for the deposit tree, in test/vectors/tree.json.
///         The test performs 8 real deposits on a fresh vault (mock token code placed at the mainnet USDC, DAI and
///         USDT addresses), takes every leaf and root from the vault, checks them against the reference tree, and
///         compares the resulting JSON byte for byte with the committed file. With FERRATA_WRITE_VECTORS=1 it
///         rewrites the file first.
contract TreeVectorsTest is VaultTestBase {
    string internal constant VECTORS_PATH = "test/vectors/tree.json";

    address internal constant USDC_MAINNET = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    address internal constant DAI_MAINNET = 0x6B175474E89094C44Da98b954EedeAC495271d0F;
    address internal constant USDT_MAINNET = 0xdAC17F958D2ee523a2206206994597C13D831ec7;

    struct VectorDeposit {
        address token;
        uint8 denomIndex;
    }

    function test_treeVectors_matchCommittedFile() public {
        string memory json = _buildVectors();
        // File access is the purpose of this test; foundry.toml limits it to ./test/vectors.
        if (vm.envOr("FERRATA_WRITE_VECTORS", false)) {
            // forge-lint: disable-next-line(unsafe-cheatcode)
            vm.writeFile(VECTORS_PATH, json);
        }
        // forge-lint: disable-next-line(unsafe-cheatcode)
        string memory committed = vm.readFile(VECTORS_PATH);
        assertEq(committed, json, "test/vectors/tree.json is stale: rerun with FERRATA_WRITE_VECTORS=1");
    }

    function _buildVectors() internal returns (string memory) {
        vm.etch(USDC_MAINNET, address(new MockERC20("USD Coin", "USDC", 6)).code);
        vm.etch(DAI_MAINNET, address(new MockERC20("Dai Stablecoin", "DAI", 18)).code);
        vm.etch(USDT_MAINNET, address(new MockUSDT("Tether USD", "USDT", 6)).code);
        vm.startPrank(admin);
        vault.registerToken(USDC_MAINNET, _denoms(100e6, 1000e6, 10_000e6), 1, 1, _unlimited(), _unlimited());
        vault.registerToken(DAI_MAINNET, _denoms(100e18, 1000e18, 10_000e18), 1e12, 1, _unlimited(), _unlimited());
        vault.registerToken(USDT_MAINNET, _denoms(100e6, 1000e6, 10_000e6), 1, 1, _unlimited(), _unlimited());
        vm.stopPrank();

        VectorDeposit[8] memory plan = [
            VectorDeposit(ETH, 1),
            VectorDeposit(USDC_MAINNET, 0),
            VectorDeposit(DAI_MAINNET, 1),
            VectorDeposit(ETH, 0),
            VectorDeposit(USDT_MAINNET, 1),
            VectorDeposit(USDC_MAINNET, 2),
            VectorDeposit(DAI_MAINNET, 0),
            VectorDeposit(ETH, 2)
        ];

        bytes32[] memory zeros = ReferenceMerkle.zeroHashes();
        assertEq(vault.depositRoot(), zeros[32]);

        bytes32[] memory leaves = new bytes32[](8);
        bytes32[] memory roots = new bytes32[](8);
        bytes32[] memory commitments = new bytes32[](8);
        for (uint256 i = 0; i < 8; i++) {
            commitments[i] = sha256(bytes(string.concat("ferrata test vector commitment ", vm.toString(i))));
            uint64 leafIndex = _deposit(alice, plan[i].token, plan[i].denomIndex, commitments[i]);
            assertEq(leafIndex, i);
            uint256 amount = _amount(plan[i].token, plan[i].denomIndex);
            leaves[i] = vault.leafHash(leafIndex, plan[i].token, amount, commitments[i]);
            roots[i] = vault.depositRoot();

            // Cross-check the vault against the independent reference before writing anything down.
            assertEq(leaves[i], ReferenceMerkle.leafHash(leafIndex, plan[i].token, amount, commitments[i]));
            bytes32[] memory prefix = new bytes32[](i + 1);
            for (uint256 k = 0; k <= i; k++) {
                prefix[k] = leaves[k];
            }
            assertEq(roots[i], ReferenceMerkle.root(prefix));
        }

        string memory out = string.concat(
            "{\n",
            '  "description": "FerrataVault deposit-tree vectors: 8 deposits made on the Solidity vault ',
            '(test/TreeVectors.t.sol). Leaves and roots are read from the vault.",\n',
            '  "treeDepth": 32,\n',
            '  "leafTagPreimage": "ferrata/vault/leaf/v1",\n',
            '  "leafTag": "',
            vm.toString(vault.LEAF_TAG()),
            '",\n',
            '  "leafEncoding": "sha256(0x00 || leafTag || uint64 leafIndex, 8 bytes big-endian || token, 20 bytes || ',
            'uint256 amount, 32 bytes big-endian || commitment, 32 bytes): a 125-byte preimage",\n',
            '  "nodeEncoding": "sha256(0x01 || left || right): a 65-byte preimage; the empty leaf is 32 zero bytes",\n',
            '  "ethToken": "native ETH is the zero address",\n',
            '  "zeroHashes": ',
            _jsonArray(zeros, "  "),
            ",\n",
            '  "emptyRoot": "',
            vm.toString(zeros[32]),
            '",\n'
        );
        out = string.concat(out, '  "deposits": [\n');
        for (uint64 i = 0; i < 8; i++) {
            out = string.concat(
                out,
                _depositJson(
                    i, plan[i].token, _amount(plan[i].token, plan[i].denomIndex), commitments[i], leaves, roots[i]
                ),
                i < 7 ? ",\n" : "\n"
            );
        }
        return string.concat(out, "  ],\n", '  "finalRoot": "', vm.toString(roots[7]), '"\n}\n');
    }

    function _depositJson(
        uint64 i,
        address token,
        uint256 amount,
        bytes32 commitment,
        bytes32[] memory leaves,
        bytes32 rootAfter
    ) internal view returns (string memory) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        assertEq(ReferenceMerkle.rootFromPath(leaves[i], i, siblings), vault.depositRoot());
        return string.concat(
            "    {\n",
            '      "leafIndex": ',
            vm.toString(i),
            ",\n",
            '      "token": "',
            vm.toString(abi.encodePacked(token)),
            '",\n',
            '      "amount": "',
            vm.toString(amount),
            '",\n',
            '      "commitment": "',
            vm.toString(commitment),
            '",\n',
            '      "leafPreimage": "',
            vm.toString(ReferenceMerkle.leafPreimage(i, token, amount, commitment)),
            '",\n',
            '      "leaf": "',
            vm.toString(leaves[i]),
            '",\n',
            '      "rootAfterInsert": "',
            vm.toString(rootAfter),
            '",\n',
            '      "pathToFinalRoot": ',
            _jsonArray(siblings, "      "),
            "\n    }"
        );
    }

    function _jsonArray(bytes32[] memory items, string memory indent) internal pure returns (string memory out) {
        out = "[\n";
        for (uint256 i = 0; i < items.length; i++) {
            out = string.concat(out, indent, '  "', vm.toString(items[i]), i + 1 < items.length ? '",\n' : '"\n');
        }
        out = string.concat(out, indent, "]");
    }
}
