"""Check bridge_e2e.sh's replies and write OUT_DIR/bridge_e2e.md and OUT_DIR/bridge_e2e.json.

    python3 -I modules/scripts/bridge_e2e_check.py OUT_DIR

Exits 1 if any check fails.
"""

import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])


def rep(name):
    try:
        return json.loads((out / "rep" / f"{name}.json").read_text())
    except (OSError, ValueError):
        return {}


rows, failures = [], 0


def check(group, what, ok, detail=""):
    global failures
    ok = bool(ok)
    failures += not ok
    rows.append((group, what, "yes" if ok else "NO", detail))


def kinds(r):
    return [a.get("kind") for a in r.get("accounts", [])]


info = rep("info" if (out / "rep" / "info.json").exists() else "dev_info")
d0, d1 = rep("dev_deposit0"), rep("dev_deposit1")
check("devnet", "two deposits to the user's private account /0", d0.get("leaf_index") == 0 and d1.get("leaf_index") == 1,
      f"amount {d0.get('amount')} wei each, vault {info.get('vault')}")

# import_root
b, ir, bad = rep("dev_import_block"), rep("import_root"), rep("import_root_bad")
sb, si, fees = rep("dev_submit_import_bad"), rep("dev_submit_import"), rep("dev_import_fees")
g = "import_root"
check(g, "import_root dry-runs the import with the programs' functions: importable, root(cid, 2) as the vault holds it",
      ir.get("verdict") == "importable" and ir.get("root_state", {}).get("leaf_count") == b.get("leaf_count") == 2
      and ir.get("root_state", {}).get("root") == b.get("root") == ir.get("root"),
      f"record slot {ir.get('record_slot')}, message {ir.get('message_bytes')} bytes")
check(g, "the call names config(cid) (root, its seed), header(id, slot) and root(cid, n)", kinds(ir) == ["pda", "public", "pda"])
check(g, "a wrong leaf count: the dry run refuses with IMPORT_MISMATCH (247)", bad.get("verdict") == "refused" and bad.get("code") == 247,
      f"{bad.get('error')}")
check(g, "submitted, the program refuses it with the same code", sb.get("settled") is False and sb.get("exit_code") == bad.get("code"),
      f"exit {sb.get('exit_code')}")
check(g, "the module's ImportRoot is applied on the zone", si.get("settled") is True, f"gas_exec {si.get('gas_exec')}, gas_stor {si.get('gas_stor')}")
check(g, "(stand-in) the relayer's ImportFees for the same block", fees.get("applied") is True)

# registry
r = rep("registry")
g = "registry"
tok = r.get("token", {})
check(g, "token(cid, ETH) decoded: the deposit's amount is a denomination", d0.get("amount") in tok.get("entry", {}).get("denominations", []),
      f"denominations {tok.get('entry', {}).get('denominations')}, relay fees {tok.get('entry', {}).get('relay_fees')}")
check(g, "a claim quotes only the fixed relay fee and its cap value", "range" not in r
      and "mint_fee" not in r.get("claim", {}) and "relay_fee" in r.get("claim", {}), f"claim {r.get('claim')}")
check(g, "config(cid) decoded; configId recomputed from it equals the devnet's",
      r.get("config", {}).get("phase") == "ready" and r.get("config", {}).get("config_id") == info.get("config_id"),
      f"instance {str(r.get('config', {}).get('instance_id'))[:18]}…")
check(g, "root(cid, 2) decoded", r.get("root", {}).get("leaf_count") == 2 and r.get("root", {}).get("root") == b.get("root"))

# claims
g = "claim (private)"
for k in (0, 1):
    c, s = rep(f"claim{k}"), rep(f"dev_submit_claim{k}")
    check(g, f"deposit {k}: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor",
          c.get("verdict") == "claimable" and c.get("variant") == "ClaimPrivately" and kinds(c)[:1] == ["private_self"]
          and c.get("leaf_count") == 2 and "range" not in c and "mint_fee" not in c,
          f"{c.get('message_bytes')} bytes, {len(c.get('accounts', []))} accounts, relay fee {c.get('relay_fee')}")
    check(g, f"deposit {k}: proven and settled privately on the zone", s.get("settled") is True and s.get("private") is True,
          f"proof {s.get('proof', {}).get('mode')}, prove {s.get('proof', {}).get('prove_ms')} ms, balance {s.get('balance')}")
again = rep("claim0_again")
check(g, "deposit 0 again: refused from the nullifier's account (ALREADY_CLAIMED, 250)", again.get("verdict") == "refused" and again.get("code") == 250)
s1 = rep("dev_submit_claim1")
check(g, "the private account holds both denominations", str(s1.get("balance")) == str(2 * int(d0.get("amount", "0") or 0)), f"balance {s1.get('balance')}")

# releases
g = "release (the wallet's rule)"
ca, cb, re0 = rep("dev_checkpoint_a"), rep("dev_checkpoint_b"), rep("dev_roots_early")
roots = re0.get("roots", [])
check(g, "two accepted roots: over burn 0 (delay passed) and over both (in its delay)",
      len(roots) == 2 and roots[0].get("count") == 1 and roots[1].get("count") == 2 and ca.get("release_delay_passed") is True
      and cb.get("release_delay_passed") is False and re0.get("now", 0) <= roots[1].get("finalized_at", 0) + roots[1].get("delay", 0),
      f"now {re0.get('now')}, opens {[x.get('finalized_at', 0) + x.get('delay', 0) for x in roots]}")
r0, r1e, r1 = rep("release0"), rep("release1_early"), rep("release1")
check(g, "burn 0: the newest root whose delay has passed is the older one", r0.get("verdict") == "releasable" and r0.get("picked", {}).get("index") == roots[0].get("index") if roots else False,
      f"picked checkpoint {r0.get('picked', {}).get('index')}, root {str(r0.get('burn_root'))[:18]}…")
check(g, "burn 1: only the newer root covers it, in its delay: ReleaseDelayNotElapsed, with when it opens",
      r1e.get("error") == "ReleaseDelayNotElapsed" and roots and r1e.get("first_open") == roots[1].get("finalized_at", 0) + roots[1].get("delay", 0),
      f"first open {r1e.get('first_open')}")
check(g, "after the delay: burn 1 under the newer root", r1.get("verdict") == "releasable" and r1.get("picked", {}).get("count") == 2)
p0 = rep("prove_release0")
check("prove_release", "the release guest run on the module's input through r0vm: its journal is the native one",
      p0.get("verdict") == "proven" and p0.get("journal_sha256") == r0.get("journal_sha256"),
      f"image {str(p0.get('image_id'))[:18]}…, dev mode {p0.get('dev_mode')}, {p0.get('prove_ms')} ms, receipt {p0.get('receipt_bytes')} bytes")
v0, v1 = rep("dev_vault_release0"), rep("dev_vault_release1")
check("vault", "both releases the module planned are accepted by the vault (mock verifier on the exact journals)",
      v0.get("status") is True and v1.get("status") is True and int(v0.get("paid_wei", "0")) > 0 and int(v1.get("paid_wei", "0")) > 0,
      f"paid {v0.get('paid_wei')} and {v1.get('paid_wei')} wei")
st = rep("status")
check("status", "both burns released", [x.get("status") for x in st.get("burns", [])] == ["released", "released"])

md = ["| group | check | ok | detail |", "|---|---|---|---|"]
md += [f"| {a} | {b_} | {c_} | {d_} |" for a, b_, c_, d_ in rows]
md.append(f"\n{len(rows) - failures} of {len(rows)} checks passed.")
(out / "bridge_e2e.md").write_text("\n".join(md) + "\n")
mode = (out / "mode.txt").read_text().strip() if (out / "mode.txt").exists() else ""
(out / "bridge_e2e.json").write_text(json.dumps({"mode": mode, "checks": len(rows), "failed": failures,
                                                  "rows": [dict(zip(("group", "check", "passed", "detail"), x)) for x in rows]}, indent=1) + "\n")
print("\n".join(md))
sys.exit(1 if failures else 0)
