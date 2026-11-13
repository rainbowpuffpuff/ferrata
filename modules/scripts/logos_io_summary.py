"""Summarise logos-io.sh's run: OUT_DIR/report/summary.md (and summary.json).

    python3 -I logos_io_summary.py OUT_DIR FAILURES

Which Ethereum read went through which Logos Core module (eth_rpc_module's own `route` for each call),
whether each read reply equals the plain path's (or the proofs given), what the recorded server and the
devnet saw, the end-to-end checks, the send stack's answers and the steps. Exit status 1 when a step
failed, a comparison is false or an end-to-end check failed.
"""
import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])
failures = int(sys.argv[2]) if len(sys.argv) > 2 else 0
rep = out / "report"
md = ["# Logos Core's own modules beside Ferrata's (logos-io.sh)", ""]
summary = {"comparisons": [], "e2e": {}, "failures_in_steps": failures}


def tsv(path):
    if not path.exists():
        return []
    rows = path.read_text().splitlines()
    head = rows[0].split("\t")
    return [dict(zip(head, r.split("\t"))) for r in rows[1:] if r.strip()]


md += ["## Reads through eth_rpc_module, compared", "",
       "| run | reply | path | routes (eth_rpc_module's) | equal to |", "|---|---|---|---|---|"]
bad = 0
for run, ref in (("e2e", "the plain-HTTP read"), ("bridge-e2e", "the proofs given")):
    for r in tsv(out / run / "logos-io.tsv"):
        eq = r.get("equal_to_plain", r.get("equal_to_given"))
        summary["comparisons"].append({"run": run, **r})
        if eq != "true":
            bad += 1
        md.append(f"| {run} | `{r['name']}` | {r['via']} | `{r['routes']}` | {ref}: **{eq}** |")
md.append("")

# Who asked the recorded server (e2e.sh): the module's own client sends Accept: application/json,
# eth_rpc_module's reqwest client accept: */*.
log = out / "e2e" / "rpc.log"
if log.exists():
    seen = {}
    for line in log.read_text().splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        key = (e.get("method"), e.get("accept"), e.get("user_agent"))
        seen[key] = seen.get(key, 0) + 1
    md += ["## What the recorded server saw (e2e.sh)", "", "| method | Accept | User-Agent | requests |", "|---|---|---|---:|"]
    for (m, a, u), n in sorted(seen.items(), key=lambda kv: str(kv[0])):
        md.append(f"| {m} | {a} | {u} | {n} |")
    md.append("")
    summary["recorded_server"] = [{"method": m, "accept": a, "user_agent": u, "requests": n} for (m, a, u), n in seen.items()]

md += ["## End to end through the modules", ""]
for run, name in (("e2e", "e2e.json"), ("bridge-e2e", "bridge_e2e.json")):
    p = out / run / name
    if p.exists():
        d = json.loads(p.read_text())
        summary["e2e"][run] = {"checks": d.get("checks"), "failed": d.get("failed"), "mode": d.get("mode")}
        md.append(f"- `{run}`: {d.get('checks')} checks, {d.get('failed')} failed ({d.get('mode')})")
        if d.get("failed"):
            bad += 1
    else:
        md.append(f"- `{run}`: no result")
        bad += 1
md.append("")

for name, title in (("send-stack.log", "The send stack against anvil (tx_sender_module.prepare)"),
                    ("contracts.txt", "eth_rpc_module's contract"), ("steps.md", "Steps")):
    p = rep / name
    if p.exists():
        md += [f"## {title}", ""]
        body = p.read_text().rstrip()
        md += (["```", body[:6000], "```"] if name.endswith(".log") else [body])
        md.append("")

ok = failures == 0 and bad == 0
md.append(f"**Step failures {failures}, comparison or end-to-end failures {bad}.**")
summary["bad"] = bad
(rep / "summary.md").write_text("\n".join(md) + "\n")
(rep / "summary.json").write_text(json.dumps(summary, indent=1))
print("\n".join(md))
sys.exit(0 if ok else 1)
