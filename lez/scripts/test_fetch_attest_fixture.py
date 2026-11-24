"""The Sepolia fixture's Transfer selection (`attested_transfer` in lez/scripts/fetch-attest-fixture.py), offline.

On the receipts of the committed CI record (block 12,203,185 of sequencer-attest's box run of 24 November 2026) and
on four made-up receipts: the EIP-7708 ETH-transfer log (emitter 0xff...fe), an amount of 2^128 or more, an address
topic with high bytes, and a Transfer the reference consumer can credit. Each case fails if the selection goes back to
taking the first Transfer-shaped log, which is what the record itself attested.

    python3 -I lez/scripts/test_fetch_attest_fixture.py      (CI's links job; standard library only, no network)
"""
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
_spec = importlib.util.spec_from_file_location("fetch_attest_fixture", os.path.join(HERE, "fetch-attest-fixture.py"))
f = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(f)

RECORD = os.path.join(REPO, "e2e", "runs", "ci-sequencer-2026-11-24", "attest", "fresh", "attest", "receipts_12203185.json")
TOKEN = "0x" + "11" * 20
fails = 0


def check(what, ok):
    global fails
    print(("ok   " if ok else "FAIL ") + what)
    fails += 0 if ok else 1


def word(x):
    return "0x" + format(x, "064x")


def log(address, frm, to, amount):
    return {"address": address, "topics": [f.TRANSFER, word(frm), word(to)], "data": word(amount), "logIndex": "0x0"}


def receipts(*logs):
    return [{"status": "0x1", "transactionIndex": "0x0", "logs": list(logs)}]


def picked(rs):
    t = f.attested_transfer(rs)
    return None if t is None else (t[0], t[1], t[2]["address"].lower(), int(t[2]["data"], 16))


# The committed record: its first Transfer-shaped log is the EIP-7708 one (tx 1, log 0), which the record attested;
# the selection takes the first token Transfer the consumer can credit instead.
rs = json.load(open(RECORD))["result"]
first = next((int(r["transactionIndex"], 16), i, l["address"].lower()) for r in rs if r["status"] == "0x1"
             for i, l in enumerate(r["logs"]) if len(l["topics"]) == 3 and l["topics"][0] == f.TRANSFER)
check("the record's first Transfer-shaped log is EIP-7708's (tx 1, log 0)", first == (1, 0, f.ETH_TRANSFER_LOGGER))
p = picked(rs)
check(f"the record: tx 4, log 2, token 0x7b79...e7f9 picked (got {p and p[:3]})",
      p is not None and p[:3] == (4, 2, "0x7b79995e5f793a07bc00c21412e50ecae098e7f9"))
d = f.describe(*f.attested_transfer(rs), rs)
check("describe() names the picked log's token", d["token"].lower() == "0x7b79995e5f793a07bc00c21412e50ecae098e7f9")

# Made up: each first log is one the reference consumer's Pool::accept refuses; the second is the one to pick.
check("EIP-7708's ETH-transfer log (0xff...fe) left out",
      picked(receipts(log(f.ETH_TRANSFER_LOGGER, 1, 2, 5), log(TOKEN, 3, 4, 6))) == (0, 1, TOKEN, 6))
check("EIP-7708's emitter in upper case left out too",
      picked(receipts(log(f.ETH_TRANSFER_LOGGER.upper().replace("0X", "0x"), 1, 2, 5), log(TOKEN, 3, 4, 6))) == (0, 1, TOKEN, 6))
check("an amount of 2^128 left out (AMOUNT_TOO_LARGE); 2^128 - 1 taken",
      picked(receipts(log(TOKEN, 3, 4, 1 << 128), log(TOKEN, 3, 4, (1 << 128) - 1))) == (0, 1, TOKEN, (1 << 128) - 1))
check("a `from` topic with high bytes left out",
      picked(receipts(log(TOKEN, (0xdead << 160) | 3, 4, 8), log(TOKEN, 3, 4, 9))) == (0, 1, TOKEN, 9))
check("a `to` topic with high bytes left out",
      picked(receipts(log(TOKEN, 3, (0xbeef << 160) | 4, 10), log(TOKEN, 3, 4, 11))) == (0, 1, TOKEN, 11))
valid = receipts(log(TOKEN, 0xa1, 0xb2, 12))
check("a Transfer the consumer can credit is picked", picked(valid) == (0, 0, TOKEN, 12))
d = f.describe(*f.attested_transfer(valid), valid)
check("describe() names its addresses whole", d["from"] == "0x" + format(0xa1, "040x") and d["to"] == "0x" + format(0xb2, "040x"))
check("none qualifies: None (the fetch stops and is run again later)",
      picked(receipts(log(f.ETH_TRANSFER_LOGGER, 1, 2, 5), log(TOKEN, 3, 4, 1 << 200))) is None)
failed_tx = [{"status": "0x0", "transactionIndex": "0x0", "logs": [log(TOKEN, 3, 4, 13)]}]
check("a failed transaction's log left out", picked(failed_tx) is None)

print("test_fetch_attest_fixture.py: " + ("all passed" if fails == 0 else f"{fails} failed"))
sys.exit(1 if fails else 0)
