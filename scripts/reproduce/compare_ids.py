#!/usr/bin/env python3
"""Print the rebuilt image ids beside e2e/crown/README.md's table and the deployed vault's immutables.

    python3 -I scripts/reproduce/compare_ids.py --out DIR [--readme e2e/crown/README.md] [--table docs/image-ids.md]
                                                [--vault ADDR] [--rpc URL]

The crown run's six programs are compared with e2e/crown/README.md's table (and, with --vault, a crown vault's
immutables); every guest of DIR/ids.tsv with docs/image-ids.md. DIR is the output of `scripts/reproduce-image-ids.sh build` (ids.txt: the LEZ programs' ids from r0vm; r0bf/ids.jsonl:
the four R0BF programs' ids from ferrata-e2e program-binary; methods-ids.txt: the ids risc0-build computed). Standard
library only; the vault is read with eth_call over public JSON-RPC (read-only). Exit 1 if any id differs.
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
    onchain, mode = {}, None
    try:
        if not a.vault:
            raise vs.RpcError("no --vault given")
        rpc = vs.Rpc(a.rpc)
        onchain["checkpoint_b"] = vs.words(rpc.call(a.vault, "checkpointImageId()"))[0]
        onchain["release"] = vs.words(rpc.call(a.vault, "releaseImageId()"))[0]
        mode = int(vs.words(rpc.call(a.vault, "mode()"))[0], 16)
    except vs.RpcError as e:
        print(f"vault not readable over {a.rpc}: {e}")
        mode = None
    print(f"vault {a.vault} (mode {'B' if mode == 1 else mode}): checkpointImageId is checkpoint_b's id, releaseImageId release's")
    bad = 0
    for k in ORDER:
        r, t, v, mth = rebuilt.get(k), table.get(k), onchain.get(k), methods.get(k)
        ok = r is not None and r == t and (v is None or v == r) and (mth is None or mth == r)
        bad += not ok
        print(f"{'MATCH' if ok else 'DIFFER':<7}{k:<13} rebuilt {r}  README {'= rebuilt' if t == r else t}  "
              f"vault {'-' if v is None else ('= rebuilt' if v == r else v)}"
              + (f"  risc0-build {'= rebuilt' if mth == r else mth}" if mth else "") + f"  (sha256 {(sha.get(k) or '?')[:16]}…)")
    print(f"{len(ORDER) - bad} of {len(ORDER)} ids of the crown run's programs equal the recorded ones")
    tsv = os.path.join(a.out, "ids.tsv")
    if os.path.isfile(tsv) and os.path.isfile(a.table):
        doc = dict(re.findall(r"`([a-z0-9_]+)` \| `(0x[0-9a-f]{64})`", open(a.table, encoding="utf-8").read()))
        rows = [l.rstrip("\n").split("\t") for l in open(tsv, encoding="utf-8")][1:]
        same = 0
        for r in rows:
            ok = doc.get(r[1]) == r[2]
            same += ok
            bad += not ok
            print(f"{'MATCH' if ok else 'DIFFER':<7}{r[1]:<32} rebuilt {r[2]}  docs/image-ids.md {'= rebuilt' if ok else doc.get(r[1])}")
        print(f"{same} of {len(rows)} guests equal docs/image-ids.md")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
