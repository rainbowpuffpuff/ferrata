// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

contract MerkleTest is VaultTestBase {
    /// The storage-slot contract with the LEZ side (see README, "Storage layout").
    uint256 internal constant DEPOSIT_ROOT_SLOT = 0;
    uint256 internal constant NEXT_LEAF_INDEX_SLOT = 1;
    uint256 internal constant LEZ_CONFIG_ID_SLOT = 2;
    /// `_branch[h]` lives at slot BRANCH_SLOT + h (from `forge inspect FerrataVault storageLayout`).
    uint256 internal constant BRANCH_SLOT = 3;

    bytes32[] internal leaves;

    /// @dev One deposit of a pseudo-random token, denomination, depositor and commitment; records its leaf.
    function _depositRandom(uint256 seed) internal {
        address[4] memory tokens = [ETH, address(usdc), address(dai), address(usdt)];
        address token = tokens[seed % 4];
        uint8 denomIndex = uint8((seed >> 8) % 3);
        bytes32 commitment = keccak256(abi.encode("commitment", seed));
        address depositor = ((seed >> 16) % 2 == 0) ? alice : bob;
        uint64 leafIndex = _deposit(depositor, token, denomIndex, commitment);
        leaves.push(ReferenceMerkle.leafHash(leafIndex, depositor, token, _amount(token, denomIndex), commitment));
    }

    function _load(uint256 slot) internal view returns (bytes32) {
        return vm.load(address(vault), bytes32(slot));
    }

    // ---- storage-slot contract ------------------------------------------------------------------------------------

    function test_storageSlots_root0_leafCount1_configId2() public {
        assertEq(vault.DEPOSIT_ROOT_SLOT(), DEPOSIT_ROOT_SLOT);
        assertEq(vault.NEXT_LEAF_INDEX_SLOT(), NEXT_LEAF_INDEX_SLOT);
        assertEq(vault.LEZ_CONFIG_ID_SLOT(), LEZ_CONFIG_ID_SLOT);
        assertEq(_load(DEPOSIT_ROOT_SLOT), vault.depositRoot());
        assertEq(_load(NEXT_LEAF_INDEX_SLOT), bytes32(0));
        assertEq(_load(LEZ_CONFIG_ID_SLOT), LEZ_CONFIG_ID);
        assertEq(vault.storedLezConfigId(), vault.lezConfigId());

        for (uint256 i = 0; i < 6; i++) {
            _depositRandom(i);
            assertEq(_load(DEPOSIT_ROOT_SLOT), vault.depositRoot());
            assertEq(_load(DEPOSIT_ROOT_SLOT), ReferenceMerkle.root(leaves));
            // The whole word, so nothing else is packed into slot 1.
            assertEq(_load(NEXT_LEAF_INDEX_SLOT), bytes32(uint256(i + 1)));
        }

        // Admin, freeze and release activity leaves both slots alone.
        bytes32 root = vault.depositRoot();
        vm.prank(admin);
        vault.setMinLezDepth(1);
        vm.prank(freezer);
        vault.setDepositsFrozen(true);
        _acceptRoot(keccak256("burn root"));
        _release(_request(ETH, 0, recipient, keccak256("burn root"), keccak256("n")));
        assertEq(_load(DEPOSIT_ROOT_SLOT), root);
        assertEq(_load(NEXT_LEAF_INDEX_SLOT), bytes32(uint256(6)));
        assertEq(_load(LEZ_CONFIG_ID_SLOT), LEZ_CONFIG_ID);
    }

    function test_emptyTreeRoot() public view {
        bytes32[] memory none = new bytes32[](0);
        assertEq(vault.depositRoot(), ReferenceMerkle.root(none));
        assertEq(_load(DEPOSIT_ROOT_SLOT), ReferenceMerkle.zeroHashes()[32]);
    }

    // ---- incremental root == reference root -----------------------------------------------------------------------

    function testFuzz_incrementalRootMatchesReference(uint256 seed, uint8 count) public {
        uint256 n = bound(count, 1, 40);
        for (uint256 i = 0; i < n; i++) {
            _depositRandom(uint256(keccak256(abi.encode(seed, i))));
            assertEq(vault.depositRoot(), ReferenceMerkle.root(leaves));
        }
    }

    function test_incrementalRootMatchesReference_130Deposits() public {
        for (uint256 i = 0; i < 130; i++) {
            _depositRandom(uint256(keccak256(abi.encode("fixed", i))));
            assertEq(vault.depositRoot(), ReferenceMerkle.root(leaves));
        }
    }

    function test_insertion_writesExactlyOneBranchSlot() public {
        for (uint256 i = 0; i < 40; i++) {
            uint256 leafIndex = vault.nextLeafIndex();
            vm.record();
            _depositRandom(i);
            (, bytes32[] memory writes) = vm.accesses(address(vault));

            uint256 branchWrites;
            uint256 writtenSlot;
            for (uint256 w = 0; w < writes.length; w++) {
                uint256 slot = uint256(writes[w]);
                if (slot >= BRANCH_SLOT && slot < BRANCH_SLOT + 32) {
                    branchWrites++;
                    writtenSlot = slot;
                }
            }
            // The saved subtree sits at the height of the lowest 0 bit of the leaf index.
            uint256 height = 0;
            while ((leafIndex >> height) & 1 == 1) {
                height++;
            }
            assertEq(branchWrites, 1);
            assertEq(writtenSlot, BRANCH_SLOT + height);
        }
    }

    // ---- membership proofs ----------------------------------------------------------------------------------------

    function testFuzz_merklePathVerifiesAgainstDepositRoot(uint256 seed, uint8 count, uint256 pick) public {
        uint256 n = bound(count, 1, 40);
        for (uint256 i = 0; i < n; i++) {
            _depositRandom(uint256(keccak256(abi.encode(seed, i))));
        }
        uint256 index = bound(pick, 0, n - 1);
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, index);
        bytes32 root = vault.depositRoot();
        assertEq(ReferenceMerkle.rootFromPath(leaves[index], index, siblings), root);

        // Changing the leaf, the index or any one sibling breaks the proof.
        assertTrue(ReferenceMerkle.rootFromPath(leaves[index] ^ bytes32(uint256(1)), index, siblings) != root);
        assertTrue(ReferenceMerkle.rootFromPath(leaves[index], index ^ 1, siblings) != root);
        uint256 h = seed % 32;
        siblings[h] = siblings[h] ^ bytes32(uint256(1));
        assertTrue(ReferenceMerkle.rootFromPath(leaves[index], index, siblings) != root);
    }

    // ---- high bits of the index, set up directly in storage -------------------------------------------------------

    /// @notice nextLeafIndex = 2**31: the new leaf opens the right half, so its path goes "left" up to height 30
    ///         and "right" at height 31, where the left sibling must come from the stored branch.
    function test_highIndex_usesStoredBranchAtTopLevel() public {
        bytes32 leftHalf = keccak256("stand-in root of a full left half");
        vm.store(address(vault), bytes32(BRANCH_SLOT + 31), leftHalf);
        vm.store(address(vault), bytes32(NEXT_LEAF_INDEX_SLOT), bytes32(uint256(2 ** 31)));

        bytes32 commitment = keccak256("c");
        _deposit(alice, ETH, 1, commitment);

        bytes32 leaf = ReferenceMerkle.leafHash(uint64(2 ** 31), alice, ETH, 1 ether, commitment);
        bytes32[] memory zeros = ReferenceMerkle.zeroHashes();
        bytes32 rightHalf = leaf;
        for (uint256 h = 0; h < 31; h++) {
            rightHalf = ReferenceMerkle.nodeHash(rightHalf, zeros[h]);
        }
        assertEq(vault.depositRoot(), ReferenceMerkle.nodeHash(leftHalf, rightHalf));
        assertEq(vault.nextLeafIndex(), 2 ** 31 + 1);
        assertEq(_load(BRANCH_SLOT), leaf); // saved as the completed subtree of height 0
        assertEq(_load(BRANCH_SLOT + 31), leftHalf); // untouched
    }

    /// @notice nextLeafIndex = 2**32 - 1: every index bit is 1, so every left sibling is a stored branch, no branch
    ///         is written, and afterwards the tree is full.
    function test_lastLeafCompletesTheTree() public {
        bytes32[] memory branch = new bytes32[](32);
        for (uint256 h = 0; h < 32; h++) {
            branch[h] = keccak256(abi.encode("stand-in completed subtree", h));
            vm.store(address(vault), bytes32(BRANCH_SLOT + h), branch[h]);
        }
        vm.store(address(vault), bytes32(NEXT_LEAF_INDEX_SLOT), bytes32(uint256(2 ** 32 - 1)));

        bytes32 commitment = keccak256("last");
        _deposit(alice, ETH, 0, commitment);

        bytes32 node = ReferenceMerkle.leafHash(uint64(2 ** 32 - 1), alice, ETH, 0.1 ether, commitment);
        for (uint256 h = 0; h < 32; h++) {
            node = ReferenceMerkle.nodeHash(branch[h], node);
            assertEq(_load(BRANCH_SLOT + h), branch[h]);
        }
        assertEq(vault.depositRoot(), node);
        assertEq(vault.nextLeafIndex(), 2 ** 32);

        vm.deal(bob, 1 ether);
        vm.prank(bob);
        vm.expectRevert(FerrataVault.TreeFull.selector);
        vault.depositETH{value: 1 ether}(1, commitment, 0);
    }
}
