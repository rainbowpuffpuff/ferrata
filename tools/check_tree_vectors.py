#!/usr/bin/env python3
"""Recompute test/vectors/tree.json from its inputs with hashlib, independently of the Solidity code.

Usage: python3 -I tools/check_tree_vectors.py test/vectors/tree.json
Exits non-zero on the first mismatch.
"""

import hashlib
import json
import sys


def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def unhex(value: str) -> bytes:
    assert value.startswith("0x"), value
    return bytes.fromhex(value[2:])


def node(left: bytes, right: bytes) -> bytes:
    return sha256(b"\x01" + left + right)


def root(leaves: list, zeros: list, depth: int) -> bytes:
    level = list(leaves)
    for height in range(depth):
        if len(level) % 2 == 1:
            level.append(zeros[height])
        level = [node(level[2 * k], level[2 * k + 1]) for k in range(len(level) // 2)]
    return level[0] if level else zeros[depth]


def main(path: str) -> None:
    with open(path, encoding="utf-8") as f:
        vectors = json.load(f)

    depth = vectors["treeDepth"]
    assert depth == 32

    tag = sha256(vectors["leafTagPreimage"].encode())
    assert tag == unhex(vectors["leafTag"]), "leafTag"

    zeros = [bytes(32)]
    for _ in range(depth):
        zeros.append(node(zeros[-1], zeros[-1]))
    assert [unhex(z) for z in vectors["zeroHashes"]] == zeros, "zeroHashes"
    assert unhex(vectors["emptyRoot"]) == zeros[depth], "emptyRoot"

    leaves = []
    for i, d in enumerate(vectors["deposits"]):
        assert d["leafIndex"] == i, "leafIndex"
        token = unhex(d["token"])
        assert len(token) == 20, "token length"
        preimage = (
            b"\x00"
            + tag
            + i.to_bytes(8, "big")
            + token
            + int(d["amount"]).to_bytes(32, "big")
            + unhex(d["commitment"])
        )
        assert len(preimage) == 125, "preimage length"
        assert preimage == unhex(d["leafPreimage"]), f"leafPreimage {i}"
        leaf = sha256(preimage)
        assert leaf == unhex(d["leaf"]), f"leaf {i}"
        leaves.append(leaf)
        assert root(leaves, zeros, depth) == unhex(d["rootAfterInsert"]), f"rootAfterInsert {i}"

    final = root(leaves, zeros, depth)
    assert final == unhex(vectors["finalRoot"]), "finalRoot"

    for i, d in enumerate(vectors["deposits"]):
        siblings = [unhex(s) for s in d["pathToFinalRoot"]]
        assert len(siblings) == depth, f"path length {i}"
        current = leaves[i]
        for height, sibling in enumerate(siblings):
            current = node(current, sibling) if ((i >> height) & 1) == 0 else node(sibling, current)
        assert current == final, f"pathToFinalRoot {i}"

    print(f"ok: {len(leaves)} leaves, {depth + 1} zero hashes, roots and paths match ({path})")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "test/vectors/tree.json")
