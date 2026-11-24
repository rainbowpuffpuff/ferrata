"""The run's summary (e2e/endpoints/run.sh): every `set_endpoints` the view sent to ferrata_bridge_module in Logos Core
(the runner's call log, calls.jsonl), what it answered, and the owners' own records read after each step (ops.jsonl).

    python3 -I e2e/endpoints/summary.py RUN_DIR > RUN_DIR/summary.md
"""

import json
import os
import sys


def lines(path):
    if not os.path.exists(path):
        return []
    out = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    out.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
    return out


def cell(s, n=170):
    s = str(s if s is not None else "").replace("|", "/").replace("\n", " ")
    return s if len(s) <= n else s[: n - 1] + "…"


def main():
    run = sys.argv[1]
    calls = lines(os.path.join(run, "calls.jsonl"))
    ops = lines(os.path.join(run, "ops.jsonl"))
    print(f"# endpoints run {os.path.basename(run)}: the summary\n")
    print("Every `wallet set_endpoints` the view sent to `ferrata_bridge_module` inside the headless Logos Core (the "
          "runner's log of its `logosctl call`s), in order; `session` is the deployment the module's session was opened on "
          "(`open`) and its Ethereum path (`eth`). Written by `e2e/endpoints/summary.py` from `calls.jsonl`.\n")
    print("| # | session | Ethereum RPC asked | LEZ sequencer asked | answer | ms |")
    print("|---:|---|---|---|---|---:|")
    session, n, switched, refused = "?", 0, 0, 0
    for c in calls:
        if c.get("method") != "wallet":
            continue
        req = c.get("request") or {}
        rep = c.get("reply") or {}
        m = req.get("method")
        if m == "open":
            session = "chain service " + str((req.get("params") or {}).get("chain", "?"))
        elif m == "eth":
            session += ", Ethereum through " + str((req.get("params") or {}).get("via", "?"))
        if m != "set_endpoints":
            continue
        n += 1
        p = req.get("params") or {}
        if rep.get("ok"):
            s = (rep.get("result") or {}).get("switched") or {}
            what = [k for k in ("eth_rpc", "sequencer") if s.get(k)]
            answer = "switched: " + (", ".join(what) if what else "nothing (in use)")
            switched += 1 if what else 0
        else:
            refused += 1
            answer = "refused: " + str(rep.get("error"))
        print(f"| {n} | {cell(session, 80)} | {cell(p.get('eth_rpc'), 60)} | {cell(p.get('sequencer'), 60)} | {cell(answer, 400)} | {c.get('ms', '')} |")
    print(f"\n{n} calls: {switched} switched an endpoint, {refused} refused (the endpoints in use kept).\n")
    print("The owners' own records after each step (`ops.sh`: the chain service's `module.endpoints`, eth_rpc_module's "
          "`get_chain_config`), in order:\n")
    for o in ops:
        out = str(o.get("output", "")).strip().splitlines()
        print(f"- `{o.get('ops')}` exit {o.get('exit')}: `{cell(out[-1] if out else '', 300)}`")
    t = os.path.join(run, "qmltest.txt")
    if os.path.exists(t):
        with open(t) as f:
            tot = [l.strip() for l in f if l.startswith("Totals") or l.startswith("FAIL") or l.startswith("PASS") or l.startswith("SKIP")]
        print("\nQt Quick Test (`qmltest.txt`):\n")
        for l in tot:
            print(f"- {cell(l, 300)}")


if __name__ == "__main__":
    main()
