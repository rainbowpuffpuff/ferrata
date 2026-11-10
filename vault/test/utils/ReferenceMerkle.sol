// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

/// @notice Test oracle: a plain, non-incremental restatement of the deposit-tree spec. It shares no code with the
///         vault: preimages are built with `bytes.concat` over fixed-size types, and roots are recomputed level by
///         level from the full leaf list, padding with empty subtrees.
library ReferenceMerkle {
    error IndexOutOfRange();

    uint256 internal constant DEPTH = 32;

    function leafTag() internal pure returns (bytes32) {
        return sha256(bytes("ferrata/vault/leaf/v2"));
    }

    /// @notice 0x00 || LEAF_TAG || uint64 leafIndex (8 bytes, big-endian) || depositor (20 bytes) || token (20 bytes)
    ///         || uint256 amount || commitment: 1 + 32 + 8 + 20 + 20 + 32 + 32 = 145 bytes.
    function leafPreimage(uint64 leafIndex, address depositor, address token, uint256 amount, bytes32 commitment)
        internal
        pure
        returns (bytes memory)
    {
        return bytes.concat(
            bytes1(0x00), leafTag(), bytes8(leafIndex), bytes20(depositor), bytes20(token), bytes32(amount), commitment
        );
    }

    function leafHash(uint64 leafIndex, address depositor, address token, uint256 amount, bytes32 commitment)
        internal
        pure
        returns (bytes32)
    {
        return sha256(leafPreimage(leafIndex, depositor, token, amount, commitment));
    }

    function nodeHash(bytes32 left, bytes32 right) internal pure returns (bytes32) {
        return sha256(bytes.concat(bytes1(0x01), left, right));
    }

    /// @notice zeros[h] is the root of an empty subtree of height h, for h = 0..32 (zeros[0] is the empty leaf).
    function zeroHashes() internal pure returns (bytes32[] memory zeros) {
        zeros = new bytes32[](DEPTH + 1);
        zeros[0] = bytes32(0);
        for (uint256 h = 1; h <= DEPTH; h++) {
            zeros[h] = nodeHash(zeros[h - 1], zeros[h - 1]);
        }
    }

    function root(bytes32[] memory leaves) internal pure returns (bytes32) {
        bytes32[] memory zeros = zeroHashes();
        bytes32[] memory level = leaves;
        for (uint256 h = 0; h < DEPTH; h++) {
            level = _parentLevel(level, zeros[h]);
        }
        return level.length == 0 ? zeros[DEPTH] : level[0];
    }

    /// @notice Sibling at each height, bottom-up, for the leaf at `index`.
    function path(bytes32[] memory leaves, uint256 index) internal pure returns (bytes32[] memory siblings) {
        if (index >= leaves.length) revert IndexOutOfRange();
        bytes32[] memory zeros = zeroHashes();
        siblings = new bytes32[](DEPTH);
        bytes32[] memory level = leaves;
        uint256 position = index;
        for (uint256 h = 0; h < DEPTH; h++) {
            uint256 sibling = position ^ 1;
            siblings[h] = sibling < level.length ? level[sibling] : zeros[h];
            level = _parentLevel(level, zeros[h]);
            position >>= 1;
        }
    }

    function rootFromPath(bytes32 leaf, uint256 index, bytes32[] memory siblings)
        internal
        pure
        returns (bytes32 node)
    {
        node = leaf;
        for (uint256 h = 0; h < DEPTH; h++) {
            node = ((index >> h) & 1) == 0 ? nodeHash(node, siblings[h]) : nodeHash(siblings[h], node);
        }
    }

    function _parentLevel(bytes32[] memory level, bytes32 emptySibling)
        private
        pure
        returns (bytes32[] memory parent)
    {
        parent = new bytes32[]((level.length + 1) / 2);
        for (uint256 i = 0; i < parent.length; i++) {
            bytes32 left = level[2 * i];
            bytes32 right = 2 * i + 1 < level.length ? level[2 * i + 1] : emptySibling;
            parent[i] = nodeHash(left, right);
        }
    }
}
