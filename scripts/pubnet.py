#!/usr/bin/env python3
"""pubnet: how Ferrata's scripts talk to a public network (e2e/NETWORK.md). Python standard library only.

Free public endpoints are load-balanced over nodes a few blocks apart: a read at a block one node announced can reach
a node without it ("block not found", "header not found", "missing trie node"), a read at "latest" can come from a node
behind the transaction just sent, and any of them can answer 429, 5xx or nothing. This module, and the local JSON-RPC
proxy it runs for forge and cast, does five things about it:

  1. an ordered list of free endpoints per network (scripts/pubnet-endpoints.json), the script's own first;
  2. every error classified transient (lag, 429, 5xx, timeouts, connections) or permanent (revert, invalid, funds);
     only transient ones are retried: on the next endpoint, then after an exponential backoff with jitter, within a
     time budget (a request ~30 s; a stage, in pubnet.sh, 20 min);
  3. reads at "latest" pinned to a block `lag` behind the highest head seen, and never below the block of the sender's
     last mined transaction; a read at an explicit block is either right or fails (a node without the block says so),
     so every answer a client gets describes one block. forge's simulation then runs on that block. That is safe
     because a send is checked again against the chain's head before it leaves (4);
  4. before a raw transaction leaves: its nonce equal to the sender's latest and pending nonce on two endpoints, the
     balance enough for value + gas x max fee, no earlier transaction of the journal without a known fate; the signed
     bytes and their hash written (fsync) to the journal BEFORE the first broadcast; then broadcast to every endpoint
     (the same bytes are one transaction however often they arrive); receipts asked of every endpoint;
  5. a transaction whose fate is unknown blocks every later send until it is found mined, replaced or rejected
     (`resolve`), so a stopped and resumed stage never sends twice.

CLI:
  pubnet.py endpoints <network> [rpc|beacon]          the list, in order
  pubnet.py probe <network>                           each endpoint asked read-only questions; a table
  pubnet.py proxy --network N --journal F [--sender A] [--first URL] [--port-file P] [--lag K] [--allow-unlocked]
  pubnet.py resolve --network N --journal F --sender A [--first URL] [--budget S]
                                                      exit 0: every journaled transaction has a fate and all were
                                                      mined with status 1; 3: one reverted, was replaced or rejected
                                                      (a human looks); 75: a fate is still unknown (try again later)
  pubnet.py rows --journal F --stage S                the stage's mined transactions as manifest rows (JSON)
  pubnet.py classify [--events F --since T] [LOG]     transient | permanent: why the last attempt failed
  pubnet.py selftest                                  keccak, RLP, classification
"""
import argparse
import http.client
import json
import os
import random
import re
import socket
import sys
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
UA = "curl/8"  # a generic User-Agent: nothing about who asks
TEMPFAIL = 75  # exit status: transient, try again later (sysexits EX_TEMPFAIL)

# ---- endpoints ------------------------------------------------------------------------------------------------------


def endpoints(network: str, kind: str = "rpc", first: str | None = None) -> list[str]:
    """The ordered list; PUBNET_<NETWORK>_<KIND> (comma-separated) replaces it; `first` goes in front. A local `first`
    (a fork or a devnet on this machine) is the only endpoint: a public one must never answer for a local chain."""
    env = os.environ.get(f"PUBNET_{network.upper()}_{kind.upper()}")
    if first and loopback(first) and not env:
        return [first]
    if env:
        urls = [u.strip() for u in env.split(",") if u.strip()]
    else:
        with open(os.path.join(HERE, "pubnet-endpoints.json")) as f:
            urls = list(json.load(f)[network][kind])
    if first:
        urls = [first] + [u for u in urls if u.rstrip("/") != first.rstrip("/")]
    return urls


def chain_id(network: str) -> int | None:
    try:
        with open(os.path.join(HERE, "pubnet-endpoints.json")) as f:
            return json.load(f)[network]["chainId"]
    except (OSError, KeyError):
        return None


def label(url: str) -> str:
    """An endpoint as logs show it: scheme and host only (a key in a path or a user part never reaches a log)."""
    u = urllib.parse.urlsplit(url)
    return f"{u.scheme}://{u.hostname}" + (f":{u.port}" if u.port else "")


def loopback(url: str) -> bool:
    return urllib.parse.urlsplit(url).hostname in ("127.0.0.1", "localhost", "::1")


# ---- errors ---------------------------------------------------------------------------------------------------------

TRANSIENT_RE = re.compile(
    r"block not found|header not found|unknown block|missing trie node|failed to get (account|storage|block)|"
    r"non-archive|not available|state .*pruned|pruned|-32001\b|-32005\b|too many requests|rate.?limit|\b429\b|"
    r"\b50[0234]\b|bad gateway|service unavailable|gateway time|timed? ?out|timeout|connection (refused|reset|aborted)|"
    r"remote end closed|broken pipe|temporarily|try again|overloaded|no response|eof occurred|network is unreachable|"
    r"name or service not known|temporary failure in name resolution|pubnet: transient", re.I)
PERMANENT_RE = re.compile(
    r"execution reverted|revert|invalid (argument|params|opcode|sender|signature|transaction|chain)|"
    r"insufficient funds|nonce too (low|high)|intrinsic gas|gas required exceeds|exceeds block gas limit|"
    r"replacement transaction underpriced|max fee per gas less than|fee cap|pubnet: permanent", re.I)
UNSUPPORTED_RE = re.compile(r"method not found|not supported|unsupported|-32601\b|not whitelisted|not allowed|"
                            r"disabled|requires? (an )?api key|unauthori[sz]ed|forbidden|\b40[13]\b", re.I)


class Transient(Exception):
    pass


class Permanent(Exception):
    pass


def classify(text: str) -> str:
    """transient | permanent. A message that names a revert, invalid input or missing funds is permanent even when
    it also names a timeout; an unknown message is permanent (a human looks rather than a loop)."""
    if PERMANENT_RE.search(text or ""):
        return "permanent"
    if TRANSIENT_RE.search(text or ""):
        return "transient"
    return "permanent"


def backoff(attempt: int, base: float = 2.0, cap: float = 60.0) -> float:
    """Seconds before retry `attempt` (1, 2, ...): base * 2^(attempt-1), at most cap, times a jitter of 0.5..1.5."""
    return min(cap, base * 2 ** (attempt - 1)) * random.uniform(0.5, 1.5)


# ---- HTTP, one kept-alive connection per thread and host --------------------------------------------------------

_tls = threading.local()


def http_request(url: str, body: bytes | None = None, accept: str = "application/json",
                 timeout: float = 15.0) -> tuple[int, bytes, dict]:
    u = urllib.parse.urlsplit(url)
    pool = getattr(_tls, "pool", None)
    if pool is None:
        pool = _tls.pool = {}
    key = (u.scheme, u.hostname, u.port)
    conn = pool.get(key)
    if conn is None:
        cls = http.client.HTTPSConnection if u.scheme == "https" else http.client.HTTPConnection
        conn = pool[key] = cls(u.hostname, u.port, timeout=timeout)
    path = (u.path or "/") + (f"?{u.query}" if u.query else "")
    headers = {"User-Agent": UA, "Accept": accept}
    if body is not None:
        headers["Content-Type"] = "application/json"
    try:
        conn.timeout = timeout
        if conn.sock is not None:
            conn.sock.settimeout(timeout)
        conn.request("POST" if body is not None else "GET", path, body=body, headers=headers)
        r = conn.getresponse()
        data = r.read()
        if r.getheader("connection", "").lower() == "close":
            conn.close()
            pool.pop(key, None)
        return r.status, data, {k.lower(): v for k, v in r.getheaders()}
    except BaseException:
        conn.close()
        pool.pop(key, None)
        raise


def _rpc_once(url: str, method: str, params: list, timeout: float) -> object:
    """One JSON-RPC call to one endpoint: the result, or Transient / Permanent (UNSUPPORTED counts as transient for
    this endpoint: the next one may serve it)."""
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    try:
        status, data, _ = http_request(url, body, timeout=timeout)
    except (OSError, http.client.HTTPException, socket.timeout) as e:
        raise Transient(f"{label(url)}: {type(e).__name__}: {e}") from None
    if status != 200:
        msg = f"{label(url)}: HTTP {status} {data[:160]!r}"
        if status == 429 or status >= 500 or UNSUPPORTED_RE.search(msg):
            raise Transient(msg)
        raise Permanent(msg)
    try:
        res = json.loads(data)
    except json.JSONDecodeError:
        raise Transient(f"{label(url)}: not JSON: {data[:120]!r}") from None
    if isinstance(res, dict) and res.get("error") is not None:
        err = res["error"]
        msg = f"{label(url)}: {err.get('code') if isinstance(err, dict) else ''} " \
              f"{err.get('message') if isinstance(err, dict) else err}"
        if isinstance(err, dict) and err.get("data") is not None:
            msg += f" data={str(err['data'])[:200]}"
        if UNSUPPORTED_RE.search(msg) and not PERMANENT_RE.search(msg):
            raise Transient(msg)
        if classify(msg) == "transient":
            raise Transient(msg)
        e = Permanent(msg)
        e.rpc_error = err
        raise e
    if not isinstance(res, dict) or "result" not in res:
        raise Transient(f"{label(url)}: no result: {data[:120]!r}")
    return res["result"]


class Net:
    """Failover over an ordered endpoint list, backoff with jitter, a time budget per call."""

    def __init__(self, network: str = "sepolia", first: str | None = None, urls: list[str] | None = None,
                 request_budget: float | None = None, timeout: float | None = None, log=None):
        self.network = network
        self.urls = urls or endpoints(network, "rpc", first)
        self.budget = request_budget or float(os.environ.get("PUBNET_REQUEST_BUDGET_S", "30"))
        self.timeout = timeout or float(os.environ.get("PUBNET_TIMEOUT_S", "15"))
        self.cool: dict[str, float] = {}  # endpoint -> monotonic time before which it is skipped
        self.log = log or (lambda msg: None)

    def order(self) -> list[str]:
        now = time.monotonic()
        ready = [u for u in self.urls if self.cool.get(u, 0) <= now]
        return ready + [u for u in self.urls if u not in ready]

    def call(self, method: str, params: list, *, must=False, urls: list[str] | None = None,
             budget: float | None = None, accept=None):
        """The first answer of the endpoints in order. must: a null result is a missing block (transient).
        accept(result) -> bool: an answer it refuses counts as transient (e.g. an inconsistent one)."""
        deadline = time.monotonic() + (budget or self.budget)
        attempt, last = 0, "no endpoint"
        while True:
            for url in (urls or self.order()):
                try:
                    r = _rpc_once(url, method, params, self.timeout)
                    if r is None and must:
                        raise Transient(f"{label(url)}: null (the node does not have it yet)")
                    if accept is not None and not accept(r):
                        raise Transient(f"{label(url)}: an answer refused as inconsistent")
                    return r
                except Transient as e:
                    last = str(e)
                    self.cool[url] = time.monotonic() + 5
                    self.log(f"transient {method}: {last}")
                if time.monotonic() > deadline:
                    break
            attempt += 1
            pause = backoff(attempt, base=1.0, cap=8.0)
            if time.monotonic() + pause > deadline:
                raise Transient(f"pubnet: transient: {method} failed on every endpoint for {budget or self.budget:.0f} s;"
                                f" last: {last}")
            time.sleep(pause)

    def each(self, method: str, params: list, n: int | None = None) -> list[tuple[str, object]]:
        """Up to n answers from different endpoints (errors skipped), in list order."""
        out = []
        for url in self.order():
            try:
                out.append((url, _rpc_once(url, method, params, self.timeout)))
            except (Transient, Permanent) as e:
                self.cool[url] = time.monotonic() + 5
                self.log(f"each {method}: {e}")
            if n and len(out) >= n:
                break
        return out

    def head(self) -> int:
        """The highest head of up to two endpoints (a lagging one reports less, never more)."""
        heads = [int(r, 16) for _, r in self.each("eth_blockNumber", [], n=2)]
        if not heads:
            return int(self.call("eth_blockNumber", []), 16)
        return max(heads)

    def beacon(self, path: str, ssz: bool = False, urls: list[str] | None = None,
               budget: float | None = None) -> tuple[bytes, dict, str]:
        """GET a beacon API path with failover: (body, headers, endpoint). 404 is permanent (a wrong path)."""
        urls = urls or endpoints(self.network, "beacon")
        accept = "application/octet-stream" if ssz else "application/json"
        deadline = time.monotonic() + (budget or self.budget)
        attempt, last = 0, ""
        while True:
            for url in urls:
                try:
                    status, data, headers = http_request(url.rstrip("/") + path, accept=accept, timeout=self.timeout)
                except (OSError, http.client.HTTPException) as e:
                    last = f"{label(url)}: {type(e).__name__}: {e}"
                    continue
                if status == 200:
                    return data, headers, url
                last = f"{label(url)}: HTTP {status} {data[:120]!r}"
                if status not in (429,) and status < 500:
                    raise Permanent(last)
            attempt += 1
            pause = backoff(attempt, base=1.0, cap=8.0)
            if time.monotonic() + pause > deadline:
                raise Transient(f"pubnet: transient: GET {path} failed on every endpoint; last: {last}")
            time.sleep(pause)


def retrying(fn, budget: float = 120.0, what: str = "request"):
    """fn() again after a transient failure (same endpoint), backoff with jitter, within the budget: for scripts whose
    provenance names one endpoint per answer (the fixture fetchers' cross-checks must not fail over silently)."""
    deadline = time.monotonic() + budget
    attempt = 0
    while True:
        try:
            return fn()
        except Exception as e:  # noqa: BLE001 - classified below
            if classify(f"{type(e).__name__}: {e}") != "transient" and not isinstance(e, (OSError, Transient)):
                raise
            attempt += 1
            pause = backoff(attempt, base=2.0, cap=30.0)
            if time.monotonic() + pause > deadline:
                raise
            print(f"pubnet: {what}: {type(e).__name__}: {str(e)[:160]}; again in {pause:.0f} s", file=sys.stderr)
            time.sleep(pause)


# ---- keccak-256, RLP, transactions ----------------------------------------------------------------------------------

_RC = [0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000, 0x000000000000808B,
       0x0000000080000001, 0x8000000080008081, 0x8000000000008009, 0x000000000000008A, 0x0000000000000088,
       0x0000000080008009, 0x000000008000000A, 0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
       0x8000000000008003, 0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
       0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008]
_ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]
_M = (1 << 64) - 1


def _keccak_f(a):
    for rc in _RC:
        c = [a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4] for x in range(5)]
        d = [c[(x - 1) % 5] ^ (((c[(x + 1) % 5] << 1) | (c[(x + 1) % 5] >> 63)) & _M) for x in range(5)]
        a = [[a[x][y] ^ d[x] for y in range(5)] for x in range(5)]
        b = [[0] * 5 for _ in range(5)]
        for x in range(5):
            for y in range(5):
                r = _ROT[x][y]
                b[y][(2 * x + 3 * y) % 5] = (((a[x][y] << r) | (a[x][y] >> (64 - r))) & _M) if r else a[x][y]
        a = [[b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]) for y in range(5)] for x in range(5)]
        a[0][0] ^= rc
    return a


def keccak256(data: bytes) -> bytes:
    rate = 136
    p = bytearray(data)
    p.append(0x01)
    p.extend(b"\0" * (-len(p) % rate))
    p[-1] |= 0x80
    a = [[0] * 5 for _ in range(5)]
    for off in range(0, len(p), rate):
        for i in range(rate // 8):
            a[i % 5][i // 5] ^= int.from_bytes(p[off + 8 * i:off + 8 * i + 8], "little")
        a = _keccak_f(a)
    return b"".join(a[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


def rlp_decode(b: bytes, i: int = 0):
    p = b[i]
    if p < 0x80:
        return b[i:i + 1], i + 1
    if p < 0xb8:
        return b[i + 1:i + 1 + p - 0x80], i + 1 + p - 0x80
    if p < 0xc0:
        ln = p - 0xb7
        n = int.from_bytes(b[i + 1:i + 1 + ln], "big")
        return b[i + 1 + ln:i + 1 + ln + n], i + 1 + ln + n
    if p < 0xf8:
        s, n = i + 1, p - 0xc0
    else:
        ln = p - 0xf7
        s, n = i + 1 + ln, int.from_bytes(b[i + 1:i + 1 + ln], "big")
    out, j = [], s
    while j < s + n:
        x, j = rlp_decode(b, j)
        out.append(x)
    return out, s + n


def decode_tx(raw: bytes) -> dict:
    """nonce, gas, maxFee (gas price for legacy), value, chainId, to, type and hash of a signed transaction."""
    u = lambda x: int.from_bytes(x, "big")  # noqa: E731
    if raw[0] >= 0xc0:
        f, _ = rlp_decode(raw)
        v = u(f[6])
        return {"type": 0, "nonce": u(f[0]), "maxFee": u(f[1]), "gas": u(f[2]), "to": "0x" + f[3].hex(),
                "value": u(f[4]), "chainId": (v - 35) // 2 if v >= 35 else None, "hash": "0x" + keccak256(raw).hex()}
    t = raw[0]
    f, _ = rlp_decode(raw, 1)
    if t == 3 and isinstance(f[0], list):
        raise Permanent("pubnet: permanent: blob transactions in network form are not handled")
    if t == 1:
        cid, nonce, fee, gas, to, value = f[0], f[1], f[2], f[3], f[4], f[5]
    elif t in (2, 3, 4):
        cid, nonce, fee, gas, to, value = f[0], f[1], f[3], f[4], f[5], f[6]
    else:
        raise Permanent(f"pubnet: permanent: transaction type {t} is not handled")
    return {"type": t, "nonce": u(nonce), "maxFee": u(fee), "gas": u(gas), "to": "0x" + to.hex(), "value": u(value),
            "chainId": u(cid), "hash": "0x" + keccak256(raw).hex()}


# ---- the journal ----------------------------------------------------------------------------------------------------

FINAL = ("mined", "replaced", "rejected")


class Journal:
    """Append-only JSONL, one line per event, fsynced: `signed` (raw bytes, hash, nonce, stage) BEFORE the first
    broadcast, `broadcast` (which endpoints took it), `fate` (mined with status and block, replaced, rejected)."""

    def __init__(self, path: str):
        self.path = path
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        self.lock = threading.Lock()

    def append(self, rec: dict) -> None:
        rec = {"at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), **rec}
        line = (json.dumps(rec, sort_keys=True) + "\n").encode()
        with self.lock:
            fd = os.open(self.path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)
            try:
                os.write(fd, line)  # one write: a line is never interleaved with another writer's
                os.fsync(fd)
            finally:
                os.close(fd)

    def txs(self) -> dict:
        """hash -> folded record (the signed line's fields, the last fate)."""
        out: dict[str, dict] = {}
        if not os.path.isfile(self.path):
            return out
        with open(self.path) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    r = json.loads(line)
                except json.JSONDecodeError:
                    continue  # a torn last line (a crash while writing): the signed line before it is whole
                h = r.get("hash")
                if not h:
                    continue
                cur = out.setdefault(h, {"hash": h, "state": "signed"})
                if r.get("event") == "signed":
                    cur.update({k: r[k] for k in ("raw", "nonce", "from", "stage", "at", "unlocked") if k in r})
                elif r.get("event") == "broadcast":
                    cur["state"] = cur["state"] if cur["state"] in FINAL else "broadcast"
                    cur["accepted_by"] = r.get("accepted_by", [])
                elif r.get("event") == "fate":
                    cur.update(state=r["fate"], **{k: r[k] for k in ("status", "block", "gasUsed", "to",
                                                                         "contractAddress", "why") if k in r})
        return out

    def open_txs(self) -> list[dict]:
        return [t for t in self.txs().values() if t["state"] not in FINAL]

    def floor(self) -> int:
        return max([t.get("block", 0) for t in self.txs().values() if t["state"] == "mined"] or [0])


def resolve_one(net: Net, j: Journal, t: dict, sender: str, log=print) -> str:
    """One look at a journaled transaction's fate; records it once known. Returns its state."""
    h = t["hash"]
    for url, rc in net.each("eth_getTransactionReceipt", [h]):
        if rc:
            j.append({"event": "fate", "hash": h, "fate": "mined", "status": rc.get("status"),
                      "block": int(rc["blockNumber"], 16), "gasUsed": rc.get("gasUsed"), "to": rc.get("to"),
                      "contractAddress": rc.get("contractAddress")})
            log(f"{h}: mined in block {int(rc['blockNumber'], 16)}, status {rc.get('status')} ({label(url)})")
            return "mined"
    nonces = [int(r, 16) for _, r in net.each("eth_getTransactionCount", [sender, "latest"])]
    if t.get("nonce") is not None and nonces and max(nonces) > t["nonce"]:
        # The nonce is used. Ours, on a node that has no receipt yet, or another transaction with the same nonce:
        # only after several looks over a minute with no receipt anywhere is it taken as replaced.
        seen = t.get("_nonce_used_looks", 0)
        if seen < 6:
            t["_nonce_used_looks"] = seen + 1
            return "nonce-used"
        j.append({"event": "fate", "hash": h, "fate": "replaced",
                  "why": f"the sender's nonce is {max(nonces)} > {t['nonce']} and no endpoint has a receipt"})
        log(f"{h}: replaced (nonce {t['nonce']} used by another transaction)")
        return "replaced"
    known = any(r for _, r in net.each("eth_getTransactionByHash", [h]))
    if t.get("raw"):  # the same bytes again: at most one transaction can come of them
        for url in net.urls:
            try:
                _rpc_once(url, "eth_sendRawTransaction", [t["raw"]], net.timeout)
            except (Transient, Permanent):
                pass
    return "pending" if known else "unknown"


def resolve_all(net: Net, j: Journal, sender: str, budget: float, log=print) -> int:
    """0: every transaction has a fate, all mined with status 1; 3: one reverted / replaced / rejected; 75: unknown."""
    deadline = time.monotonic() + budget
    cache: dict[str, dict] = {}
    attempt = 0
    while True:
        open_ = j.open_txs()
        for t in open_:
            t.update(cache.get(t["hash"], {}))
            resolve_one(net, j, t, sender, log)
            cache[t["hash"]] = {k: v for k, v in t.items() if k.startswith("_")}
        if not j.open_txs():
            break
        attempt += 1
        pause = min(15.0, 3.0 * attempt) * random.uniform(0.75, 1.25)
        if time.monotonic() + pause > deadline:
            log(f"pubnet: transient: {len(j.open_txs())} transaction(s) without a known fate after {budget:.0f} s: "
                + ", ".join(t["hash"] for t in j.open_txs()))
            return TEMPFAIL
        time.sleep(pause)
    bad = [t for t in j.txs().values() if t["state"] != "mined" or t.get("status") not in ("0x1", 1)]
    for t in bad:
        log(f"pubnet: permanent: {t['hash']} (stage {t.get('stage')}): {t['state']}, status {t.get('status')}"
            f" {t.get('why', '')}".rstrip())
    return 3 if bad else 0


# ---- the proxy ------------------------------------------------------------------------------------------------------

STATE_READS = {"eth_call": 1, "eth_estimateGas": 1, "eth_getBalance": 1, "eth_getCode": 1, "eth_getStorageAt": 2,
               "eth_getProof": 2, "eth_createAccessList": 1, "eth_getTransactionCount": 1}
ANYWHERE = ("eth_getTransactionReceipt", "eth_getTransactionByHash")
LOCAL_ONLY = ("anvil_", "evm_", "hardhat_", "debug_setHead", "personal_")


class Proxy:
    def __init__(self, a):
        self.net = Net(a.network, first=a.first, log=self.log)
        self.all_loopback = all(loopback(u) for u in self.net.urls)
        self.lag = a.lag if a.lag is not None else (0 if self.all_loopback else 3)
        # a local chain (lag 0): every "latest" is the head as it is now (an evm_mine or a warp shows at once)
        self.pin_ttl = float(os.environ.get("PUBNET_PIN_TTL_S", "12" if self.lag else "0"))
        self.sender = (a.sender or "").lower() or None
        self.allow_unlocked = a.allow_unlocked
        self.chain = None
        self.journal = Journal(a.journal)
        self.events = os.path.join(os.path.dirname(os.path.abspath(a.journal)), "events.jsonl")
        self.floor = self.journal.floor()
        self._pin, self._pin_t = None, 0.0
        self.check_chains(chain_id(a.network) if not self.all_loopback else None)
        self.pin_lock, self.send_lock = threading.Lock(), threading.Lock()

    def log(self, msg: str) -> None:
        print(f"[{time.strftime('%H:%M:%S')}] {msg}", file=sys.stderr, flush=True)

    def check_chains(self, want: int | None) -> None:
        """Every endpoint on the same chain (the network's, or the first answer's); one on another chain is dropped."""
        keep = []
        for url in self.net.urls:
            try:
                cid = int(_rpc_once(url, "eth_chainId", [], self.net.timeout), 16)
            except (Transient, Permanent) as e:
                self.log(f"chain id of {label(url)} unknown now ({str(e)[:100]}); kept")
                keep.append(url)
                continue
            if want is None:
                want = cid
            if cid == want:
                keep.append(url)
            else:
                self.log(f"{label(url)} is on chain {cid}, not {want}: dropped")
        if not keep:
            raise SystemExit("pubnet: no endpoint on the expected chain")
        self.net.urls = keep

    def event(self, **kw) -> None:
        with open(self.events, "a") as f:
            f.write(json.dumps({"t": time.time(), **kw}) + "\n")

    # -- the pinned block --
    def pin(self) -> int:
        with self.pin_lock:
            if self._pin is not None and time.monotonic() - self._pin_t < self.pin_ttl:
                return self._pin
            head = self.net.head()
            self.floor = max(self.floor, self.journal.floor())  # fates `resolve` recorded since
            p = max(head - self.lag, self.floor, self._pin or 0)
            if self.sender and p < head:
                # The sender's nonce at p must be its nonce at the head: else one of its transactions came after p
                # (sent by hand, or before this proxy started) and a client would plan on a state without it.
                at_p = int(self.net.call("eth_getTransactionCount", [self.sender, hex(p)], must=True), 16)
                at_h = max([int(r, 16) for _, r in self.net.each("eth_getTransactionCount", [self.sender, "latest"], 2)]
                           or [at_p])
                if at_h > at_p:
                    self.log(f"pin: the sender's nonce is {at_h} at the head, {at_p} at {p}: pin = head {head}")
                    p = head
            if p != self._pin:
                self.event(kind="pin", block=p, head=head)
            self._pin, self._pin_t = p, time.monotonic()
            return p

    def bump(self, block: int) -> None:
        with self.pin_lock:
            if block > self.floor:
                self.floor = block
            if self._pin is None or block > self._pin:
                self._pin, self._pin_t = block, time.monotonic()

    # -- requests --
    def handle(self, req: dict, stage: str) -> dict:
        rid, method, params = req.get("id"), req.get("method", ""), list(req.get("params") or [])
        try:
            return {"jsonrpc": "2.0", "id": rid, "result": self.dispatch(method, params, stage)}
        except Transient as e:
            self.event(kind="error", cls="transient", method=method, stage=stage, msg=str(e)[:300])
            return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32000, "message": f"pubnet: transient: {e}"}}
        except Permanent as e:
            err = getattr(e, "rpc_error", None)
            if isinstance(err, dict) and ("revert" in str(err.get("message", "")).lower() or err.get("code") == 3):
                return {"jsonrpc": "2.0", "id": rid, "error": err}  # a revert as the node said it (forge decodes it)
            self.event(kind="error", cls="permanent", method=method, stage=stage, msg=str(e)[:300])
            return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32000, "message": f"pubnet: permanent: {e}"}}

    def dispatch(self, method: str, params: list, stage: str):
        if method == "eth_chainId":
            if self.chain is None:
                self.chain = self.net.call("eth_chainId", [])
            return self.chain
        if method == "eth_blockNumber":
            return hex(self.pin())
        if method == "eth_getBlockByNumber" and params:
            tag = params[0]
            if tag in ("latest", "pending"):
                return self.net.call(method, [hex(self.pin())] + params[1:], must=True)
            if isinstance(tag, str) and tag.startswith("0x") and int(tag, 16) <= (self._pin or 0):
                return self.net.call(method, params, must=True)
            return self.net.call(method, params)
        if method == "eth_getTransactionCount" and params and self.sender and params[0].lower() == self.sender \
                and (len(params) < 2 or params[1] == "pending"):
            return hex(self.agreed_nonce()[0])
        if method in STATE_READS:
            k = STATE_READS[method]
            p = params + [None] * (k + 1 - len(params)) if len(params) <= k else list(params)
            if p[k] in (None, "latest", "pending"):
                p[k] = hex(self.pin())
            elif isinstance(p[k], dict) and p[k].get("blockNumber") in ("latest", "pending"):
                p[k] = {"blockNumber": hex(self.pin())}
            return self.net.call(method, p)
        if method == "eth_getLogs" and params and isinstance(params[0], dict):
            f = dict(params[0])
            for key in ("fromBlock", "toBlock"):
                if key in f and f[key] in ("latest", "pending") or (key == "toBlock" and "toBlock" not in f
                                                                    and "blockHash" not in f):
                    f[key] = hex(self.pin())
            return self.net.call(method, [f])
        if method in ANYWHERE:
            for url, r in self.net.each(method, params):
                if r:
                    if method == "eth_getTransactionReceipt" and self.sender \
                            and str(r.get("from", "")).lower() == self.sender:
                        self.bump(int(r["blockNumber"], 16))
                    return r
            return None
        if method == "eth_sendRawTransaction":
            return self.send_raw(params[0], stage)
        if method == "eth_sendTransaction":
            return self.send_unlocked(params[0], stage)
        if method.startswith(LOCAL_ONLY):
            if not self.all_loopback:
                raise Permanent(f"pubnet: permanent: {method} goes to a local chain only")
            r = self.net.call(method, params, urls=self.net.urls[:1])
            self._pin_t = 0.0  # the local chain moved (evm_mine, evm_increaseTime, ...): pin again
            return r
        return self.net.call(method, params)

    def agreed_nonce(self) -> tuple[int, int]:
        """(nonce, how many endpoints): the latest and the pending nonce equal on two endpoints (one if only one is
        configured); a disagreement is transient (a lagging node, or a transaction in flight)."""
        need = 1 if len(self.net.urls) == 1 else 2
        deadline = time.monotonic() + self.net.budget
        attempt, seen = 0, []
        while True:
            seen = []
            for url in self.net.order():
                try:
                    lat = int(_rpc_once(url, "eth_getTransactionCount", [self.sender, "latest"], self.net.timeout), 16)
                    pen = int(_rpc_once(url, "eth_getTransactionCount", [self.sender, "pending"], self.net.timeout), 16)
                except (Transient, Permanent) as e:
                    self.log(f"nonce: {e}")
                    continue
                seen.append((label(url), lat, pen))
                top = max(max(x[1], x[2]) for x in seen)
                if len([1 for _, la, pe in seen if la == pe == top]) >= need:
                    break
            # A lagging node shows an older nonce, never a newer one: the highest value is the chain's; it must be
            # both the latest and the pending nonce on `need` endpoints (pending above latest: a transaction in flight).
            top = max([max(lat, pen) for _, lat, pen in seen] or [-1])
            agree = [ep for ep, lat, pen in seen if lat == pen == top]
            if len(agree) >= need:
                return top, len(agree)
            attempt += 1
            pause = backoff(attempt, base=2.0, cap=8.0)
            if time.monotonic() + pause > deadline:
                raise Transient(f"pubnet: transient: the sender's latest and pending nonces do not agree on {need}"
                                f" endpoints: {seen}")
            time.sleep(pause)

    def send_raw(self, raw_hex: str, stage: str) -> str:
        raw = bytes.fromhex(raw_hex[2:] if raw_hex.startswith("0x") else raw_hex)
        tx = decode_tx(raw)
        h = tx["hash"]
        with self.send_lock:
            txs = self.journal.txs()
            if h in txs:  # the same bytes again (a client retry): at most one transaction can come of them
                if txs[h]["state"] not in FINAL:
                    self.broadcast(raw_hex, h)
                return h
            open_ = [t for t in txs.values() if t["state"] not in FINAL]
            if open_:
                for t in open_:
                    resolve_one(self.net, self.journal, t, self.sender or t.get("from", ""), self.log)
                open_ = self.journal.open_txs()
                if open_:
                    raise Transient("pubnet: transient: an earlier transaction's fate is unknown: "
                                    + ", ".join(t["hash"] for t in open_))
            if not self.sender:
                raise Permanent("pubnet: permanent: the proxy was started without --sender; it sends nothing")
            chain = int(self.dispatch("eth_chainId", [], stage), 16)
            if tx["chainId"] is not None and tx["chainId"] != chain:
                raise Permanent(f"pubnet: permanent: the transaction is for chain {tx['chainId']}, not {chain}")
            nonce, n_ep = self.agreed_nonce()
            if tx["nonce"] < nonce:
                raise Transient(f"pubnet: transient: stale plan: the transaction's nonce {tx['nonce']} is used (the"
                                f" sender's nonce is {nonce}); planned on a block before the sender's last transaction")
            if tx["nonce"] > nonce:
                raise Permanent(f"pubnet: permanent: nonce gap: the transaction's nonce {tx['nonce']} > {nonce}")
            bal = int(self.net.call("eth_getBalance", [self.sender, "latest"]), 16)
            need = tx["value"] + tx["gas"] * tx["maxFee"]
            if bal < need:
                raise Permanent(f"pubnet: permanent: insufficient funds: balance {bal} < value + gas x max fee {need}")
            self.journal.append({"event": "signed", "hash": h, "raw": raw_hex, "nonce": tx["nonce"],
                                 "from": self.sender, "stage": stage, "to": tx["to"], "gas": tx["gas"],
                                 "nonce_agreed_by": n_ep})
            self.event(kind="send", hash=h, nonce=tx["nonce"], stage=stage)
            return self.broadcast(raw_hex, h)

    def broadcast(self, raw_hex: str, h: str) -> str:
        accepted, rejected, unknown = [], [], []
        for url in self.net.urls:
            try:
                _rpc_once(url, "eth_sendRawTransaction", [raw_hex], self.net.timeout)
                accepted.append(label(url))
            except Transient as e:
                unknown.append(str(e)[:160])
            except Permanent as e:
                msg = str(e)
                if re.search(r"already known|known transaction|already imported|alreadyknown", msg, re.I):
                    accepted.append(label(url))
                elif re.search(r"nonce too low", msg, re.I):
                    unknown.append(msg[:160])  # perhaps these very bytes, mined through another endpoint
                else:
                    rejected.append(msg[:160])
        self.journal.append({"event": "broadcast", "hash": h, "accepted_by": accepted, "rejected": rejected,
                             "unknown": unknown})
        self.log(f"broadcast {h}: accepted by {accepted}, rejected {rejected}, unknown {unknown}")
        if not accepted and not unknown:
            # Every endpoint refused it outright: it is in no mempool. Its fate is known once the chain agrees.
            st = resolve_one(self.net, self.journal, {"hash": h, "nonce": decode_tx(bytes.fromhex(raw_hex[2:]))["nonce"]},
                             self.sender, self.log)
            if st not in FINAL:
                self.journal.append({"event": "fate", "hash": h, "fate": "rejected", "why": "; ".join(rejected)[:300]})
            raise Permanent(f"pubnet: permanent: every endpoint refused the transaction: {'; '.join(rejected)[:300]}")
        return h

    def send_unlocked(self, tx: dict, stage: str) -> str:
        """eth_sendTransaction (a local fork that signs for an impersonated sender): one endpoint, journaled."""
        if not (self.allow_unlocked and self.all_loopback):
            raise Permanent("pubnet: permanent: eth_sendTransaction only to a local chain with --allow-unlocked")
        with self.send_lock:
            open_ = self.journal.open_txs()
            for t in open_:
                resolve_one(self.net, self.journal, t, (t.get("from") or self.sender or ""), self.log)
            if self.journal.open_txs():
                raise Transient("pubnet: transient: an earlier transaction's fate is unknown")
            h = _rpc_once(self.net.urls[0], "eth_sendTransaction", [tx], self.net.timeout)
            self.journal.append({"event": "signed", "hash": h, "nonce": int(tx["nonce"], 16) if "nonce" in tx else None,
                                 "from": tx.get("from", "").lower(), "stage": stage, "unlocked": True})
            self.journal.append({"event": "broadcast", "hash": h, "accepted_by": [label(self.net.urls[0])]})
            return h


def serve(a) -> None:
    import fcntl
    proxy = Proxy(a)
    lockf = open(a.journal + ".lock", "w")
    try:
        fcntl.flock(lockf, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        sys.exit(f"pubnet: another proxy holds {a.journal}")

    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def do_POST(self):
            n = int(self.headers.get("content-length", 0))
            stage = urllib.parse.unquote(self.path.strip("/")) or "-"
            try:
                req = json.loads(self.rfile.read(n))
                res = [proxy.handle(r, stage) for r in req] if isinstance(req, list) else proxy.handle(req, stage)
            except Exception as e:  # noqa: BLE001 - a client always gets an answer
                proxy.log(f"internal error: {type(e).__name__}: {e}")
                res = {"jsonrpc": "2.0", "id": None,
                       "error": {"code": -32603, "message": f"pubnet: permanent: internal: {type(e).__name__}: {e}"}}
            body = json.dumps(res).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    srv = ThreadingHTTPServer(("127.0.0.1", a.port), H)
    srv.daemon_threads = True
    port = srv.server_address[1]
    proxy.log(f"pubnet proxy on 127.0.0.1:{port}: {len(proxy.net.urls)} endpoint(s) "
              f"{[label(u) for u in proxy.net.urls]}, lag {proxy.lag}, sender {proxy.sender}, floor {proxy.floor}")
    if a.port_file:
        with open(a.port_file + ".part", "w") as f:
            f.write(f"{port}\n")
        os.replace(a.port_file + ".part", a.port_file)
    srv.serve_forever()


# ---- CLI ------------------------------------------------------------------------------------------------------------


def probe(network: str) -> int:
    """Read-only questions to every listed endpoint: chain id, head, a block 3 behind, a balance there; for beacon
    endpoints the head header and the light-client finality update. One line each."""
    want = chain_id(network)
    ok_all = True
    heads = {}
    for url in endpoints(network, "rpc"):
        try:
            heads[url] = int(_rpc_once(url, "eth_blockNumber", [], 15), 16)
        except (Transient, Permanent):
            pass
    top = max(heads.values() or [0])
    for url in endpoints(network, "rpc"):
        t0 = time.monotonic()
        try:
            cid = int(_rpc_once(url, "eth_chainId", [], 15), 16)
            head = int(_rpc_once(url, "eth_blockNumber", [], 15), 16)
            blk = _rpc_once(url, "eth_getBlockByNumber", [hex(head - 3), False], 15)
            _rpc_once(url, "eth_getBalance", ["0x" + "00" * 19 + "01", hex(head - 3)], 15)
            ok = cid == want and blk is not None and top - head <= 16
            print(f"rpc    {'OK  ' if ok else ('LAG ' if blk else 'BAD ')} {url}  chain {cid} head {head} "
                  f"({top - head} behind the highest) block-3 {'yes' if blk else 'no'} {time.monotonic() - t0:.1f}s")
        except (Transient, Permanent) as e:
            ok = False
            print(f"rpc    FAIL {url}  {str(e)[:150]}")
        ok_all &= ok
    lc = set(endpoints(network, "beacon_lc"))
    for url in endpoints(network, "beacon"):
        t0 = time.monotonic()
        try:
            st, data, _ = http_request(url.rstrip("/") + "/eth/v1/beacon/headers/head", timeout=15)
            slot = json.loads(data)["data"]["header"]["message"]["slot"] if st == 200 else None
            st2, _, _ = http_request(url.rstrip("/") + "/eth/v1/beacon/light_client/finality_update", timeout=15)
            ok = st == 200 and (st2 == 200 or url not in lc)
            print(f"beacon {'OK  ' if ok else 'BAD '} {url}  head slot {slot} finality_update HTTP {st2} "
                  f"{time.monotonic() - t0:.1f}s")
        except (OSError, http.client.HTTPException, ValueError, KeyError) as e:
            ok = False
            print(f"beacon FAIL {url}  {type(e).__name__}: {str(e)[:150]}")
        ok_all &= ok
    return 0 if ok_all else 1


def classify_failure(events: str | None, since: float, log: str | None) -> str:
    """Why an attempt failed: permanent if the proxy gave a client a permanent error since `since` or the log's last
    lines name a revert, invalid input or missing funds; transient if they name only a transient cause (the proxy's
    "pubnet: transient", a lag, a timeout, a connection); otherwise permanent (a human looks)."""
    if events and os.path.isfile(events):
        with open(events) as f:
            for line in f:
                try:
                    e = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if e.get("kind") == "error" and e.get("t", 0) >= since and e.get("cls") == "permanent":
                    return "permanent"
    text = ""
    if log and os.path.isfile(log):
        with open(log, errors="replace") as f:
            text = "\n".join(f.read().splitlines()[-60:])
    return classify(text)


def selftest() -> int:
    assert keccak256(b"").hex() == "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    assert keccak256(b"abc").hex() == "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
    assert keccak256(b"a" * 135).hex() == keccak256(bytes(b"a" * 135)).hex()
    assert keccak256(b"a" * 136).hex() != keccak256(b"a" * 135).hex()
    assert rlp_decode(bytes.fromhex("c88363617483646f67"))[0] == [b"cat", b"dog"]
    for text, want in (("header not found", "transient"), ("HTTP 429", "transient"), ("-32001 block not found",
                       "transient"), ("execution reverted: x", "permanent"), ("insufficient funds for gas", "permanent"),
                       ("timeout; execution reverted", "permanent"), ("something odd", "permanent"),
                       ("missing trie node abc", "transient"), ("HTTP 503", "transient")):
        assert classify(text) == want, (text, classify(text))
    print("pubnet selftest: keccak, RLP, classification OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("endpoints")
    e.add_argument("network")
    e.add_argument("kind", nargs="?", default="rpc")
    e.add_argument("--first")
    sub.add_parser("probe").add_argument("network")
    p = sub.add_parser("proxy")
    p.add_argument("--network", default="sepolia")
    p.add_argument("--journal", required=True)
    p.add_argument("--sender")
    p.add_argument("--first")
    p.add_argument("--port", type=int, default=0)
    p.add_argument("--port-file")
    p.add_argument("--lag", type=int)
    p.add_argument("--allow-unlocked", action="store_true")
    r = sub.add_parser("resolve")
    r.add_argument("--network", default="sepolia")
    r.add_argument("--journal", required=True)
    r.add_argument("--sender", required=True)
    r.add_argument("--first")
    r.add_argument("--budget", type=float, default=float(os.environ.get("PUBNET_RESOLVE_BUDGET_S", "600")))
    w = sub.add_parser("rows")
    w.add_argument("--journal", required=True)
    w.add_argument("--stage", required=True)
    c = sub.add_parser("classify")
    c.add_argument("--events")
    c.add_argument("--since", type=float, default=0.0)
    c.add_argument("log", nargs="?")
    sub.add_parser("selftest")
    a = ap.parse_args()
    if a.cmd == "endpoints":
        print("\n".join(endpoints(a.network, a.kind, a.first)))
    elif a.cmd == "probe":
        return probe(a.network)
    elif a.cmd == "proxy":
        serve(a)
    elif a.cmd == "resolve":
        return resolve_all(Net(a.network, first=a.first), Journal(a.journal), a.sender.lower(), a.budget,
                           log=lambda m: print(f"pubnet resolve: {m}", file=sys.stderr))
    elif a.cmd == "rows":
        rows = [{"hash": t["hash"], "blockNumber": hex(t["block"]), "status": t.get("status"),
                 "gasUsed": t.get("gasUsed"), "to": t.get("to"), "contractAddress": t.get("contractAddress")}
                for t in Journal(a.journal).txs().values() if t.get("stage") == a.stage and t["state"] == "mined"]
        print(json.dumps(rows))
    elif a.cmd == "classify":
        print(classify_failure(a.events, a.since, a.log))
    elif a.cmd == "selftest":
        return selftest()
    return 0


if __name__ == "__main__":
    sys.exit(main())
