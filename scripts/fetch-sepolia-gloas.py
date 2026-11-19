#!/usr/bin/env python3
"""Fetch the Sepolia Gloas fixture for `tests/sepolia_gloas.rs`.

Sepolia activated Gloas at epoch 353,024 (6 Oct 2026; GLOAS_FORK_VERSION 0x90000076 in
eth-clients/sepolia `metadata/config.yaml`), the first slot of sync-committee period 1379. Under
Gloas a light-client header binds only the execution block hash of the bid's parent block
(EIP-7732); the record's roots come from that block's RLP header. This script fetches, in order:

1. the latest light-client finality update (SSZ, Gloas layout) from a beacon node that serves it;
2. the execution block its finalized header binds, by hash, from an execution provider, rebuilt as
   RLP and checked against the hash; the same block from a second execution provider;
3. the finalized beacon block from a second beacon provider (its bid's `parent_block_hash` is the
   bound hash);
4. a bootstrap: first for the finality update's own period, which the one public node found
   serving light-client data refuses under Gloas today (the refusal is recorded in SOURCE.md),
   then for the first checkpoint of the last pre-Gloas period, which it serves;
5. every period's light-client update from that period to the current one (SSZ), so a store
   bootstrapped before the fork follows the chain into Gloas; and, from the second beacon
   provider, the canonical root of every finalized header the data names.

Python standard library only; it reuses the helpers of `fetch-mainnet-e2e.py` (Keccak, RLP,
requests). Every request goes to a public endpoint without an API key, with the generic
User-Agent of scripts/pubnet.py (`curl/8`) and nothing else that identifies the caller.

Usage: scripts/fetch-sepolia-gloas.py OUTDIR      (OUTDIR must not exist yet)
"""
import hashlib
import importlib.util
import json
import os
import sys
import urllib.error

_spec = importlib.util.spec_from_file_location("e2e", os.path.join(os.path.dirname(os.path.abspath(__file__)), "fetch-mainnet-e2e.py"))
e2e = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(e2e)

BEACON = ["https://lodestar-sepolia.chainsafe.io", "https://ethereum-sepolia-beacon-api.publicnode.com"]
EXECUTION = [
    "https://ethereum-sepolia-rpc.publicnode.com",
    "https://1rpc.io/sepolia",
    "https://rpc.sepolia.ethpandaops.io",
    "https://eth-sepolia.api.onfinality.io/public",
]
SLOTS_PER_PERIOD = 8192
HEADER = 112 + 32 + 11 * 32  # Gloas LightClientHeader: beacon, execution_block_hash, branch


def parse_header(h):
    assert len(h) == HEADER
    return {"slot": e2e.u64(h, 0), "beacon": h[:112], "block_hash": h[112:144].hex()}


def parse_finality_update(b):
    """Gloas LightClientFinalityUpdate: every field is fixed-size."""
    if len(b) != 2 * HEADER + 9 * 32 + 64 + 96 + 8:
        raise SystemExit(f"unexpected Gloas finality update length {len(b)}")
    bits = b[2 * HEADER + 288:2 * HEADER + 288 + 64]
    return {
        "attested": parse_header(b[:HEADER]),
        "finalized": parse_header(b[HEADER:2 * HEADER]),
        "signature_slot": e2e.u64(b, len(b) - 8),
        "participants": sum(bin(x).count("1") for x in bits),
    }


GLOAS_EPOCH = 353024
LAST_FULU_PERIOD = GLOAS_EPOCH * 32 // SLOTS_PER_PERIOD - 1
COMMITTEE = 513 * 48
GLOAS_UPDATE = HEADER + COMMITTEE + 11 * 32 + HEADER + 9 * 32 + 160 + 8
ELECTRA_FIXED = 4 + COMMITTEE + 6 * 32 + 4 + 7 * 32 + 160 + 8


def parse_update(b):
    """A LightClientUpdate in the Gloas (fixed-size) or Electra (offsets) layout."""
    if len(b) == GLOAS_UPDATE:
        fin_at = HEADER + COMMITTEE + 11 * 32
        return {"layout": "gloas", "attested": e2e.u64(b, 0), "finalized_beacon": b[fin_at:fin_at + 112],
                "signature_slot": e2e.u64(b, len(b) - 8), "bits": b[len(b) - 168:len(b) - 104]}
    att, fin = e2e.u32(b, 0), e2e.u32(b, 4 + COMMITTEE + 6 * 32)
    if att != ELECTRA_FIXED:
        raise SystemExit(f"unknown update layout ({len(b)} bytes)")
    return {"layout": "electra", "attested": e2e.u64(b, att), "finalized_beacon": b[fin:fin + 112],
            "signature_slot": e2e.u64(b, ELECTRA_FIXED - 8), "bits": b[ELECTRA_FIXED - 168:ELECTRA_FIXED - 104]}


def split_chunks(raw):
    """The SSZ stream of the `updates` endpoint: per update, a u64 length, a fork digest, the SSZ.
    Accepts a length that counts the digest or one that does not, if it parses the whole stream."""
    for counts_digest in (True, False):
        out, at = [], 0
        try:
            while at < len(raw):
                n = e2e.u64(raw, at)
                size = n - 4 if counts_digest else n
                digest, payload = raw[at + 8:at + 12], raw[at + 12:at + 12 + size]
                if size <= 0 or len(payload) != size:
                    raise ValueError
                out.append((digest, payload))
                at += 12 + size
            return out
        except ValueError:
            continue
    raise SystemExit("the updates stream does not parse")


def first_checkpoint(f, period, before_slot):
    """The first epoch-start slot of `period` that holds a block, and its root (second provider)."""
    for epoch in range(period * SLOTS_PER_PERIOD // 32, before_slot // 32):
        slot = epoch * 32
        try:
            return slot, json.loads(f.beacon(BEACON[1], f"/eth/v1/beacon/headers/{slot}", None, ssz=False))["data"]["root"]
        except urllib.error.HTTPError:
            continue
    raise SystemExit(f"no checkpoint block in period {period}")


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out)
    f = e2e.Fetcher(out)
    started = e2e.now()

    fu = parse_finality_update(f.beacon(BEACON[0], "/eth/v1/beacon/light_client/finality_update", "finality_update.ssz", ssz=True))
    fin = fu["finalized"]
    period = fu["signature_slot"] // SLOTS_PER_PERIOD
    if fin["slot"] // SLOTS_PER_PERIOD != period:
        raise SystemExit("finalized slot and signature slot are in different periods; retry later")
    print(f"finalized slot {fin['slot']}, bound execution block 0x{fin['block_hash']} (period {period})", flush=True)

    # The bound execution block, by hash, from two providers; its RLP header, checked.
    block, _ = f.rpc_any("eth_getBlockByHash", ["0x" + fin["block_hash"], False], "block.json", EXECUTION)
    rlp = e2e.header_rlp(block)
    if "0x" + e2e.keccak256(rlp).hex() != block["hash"] or block["hash"] != "0x" + fin["block_hash"]:
        raise SystemExit("the rebuilt header does not hash to the bound block hash")
    f.save("header.rlp", rlp)
    second, _ = f.rpc_any("eth_getBlockByHash", ["0x" + fin["block_hash"], False], "block_second.json", EXECUTION[1:])
    for field in ["hash", "number", "stateRoot", "receiptsRoot", "parentHash", "timestamp"]:
        if second[field] != block[field]:
            raise SystemExit(f"execution providers disagree on {field}")

    # The finalized beacon block from a second beacon provider: its bid's parent_block_hash.
    fin_root = "0x" + e2e.beacon_header_root(fin["beacon"])
    raw = f.beacon(BEACON[1], f"/eth/v2/beacon/blocks/{fin_root}", "beacon_block_finalized_second.json", ssz=False)
    bid = json.loads(raw)["data"]["message"]["body"]["signed_execution_payload_bid"]["message"]
    if bid["parent_block_hash"] != "0x" + fin["block_hash"]:
        raise SystemExit("the finalized block's bid does not name the bound hash as its parent")

    # A bootstrap under Gloas, if the node serves one now (it did not when this was written).
    _, gloas_root = first_checkpoint(f, period, fin["slot"])
    path = f"/eth/v1/beacon/light_client/bootstrap/{gloas_root}"
    try:
        f.beacon(BEACON[0], path, "bootstrap.ssz", ssz=True)
        raise SystemExit("a Gloas bootstrap is served now: rewrite the fixture around it")
    except urllib.error.HTTPError as e:
        e2e.LOG.append({"at": e2e.now(), "endpoint": BEACON[0], "request": f"GET {path}", "file": None,
                        "note": f"HTTP {e.code}: {e.read()[:120].decode(errors='replace')}"})
    # The last pre-Gloas period's first checkpoint, and every period's update since.
    boot_slot, boot_root = first_checkpoint(f, LAST_FULU_PERIOD, (LAST_FULU_PERIOD + 1) * SLOTS_PER_PERIOD)
    f.beacon(BEACON[0], f"/eth/v1/beacon/light_client/bootstrap/{boot_root}", "bootstrap.ssz", ssz=True)
    count = period - LAST_FULU_PERIOD + 1
    stream = f.beacon(BEACON[0], f"/eth/v1/beacon/light_client/updates?start_period={LAST_FULU_PERIOD}&count={count}", None, ssz=True)
    chunks = split_chunks(stream)
    if len(chunks) != count:
        raise SystemExit(f"{len(chunks)} updates for {count} periods")
    rows, check = [], [(boot_slot, boot_root), (fin["slot"], fin_root)]
    for i, (digest, payload) in enumerate(chunks):
        p = LAST_FULU_PERIOD + i
        u = parse_update(payload)
        if u["signature_slot"] // SLOTS_PER_PERIOD != p:
            raise SystemExit(f"update {i} is not signed in period {p}")
        f.save(f"update_{p}.ssz", payload)
        fslot = e2e.u64(u["finalized_beacon"], 0)
        froot = "0x" + e2e.beacon_header_root(u["finalized_beacon"])
        signers = sum(bin(x).count("1") for x in u["bits"])
        rows.append(f"| {p} | {u['layout']} | 0x{digest.hex()} | {u['attested']} | {u['signature_slot']} | {fslot} | {signers}/512 |")
        check.append((fslot, froot))
    for slot, want in sorted(set(check)):
        got = json.loads(f.beacon(BEACON[1], f"/eth/v1/beacon/headers/{slot}", f"beacon_header_{slot}_second.json", ssz=False))
        if got["data"]["root"] != want:
            raise SystemExit(f"second beacon provider reports another root at slot {slot}")

    files = sorted(x for x in os.listdir(out) if x not in ("SOURCE.md", "SHA256SUMS"))
    sums = {x: hashlib.sha256(open(os.path.join(out, x), "rb").read()).hexdigest() for x in files}
    with open(os.path.join(out, "SHA256SUMS"), "w") as s:
        s.writelines(f"{sums[x]}  {x}\n" for x in files)
    lines = [
        "# Sepolia into Gloas: a pre-Gloas bootstrap, every period's update through the fork, a Gloas finality update",
        "",
        f"Fetched by `scripts/fetch-sepolia-gloas.py` between {started} and {e2e.now()} (UTC). Every request",
        f"went to a public endpoint without an API key, with User-Agent `{e2e.UA}`. Responses are stored",
        "byte for byte, except the `updates` stream, which is split into one file per period with its",
        "8-byte length and 4-byte fork digest removed (the digests are listed below); `header.rlp` is",
        "rebuilt from `block.json` by the script, which checked that it hashes to the bound block hash.",
        "`SHA256SUMS` pins every file.",
        "",
        "**Why the bootstrap is not under Gloas.** Of the public Sepolia beacon APIs tried, only Lodestar",
        "(lodestar-sepolia.chainsafe.io) serves light-client data, and it serves no bootstrap for a",
        "Gloas-era checkpoint: HTTP 500 \"syncCommitteeWitness not available\" (recorded below for this",
        "period's first checkpoint; the same on 2026-11-10 for the first checkpoints of periods 1379, 1380,",
        "1390, 1400 and 1408 and for the finalized checkpoint of epoch 360,844). Lighthouse",
        "(ethereum-sepolia-beacon-api.publicnode.com) answers 404 to every light-client request.",
        f"Lodestar serves the bootstrap of the last pre-Gloas period ({LAST_FULU_PERIOD}) and every period's",
        "update since, so the fixture starts there and follows the chain through the fork.",
        "",
        "| | |",
        "|---|---|",
        "| chain | Sepolia: chain id 11155111, `genesis_validators_root` 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078; Gloas since epoch 353,024 (version 0x90000076), the first slot of period 1379 |",
        f"| bootstrap (Fulu, Electra layout) | slot {boot_slot}, block root {boot_root} |",
        f"| finality update (Gloas) | attested slot {fu['attested']['slot']}, signature slot {fu['signature_slot']}, {fu['participants']}/512 signers |",
        f"| finalized beacon block | slot {fin['slot']}, root {fin_root} |",
        f"| bound execution block (the bid's parent) | {int(block['number'], 16)}, hash {block['hash']} |",
        f"| its state root / receipts root | {block['stateRoot']} / {block['receiptsRoot']} |",
        "",
        "| period | layout | fork digest | attested slot | signature slot | finalized slot | signers |",
        "|---|---|---|---|---|---|---|",
    ] + rows + [
        "",
        "Cross-checks made by the script: the RLP header rebuilt from `block.json` hashes to the bound hash; a",
        "second execution provider (`block_second.json`) reports the same block; a second beacon provider",
        "reports the root of the bootstrap block and of every finalized header named here",
        "(`beacon_header_*_second.json`), and the finalized block, whose bid names the bound hash as",
        "`parent_block_hash` (`beacon_block_finalized_second.json`).",
        "",
        "## Requests",
        "",
        "| UTC | endpoint | request | file | note |",
        "|---|---|---|---|---|",
    ]
    lines += [f"| {r['at']} | {r['endpoint']} | `{r['request']}` | {r['file'] or ''} | {r['note']} |" for r in e2e.LOG]
    with open(os.path.join(out, "SOURCE.md"), "w") as s:
        s.write("\n".join(lines) + "\n")
    print("done:", out)


if __name__ == "__main__":
    main()
