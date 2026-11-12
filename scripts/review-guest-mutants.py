#!/usr/bin/env python3
"""Mutants of the agent review of 12 November 2026 (docs/REVIEW-guests.md) in the guests' statement code: the
bridge core (release, claim and gas-drop statements) and the checkpoint core (shared steps, rule (B), wire,
frontier, journal), run against those crates' own host tests only.

Each mutant is one textual replacement in a source file, applied in place, tested with
`cargo test -p <crate> --locked`, and undone in a `finally` (also on SIGTERM); the script then checks that
`git diff` of the touched files is empty. Rule (A)'s own tests (`checkpoint/fidelity`, built against the LEZ and
Logos Blockchain sources) are not run here, so a checkpoint mutant that survives is "not pinned by
checkpoint-core's tests", not "not pinned at all". Nothing here changes a guest: the guests' sources are restored
before the script ends, and their image ids do not depend on tests.

Usage (from the repository root): python3 -I scripts/review-guest-mutants.py [ID ...]
"""

import os
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BC = ("bridge", "ferrata-bridge-core", "bridge/crates/ferrata-bridge-core/src/")
CC = ("checkpoint", "checkpoint-core", "checkpoint/crates/checkpoint-core/src/")
AC = ("checkpoint", "attestor-core", "checkpoint/crates/attestor-core/src/")

# (id, (workspace, package, source dir), file, what, original text, replacement)
MUTANTS = [
    ("G01", BC, "tree.rs", "verify_path: direction bits not compared with the leaf index",
     "    if u64::from(path.directions) != leaf_index {\n",
     "    if u64::from(path.directions) != leaf_index && false {\n"),
    ("G02", BC, "outbound.rs", "release: NOut from the leaf (index-dependent, as v1) instead of sOut alone",
     "    Ok(burn_nullifier(&w.s_out))\n",
     "    Ok(burn_nullifier(&leaf))\n"),
    ("G03", BC, "gasdrop.rs", "gas drop: the journal's relayer is the one-time address",
     "        relayer: input.relayer,\n",
     "        relayer: input.one_time_address,\n"),
    ("G04", BC, "inbound.rs", "claim: N without fid (not bound to chain, vault and index)",
     "    Ok(nullifier(&w.s, &deposit_id(chain_id, vault, w.leaf_index, &leaf)))\n",
     "    Ok(nullifier(&w.s, &leaf))\n"),
    ("G05", BC, "encoding.rs", "release journal: recipient and relayer words swapped",
     "            .put(&address_word(&self.recipient))\n            .put(&address_word(&self.relayer))\n",
     "            .put(&address_word(&self.relayer))\n            .put(&address_word(&self.recipient))\n"),
    ("G06", BC, "outbound.rs", "release: the burn leaf binds index 0 whatever the path says",
     "    let leaf = burn_leaf(&p.config_id, w.burn_index, &p.token, &p.amount, &c_out, &w.r);\n",
     "    let leaf = burn_leaf(&p.config_id, 0, &p.token, &p.amount, &c_out, &w.r);\n"),
    ("G07", CC, "rule_b.rs", "rule (B): a repeated attestation key under two channel keys counts twice",
     "        if channel_keys.contains(&&a.channel_key) || att_keys.contains(&&a.att_key) {\n",
     "        if channel_keys.contains(&&a.channel_key) {\n"),
    ("G08", CC, "rule_b.rs", "rule (B): an attested tip at the right height but another hash is on the path",
     "            .is_some_and(|hash| *hash == a.tip.tip_hash);\n",
     "            .is_some();\n"),
    ("G09", CC, "rule_b.rs", "rule (B): T's producer need not be in K",
     "    let producer = input.producer.check(input.blocks[t_index], &config.channel_id)?;\n    if !state.keys.contains(&producer.signer) {\n",
     "    let producer = input.producer.check(input.blocks[t_index], &config.channel_id)?;\n    if false && !state.keys.contains(&producer.signer) {\n"),
    ("G10", CC, "rule_b.rs", "rule (B): one attestation short of t(n) accepted",
     "    if have < need {\n        return Err(Error::InsufficientAttestations { have, need });\n",
     "    if have + 1 < need {\n        return Err(Error::InsufficientAttestations { have, need });\n"),
    ("G11", CC, "wire.rs", "wire: trailing bytes after a message accepted",
     "        r.finish()?;\n        Ok(Some(msg))\n",
     "        Ok(Some(msg))\n"),
    ("G12", CC, "rule.rs", "rotation: the posting timeframe may change",
     "    if op.posting_timeframe != state.posting_timeframe || op.posting_timeframe == 0 {\n",
     "    if op.posting_timeframe == 0 {\n"),
    ("G13", CC, "rule.rs", "key set: a repeated key accepted",
     "    if keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k)) {\n",
     "    if false {\n"),
    ("G14", CC, "tree.rs", "frontier: a count above the capacity is canonical",
     "        self.count <= MAX_LEAVES && (0..DEPTH)",
     "        (0..DEPTH)"),
    ("G15", CC, "journal.rs", "journal: tipId and burnCount words swapped",
     "            word_u64(self.burn_count),\n            word_u64(self.imported_leaf_count),\n            word_u64(u64::from(self.sunset)),\n            self.echo_nonce,\n            word_u64(self.tip_id),\n",
     "            word_u64(self.tip_id),\n            word_u64(self.imported_leaf_count),\n            word_u64(u64::from(self.sunset)),\n            self.echo_nonce,\n            word_u64(self.burn_count),\n"),
    ("G16", CC, "rule.rs", "segments: a segment of another configuration accepted",
     "            if segments.iter().any(|s| s.lez_config_id != config_id) {\n",
     "            if false {\n"),
    ("G17", CC, "rule.rs", "extraction: an ImportRoot after Sunset still raises the imported count",
     "        BridgeMessage::ImportRoot(m) if !state.sunset => {\n",
     "        BridgeMessage::ImportRoot(m) => {\n"),
    ("G18", AC, "lib.rs", "attestor: certificate and attestation messages do not bind the vault",
     "        out[53..73].copy_from_slice(&self.vault);\n",
     ""),
]

_restore = []


def _undo(*_):
    while _restore:
        path, text = _restore.pop()
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
    sys.exit(3)


def run(mutant):
    mid, (ws, pkg, src), name, what, old, new = mutant
    path = os.path.join(ROOT, src, name)
    with open(path, encoding="utf-8") as f:
        text = f.read()
    if text.count(old) != 1:
        return mid, "BAD TEXT", f"found {text.count(old)} times", 0.0
    _restore.append((path, text))
    start = time.time()
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(text.replace(old, new))
        proc = subprocess.run(
            ["cargo", "test", "-p", pkg, "--locked", "-q"],
            cwd=os.path.join(ROOT, ws), capture_output=True, text=True, timeout=1500,
        )
        out = proc.stdout + proc.stderr
        if "error[E" in out or "could not compile" in out and "test result" not in out:
            status, detail = "compile error", next((l for l in out.splitlines() if l.startswith("error")), "")
        elif proc.returncode != 0:
            failed = [l.strip() for l in out.splitlines() if l.strip().startswith("---- ")]
            status, detail = "killed", (failed[0] if failed else "tests failed")
        else:
            status, detail = "SURVIVED", ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        _restore.pop()
    return mid, status, detail[:140], time.time() - start


def main():
    signal.signal(signal.SIGTERM, _undo)
    wanted = set(sys.argv[1:])
    chosen = [m for m in MUTANTS if not wanted or m[0] in wanted]
    for ws, pkg in sorted({(m[1][0], m[1][1]) for m in chosen}):
        base = subprocess.run(["cargo", "test", "-p", pkg, "--locked", "-q"], cwd=os.path.join(ROOT, ws),
                              capture_output=True, text=True)
        if base.returncode != 0:
            print(f"baseline {pkg} failed:\n{(base.stdout + base.stderr)[-2000:]}", file=sys.stderr)
            return 2
        print(f"baseline {pkg}: pass", flush=True)
    results = []
    for mutant in chosen:
        mid, status, detail, secs = run(mutant)
        results.append((mid, status))
        print(f"{mid} {status:13s} {secs:6.1f}s  {mutant[3]}   {detail}", flush=True)
    files = sorted({os.path.join(m[1][2], m[2]) for m in chosen})
    diff = subprocess.run(["git", "diff", "--stat", "--"] + files, cwd=ROOT, capture_output=True, text=True)
    print("sources restored:", "yes" if not diff.stdout.strip() else "NO\n" + diff.stdout)
    killed = sum(1 for _, s in results if s == "killed")
    print(f"\n{killed}/{len(results)} killed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
