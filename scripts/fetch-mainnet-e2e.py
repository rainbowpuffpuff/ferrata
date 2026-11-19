#!/usr/bin/env python3
"""Fetch the mainnet end-to-end fixture for `tests/mainnet_e2e.rs`.

What it fetches, in this order (most public nodes keep state for about 128 blocks, and the
finalized block is already 64 to 95 blocks old, so the state-dependent requests come first):

1. the latest light-client finality update (SSZ) from a beacon node, which finalizes execution
   block `n`; then `k`, the newest block before `n` with a failed transaction (receipts scan);
2. at `m1 = n - HOP` and at `n`: `eth_getProof` for WETH (a non-zero balance slot and an empty
   slot), USDC (a balance slot), an address that has no account, and the EIP-2935 history contract
   (the slots of m1 and k at n, the slot of m2 at m1); `eth_call balanceOf` for the same holder;
   at `m2 = m1 - 8191` (the oldest block m1 reaches), the WETH slot again if any public node
   serves state that old;
3. the block headers of n, k, m1, m2 and m2 - 1 (one block past m1's window), rebuilt as RLP from
   `eth_getBlockByNumber` and checked against their hashes; the history contract's code;
   `eth_getBlockReceipts` for n and k;
4. the same headers from a second execution provider, as a cross-check;
5. a light-client bootstrap for a finalized checkpoint earlier in the same sync-committee period,
   and the canonical roots of both beacon blocks from a second beacon provider.

Python standard library only. Every request goes to a public endpoint without an API key, with the
generic User-Agent of scripts/pubnet.py (`curl/8`) and nothing else that identifies the caller. A
transient failure (429, 5xx, a timeout, a dropped connection) is tried again on the same endpoint
after a backoff with jitter (pubnet.retrying), so the provenance of every answer stays one endpoint;
when every execution provider failed transiently, the round is tried again. Each response is saved
byte for byte, and SOURCE.md and SHA256SUMS are written at the end.

Usage: scripts/fetch-mainnet-e2e.py OUTDIR      (OUTDIR must not exist yet)
"""
import datetime
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pubnet  # noqa: E402  (scripts/pubnet.py: transient/permanent, backoff; e2e/NETWORK.md)

UA = pubnet.UA
BEACON = ["https://lodestar-mainnet.chainsafe.io", "https://ethereum-beacon-api.publicnode.com"]
EXECUTION = [
    "https://ethereum-rpc.publicnode.com",
    "https://eth.drpc.org",
    "https://1rpc.io/eth",
    "https://rpc.mevblocker.io",
    "https://eth.rpc.blxrbdn.com",
]

WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
USDC = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
HISTORY = "0x0000f90827f1c53a10cb7a02335b175320002935"
# Uniswap v2 USDC/WETH pair: holds both tokens, so one holder serves both balance slots.
HOLDER = "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"
WETH_BALANCE_SLOT = 3  # WETH9: name, symbol, decimals, balanceOf, allowance
USDC_BALANCE_SLOT = 9  # FiatTokenV2_2: balanceAndBlacklistStates
EMPTY_SLOT = "0x" + "00" * 31 + "ff"  # WETH9 writes only slots 0-4 and mapping entries
HISTORY_SERVE_WINDOW = 8191
HOP = 16
FAILED_SCAN = 30
SLOTS_PER_PERIOD = 8192

LOG = []


# --- Keccak-256 (pure Python; only used to check rebuilt headers and to derive storage keys) ---

_RC = [
    0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000,
    0x000000000000808B, 0x0000000080000001, 0x8000000080008081, 0x8000000000008009,
    0x000000000000008A, 0x0000000000000088, 0x0000000080008009, 0x000000008000000A,
    0x000000008000808B, 0x800000000000008B, 0x8000000000008089, 0x8000000000008003,
    0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
    0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008,
]
_ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]
_M = (1 << 64) - 1


def _rol(v, n):
    return ((v << n) | (v >> (64 - n))) & _M if n else v


def _keccak_f(a):
    for rc in _RC:
        c = [a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4] for x in range(5)]
        d = [c[(x - 1) % 5] ^ _rol(c[(x + 1) % 5], 1) for x in range(5)]
        a = [[a[x][y] ^ d[x] for y in range(5)] for x in range(5)]
        b = [[0] * 5 for _ in range(5)]
        for x in range(5):
            for y in range(5):
                b[y][(2 * x + 3 * y) % 5] = _rol(a[x][y], _ROT[x][y])
        a = [[b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]) for y in range(5)] for x in range(5)]
        a[0][0] ^= rc
    return a


def keccak256(data):
    rate = 136
    msg = bytearray(data) + b"\x01"
    while len(msg) % rate:
        msg.append(0)
    msg[-1] |= 0x80
    a = [[0] * 5 for _ in range(5)]
    for off in range(0, len(msg), rate):
        for i in range(rate // 8):
            a[i % 5][i // 5] ^= int.from_bytes(msg[off + 8 * i:off + 8 * i + 8], "little")
        a = _keccak_f(a)
    return b"".join(a[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


assert keccak256(b"").hex() == "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
assert keccak256(b"abc").hex() == "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"


# --- RLP encoding (header rebuild) ---

def _len_prefix(n, offset):
    if n < 56:
        return bytes([offset + n])
    nb = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return bytes([offset + 55 + len(nb)]) + nb


def rlp_bytes(b):
    return b if len(b) == 1 and b[0] < 0x80 else _len_prefix(len(b), 0x80) + b


def rlp_int(i):
    return rlp_bytes(i.to_bytes((i.bit_length() + 7) // 8, "big") if i else b"")


def rlp_list(items):
    payload = b"".join(items)
    return _len_prefix(len(payload), 0xC0) + payload


# eth_getBlockByNumber field names in header order; "i" fields are integers.
HEADER_FIELDS = [
    ("parentHash", "b"), ("sha3Uncles", "b"), ("miner", "b"), ("stateRoot", "b"), ("transactionsRoot", "b"),
    ("receiptsRoot", "b"), ("logsBloom", "b"), ("difficulty", "i"), ("number", "i"), ("gasLimit", "i"),
    ("gasUsed", "i"), ("timestamp", "i"), ("extraData", "b"), ("mixHash", "b"), ("nonce", "b"),
    ("baseFeePerGas", "i"), ("withdrawalsRoot", "b"), ("blobGasUsed", "i"), ("excessBlobGas", "i"),
    ("parentBeaconBlockRoot", "b"), ("requestsHash", "b"), ("blockAccessListHash", "b"), ("slotNumber", "i"),
]


def header_rlp(block):
    items, present = [], [name for name, _ in HEADER_FIELDS if name in block]
    names = [name for name, _ in HEADER_FIELDS]
    if present != names[: len(present)]:
        raise SystemExit(f"header fields are not a prefix of the known order: {present}")
    for name, kind in HEADER_FIELDS[: len(present)]:
        v = bytes.fromhex(block[name][2:]) if kind == "b" else None
        items.append(rlp_bytes(v) if kind == "b" else rlp_int(int(block[name], 16)))
    return rlp_list(items)


# --- SSZ (light-client finality update and header, Electra layout) ---

def u32(b, o):
    return int.from_bytes(b[o:o + 4], "little")


def u64(b, o):
    return int.from_bytes(b[o:o + 8], "little")


def parse_lc_header(h):
    beacon = h[:112]
    if u32(h, 112) != 244:
        raise SystemExit("unexpected LightClientHeader layout (not Electra)")
    e = h[244:]
    return {
        "slot": u64(beacon, 0),
        "beacon": beacon,
        "block_number": u64(e, 404),
        "parent_hash": e[0:32].hex(),
        "state_root": e[52:84].hex(),
        "receipts_root": e[84:116].hex(),
        "timestamp": u64(e, 428),
        "block_hash": e[472:504].hex(),
    }


def parse_finality_update(b):
    att, fin = u32(b, 0), u32(b, 4)
    if att != 400 or not att < fin <= len(b):
        raise SystemExit("unexpected LightClientFinalityUpdate layout (not Electra)")
    bits = b[232:296]
    return {
        "attested": parse_lc_header(b[att:fin]),
        "finalized": parse_lc_header(b[fin:]),
        "signature_slot": u64(b, 392),
        "participants": sum(bin(x).count("1") for x in bits),
    }


def beacon_header_root(beacon):
    chunks = [beacon[0:8] + b"\0" * 24, beacon[8:16] + b"\0" * 24, beacon[16:48], beacon[48:80], beacon[80:112]]
    chunks += [b"\0" * 32] * 3
    while len(chunks) > 1:
        chunks = [hashlib.sha256(chunks[i] + chunks[i + 1]).digest() for i in range(0, len(chunks), 2)]
    return chunks[0].hex()


# --- HTTP ---

def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def http(url, body=None, accept="application/json"):
    headers = {"User-Agent": UA, "Accept": accept}
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=body, headers=headers)
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read(), dict(r.headers)


class Fetcher:
    def __init__(self, out):
        self.out = out

    def save(self, name, data):
        with open(os.path.join(self.out, name), "wb") as f:
            f.write(data)

    def beacon(self, endpoint, path, name, ssz):
        at = now()
        data, headers = pubnet.retrying(
            lambda: http(endpoint + path, accept="application/octet-stream" if ssz else "application/json"),
            budget=float(os.environ.get("PUBNET_REQUEST_BUDGET_S", "180")), what=f"GET {path}")
        version = headers.get("Eth-Consensus-Version") or headers.get("eth-consensus-version") or ""
        LOG.append({"at": at, "endpoint": endpoint, "request": f"GET {path}", "file": name, "note": version})
        if name:
            self.save(name, data)
        return data

    def rpc(self, endpoint, method, params, name):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        at = now()
        data, _ = http(endpoint, body=body)
        res = json.loads(data)
        if "result" not in res or res["result"] is None:
            raise RuntimeError(f"{endpoint} {method}: {str(res.get('error'))[:200]}")
        LOG.append({"at": at, "endpoint": endpoint, "request": f"{method} {json.dumps(params)}", "file": name,
                    "note": ""})
        if name:
            self.save(name, data)
        return res["result"]

    def rpc_any(self, method, params, name, providers=EXECUTION):
        errors = []
        for round_ in range(1, 4):  # every provider once per round; another round only if all failed transiently
            for endpoint in providers:
                try:
                    return self.rpc(endpoint, method, params, name), endpoint
                except (RuntimeError, OSError, TimeoutError, json.JSONDecodeError, ValueError) as e:
                    errors.append((endpoint, e))
            last = errors[-len(providers):]
            if round_ == 3 or not all(pubnet.transient_exception(e) for _, e in last):
                break
            time.sleep(pubnet.backoff(round_, base=4.0, cap=30.0))
        raise SystemExit(f"no provider served {method} {params}:\n  " + "\n  ".join(f"{u}: {e}" for u, e in errors))


def mapping_slot(holder, base):
    return "0x" + keccak256(bytes.fromhex(holder[2:]).rjust(32, b"\0") + base.to_bytes(32, "big")).hex()


def word(i):
    return "0x" + i.to_bytes(32, "big").hex()


ABSENT = "0x" + keccak256(b"ferrata/absent-account")[12:].hex()


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out)
    f = Fetcher(out)
    started = now()

    # 1. Finality update, and the newest earlier block with a failed transaction.
    fu_raw = f.beacon(BEACON[0], "/eth/v1/beacon/light_client/finality_update", "finality_update.ssz", ssz=True)
    fu = parse_finality_update(fu_raw)
    fin = fu["finalized"]
    n = fin["block_number"]
    m1 = n - HOP
    m2 = m1 - HISTORY_SERVE_WINDOW
    period = fu["signature_slot"] // SLOTS_PER_PERIOD
    if fin["slot"] // SLOTS_PER_PERIOD != period:
        raise SystemExit("finalized slot and signature slot are in different periods; retry later")
    print(f"finalized slot {fin['slot']} block {n} 0x{fin['block_hash']} (period {period})", flush=True)
    k = None
    for number in range(n - 1, n - 1 - FAILED_SCAN, -1):
        receipts, _ = f.rpc_any("eth_getBlockReceipts", [hex(number)], None)
        if any(r["status"] == "0x0" for r in receipts):
            k = number
            break
    if k is None:
        raise SystemExit(f"no failed transaction in the {FAILED_SCAN} blocks before {n}")

    # 2. State-dependent requests, oldest block first.
    weth_holder = mapping_slot(HOLDER, WETH_BALANCE_SLOT)
    usdc_holder = mapping_slot(HOLDER, USDC_BALANCE_SLOT)
    call = "0x70a08231" + HOLDER[2:].rjust(64, "0")
    served = {}
    _, served["m1"] = f.rpc_any("eth_getProof", [WETH, [weth_holder], hex(m1)], f"proof_weth_{m1}.json")
    at_m1 = [served["m1"]] + EXECUTION
    f.rpc_any("eth_getProof", [HISTORY, [word(m2 % HISTORY_SERVE_WINDOW)], hex(m1)], f"proof_history_{m1}.json", at_m1)
    f.rpc_any("eth_call", [{"to": WETH, "data": call}, hex(m1)], f"call_weth_balance_{m1}.json", at_m1)
    _, served["n"] = f.rpc_any("eth_getProof", [WETH, [weth_holder, EMPTY_SLOT], hex(n)], f"proof_weth_{n}.json")
    at_n = [served["n"]] + EXECUTION
    f.rpc_any("eth_getProof", [USDC, [usdc_holder], hex(n)], f"proof_usdc_{n}.json", at_n)
    f.rpc_any("eth_getProof", [ABSENT, [word(0)], hex(n)], f"proof_absent_{n}.json", at_n)
    history_slots = sorted({word(m1 % HISTORY_SERVE_WINDOW), word(k % HISTORY_SERVE_WINDOW)})
    f.rpc_any("eth_getProof", [HISTORY, history_slots, hex(n)], f"proof_history_{n}.json", at_n)
    f.rpc_any("eth_call", [{"to": WETH, "data": call}, hex(n)], f"call_weth_balance_{n}.json", at_n)
    f.rpc_any("eth_call", [{"to": USDC, "data": call}, hex(n)], f"call_usdc_balance_{n}.json", at_n)
    f.rpc_any("eth_getCode", [HISTORY, hex(n)], f"code_history_{n}.json", at_n)
    head, _ = f.rpc_any("eth_blockNumber", [], None, at_n)
    print(f"proofs at {m1} and {n} served; head {int(head, 16)}", flush=True)
    try:
        _, served["m2"] = f.rpc_any("eth_getProof", [WETH, [weth_holder], hex(m2)], f"proof_weth_{m2}.json")
        f.rpc_any("eth_call", [{"to": WETH, "data": call}, hex(m2)], f"call_weth_balance_{m2}.json",
                  [served["m2"]] + EXECUTION)
    except SystemExit as e:
        served["m2"] = None
        print(f"no public node served state at {m2}: {e}", flush=True)

    # 3. Headers (rebuilt as RLP and checked) and receipts.
    rebuilt = {}
    for number in sorted({n, k, m1, m2, m2 - 1}, reverse=True):
        block, served[f"block_{number}"] = f.rpc_any("eth_getBlockByNumber", [hex(number), False],
                                                     f"block_{number}.json")
        rlp = header_rlp(block)
        if "0x" + keccak256(rlp).hex() != block["hash"] or int(block["number"], 16) != number:
            raise SystemExit(f"rebuilt header {number} does not hash to {block['hash']}")
        f.save(f"header_{number}.rlp", rlp)
        rebuilt[number] = block
    if rebuilt[n]["hash"] != "0x" + fin["block_hash"] or rebuilt[n]["stateRoot"] != "0x" + fin["state_root"]:
        raise SystemExit("execution block does not match the finalized light-client header")
    counts = {}
    for number in sorted({n, k}, reverse=True):
        receipts, _ = f.rpc_any("eth_getBlockReceipts", [hex(number)], f"receipts_{number}.json")
        counts[number] = (len(receipts), sum(1 for r in receipts if r["status"] == "0x0"))
        print(f"receipts {number}: {counts[number][0]} ({counts[number][1]} failed)", flush=True)

    # 4. Second execution provider.
    second = [e for e in EXECUTION if e != served[f"block_{n}"]]
    for number in sorted({n, k, m1, m2}, reverse=True):
        block, _ = f.rpc_any("eth_getBlockByNumber", [hex(number), False], f"block_{number}_second.json", second)
        for field in ["hash", "stateRoot", "receiptsRoot"]:
            if block[field] != rebuilt[number][field]:
                raise SystemExit(f"providers disagree on block {number} {field}")

    # 5. Bootstrap for a checkpoint earlier in the same period, and second-provider beacon roots.
    first_epoch = period * SLOTS_PER_PERIOD // 32
    boot_slot = boot_root = None
    for epoch in range(first_epoch, fin["slot"] // 32):
        slot = epoch * 32
        try:
            hdr = json.loads(f.beacon(BEACON[1], f"/eth/v1/beacon/headers/{slot}", None, ssz=False))["data"]
        except urllib.error.HTTPError:
            continue  # empty slot: no checkpoint block at the epoch start
        try:
            f.beacon(BEACON[0], f"/eth/v1/beacon/light_client/bootstrap/{hdr['root']}", "bootstrap.ssz", ssz=True)
        except urllib.error.HTTPError as e:
            print(f"bootstrap for slot {slot} refused ({e.code}); trying the next epoch", flush=True)
            continue
        boot_slot, boot_root = slot, hdr["root"]
        break
    if boot_slot is None:
        raise SystemExit("no bootstrap served for this period")
    for slot, name in [(boot_slot, "bootstrap"), (fin["slot"], "finalized")]:
        raw = f.beacon(BEACON[1], f"/eth/v1/beacon/headers/{slot}", f"beacon_header_{slot}_second.json", ssz=False)
        root = json.loads(raw)["data"]["root"]
        if name == "finalized" and root != "0x" + beacon_header_root(fin["beacon"]):
            raise SystemExit("second beacon provider reports another finalized root")
        if name == "bootstrap" and root != boot_root:
            raise SystemExit("bootstrap root changed between requests")

    # SOURCE.md and SHA256SUMS.
    files = sorted(x for x in os.listdir(out) if x not in ("SOURCE.md", "SHA256SUMS"))
    sums = {x: hashlib.sha256(open(os.path.join(out, x), "rb").read()).hexdigest() for x in files}
    with open(os.path.join(out, "SHA256SUMS"), "w") as s:
        s.writelines(f"{sums[x]}  {x}\n" for x in files)
    m2_state = (f"served by {served['m2']}" if served["m2"] else "not served by any public node tried")
    lines = [
        "# Mainnet end-to-end fixture: light-client finality to execution facts",
        "",
        f"Fetched by `scripts/fetch-mainnet-e2e.py` between {started} and {now()} (UTC). Every request",
        f"went to a public endpoint without an API key, with User-Agent `{UA}`. Responses are stored",
        "byte for byte; `header_*.rlp` are rebuilt from `block_*.json` by the script, which checked that",
        "each hashes to the block's hash. `SHA256SUMS` pins every file.",
        "",
        "| | |",
        "|---|---|",
        f"| sync-committee period | {period} |",
        f"| bootstrap | slot {boot_slot}, block root {boot_root} |",
        f"| finality update | attested slot {fu['attested']['slot']}, signature slot {fu['signature_slot']}, "
        f"{fu['participants']}/512 signers |",
        f"| finalized beacon block | slot {fin['slot']}, root 0x{beacon_header_root(fin['beacon'])} |",
        f"| finalized execution block `n` | {n}, hash 0x{fin['block_hash']} |",
        f"| `n` state root / receipts root | 0x{fin['state_root']} / 0x{fin['receipts_root']} |",
        f"| `n` receipts | {counts[n][0]} ({counts[n][1]} failed) |",
        f"| `k`, newest block before n with a failed transaction | {k}, hash {rebuilt[k]['hash']}, "
        f"{counts[k][0]} receipts ({counts[k][1]} failed) |",
        f"| `m1` = n - {HOP} (one EIP-2935 hop from n) | {m1}, hash {rebuilt[m1]['hash']} |",
        f"| `m2` = m1 - 8191 (one hop from m1, the oldest block it reaches) | {m2}, hash {rebuilt[m2]['hash']}; "
        f"state {m2_state} |",
        f"| `m2 - 1` (one block past m1's window) | {m2 - 1}, hash {rebuilt[m2 - 1]['hash']} |",
        f"| execution head when the proofs at n were fetched | {int(head, 16)} |",
        f"| holder (Uniswap v2 USDC/WETH pair) | {HOLDER} |",
        f"| WETH balance slot, empty slot | {weth_holder}, {EMPTY_SLOT} |",
        f"| USDC balance slot | {usdc_holder} |",
        f"| address without an account | {ABSENT} (`keccak256(\"ferrata/absent-account\")[12..]`) |",
        "",
        "Cross-checks made by the script: the finalized light-client header's execution block hash and state",
        "root equal those of `block_n`; a second execution provider (`block_*_second.json`) reports the same",
        "hash, state root and receipts root for n, k, m1 and m2; a second beacon provider",
        "(`beacon_header_*_second.json`) reports the finalized block root computed from the update, and the",
        "bootstrap's block root.",
        "",
        "## Requests",
        "",
        "| UTC | endpoint | request | file | note |",
        "|---|---|---|---|---|",
    ]
    lines += [f"| {r['at']} | {r['endpoint']} | `{r['request']}` | {r['file'] or ''} | {r['note']} |" for r in LOG]
    with open(os.path.join(out, "SOURCE.md"), "w") as s:
        s.write("\n".join(lines) + "\n")
    print("done:", out)


if __name__ == "__main__":
    main()
