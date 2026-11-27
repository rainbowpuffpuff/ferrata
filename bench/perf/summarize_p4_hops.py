#!/usr/bin/env python3
"""RFP-021 P.4, hops inside the user's proof: a table from what lez/results/p4-minimum/minimum.py wrote for its jobs
claim-hopsK-pad (fetched to RESULTS/<tag>/<job>/: container.json, run-N/run.json, run-N/prove.log or prove.log.gz),
parsed with that
pass's own helpers (lez/results/p4-minimum/summarize.py, summarize_scale.py). Usage:
  python3 -I bench/perf/summarize_p4_hops.py bench/perf/results/p4-hops > lez/results/p4-hops/table.md
  python3 -I bench/perf/summarize_p4_hops.py --test       # the parser on built-in lines"""

import gzip
import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "lez/results/p4-minimum"))
from summarize import parse_log  # noqa: E402
from summarize_scale import machine, short  # noqa: E402

GIB = 1 << 30
# prove_vault_claim's line (harness/src/bin/prove_vault_claim.rs), with grouped numbers
CLAIM = re.compile(r"^REAL PROOF, receipt verified at settlement: execute_and_prove ([\d.]+) s \(prove \+ settle ([\d.]+) s\); "
                   r"receipt ([\d,]+) bytes; transaction ([\d,]+) bytes$")
# prove_vault_claim's first line: what the binary was asked to prove
HEADER = re.compile(r"; hops (\d+); shape (\w+)$")
JOB = re.compile(r"^claim-hops(\d+)-pad$")


def normalize(log: str) -> str:
    """prove_vault_claim's proof line in the form summarize.parse_log reads (prove_attest's)."""
    out = []
    for line in log.splitlines():
        m = CLAIM.match(line.strip())
        if m:
            line = (f"private claim: REAL PROOF: execute_and_prove {m.group(1)} s (prove + settle {m.group(2)} s); receipt "
                    f"(succinct InnerReceipt, borsh) {m.group(3).replace(',', '')} bytes; transaction "
                    f"{m.group(4).replace(',', '')} bytes")
        out.append(line)
    return "\n".join(out)


def runs(root: pathlib.Path) -> list:
    out = []
    for d in sorted(root.glob("*/claim-hops*-pad")):
        k = JOB.match(d.name)
        if not k or not (d / "container.json").exists():
            continue
        c = json.load(open(d / "container.json"))
        cpus = c.get("declared", {}).get("modal_cpu_request_and_limit")
        mem = c.get("declared", {}).get("modal_memory_mib_request_and_limit")
        for rd in sorted(d.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
            r = json.load(open(rd / "run.json"))
            log = rd / "prove.log"
            text = log.read_text(errors="replace") if log.exists() else \
                gzip.decompress((rd / "prove.log.gz").read_bytes()).decode(errors="replace")
            proofs = parse_log(normalize(text), "private claim")["proofs"]
            first = HEADER.search(text.splitlines()[0]) if text else None
            asked = (int(first.group(1)), first.group(2)) if first else None
            out.append({"hops": int(k.group(1)), "tag": d.parent.name, "cpus": cpus, "mem_mib": mem,
                        "machine": short(machine(c)), "run": r, "proofs": proofs, "asked": asked})
    return out


def table(rows: list) -> str:
    lines = ["| hops | container | `execute_and_prove` (s) | the binary's wall (s) | user cycles proven (segments) | "
             "peak GiB: r0vm RSS / container | receipt (B) | transaction (B) | settled, receipt verified | "
             "one flipped bit: settlement's exit |",
             "|---:|---|---:|---:|---:|---:|---:|---:|---|---|"]
    for x in sorted(rows, key=lambda x: (x["hops"], -(x["cpus"] or 0), x["tag"])):
        r = x["run"]
        for p in x["proofs"] or [None]:
            rss = int(r["gnu_time"].get("max_rss_kib", 0)) * 1024 / GIB
            cg = r["sampled"]["max"].get("cgroup_current_bytes", 0) / GIB
            settled = "yes" if r.get("settle_exit") == 0 and r.get("exit_code") == 0 else \
                f"no (prove exit {r.get('exit_code')}, settle exit {r.get('settle_exit')})"
            if x["asked"] != (x["hops"], "wallet"):
                settled += f"; the log asked {x['asked']}, not ({x['hops']}, 'wallet')"
            tamper = ", ".join(str(t["settle_exit"]) for t in r.get("tamper", [])) or "not run"
            if p is None:
                lines.append(f"| {x['hops']} | {x['cpus']:g} vCPU, {x['mem_mib'] // 1024} GiB ({x['machine']}) | no proof "
                             f"(exit {r['exit_code']}) | {r['wall_s']:.1f} | | {rss:.2f} / {cg:.2f} | | | {settled} | {tamper} |")
                continue
            lines.append(f"| {x['hops']} | {x['cpus']:g} vCPU, {x['mem_mib'] // 1024} GiB ({x['machine']}) | "
                         f"{p['execute_and_prove_s']:.1f} | {r['wall_s']:.1f} | {p['user_cycles']:,} ({p['segments']}) | "
                         f"{rss:.2f} / {cg:.2f} | {p['receipt_bytes']:,} | {p['tx_bytes']:,} | {settled} | {tamper} |")
    return "\n".join(lines)


def test() -> None:
    line = ("REAL PROOF, receipt verified at settlement: execute_and_prove 1234.5 s (prove + settle 1235.0 s); "
            "receipt 239,246 bytes; transaction 255,366 bytes")
    sessions = "\n".join([
        "2026-11-27T15:00:00.000000Z  INFO risc0_zkvm::host::server::exec::executor: execution time: 1s",
        "2026-11-27T15:00:00.100000Z  INFO risc0_zkvm::host::server::session: number of segments: 7",
        "2026-11-27T15:00:00.100001Z  INFO risc0_zkvm::host::server::session: 7340032 total cycles",
        "2026-11-27T15:00:00.100002Z  INFO risc0_zkvm::host::server::session: 5872696 user cycles (80.00%)",
        "2026-11-27T15:10:00.000000Z  INFO risc0_zkvm::host::server::exec::executor: execution time: 2ms",
    ])
    p = parse_log(normalize(sessions + "\n" + line), "private claim")["proofs"]
    assert len(p) == 1, p
    assert (p[0]["label"], p[0]["execute_and_prove_s"], p[0]["receipt_bytes"], p[0]["tx_bytes"]) == \
        ("private claim", 1234.5, 239246, 255366), p[0]
    assert (p[0]["user_cycles"], p[0]["segments"]) == (5872696, 7), p[0]
    assert normalize("REAL PROOF: other") == "REAL PROOF: other"
    print("summarize_p4_hops: parser test ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--test"]:
        test()
    else:
        print(table(runs(pathlib.Path(sys.argv[1]))))
