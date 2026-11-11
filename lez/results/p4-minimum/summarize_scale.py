#!/usr/bin/env python3
"""Tables of the second pass, from what `minimum.py` wrote under runs/<tag>/<job>/: the same three proofs at
16 CPUs, LEZ's wallet yardstick (`transfer`) at 8 and 16 CPUs, and the release program's proof and wrap
(`release`) at 8 and 16 CPUs. The first pass's runs (runs/attest, runs/bridge: 8 CPUs) are read too, for the
comparison. usage: python3 -I summarize_scale.py  (writes summary-scale.json beside this file, prints the
Markdown tables). Each run is listed with the machine its container reported: one job's runs can come from
two containers when a call was repeated (tags <tag> and <tag>-2), and Modal gives no choice of CPU model.
"""

import json
import pathlib
import statistics
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from summarize import parse_log  # noqa: E402

GIB = 1 << 30
LABELS = {"attest": "private attestation", "transfer": "LEZ wallet's deshielded native transfer"}


def machine(c: dict) -> dict:
    ids = {k: v for k, v in c.get("cpuinfo_ids", []) if k in ("vendor_id", "cpu family", "model", "bogomips")}
    env = c.get("env", {})
    return {
        "nproc": c.get("nproc"), "cfs_quota_us": c.get("cgroup", {}).get("cpu/cpu.cfs_quota_us"),
        "memory_limit_bytes": c.get("cgroup", {}).get("memory/memory.limit_in_bytes"),
        "vendor": ids.get("vendor_id"), "family": ids.get("cpu family"), "model": ids.get("model"),
        "bogomips": ids.get("bogomips"), "flags": c.get("cpu_flags"),
        "cloud": env.get("MODAL_CLOUD_PROVIDER", "").replace("CLOUD_PROVIDER_", ""), "region": env.get("MODAL_REGION"),
    }


def short(m: dict) -> str:
    cpu = {("AuthenticAMD", "25", "1"): "AMD Zen 3", ("AuthenticAMD", "25", "17"): "AMD Zen 4",
           ("GenuineIntel", "6", "85"): "Intel Xeon (Skylake/Cascade Lake)", ("GenuineIntel", "6", "106"): "Intel Xeon (Ice Lake)",
           ("GenuineIntel", "6", "143"): "Intel Xeon (Sapphire Rapids)"}.get((m["vendor"], m["family"], m["model"]),
                                                                        f"{m['vendor']} family {m['family']} model {m['model']}")
    return f"{cpu}, {m['cloud']} {m['region']}"


def proof_runs(job_dir: pathlib.Path, job: str) -> list:
    c = json.load(open(job_dir / "container.json"))
    m = machine(c)
    out = []
    for rd in sorted(job_dir.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
        r = json.load(open(rd / "run.json"))
        parsed = parse_log(open(rd / "prove.log", errors="replace").read(), LABELS.get(job, "private attestation"))
        out.append({"container": str(job_dir.relative_to(HERE)), "machine": m, "run": r["run"], "exit_code": r["exit_code"],
                    "wall_s": r["wall_s"], "gnu_time": r["gnu_time"], "sampled_max": r["sampled"]["max"],
                    "settle_exit": r["settle_exit"], "tamper": [t["settle_exit"] for t in r["tamper"]], "proofs": parsed["proofs"]})
    return out


def release_runs(job_dir: pathlib.Path) -> list:
    c = json.load(open(job_dir / "container.json"))
    m = machine(c)
    exec_json = [json.loads(l) for l in open(job_dir / "exec.log", errors="replace").read().splitlines() if l.startswith("{")]
    out = []
    for rd in sorted(job_dir.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
        r = json.load(open(rd / "run.json"))
        g = json.load(open(rd / "wrap" / "groth16-run.json")) if (rd / "wrap" / "groth16-run.json").is_file() else None
        f = json.load(open(rd / "finish.json")) if (rd / "finish.json").is_file() else None
        out.append({
            "container": str(job_dir.relative_to(HERE)), "machine": m, "run": r["run"], "exec": exec_json[-1] if exec_json else None,
            "prove": {"s": r["prove"]["record"]["wall_s"], "process_s": r["prove"]["wall_s"], "gnu_time": r["prove"]["gnu_time"],
                      "sampled_max": r["prove"]["sampled"]["max"], "record": r["prove"]["record"]},
            "p254": {"s": r["p254"]["record"]["wall_s"], "process_s": r["p254"]["wall_s"], "gnu_time": r["p254"]["gnu_time"],
                     "sampled_max": r["p254"]["sampled"]["max"]},
            "succinct_verified": r.get("succinct_verified"),
            "groth16": None if g is None else {"s": g["seconds"], "cpu_s": g["cpu_s"], "max_rss_kib": g["max_rss_kib"],
                                                "sampled_max": g["sampled"]["max"], "machine": machine(g["container"])},
            "finish": f,
        })
    return out


def cell(xs, fmt="{:.1f}"):
    return " / ".join(fmt.format(x) for x in xs)


def main() -> None:
    runs = HERE / "runs"
    out = {"proofs": {}, "release": {}}
    # (cpus, job) -> runs; the first pass's 8-CPU runs sit in runs/<job>
    for job in ("attest", "bridge"):
        if (runs / job / "container.json").is_file():
            out["proofs"].setdefault(f"8/{job}", []).extend(proof_runs(runs / job, job))
    for tag_dir in sorted(p for p in runs.iterdir() if p.is_dir() and p.name.startswith("cpu")):
        cpus = tag_dir.name.split("-")[0].removeprefix("cpu")
        for job_dir in sorted(p for p in tag_dir.iterdir() if (p / "container.json").is_file()):
            if job_dir.name == "release":
                out["release"].setdefault(cpus, []).extend(release_runs(job_dir))
            else:
                out["proofs"].setdefault(f"{cpus}/{job_dir.name}", []).extend(proof_runs(job_dir, job_dir.name))
    (HERE / "summary-scale.json").write_text(json.dumps(out, indent=1) + "\n")

    print("| CPUs | proof | runs: `execute_and_prove` (s), machine | median (s) | user cycles proven | segments (padded cycles) "
          "| peak GiB: r0vm RSS / container | receipt verified; one flipped bit refused |")
    print("|---:|---|---|---:|---:|---:|---:|---|")
    for key in sorted(out["proofs"], key=lambda k: (k.split("/")[1], int(k.split("/")[0]))):
        cpus, job = key.split("/")
        rs = [r for r in out["proofs"][key] if r["exit_code"] == 0]
        if not rs:
            continue
        for k, p0 in enumerate(rs[0]["proofs"]):
            ps = [(r, r["proofs"][k]) for r in rs if len(r["proofs"]) > k]
            t = [p["execute_and_prove_s"] for _, p in ps]
            uc = sorted({p["user_cycles"] for _, p in ps})
            ucell = f"{uc[0]:,}" if len(uc) == 1 else f"{uc[0]:,}–{uc[-1]:,}"
            each = "; ".join(f"{p['execute_and_prove_s']:.1f} ({short(r['machine'])})" for r, p in ps)
            rss = max(int(r["gnu_time"].get("max_rss_kib", 0)) * 1024 for r, _ in ps)
            cg = max(r["sampled_max"]["cgroup_current_bytes"] for r, _ in ps)
            ok = sum(r["settle_exit"] == 0 for r, _ in ps)
            tam = [x for r, _ in ps for x in r["tamper"]]
            print(f"| {cpus} | {p0['label']} | {each} | {statistics.median(t):.1f} | {ucell} | {p0['segments']} ({p0['total_cycles']:,}) | "
                  f"{rss / GIB:.2f} / {cg / GIB:.2f} | {ok} of {len(ps)}; tamper exits {sorted(set(tam))} ({len(tam)} tried) |")

    print("\n| CPUs | proof | proven session | user cycles | segments (padded) | proving time, each run (s) |")
    print("|---:|---|---|---:|---:|---|")
    for key in sorted(out["proofs"], key=lambda k: (k.split("/")[1], int(k.split("/")[0]))):
        cpus, job = key.split("/")
        rs = [r for r in out["proofs"][key] if r["exit_code"] == 0]
        for k, p0 in enumerate(rs[0]["proofs"] if rs else []):
            n = len(p0["proven_sessions"])
            for j, s0 in enumerate(p0["proven_sessions"]):
                name = "LEZ's privacy-preserving circuit" if j == n - 1 else f"private transition {j + 1}"
                gaps = [r["proofs"][k]["proven_sessions"][j]["gap_s"] for r in rs if len(r["proofs"]) > k and
                        len(r["proofs"][k]["proven_sessions"]) == n]
                users = sorted({r["proofs"][k]["proven_sessions"][j]["user_cycles"] for r in rs if len(r["proofs"]) > k and
                                len(r["proofs"][k]["proven_sessions"]) == n})
                ucell = f"{users[0]:,}" if len(users) == 1 else f"{users[0]:,}–{users[-1]:,}"
                print(f"| {cpus} | {p0['label'] if j == 0 else ''} | {name} | {ucell} | {s0['segments']} ({s0['total_cycles']:,}) | {cell(gaps)} |")

    print("\n| CPUs | release run | machine (proof) | succinct proof (s) | peak GiB r0vm / container | p254 (s) | Groth16 prover (s), machine "
          "| Groth16 peak GiB (RSS / container) | verified: succinct; Groth16 receipt; seal; journal |")
    print("|---:|---:|---|---:|---:|---:|---|---:|---|")
    for cpus in sorted(out["release"], key=int):
        for r in out["release"][cpus]:
            g, f = r["groth16"], r["finish"] or {}
            gcell = f"{g['s']:.1f} ({short(g['machine'])})" if g else "—"
            gmem = f"{g['max_rss_kib'] * 1024 / GIB:.2f} / {g['sampled_max']['cgroup_current_bytes'] / GIB:.2f}" if g else "—"
            print(f"| {cpus} | {r['run']} | {short(r['machine'])} | {r['prove']['s']:.1f} | "
                  f"{int(r['prove']['gnu_time'].get('max_rss_kib', 0)) * 1024 / GIB:.2f} / {r['prove']['sampled_max']['cgroup_current_bytes'] / GIB:.2f} | "
                  f"{r['p254']['s']:.1f} | {gcell} | {gmem} | {r['succinct_verified']}; {f.get('verify_exit') == 0}; "
                  f"{f.get('seal_bytes')} B {f.get('selector')}; {f.get('journal_equals_native')} |")
    ex = next((r["exec"] for rs in out["release"].values() for r in rs if r["exec"]), None)
    if ex:
        print(f"\nrelease executor: {ex['user_cycles']:,} user cycles, {ex['segments']} segment ({ex['padded_cycles']:,} padded), "
              f"journal {ex['journal_bytes']} B equal to the native one: {ex['journal_equals_native']}")


if __name__ == "__main__":
    main()
