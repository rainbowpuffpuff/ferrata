#!/usr/bin/env python3
"""M0 check 2, the gas drop, on one LEZ tag, read from its session's records.

    python3 -I lez/scripts/gas-drop-summary.py lez/results/v0.4.0-rc3/gas-drop [--write] [--session NAME]

Reads, never measures. From the session (default `session/`; lez/live/run.sh run with the hook
lez/live/hooks/gasdrop.sh, whose records are in `<session>/hook/`): the session's tag, LEZ commit, node and whether
its sequencer ran without RISC0_DEV_MODE; the claimant's balances before (its wallet, and the chain); the negative
control (the same claim with no credit: refused by the wallet, nothing sent); the gas-drop receipt (proven for real,
verified) and the relayer's decision (ferrata_cli::relayer::gas_drop) and refusals; the relayer's credit and the
claim (manifest rows: status, block, proving time, sizes, fee; Bedrock status; the time the sequencer's log says the
block became final), the credit as the claimant's wallet read it (sealed, no recovery binding), the claim's dry run
(the credit claimed before the root, the one-time account paying), the balances and the accounting after. Prints the
verdict; --write also writes summary.json beside the records, which scripts/m0.py reads for check 2. Exit 0 when the
gas drop passed. Standard library only.

It passes when every one of these holds (`checks` in the summary, each with what was read):
  session      the tag's own sequencer, LEZ tree unmodified, without RISC0_DEV_MODE
  zero_before  the claimant's private accounts hold no native balance (its wallet), its public account and the two
               private ids hold 0 on the chain, and nothing is pending to it
  negative     the same claim before the credit: refused by the wallet before sending, for want of the fee
  receipt      the gas-drop receipt proven for real and verified; its N the claim's
  relayer      the relayer's built gas_drop sent one credit for N; a second drop and another relayer refused
  sealed       the credit applied with a real proof in a final block; the claimant's wallet reads it as a sealed
               publication of exactly the amount, with no recovery binding at the one-time address
  claim        ClaimPrivately applied with a real proof in a block final on Bedrock; its dry run claimed that credit
               (1 claim, the amount swept, the one-time account paying); the nullifier spent; the credit spent
  accounting   credit = the one-time account's note after + the refund pending to it + the fee charged
"""

import csv
import datetime
import json
import os
import re
import sys


def rows(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter="\t"))


def text(path):
    return open(path, errors="replace").read() if os.path.exists(path) else ""


def load(path):
    return json.load(open(path)) if os.path.exists(path) else None


def number(pattern, s, kind=float):
    m = re.search(pattern, s or "")
    return kind(m.group(1)) if m else None


def finalized_at(log):
    """(first block, last block, unix seconds) of every 'Channel update: ... finalized A..=B' line."""
    out = []
    for line in log.splitlines():
        m = re.match(r"^\[(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)Z .*finalized (\d+)\.\.=(\d+)", line)
        if m:
            t = datetime.datetime.strptime(m.group(1), "%Y-%m-%dT%H:%M:%S").replace(tzinfo=datetime.timezone.utc)
            out.append((int(m.group(2)), int(m.group(3)), t.timestamp()))
    return out


def session_facts(d):
    env = text(os.path.join(d, "environment.md"))
    proc = text(os.path.join(d, "config", "sequencer-process.txt"))
    m = re.search(r"LEZ: (v[\w.\-]+), commit ([0-9a-f]{40}), source tree (\w+)", env)
    node = re.search(r'"release_tag": "([^"]+)"', env)
    sha = re.search(r'"resolved_sha": "([0-9a-f]+)"', env)
    return {
        "tag": m.group(1) if m else None,
        "lez_commit": m.group(2) if m else None,
        "lez_tree": m.group(3) if m else None,
        "node": (node.group(1) if node else None),
        "node_commit": (sha.group(1) if sha else None),
        "sequencer_dev_mode": None if not proc else ("RISC0_DEV_MODE absent" not in proc),
        "started": number(r"started (\S+Z)", env, str),
        "finished": number(r"finished (\S+Z)", env, str),
    }


def tx_item(d, hook, prefix):
    """The manifest row whose step starts with `prefix` (the hook's crown-driver export), with its block's finality."""
    found = [r for r in rows(os.path.join(hook, "export", "run.tsv")) if r["step"].startswith(prefix)]
    if not found:
        return {"found": False}
    r = found[-1]
    note = r.get("note", "")
    block = int(r["block"]) if r.get("block", "-").isdigit() else None
    item = {
        "found": True, "step": r["step"], "status": r["status"], "block": block, "tx_hash": r.get("tx_hash"),
        "exec_gas": r.get("exec_gas"), "stor_gas": r.get("stor_gas"), "fee": r.get("fee"),
        "base_fee_exec": r.get("base_fee_exec"), "base_fee_stor": r.get("base_fee_stor"),
        "prove_and_submit_s": number(r"execute_and_prove and submission ([\d.]+) s", note),
        "tx_bytes": number(r"transaction (\d+) bytes", note, int),
        "receipt_bytes": number(r"receipt (\d+) bytes", note, int),
        "private_actions": number(r"(\d+) private action\(s\)", note, int),
        "real_proof": r["step"].endswith("(real proof)"),
    }
    chain = {int(c["block"]): c for c in rows(os.path.join(d, "chain.tsv")) if c["block"].isdigit()}
    if block not in chain:
        chain = {int(c["block"]): c for c in rows(os.path.join(hook, "export", "chain.tsv")) if c["block"].isdigit()}
    c = chain.get(block)
    item["bedrock_status"] = c["bedrock_status"] if c else None
    item["final_after_s"] = None
    if c and c.get("timestamp_ms", "0").isdigit() and int(c["timestamp_ms"]) > 0:
        published = int(c["timestamp_ms"]) / 1000
        fin = [t for a, b, t in finalized_at(text(os.path.join(d, "logs", "sequencer.log"))) if a <= block <= b]
        item["final_after_s"] = round(fin[0] - published, 1) if fin else None
    item["applied_final_real"] = bool(item["real_proof"] and item["status"] == "applied" and item["bedrock_status"] == "Finalized")
    return item


def num(v):
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


def summarize(root, session="session"):
    d = os.path.join(root, session)
    hook = os.path.join(d, "hook")
    facts = session_facts(d)
    prepare = load(os.path.join(hook, "prepare.json")) or {}
    receipt = load(os.path.join(hook, "receipt-proof.json")) or {}
    relay = load(os.path.join(hook, "relay.json")) or {}
    again = load(os.path.join(hook, "relay-again.json")) or {}
    other = load(os.path.join(hook, "relay-other.json")) or {}
    after_claim = load(os.path.join(hook, "relay-after-claim.json")) or {}
    credit = load(os.path.join(hook, "credit.json")) or {}
    claim = load(os.path.join(hook, "claim.json")) or {}
    request = load(os.path.join(hook, "request.json")) or {}
    credit_row = tx_item(d, hook, "gas drop: the relayer's native credit")
    claim_row = tx_item(d, hook, "gas drop: ClaimPrivately")
    checks = {}

    checks["session"] = {
        "passed": facts["sequencer_dev_mode"] is False and facts["lez_tree"] == "unmodified" and facts["tag"] is not None,
        "read": f"{facts['tag']} ({facts['lez_commit']}, tree {facts['lez_tree']}), node {facts['node']}, "
                f"sequencer RISC0_DEV_MODE {'absent' if facts['sequencer_dev_mode'] is False else 'not shown absent'}",
    }

    b = prepare.get("before", {})
    priv = b.get("private", {})
    chain_b = b.get("chain", {})
    zero = (bool(b) and all(num(priv.get(k, {}).get("native")) == 0 for k in ("main", "oneTime"))
            and all(num(chain_b.get(k)) == 0 for k in ("public", "mainId", "oneTimeId")) and not b.get("pendingMessages"))
    checks["zero_before"] = {
        "passed": zero,
        "read": f"block {b.get('atBlock')}: private account {priv.get('main', {}).get('native')} ({priv.get('main', {}).get('wallet')}), "
                f"one-time account {priv.get('oneTime', {}).get('native')} ({priv.get('oneTime', {}).get('wallet')}) in its wallet; "
                f"on the chain its public account {chain_b.get('public')}, the private ids {chain_b.get('mainId')} and "
                f"{chain_b.get('oneTimeId')}; {len(b.get('pendingMessages') or [])} pending message(s)",
    }

    n = prepare.get("negativeControl", {})
    checks["negative"] = {
        "passed": bool(n.get("refused")) and n.get("refusedBy") == "wallet" and not n.get("transactionWritten")
                  and bool(n.get("reasonIsFee")),
        "read": (n.get("error") or "")[:400],
        "dry_run": n.get("dryRun"),
    }

    j = receipt.get("journal", {})
    nullifier = prepare.get("nullifier")
    checks["receipt"] = {
        "passed": bool(receipt.get("verified")) and receipt.get("imageId") is not None and j.get("nullifier") == nullifier,
        "read": f"image {receipt.get('imageId')}, {receipt.get('provenS')} s, {receipt.get('userCycles')} user cycles, "
                f"{receipt.get('receiptJsonBytes')} B (JSON), journal N {j.get('nullifier')}",
    }

    checks["relayer"] = {
        "passed": relay.get("decision") == "sent" and again.get("refusal") == "AlreadyDropped"
                  and other.get("refusal") == "OtherRelayer",
        "read": f"{relay.get('decision')} at {relay.get('at')} (tx {relay.get('tx')}); again: {again.get('reason')}; "
                f"another relayer: {other.get('reason')}; after the claim: {after_claim.get('reason')}",
    }

    cb = claim.get("before", {})
    amount = num(claim.get("accounting", {}).get("credit"))
    sealed_ok = (credit_row.get("applied_final_real", False) and bool(cb.get("creditSealed"))
                 and cb.get("recoveryBindingAtOneTimeAddress") is False
                 and num((cb.get("credit") or {}).get("nativeCredit")) == amount and amount is not None
                 and num(relay.get("amount")) == amount)
    checks["sealed"] = {
        "passed": sealed_ok,
        "read": f"credit {credit_row.get('status')} in block {credit_row.get('block')} ({credit_row.get('bedrock_status')}); "
                f"publication {(cb.get('credit') or {}).get('publication')} of {(cb.get('credit') or {}).get('nativeCredit')}; "
                f"recovery binding at the one-time address: {cb.get('recoveryBindingAtOneTimeAddress')}",
    }

    dry = claim.get("dryRun", {})
    after = claim.get("after", {})
    one_time = (load(os.path.join(hook, "keys.json")) or {}).get("oneTime", {}).get("id")
    claim_ok = (claim_row.get("applied_final_real", False) and dry.get("claims") == 1 and num(dry.get("swept")) == amount
                and dry.get("payer") == one_time and one_time is not None and bool(after.get("nullifierSpent"))
                and claim.get("accounting", {}).get("creditStillPending") is False)
    checks["claim"] = {
        "passed": claim_ok,
        "read": f"{claim_row.get('status')} in block {claim_row.get('block')} ({claim_row.get('bedrock_status')}); dry run: "
                f"{dry.get('claims')} claim(s), {dry.get('swept')} swept, payer {dry.get('payer')}, reserve {dry.get('reserve')}; "
                f"nullifier spent {after.get('nullifierSpent')}",
    }

    acc = claim.get("accounting", {})
    checks["accounting"] = {
        "passed": bool(acc.get("creditEqualsNotePlusRefundPlusFee")),
        "read": f"credit {acc.get('credit')} = note after {acc.get('oneTimeNoteAfter')} + refund {acc.get('refundPending')} "
                f"+ fee {acc.get('feeCharged')} (reserve {acc.get('reserve')})",
    }

    passed = all(c["passed"] for c in checks.values())
    return {
        "tag": facts["tag"],
        "session": facts,
        "checks": checks,
        "passed": passed,
        "failed": [k for k, c in checks.items() if not c["passed"]],
        "credit": credit_row,
        "claim": claim_row,
        "accounting": acc,
        "after": {
            "claimant_wrapped_eth": acc.get("wrappedEthToClaimant"),
            "relay_fee_to_relayer": acc.get("relayFeeToRelayer"),
            "one_time_note": acc.get("oneTimeNoteAfter"),
            "refund_pending": acc.get("refundPending"),
            "refund_blinded": acc.get("refundIsBlinded"),
            "public_account_chain": (after.get("chain") or {}).get("public"),
            "main_private_native": ((after.get("private") or {}).get("main") or {}).get("native"),
        },
        "relayer_learned": {
            "at": relay.get("at"),
            "receipt_journal": j,
            "request": {"oneTimeAddress": request.get("oneTimeAddress"), "oneTimeNpk": request.get("oneTimeNpk"),
                        "oneTimeVpkBytes": len((request.get("oneTimeVpk") or "0x")[2:]) // 2},
            "its_credit_tx": credit_row.get("tx_hash"),
            "the_claim_tx": claim_row.get("tx_hash"),
            "the_claim_block": claim_row.get("block"),
        },
        "generated_by": "lez/scripts/gas-drop-summary.py",
    }


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    session = "session"
    if "--session" in sys.argv:
        session = sys.argv[sys.argv.index("--session") + 1]
        args = [a for a in args if a != session]
    if len(args) != 1:
        print(__doc__.split("\n\n")[1])
        sys.exit(2)
    root = args[0]
    s = summarize(root, session)
    print(f"LEZ {s['tag']}: the gas drop {'passed' if s['passed'] else 'did not pass'}"
          + ("" if s["passed"] else f" ({', '.join(s['failed'])})"))
    for k, c in s["checks"].items():
        print(f"  {k}: {c['read']}  [{'passed' if c['passed'] else 'not passed'}]")
    for k in ("credit", "claim"):
        i = s[k]
        if i.get("found"):
            print(f"  {k}: proven and sent in {i['prove_and_submit_s']} s; transaction {i['tx_bytes']} B, receipt "
                  f"{i['receipt_bytes']} B; block {i['block']} final after {i['final_after_s']} s; fee {i['fee']}")
    if "--write" in sys.argv:
        with open(os.path.join(root, "summary.json"), "w") as f:
            json.dump(s, f, indent=1)
            f.write("\n")
    sys.exit(0 if s["passed"] else 1)


if __name__ == "__main__":
    main()
