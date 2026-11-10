#!/usr/bin/env python3
"""Write vectors/acc-leaf.json: test vectors for accumulator-leaf facts (ferrata-core's
`accumulator` module), computed here with hashlib, independently of the Rust code.

The tree: depth 32, inner node sha256(0x01 || left || right), empty leaf 32 zero bytes. A leaf's
path is its 32 siblings from the leaf level up; at height h, bit h of the leaf index is 1 when the
running node is the right child. The identifier of "leaf i of the accumulator at `slot` of
`address` on chain `chainId`" is sha256("ferrata/fact/acc-leaf/v1" || chainId (uint256) ||
address (20 bytes) || slot (uint256) || i (uint64) || leaf), big-endian, and does not depend on
the root. Leaves are opaque here: leaf i is sha256("ferrata/test/acc-leaf/" || uint64 i).

The file is shared with the bridge crate, whose vault keeps such a root in its storage.

Usage: scripts/gen-acc-leaf-vectors.py [OUT]     (default: vectors/acc-leaf.json)
"""
import hashlib
import json
import os
import sys

DEPTH = 32
TAG = b"ferrata/fact/acc-leaf/v1"


def sha256(data):
    return hashlib.sha256(data).digest()


def node(left, right):
    return sha256(b"\x01" + left + right)


ZEROS = [bytes(32)]
for _ in range(DEPTH):
    ZEROS.append(node(ZEROS[-1], ZEROS[-1]))


def leaf(i):
    return sha256(b"ferrata/test/acc-leaf/" + i.to_bytes(8, "big"))


def tree(leaves):
    """The root, and a function giving each index's path (empty positions included)."""
    levels = [list(leaves)]
    for h in range(DEPTH):
        cur = levels[-1]
        levels.append([node(cur[k], cur[k + 1] if k + 1 < len(cur) else ZEROS[h]) for k in range(0, len(cur), 2)])
    root = levels[DEPTH][0] if leaves else ZEROS[DEPTH]

    def path(index):
        out, i = [], index
        for h in range(DEPTH):
            sibling = i ^ 1
            out.append(levels[h][sibling] if sibling < len(levels[h]) else ZEROS[h])
            i >>= 1
        return out

    return root, path


def root_of(index, value, path):
    n = value
    for h, sibling in enumerate(path):
        n = node(sibling, n) if (index >> h) & 1 else node(n, sibling)
    return n


def identifier(chain_id, address, slot, index, value):
    return sha256(TAG + chain_id.to_bytes(32, "big") + address + slot + index.to_bytes(8, "big") + value)


def hx(b):
    return "0x" + b.hex()


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "vectors", "acc-leaf.json")
    chain_id, address, slot = 1, bytes([0x11]) * 20, bytes(32)
    snapshots = []
    for count in (5, 9):
        leaves = [leaf(i) for i in range(count)]
        root, path = tree(leaves)
        entries = []
        for i, value in enumerate(leaves):
            p = path(i)
            assert root_of(i, value, p) == root
            entries.append({"leafIndex": i, "leaf": hx(value), "path": [hx(s) for s in p],
                            "id": hx(identifier(chain_id, address, slot, i, value))})
        snapshots.append({"leafCount": count, "root": hx(root), "leaves": entries})

    # Refusals, with the ferrata-core error each must produce.
    leaves5 = [leaf(i) for i in range(5)]
    root5, path5 = tree(leaves5)
    _, path9 = tree([leaf(i) for i in range(9)])
    flipped = path5(2)
    flipped[7] = bytes([flipped[7][0] ^ 1]) + flipped[7][1:]
    assert root_of(5, bytes(32), path5(5)) == root5  # an empty position hashes to the root
    invalid = [
        {"case": "index 2^32 + 3 with leaf 3's path", "leafIndex": 3 + 2**32, "leaf": hx(leaves5[3]),
         "path": [hx(s) for s in path5(3)], "root": hx(root5), "error": "AccumulatorIndexOutOfRange"},
        {"case": "31 siblings", "leafIndex": 3, "leaf": hx(leaves5[3]), "path": [hx(s) for s in path5(3)[:31]],
         "root": hx(root5), "error": "AccumulatorPathInvalid"},
        {"case": "33 siblings", "leafIndex": 3, "leaf": hx(leaves5[3]), "path": [hx(s) for s in path5(3) + [ZEROS[0]]],
         "root": hx(root5), "error": "AccumulatorPathInvalid"},
        {"case": "a flipped sibling at height 7", "leafIndex": 2, "leaf": hx(leaves5[2]), "path": [hx(s) for s in flipped],
         "root": hx(root5), "error": "AccumulatorRootMismatch"},
        {"case": "another index (2 ^ 1)", "leafIndex": 3, "leaf": hx(leaves5[2]), "path": [hx(s) for s in path5(2)],
         "root": hx(root5), "error": "AccumulatorRootMismatch"},
        {"case": "leaf 0's path in the 9-leaf tree, against the 5-leaf root", "leafIndex": 0, "leaf": hx(leaves5[0]),
         "path": [hx(s) for s in path9(0)], "root": hx(root5), "error": "AccumulatorRootMismatch"},
        {"case": "the empty leaf at the first free position, with its true path", "leafIndex": 5, "leaf": hx(bytes(32)),
         "path": [hx(s) for s in path5(5)], "root": hx(root5), "error": "AccumulatorLeafEmpty"},
    ]
    variants = [
        {"chainId": c, "address": hx(a), "slot": hx(s), "leafIndex": 3, "leaf": hx(leaves5[3]),
         "id": hx(identifier(c, a, s, 3, leaves5[3]))}
        for c, a, s in [(1, address, slot), (11155111, address, slot), (1, bytes([0x22]) * 20, slot),
                        (1, address, (1).to_bytes(32, "big"))]
    ]
    doc = {
        "description": "Accumulator-leaf facts of ferrata-core (crates/ferrata-core/src/accumulator.rs), generated by "
                       "scripts/gen-acc-leaf-vectors.py with hashlib. Shared with the bridge crate.",
        "treeDepth": DEPTH,
        "nodeEncoding": "sha256(0x01 || left || right); the empty leaf is 32 zero bytes",
        "pathEncoding": "32 siblings from the leaf level up; at height h, bit h of leafIndex is 1 when the running node "
                        "is the right child; leafIndex < 2^32",
        "identifierEncoding": "sha256(\"ferrata/fact/acc-leaf/v1\" || chainId (uint256, 32 bytes) || address (20 bytes) "
                              "|| slot (uint256, 32 bytes) || leafIndex (uint64, 8 bytes) || leaf (32 bytes)), big-endian; "
                              "the root is not part of it",
        "leafEncoding": "opaque here: leaf i is sha256(\"ferrata/test/acc-leaf/\" || uint64 i, big-endian)",
        "chainId": chain_id,
        "address": hx(address),
        "slot": hx(slot),
        "zeroHashes": [hx(z) for z in ZEROS],
        "emptyRoot": hx(ZEROS[DEPTH]),
        "snapshots": snapshots,
        "invalid": invalid,
        "identifierVariants": variants,
    }
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print("wrote", out)


if __name__ == "__main__":
    main()
