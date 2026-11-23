#!/usr/bin/env python3
"""The record of live-testnet.sh's run, and its figures (modules/results/live-testnet-*/).

  python3 -I modules/scripts/live_testnet.py collect OUT RECORD   on the build box: the run's files OUT -> RECORD, with
                                                                   no keys and no wallet storage, the host's name and the
                                                                   box's paths replaced; then `summary`
  python3 -I modules/scripts/live_testnet.py summary RECORD        the figures and checks, from the record alone:
                                                                   RECORD/summary.json and RECORD/summary.md
  python3 -I modules/scripts/live_testnet.py verify RECORD         the same computed again, compared with the committed
                                                                   summary.json (exit 1 on any difference)

Every figure comes from a file of the record: the process samples (bench/perf/tree_sample.py: CPU seconds of each
process with its children, resident memory), the relays' byte counts (bench/perf/http_relay.py), GNU time's output
for the one-shot tools, the module calls' replies and durations (calls.tsv), the phase marks (marks.tsv), the
updater's journal and its backend's manifest (journal.tsv, run.tsv), its /health probes and the module's status.
"""
import csv
import gzip
import json
import os
import re
import shutil
import statistics
import subprocess
import sys

try:
    import tomllib
except ImportError:  # Python < 3.11
    tomllib = None

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

# ---- collect -----------------------------------------------------------------------------------------------------

# What the record keeps of OUT (relative paths; directories copied whole, after the exclusions below).
KEEP = [
    "report-build/steps.md", "report-build/artifacts.txt", "report-build/logs/versions.log",
    "report-build/logs/stage-updater-rc3.log", "report-build/logs/lock-updater-rc3.log", "report-build/logs/rc3-copy.log",
    "report-build/logs/rc3-tools-test.log",
    "report-instance/steps.md", "report-instance/logs/send-init.log", "report-instance/logs/sign-init.log",
    "report-instance/logs/checkpoint.log", "report-instance/logs/faucet-payer.log", "report-instance/logs/faucet-daemon.log",
    "report-session/steps.md", "report-session/notes.log",
    "instance",
    "session/req", "session/rep", "session/relay", "session/monitor", "session/attest", "session/cli",
    "session/probes-u1", "session/calls.tsv", "session/marks.tsv", "session/samples.csv", "session/journal.tsv",
    "session/run.tsv", "session/final-state.json", "session/processes.txt",
    "session/install.log", "session/core-start.log", "session/core-stop.log", "session/updater-state-files.txt",
    # RFP-022's monitor and relayer as Logos modules on the anvil fixture (bench/perf/daemons-anvil-modules.sh)
    "daemons-anvil-modules/samples.csv", "daemons-anvil-modules/phases.tsv", "daemons-anvil-modules/rpc-monitor.json",
    "daemons-anvil-modules/rpc-relayer.json", "daemons-anvil-modules/machine.txt", "daemons-anvil-modules/log.txt",
    "daemons-anvil-modules/rep", "daemons-anvil-modules/run/alerts.jsonl", "daemons-anvil-modules/run/core-stop.log",
    "daemons-anvil-modules/run/install.log", "daemons-anvil-modules/run/node-view.json",
]
NEVER = re.compile(r"(\.key$|/storage\.json$|/keys/|/wallet|\.raw$|attest-messages|/h\.json$)")
BIG = 4 << 20  # text files above this are kept gzipped


def scrub_text(t, subs):
    for a, b in subs:
        t = t.replace(a, b)
    return t


def collect(out, rec):
    out = os.path.abspath(out)
    rec = os.path.abspath(rec)
    rpc = os.environ.get("FERRATA_TESTNET_RPC", "")
    host = re.sub(r"^https?://", "", rpc).split("/", 1)[0] if rpc else ""
    spike = os.path.dirname(REPO)
    subs = []
    if host:
        subs += [(rpc, "https://<domain>/rpc"), ("https://" + host, "https://<domain>"), (host, "<domain>")]
    subs += [(REPO + "/", ""), (REPO, "."), (out + "/", "<out>/"), (out, "<out>"),
             (os.environ.get("ML_WORK", "/var/tmp/ml") + "/", "<work>/"),
             (os.environ.get("LOGOS_TMPDIR", "/var/tmp/mls") + "/", "<logos-tmp>/"),
             (spike + "/", "<spike>/"), (os.path.expanduser("~") + "/", "~/")]
    # Everything collect writes is written again; the record's README (written by hand) stays.
    if os.path.exists(rec):
        for f in os.listdir(rec):
            if f != "README.md":
                p = os.path.join(rec, f)
                shutil.rmtree(p) if os.path.isdir(p) else os.remove(p)
    os.makedirs(rec, exist_ok=True)
    kept = []
    for item in KEEP:
        src = os.path.join(out, item)
        if not os.path.exists(src):
            print(f"collect: absent: {item}")
            continue
        files = [src] if os.path.isfile(src) else [os.path.join(d, f) for d, _, fs in os.walk(src) for f in fs]
        for f in sorted(files):
            rel = os.path.relpath(f, out)
            if NEVER.search("/" + rel):
                continue
            dst = os.path.join(rec, rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            data = open(f, "rb").read()
            try:
                text = data.decode()
            except UnicodeDecodeError:
                print(f"collect: binary, left out: {rel}")
                continue
            text = scrub_text(text, subs)
            if len(text) > BIG:
                with gzip.open(dst + ".gz", "wt") as g:
                    g.write(text)
                kept.append(rel + ".gz")
            else:
                open(dst, "w").write(text)
                kept.append(rel)
    # The updater module's own lines, from Logos Core's log, without the prefix Logos Core adds to a module's output
    # (`[time] [out] [ferrata_updater_module] ` for its standard output, `[time] [info] [logos] [ferrata_updater_module] `
    # for its log): the lines check.py reads (lez/updater/results/live-sepolia/check.py).
    src = os.path.join(out, "session", "logos-logs")
    dated = sorted(f for f in os.listdir(src) if f.startswith("daemon_")) if os.path.isdir(src) else []
    if dated:
        prefix = re.compile(r"^\[[^\]]*\] \[(?:out|err|info|warning|error|debug)\] (?:\[logos\] )?\[ferrata_updater_module\] ")
        lines = [prefix.sub("", l) for l in open(os.path.join(src, dated[-1]), errors="replace").read().splitlines() if prefix.match(l)]
        open(os.path.join(rec, "session", "updater-module.log"), "w").write(scrub_text("\n".join(lines) + "\n", subs))
        kept.append("session/updater-module.log")
    # The Logos Core logs (every module's lines), gzipped; daemon.log is a link to the newest dated one.
    for d in ("session/logos-logs", "daemons-anvil-modules/run"):
        src = os.path.join(out, d)
        for f in sorted(os.listdir(src)) if os.path.isdir(src) else []:
            if not f.startswith("daemon_"):
                continue
            text = scrub_text(open(os.path.join(src, f), errors="replace").read(), subs)
            os.makedirs(os.path.join(rec, d), exist_ok=True)
            with gzip.open(os.path.join(rec, d, f + ".gz"), "wt") as g:
                g.write(text)
            kept.append(f"{d}/{f}.gz")
    print(f"collect: {len(kept)} files into {rec}")
    # Nothing of the host's name, the workspaces or the box's paths may remain (scripts/scrub_records.py checks).
    # Names kept outside the repository (cloud workspaces, a host's name) come from the environment only.
    deny = [re.escape(host)] if host else []
    extra = os.environ.get("FERRATA_SCRUB_DENY", "")
    env = dict(os.environ, FERRATA_SCRUB_DENY="|".join(deny + ([extra] if extra else [])) or "^$")
    r = subprocess.run([sys.executable, "-I", os.path.join(REPO, "scripts", "scrub_records.py"), "--check-only", rec],
                       env=env, capture_output=True, text=True)
    print(r.stdout[-2000:], r.stderr[-2000:])
    if r.returncode != 0:
        sys.exit("collect: the record still holds a name or a path (scrub_records.py above)")
    summary(rec)


# ---- summary -----------------------------------------------------------------------------------------------------

def rd(rec, rel, default=None):
    p = os.path.join(rec, rel)
    if os.path.exists(p):
        return open(p, errors="replace").read()
    if os.path.exists(p + ".gz"):
        with gzip.open(p + ".gz", "rt", errors="replace") as g:
            return g.read()
    return default


def js(rec, rel):
    t = rd(rec, rel)
    if t is None:
        return None
    try:
        return json.loads(t)
    except ValueError:
        return None


def jsonl(rec, rel):
    return [json.loads(l) for l in (rd(rec, rel) or "").splitlines() if l.strip()]


def gnu_time(text):
    """GNU time -v: user + system CPU seconds, wall seconds, peak resident kB."""
    if not text:
        return None
    def num(label):
        m = re.search(re.escape(label) + r":\s*([0-9.:]+)", text)
        return m.group(1) if m else None
    u, s, e, m = num("User time (seconds)"), num("System time (seconds)"), num("Elapsed (wall clock) time (h:mm:ss or m:ss)"), num("Maximum resident set size (kbytes)")
    if u is None or e is None:
        return None
    parts = [float(x) for x in e.split(":")]
    wall = sum(p * 60 ** i for i, p in enumerate(reversed(parts)))
    return {"cpu_s": round(float(u) + float(s), 2), "wall_s": round(wall, 2), "peak_rss_mib": round(int(m) / 1024, 1),
            "cpu_share": round((float(u) + float(s)) / wall, 4) if wall else None}


def marks_of(rec):
    m = {}
    for row in csv.reader((rd(rec, "session/marks.tsv") or "").splitlines()[1:], delimiter="\t"):
        if len(row) >= 2:
            m.setdefault(row[1], float(row[0]))
    return m


def samples_of(rec, rel="session/samples.csv"):
    by = {}
    text = rd(rec, rel) or ""
    for r in csv.DictReader(text.splitlines()):
        if r["pid"] == "0" or not r["cpu_tree_s"]:
            continue
        by.setdefault(r["name"], []).append({"t": float(r["t"]), "pid": int(r["pid"]), "cpu": float(r["cpu_tree_s"]),
                                             "self": float(r["cpu_s"]), "rss": int(r["rss_tree_kb"]), "rss_self": int(r["rss_kb"]),
                                             "hwm": int(r["hwm_kb"]), "procs": int(r["procs"])})
    return by


def own(rows):
    """The process alone (Logos Core's daemon without the module host processes it starts)."""
    return [dict(r, cpu=r["self"], rss=r["rss_self"], procs=1) for r in rows]


def window(rows, t0, t1):
    """CPU seconds and memory of one process tree between t0 and t1 (its samples within; a restart of the process,
    a new pid, adds the new process's CPU from its first sample)."""
    w = [r for r in rows if t0 <= r["t"] <= t1]
    if len(w) < 2:
        return None
    cpu, prev = 0.0, w[0]
    for r in w[1:]:
        cpu += r["cpu"] - prev["cpu"] if r["pid"] == prev["pid"] and r["cpu"] >= prev["cpu"] else r["cpu"]
        prev = r
    wall = w[-1]["t"] - w[0]["t"]
    rss = [r["rss"] for r in w]
    return {"wall_s": round(wall, 1), "cpu_s": round(cpu, 2), "cpu_share": round(cpu / wall, 4) if wall else None,
            "peak_rss_mib": round(max(rss) / 1024, 1), "median_rss_mib": round(statistics.median(rss) / 1024, 1),
            "peak_hwm_mib": round(max(r["hwm"] for r in w) / 1024, 1), "samples": len(w), "max_procs": max(r["procs"] for r in w)}


def relay_of(rec, name):
    s = js(rec, f"session/relay/{name}.json")
    if not s:
        return None
    lc = sum(v["n"] for k, v in s["by"].items() if "light_client" in k)
    return {"up": s["up"], "down": s["down"], "requests": s["requests"], "errors": s["errors"], "seconds": s["seconds"],
            "light_client_requests": lc,
            "by": {k: {"n": v["n"], "up": v["up"], "down": v["down"]} for k, v in sorted(s["by"].items())}}


def summary(rec, write=True):
    S = {"record": os.path.basename(os.path.abspath(rec))}
    checks = []

    def check(ok, what):
        checks.append({"ok": bool(ok), "what": what})

    marks = marks_of(rec)
    samples = samples_of(rec)
    t_up, t_stop = marks.get("updater-start"), marks.get("stop")
    S["marks"] = {k: round(v - marks.get("session-start", v), 1) for k, v in sorted(marks.items(), key=lambda kv: kv[1])}
    # ---- the components' resources -------------------------------------------------------------------------------
    comp = {}
    if t_up and t_stop:
        for name in ("updater-module", "beacon_api_module", "eth_rpc_module", "attest-module", "attest-monitor"):
            if name in samples:
                comp[name] = window(samples[name], t_up, t_stop)
        if "core" in samples:
            comp["Logos Core daemon (logosctl, its own process)"] = window(own(samples["core"]), t_up, t_stop)
            comp["Logos Core with every module host process (its tree)"] = window(samples["core"], t_up, t_stop)
        if "relays" in samples:
            comp["measurement: one counting relay (lez-updater)"] = window(samples["relays"], t_up, t_stop)
    # The updater module's start: its follower brought from the network's genesis to its last final block.
    log = rd(rec, "session/updater-module.log") or ""
    follow = [m for m in re.finditer(r"\[sequencer\] \S+: head block (\d+), following to block (\d+) \(final on Bedrock: (\w+)\) in ([0-9.]+) s", log)]
    if follow:
        S["follower_start"] = [{"head": int(m.group(1)), "followed_to": int(m.group(2)), "seconds": float(m.group(4))} for m in follow]
    # `start` answers once its set-up is done (the follower from the network's genesis): the marks bound the two phases.
    t_started = marks.get("updater-started")
    if t_up and t_stop and t_started and "updater-module" in samples:
        S["updater_start_seconds"] = round(t_started - t_up, 1)
        comp["updater-module (start: follower from genesis)"] = window(samples["updater-module"], t_up, t_started)
        comp["updater-module (after the start)"] = window(samples["updater-module"], t_started, t_stop)
    a0, a1 = marks.get("attest-start"), marks.get("attest-module-end")
    if a0 and a1:
        for name in ("attest-module", "eth_rpc_module"):
            if name in samples:
                comp[f"{name} (its calls)"] = window(samples[name], a0 - 5, a1 + 5)
    S["components"] = comp
    tools = {}
    for label, rel in (("ferrata-day1 send (Init)", "report-instance/logs/send-init.log"),
                       ("ferrata-day1 sign (Attest)", "session/attest/attest-sign.log"),
                       ("ferrata-day1 send (Attest)", "session/attest/attest-send.log"),
                       ("ferrata-attest CLI journey", "session/cli/stderr.txt")):
        g = gnu_time(rd(rec, rel))
        if g:
            tools[label] = g
    S["tools"] = tools
    # ---- traffic ---------------------------------------------------------------------------------------------------
    S["traffic"] = {n: relay_of(rec, n) for n in ("lez-updater", "lodestar-updater", "nimbus-updater", "lez-monitor",
                                                  "lodestar-monitor", "publicnode-monitor") if relay_of(rec, n)}
    # ---- the module calls ------------------------------------------------------------------------------------------
    calls = []
    for row in csv.DictReader((rd(rec, "session/calls.tsv") or "").splitlines(), delimiter="\t"):
        calls.append({"name": row["name"], "module": row["module"], "method": row["method"], "exit": int(row["exit"]),
                      "seconds": float(row["seconds"])})
    S["calls"] = [c for c in calls if not c["name"].startswith("status-")]
    # ---- RFP-021 U.3: what the run shows ------------------------------------------------------------------------------
    loaded = js(rec, "session/rep/modules-loaded.json")
    names = json.dumps(loaded) if loaded is not None else ""
    for m in ("ferrata_updater_module", "beacon_api_module", "eth_rpc_module", "ferrata_attest_module"):
        check(m in names, f"{m} loaded in the headless Logos Core (session/rep/modules-loaded.json)")
    ver = js(rec, "session/rep/U-version.json") or {}
    vtext = json.dumps(ver)
    check("sequencer" in vtext and "v0.4.0-rc3" in vtext, "the updater module carries the sequencer backend, built on LEZ v0.4.0-rc3 (U-version)")
    toml_text = rd(rec, "session/req/updater.toml") or ""
    srcs = []
    if tomllib and toml_text:
        cfg = tomllib.loads(toml_text)
        srcs = [s for c in cfg.get("chain", []) for s in c.get("source", [])]
        S["updater_config"] = {"ledger": cfg["daemon"]["ledger"], "sources": [{k: s.get(k) for k in ("name", "via", "provider", "chain_id")} for s in srcs],
                               "min_agreeing": cfg["policy"]["min_agreeing"], "poll_secs": cfg["daemon"]["poll_secs"]}
    check(srcs and all(s.get("via") == "beacon_api_module" for s in srcs) and len(srcs) >= 2,
          "every light-client source of the updater goes through beacon_api_module (updater.toml), two required to agree")
    tr = S["traffic"]
    lc = [tr.get("lodestar-updater", {}).get("light_client_requests", 0), tr.get("nimbus-updater", {}).get("light_client_requests", 0)]
    check(sum(lc) > 0, f"beacon_api_module carried the updater's light-client requests: {lc[0]} to Lodestar, {lc[1]} to Nimbus (the relays' counts)")
    journal = [l.split("\t") for l in (rd(rec, "session/journal.tsv") or "").splitlines() if l.strip()]
    J = ["round", "step", "job", "finalized_slot", "result", "exit", "gas_exec", "gas_stor", "fee", "block", "utc", "label"]
    journal = [dict(zip(J, r)) for r in journal]
    applied = [j for j in journal if j["result"] == "applied"]
    jobs = {}
    for j in applied:
        jobs.setdefault(j["job"], []).append(j["step"])
    finished = [k for k, v in jobs.items() if "finish" in v]
    adopted = [k for k, v in jobs.items() if any(s.startswith("adopt") for s in v)]
    S["journal"] = {"transactions": len(journal), "applied": len(applied), "results": {r: sum(j["result"] == r for j in journal) for r in sorted({j["result"] for j in journal})},
                    "jobs_finished": len(finished), "jobs_with_adoption": len(adopted),
                    "fees": sum(int(j["fee"] or 0) for j in applied), "gas_exec": sum(int(j["gas_exec"] or 0) for j in applied),
                    "blocks": [int(applied[0]["block"]), int(applied[-1]["block"])] if applied else None,
                    "finalized_slots": sorted({int(j["finalized_slot"]) for j in applied if j["finalized_slot"].isdigit()})}
    check(len(finished) >= 1 and len(applied) == len(journal), f"the module applied {len(applied)} of {len(journal)} transactions on the network, {len(finished)} jobs finished")
    # The backend's line per transaction: `[updater] sepolia <step> applied gas N block B + T s <hash>`, T the time from
    # the send to the block final on Bedrock (the follower applied it).
    txl = re.findall(r"^\[updater\] (\S+ .+?)\s+(applied|reverted|dropped|rejected)\s+gas\s+(\d+) block (\d+) \+ ([0-9.]+)s ([0-9a-f]{64})$",
                     log, re.M)
    if txl:
        secs = sorted(float(t[4]) for t in txl if t[1] == "applied")
        S["send_to_final_s"] = {"transactions": len(txl), "applied": len(secs), "min": secs[0] if secs else None,
                                "median": round(statistics.median(secs), 1) if secs else None, "max": secs[-1] if secs else None}
    # attest-monitor's verdict on every finalized header the instance held (its state file, kept across polls).
    seen = js(rec, "session/monitor/seen.json")
    if seen is not None:
        S["attest_monitor_seen"] = {k: {"root": v.get("root"), "divergence": v.get("divergence"), "finalized_by": v.get("finalized_by")}
                                    for k, v in sorted(seen.items())}
        cp = (js(rec, "instance/instance.json") or {}).get("checkpoint_slot") or 0
        moved = sorted({int(j["finalized_slot"]) for j in applied if j["step"] == "finish" and j["finalized_slot"].isdigit()
                        and int(j["finalized_slot"]) > cp})
        confirmed = [x for x in moved if (seen.get(str(x)) or {}).get("divergence") == "confirmed"]
        check(moved and len(confirmed) == len(moved),
              f"every finalized header the module's jobs moved the instance to ({len(moved)}) was confirmed canonical by "
              f"attest-monitor's two providers (session/monitor/seen.json: {len(confirmed)} confirmed)")
    probes = jsonl(rec, "session/probes-u1/health.jsonl")
    ok = [p for p in probes if p.get("http") == 200 and p.get("health") and all(h.get("status") == "ok" for h in p["health"])]
    S["health"] = {"probes": len(probes), "ok": len(ok), "lives": sorted({p["life"] for p in probes})}
    check(probes and len(ok) >= len(probes) - 2, f"/health answered ok to {len(ok)} of {len(probes)} probes (the module's own server)")
    status = jsonl(rec, "session/probes-u1/status.jsonl")
    S["status_through_core"] = {"calls": len(status), "running": sum(1 for s in status if (s.get("status") or {}).get("running"))}
    check(status and S["status_through_core"]["running"] >= len(status) - 2, f"status through Logos Core: running in {S['status_through_core']['running']} of {len(status)} calls")
    restart = {k: js(rec, f"session/rep/{k}.json") for k in ("U-stop-1", "U-start-2", "U-stop-final")}
    S["restart"] = {k: (v if isinstance(v, dict) and len(json.dumps(v)) < 2000 else ("reply" if v else None)) for k, v in restart.items()}
    check(all(restart.values()), "the module's stop, start (a restart halfway) and the final stop answered (rep/U-stop-1, U-start-2, U-stop-final)")
    if marks.get("restart-start"):
        after = [j for j in applied if j["utc"] and j["utc"] > iso(marks["restart-start"])]
        check(after, f"after the restart the module went on: {len(after)} transactions applied")
    core = rd(rec, "session/core-stop.log") or ""
    check("logosctl daemon stop exit 0" in core, "Logos Core stopped cleanly (logosctl daemon stop exit 0)")
    mon_exit = rd(rec, "session/monitor/exit.txt") or ""
    S["attest_monitor_exit"] = mon_exit.strip()
    fs = js(rec, "session/final-state.json") or {}
    inst = js(rec, "instance/instance.json") or {}
    S["instance"] = {"id": inst.get("instance_id"), "checkpoint_slot": inst.get("checkpoint_slot"),
                     "final": {k: fs.get(k) for k in ("active", "applied", "finalized_slot", "finalized_root", "next_committee_known")}}
    check(fs.get("active") and inst.get("checkpoint_slot") and fs.get("finalized_slot", 0) > inst["checkpoint_slot"],
          f"the instance is active and moved from its checkpoint (slot {inst.get('checkpoint_slot')}) to slot {fs.get('finalized_slot')}")
    send = rd(rec, "session/attest/attest-send.log") or ""
    check("every transaction applied" in send, "an attestation of the Sepolia vault's slots 0 and 1 under the instance's record, applied on the network (attest-send.log)")
    for c in ("A-read_instance", "A-verify_checkpoint", "A-fetch_log_proof", "A-dry_run_storage", "A-dry_run_storage_eth"):
        r = js(rec, f"session/rep/{c}.json")
        check(isinstance(r, dict) and "call_failed" not in r and "error" not in r, f"ferrata_attest_module {c[2:]} answered (rep/{c}.json)")
    vc = js(rec, "session/rep/A-verify_checkpoint.json") or {}
    S["verify_checkpoint"] = {k: vc.get(k) for k in ("verdict", "instance_id_matches", "providers_agreeing")}
    S["verify_checkpoint"]["execution"] = (vc.get("execution") or {}).get("verdict") if isinstance(vc.get("execution"), dict) else vc.get("execution")
    # ---- RFP-022 P.6: monitor and relayer as Logos modules on the anvil fixture -------------------------------------
    B = "daemons-anvil-modules"
    phases = [(float(l.split("\t")[0]), l.split("\t", 1)[1]) for l in (rd(rec, f"{B}/phases.tsv") or "").splitlines() if "\t" in l]
    if phases:
        bs = samples_of(rec, f"{B}/samples.csv")
        t0, t1 = phases[0][0], phases[-1][0]
        bm = {"phases": [{"at_s": round(t - t0), "mark": m} for t, m in phases], "components": {}, "per_phase_cpu_s": {}}
        for name in ("monitor-module", "relayer-module", "core"):
            if name in bs:
                rows = own(bs[name]) if name == "core" else bs[name]
                label = "Logos Core daemon (its own process)" if name == "core" else name
                bm["components"][label] = window(rows, t0, t1)
                bm["per_phase_cpu_s"][label] = [(window(rows, a, b) or {}).get("cpu_s") for (a, _), (b, _) in zip(phases, phases[1:])]
        if "core" in bs:
            bm["components"]["Logos Core with every module host process (its tree)"] = window(bs["core"], t0, t1)
        for who in ("monitor", "relayer"):
            r = js(rec, f"{B}/rpc-{who}.json")
            if r:
                bm["components"].setdefault(f"{who}-module", {})["rpc"] = {"up": r["up"], "down": r["down"], "connections": r.get("connections")}
        S["bridge_modules"] = bm
        end = [m for _, m in phases if m.startswith("activity end")]
        reached = bool(end) and re.search(r"final state (0x[0-9a-f]+)….*wanted (0x[0-9a-f]+)…", end[0])
        check(reached and reached.group(1) == reached.group(2), "RFP-022: the relayer module finalised the honest candidate (phases.tsv: final state = wanted)")
        for k in ("monitor-start", "relayer-start", "monitor-stop", "relayer-stop"):
            r = js(rec, f"{B}/rep/{k}.json")
            res = r.get("result") if isinstance(r, dict) else None
            if isinstance(res, str):
                try:
                    res = json.loads(res)
                except ValueError:
                    pass
            ok = isinstance(res, dict) and res.get("verdict") not in ("error", "invalid_request") and res.get("exit", 0) in (0, None)
            check(ok, f"RFP-022: {k} answered through Logos Core, no error ({B}/rep/{k}.json: verdict "
                      f"{res.get('verdict') if isinstance(res, dict) else None})")
        check("logosctl daemon stop exit 0" in (rd(rec, f"{B}/run/core-stop.log") or ""), "RFP-022: that Logos Core stopped cleanly")
    S["checks"] = checks
    S["checks_failed"] = sum(not c["ok"] for c in checks)
    if write:
        with open(os.path.join(rec, "summary.json"), "w") as f:
            json.dump(S, f, indent=1, sort_keys=True)
            f.write("\n")
        open(os.path.join(rec, "summary.md"), "w").write(markdown(S))
        print(markdown(S))
    return S


def iso(t):
    import datetime
    return datetime.datetime.fromtimestamp(t, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def mib(b):
    return f"{b / 1048576:.2f} MiB"


def markdown(S):
    o = [f"# {S['record']}: figures and checks (written by `modules/scripts/live_testnet.py summary`)", ""]
    o += ["## Checks", ""] + [("- PASS " if c["ok"] else "- FAIL ") + c["what"] for c in S["checks"]] + [""]
    o += [f"Checks failed: {S['checks_failed']}", "", "## Components (measured: process samples every 5 s)", "",
          "| component | wall s | CPU s | CPU share of one core | peak RSS MiB | median RSS MiB | processes (max) |", "|---|---:|---:|---:|---:|---:|---:|"]
    for k, v in S["components"].items():
        if v:
            o.append(f"| {k} | {v['wall_s']} | {v['cpu_s']} | {v['cpu_share']} | {v['peak_rss_mib']} | {v['median_rss_mib']} | {v['max_procs']} |")
    o += ["", "## One-shot tools (measured: GNU time)", "", "| tool | wall s | CPU s | peak RSS MiB |", "|---|---:|---:|---:|"]
    for k, v in S["tools"].items():
        o.append(f"| {k} | {v['wall_s']} | {v['cpu_s']} | {v['peak_rss_mib']} |")
    o += ["", "## Traffic (measured: each daemon's own relay, HTTP level, TLS not counted)", "",
          "| relay | requests | errors | up | down | seconds |", "|---|---:|---:|---:|---:|---:|"]
    for k, v in S["traffic"].items():
        o.append(f"| {k} | {v['requests']} | {v['errors']} | {mib(v['up'])} | {mib(v['down'])} | {v['seconds']} |")
    o += ["", "## Module calls", "", "| call | module | method | exit | seconds |", "|---|---|---|---:|---:|"]
    for c in S["calls"]:
        o.append(f"| {c['name']} | {c['module']} | {c['method']} | {c['exit']} | {c['seconds']} |")
    o += ["", "## Journal", "", "```", json.dumps(S["journal"], indent=1), "```", ""]
    bm = S.get("bridge_modules")
    if bm:
        o += ["## RFP-022: monitor and relayer as Logos modules (anvil fixture)", "",
              "Phases: " + "; ".join(f"{p['mark']} at +{p['at_s']} s" for p in bm["phases"]) + ".", "",
              "| process | wall s | CPU s | CPU share of one core | peak RSS MiB | median RSS MiB | JSON-RPC bytes up / down |", "|---|---:|---:|---:|---:|---:|---:|"]
        for k, v in bm["components"].items():
            if v and "wall_s" in v:
                rpc = v.get("rpc") or {}
                o.append(f"| {k} | {v['wall_s']} | {v['cpu_s']} | {v['cpu_share']} | {v['peak_rss_mib']} | {v['median_rss_mib']} | "
                         f"{rpc.get('up', '-')} / {rpc.get('down', '-')} |")
        o += ["", "CPU seconds per phase: " + "; ".join(f"{k} {v}" for k, v in bm["per_phase_cpu_s"].items()), ""]
    return "\n".join(o) + "\n"


def verify(rec):
    want = json.load(open(os.path.join(rec, "summary.json")))
    got = json.loads(json.dumps(summary(rec, write=False), sort_keys=True))
    if got != want:
        for k in sorted(set(got) | set(want)):
            if got.get(k) != want.get(k):
                print(f"differs: {k}")
        sys.exit(1)
    print(f"verify: {rec}: summary.json recomputed from the record, equal; checks failed {want['checks_failed']}")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[:1] == ["collect"] and len(a) == 3:
        collect(a[1], a[2])
    elif a[:1] == ["summary"] and len(a) == 2:
        summary(a[1])
    elif a[:1] == ["verify"] and len(a) == 2:
        verify(a[1])
    else:
        sys.exit(__doc__)
