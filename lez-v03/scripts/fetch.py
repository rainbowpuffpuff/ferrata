#!/usr/bin/env python3
"""Fetch fresh Sepolia light-client data and storage proofs into a run directory for `ferrata-v03`.

  scripts/fetch.py checkpoint RUN                 the newest finalized checkpoint: its bootstrap (Gloas layout)
                                                  from Lodestar, its block root checked against publicnode's
                                                  beacon header at the same slot; writes chain.json, bootstrap.ssz
  scripts/fetch.py finality RUN AFTER [TIMEOUT]   wait (a request every 30 s, up to TIMEOUT s, default 1200)
                                                  for a finality update that finalizes a slot after AFTER;
                                                  writes updates/NNN-finality-<slot>.ssz and prints its path
  scripts/fetch.py period RUN PERIOD              the period's best update: updates/NNN-period-<PERIOD>.ssz
  scripts/fetch.py proof RUN SLOT HASH ADDR SLOTS the execution block HASH (its RLP header rebuilt and checked
                                                  against HASH) and eth_getProof of ADDR's SLOTS (comma-separated)
                                                  at it: proofs/<SLOT>/{block.json,header.rlp,proof.json}

Every request goes to a public endpoint without an API key, with the User-Agent `ferrata-v03-fetch` and nothing
else about who asks; each is listed in RUN/requests.tsv. Responses are stored byte for byte (the RLP header is
rebuilt from the block's JSON by `scripts/fetch-mainnet-e2e.py`'s `header_rlp`, then checked). No transaction is
sent anywhere.
"""
import importlib.util
import json
import os
import sys
import time
import urllib.error

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
SCRIPTS = os.path.join(HERE, "..", "..", "scripts")


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, os.path.join(SCRIPTS, file))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


e2e = load("fetch_mainnet_e2e", "fetch-mainnet-e2e.py")
g = load("fetch_sepolia_gloas", "fetch-sepolia-gloas.py")
e2e.UA = "ferrata-v03-fetch"

LODESTAR = "https://lodestar-sepolia.chainsafe.io"
SECOND = "https://ethereum-sepolia-beacon-api.publicnode.com"
EXECUTION = g.EXECUTION
SLOTS_PER_PERIOD = 8192
BOOTSTRAP_LEN = g.HEADER + g.COMMITTEE + 11 * 32
FINALITY_LEN = 2 * g.HEADER + 9 * 32 + 160 + 8


def log_requests(run):
    path = os.path.join(run, "requests.tsv")
    new = not os.path.exists(path)
    with open(path, "a") as f:
        if new:
            f.write("utc\tendpoint\trequest\tfile\tnote\n")
        for r in e2e.LOG:
            f.write("\t".join(str(r.get(k) or "") for k in ("at", "endpoint", "request", "file", "note")) + "\n")
    e2e.LOG.clear()


def second_root(f, slot):
    """publicnode's block root at `slot` (None if the slot is empty there)."""
    try:
        data = f.beacon(SECOND, f"/eth/v1/beacon/headers/{slot}", None, ssz=False)
    except urllib.error.HTTPError:
        return None
    return json.loads(data)["data"]["root"]


def checkpoint(run):
    os.makedirs(run, exist_ok=True)
    f = e2e.Fetcher(run)
    fu = g.parse_finality_update(f.beacon(LODESTAR, "/eth/v1/beacon/light_client/finality_update", None, ssz=True))
    fin = fu["finalized"]
    slot = fin["slot"]
    root = "0x" + e2e.beacon_header_root(fin["beacon"])
    b = f.beacon(LODESTAR, f"/eth/v1/beacon/light_client/bootstrap/{root}", "bootstrap.ssz", ssz=True)
    if len(b) != BOOTSTRAP_LEN:
        raise SystemExit(f"the bootstrap is {len(b)} bytes, not the Gloas layout's {BOOTSTRAP_LEN}")
    if e2e.u64(b, 0) != slot or "0x" + e2e.beacon_header_root(b[:112]) != root:
        raise SystemExit("the bootstrap's header is not the finalized checkpoint's")
    if second_root(f, slot) != root:
        raise SystemExit(f"the second provider reports another block root at slot {slot}")
    chain = {"chain": "sepolia", "layout": "gloas", "checkpoint": root[2:], "bootstrap_slot": slot,
             "bootstrap_period": slot // SLOTS_PER_PERIOD, "fetched_utc": e2e.now(),
             "sources": {"bootstrap": LODESTAR, "root_check": SECOND}}
    with open(os.path.join(run, "chain.json"), "w") as out:
        json.dump(chain, out, indent=1)
        out.write("\n")
    log_requests(run)
    print(f"checkpoint: slot {slot}, period {slot // SLOTS_PER_PERIOD}, block root {root}")


def next_name(run, kind, tag):
    d = os.path.join(run, "updates")
    os.makedirs(d, exist_ok=True)
    n = len([x for x in os.listdir(d) if x.endswith(".ssz")])
    return os.path.join(d, f"{n:03d}-{kind}-{tag}.ssz")


def finality(run, after, timeout):
    f = e2e.Fetcher(run)
    deadline = time.time() + timeout
    while True:
        b = f.beacon(LODESTAR, "/eth/v1/beacon/light_client/finality_update", None, ssz=True)
        if len(b) != FINALITY_LEN:
            raise SystemExit(f"a finality update of {len(b)} bytes, not the Gloas layout's {FINALITY_LEN}")
        fu = g.parse_finality_update(b)
        if fu["finalized"]["slot"] > after:
            root = "0x" + e2e.beacon_header_root(fu["finalized"]["beacon"])
            second = second_root(f, fu["finalized"]["slot"])
            if second is not None and second != root:
                raise SystemExit("the second provider reports another finalized block root")
            path = next_name(run, "finality", fu["finalized"]["slot"])
            with open(path, "wb") as out:
                out.write(b)
            e2e.LOG.append({"at": e2e.now(), "endpoint": LODESTAR, "request": "(kept)", "file": os.path.basename(path),
                            "note": f"finalized {fu['finalized']['slot']} root {root}, {fu['participants']}/512"})
            log_requests(run)
            print(path)
            return
        if time.time() > deadline:
            log_requests(run)
            raise SystemExit(f"no finality update after slot {after} within {timeout} s")
        time.sleep(30)


def period(run, p):
    f = e2e.Fetcher(run)
    stream = f.beacon(LODESTAR, f"/eth/v1/beacon/light_client/updates?start_period={p}&count=1", None, ssz=True)
    chunks = g.split_chunks(stream)
    if len(chunks) != 1:
        raise SystemExit(f"{len(chunks)} updates for one period")
    _, payload = chunks[0]
    path = next_name(run, "period", p)
    with open(path, "wb") as out:
        out.write(payload)
    log_requests(run)
    print(path)


def proof(run, slot, block_hash, address, slots):
    d = os.path.join(run, "proofs", str(slot))
    os.makedirs(d, exist_ok=True)
    f = e2e.Fetcher(d)
    block, _ = f.rpc_any("eth_getBlockByHash", [block_hash, False], "block.json", EXECUTION)
    rlp = e2e.header_rlp(block)
    if "0x" + e2e.keccak256(rlp).hex() != block_hash.lower():
        raise SystemExit("the rebuilt RLP header does not hash to the block hash")
    with open(os.path.join(d, "header.rlp"), "wb") as out:
        out.write(rlp)
    keys = ["0x" + int(s, 0).to_bytes(32, "big").hex() for s in slots.split(",")]
    f.rpc_any("eth_getProof", [address, keys, block["number"]], "proof.json", EXECUTION)
    log_requests(run)
    print(f"proof: {address} slots {slots} at block {int(block['number'], 16)} ({block_hash})")


def main():
    a = sys.argv[1:]
    if a[:1] == ["checkpoint"] and len(a) == 2:
        checkpoint(a[1])
    elif a[:1] == ["finality"] and len(a) in (3, 4):
        finality(a[1], int(a[2]), int(a[3]) if len(a) == 4 else 1200)
    elif a[:1] == ["period"] and len(a) == 3:
        period(a[1], int(a[2]))
    elif a[:1] == ["proof"] and len(a) == 6:
        proof(a[1], int(a[2]), a[3], a[4], a[5])
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
