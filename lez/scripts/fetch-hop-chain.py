#!/usr/bin/env python3
"""Fetch chains of EIP-2935 hops for `lez/harness/tests/history_hops.rs`.

A record of the attestation program holds one finalized execution block `R`. One EIP-2935 hop
proves an older block `n` (`R - 8191 <= n < R`) from it: the history contract's account proof and
the proof of its slot `n mod 8191`, both in the state of `R`, and `n`'s RLP header. A chain of hops
repeats this from each block reached, so it reaches any block back to the Prague fork's parent;
the hop from block `b` needs `eth_getProof` of the history contract **at `b`**, an archive request
for any `b` older than about 128 blocks.

For every chain (`--chain n1,n2,...`, newest first, or `--window-chain K`: `R - 8191 k` for
k = 1..K, every hop the full window) the script fetches:

1. `eth_getProof` of the history contract at every block of the chain and at `R`, for the slots of
   the hops that start there (an empty slot list at a chain's last block: its account proof is what
   the tests attest there), from an archive endpoint; it checks that each hop's slot holds the
   hash of the block hopped to;
2. every block of the chain by number from two execution providers (`block_N.json`,
   `block_N_second.json`), its RLP header rebuilt (`header_N.rlp`) and checked against the hash both
   report;
3. with `--receipts B`: `eth_getBlockReceipts` of `B`, and the first ERC-20 `Transfer` in a receipt
   that succeeded (recorded in `fixture.json`);
4. with `--storage B:ADDR:SLOT[:SIG[:ARG]]`: `eth_getProof` of `ADDR`'s `SLOT` at `B`, cross-checked
   by `eth_call` of `SIG` (an address `ARG` as its argument) at `B`;
5. with `--header B`: block `B`'s header alone (the refusal tests' hop above the record and hop past
   the window).

Python standard library only; the HTTP, Keccak and RLP helpers are those of
`scripts/fetch-mainnet-e2e.py`. Every request goes to a public endpoint without an API key, with the
generic User-Agent `ferrata-fixture-fetch` and nothing else that identifies the caller.
`fixture.json` records every choice, SOURCE.md every request, SHA256SUMS every file.

Usage:
  lez/scripts/fetch-hop-chain.py --network sepolia|mainnet --record R [--chain N,N,...]...
      [--window-chain K] [--receipts B]... [--storage B:ADDR:SLOT[:SIG[:ARG]]]... [--header B]... OUTDIR
(OUTDIR must not exist yet.)
"""
import argparse
import hashlib
import importlib.util
import json
import os
import sys
import time
import urllib.error

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True  # no __pycache__ beside the imported script
_spec = importlib.util.spec_from_file_location(
    "fetch_mainnet_e2e", os.path.join(HERE, "..", "..", "scripts", "fetch-mainnet-e2e.py"))
e2e = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(e2e)

HISTORY = e2e.HISTORY
WINDOW = e2e.HISTORY_SERVE_WINDOW
TRANSFER = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
NETWORKS = {
    "sepolia": {
        "chain_id": 11155111,
        # Archive state (eth_getProof at blocks weeks old): the public node of the Ethereum
        # Foundation's DevOps team; publicnode and 1rpc answer "exceeds maximum proof window".
        "archive": ["https://rpc.sepolia.ethpandaops.io"],
        "blocks": ["https://ethereum-sepolia-rpc.publicnode.com", "https://rpc.sepolia.ethpandaops.io"],
    },
    "mainnet": {
        "chain_id": 1,
        # Archive state: MEV Blocker's public endpoint (publicnode and 1rpc ask for a personal token).
        "archive": ["https://rpc.mevblocker.io"],
        "blocks": ["https://ethereum-rpc.publicnode.com", "https://rpc.mevblocker.io"],
    },
}


def q(n):
    return hex(n)


def selector(signature):
    return e2e.keccak256(signature.encode())[:4].hex()


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--network", required=True, choices=sorted(NETWORKS))
    p.add_argument("--record", required=True, type=int)
    p.add_argument("--chain", action="append", default=[])
    p.add_argument("--window-chain", type=int, default=0)
    p.add_argument("--receipts", action="append", type=int, default=[])
    p.add_argument("--storage", action="append", default=[])
    p.add_argument("--header", action="append", type=int, default=[])
    p.add_argument("outdir")
    a = p.parse_args()
    if os.path.exists(a.outdir):
        raise SystemExit(f"{a.outdir} exists")
    net = NETWORKS[a.network]
    r = a.record
    chains = [[int(x) for x in c.split(",")] for c in a.chain]
    if a.window_chain:
        chains.append([r - WINDOW * k for k in range(1, a.window_chain + 1)])
    slots = {r: set()}
    for chain in chains:
        prev = r
        for n in chain:
            if not (n < prev and prev - n <= WINDOW):
                raise SystemExit(f"hop {prev} -> {n} is outside the window")
            slots.setdefault(prev, set()).add(n % WINDOW)
            slots.setdefault(n, set())
            prev = n
    os.makedirs(a.outdir)
    f = e2e.Fetcher(a.outdir)
    plain = f.rpc

    def rpc(endpoint, method, params, name):
        # Public endpoints drop a connection now and then: up to five attempts, a failed one leaves
        # no file and no SOURCE.md row (the Fetcher writes both only for an answer).
        for attempt in range(5):
            try:
                return plain(endpoint, method, params, name)
            except (ConnectionError, TimeoutError, urllib.error.URLError, OSError) as e:
                if attempt == 4:
                    raise
                print(f"retry {method} at {endpoint}: {e}", file=sys.stderr)
                time.sleep(2 + 3 * attempt)

    f.rpc = rpc

    def pause():
        time.sleep(0.25)

    # 2. Headers, from two providers each.
    hashes = {}
    for b in sorted(set(slots) | set(a.header), reverse=True):
        one = f.rpc(net["blocks"][0], "eth_getBlockByNumber", [q(b), False], f"block_{b}.json")
        two = f.rpc(net["blocks"][1], "eth_getBlockByNumber", [q(b), False], f"block_{b}_second.json")
        pause()
        if one["hash"] != two["hash"]:
            raise SystemExit(f"block {b}: the providers disagree")
        rlp = e2e.header_rlp(one)
        if "0x" + e2e.keccak256(rlp).hex() != one["hash"]:
            raise SystemExit(f"block {b}: the rebuilt header does not hash to {one['hash']}")
        f.save(f"header_{b}.rlp", rlp)
        hashes[b] = one["hash"]
    # 1. The history contract at every block a hop starts from, and at every block of a chain.
    for b in sorted(slots, reverse=True):
        keys = [e2e.word(s) for s in sorted(slots[b])]
        res = f.rpc(net["archive"][0], "eth_getProof", [HISTORY, keys, q(b)], f"proof_history_{b}.json")
        pause()
        held = {int(s["key"], 16): s["value"] for s in res["storageProof"]}
        for n in [n for n in hashes if n < b and b - n <= WINDOW and n % WINDOW in slots[b]]:
            if int(held[n % WINDOW], 16) != int(hashes[n], 16):
                raise SystemExit(f"the history slot of {n} at {b} is not its hash")
    # 3. Receipts and the first ERC-20 Transfer of a successful receipt.
    transfers = {}
    for b in a.receipts:
        # publicnode answers `null` for receipts of blocks a few weeks old: then the archive endpoint.
        for endpoint in [net["blocks"][0], net["archive"][0]]:
            try:
                res = f.rpc(endpoint, "eth_getBlockReceipts", [q(b)], f"receipts_{b}.json")
                break
            except RuntimeError as e:
                print(f"{e}; next endpoint", file=sys.stderr)
        else:
            raise SystemExit(f"no endpoint served the receipts of {b}")
        pause()
        for tx, rc in enumerate(res):
            if int(rc["status"], 16) != 1:
                continue
            hit = [(i, lg) for i, lg in enumerate(rc["logs"]) if len(lg["topics"]) == 3 and lg["topics"][0] == TRANSFER]
            if hit:
                i, lg = hit[0]
                transfers[str(b)] = {"tx_index": tx, "log_index": i, "block_log_index": int(lg["logIndex"], 16),
                                     "address": lg["address"], "logs_in_receipt": len(rc["logs"])}
                break
        else:
            raise SystemExit(f"block {b}: no ERC-20 Transfer")
    # 4. Storage values, each with an eth_call cross-check.
    storage = []
    for spec in a.storage:
        parts = spec.split(":")
        b, addr, slot = int(parts[0]), parts[1].lower(), parts[2]
        slot = e2e.word(int(slot, 0))
        res = f.rpc(net["archive"][0], "eth_getProof", [addr, [slot], q(b)], f"proof_{addr}_{b}.json")
        value = res["storageProof"][0]["value"]
        entry = {"block": b, "address": addr, "slot": slot, "value": e2e.word(int(value, 16))}
        if len(parts) > 3:
            sig = parts[3]
            data = "0x" + selector(sig) + (parts[4][2:].lower().rjust(64, "0") if len(parts) > 4 else "")
            name = sig.split("(")[0]
            call = f.rpc(net["archive"][0], "eth_call", [{"to": addr, "data": data}, q(b)],
                         f"call_{addr}_{name}_{b}.json")
            if int(call, 16) != int(value, 16):
                raise SystemExit(f"{addr} slot {slot} at {b}: {value} but {sig} gives {call}")
            entry["getter"] = sig if len(parts) == 4 else f"{sig} of {parts[4]}"
        storage.append(entry)
        pause()

    manifest = {
        "network": a.network,
        "chain_id": net["chain_id"],
        "record": r,
        "window": WINDOW,
        "chains": chains,
        "headers": {str(b): hashes[b] for b in sorted(hashes, reverse=True)},
        "history_slots": {str(b): sorted(slots[b]) for b in sorted(slots, reverse=True)},
        "receipts": a.receipts,
        "transfers": transfers,
        "storage": storage,
        "extra_headers": a.header,
    }
    f.save("fixture.json", (json.dumps(manifest, indent=2) + "\n").encode())
    lines = [
        f"# EIP-2935 hop chains on {a.network} from block {r}",
        "",
        f"Fetched by `lez/scripts/fetch-hop-chain.py {' '.join(sys.argv[1:-1])} OUTDIR` between "
        f"{e2e.LOG[0]['at']} and {e2e.LOG[-1]['at']} (UTC). Every request went to a public endpoint without an API "
        "key, with User-Agent `ferrata-fixture-fetch`. Responses are stored byte for byte; each `header_N.rlp` is "
        "rebuilt from `block_N.json` and was checked to hash to the hash both providers gave; each hop's history "
        "slot was checked to hold that hash; `fixture.json` records the choices. `SHA256SUMS` pins every file.",
        "",
        "Chains (newest first, from the record's block): "
        + "; ".join(f"{len(c)} hops to {c[-1]}" for c in chains) + ".",
        "",
        "## Requests",
        "",
        "| UTC | endpoint | request | file | note |",
        "|---|---|---|---|---|",
    ]
    lines += [f"| {x['at']} | {x['endpoint']} | `{x['request']}` | {x['file'] or ''} | {x['note']} |" for x in e2e.LOG]
    f.save("SOURCE.md", ("\n".join(lines) + "\n").encode())
    files = sorted(x for x in os.listdir(a.outdir) if x != "SHA256SUMS")
    with open(os.path.join(a.outdir, "SHA256SUMS"), "w") as s:
        for x in files:
            with open(os.path.join(a.outdir, x), "rb") as fh:
                s.write(f"{hashlib.sha256(fh.read()).hexdigest()}  {x}\n")
    print(f"{a.outdir}: {len(files)} files, {len(e2e.LOG)} requests")


if __name__ == "__main__":
    try:
        main()
    except urllib.error.URLError as e:
        raise SystemExit(f"network: {e}")
