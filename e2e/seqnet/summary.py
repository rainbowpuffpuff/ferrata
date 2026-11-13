#!/usr/bin/env python3
"""The record of one e2e/seqnet run, from its files: every Ethereum and LEZ transaction with who sent it, and what the
user ran and saw.  summary.py RUN_DIR  ->  RUN_DIR/SUMMARY.md and RUN_DIR/summary.json"""
import json
import pathlib
import re
import sys

run = pathlib.Path(sys.argv[1])


def lines(p):
    try:
        return (run / p).read_text().splitlines()
    except OSError:
        return []


def tsv(p):
    rows = lines(p)
    if not rows:
        return []
    head = rows[0].split("\t")
    return [dict(zip(head, r.split("\t"))) for r in rows[1:] if r.strip()]


def jsonl(p):
    out = []
    for line in lines(p):
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out


def short(h, n=10):
    h = str(h)
    return h if len(h) <= 2 * n + 3 else h[: n + 2] + "…" + h[-n:]


# Who sent which LEZ transaction: the operator's (crown-a, crown-b), the gas drop's stand-in, the user's.
who = {}
for line in lines("lez/crown-a.log") + lines("lez/crown-b-operator.log"):
    m = re.search(r"^\[crown-[ab]\]\s+(.*?)\s+applied\s.*\s([0-9a-f]{64})\s*$", line)
    if m:
        who[m.group(2)] = ("operator", m.group(1).strip())
for f in jsonl("lez/fund.jsonl"):
    if f.get("tx"):
        who[f["tx"]] = ("gas drop (stand-in)", f.get("step", ""))
for p in sorted((run / "user").glob("*.txt")) if (run / "user").is_dir() else []:
    for m in re.finditer(r"sent: ([0-9a-fA-F]{64})", p.read_text()):
        who[m.group(1).lower()] = ("user (ferrata)", p.stem)
for c in jsonl("miniapp/chain-calls.jsonl"):
    r = (c.get("reply") or {}).get("result") or {}
    if c.get("method") == "module.submit" and r.get("tx_hash"):
        who[str(r["tx_hash"]).lower()] = ("user (mini-app via ferrata_bridge_module)", r.get("what", ""))

lez = tsv("lez/transactions.tsv")
try:
    t0 = json.loads((run / "lez/session-a.json").read_text())["tip"]["id"]
except (OSError, ValueError, KeyError):
    t0 = -1
for t in lez:
    h = t.get("tx_hash", "").lower()
    if h not in who and int(t.get("block", "0") or 0) <= t0:
        who[h] = ("operator", "session A (crown-a): the programs' deployment and the light-client instance")
eth = tsv("eth/transactions.tsv")
index = [l.split("\t") for l in lines("user/INDEX")]

md = [f"# seqnet run `{run.name}`", ""]
md += [l for l in lines("run.log") if re.match(r"^\[\d\d:\d\d:\d\d\] seqnet ", l)][:1]
md += ["", "## LEZ transactions (the zone's every applied transaction, `seqnet-txs`)", "",
       "| block | transaction | kind | program | bridge call (visible calls of a private one) | reverted | sent by |",
       "|---:|---|---|---|---|---|---|"]
for t in lez:
    h = t.get("tx_hash", "").lower()
    w = who.get(h, ("", ""))
    call = t.get("bridge_variant", "-") if t.get("kind") == "public" else t.get("visible_bridge_calls", "-")
    md.append(f"| {t.get('block')} | `{short(h)}` | {t.get('kind')} | {t.get('program')} | {call} | {t.get('reverted')} | "
              f"{w[0]}{': ' + w[1] if w[1] else ''} |")
md += ["", "## Ethereum transactions (anvil, `ethtxs.py`)", "",
       "| block | transaction | from | to | call | status | gas |", "|---:|---|---|---|---|---|---:|"]
for t in eth:
    md.append(f"| {t.get('block')} | `{short(t.get('tx_hash'))}` | {t.get('from')} | {t.get('to')} | {t.get('call')} | "
              f"{t.get('status')} | {t.get('gas_used')} |")
if index:
    md += ["", "## What the user ran and saw (`user/`: the tool's own output, one file per command)", "",
           "| file | exit | seconds | first line of the report |", "|---|---:|---:|---|"]
    for row in index:
        name = row[0]
        body = lines(f"user/{name}.txt")
        first = next((l for l in body[2:] if l.strip()), "")
        md.append(f"| [`{name}.txt`](user/{name}.txt) | {row[1].replace('exit ', '') if len(row) > 1 else ''} | "
                  f"{row[2].replace(' s', '') if len(row) > 2 else ''} | {first[:140].replace('|', '/')} |")
calls = jsonl("miniapp/calls.jsonl")
if calls:
    md += ["", "## What the view called (`miniapp/calls.jsonl`: logosctl call ferrata_bridge_module wallet)", "",
           "| # | view method | ok | ms | reply (excerpt) |", "|---:|---|---|---:|---|"]
    for i, c in enumerate(calls):
        req = c.get("request") if isinstance(c.get("request"), dict) else {}
        rep = c.get("reply") if isinstance(c.get("reply"), dict) else {}
        excerpt = json.dumps(rep.get("result") if rep.get("ok") else rep.get("error"))[:120].replace("|", "/")
        md.append(f"| {i + 1} | {req.get('method', c.get('method'))} | {rep.get('ok')} | {c.get('ms')} | {excerpt} |")
shots = sorted((run / "miniapp" / "screenshots").glob("*.png")) if (run / "miniapp" / "screenshots").is_dir() else []
if shots:
    md += ["", "## What the user saw (the view's screenshots)", ""] + [f"- [`{s.name}`](miniapp/screenshots/{s.name})" for s in shots]
md += ["", "## The run's log", "", "```text"] + [l[:300] for l in lines("run.log")] + ["```", ""]
(run / "SUMMARY.md").write_text("\n".join(md) + "\n")
(run / "summary.json").write_text(json.dumps({
    "lez_transactions": len(lez), "lez_reverted": sum(1 for t in lez if t.get("reverted") == "true"),
    "eth_transactions": len(eth), "eth_reverted": sum(1 for t in eth if t.get("status") != "ok"),
    "user_commands": len(index), "view_calls": len(calls), "screenshots": len(shots),
    "senders": {k: v[0] for k, v in who.items()},
}, indent=1) + "\n")
print(f"{run.name}: {len(lez)} LEZ and {len(eth)} Ethereum transactions, {len(index)} user commands, {len(calls)} view calls")
