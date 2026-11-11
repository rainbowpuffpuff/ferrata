#!/usr/bin/env python3
"""Tables of the p4-minimum runs, from the files `minimum.py` wrote (container.json, run-<i>/run.json,
run-<i>/prove.log). usage: python3 -I summarize.py [RESULTS_DIR]   (default: runs/ beside this file)

Which r0vm sessions were proven: r0vm logs every session it executes (RISC0_INFO=1, RUST_LOG=info),
the dev-mode executions the binaries make for their tables and checks as well as the proven ones.
A session counts as proven when the next timestamped log line comes more than 3 s later (proving
it is what fills the gap: executions take milliseconds). The gap is that session's proving time,
recursion included. Sessions are assigned to the proof whose `REAL PROOF` line follows them.
"""

import datetime
import json
import pathlib
import re
import statistics
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
TS = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+)Z\s+INFO\s+(\S+):\s+(.*)$")
PROOF = re.compile(r"^(?:(private claim|private burn): )?REAL PROOF: execute_and_prove ([\d.]+) s \(prove \+ settle ([\d.]+) s\); "
                   r"receipt \(succinct InnerReceipt, borsh\) (\d+) bytes; transaction (\d+) bytes")
TABLE_ROW = re.compile(r"^\| (AttestPrivately \(sends Read\)|the record's reply \(verifies the fact\)) \|.*?\| ([\d,]+) \| (\d+) \| ([\d,]+) \| (\d+) \|$")


def ts(s: str) -> float:
    return datetime.datetime.fromisoformat(s).replace(tzinfo=datetime.timezone.utc).timestamp()


def parse_log(text: str) -> dict:
    lines = [ANSI.sub("", l) for l in text.splitlines()]
    events = []  # ("session", t, dict) | ("line", t or None, text)
    cur = None
    for l in lines:
        m = TS.match(l)
        if m:
            t, mod, msg = ts(m.group(1)), m.group(2), m.group(3)
            if mod.endswith("executor") and msg.startswith("execution time"):
                cur = {"t": t}
            elif mod.endswith("session") and cur is not None:
                if msg.startswith("number of segments"):
                    cur["segments"] = int(msg.split(": ")[1])
                elif msg.endswith("total cycles"):
                    cur["total_cycles"] = int(msg.split()[0])
                elif "user cycles" in msg:
                    cur["user_cycles"] = int(msg.split()[0])
                    events.append(("session", cur["t"], cur))
                    cur = None
            if not mod.endswith("session"):  # the next session's start, or anything else r0vm logs
                events.append(("ts", t, None))
        elif PROOF.match(l.strip()):
            events.append(("proof", None, PROOF.match(l.strip())))
    # gap after each session = time to the next timestamped line
    stamps = [e[1] for e in events if e[0] == "ts"]
    proofs, pending = [], []
    for e in events:
        if e[0] == "session":
            s = e[2]
            later = [t for t in stamps if t > s["t"] + 1e-9]
            s["gap_s"] = round(later[0] - s["t"], 1) if later else None
            if s["gap_s"] and s["gap_s"] > 3:
                pending.append(s)
        elif e[0] == "proof":
            m = e[2]
            proofs.append({
                "label": m.group(1) or "private attestation", "execute_and_prove_s": float(m.group(2)),
                "prove_and_settle_s": float(m.group(3)), "receipt_bytes": int(m.group(4)), "tx_bytes": int(m.group(5)),
                "proven_sessions": [{k: s[k] for k in ("segments", "total_cycles", "user_cycles", "gap_s")} for s in pending],
            })
            pending = []
    for p in proofs:
        ss = p["proven_sessions"]
        p["user_cycles"] = sum(s["user_cycles"] for s in ss)
        p["segments"] = sum(s["segments"] for s in ss)
        p["total_cycles"] = sum(s["total_cycles"] for s in ss)
    consumer = [(m.group(1), int(m.group(2).replace(",", "")), int(m.group(3))) for m in map(TABLE_ROW.match, lines) if m]
    return {"proofs": proofs, "consumer_table": consumer}


def main() -> None:
    here = pathlib.Path(__file__).resolve().parent
    root = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else here / "runs"
    out = {}
    for job_dir in sorted(p for p in root.iterdir() if p.is_dir()):
        c = json.load(open(job_dir / "container.json"))
        runs = []
        for rd in sorted(job_dir.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
            r = json.load(open(rd / "run.json"))
            parsed = parse_log(open(rd / "prove.log", errors="replace").read())
            runs.append({"run": r["run"], "exit_code": r["exit_code"], "wall_s": r["wall_s"], "gnu_time": r["gnu_time"],
                         "sampled_max": r["sampled"]["max"], "cpu_s": r["sampled"].get("cpu_s_by_process_name"),
                         "settle_exit": r["settle_exit"], "settle_lines": r["settle_lines"], "tamper": r["tamper"],
                         "txs": r["txs"], **parsed})
        out[job_dir.name] = {"container": {k: c.get(k) for k in (
            "declared", "nproc", "sched_getaffinity", "cpuinfo_processors", "cpuinfo_model_names", "cpuinfo_ids",
            "cpu_flags", "cgroup", "meminfo_kib", "kernel", "os", "glibc", "env", "r0vm_version", "r0vm_sha256",
            "input_hashes_equal_manifest", "image_ids")}, "manifest_commit": c["manifest"]["ferrata_commit"], "runs": runs}
    print(json.dumps(out, indent=1))
    # the table
    gib = 1 << 30
    rows = []
    for job, d in out.items():
        labels = [p["label"] for p in d["runs"][0]["proofs"]] if d["runs"] else []
        for k, label in enumerate(labels):
            ps = [r["proofs"][k] for r in d["runs"] if len(r["proofs"]) > k]
            t = [p["execute_and_prove_s"] for p in ps]
            rss = [int(r["gnu_time"].get("max_rss_kib", 0)) * 1024 for r in d["runs"]]
            cg = [r["sampled_max"]["cgroup_current_bytes"] for r in d["runs"]]
            ok = all(r["settle_exit"] == 0 for r in d["runs"])
            p0 = ps[0]
            rows.append(
                f"| {label} | {len(ps)} | {' / '.join(f'{x:.1f}' for x in t)} | {statistics.median(t):.1f} | "
                f"{p0['user_cycles']:,} | {p0['segments']} ({p0['total_cycles']:,}) | "
                f"{max(rss) / gib:.2f} / {max(cg) / gib:.2f} | {p0['receipt_bytes']:,} | {p0['tx_bytes']:,} | "
                f"{'yes, ' + str(len(d['runs'])) + ' of ' + str(len(d['runs'])) if ok else 'NO'} |")
    print("\n| proof | runs | `execute_and_prove`, each run (s) | median (s) | user cycles proven | segments (cycles with padding) "
          "| peak memory, GiB: r0vm RSS / container (whole run, max of the runs) | receipt (B) | transaction (B) | receipt verified |")
    print("|---|---:|---|---:|---:|---:|---:|---:|---:|---|")
    print("\n".join(rows))


if __name__ == "__main__":
    main()
