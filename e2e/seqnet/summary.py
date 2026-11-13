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
for c in jsonl("lezcore/chain-calls.jsonl"):
    r = (c.get("reply") or {}).get("result") or {}
    if c.get("method") == "module.submit" and r.get("tx_hash"):
        who[str(r["tx_hash"]).lower()] = ("user (ferrata_bridge_module; sent by seqnet-chain)", r.get("what", ""))
for c in jsonl("lezcore/calls.jsonl"):
    r = (c.get("reply") or {}).get("result") or {}
    if c.get("method") in ("claim", "burn") and isinstance(r, dict) and r.get("tx_hash"):
        via = (r.get("lez") or {}).get("via", "")
        who[str(r["tx_hash"]).lower()] = (f"user (ferrata_bridge_module; executed, proven and sent by {'Logos Core’s lez_core' if via == 'lez_core' else 'seqnet-chain'})", r.get("what", ""))

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
lcalls = jsonl("lezcore/calls.jsonl")
if lcalls:
    md += ["", "## What the module was called with (`lezcore/calls.jsonl`: logosctl call ferrata_bridge_module wallet)", "",
           "| # | method | ok | ms | reply (excerpt) |", "|---:|---|---|---:|---|"]
    for i, c in enumerate(lcalls):
        rep_ = c.get("reply") if isinstance(c.get("reply"), dict) else {}
        res = rep_.get("result")
        if isinstance(res, dict):
            res = {k: v for k, v in res.items() if k not in ("lez", "accounts", "calls", "log", "detail")}
        excerpt = json.dumps(res if rep_.get("ok") else rep_.get("error"))[:140].replace("|", "/")
        md.append(f"| {i + 1} | {c.get('method')} | {rep_.get('ok')} | {c.get('ms')} | {excerpt} |")
try:
    lr = json.loads((run / "lezcore" / "lez-report.json").read_text())
except (OSError, ValueError):
    lr = {}
if lr.get("log"):
    md += ["", "## Through Logos Core's `lez_core`, beside the plain path (`lezcore/lez-report.json`)", "",
           f"Reads: {lr.get('reads')} bridge actor states through `get_account_public`, {lr.get('reads_equal')} equal to seqnet-chain's "
           f"({lr.get('reads_failed')} failed); private holding: {lr.get('private_reads')} reads after `sync_to_block`, "
           f"{lr.get('private_reads_equal')} equal; private sends: {lr.get('sends')} attempts, {lr.get('sends_sent')} sent.", "",
           "| kind | what | through lez_core | beside the plain path | ms |", "|---|---|---|---|---:|"]
    for e in lr["log"]:
        k = e.get("kind")
        if k == "read":
            md.append(f"| read | {e.get('what')} `{short(e.get('account', ''))}` | {e.get('bytes', '-')} B | "
                      f"{'equal' if e.get('equal') else ('ERROR ' + str(e.get('error'))[:80] if e.get('error') else 'DIFFERS')} | {e.get('ms', '')} |")
        elif k == "private":
            md.append(f"| private holding | `{short(e.get('account', ''))}` after sync to {e.get('head')} | {e.get('bytes')} B | "
                      f"{'equal' if e.get('equal') else 'DIFFERS'} | {(e.get('ms') or {}).get('sync', '')} + {(e.get('ms') or {}).get('read', '')} |")
        elif k == "send":
            r = e.get("reply") or {}
            md.append(f"| private send | {e.get('what')}: {e.get('attempt')} | "
                      f"{'sent ' + short(r.get('tx_hash', '')) if r.get('success') else 'refused: ' + str(r.get('error') or r.get('raw'))[:90]} | "
                      f"{'(seqnet-chain read the block that applied it)' if r.get('success') else '-'} | {e.get('ms')} |")
        elif k == "poll":
            md.append(f"| poll_transaction_status | `{short(e.get('tx_hash', ''))}` | {e.get('found')} | - | {e.get('ms', '')} |")
shots = sorted((run / "miniapp" / "screenshots").glob("*.png")) if (run / "miniapp" / "screenshots").is_dir() else []
if shots:
    md += ["", "## What the user saw (the view's screenshots)", ""] + [f"- [`{s.name}`](miniapp/screenshots/{s.name})" for s in shots]
md += ["", "## The run's log", "", "```text"] + [l[:300] for l in lines("run.log")] + ["```", ""]
(run / "SUMMARY.md").write_text("\n".join(md) + "\n")
(run / "summary.json").write_text(json.dumps({
    "lez_transactions": len(lez), "lez_reverted": sum(1 for t in lez if t.get("reverted") == "true"),
    "eth_transactions": len(eth), "eth_reverted": sum(1 for t in eth if t.get("status") != "ok"),
    "user_commands": len(index), "view_calls": len(calls), "screenshots": len(shots), "module_calls": len(lcalls),
    "lez_core": {k: lr.get(k) for k in ("reads", "reads_equal", "reads_failed", "private_reads", "private_reads_equal", "sends", "sends_sent")} if lr else None,
    "senders": {k: v[0] for k, v in who.items()},
}, indent=1) + "\n")
print(f"{run.name}: {len(lez)} LEZ and {len(eth)} Ethereum transactions, {len(index)} user commands, {len(calls)} view calls")
