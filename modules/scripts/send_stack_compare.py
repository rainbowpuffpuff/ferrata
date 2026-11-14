#!/usr/bin/env python3
"""Compare each daemon transaction sent the plain way with the same one sent through Logos Core's tx_sender_module.

    send_stack_compare.py RUN_DIR > compare.md      (also writes RUN_DIR/compare.json)

RUN_DIR is `send-stack.sh run`'s OUT/run: per step a `plain/` and a `logos/` directory (the two branches from one
anvil snapshot), each with `txs.jsonl` (every transaction mined in the step: `cast tx` and `cast receipt` JSON),
`probe.json` (the vault's state after it) and the daemon's own report (`reply.json`; the monitor's `alerts.jsonl`).
`selectors.json` (forge's methodIdentifiers) and `events.txt` (topic0 and signature) name calls and events.

Equal on both paths (checked): the call (to, input, value, from, nonce, chain), the receipt's status, gasUsed and
logs (address, topics, data), the vault's state after the step. Different by construction and reported: the hash,
the gas limit, the fee fields (each path prices the transaction its own way: alloy's estimate and fee filler, or
fee_module), the cost. Exit 1 when an equality fails or a step is missing a side.
"""
import json
import pathlib
import sys

R = pathlib.Path(sys.argv[1])


def jl(p):
    return [json.loads(x) for x in p.read_text().splitlines() if x.strip()] if p.exists() else []


sel = {}
if (R / "selectors.json").exists():
    sel = {("0x" + v).lower(): k for k, v in json.loads((R / "selectors.json").read_text()).items()}
events = {}
if (R / "events.txt").exists():
    for line in (R / "events.txt").read_text().splitlines():
        h, _, sig = line.partition(" ")
        events[h.lower()] = sig.split("(")[0]


def num(v):
    if v is None:
        return None
    if isinstance(v, int):
        return v
    v = str(v)
    return int(v, 16) if v.startswith("0x") else int(v)


def call_name(inp):
    return sel.get((inp or "")[:10].lower(), (inp or "")[:10]).split("(")[0]


def logs_of(rc):
    return [
        {"address": l["address"].lower(), "topics": [t.lower() for t in l["topics"]], "data": l["data"].lower()}
        for l in rc.get("logs", [])
    ]


def via_of(side):
    """The path each transaction of a branch went by, from the daemon's own report."""
    out = []
    alerts = jl(side / "alerts.jsonl")
    for a in alerts:
        if a.get("tx") and a.get("via"):
            out.append({"tx": a["tx"].lower(), "kind": a.get("kind"), "via": a["via"]})
    if (side / "reply.json").exists():
        try:
            rep = json.loads((side / "reply.json").read_text())
        except json.JSONDecodeError:
            rep = {}
        for e in rep.get("events", []) if isinstance(rep, dict) else []:
            line = e if isinstance(e, dict) else {}
            if isinstance(e, str):
                try:
                    line = json.loads(e)
                except json.JSONDecodeError:
                    line = {}
            for holder in (line, line.get("result") if isinstance(line.get("result"), dict) else {}):
                if isinstance(holder, dict) and holder.get("tx") and holder.get("via"):
                    out.append({"tx": str(holder["tx"]).lower(), "kind": holder.get("kind") or line.get("event"),
                                "via": holder["via"]})
    return out


steps, failures = [], 0
for d in sorted(p for p in R.iterdir() if p.is_dir() and (p / "plain").is_dir() and (p / "logos").is_dir()):
    a, b = jl(d / "plain" / "txs.jsonl"), jl(d / "logos" / "txs.jsonl")
    pa = json.loads((d / "plain" / "probe.json").read_text()) if (d / "plain" / "probe.json").exists() else None
    pb = json.loads((d / "logos" / "probe.json").read_text()) if (d / "logos" / "probe.json").exists() else None
    vias = {"plain": via_of(d / "plain"), "logos": via_of(d / "logos")}
    pairs = []
    ok = bool(a) and len(a) == len(b)
    for x, y in zip(a, b):
        tx, ty, rx, ry = x["tx"], y["tx"], x["receipt"], y["receipt"]
        same = {
            "to": (tx.get("to") or "").lower() == (ty.get("to") or "").lower(),
            "input": (tx.get("input") or "").lower() == (ty.get("input") or "").lower(),
            "value": num(tx.get("value")) == num(ty.get("value")),
            "from": tx["from"].lower() == ty["from"].lower(),
            "nonce": num(tx.get("nonce")) == num(ty.get("nonce")),
            "chainId": num(tx.get("chainId")) == num(ty.get("chainId")),
            "status": num(rx.get("status")) == num(ry.get("status")) == 1,
            "gasUsed": num(rx.get("gasUsed")) == num(ry.get("gasUsed")),
            "logs": logs_of(rx) == logs_of(ry),
            "block": num(rx.get("blockNumber")) == num(ry.get("blockNumber")),
        }
        side = lambda t, r: {
            "hash": t["hash"],
            "nonce": num(t.get("nonce")),
            "type": num(t.get("type")),
            "gasLimit": num(t.get("gas")),
            "gasUsed": num(r.get("gasUsed")),
            "maxFeePerGas": num(t.get("maxFeePerGas")),
            "maxPriorityFeePerGas": num(t.get("maxPriorityFeePerGas")),
            "effectiveGasPrice": num(r.get("effectiveGasPrice")),
            "costWei": num(r.get("gasUsed")) * num(r.get("effectiveGasPrice") or 0),
            "block": num(r.get("blockNumber")),
            "events": [events.get(l["topics"][0], l["topics"][0][:10]) for l in logs_of(r) if l["topics"]],
        }
        pairs.append({
            "call": call_name(tx.get("input")),
            "to": tx.get("to"),
            "inputBytes": (len(tx.get("input") or "0x") - 2) // 2,
            "equal": same,
            "plain": side(tx, rx),
            "logos": side(ty, ry),
        })
        ok = ok and all(same.values())
    state_equal = pa == pb and pa is not None
    ok = ok and state_equal
    on_logos = all(any(v["tx"] == p["logos"]["hash"].lower() and v["via"].get("path") == "logos" for v in vias["logos"])
                   for p in pairs)
    on_plain = all(any(v["tx"] == p["plain"]["hash"].lower() and v["via"].get("path") == "plain" for v in vias["plain"])
                   for p in pairs)
    ok = ok and on_logos and on_plain
    failures += 0 if ok else 1
    steps.append({"step": d.name, "ok": ok, "transactions": {"plain": len(a), "logos": len(b)}, "stateEqual": state_equal,
                  "viaReported": {"plain": on_plain, "logos": on_logos}, "pairs": pairs, "via": vias,
                  "state": {"plain": pa, "logos": pb}})

fallback = R / "F-fallback"
fb = None
if fallback.is_dir():
    t = jl(fallback / "txs.jsonl")
    fb = {"transactions": len(t), "calls": [call_name(x["tx"].get("input")) for x in t],
          "status": [num(x["receipt"].get("status")) for x in t], "via": via_of(fallback)}

json.dump({"steps": steps, "fallback": fb}, open(R / "compare.json", "w"), indent=1)

print("| step | call | equal: to, input, value, from, nonce, status, gasUsed, logs, block; state after | events | gas used | "
      "plain: gas limit, effective gas price | Logos (tx_sender_module): gas limit, effective gas price | path reported |")
print("|---|---|---|---|---|---|---|---|")
for s in steps:
    for p in s["pairs"]:
        eq = "all equal" if all(p["equal"].values()) and s["stateEqual"] else \
            "DIFFERENT: " + ", ".join([k for k, v in p["equal"].items() if not v] + ([] if s["stateEqual"] else ["state"]))
        via = f"plain {'yes' if s['viaReported']['plain'] else 'NO'}, logos {'yes' if s['viaReported']['logos'] else 'NO'}"
        print(f"| {s['step']} | `{p['call']}` ({p['inputBytes']} B) | {eq} | {', '.join(p['logos']['events'])} | "
              f"{p['logos']['gasUsed']:,} | {p['plain']['gasLimit']:,}, {p['plain']['effectiveGasPrice']:,} wei | "
              f"{p['logos']['gasLimit']:,}, {p['logos']['effectiveGasPrice']:,} wei | {via} |")
    if not s["pairs"]:
        print(f"| {s['step']} | (no transaction: plain {s['transactions']['plain']}, logos {s['transactions']['logos']}) | "
              f"DIFFERENT | | | | | |")
if fb:
    print()
    print(f"Fallback (tx_sender_module unloaded, the watcher key held): {fb['transactions']} transaction(s) "
          f"{', '.join(fb['calls'])}, status {fb['status']}, reported as {json.dumps([v['via'] for v in fb['via']])}.")
print()
print(f"{len(steps) - failures} of {len(steps)} steps equal on both paths.")
sys.exit(1 if failures else 0)
