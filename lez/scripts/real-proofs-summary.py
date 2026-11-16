#!/usr/bin/env python3
"""M0 check 4's four real proofs on one LEZ tag, read from the sessions' records.

    python3 -I lez/scripts/real-proofs-summary.py lez/results/v0.4.0-rc3/real-proofs [--write]

Reads, never measures: for each of the four private transactions (the private update, the private attestation, the
private claim, the private burn) the session's manifest row (status, block, transaction hash, proving time, transaction
size, receipt size), the block's Bedrock status in the session's chain.tsv and the time the sequencer's log says the
block became final, and the session's evidence that its sequencer verified receipts (RISC0_DEV_MODE absent from the
sequencer's environment; the fake receipt dropped). Prints one line per transaction; --write also writes summary.json
beside the records, which scripts/m0.py reads. Standard library only.

The records (lez/results/<tag>/real-proofs/README.md): update/ (lez/live/run.sh run, phase private-real),
attest-claim-burn/ (phases attest-private and hook; the hook's crown-driver records in attest-claim-burn/crown/).
"""

import csv
import datetime
import json
import os
import re
import sys

ITEMS = [
    # key, what, session, manifest (relative to the session), row matcher
    ("update", "private update: VerifyAll at a private account's actor advances the public instance", "update", "run.tsv",
     lambda r: r["phase"] == "private" and r["step"].startswith("private update for the live instance, real proof")),
    ("attestation", "private attestation: the reference consumer's AttestPrivately reads a record", "attest-claim-burn",
     "run.tsv", lambda r: r["phase"] == "attest-private" and r["step"].startswith("private attestation")),
    ("claim", "private claim: the bridge's ClaimPrivately of deposit 1", "attest-claim-burn", "crown/export/run.tsv",
     lambda r: "ClaimPrivately" in r["step"] and r["step"].endswith("(real proof)")),
    ("burn", "private burn: the bridge's Burn from a private account", "attest-claim-burn", "crown/export/run.tsv",
     lambda r: r["step"].startswith("bridge Burn (private)") and r["step"].endswith("(real proof)")),
]


def rows(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter="\t"))


def text(path):
    return open(path, errors="replace").read() if os.path.exists(path) else ""


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


def describe(d):
    """describe-tx.tsv (live-driver describe-tx) lines: file name -> receipt bytes."""
    out = {}
    for line in text(os.path.join(d, "describe-tx.tsv")).splitlines():
        name, _, desc = line.partition("\t")
        r = number(r"receipt (\d+) bytes", desc, int)
        t = number(r"transaction (\d+) bytes", desc, int)
        out[os.path.basename(name)] = (t, r)
    return out


def summarize(root):
    sessions, items = {}, []
    for key, what, session, manifest, match in ITEMS:
        d = os.path.join(root, session)
        if session not in sessions:
            sessions[session] = session_facts(d)
        found = [r for r in rows(os.path.join(d, manifest)) if match(r)]
        item = {"key": key, "what": what, "session": session, "manifest": f"{session}/{manifest}", "found": bool(found)}
        if found:
            r = found[-1]
            note = r.get("note", "")
            block = int(r["block"]) if r.get("block", "-").isdigit() else None
            item.update({
                "step": r["step"], "status": r["status"], "block": block, "tx_hash": r.get("tx_hash"),
                "exit": r.get("exit"), "exec_gas": r.get("exec_gas"),
                "prove_and_submit_s": number(r"execute_and_prove and submission ([\d.]+) s", note),
                "tx_bytes": number(r"transaction (\d+) bytes", note, int),
                "receipt_bytes": number(r"receipt (\d+) bytes", note, int),
                "real_proof": "real proof" in r["step"],
            })
            if item["receipt_bytes"] is None:
                for name, (t, rb) in describe(d).items():
                    if key in name:
                        item["receipt_bytes"] = rb
                        item["tx_bytes"] = item["tx_bytes"] or t
            chain = {int(c["block"]): c for c in rows(os.path.join(d, "chain.tsv")) if c["block"].isdigit()}
            c = chain.get(block)
            item["bedrock_status"] = c["bedrock_status"] if c else None
            if c and c.get("timestamp_ms", "0").isdigit() and int(c["timestamp_ms"]) > 0:
                published = int(c["timestamp_ms"]) / 1000
                fin = [t for a, b, t in finalized_at(text(os.path.join(d, "logs", "sequencer.log"))) if a <= block <= b]
                item["final_after_s"] = round(fin[0] - published, 1) if fin else None
        f = sessions[session]
        item["passed"] = bool(found and item["real_proof"] and item["status"] == "applied"
                              and item.get("bedrock_status") == "Finalized" and f["sequencer_dev_mode"] is False)
        items.append(item)
    tags = {s["tag"] for s in sessions.values() if s["tag"]}
    fake = [r for r in rows(os.path.join(root, "update", "run.tsv"))
            if r["phase"] == "private" and "fake receipt" in r["step"]]
    return {
        "tag": tags.pop() if len(tags) == 1 else None,
        "sessions": sessions,
        "fake_receipt": ({"status": fake[-1]["status"], "note": fake[-1]["note"][:300]} if fake else None),
        "items": items,
        "passed": sum(1 for i in items if i["passed"]),
        "generated_by": "lez/scripts/real-proofs-summary.py",
    }


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        print(__doc__.split("\n\n")[1])
        sys.exit(2)
    root = args[0]
    s = summarize(root)
    print(f"LEZ {s['tag']}: {s['passed']} of {len(s['items'])} passed (applied with a real proof in a block final on "
          f"Bedrock, the sequencer without RISC0_DEV_MODE)")
    for i in s["items"]:
        if not i["found"]:
            print(f"  {i['key']}: no row in {i['manifest']}  [not passed]")
            continue
        print(f"  {i['key']}: {i['status']} in block {i['block']} ({i['bedrock_status']}, final after {i.get('final_after_s')} s); "
              f"proven and sent in {i['prove_and_submit_s']} s; transaction {i['tx_bytes']} B, receipt {i['receipt_bytes']} B"
              f"  [{'passed' if i['passed'] else 'not passed'}]")
    if s["fake_receipt"]:
        print(f"  fake receipt: {s['fake_receipt']['status']}")
    if "--write" in sys.argv:
        with open(os.path.join(root, "summary.json"), "w") as f:
            json.dump(s, f, indent=1)
            f.write("\n")
    sys.exit(0 if s["passed"] == len(s["items"]) else 1)


if __name__ == "__main__":
    main()
