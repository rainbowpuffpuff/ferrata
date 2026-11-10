#!/usr/bin/env python3
"""Lines of non-test Rust in crates/ferrata-core/src, per file, as a markdown table.

* physical: every line of the file, as `wc -l` counts them, minus in-crate test modules;
* code: those lines that are neither blank nor a comment-only line (`//`, `///`, `//!`).

An in-crate test module (`#[cfg(test)] mod tests { ... }`) is excluded from its attribute line to
the end of the file; the script checks that each such module is the last item of its file.
The category column is this repository's own classification (README, "Reuse and audit surface").

Usage: python3 -I scripts/count-lines.py
"""
import os

ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "crates", "ferrata-core", "src")

CATEGORY = {
    "validate.rs": "consensus-spec transcription",
    "store.rs": "consensus-spec transcription",
    "upgrade.rs": "consensus-spec transcription",
    "types.rs": "consensus-spec transcription",
    "ssz.rs": "consensus-spec transcription",
    "merkle.rs": "consensus-spec transcription",
    "fork.rs": "consensus-spec transcription",
    "hash.rs": "consensus-spec transcription",
    "policy.rs": "new protocol logic",
    "bls/mod.rs": "BLS glue and new logic",
    "bls/blst_backend.rs": "BLS glue and new logic",
    "bls/zkcrypto_backend.rs": "BLS glue and new logic",
    "rlp.rs": "execution-spec transcription",
    "mpt.rs": "execution-spec transcription",
    "exec.rs": "execution-spec transcription",
    "keccak.rs": "execution-spec transcription",
    "attest.rs": "new protocol logic",
    "accumulator.rs": "new protocol logic",
    "error.rs": "plumbing",
    "lib.rs": "plumbing",
}


def count(path):
    lines = open(path).read().split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    end = len(lines)
    for i, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            depth, opened = 0, False
            for j in range(i, len(lines)):
                depth += lines[j].count("{") - lines[j].count("}")
                opened = opened or "{" in lines[j]
                if opened and depth == 0:
                    assert all(not l.strip() for l in lines[j + 1:]), f"{path}: test module is not last"
                    break
            end = i
            break
    kept = lines[:end]
    code = sum(1 for l in kept if l.strip() and not l.strip().startswith("//"))
    return len(kept), code, len(lines) - end


def main():
    rows, total_physical, total_code, tests = [], 0, 0, 0
    by_category = {}
    for rel in sorted(CATEGORY, key=lambda r: (CATEGORY[r], r)):
        physical, code, test_lines = count(os.path.join(ROOT, rel))
        rows.append((rel, CATEGORY[rel], physical, code))
        total_physical, total_code, tests = total_physical + physical, total_code + code, tests + test_lines
        p, c = by_category.get(CATEGORY[rel], (0, 0))
        by_category[CATEGORY[rel]] = (p + physical, c + code)
    found = sorted(os.path.relpath(os.path.join(d, f), ROOT) for d, _, fs in os.walk(ROOT) for f in fs if f.endswith(".rs"))
    assert found == sorted(CATEGORY), f"files not classified: {set(found) ^ set(CATEGORY)}"
    print("| File | Category | Lines | Code lines |")
    print("|---|---|---:|---:|")
    for rel, cat, physical, code in rows:
        print(f"| `{rel}` | {cat} | {physical} | {code} |")
    print(f"| **total** | | **{total_physical}** | **{total_code}** |")
    print()
    print("| Category | Lines | Code lines |")
    print("|---|---:|---:|")
    for cat, (p, c) in sorted(by_category.items()):
        print(f"| {cat} | {p} | {c} |")
    print(f"\n(in-crate test modules excluded: {tests} lines)")


if __name__ == "__main__":
    main()
