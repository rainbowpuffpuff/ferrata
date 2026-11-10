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

# (id, file, original snippet, mutated snippet, what it breaks). SPEC-v2 code; the v1 list and its
# results are in commit 104d746 and artifacts/v1/logs/mutants.log.
MUTANTS = [
    # ---- tree.rs
    ("T01", "tree.rs", "    if u64::from(path.directions) != leaf_index {",
     "    if false && u64::from(path.directions) != leaf_index {", "direction bits not checked against the index"),
    ("T02", "tree.rs", "    if u64::from(path.directions) != leaf_index {",
     "    if path.directions != leaf_index as u32 {", "direction check truncates the index to 32 bits"),
    ("T03", "tree.rs", "        *l = a ^ x;\n        *r = b ^ x;", "        *l = b ^ x;\n        *r = a ^ x;", "left and right swapped"),
    ("T04", "tree.rs", "0u8.wrapping_sub((bit & 1) as u8)", "0u8.wrapping_sub(((bit >> 1) & 1) as u8)", "direction taken from the wrong bit"),
    ("T05", "tree.rs", "    if root_from_path(leaf, path) == *root {", "    if root_from_path(leaf, path) != *root {", "root comparison inverted"),
    ("T06", "tree.rs", "            if (index >> h) & 1 == 1 {\n                running = node(&self.branch[h], &running);",
     "            if (index >> h) & 1 == 0 {\n                running = node(&self.branch[h], &running);", "incremental append takes the wrong side"),
    ("T07", "tree.rs", "                if !saved {\n                    self.branch[h] = running;\n                    saved = true;\n                }",
     "                self.branch[h] = running;\n                let _ = saved;",
     "append writes a branch at every 0 bit (EQUIVALENT for roots: extra writes are overwritten before they are read)"),
    ("T08", "tree.rs", "            if (self.next_index >> h) & 1 == 1 {\n                *slot = self.branch[h];",
     "            if true {\n                *slot = self.branch[h];", "frontier not canonical (stale entries kept)"),
    ("T09", "tree.rs", "            if (count >> h) & 1 == 0 && *f != [0u8; 32] {", "            if false {", "from_frontier accepts non-canonical frontiers"),
    ("T10", "tree.rs", "{ node(&frontier[h], &running) }", "{ node(&running, &frontier[h]) }", "from_frontier puts the completed subtree on the right"),
    # ---- encoding.rs
    ("E01", "encoding.rs", "put(&[kind as u8])", "put(&[0x01])", "C ignores the destination kind"),
    ("E02", "encoding.rs", "        .put(&DEPOSIT_ROOT_SLOT)", "        .put(&[1u8; 32])", "fid with another slot"),
    ("E03", "encoding.rs", "        .put(&leaf_index.to_be_bytes())\n        .put(depositor)\n        .put(token)",
     "        .put(&leaf_index.to_be_bytes())\n        .put(token)\n        .put(token)", "deposit leaf does not bind the depositor"),
    ("E04", "encoding.rs", "        .put(&[BURN_LEAF_PREFIX])", "        .put(&[LEAF_PREFIX])", "burn leaf with the deposit-leaf prefix"),
    ("E05", "encoding.rs", "        .put(c_out)\n        .put(r)\n        .finish()", "        .put(c_out)\n        .put(c_out)\n        .finish()", "burn leaf does not bind r"),
    ("E06", "encoding.rs", "        .put(&BURN_TAG)\n        .put(config_id)", "        .put(&BURN_TAG)\n        .put(&BURN_TAG)", "burn leaf does not bind configId"),
    ("E07", "encoding.rs", "Concat::new().put(TAG_S_OUT).put(config_id).put(r).finish()", "Concat::new().put(TAG_S_OUT).put(r).put(r).finish()",
     "sOut not bound to configId"),
    ("E08", "encoding.rs", "put(TAG_BURN_MAC).put(r).put(token).put(amount).put(c_out)", "put(TAG_BURN_MAC).put(r).put(token).put(&[0u8; 32]).put(c_out)",
     "MAC does not cover the amount"),
    ("E09", "encoding.rs", "            .put(&address_word(&self.recipient))\n            .put(&address_word(&self.relayer))",
     "            .put(&address_word(&self.relayer))\n            .put(&address_word(&self.recipient))", "release journal recipient / relayer swapped"),
    ("E10", "encoding.rs", "            .put(&u64_word(u64::from(self.sunset)))", "            .put(&u64_word(0))", "checkpoint journal drops sunset"),
    ("E11", "encoding.rs", "        .put(&[u8::from(s.sunset)])", "        .put(&[0])", "S drops sunset"),
    ("E12", "encoding.rs", "    h.update(n.to_be_bytes());", "    h.update([0u8; 4]);", "keySetHash without the key count"),
    ("E13", "encoding.rs", "                1 => true,\n                _ => return None,", "                _ => true,", "journal decode accepts any non-zero sunset word"),
    ("E14", "encoding.rs", "    if word[..12].iter().any(|b| *b != 0) {\n        return None;\n    }\n    word[12..]", "    word[12..]",
     "journal decode ignores address padding"),
    ("E15", "encoding.rs", ".put(depositor).put(&j.to_be_bytes()).finish()", ".put(depositor).put(&[0u8; 8]).finish()", "s does not bind j"),
    ("E16", "encoding.rs", "            .put(&u64_word(self.burn_count))\n            .put(&u64_word(self.imported_leaf_count))",
     "            .put(&u64_word(self.imported_leaf_count))\n            .put(&u64_word(self.burn_count))", "checkpoint journal burnCount / importedLeafCount swapped"),
    # ---- inbound.rs
    ("I01", "inbound.rs", "    Ok(nullifier(&w.s, &deposit_id(chain_id, vault, w.leaf_index, &leaf)))", "    Ok(nullifier(&w.s, root))",
     "claim nullifier keyed on the root (one mint per root)"),
    ("I02", "inbound.rs", "    let c = commitment(&w.s, w.kind, &w.dest);", "    let c = commitment(&w.s, DestKind::Private, &w.dest);",
     "claim ignores the witness kind"),
    ("I03", "inbound.rs", "    if nullifiers.insert_new((*chain_id, *vault, *n)) {", "    if nullifiers.insert_new((*chain_id, *vault, *n)) || true {",
     "nullifier registry never refuses"),
    ("I04", "inbound.rs", "insert_new((*chain_id, *vault, *n))", "insert_new((*chain_id, [0u8; 20], *n))", "nullifier registry not keyed by vault"),
    ("I05", "inbound.rs", "DestKind::ALL.map(|kind| (kind, a, dest_of(kind, a)))", "[DestKind::Private].map(|kind| (kind, a, dest_of(kind, a)))",
     "recovery tries one kind only"),
    ("I06", "inbound.rs", "(0..gap).flat_map", "(0..gap.saturating_sub(1)).flat_map", "recovery window one account short"),
    ("I07", "inbound.rs", "            0x01 => Some(DestKind::Private),", "            0x01 | 0x03 => Some(DestKind::Private),", "an unknown kind byte accepted"),
    ("I08", "inbound.rs", "    verify_path(root, &leaf, w.leaf_index, &w.path)?;", "    let _ = verify_path(root, &leaf, w.leaf_index, &w.path);",
     "claim ignores the path check"),
    # ---- outbound.rs
    ("O01", "outbound.rs", "&p.amount, &c_out, &w.r);", "&p.amount, &c_out, &c_out);", "release leaf ignores r"),
    ("O02", "outbound.rs", "    let c_out = burn_commitment(&w.s_out, &p.chain_id, &p.vault, &p.recipient);",
     "    let c_out = burn_commitment(&w.s_out, &p.chain_id, &[0u8; 20], &p.recipient);", "release COut ignores the vault"),
    ("O03", "outbound.rs", "    Ok(burn_nullifier(&w.s_out, &burn_id(&p.config_id, w.burn_index, &leaf)))", "    Ok(burn_nullifier(&w.s_out, burn_root))",
     "NOut keyed on the burn root"),
    ("O04", "outbound.rs", "acc | (a ^ b)) == 0", "acc | (a ^ b)) == 0 || true", "MAC check always passes"),
    ("O05", "outbound.rs", "want.iter().zip(self.mac.iter())", "want.iter().zip(self.mac.iter()).take(1)", "MAC check compares one byte"),
    ("O06", "outbound.rs", "    if tags.insert_new(note.r) {", "    if tags.insert_new(note.r) || true {", "burn-tag registry never refuses"),
    ("O07", "outbound.rs", ".filter(|b| b.note.mac_verifies(view))", ".filter(|_| true)", "burn scan ignores the MAC"),
    ("O08", "outbound.rs", "            if burn_commitment(&s_out, chain_id, vault, &v.recipient) != note.c_out {", "            if false {",
     "burn scan does not authenticate the memo"),
    ("O09", "outbound.rs", "    while i - next_i < gap_limit {", "    while i - next_i <= gap_limit {", "burn scan gap limit off by one"),
    ("O10", "outbound.rs", "        let r = burn_tag(&keys.view, config_id, i);", "        let r = burn_tag(&keys.auth, config_id, i);", "tags from kAuth"),
    ("O11", "outbound.rs", "        *o = b ^ p;", "        *o = *b;", "memo not encrypted"),
    ("O12", "outbound.rs", "        let s_out = burn_secret(&keys.auth, config_id, &r);", "        let s_out = burn_secret(&keys.view, config_id, &r);",
     "sOut from kView (a viewer could release)"),
    # ---- hash.rs, checkpoint.rs
    ("H01", "hash.rs", "(hmac_sha256(bridge_seed, TAG_AUTH), hmac_sha256(bridge_seed, TAG_VIEW))",
     "(hmac_sha256(bridge_seed, TAG_VIEW), hmac_sha256(bridge_seed, TAG_AUTH))", "kAuth and kView swapped"),
    ("H02", "hash.rs", "        let mut mac = self.0.clone();\n        mac.update(message);", "        let mut mac = self.0.clone();\n        mac.update(&message[1..]);",
     "PRF drops the first message byte"),
    ("C01", "checkpoint.rs", "two_thirds_up.min(n.saturating_sub(1)).max(1)", "two_thirds_up.max(1)", "t(n) without the n - 1 cap"),
    ("C02", "checkpoint.rs", "(2 * u64::from(n)).div_ceil(3)", "(2 * u64::from(n)) / 3", "t(n) rounds 2n/3 down"),
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
