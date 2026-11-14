"""verified-proxy-session.sh's plan and summary.

    python3 -I verified_proxy.py plan REPO W      W/plan.json: what the session reads (from the live runs' records)
    python3 -I verified_proxy.py summary REPO W   W/summary.md, W/summary.json: every comparison; exit 1 on a failure

The records compared with: e2e/runs/crown-live-3 and crown-live-1 (their manifests: the deposits' transactions and
blocks; their captures: the vault's storage at the capture blocks, each block's hash and state root and the
eth_getProof the run used), crates/ferrata-spectests/fixtures/sepolia-2026-11-12 (the checkpoint and its light-client
data) and modules/results/verify-checkpoint-2026-11-13 (verify_checkpoint's reply of 13 November).
"""
import json
import pathlib
import sys

RUNS = ("crown-live-3", "crown-live-1")
FIXTURE = "crates/ferrata-spectests/fixtures/sepolia-2026-11-12"


def load(p):
    try:
        return json.loads(pathlib.Path(p).read_text())
    except (OSError, ValueError):
        return None


def plan(repo, w):
    deposits, captures = [], []
    for run in RUNS:
        d = repo / "e2e/runs" / run
        stages = load(d / "manifest.json")["stages"]
        for k in ("deposit-0", "deposit-1"):
            tx = stages[k][0]
            deposits.append({"id": f"{run}-{k}", "run": run, "tx": tx["hash"], "block": int(tx["blockNumber"], 16),
                             "vault": tx["to"].lower(), "status": tx["status"]})
        for c in ("capture-1", "capture-2"):
            v = load(d / "captures" / f"{c}.json")
            proof = next((d / "captures" / c).glob("proof_*.json"))
            captures.append({"id": f"{run}-{c}", "run": run, "block": v["block"], "vault": v["vault"].lower(),
                             "hash": v["proofs"]["hash"], "stateRoot": v["proofs"]["stateRoot"],
                             "slots": [{"slot": s["slot"], "what": s["what"], "value": s["value"]} for s in v["slots"]],
                             "proof_file": str(proof.relative_to(repo))})
    req = load(repo / "modules/ferrata_attest_module/rust-lib/calls/tests/fixtures/verify-checkpoint-sepolia.json")
    req.pop("_comment", None)
    req["bootstrap"] = "0x" + (repo / FIXTURE / "bootstrap.ssz").read_bytes().hex()
    req["timeout_secs"] = 240
    out = {"deposits": deposits, "captures": captures, "verify_checkpoint": req,
           "checkpoint_root": req["checkpoint"], "fixture": FIXTURE}
    (w / "plan.json").write_text(json.dumps(out, indent=1) + "\n")


class Checks:
    def __init__(self):
        self.rows, self.failed = [], 0

    def add(self, ok, what, detail=""):
        self.rows.append({"ok": bool(ok), "what": what, "detail": detail})
        self.failed += 0 if ok else 1


def reply(w, name):
    """A logosctl reply: the method's own JSON (string results parsed), or None."""
    v = load(w / "rep" / f"{name}.json")
    return v


def rpc_result(v):
    """eth_rpc_module.raw_rpc's reply: (ok, result, route, error)."""
    if not isinstance(v, dict):
        return False, None, None, "no reply"
    return v.get("ok") is True, v.get("result"), v.get("route"), v.get("error")


def norm_hex(x):
    return x.lower() if isinstance(x, str) else x


def summary(repo, w):
    p = load(w / "plan.json")
    c = Checks()
    facts = {}

    # 1. The trusted root, and the proxy's sync.
    tr = load(w / "trusted-root.json") or {}
    facts["trusted_root"] = tr
    c.add(tr.get("agree") is True, "trusted block root: Lodestar's finalized header, the block publicnode holds "
          "canonical at that slot and Nimbus's header of that root agree, at or below publicnode's finalized epoch",
          f"root {tr.get('root')}, slot {tr.get('slot')}")
    started = reply(w, "vp-start")
    c.add(isinstance(started, dict) and started.get("ok", started.get("success", True)) is not False
          and "error" not in json.dumps(started).lower()[:200], "verified_proxy_module.start: the light client "
          "bootstrapped from that root and synced once", json.dumps(started)[:300])
    st = [reply(w, f"vp-status-{i}") for i in range(4)]
    st = [s for s in st if isinstance(s, dict)]
    heads = [((s.get("head") or {}).get("number"), (s.get("sync") or {}).get("lastSyncedAt"), s.get("state")) for s in st]
    facts["proxy_status_samples"] = heads
    nums = [h[0] for h in heads if isinstance(h[0], int)]
    syncs = [h[1] for h in heads if h[1] is not None]
    c.add(len(nums) >= 2 and nums[-1] > nums[0] and len(set(syncs)) >= 2 and all(h[2] in ("running", "ready") for h in heads),
          "syncing: the verified head and the last sync time move between status samples, state running",
          f"(head, lastSyncedAt, state) = {heads}")
    beacon = (st[-1].get("beacon") if st else None) or {}
    facts["proxy_beacon"] = beacon
    c.add(beacon.get("source") == "module", "the proxy's beacon providers came from beacon_api_module (beaconSource "
          "module)", json.dumps(beacon)[:300])

    # 2. eth_rpc_module in required mode.
    gate = reply(w, "rpc-gate")
    facts["gate"] = gate
    c.add(isinstance(gate, dict) and gate.get("usable") is True and gate.get("mode") == "required",
          "eth_rpc_module.verified_proxy_status(11155111): required, usable", json.dumps(gate)[:300])
    for d in p["deposits"]:
        v, dd = reply(w, f"V-flp-{d['id']}"), reply(w, f"D-flp-{d['id']}")
        ok = isinstance(v, dict) and v.get("verdict") == "fetched"
        routes = (v or {}).get("eth", {}).get("routes") if isinstance(v, dict) else None
        c.add(ok and routes and all(r in ("proxied", "verified") for r in routes),
              f"{d['id']}: fetch_log_proof through eth_rpc_module in required mode (verified_proxy_module answered "
              f"eth_getBlockByNumber and eth_getBlockReceipts)", f"routes {routes}, verdict {(v or {}).get('verdict')}"
              + ("" if ok else f", error {json.dumps(v)[:300]}"))
        if not ok:
            continue
        pl = v.get("provider_log", {})
        c.add(norm_hex(pl.get("transactionHash")) == d["tx"].lower() and v.get("block") == d["block"]
              and v.get("receipt_status") == d["status"] and norm_hex(pl.get("address")) == d["vault"],
              f"{d['id']}: the log is the run's deposit (transaction, block, status, the vault) as the run's manifest "
              f"records it", f"tx {pl.get('transactionHash')}, block {v.get('block')}")
        same = isinstance(dd, dict) and dd.get("verdict") == "fetched" and all(
            dd.get(k) == v.get(k) for k in ("block_hash", "receipts_root", "receipts", "receipt_status", "log", "provider_log"))
        c.add(same, f"{d['id']}: equal to the same call with the chain's mode off (eth_rpc_module direct to the "
              "endpoint): block hash, receipts root, receipt count, the receipt proof, the log",
              f"block_hash {v.get('block_hash')}, receipts {v.get('receipts')}, proof nodes {len(v['log']['receipt_proof'])}")
        pn = load(w / "plain" / f"pn-hdr-{d['block']}.json") or {}
        c.add(norm_hex((pn.get("result") or {}).get("hash")) == norm_hex(v.get("block_hash")),
              f"{d['id']}: the block hash equals publicnode's (a second provider, plain JSON-RPC)",
              f"{(pn.get('result') or {}).get('hash')}")
    for cap in p["captures"]:
        ok, h, route, err = rpc_result(reply(w, f"V-hdr-{cap['id']}"))
        c.add(ok and norm_hex(h.get("hash")) == cap["hash"].lower() and norm_hex(h.get("stateRoot")) == cap["stateRoot"].lower(),
              f"{cap['id']}: block {cap['block']}'s header through the verified proxy (route {route}): hash and "
              "state root equal to the capture's record", f"{(h or {}).get('hash')} / {(h or {}).get('stateRoot')}"
              + ("" if ok else f" error {err}"))
        vals, labels = [], set()
        for i, s in enumerate(cap["slots"]):
            ok_s, val, route_s, err_s = rpc_result(reply(w, f"V-sto-{cap['id']}-{i}"))
            vals.append((ok_s, val, s["value"], err_s))
            labels.add(route_s)
        good = all(o and norm_hex(x) == y.lower() for o, x, y, _ in vals)
        c.add(good and labels == {"verified"}, f"{cap['id']}: the vault's six slots (eth_getStorageAt, route verified: "
              "the proxy checked an account and storage proof against the verified state root) equal the run's capture",
              f"{sum(1 for o, x, y, _ in vals if o and norm_hex(x) == y.lower())}/6 equal; routes {sorted(map(str, labels))}"
              + ("" if good else f"; first error {next((e for o, _, _, e in vals if not o), None)}"))
        okp, _, _, errp = rpc_result(reply(w, f"V-proof-{cap['id']}"))
        facts.setdefault("eth_getProof_required", {})[cap["id"]] = errp
        c.add(not okp, f"{cap['id']}: eth_getProof in required mode is refused (libverifproxy has no eth_getProof)",
              f"{errp}")
        okd, proof, _, errd = rpc_result(reply(w, f"D-proof-{cap['id']}"))
        rec = (load(repo / cap["proof_file"]) or {}).get("result")
        c.add(okd and proof == rec, f"{cap['id']}: eth_getProof with the mode off (direct) equals the proof the run "
              "used (byte for byte, every field)", "equal" if okd and proof == rec else f"error {errd}")
    vc, dc, hc = reply(w, "V-verify_checkpoint"), reply(w, "D-verify_checkpoint"), reply(w, "H-verify_checkpoint")
    old = load(repo / "modules/results/verify-checkpoint-2026-11-13/reply.json") or {}
    ex = (vc or {}).get("execution", {}) if isinstance(vc, dict) else {}
    facts["verify_checkpoint_execution"] = {k: ex.get(k) for k in ("verdict", "block_hash", "block_number",
                                                                    "finalized_number", "eth", "unavailable")}
    c.add(ex.get("verdict") == "agrees" and all(r in ("proxied", "verified") for r in (ex.get("eth") or {}).get("routes", [None])),
          "verify_checkpoint: the checkpoint's execution block checked through eth_rpc_module in required mode "
          "(verified_proxy_module: canonical by hash and at its number, at or below its finalized block, chain id)",
          f"verdict {ex.get('verdict')}, block {ex.get('block_number')}, finalized {ex.get('finalized_number')}, "
          f"routes {(ex.get('eth') or {}).get('routes')}, unavailable {ex.get('unavailable')}")
    c.add(isinstance(vc, dict) and vc.get("verdict") == "ok" and vc.get("failed") == [],
          "verify_checkpoint: verdict ok (instance id, bootstrap, committee, three beacon providers, execution)",
          f"verdict {(vc or {}).get('verdict') if isinstance(vc, dict) else None}, failed {(vc or {}).get('failed') if isinstance(vc, dict) else None}")
    oex = old.get("execution", {})
    c.add(ex.get("block_number") == oex.get("block_number") and ex.get("block_hash") == oex.get("block_hash"),
          "verify_checkpoint: the same execution block as 13 November's check through publicnode's plain RPC",
          f"{ex.get('block_number')} {ex.get('block_hash')} vs {oex.get('block_number')} {oex.get('block_hash')}")
    for tag, other in (("D", dc), ("H", hc)):
        oe = (other or {}).get("execution", {}) if isinstance(other, dict) else {}
        what = {"D": "with the chain's mode off (direct)", "H": "through the proxy's own JSON-RPC endpoint (curl)"}[tag]
        c.add(oe.get("verdict") == "agrees" and oe.get("block_number") == ex.get("block_number"),
              f"verify_checkpoint's execution check {what} agrees on the same block", f"{oe.get('verdict')} {oe.get('block_number')}")

    # 3. The updater's light-client data through beacon_api_module.
    fx = repo / p["fixture"]
    b = reply(w, "B-bootstrap")
    body = json.loads(b["body"]) if isinstance(b, dict) and b.get("ok") else None
    pj = load(w / "plain" / "nimbus-bootstrap.json")
    c.add(body is not None and body == pj, "beacon_api_module.lc_request getLightClientBootstrap (the fixture's root): "
          "the provider's JSON, equal to Nimbus's own answer", f"provider {(b or {}).get('provider')}, attempts "
          f"{json.dumps((b or {}).get('attempts'))[:200]}")
    u = reply(w, "B-updates")
    ubody = json.loads(u["body"]) if isinstance(u, dict) and u.get("ok") else None
    uj = load(w / "plain" / "lodestar-updates.json")
    c.add(ubody is not None and ubody == uj, "beacon_api_module.lc_request getLightClientUpdatesByRange (1411, 3): "
          "equal to Lodestar's own JSON answer", f"provider {(u or {}).get('provider')}, {len(ubody or [])} updates")
    f = reply(w, "B-finality")
    fbody = json.loads(f["body"]) if isinstance(f, dict) and f.get("ok") else None
    fj = load(w / "plain" / "lodestar-finality.json")
    c.add(fbody is not None and fbody == fj, "beacon_api_module.lc_request getLightClientFinalityUpdate: equal to "
          "Lodestar's own JSON answer at the same signature slot",
          f"signature slot {((fbody or {}).get('data') or {}).get('signature_slot')}")
    conv = load(w / "lc-ssz-compare.json") or {}
    facts["lc_json_to_ssz"] = conv
    c.add(conv.get("all_equal") is True, "those bodies turned into SSZ by the updater's converter (lez/updater "
          "lc_json) equal the SSZ the providers served (and the fixture's bootstrap.ssz and update_1411.ssz)",
          json.dumps(conv.get("rows"))[:400])
    run = reply(w, "U-run")
    facts["updater_run"] = run if isinstance(run, dict) else None
    insts = (run or {}).get("instances") if isinstance(run, dict) else None
    sep = (insts or [{}])[0] if isinstance(insts, list) else (insts or {}).get("sepolia", {}) if isinstance(insts, dict) else {}
    c.add(isinstance(run, dict) and run.get("verdict") in ("ok", "up_to_date", "done") and sep,
          "ferrata_updater_module.run with its sources through beacon_api_module (two providers, min_agreeing 2), on "
          "the in-process LEZ chain: the instance initialised from the fixture's checkpoint and followed Sepolia",
          json.dumps(run)[:500] if isinstance(run, dict) else "no reply")
    fin = load(w / "plain" / "updater-finalized-check.json") or {}
    c.add(fin.get("agree") is True, "the instance's finalized header after the run is the block publicnode holds "
          "canonical at that slot", json.dumps(fin)[:300])

    md = ["# verified-proxy session: summary", "",
          f"Checks: {len(c.rows)}, failed: {c.failed}.", "",
          "| | check | detail |", "|---|---|---|"]
    for r in c.rows:
        md.append(f"| {'PASS' if r['ok'] else 'FAIL'} | {r['what']} | {r['detail'].replace('|', '/')} |")
    (w / "summary.md").write_text("\n".join(md) + "\n")
    (w / "summary.json").write_text(json.dumps({"checks": c.rows, "failed": c.failed, "facts": facts}, indent=1) + "\n")
    print(f"{len(c.rows)} checks, {c.failed} failed")
    return c.failed


if __name__ == "__main__":
    cmd, repo, w = sys.argv[1], pathlib.Path(sys.argv[2]).resolve(), pathlib.Path(sys.argv[3]).resolve()
    if cmd == "plan":
        plan(repo, w)
    elif cmd == "summary":
        sys.exit(1 if summary(repo, w) else 0)
    else:
        sys.exit(f"{cmd}: plan or summary")
