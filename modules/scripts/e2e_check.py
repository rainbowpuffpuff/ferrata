#!/usr/bin/env python3
"""Check e2e.sh's replies and write OUT_DIR/e2e.md and OUT_DIR/e2e.json.

    python3 -I modules/scripts/e2e_check.py OUT_DIR BRIDGE_VECTORS_DIR

Exits 1 if any check fails.
"""

import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])
vectors = pathlib.Path(sys.argv[2])
fixture = json.loads((pathlib.Path(__file__).resolve().parents[2]
                      / "crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/fixture.json").read_text())


def rep(name):
    try:
        return json.loads((out / "rep" / f"{name}.json").read_text())
    except (OSError, ValueError):
        return {}


rows, failures = [], 0


def check(group, what, ok, detail=""):
    global failures
    failures += not ok
    rows.append((group, what, "yes" if ok else "NO", detail))


info = rep("info")
check("chain", "harness info read", bool(info.get("instance_id")), f"instance {info.get('instance_id', '')[:18]}…, block {info.get('block')}, slot {info.get('finalized_slot')}")

ri = rep("read_instance")
check("chain", "read_instance decodes the instance's state; the id recomputed from its config and checkpoint",
      ri.get("verdict") == "ok" and ri.get("instance_matches") is True and ri.get("id_matches_config") is True
      and ri.get("active") is True and ri.get("config", {}).get("chain_id") == info.get("chain_id")
      and ri.get("store", {}).get("finalized_slot") == info.get("finalized_slot"),
      f"chain {ri.get('config', {}).get('chain_id')}, finalized slot {ri.get('store', {}).get('finalized_slot')}, "
      f"{ri.get('applied')} jobs applied, store {ri.get('store', {}).get('fork')}")

cases = {"transfer": fixture["transfer"], "largest_receipt": fixture["transfer_in_largest_receipt"]}
for name, want in cases.items():
    f, d, t, e, r = rep(f"fetch_{name}"), rep(f"dry_{name}"), rep(f"tx_{name}"), rep(f"exec_{name}"), rep(f"read_{name}")
    g = f"log {name}"
    check(g, "fetch_log_proof over JSON-RPC: receipts rebuild the block's receiptsRoot", f.get("verdict") == "fetched",
          f"{f.get('receipts')} receipts, {len(f.get('log', {}).get('receipt_proof', []))} proof nodes, {f.get('rpc_calls')} RPC calls")
    check(g, "the log located is the fixture's", (f.get("log", {}).get("tx_index"), f.get("log", {}).get("log_index")) == (want["tx_index"], want["log_index"])
          and f.get("provider_log", {}).get("address", "").lower() == want["token"], f"tx {want['tx_index']}, log {want['log_index']}, token {want['token']}")
    fact = d.get("fact", {})
    check(g, "dry_run_attest on the record read from the chain: attested, code 0", d.get("verdict") == "attested" and d.get("code") == 0, f"id {d.get('id', '')[:18]}…")
    pl = f.get("provider_log", {})
    check(g, "the attested fact is the provider's log", fact.get("address") == pl.get("address", "").lower()
          and fact.get("topics") == [x.lower() for x in pl.get("topics", [])] and fact.get("data") == pl.get("data", "").lower(),
          f"{len(fact.get('topics', []))} topics, data {fact.get('data', '')[:20]}…")
    check(g, "build_attest_tx names header(id, slot)", t.get("account") == info.get("header_account"), f"message {t.get('message_bytes')} bytes")
    check(g, "the program decodes the module's bytes (canonical Borsh)", e.get("decoded") is True and e.get("canonical") is True)
    check(g, "the transaction is applied", e.get("applied") is True and e.get("exit_code") is None,
          f"gas_exec {e.get('gas_exec')}, gas_stor {e.get('gas_stor')}, tx {e.get('tx_bytes')} bytes")
    check(g, "the program's event = native evaluation", e.get("event_matches_native") is True and e.get("event_emitter_is_program") is True)
    check(g, "the program's statement ids = the module's dry-run ids", e.get("native", {}).get("ids") == d.get("ids") == [d.get("id")])
    check(g, "read_attestation: event matches the dry run and the instance", r.get("matches_expected") is True and r.get("instance_matches") is True
          and r.get("block_number") == info.get("block"), f"block {r.get('block_number')}")

d, t, e, r = rep("dry_storage"), rep("tx_storage"), rep("exec_storage"), rep("read_storage")
g = "storage slot (WETH balance)"
check(g, "dry_run_attest: attested, the provider's value", d.get("verdict") == "attested" and d.get("code") == 0 and d.get("provider_value_matches") is True,
      f"value {d.get('fact', {}).get('value', '')[:20]}…")
check(g, "build_attest_tx names header(id, slot); two facts (the account, then the slot)", t.get("account") == info.get("header_account") and t.get("facts") == 2,
      f"message {t.get('message_bytes')} bytes")
check(g, "decoded (canonical), applied, event = native = dry-run ids (account, slot)", e.get("decoded") is True and e.get("canonical") is True
      and e.get("applied") is True and e.get("event_matches_native") is True and e.get("native", {}).get("ids") == d.get("ids")
      and len(d.get("ids", [])) == 2 and d["ids"][1] == d.get("id"),
      f"gas_exec {e.get('gas_exec')}, gas_stor {e.get('gas_stor')}")
check(g, "read_attestation matches", r.get("matches_expected") is True and r.get("instance_matches") is True)

d, e = rep("dry_tampered"), rep("exec_tampered")
check("refusal: tampered receipt proof", "dry run refuses", d.get("verdict") == "refused", f"code {d.get('code')} {d.get('error')}")
check("refusal: tampered receipt proof", "the program refuses with the predicted code", e.get("applied") is False and e.get("exit_code") == d.get("code"),
      f"exit {e.get('exit_code')}, gas_exec {e.get('gas_exec')}")
e = rep("exec_wrong_instance")
check("refusal: another instance", "the program refuses with WRONG_INSTANCE (211)", e.get("applied") is False and e.get("exit_code") == 211,
      f"exit {e.get('exit_code')}")

ob = json.loads((vectors / "outbound-v2.json").read_text())["cases"][0]
b, db = rep("bridge_burn"), rep("decode_burn")
fields = db.get("fields", {})
note = b.get("note", {})
check("bridge Burn", "the burn note equals the specification vector", note.get("r") == ob["r"] and note.get("memo") == ob["memo"]
      and note.get("mac") == ob["mac"] and note.get("c_out") == ob["C_out"], "r, memo, mac, C_out")
check("bridge Burn", "leaf, burn id and nullifier equal the vector", b.get("burn_leaf") == ob["burn_leaf"] and b.get("burn_id") == ob["burn_id"]
      and b.get("nullifier") == ob["N_out"])
check("bridge Burn", "the bridge program decodes the message (canonical) to the same note",
      db.get("decoded") is True and db.get("canonical") is True and fields.get("variant") == "Burn"
      and all(fields.get(k) == note.get(k) for k in ("r", "memo", "mac", "token", "amount", "c_out")), f"{db.get('message_bytes')} bytes")

ic = json.loads((vectors / "inbound-v2.json").read_text())["claim"]
c, dc = rep("bridge_claim"), rep("decode_claim")
fields = dc.get("fields", {})
check("bridge Claim", "claimable; nullifier and root equal the vector", c.get("verdict") == "claimable" and c.get("nullifier") == ic["N"] and c.get("root") == ic["root"])
check("bridge Claim", "the path equals the vector's", c.get("path") == ic["path"])
check("bridge Claim", "the bridge program decodes the message (canonical) to the same witness",
      dc.get("decoded") is True and dc.get("canonical") is True and fields.get("variant") == "Claim"
      and fields.get("leaf_index") == ic["leaf_index"] and fields.get("kind") == ic["kind"] and fields.get("dest") == ic["dest"]
      and fields.get("depositor") == ic["depositor"] and fields.get("amount") == ic["amount"] and fields.get("path_len") == 32
      and fields.get("leaf_count") == len(ic["leaves"]) and fields.get("range_covers_leaf") is True, f"{dc.get('message_bytes')} bytes")

rpc = [json.loads(line) for line in (out / "rpc.log").read_text().splitlines() if line.strip()]
methods = {}
for x in rpc:
    methods[x["method"]] = methods.get(x["method"], 0) + 1
check("JSON-RPC", "every read went over the network to the recorded-data server", methods.get("eth_getBlockReceipts", 0) >= 2,
      ", ".join(f"{k} x{v}" for k, v in sorted(methods.items())) + f", {sum(x['bytes'] for x in rpc):,} bytes served")

mode = (out / "mode.txt").read_text().strip()
lines = [f"# e2e.sh ({mode})", "", "| group | check | passed | detail |", "|---|---|---|---|"]
lines += [f"| {g} | {w} | {ok} | {dt} |" for g, w, ok, dt in rows]
lines += ["", f"Checks: {len(rows)}, failed: {failures}.", ""]
(out / "e2e.md").write_text("\n".join(lines))
(out / "e2e.json").write_text(json.dumps({"mode": mode, "checks": len(rows), "failed": failures,
                                          "rows": [dict(zip(("group", "check", "passed", "detail"), r)) for r in rows]}, indent=1))
print("\n".join(lines))
sys.exit(1 if failures else 0)
