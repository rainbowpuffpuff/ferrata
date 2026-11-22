#!/usr/bin/env python3
"""Print the rebuilt image ids beside e2e/crown/README.md's table and the deployed vault's immutables.

    python3 -I scripts/reproduce/compare_ids.py --out DIR [--readme e2e/crown/README.md] [--table docs/image-ids.md]
                                                [--vault ADDR] [--rpc URL]

The crown run's six programs are compared with e2e/crown/README.md's table (and, with --vault, a crown vault's
immutables); every guest of DIR/ids.tsv with docs/image-ids.md. DIR is the output of `scripts/reproduce-image-ids.sh build` (ids.txt: the LEZ programs' ids from r0vm; r0bf/ids.jsonl:
the four R0BF programs' ids from ferrata-e2e program-binary; methods-ids.txt: the ids risc0-build computed). Standard
library only; the vault is read with eth_call over public JSON-RPC (read-only). Exit 1 if any id differs.

Fails closed: with --vault, a vault that cannot be read (an RPC error, no endpoint answering, empty return data, an
unknown mode) counts as a difference, and so does an immutable that differs (checkpointImageId is the rule's program:
checkpoint in mode A, checkpoint_b in mode B; releaseImageId is release's). A row of DIR/ids.tsv counts only if
r0vm computed the same id from the binary ("= id"), and every guest of docs/image-ids.md's first table (the guests
`scripts/reproduce-image-ids.sh build` builds) must be in DIR/ids.tsv.
"""
import argparse
import importlib.util
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("verify_sepolia", os.path.join(HERE, "..", "verify-sepolia.py"))
vs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vs)

ORDER = ["attestation", "bridge", "checkpoint_b", "checkpoint", "segment", "release"]
FILES = {"ferrata_lez.bin": "attestation", "ferrata_bridge.bin": "bridge", "checkpoint_b.bin": "checkpoint_b",
         "checkpoint.bin": "checkpoint", "segment.bin": "segment", "release.bin": "release"}
METHODS = {"CHECKPOINT_B_ID": "checkpoint_b", "CHECKPOINT_ID": "checkpoint", "SEGMENT_ID": "segment",
           "RELEASE_ID": "release"}


def read_vault(rpc, vault: str):
    """The vault's immutables: ({key: id}, the keys the vault pins, mode). ValueError when the answer is not a vault's."""
    ck, rl, md = (vs.words(rpc.call(vault, sig)) for sig in ("checkpointImageId()", "releaseImageId()", "mode()"))
    if not (ck and rl and md):
        raise ValueError("empty return data (no contract at this address, or not a FerrataVault)")
    mode = int(md[0], 16)
    if mode not in (0, 1):
        raise ValueError(f"mode() is {mode}, neither A (0) nor B (1)")
    key = "checkpoint" if mode == 0 else "checkpoint_b"
    return {key: ck[0], "release": rl[0]}, {key, "release"}, mode


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--readme", default=os.path.join(HERE, "..", "..", "e2e", "crown", "README.md"))
    ap.add_argument("--table", default=os.path.join(HERE, "..", "..", "docs", "image-ids.md"))
    ap.add_argument("--vault", default="")
    ap.add_argument("--rpc", default=os.environ.get("SEPOLIA_RPC_URL") or vs.DEFAULT_RPC)
    a = ap.parse_args()
    rebuilt, sha = {}, {}
    with open(os.path.join(a.out, "ids.txt"), encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if len(parts) >= 3 and parts[0] in FILES:
                idv = parts[2] if parts[2].startswith("0x") else "0x" + parts[2]
                rebuilt[FILES[parts[0]]], sha[FILES[parts[0]]] = idv.lower(), parts[1]
    with open(os.path.join(a.out, "r0bf", "ids.jsonl"), encoding="utf-8") as f:
        for line in f:
            if line.strip():
                row = json.loads(line)
                k = FILES.get(row["program"])
                if k:
                    rebuilt[k], sha[k] = row["image_id"].lower(), row["sha256"]
    methods = {}
    mpath = os.path.join(a.out, "methods-ids.txt")
    if os.path.isfile(mpath):
        for m in re.finditer(r"pub const ([A-Z0-9_]+): \[u32; 8\] = \[([^\]]+)\]", open(mpath, encoding="utf-8").read()):
            if m.group(1) in METHODS:
                words = [int(x.strip()) for x in m.group(2).split(",") if x.strip()]
                methods[METHODS[m.group(1)]] = "0x" + b"".join(w.to_bytes(4, "little") for w in words).hex()
    table = vs.readme_ids(a.readme) if os.path.isfile(a.readme) else vs.image_ids_md(a.table)
    onchain, pinned, bad, crown_same = {}, set(), 0, 0
    if not a.vault:
        print("vault: not checked (no --vault given)")
    else:
        try:
            onchain, pinned, mode = read_vault(vs.Rpc(a.rpc), a.vault)
            print(f"vault {a.vault} (mode {'AB'[mode]}): checkpointImageId is {'checkpoint' if mode == 0 else 'checkpoint_b'}'s id, "
                  "releaseImageId release's")
        except (vs.RpcError, vs.RpcUnavailable, ValueError) as e:
            # The vault was asked for and not read: that is a failed comparison, never a pass.
            print(f"DIFFER vault {a.vault} not read over {a.rpc}: {e}")
            pinned, bad = {"checkpoint", "checkpoint_b", "release"}, 1
    for k in ORDER:
        r, t, v, mth = rebuilt.get(k), table.get(k), onchain.get(k), methods.get(k)
        vault_ok = k not in pinned or (v is not None and v == r)
        ok = r is not None and r == t and vault_ok and (mth is None or mth == r)
        bad += not ok
        crown_same += ok
        print(f"{'MATCH' if ok else 'DIFFER':<7}{k:<13} rebuilt {r}  README {'= rebuilt' if t == r else t}  "
              f"vault {('NOT READ' if k in pinned else '-') if v is None else ('= rebuilt' if v == r else v)}"
              + (f"  risc0-build {'= rebuilt' if mth == r else mth}" if mth else "") + f"  (sha256 {(sha.get(k) or '?')[:16]}…)")
    print(f"{crown_same} of {len(ORDER)} ids of the crown run's programs equal the recorded ones"
          + ("" if not a.vault else " and the vault's immutables" if onchain else "; the vault was NOT read"))
    tsv = os.path.join(a.out, "ids.tsv")
    if not (os.path.isfile(tsv) and os.path.isfile(a.table)):
        bad += 1
        print(f"DIFFER guests not compared: {tsv if not os.path.isfile(tsv) else a.table} missing")
    else:
        text = open(a.table, encoding="utf-8").read()
        doc = dict(re.findall(r"`([a-z0-9_]+)` \| `(0x[0-9a-f]{64})`", text))
        first = re.split(r"\n## ", text, maxsplit=1)[0]
        expected = [n for n, _ in re.findall(r"`([a-z0-9_]+)` \| `(0x[0-9a-f]{64})`", first)]
        rows = [l.rstrip("\n").split("\t") for l in open(tsv, encoding="utf-8")][1:]
        same = 0
        for r in rows:
            r += [""] * (7 - len(r))
            ok = doc.get(r[1]) == r[2] and r[6] == "= id"
            same += ok
            bad += not ok
            r0 = "" if r[6] == "= id" else f"  r0vm {r[6] or '?'} (must be '= id')"
            print(f"{'MATCH' if ok else 'DIFFER':<7}{r[1]:<32} rebuilt {r[2]}  docs/image-ids.md "
                  f"{'= rebuilt' if doc.get(r[1]) == r[2] else doc.get(r[1])}{r0}")
        built = {r[1] for r in rows}
        for n in expected:
            if n not in built:
                bad += 1
                print(f"MISSING {n:<32} in docs/image-ids.md's first table, not in {tsv}")
        print(f"{same} of {len(rows)} guests equal docs/image-ids.md; "
              f"{sum(n not in built for n in expected)} of its first table's {len(expected)} missing")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
