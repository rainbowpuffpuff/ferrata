#!/usr/bin/env python3
"""The inputs of one lab session for one chain, from public beacon endpoints (read only).

    fetch.py --chain sepolia|mainnet --kept RUN_DIR/net/CHAIN --out INPUTS/CHAIN --repo REPO

Writes into --out: `finality_update.ssz` (the newest), `updates/PPPPPP.ssz` (every period's
update from the instance's finalized period to the newest), and, if the instance does not exist
yet (no `bootstrap.ssz` in --kept), `bootstrap.ssz` and `checkpoint`: for Sepolia the period-1378
bootstrap of the committed fixture (pinned by its SHA256SUMS), for mainnet the bootstrap of the
current finalized checkpoint, whose block a second provider must report canonical. `fetch.json`
records every request (path, status, bytes, SHA-256, time) and what a second provider reports
for the newest finalized slot.

Only public endpoints without an API key; a generic User-Agent and nothing else that identifies
the caller. A failed request is recorded and the session goes on with what it has.
"""
import argparse
import hashlib
import json
import os
import struct
import sys
import time
import urllib.error
import urllib.request

UA = "ferrata-lab-fetch"
SLOTS_PER_PERIOD = 8192
LC = {"sepolia": "https://lodestar-sepolia.chainsafe.io", "mainnet": "https://lodestar-mainnet.chainsafe.io"}
SECOND = {"sepolia": "https://ethereum-sepolia-beacon-api.publicnode.com",
          "mainnet": "https://ethereum-beacon-api.publicnode.com"}
SEPOLIA_FIXTURE = "crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10"
SEPOLIA_CHECKPOINT = "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d"
GLOAS_FINALITY_LEN = 2 * (112 + 32 + 11 * 32) + 9 * 32 + 160 + 8
log = []


def get(base, path, accept):
    url = base + path
    for attempt in range(3):
        t = time.time()
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept})
            with urllib.request.urlopen(req, timeout=40) as r:
                body = r.read()
                log.append({"url": url, "status": r.status, "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest(),
                            "version": r.headers.get("Eth-Consensus-Version"), "at": round(t, 1)})
                return body
        except urllib.error.HTTPError as e:
            log.append({"url": url, "status": e.code, "error": e.read()[:300].decode("utf-8", "replace"), "at": round(t, 1)})
            if e.code < 500:
                return None
        except Exception as e:  # network errors: recorded, retried
            log.append({"url": url, "status": None, "error": repr(e)[:300], "at": round(t, 1)})
        time.sleep(3 * (attempt + 1))
    return None


def u32(b, o):
    return struct.unpack_from("<I", b, o)[0]


def u64(b, o):
    return struct.unpack_from("<Q", b, o)[0]


def attested_slot_of_update(b):
    """The attested header's slot: Gloas updates are fixed-size (the slot first), Electra ones
    start with the attested header's offset."""
    off = u32(b, 0)
    # Electra: the first four bytes are an offset that points past the fixed part (> 25,000).
    if 24000 < off < len(b):
        return u64(b, off)
    return u64(b, 0)


def finality_slots(b):
    """(attested slot, finalized slot) of a LightClientFinalityUpdate."""
    if len(b) == GLOAS_FINALITY_LEN:
        h = 112 + 32 + 11 * 32
        return u64(b, 0), u64(b, h)
    att, fin = u32(b, 0), u32(b, 4)
    return u64(b, att), u64(b, fin)


def split_chunks(raw):
    """The SSZ stream of the `updates` endpoint: per update a u64 length, a fork digest, the SSZ.
    Accepts a length that counts the digest or one that does not (as scripts/fetch-sepolia-gloas.py)."""
    for counts_digest in (True, False):
        out, at = [], 0
        try:
            while at < len(raw):
                n = u64(raw, at)
                size = n - 4 if counts_digest else n
                digest, payload = raw[at + 8:at + 12], raw[at + 12:at + 12 + size]
                if size <= 0 or len(payload) != size:
                    raise ValueError
                out.append((digest, payload))
                at += 12 + size
            return out
        except (ValueError, struct.error):
            continue
    return []


def kept_finalized_slot(kept):
    """The instance's finalized slot as the kept jobs say (None: no instance yet)."""
    if not os.path.exists(os.path.join(kept, "bootstrap.ssz")):
        return None
    best = 0
    try:
        with open(os.path.join(kept, "jobs.tsv")) as f:
            head = f.readline().rstrip("\n").split("\t")
            i, s = head.index("finalized_slot"), head.index("status")
            for line in f:
                c = line.rstrip("\n").split("\t")
                if c[s] == "applied":
                    best = max(best, int(c[i]))
    except (OSError, ValueError):
        pass
    return best


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--chain", required=True, choices=["sepolia", "mainnet"])
    ap.add_argument("--kept", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--repo", required=True)
    a = ap.parse_args()
    lc, second = LC[a.chain], SECOND[a.chain]
    os.makedirs(os.path.join(a.out, "updates"), exist_ok=True)
    summary = {"chain": a.chain, "lc_endpoint": lc, "second_endpoint": second, "user_agent": UA, "started": time.time()}

    fin = kept_finalized_slot(a.kept)
    if fin is None:
        if a.chain == "sepolia":
            d = os.path.join(a.repo, SEPOLIA_FIXTURE)
            data = open(os.path.join(d, "bootstrap.ssz"), "rb").read()
            sums = dict(reversed(l.split()) for l in open(os.path.join(d, "SHA256SUMS")) if l.strip())
            if hashlib.sha256(data).hexdigest() != sums.get("bootstrap.ssz"):
                sys.exit("the fixture's bootstrap does not match its SHA256SUMS")
            checkpoint, fin = SEPOLIA_CHECKPOINT, 11288576
            summary["bootstrap"] = f"fixture {SEPOLIA_FIXTURE} (period 1378, slot {fin})"
        else:
            cp = get(lc, "/eth/v1/beacon/states/finalized/finality_checkpoints", "application/json")
            if cp is None:
                sys.exit("no finalized checkpoint")
            checkpoint = json.loads(cp)["data"]["finalized"]["root"].removeprefix("0x")
            hdr = get(second, f"/eth/v1/beacon/headers/0x{checkpoint}", "application/json")
            hj = json.loads(hdr)["data"] if hdr else None
            if not hj or not hj.get("canonical"):
                sys.exit(f"the second provider does not report 0x{checkpoint} canonical")
            data = get(lc, f"/eth/v1/beacon/light_client/bootstrap/0x{checkpoint}", "application/octet-stream")
            if data is None:
                sys.exit("no bootstrap served")
            fin = int(hj["header"]["message"]["slot"])
            summary["bootstrap"] = f"finalized checkpoint 0x{checkpoint} (slot {fin}), canonical at {second}"
        open(os.path.join(a.out, "bootstrap.ssz"), "wb").write(data)
        open(os.path.join(a.out, "checkpoint"), "w").write(checkpoint + "\n")
    summary["instance_finalized_slot"] = fin

    fu = get(lc, "/eth/v1/beacon/light_client/finality_update", "application/octet-stream")
    head_period = fin // SLOTS_PER_PERIOD
    if fu is not None:
        open(os.path.join(a.out, "finality_update.ssz"), "wb").write(fu)
        att, fslot = finality_slots(fu)
        head_period = att // SLOTS_PER_PERIOD
        summary.update({"finality_attested_slot": att, "finality_finalized_slot": fslot, "head_period": head_period})
        h = get(second, f"/eth/v1/beacon/headers/{fslot}", "application/json")
        if h:
            summary["second_provider_finalized_root"] = json.loads(h)["data"]["root"]
    start = fin // SLOTS_PER_PERIOD
    count = min(head_period - start + 1, 128)
    if count > 0:
        raw = get(lc, f"/eth/v1/beacon/light_client/updates?start_period={start}&count={count}", "application/octet-stream")
        written = []
        for digest, payload in split_chunks(raw or b""):
            period = attested_slot_of_update(payload) // SLOTS_PER_PERIOD
            open(os.path.join(a.out, "updates", f"{period:06}.ssz"), "wb").write(payload)
            written.append({"period": period, "fork_digest": digest.hex(), "bytes": len(payload)})
        summary["updates"] = written
    summary["finished"] = time.time()
    summary["requests"] = log
    json.dump(summary, open(os.path.join(a.out, "fetch.json"), "w"), indent=1)
    print(f"{a.chain}: instance at slot {fin}, head period {head_period}, "
          f"{len(summary.get('updates', []))} update(s), finality update {'yes' if fu else 'NO'}; {len(log)} request(s)")


if __name__ == "__main__":
    main()
