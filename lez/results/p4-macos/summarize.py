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
            pj = json.loads((rd / "prove" / "prove.json").read_text()) if (rd / "prove" / "prove.json").exists() else {}
            rec["release"].append({"run": int(rd.name.split("-")[1]), **r, "time_l": time_l(log), "prove_json": pj,
                                   "mem": mem_csv(rd / "mem.csv"),
                                   "verify_exit": int(m.group(1)) if (m := re.search(r"verify exit (\d+)", v)) else None})
        summary[mac.name] = rec
    out_path.write_text(json.dumps(summary, indent=1) + "\n")

    tables(summary)


MACS = {"m1": "M1 (8 cores, 16 GB)", "m4": "M4 (10 cores, 24 GB)"}
# p4-minimum's padded pass, medians of 2 (`execute_and_prove`, s) at 8 and 16 cloud CPUs, 16 GiB; release: second pass
CLOUD = {"private attestation": (1236.8, 633.3), "private claim": (1146.0, 600.7), "private burn": (988.9, 525.9)}
CLOUD_RELEASE = (46.0, 26.7)
TX = {"private attestation": "tx-attest.borsh", "private claim": "tx-claim.borsh", "private burn": "tx-burn.borsh"}


def med(xs):
    return round(statistics.median(xs), 1) if xs else None


def tables(summary: dict) -> None:
    labels = ["private attestation", "private claim", "private burn"]
    by = {(mac, l): [p for p in rec["proofs"] if p.get("label") == l and p.get("execute_and_prove_s")]
          for mac, rec in summary.items() for l in labels}
    print("| proof | " + " | ".join(f"{MACS.get(m, m)}: each run (s) | median" for m in summary)
          + " | cloud, 8 CPUs: median | cloud, 16 CPUs: median | " + " | ".join(f"{m.upper()} / 16 CPUs" for m in summary)
          + " | user cycles proven (segments) | LEZ's circuit: user cycles (segments) | settled, receipt verified |")
    print("|---|" + "---|---:|" * len(summary) + "---:|---:|" + "---:|" * len(summary) + "---:|---:|---|")
    for l in labels:
        row, ratios, cyc, circ, settled, total = [], [], [], [], 0, 0
        for mac in summary:
            ps = sorted(by[(mac, l)], key=lambda p: p["run"])
            xs = [p["execute_and_prove_s"] for p in ps]
            row += [" / ".join(f"{x:.1f}" for x in xs) or "—", f"{med(xs)}" if xs else "—"]
            ratios.append(f"{med(xs) / CLOUD[l][1]:.2f}×" if xs else "—")
            cyc += [p["user_cycles"] for p in ps]
            circ += [(p["proven_sessions"][-1]["user_cycles"], p["proven_sessions"][-1]["segments"]) for p in ps if p.get("proven_sessions")]
            for p in ps:
                total += 1
                if p.get("settle_exit") == 0 and any(TX[l] in x or l in x for x in p["settle"] if "SETTLED" in x):
                    settled += 1
        segs = {p["segments"] for mac in summary for p in by[(mac, l)]}
        rng = lambda v: f"{min(v):,}" if min(v) == max(v) else f"{min(v):,}–{max(v):,}"
        print(f"| {l} | " + " | ".join(row) + f" | {CLOUD[l][0]} | {CLOUD[l][1]} | " + " | ".join(ratios)
              + f" | {rng(cyc) if cyc else '—'} ({'/'.join(map(str, sorted(segs)))}) | "
              + (f"{rng([c for c, _ in circ])} ({'/'.join(map(str, sorted({s for _, s in circ})))})" if circ else "—")
              + f" | {settled} of {total} |")
    print()
    print("| Mac | run | job | proofs: `execute_and_prove` (s) | the sessions proven: user cycles, segments, proving time (s) "
          "| `time -l`: wall (s), user (s), max RSS (GiB) | r0vm RSS, sampled peak (GiB) | vm_stat: least free, most compressed (GiB); swapouts "
          "| GPU active residency (powermetrics, %) | settle exit | one flipped bit: settle exit |")
    print("|---|---:|---|---|---|---|---:|---|---|---:|---|")
    for mac, rec in summary.items():
        seen = {}
        for p in rec["proofs"]:
            seen.setdefault((p["run"], p["job"]), []).append(p)
        for (run, job), ps in sorted(seen.items()):
            b = ps[0]
            tl, mem, pm = b["time_l"], b["mem"], b["powermetrics"]
            proofs = "; ".join(f"{p.get('label')}: {p.get('execute_and_prove_s', '—')}" for p in ps)
            sess = "; ".join(", ".join(f"{s['user_cycles']:,} / {s['segments']} / {s['gap_s']}" for s in p.get("proven_sessions", [])) for p in ps)
            gpu = pm.get("gpu_active_pct")
            print(f"| {mac} | {run} | {job} | {proofs} | {sess or '—'} | "
                  + (f"{tl['real_s']:.1f}, {tl['user_s']:.0f}, {tl['max_rss_bytes'] / GIB:.2f}" if 'real_s' in tl else "—")
                  + f" | {mem.get('r0vm_rss_max_gib', '—')} | {mem.get('free_min_gib', '—')}, {mem.get('compressor_max_gib', '—')}; {mem.get('swapouts_during', '—')} | "
                  + (f"{min(gpu):.2f}–{max(gpu):.2f} ({len(gpu)} samples of 10 s)" if gpu else "—")
                  + f" | {b.get('settle_exit', '—')} | {', '.join(str(t['settle_exit']) for t in b['tamper']) or '—'} |")
    print()
    print("| Mac | run | succinct proof, `ferrata-e2e prove` (s) | user cycles (segments) | `time -l` wall (s), max RSS (GiB) | r0vm RSS, sampled peak (GiB) | verified |")
    print("|---|---:|---:|---:|---|---:|---|")
    for mac, rec in summary.items():
        for r in rec["release"]:
            tl, pj = r["time_l"], r["prove_json"]
            print(f"| {mac} | {r['run']} | {round(pj['wall_s'], 1) if 'wall_s' in pj else '—'} | "
                  f"{pj.get('user_cycles', 0):,} ({pj.get('segments', '—')}) | "
                  + (f"{tl['real_s']:.1f}, {tl['max_rss_bytes'] / GIB:.2f}" if 'real_s' in tl else "—")
                  + f" | {r['mem'].get('r0vm_rss_max_gib', '—')} | {'yes' if r.get('verify_exit') == 0 else r.get('verify_exit')} |")

if __name__ == "__main__":
    main()
