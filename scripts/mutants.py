#!/usr/bin/env python3
"""Hand-written mutation pass over ferrata-bridge-core: applies one mutation at a time (an exact source
snippet replaced by a wrong one), runs `cargo test -p ferrata-bridge-core`, records whether the suite
fails (killed) or passes (survived), and always restores the file. Mutations that fail to compile are
reported separately. Run from the repository root: python3 -I scripts/mutants.py [M01 M02 ...]"""

import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/ferrata-bridge-core/src"

# (id, file, original snippet, mutated snippet, what it breaks)
MUTANTS = [
    ("M01", "tree.rs", "if leaf_index >= MAX_LEAVES {\n        return Err(VerifyError::LeafIndexOutOfRange);",
     "if leaf_index > MAX_LEAVES {\n        return Err(VerifyError::LeafIndexOutOfRange);", "path index range check off by one"),
    ("M02", "tree.rs", "{ node(&running, sibling) } else { node(sibling, &running) }",
     "{ node(sibling, &running) } else { node(&running, sibling) }", "path verification swaps left and right"),
    ("M03", "tree.rs", "if (index >> h) & 1 == 1 {\n                running = node(&self.branch[h], &running);",
     "if (index >> h) & 1 == 0 {\n                running = node(&self.branch[h], &running);", "incremental append takes the wrong branch"),
    ("M04", "tree.rs", "                if !saved {\n                    self.branch[h] = running;\n                    saved = true;\n                }",
     "                self.branch[h] = running;\n                let _ = saved;",
     "incremental append writes a branch at every 0 bit (EQUIVALENT: the extra writes are always overwritten "
     "by the insertion that completes that subtree before any read; only Solidity gas would differ)"),
    ("M05", "encoding.rs", "Concat::new().put(TAG_C).put(s).put(dest).finish()",
     "Concat::new().put(TAG_C).put(dest).put(s).finish()", "C preimage field order"),
    ("M06", "encoding.rs", "        .put(&chain_id.to_be_bytes())\n        .put(vault)\n        .put(&leaf_index.to_be_bytes())\n        .put(leaf)",
     "        .put(&chain_id.to_le_bytes())\n        .put(vault)\n        .put(&leaf_index.to_be_bytes())\n        .put(leaf)", "fid chain id little-endian"),
    ("M07", "encoding.rs", "Concat::new().put(TAG_NF_OUT).put(s_out).put(burn_id).finish()",
     "Concat::new().put(TAG_NF).put(&s_out[..]).put(burn_id).put(&[0u8; 4]).finish()", "N_out uses the inbound nullifier tag"),
    ("M08", "encoding.rs", "            .put(&address_word(&self.recipient))\n            .put(&address_word(&self.fee_recipient))",
     "            .put(&address_word(&self.fee_recipient))\n            .put(&address_word(&self.recipient))", "journal recipient / fee recipient swapped"),
    ("M09", "encoding.rs", "    if word[..12].iter().any(|b| *b != 0) {\n        return None;\n    }\n    word[12..]",
     "    word[12..]", "journal decode ignores address padding"),
    ("M10", "encoding.rs", "pub const BURN_LEAF_TAG: Hash = hex32(\"b9a876f3673cc2c3546df046edc81938bcdbc115e53e6b0614b94bf16ae7f53b\");",
     "pub const BURN_LEAF_TAG: Hash = LEAF_TAG;", "burn leaves share the deposit leaf tag"),
    ("M11", "inbound.rs", "    Ok(nullifier(&w.s, &deposit_id(chain_id, vault, w.leaf_index, &leaf)))",
     "    Ok(nullifier(&w.s, root))", "claim nullifier keyed on the root"),
    ("M12", "inbound.rs", "            if commitment(&s, &dest) != d.commitment {\n                return None;\n            }",
     "            let _ = commitment(&s, &dest);", "recovery claims every deposit"),
    ("M13", "outbound.rs", "    let c_out = burn_commitment(&w.s_out, p.chain_id, &p.vault, &p.recipient);\n    let leaf = burn_leaf(",
     "    let c_out = burn_commitment(&w.s_out, p.chain_id, &[0u8; 20], &p.recipient);\n    let leaf = burn_leaf(", "release ignores the vault in C_out"),
    ("M14", "outbound.rs", "        *o = b ^ p;", "        *o = *b;", "memo not encrypted"),
    ("M15", "outbound.rs", "    while i - next_i < gap_limit {", "    while i - next_i <= gap_limit {", "burn scan gap limit off by one"),
    ("M16", "outbound.rs", "                if burn_commitment(&s_out, chain_id, vault, &recipient) != burn.note.c_out {\n                    continue;\n                }",
     "", "burn scan does not authenticate the memo"),
    ("M17", "outbound.rs", "    Ok(burn_nullifier(&w.s_out, &burn_id(&p.lez_config_id, w.leaf_index, &leaf)))",
     "    Ok(burn_nullifier(&w.s_out, &burn_id(&p.lez_config_id, w.leaf_index, burn_root)))", "N_out keyed on the burn root"),
    ("M18", "hash.rs", "        mac.update(message);", "        mac.update(&message[1..]);", "PRF drops the first message byte"),
]


def run_tests():
    env = dict(os.environ, TMPDIR=str(ROOT.parent / "tmp"), CARGO_TARGET_DIR=str(ROOT / "target"))
    start = time.time()
    r = subprocess.run(["cargo", "test", "-p", "ferrata-bridge-core", "--no-fail-fast"], cwd=ROOT, env=env,
                       capture_output=True, text=True, timeout=900)
    out = r.stdout + r.stderr
    failed = sorted({line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")})
    compiled = "could not compile" not in out
    return r.returncode, compiled, failed, time.time() - start


def main():
    code, compiled, failed, secs = run_tests()
    if code != 0:
        sys.exit(f"baseline fails: {failed}")
    print(f"baseline: all tests pass ({secs:.0f} s)", flush=True)
    only = set(sys.argv[1:])  # optional mutant ids, e.g. M02
    killed = survived = broken = 0
    for mid, name, old, new, what in MUTANTS:
        if only and mid not in only:
            continue
        path = SRC / name
        original = path.read_text()
        if original.count(old) != 1:
            print(f"{mid} SKIP  snippet occurs {original.count(old)} times in {name}", flush=True)
            continue
        path.write_text(original.replace(old, new))
        try:
            code, compiled, failed, secs = run_tests()
        finally:
            path.write_text(original)
        if not compiled:
            broken += 1
            print(f"{mid} NOCOMPILE {name:12} {what}", flush=True)
        elif code != 0:
            killed += 1
            print(f"{mid} killed    {name:12} {what}  [{', '.join(failed[:4])}{' …' if len(failed) > 4 else ''}] ({secs:.0f} s)", flush=True)
        else:
            survived += 1
            print(f"{mid} SURVIVED  {name:12} {what} ({secs:.0f} s)", flush=True)
    print(f"total {killed + survived + broken} run: killed {killed}, survived {survived}, did not compile {broken}")
    sys.exit(1 if survived else 0)


if __name__ == "__main__":
    main()
