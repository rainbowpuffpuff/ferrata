#!/usr/bin/env python3
"""scripts/privacy/proofs.py RUN_DIR OUT.json: every real proof of a `real.sh` run in one small file (committed;
the raw data is not): per LEZ transaction its proving time, proof and transaction sizes and sha256 and what the
collector checked; per release its succinct proof (time, cycles, receipt size), identity_p254 and Groth16 step
(time, the seal itself, 260 bytes) and the verifications; the GPU container's record. Standard library only."""

import glob
import json
import os
import re
import sys


def load(path, default=None):
    try:
        return json.load(open(path))
    except (OSError, ValueError):
        return default


def last_json_lines(path):
    out = []
    for line in open(path, errors="replace") if os.path.exists(path) else []:
        line = line.strip()
        if line.startswith("{"):
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
    return out


def main():
    g, out_path = sys.argv[1], sys.argv[2]
    run = load(os.path.join(g, "run.json"), {})
    lez = load(os.path.join(g, "data", "lez.json"), {})
    txs = []
    for r in lez.get("records", []):
        m = r.get("meta") or {}
        if r["variant"] == "private" and r["kind"] in ("claim", "burn"):
            txs.append({"id": r["id"], **{k: m.get(k) for k in (
                "kind", "prove_s", "proof_bytes", "tx_bytes", "tx_sha256", "seal_words", "journal_bytes",
                "claim_is_ok_of_journal", "seal_outputs_carry_claim_digest", "claim_is_pruned")}})
    releases = []
    for d in sorted(glob.glob(os.path.join(g, "releases", "release-*/"))):
        d = d.rstrip("/")
        name = os.path.basename(d)
        prove = load(os.path.join(d, "prove.json"), {})
        p254 = load(os.path.join(d, "p254", "p254.json"), {})
        g16 = load(os.path.join(d, "groth16", "groth16.json"), {})
        wrap = ""
        if os.path.exists(d + ".wrap.log"):
            hits = [l for l in open(d + ".wrap.log", errors="replace") if l.startswith("[wrap]")]
            wrap = hits[-1].strip() if hits else ""
        m = re.search(r"exit (\d+), (\d+) s, (\d+) CPU-s, about ([0-9.]+) USD", wrap)
        seal = os.path.join(d, "groth16", "seal.hex")
        releases.append({
            "name": name,
            "succinct": {k: prove.get(k) for k in ("wall_s", "segments", "user_cycles", "total_cycles", "receipt_bytes",
                                                    "receipt_sha256", "image_id")},
            "p254_s": p254.get("wall_s"),
            "groth16_prover": {"exit": int(m.group(1)), "seconds": int(m.group(2)), "cpu_s": int(m.group(3)),
                               "usd_estimate": float(m.group(4))} if m else None,
            "groth16_receipt_bytes": g16.get("receipt_bytes"), "selector": g16.get("selector"),
            "seal": open(seal).read().strip() if os.path.exists(seal) else None,
            "verified": [v.get("kind") for v in last_json_lines(d + ".verify.json") if v.get("verifies")],
        })
    keep = {k: run.get(k) for k in ("run", "ok", "gpu", "nproc", "r0vm_version", "r0vm_sha256", "r0vm_equal_manifest",
                                     "release_image_id", "release_id_equal_expected", "staged_hashes_equal_manifest",
                                     "lez_half", "gpu_stats", "container_s", "started_at_utc", "finished_at_utc")}
    keep["manifest"] = {k: (run.get("manifest") or {}).get(k) for k in ("commit", "files", "expected", "r0vm_cuda_sha256", "glibc")}
    out = {"gpu_container": keep, "bridge_image_id": lez.get("bridge_image_id"),
           "circuit_image_id": lez.get("circuit_image_id"), "proofs": lez.get("proofs"),
           "lez_private_transactions": txs, "releases": releases}
    with open(out_path, "w") as f:
        json.dump(out, f, indent=1)
    print(json.dumps({"lez_private_transactions": len(txs), "releases": len(releases),
                      "releases_with_seal": sum(1 for r in releases if r["seal"]),
                      "releases_verified_both": sum(1 for r in releases if sorted(r["verified"]) == ["groth16", "succinct"])}))


if __name__ == "__main__":
    main()
