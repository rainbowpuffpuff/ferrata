#!/usr/bin/env python3
"""Tables of the p4-macos runs, from the files pipeline.sh and release.sh wrote on each Mac (runs/<mac>/).
usage: python3 -I summarize.py [RUNS_DIR [JSON_OUT]]   (defaults: runs/ and summary.json beside this file)

The r0vm session log is read with p4-minimum's parser (../p4-minimum/summarize.py, `parse_log`): a session counts
as proven when the next timestamped line comes more than 3 s later, and that gap is its proving time.
`/usr/bin/time -l` is macOS's: wall, user and sys seconds, the maximum resident set size in bytes and the peak
memory footprint in bytes (both of the host binary and the children it waited for, r0vm). mem.csv is a 2-s sampler
of r0vm's RSS and of vm_stat's page counts (16 KiB pages).
"""

import json
import pathlib
import re
import statistics
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "p4-minimum"))
from summarize import parse_log  # noqa: E402  (p4-minimum's parser, unchanged)

TIME_L = {
    "real_s": re.compile(r"^\s*([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys"),
    "max_rss_bytes": re.compile(r"^\s*(\d+)\s+maximum resident set size"),
    "peak_footprint_bytes": re.compile(r"^\s*(\d+)\s+peak memory footprint"),
}
GPU = re.compile(r"GPU HW active residency:\s+([\d.]+)%")
CLUSTER = re.compile(r"^(E\d?-Cluster|P\d?-Cluster) HW active residency:\s+([\d.]+)%")
GIB = 1024 ** 3


def time_l(text: str) -> dict:
    out = {}
    for line in text.splitlines():
        m = TIME_L["real_s"].match(line)
        if m:
            out.update(real_s=float(m.group(1)), user_s=float(m.group(2)), sys_s=float(m.group(3)))
        for k in ("max_rss_bytes", "peak_footprint_bytes"):
            m = TIME_L[k].match(line)
            if m:
                out[k] = int(m.group(1))
    return out


def mem_csv(path: pathlib.Path) -> dict:
    if not path.exists():
        return {}
    rows = [r.split(",") for r in path.read_text().splitlines()[1:] if r.count(",") >= 3]
    if not rows:
        return {}
    r0 = [int(r[1]) for r in rows]
    out = {"samples": len(rows), "r0vm_rss_max_gib": round(max(r0) * 1024 / GIB, 2)}
    if len(rows[0]) >= 10:
        free = [int(r[4]) for r in rows if r[4]]
        comp = [int(r[8]) for r in rows if r[8]]
        swp = [int(r[9]) for r in rows if r[9]]
        out.update(free_min_gib=round(min(free) * 16384 / GIB, 2), compressor_max_gib=round(max(comp) * 16384 / GIB, 2),
                   swapouts_during=(max(swp) - min(swp)) if swp else None)
    return out


def powermetrics(path: pathlib.Path) -> dict:
    if not path.exists():
        return {}
    text = path.read_text(errors="replace")
    gpu = [float(x) for x in GPU.findall(text)]
    clusters = {}
    for line in text.splitlines():
        m = CLUSTER.match(line.strip())
        if m:
            clusters.setdefault(m.group(1), []).append(float(m.group(2)))
    return {"gpu_active_pct": gpu, "cluster_active_pct": clusters, "bytes": len(text)}


def result_txt(path: pathlib.Path) -> dict:
    out = {"tamper": []}
    if not path.exists():
        return out
    for line in path.read_text().splitlines():
        if m := re.match(r"prove exit (\d+) wall_s (\d+)", line):
            out["prove_exit"], out["wall_s"] = int(m.group(1)), int(m.group(2))
        elif m := re.match(r"settle exit (\d+)", line):
            out["settle_exit"] = int(m.group(1))
        elif m := re.match(r"tampered (\S+) \(bit 0 of byte (\d+)\) settle exit (\d+)", line):
            out["tamper"].append({"tx": m.group(1), "byte": int(m.group(2)), "settle_exit": int(m.group(3))})
    return out


def settle_lines(path: pathlib.Path) -> list:
    if not path.exists():
        return []
    return [l.strip() for l in path.read_text().splitlines() if "SETTLED FROM" in l or "settled:" in l]


def main() -> None:
    runs = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "runs"
    out_path = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else HERE / "summary.json"
    summary = {}
    for mac in sorted(p for p in runs.iterdir() if p.is_dir()):
        rec = {"proofs": [], "release": []}
        for rd in sorted(mac.glob("runs/run-*/*")):
            run, job = int(rd.parent.name.split("-")[1]), rd.name
            log = (rd / "prove.log").read_text(errors="replace") if (rd / "prove.log").exists() else ""
            parsed = parse_log(log, "private attestation")
            base = {"run": run, "job": job, **result_txt(rd / "result.txt"), "time_l": time_l(log),
                    "mem": mem_csv(rd / "mem.csv"), "powermetrics": powermetrics(rd / "powermetrics.txt"),
                    "settle": settle_lines(rd / "settle.log")}
            for p in parsed["proofs"]:
                rec["proofs"].append({**base, **p})
            if not parsed["proofs"]:
                rec["proofs"].append({**base, "label": f"{job} (no REAL PROOF line)"})
        for rd in sorted(mac.glob("release/run-*")):
            log = (rd / "prove.log").read_text(errors="replace") if (rd / "prove.log").exists() else ""
            r = result_txt(rd / "result.txt")
            v = (rd / "result.txt").read_text() if (rd / "result.txt").exists() else ""
            rec["release"].append({"run": int(rd.name.split("-")[1]), **r, "time_l": time_l(log),
                                   "mem": mem_csv(rd / "mem.csv"),
                                   "verify_exit": int(m.group(1)) if (m := re.search(r"verify exit (\d+)", v)) else None})
        summary[mac.name] = rec
    out_path.write_text(json.dumps(summary, indent=1) + "\n")

    print("| Mac | proof | run | `execute_and_prove` (s) | user cycles proven (segments) | LEZ's circuit: user cycles (segments) "
          "| r0vm RSS peak (GiB, sampled) | `time -l`: max RSS / peak footprint (GiB) | receipt / transaction (B) | settled | flipped bit refused |")
    print("|---|---|---:|---:|---:|---:|---:|---:|---:|---|---|")
    for mac, rec in summary.items():
        for p in rec["proofs"]:
            ss = p.get("proven_sessions", [])
            circ = ss[-1] if ss else {}
            tl = p["time_l"]
            label = p.get("label", "?")
            tx = {"private attestation": "tx-attest.borsh", "private claim": "tx-claim.borsh", "private burn": "tx-burn.borsh"}.get(label)
            settled = [l for l in p["settle"] if tx and tx in l]
            if label != "private attestation" and tx:  # prove_bridge's settle log names each transaction
                settled = [l for l in p["settle"] if label in l and "SETTLED" in l] or settled
            tam = [t for t in p["tamper"] if t["tx"] == tx]
            print(f"| {mac} | {label} | {p['run']} | {p.get('execute_and_prove_s', '—')} | "
                  f"{p.get('user_cycles', 0):,} ({p.get('segments', 0)}) | {circ.get('user_cycles', 0):,} ({circ.get('segments', 0)}) | "
                  f"{p['mem'].get('r0vm_rss_max_gib', '—')} | "
                  f"{round(tl.get('max_rss_bytes', 0) / GIB, 2)} / {round(tl.get('peak_footprint_bytes', 0) / GIB, 2)} | "
                  f"{p.get('receipt_bytes', '—')} / {p.get('tx_bytes', '—')} | "
                  f"{'yes' if p.get('settle_exit') == 0 and settled else ('exit ' + str(p.get('settle_exit')))} | "
                  f"{', '.join('exit ' + str(t['settle_exit']) for t in tam) or '—'} |")
    print()
    for mac, rec in summary.items():
        for p in rec["proofs"]:
            ss = p.get("proven_sessions", [])
            print(f"{mac} run {p['run']} {p.get('label')}: sessions "
                  + "; ".join(f"{s['user_cycles']:,} user cycles, {s['segments']} seg ({s['total_cycles']:,}), {s['gap_s']} s" for s in ss)
                  + f"; GPU active % {p['powermetrics'].get('gpu_active_pct')}; mem {p['mem']}")
        for r in rec["release"]:
            print(f"{mac} release run {r['run']}: prove exit {r.get('prove_exit')} wall {r.get('wall_s')} s; time -l {r['time_l']}; "
                  f"mem {r['mem']}; verify exit {r.get('verify_exit')}")


if __name__ == "__main__":
    main()
