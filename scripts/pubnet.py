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
  3. reads at "latest" answered at a pin: the agreed head (the highest block two endpoints report) minus `lag`,
     never below the fence (the block of the sender's last mined transaction in the journal), recomputed at most
     every PUBNET_PIN_TTL_S; every read a plan or check decides on (calls, storage, code, balances, nonces, proofs,
     gas estimates, blocks) given alike by two endpoints at the same explicit block, or refused (transient); a
     receipt counts only when two endpoints give it alike. One stale, lagging or lying endpoint can delay but not
     steer (e2e/netfault/attacks);
  4. before a raw transaction leaves: signed by the sender; its nonce the highest any endpoint reports, on two
     endpoints as both latest and pending; the balance (two endpoints alike) enough for value + gas x max fee; no
     journaled transaction without a known fate or with a bad one not acknowledged; no journaled mined transaction
     with the same to, value and calldata (the same step again); the signed bytes and their hash written (fsync) to
     the journal BEFORE the first broadcast; then broadcast to every endpoint at once, and again while a receipt is
     awaited;
  5. a transaction whose fate is unknown blocks every later send until it is found mined, replaced or rejected
     (`resolve`), so a stopped and resumed stage never sends twice; a bad fate blocks them until `ack`.

CLI:
  pubnet.py endpoints <network> [rpc|beacon]          the list, in order
  pubnet.py probe <network>                           each endpoint asked read-only questions; a table
  pubnet.py proxy --network N --journal F [--sender A] [--first URL] [--port-file P] [--lag K] [--allow-unlocked]
  pubnet.py resolve --network N --journal F --sender A [--first URL] [--budget S]
                                                      exit 0: every journaled transaction has a fate and all were
                                                      mined with status 1; 3: one reverted, was replaced or rejected
                                                      (a human looks); 75: a fate is still unknown (try again later)
  pubnet.py ack --journal F --hash H [--why TEXT]     an operator has looked at a transaction with a bad or unknown
                                                      fate (reverted, replaced, rejected, lost): sends may go on
  pubnet.py gone --journal F --hash H --sender A      a journaled transaction checked absent from the chain (no
                                                      endpoint has its receipt, the sender's nonce not above it) is
                                                      marked gone: it leaves the fence, the nonce floor and the done
                                                      set; refused if any endpoint still has its receipt
  pubnet.py rows --journal F --stage S                the stage's mined transactions as manifest rows (JSON), those
                                                      an operator acknowledged left out
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
    r"name or service not known|temporary failure in name resolution|pubnet: transient|"
    r"tx not found|transaction not found|receipt not found|dropped from the mempool|"  # hashes we sent: lag
    r"blockoutofrange|block height is \d+ but requested", re.I)  # anvil asked for a block it does not have yet
PERMANENT_RE = re.compile(
    r"execution reverted|revert|invalid (argument|params|opcode|sender|signature|transaction|chain)|"
    r"insufficient funds|nonce too (low|high)|intrinsic gas|gas required exceeds|exceeds block gas limit|"
    r"replacement transaction underpriced|max fee per gas less than|fee cap|pubnet: permanent", re.I)
# A plan refused because the chain's time has not come yet (Crown.s.sol: "the candidate's wait is not over", "the
# release delay is not over"): at a block pinned a few behind the head this can be true for a few slots more; forge
# sent nothing, so it is tried again (transient), within the stage's budget.
NOT_YET_RE = re.compile(r"wait is not over|delay is not over", re.I)
UNSUPPORTED_RE = re.compile(r"method not found|not supported|unsupported|-32601\b|not whitelisted|not allowed|"
                            r"disabled|requires? (an )?api key|unauthori[sz]ed|forbidden|\b40[13]\b", re.I)


class Transient(Exception):
    pass


class Permanent(Exception):
    pass


def classify(text: str) -> str:
    """transient | permanent. A message that names a revert, invalid input or missing funds is permanent even when
    it also names a timeout; an unknown message is permanent (a human looks rather than a loop)."""
    if NOT_YET_RE.search(text or ""):
        return "transient"
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
        self.cool: dict[str, float] = {}  # endpoint -> monotonic time before which it goes last
        self.fails: dict[str, int] = {}  # endpoint -> failures in a row
        self.log = log or (lambda msg: None)

    def failed(self, url: str) -> None:
        """An endpoint that keeps failing waits longer each time (5 s, 10 s, ... 300 s) before it is tried first again."""
        n = self.fails[url] = self.fails.get(url, 0) + 1
        self.cool[url] = time.monotonic() + min(300.0, 5.0 * 2 ** (n - 1))

    def served(self, url: str) -> None:
        self.fails.pop(url, None)
        self.cool.pop(url, None)

    def order(self) -> list[str]:
        now = time.monotonic()
        ready = [u for u in self.urls if self.cool.get(u, 0) <= now]
        return ready + [u for u in self.urls if u not in ready]

    def ready(self) -> list[str]:
        """The endpoints not cooling, or all of them if every one is."""
        now = time.monotonic()
        return [u for u in self.urls if self.cool.get(u, 0) <= now] or list(self.urls)

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
                    self.served(url)
                    return r
                except Transient as e:
                    last = str(e)
                    self.failed(url)
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
        for url in self.ready():
            try:
                out.append((url, _rpc_once(url, method, params, self.timeout)))
                self.served(url)
            except (Transient, Permanent) as e:
                self.failed(url)
                self.log(f"each {method}: {e}")
            if n and len(out) >= n:
                break
        return out

    def anywhere(self, method: str, params: list):
        """The first non-null answer of any endpoint, every one asked in turn, cooling ones last (receipts,
        transactions by hash, blocks by number or hash): a null from one node (behind, or with its history pruned) is
        not the chain's answer. None if no endpoint has it; Transient if none answered at all."""
        answered = False
        for url in self.order():
            try:
                r = _rpc_once(url, method, params, self.timeout)
                self.served(url)
            except (Transient, Permanent) as e:
                self.failed(url)
                self.log(f"anywhere {method}: {e}")
                continue
            answered = True
            if r:
                return r
        if not answered:
            return self.call(method, params)
        return None

    def need(self) -> int:
        """How many endpoints must give the same answer: 2, or 1 when only one is configured (a local chain)."""
        return 1 if len(self.urls) == 1 else 2

    def quorum(self, method: str, params: list, *, key=None, near=None, null_is_missing: bool = False,
               budget: float | None = None):
        """The answer `need` endpoints give alike to the same request (with its explicit block): one stale, lagging or
        lying endpoint is outvoted, or, when no two agree, the request fails (transient) and nothing is decided on it.
        A node's own refusal (a revert) is an answer too: two alike are raised as it. key(result): what is compared
        (default the whole result); near(a, b): for numbers that may differ a little between clients (gas estimates),
        two near ones agree and the larger is returned."""
        need = self.need()
        deadline = time.monotonic() + (budget or self.budget)
        attempt = 0
        while True:
            oks: list[tuple[str, object]] = []
            errs: dict[str, list] = {}
            seen = []
            for url in self.order():
                try:
                    r = _rpc_once(url, method, params, self.timeout)
                    self.served(url)
                except Transient as e:
                    self.failed(url)
                    seen.append(f"{label(url)}: {str(e)[len(label(url)) + 2:][:60]}")
                    continue
                except Permanent as e:
                    err = getattr(e, "rpc_error", None)
                    if not isinstance(err, dict):  # this endpoint refused the request itself (HTTP 4xx): not an answer
                        self.failed(url)
                        seen.append(f"{label(url)}: {str(e)[len(label(url)) + 2:][:60]}")
                        continue
                    k = str(err.get("data") or err.get("message") or "")[:200].lower()
                    errs.setdefault(k, []).append(e)
                    seen.append(f"{label(url)}: refused {k[:40]}")
                    if len(errs[k]) >= need:
                        raise errs[k][0]
                    continue
                if r is None and null_is_missing:
                    seen.append(f"{label(url)}: null")
                    continue
                v = key(r) if key else r
                seen.append(f"{label(url)}: {json.dumps(v)[:40]}")
                if near is not None:
                    match = [(u, o) for u, o in oks if near(o, r)]
                    if len(match) + 1 >= need:
                        return max([r] + [o for _, o in match], key=lambda x: int(x, 16))
                elif len([1 for _, o in oks if (key(o) if key else o) == v]) + 1 >= need:
                    return r
                oks.append((url, r))
            attempt += 1
            pause = backoff(attempt, base=1.0, cap=8.0)
            if time.monotonic() + pause > deadline:
                raise Transient(f"pubnet: transient: {method}: no {need} endpoints give the same answer"
                                f" ({'; '.join(seen[-6:])})")
            time.sleep(pause)

    def head(self) -> int:
        """The agreed head: the highest block at least `need` endpoints report having (the need-th highest of their
        heads). One endpoint's claim alone never moves it; a lagging one reports less."""
        deadline = time.monotonic() + self.budget
        attempt = 0
        while True:
            heads = sorted((int(r, 16) for _, r in self.each("eth_blockNumber", [])), reverse=True)
            if len(heads) >= self.need():
                return heads[self.need() - 1]
            attempt += 1
            pause = backoff(attempt, base=1.0, cap=8.0)
            if time.monotonic() + pause > deadline:
                raise Transient(f"pubnet: transient: eth_blockNumber: fewer than {self.need()} endpoints answer")
            time.sleep(pause)

    def receipt(self, h: str):
        """The receipt of h as `need` endpoints give it (the same block hash, number and status); None while no two
        agree (not mined, lagging nodes, or one endpoint inventing it). Transient if no endpoint answered."""
        found: dict[tuple, list] = {}
        answered = False
        for url in self.order():
            try:
                r = _rpc_once(url, "eth_getTransactionReceipt", [h], self.timeout)
                self.served(url)
            except (Transient, Permanent) as e:
                self.failed(url)
                self.log(f"receipt {h[:12]}: {e}")
                continue
            answered = True
            if not r:
                continue
            k = (r.get("blockHash"), r.get("blockNumber"), r.get("status"))
            found.setdefault(k, []).append(r)
            if len(found[k]) >= self.need():
                return found[k][0]
        if not answered:
            raise Transient(f"pubnet: transient: eth_getTransactionReceipt: no endpoint answered")
        return None

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


def transient_exception(e: BaseException) -> bool:
    """An exception from a request (urllib, http.client, this module, a script's own RuntimeError with the node's
    message) that a later attempt may not meet: HTTP 429 or 5xx, a timeout, a dropped connection, a lag message."""
    import urllib.error
    if isinstance(e, urllib.error.HTTPError):
        return e.code == 429 or e.code >= 500
    if isinstance(e, Transient):
        return True
    if isinstance(e, Permanent):
        return False
    if isinstance(e, (TimeoutError, ConnectionError, socket.timeout, urllib.error.URLError, http.client.HTTPException)):
        return True
    return classify(f"{type(e).__name__}: {e}") == "transient"


def retrying(fn, budget: float = 120.0, what: str = "request"):
    """fn() again after a transient failure (same endpoint), backoff with jitter, within the budget: for scripts whose
    provenance names one endpoint per answer (the fixture fetchers' cross-checks must not fail over silently). Any
    other failure, or the budget spent, raises what fn raised."""
    deadline = time.monotonic() + budget
    attempt = 0
    while True:
        try:
            return fn()
        except Exception as e:  # noqa: BLE001 - classified below
            if not transient_exception(e):
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


def rlp_encode(x) -> bytes:
    def length(n: int, off: int) -> bytes:
        if n < 56:
            return bytes([off + n])
        b = n.to_bytes((n.bit_length() + 7) // 8, "big")
        return bytes([off + 55 + len(b)]) + b
    if isinstance(x, (bytes, bytearray)):
        return bytes(x) if len(x) == 1 and x[0] < 0x80 else length(len(x), 0x80) + bytes(x)
    out = b"".join(rlp_encode(i) for i in x)
    return length(len(out), 0xc0) + out


# secp256k1, for the signer of a raw transaction (affine arithmetic: a few milliseconds per recovery)
_P = 2 ** 256 - 2 ** 32 - 977
_N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
_G = (0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
      0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8)


def _ec_add(a, b):
    if a is None:
        return b
    if b is None:
        return a
    if a[0] == b[0] and (a[1] + b[1]) % _P == 0:
        return None
    if a == b:
        m = 3 * a[0] * a[0] * pow(2 * a[1], -1, _P) % _P
    else:
        m = (b[1] - a[1]) * pow(b[0] - a[0], -1, _P) % _P
    x = (m * m - a[0] - b[0]) % _P
    return x, (m * (a[0] - x) - a[1]) % _P


def _ec_mul(k: int, pt):
    out = None
    while k:
        if k & 1:
            out = _ec_add(out, pt)
        pt = _ec_add(pt, pt)
        k >>= 1
    return out


def recover_address(z: int, r: int, s: int, parity: int) -> str:
    """The address whose key signed the hash z with (r, s, y parity)."""
    if not (0 < r < _N and 0 < s < _N):
        raise Permanent("pubnet: permanent: an invalid signature")
    y = pow((r * r * r + 7) % _P, (_P + 1) // 4, _P)
    if y % 2 != parity:
        y = _P - y
    rinv = pow(r, -1, _N)
    q = _ec_add(_ec_mul(s * rinv % _N, (r, y)), _ec_mul((-z * rinv) % _N, _G))
    if q is None:
        raise Permanent("pubnet: permanent: an invalid signature")
    return "0x" + keccak256(q[0].to_bytes(32, "big") + q[1].to_bytes(32, "big"))[12:].hex()


def decode_tx(raw: bytes) -> dict:
    """nonce, gas, maxFee (gas price for legacy), value, chainId, to, a hash of the data, type, hash and signer
    (recovered from the signature) of a signed transaction."""
    u = lambda x: int.from_bytes(x, "big")  # noqa: E731
    if raw[0] >= 0xc0:
        f, _ = rlp_decode(raw)
        v = u(f[6])
        if v >= 35:
            cid, parity = (v - 35) // 2, (v - 35) % 2
            z = keccak256(rlp_encode(f[:6] + [cid.to_bytes((cid.bit_length() + 7) // 8, "big"), b"", b""]))
        else:
            cid, parity = None, v - 27
            z = keccak256(rlp_encode(f[:6]))
        return {"type": 0, "nonce": u(f[0]), "maxFee": u(f[1]), "gas": u(f[2]), "to": "0x" + f[3].hex(),
                "value": u(f[4]), "datahash": "0x" + keccak256(f[5]).hex(), "chainId": cid,
                "hash": "0x" + keccak256(raw).hex(), "from": recover_address(u(z), u(f[7]), u(f[8]), parity)}
    t = raw[0]
    f, _ = rlp_decode(raw, 1)
    if t == 3 and isinstance(f[0], list):
        raise Permanent("pubnet: permanent: blob transactions in network form are not handled")
    if t == 1:
        cid, nonce, fee, gas, to, value, data = f[0], f[1], f[2], f[3], f[4], f[5], f[6]
    elif t in (2, 3, 4):
        cid, nonce, fee, gas, to, value, data = f[0], f[1], f[3], f[4], f[5], f[6], f[7]
    else:
        raise Permanent(f"pubnet: permanent: transaction type {t} is not handled")
    z = keccak256(bytes([t]) + rlp_encode(f[:-3]))
    return {"type": t, "nonce": u(nonce), "maxFee": u(fee), "gas": u(gas), "to": "0x" + to.hex(), "value": u(value),
            "datahash": "0x" + keccak256(data).hex(), "chainId": u(cid), "hash": "0x" + keccak256(raw).hex(),
            "from": recover_address(u(z), u(f[-2]), u(f[-1]), u(f[-3]))}


# ---- the journal ----------------------------------------------------------------------------------------------------

FINAL = ("mined", "replaced", "rejected", "acked", "gone")


class Journal:
    """Append-only JSONL, one line per event, fsynced: `signed` (raw bytes, hash, nonce, stage) BEFORE the first
    broadcast, `broadcast` (which endpoints took it), `fate` (mined with status and block, replaced, rejected)."""

    def __init__(self, path: str):
        self.path = path
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        self.lock = threading.Lock()
        self._cache = (None, {})

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
        """hash -> folded record (the signed line's fields, the last fate). Cached while the file is unchanged."""
        try:
            st = os.stat(self.path)
        except FileNotFoundError:
            return {}
        if self._cache[0] == (st.st_mtime_ns, st.st_size):
            return {h: dict(v) for h, v in self._cache[1].items()}
        out: dict[str, dict] = {}
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
                    if cur["state"] == "gone":  # the same bytes signed off again after an operator's gone: open again
                        cur.update(state="signed")
                        cur.pop("why", None)
                    cur.update({k: r[k] for k in ("raw", "nonce", "from", "stage", "at", "unlocked", "value",
                                                  "datahash", "to") if k in r})
                    if "raw" in r and "datahash" not in r:  # a line written before the step identity was recorded
                        try:
                            d = decode_tx(bytes.fromhex(r["raw"][2:]))
                            cur.update(to=d["to"], value=d["value"], datahash=d["datahash"])
                        except Exception:  # noqa: BLE001 - an unreadable old line just has no step identity
                            pass
                elif r.get("event") == "broadcast":
                    cur["state"] = cur["state"] if cur["state"] in FINAL else "broadcast"
                    cur["accepted_by"] = r.get("accepted_by", [])
                elif r.get("event") == "fate":
                    cur.update(state=r["fate"], **{k: r[k] for k in ("status", "block", "blockHash", "gasUsed",
                                                                         "contractAddress", "why") if k in r})
                elif r.get("event") == "ack":  # an operator looked at a bad or unknown fate (pubnet.py ack)
                    cur["acked"] = r.get("why", "")
                    if cur["state"] not in FINAL:
                        cur["state"] = "acked"
                elif r.get("event") == "gone":  # checked absent from the chain (pubnet.py gone): not mined after all
                    cur.update(state="gone", why=r.get("why", ""))
        self._cache = ((st.st_mtime_ns, st.st_size), out)
        return {h: dict(v) for h, v in out.items()}

    def open_txs(self) -> list[dict]:
        return [t for t in self.txs().values() if t["state"] not in FINAL]

    def bad_txs(self) -> list[dict]:
        """Reverted, replaced or rejected, and not acknowledged by an operator (pubnet.py ack)."""
        return [t for t in self.txs().values() if not t.get("acked") and (t["state"] in ("replaced", "rejected") or (
            t["state"] == "mined" and t.get("status") not in ("0x1", 1)))]

    def fence_tx(self) -> dict | None:
        """The sender's last mined transaction (the highest block): the journal's fence."""
        mined = [t for t in self.txs().values() if t["state"] == "mined" and t.get("block") is not None]
        return max(mined, key=lambda t: t["block"]) if mined else None

    def floor(self) -> int:
        """The fence: the block of the sender's last mined transaction (each fate from a receipt two endpoints gave
        alike)."""
        return max([t.get("block", 0) for t in self.txs().values() if t["state"] == "mined"] or [0])


def resolve_one(net: Net, j: Journal, t: dict, sender: str, log=print) -> str:
    """One look at a journaled transaction's fate; records it once known. Returns its state."""
    h = t["hash"]
    try:
        rc = net.receipt(h)  # two endpoints alike (block hash, number, status): one invented receipt is not a fate
    except Transient:
        rc = None
    if rc:
        j.append({"event": "fate", "hash": h, "fate": "mined", "status": rc.get("status"),
                  "block": int(rc["blockNumber"], 16), "blockHash": rc.get("blockHash"), "gasUsed": rc.get("gasUsed"),
                  "contractAddress": rc.get("contractAddress")})
        log(f"{h}: mined in block {int(rc['blockNumber'], 16)}, status {rc.get('status')}")
        return "mined"
    try:  # the nonce as two endpoints agree at the agreed head: one endpoint cannot declare the transaction replaced
        n_head = int(net.quorum("eth_getTransactionCount", [sender, hex(net.head())]), 16)
    except Transient:
        n_head = None
    if t.get("nonce") is not None and n_head is not None and n_head > t["nonce"]:
        nonces = [n_head]
        # The nonce is used. Ours, on a node that has no receipt yet, or another transaction with the same nonce:
        # only after PUBNET_REPLACED_AFTER_S (60 s) of looks with no receipt anywhere is it taken as replaced.
        since = t.setdefault("_nonce_used_since", time.monotonic())
        if time.monotonic() - since < float(os.environ.get("PUBNET_REPLACED_AFTER_S", "60")):
            return "nonce-used"
        j.append({"event": "fate", "hash": h, "fate": "replaced",
                  "why": f"the sender's nonce is {max(nonces)} > {t['nonce']} and no endpoint has a receipt"})
        log(f"{h}: replaced (nonce {t['nonce']} used by another transaction)")
        return "replaced"
    try:
        known = bool(net.anywhere("eth_getTransactionByHash", [h]))
    except Transient:
        known = False
    if t.get("raw"):  # the same bytes again: at most one transaction can come of them
        for url in net.urls:
            try:
                _rpc_once(url, "eth_sendRawTransaction", [t["raw"]], net.timeout)
            except (Transient, Permanent):
                pass
    return "pending" if known else "unknown"


def absent(net: Net, t: dict, sender: str) -> str | None:
    """Why the journaled transaction t is not on the chain, or None if it may be: every endpoint asked for its receipt
    (one that has it is enough to keep it), and the sender's nonce, agreed by two endpoints at the agreed head, not
    above its nonce."""
    for url in net.urls:
        try:
            r = _rpc_once(url, "eth_getTransactionReceipt", [t["hash"]], net.timeout)
        except (Transient, Permanent):
            return None  # an endpoint that does not answer might have it
        if r:
            return None
    head = net.head()
    n = int(net.quorum("eth_getTransactionCount", [sender, hex(head)]), 16)
    if t.get("nonce") is None or n > t["nonce"]:
        return None
    return f"no endpoint has its receipt; the sender's nonce is {n} at block {head} (the transaction's {t['nonce']})"


def fence_lost(net: Net, j: Journal, sender: str) -> dict | None:
    """The journal's fence transaction, if it is no longer on the chain (else None)."""
    t = j.fence_tx()
    if t is None:
        return None
    try:
        return t if absent(net, t, sender) else None
    except Transient:
        return None


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
    lost = fence_lost(net, j, sender)
    if lost:
        log(f"pubnet: permanent: the journal's last mined transaction {lost['hash']} (stage {lost.get('stage')}, block"
            f" {lost.get('block')}, nonce {lost.get('nonce')}) is not on the chain now: no endpoint has its receipt and"
            f" the sender's nonce is not above it (a reorg, or a chain reloaded from an older state). Nothing is sent"
            f" until an operator has checked; then: pubnet.py gone --journal {j.path} --hash {lost['hash']} --sender"
            f" {sender}" + (f" --network {net.network}" if net.network else ""))
        return 3
    for t in j.bad_txs():  # a rejected or replaced one that a pool kept after all: look for its receipt once more
        if t["state"] in ("rejected", "replaced"):
            try:
                rc = net.receipt(t["hash"])
            except Transient:
                rc = None
            if rc:
                j.append({"event": "fate", "hash": t["hash"], "fate": "mined", "status": rc.get("status"),
                          "block": int(rc["blockNumber"], 16), "blockHash": rc.get("blockHash"),
                          "gasUsed": rc.get("gasUsed"), "contractAddress": rc.get("contractAddress")})
                log(f"{t['hash']}: was {t['state']}, found mined in block {int(rc['blockNumber'], 16)}")
    bad = j.bad_txs()
    for t in bad:
        log(f"pubnet: permanent: {t['hash']} (stage {t.get('stage')}): {t['state']}, status {t.get('status')}"
            f" {t.get('why', '')}; once checked by a human: pubnet.py ack --journal {j.path} --hash {t['hash']}")
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
        self._pin, self._pin_t = None, 0.0
        self.check_chains(chain_id(a.network) if not self.all_loopback else None)
        self.pin_lock, self.send_lock = threading.Lock(), threading.Lock()
        self.last_bcast: dict[str, float] = {}

    def log(self, msg: str) -> None:
        print(f"[{time.strftime('%H:%M:%S')}] {msg}", file=sys.stderr, flush=True)

    def check_chains(self, want: int | None) -> None:
        """Every endpoint on the same chain (the network's, or the first answer's); one on another chain is dropped. All
        asked at once, 5 s each: a hanging endpoint does not delay the start."""
        from concurrent.futures import ThreadPoolExecutor

        def ask(url):
            try:
                return int(_rpc_once(url, "eth_chainId", [], min(5.0, self.net.timeout)), 16)
            except (Transient, Permanent, ValueError) as e:
                return e

        with ThreadPoolExecutor(max_workers=len(self.net.urls)) as ex:
            answers = list(ex.map(ask, self.net.urls))
        keep = []
        for url, cid in zip(self.net.urls, answers):
            if isinstance(cid, Exception):
                self.log(f"chain id of {label(url)} unknown now ({str(cid)[:100]}); kept")
                self.net.failed(url)
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
        """The block every read at latest/pending/no block is answered at, recomputed every PUBNET_PIN_TTL_S (and at
        once after a receipt of the sender): max(agreed head - lag, the fence, the previous pin). The agreed head is
        the highest block two endpoints report; the fence is the block of the sender's last mined transaction in the
        journal (each from a receipt two endpoints gave alike). If the sender's nonce (two endpoints alike) is higher at
        the head than at that block, a transaction of its came later (sent around the proxy): the pin is the head."""
        with self.pin_lock:
            if self._pin is not None and time.monotonic() - self._pin_t < self.pin_ttl:
                return self._pin
            head = self.net.head()
            fence = self.journal.floor()
            if fence > head + 64:  # no block two endpoints have: a journal from before receipts needed two endpoints
                self.log(f"pin: the journal's fence {fence} is far above the agreed head {head}: not used")
                fence = 0
            # The previous pin is kept (a client's view never goes back) only while the agreed head is at or above it:
            # a head below it means the chain lost blocks (a reorg, an anvil reloaded from an older state) or the
            # endpoints fell behind; either way a pin above the head would be a block no endpoint has.
            prev = self._pin if self._pin is not None and self._pin <= head else 0
            if self._pin is not None and self._pin > head:
                self.log(f"pin: the agreed head {head} is below the previous pin {self._pin}: pinned again from the head")
            p = max(head - self.lag, fence, prev)
            if self.sender and p < head:
                at_p = int(self.net.quorum("eth_getTransactionCount", [self.sender, hex(p)]), 16)
                at_h = int(self.net.quorum("eth_getTransactionCount", [self.sender, hex(head)]), 16)
                if at_h > at_p:
                    self.log(f"pin: the sender's nonce is {at_h} at the head, {at_p} at {p}: pin = head {head}")
                    p = head
            if p != self._pin:
                self.event(kind="pin", block=p, head=head, fence=fence)
            self._pin, self._pin_t = p, time.monotonic()
            return p

    def bump(self, block: int) -> None:
        """A receipt of the sender's that two endpoints gave alike: reads from now on see its effects."""
        with self.pin_lock:
            if self._pin is None or block > self._pin:
                self._pin, self._pin_t = block, time.monotonic()

    def state_read(self, method: str, params: list):
        """A read a plan or a check decides on: at an explicit block (the pin for latest/pending/none), the same
        answer from two endpoints (pubnet.Net.quorum); an endpoint without the block, or with another answer, is not
        used. Gas estimates may differ a little between clients: two within 25 % agree, the larger is returned."""
        k = STATE_READS[method]
        p = params + [None] * (k + 1 - len(params)) if len(params) <= k else list(params)
        if p[k] in (None, "latest", "pending"):
            p[k] = hex(self.pin())
        elif isinstance(p[k], dict) and p[k].get("blockNumber") in ("latest", "pending"):
            p[k] = {"blockNumber": hex(self.pin())}
        near = None
        if method == "eth_estimateGas":
            near = lambda a, b: max(int(a, 16), int(b, 16)) <= 1.25 * min(int(a, 16), int(b, 16))  # noqa: E731
        return self.net.quorum(method, p, near=near)

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
        if method == "pubnet_status":  # answered here, without the network: is this the proxy of that journal?
            return {"journal": os.path.abspath(self.journal.path), "sender": self.sender, "pid": os.getpid(),
                    "endpoints": len(self.net.urls)}
        if method == "eth_chainId":
            if self.chain is None:
                self.chain = self.net.call("eth_chainId", [])
            return self.chain
        if method == "eth_blockNumber":
            return hex(self.pin())
        if method == "eth_getBlockByNumber" and params and params[0] in ("finalized", "safe"):
            return self.lowest_tagged(params)
        if method == "eth_getBlockByNumber" and params:
            tag = params[0]
            blockhash = lambda b: b.get("hash") if isinstance(b, dict) else b  # noqa: E731
            if tag in ("latest", "pending"):
                return self.net.quorum(method, [hex(self.pin())] + params[1:], key=blockhash, null_is_missing=True)
            if isinstance(tag, str) and tag.startswith("0x") and int(tag, 16) <= (self._pin or 0):
                return self.net.quorum(method, params, key=blockhash, null_is_missing=True)
            return self.net.call(method, params)
        if method == "eth_getTransactionCount" and params and self.sender and params[0].lower() == self.sender \
                and (len(params) < 2 or params[1] == "pending"):
            return hex(self.agreed_nonce()[0])
        if method in STATE_READS:
            return self.state_read(method, params)
        if method == "eth_getLogs" and params and isinstance(params[0], dict):
            f = dict(params[0])
            for key in ("fromBlock", "toBlock"):
                if key in f and f[key] in ("latest", "pending") or (key == "toBlock" and "toBlock" not in f
                                                                    and "blockHash" not in f):
                    f[key] = hex(self.pin())
            return self.net.call(method, [f])
        if method == "eth_getTransactionReceipt" and params:
            r = self.net.receipt(params[0])  # two endpoints alike, or none yet
            if r and self.sender and str(r.get("from", "")).lower() == self.sender:
                self.bump(int(r["blockNumber"], 16))
            if r is None:
                self.rebroadcast_if_due(params[0])
            return r
        if method in ANYWHERE:
            return self.net.anywhere(method, params)
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

    def lowest_tagged(self, params: list):
        """"finalized" / "safe": the lowest of the blocks `need` endpoints give (a node behind gives an older one,
        which only delays; one endpoint claiming a later block final is outvoted)."""
        deadline = time.monotonic() + self.net.budget
        attempt = 0
        while True:
            blocks = [b for _, b in self.net.each("eth_getBlockByNumber", params) if isinstance(b, dict) and b.get("number")]
            if len(blocks) >= self.net.need():
                return min(blocks, key=lambda b: int(b["number"], 16))
            attempt += 1
            pause = backoff(attempt, base=1.0, cap=8.0)
            if time.monotonic() + pause > deadline:
                raise Transient(f"pubnet: transient: eth_getBlockByNumber {params[0]}: fewer than {self.net.need()}"
                                " endpoints answer")
            time.sleep(pause)

    def rebroadcast_if_due(self, h: str) -> None:
        """A client waits for the receipt of a journaled transaction no endpoint has mined: the same bytes again
        every PUBNET_REBROADCAST_S (30 s), in case every mempool lost them (a broadcast during an outage). At most one
        transaction can come of them."""
        now = time.monotonic()
        if now - self.last_bcast.get(h, 0.0) < float(os.environ.get("PUBNET_REBROADCAST_S", "30")):
            return
        t = self.journal.txs().get(h)
        if not t or t["state"] in FINAL or not t.get("raw"):
            return
        self.last_bcast[h] = now
        self.log(f"{h}: no receipt yet; the same bytes again")
        self.broadcast(t["raw"], h, quiet=True)

    def agreed_nonce(self) -> tuple[int, int]:
        """(nonce, how many endpoints): every endpoint asked for the sender's latest and pending nonce; the highest
        value any of them reports must be both the latest and the pending nonce on two of them (one if only one is
        configured). Endpoints behind (a lower value) are outvoted; a higher value on one endpoint alone, or pending
        above latest (a transaction in flight), is waited out (transient)."""
        need = self.net.need()
        deadline = time.monotonic() + self.net.budget
        # The journal's own floor: one past the highest nonce of the sender's mined transactions (each from a receipt
        # two endpoints gave alike); two endpoints agreeing below it are behind (or lying), not the chain.
        mined = [t["nonce"] for t in self.journal.txs().values() if t["state"] == "mined" and t.get("nonce") is not None]
        floor = max(mined) + 1 if mined else 0
        attempt, seen = 0, []
        while True:
            seen = []
            for url in self.net.order():
                try:
                    lat = int(_rpc_once(url, "eth_getTransactionCount", [self.sender, "latest"], self.net.timeout), 16)
                    pen = int(_rpc_once(url, "eth_getTransactionCount", [self.sender, "pending"], self.net.timeout), 16)
                    self.net.served(url)
                except (Transient, Permanent) as e:
                    self.net.failed(url)
                    self.log(f"nonce: {e}")
                    continue
                seen.append((label(url), lat, pen))
            # A lagging node shows an older nonce, never a newer one: the highest value is the chain's; it must be
            # both the latest and the pending nonce on `need` endpoints (pending above latest: a transaction in flight).
            top = max([max(lat, pen) for _, lat, pen in seen] or [-1])
            agree = [ep for ep, lat, pen in seen if lat == pen == top]
            if len(agree) >= need and top >= floor:
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
        if self.sender and tx["from"] != self.sender:
            raise Permanent(f"pubnet: permanent: the transaction is signed by {tx['from']}, not the sender {self.sender}"
                            "; this proxy sends only the sender's")
        with self.send_lock:
            txs = self.journal.txs()
            if h in txs and txs[h]["state"] != "gone":  # the same bytes again (a retry): at most one transaction
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
            txs = self.journal.txs()  # as it is now: the inline resolution above may have recorded fates
            bad = self.journal.bad_txs()
            if bad:
                raise Permanent(f"pubnet: permanent: an earlier transaction was {bad[0]['state']}"
                                f" (status {bad[0].get('status')}): {bad[0]['hash']}; nothing is sent until an operator"
                                f" has looked (pubnet.py ack --journal {self.journal.path} --hash {bad[0]['hash']})")
            done = [t for t in txs.values() if t["state"] == "mined" and t.get("status") in ("0x1", 1)
                    and t.get("to") == tx["to"] and t.get("value") == tx["value"] and t.get("datahash") == tx["datahash"]]
            if done:
                raise Permanent(f"pubnet: permanent: this step was sent and mined already ({done[0]['hash']}, stage"
                                f" {done[0].get('stage')}): the same call again (to, value, calldata). Either its plan"
                                " read a stale state, or the flow repeats a call on purpose: one journal never holds two"
                                " identical calls, so such a flow needs its own journal or distinct calldata"
                                " (e2e/NETWORK.md, Limits)")
            if not self.sender:
                raise Permanent("pubnet: permanent: the proxy was started without --sender; it sends nothing")
            chain = int(self.dispatch("eth_chainId", [], stage), 16)
            if tx["chainId"] is not None and tx["chainId"] != chain:
                raise Permanent(f"pubnet: permanent: the transaction is for chain {tx['chainId']}, not {chain}")
            nonce, n_ep = self.agreed_nonce()
            if tx["nonce"] < nonce:
                raise Transient(f"pubnet: transient: stale plan: the transaction's nonce {tx['nonce']} is used (the"
                                f" sender's nonce is {nonce}); planned on a block before the sender's last transaction")
            if tx["nonce"] > nonce:  # the agreed view is behind the plan (two endpoints behind alike): wait it out
                raise Transient(f"pubnet: transient: the transaction's nonce {tx['nonce']} is above the agreed {nonce}:"
                                " the endpoints that agreed are behind")
            bal = int(self.state_read("eth_getBalance", [self.sender, "latest"]), 16)  # pinned, two endpoints alike
            need = tx["value"] + tx["gas"] * tx["maxFee"]
            if bal < need:
                raise Permanent(f"pubnet: permanent: insufficient funds: balance {bal} < value + gas x max fee {need}")
            self.journal.append({"event": "signed", "hash": h, "raw": raw_hex, "nonce": tx["nonce"],
                                 "from": tx["from"], "stage": stage, "to": tx["to"], "value": tx["value"],
                                 "datahash": tx["datahash"], "gas": tx["gas"], "nonce_agreed_by": n_ep})
            self.event(kind="send", hash=h, nonce=tx["nonce"], stage=stage)
            return self.broadcast(raw_hex, h)

    def broadcast(self, raw_hex: str, h: str, quiet: bool = False) -> str:
        from concurrent.futures import ThreadPoolExecutor

        def send(url):  # the same bytes to every endpoint at once: a hanging one delays nothing
            try:
                _rpc_once(url, "eth_sendRawTransaction", [raw_hex], self.net.timeout)
                return url, None
            except (Transient, Permanent) as e:
                return url, e

        with ThreadPoolExecutor(max_workers=len(self.net.urls)) as ex:
            results = list(ex.map(send, self.net.urls))
        accepted, rejected, unknown = [], [], []
        for url, e in results:
            if e is None:
                accepted.append(label(url))
            elif isinstance(e, Transient):
                unknown.append(str(e)[:160])
            else:
                msg = str(e)
                if re.search(r"already known|known transaction|already imported|alreadyknown", msg, re.I):
                    accepted.append(label(url))
                elif re.search(r"nonce too low", msg, re.I):
                    unknown.append(msg[:160])  # perhaps these very bytes, mined through another endpoint
                else:
                    rejected.append(msg[:160])
        self.last_bcast[h] = time.monotonic()
        self.journal.append({"event": "broadcast", "hash": h, "accepted_by": accepted, "rejected": rejected,
                             "unknown": unknown})
        self.log(f"broadcast {h}: accepted by {accepted}, rejected {rejected}, unknown {unknown}")
        if quiet:
            return h
        if not accepted and not unknown:
            # Every endpoint answered with an error. One may have kept the bytes all the same: only if no endpoint
            # has them (no receipt, not known in any pool) is the fate "rejected"; otherwise it stays open.
            st = resolve_one(self.net, self.journal, {"hash": h, "nonce": decode_tx(bytes.fromhex(raw_hex[2:]))["nonce"]},
                             self.sender, self.log)
            if st == "unknown":
                self.journal.append({"event": "fate", "hash": h, "fate": "rejected", "why": "; ".join(rejected)[:300]})
                raise Permanent(f"pubnet: permanent: every endpoint refused the transaction: {'; '.join(rejected)[:300]}")
            self.log(f"broadcast {h}: every endpoint answered an error, but it is {st}: its fate stays open")
        return h

    def send_unlocked(self, tx: dict, stage: str) -> str:
        """eth_sendTransaction (a local fork that signs for an impersonated sender): one endpoint, journaled."""
        if not (self.allow_unlocked and self.all_loopback):
            raise Permanent("pubnet: permanent: eth_sendTransaction only to a local chain with --allow-unlocked")
        if self.sender and str(tx.get("from", "")).lower() != self.sender:
            raise Permanent(f"pubnet: permanent: eth_sendTransaction from {tx.get('from')}, not the sender {self.sender}")
        with self.send_lock:
            open_ = self.journal.open_txs()
            for t in open_:
                resolve_one(self.net, self.journal, t, (t.get("from") or self.sender or ""), self.log)
            if self.journal.open_txs():
                raise Transient("pubnet: transient: an earlier transaction's fate is unknown")
            if self.journal.bad_txs():
                raise Permanent("pubnet: permanent: an earlier transaction reverted, was replaced or rejected:"
                                " nothing is sent until an operator has looked (pubnet.py ack)")
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
              f"{[label(u) for u in proxy.net.urls]}, lag {proxy.lag}, sender {proxy.sender}, fence {proxy.journal.floor()}")
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
    assert rlp_encode([b"cat", b"dog"]).hex() == "c88363617483646f67" and rlp_encode(b"\x05") == b"\x05"
    # signers recovered from transactions cast signed with anvil's second test key (legacy EIP-155, EIP-1559)
    legacy = ("f86707843b9aca0082520894000000000000000000000000000000000000dead05808401546d72a0485ab50394ff291b025666f6"
              "1d9518e4918c13d557ccc2c292e9d12849da5316a03f7f8a3ac3b95ca5cb2abb36f27998faa7494b543dd95e426301f9b86d36dc41")
    eip1559 = ("02f89183aa36a708843b9aca00847735940082c35094000000000000000000000000000000000000dead80a4b3de648b00000000"
               "0000000000000000000000000000000000000000000000000000002ac080a06eb283908c4da19f2c3e7d0e88d51a77cdc711c03d95"
               "01e27f0d4febac7ff8eba0305817dceaf48292a993de17e4c88c91140ce7ff4d5fa1c5127959c5977cd199")
    for raw, nonce in ((legacy, 7), (eip1559, 8)):
        d = decode_tx(bytes.fromhex(raw))
        assert d["from"] == "0x70997970c51812dc3a010c7d01b50e0d17dc79c8" and d["nonce"] == nonce, d
        assert d["chainId"] == 11155111
    for text, want in (("header not found", "transient"), ("HTTP 429", "transient"), ("-32001 block not found",
                       "transient"), ("execution reverted: x", "permanent"), ("insufficient funds for gas", "permanent"),
                       ("timeout; execution reverted", "permanent"), ("something odd", "permanent"),
                       ("missing trie node abc", "transient"), ("HTTP 503", "transient"),
                       ("[Revert] the candidate's wait is not over", "transient")):
        assert classify(text) == want, (text, classify(text))
    print("pubnet selftest: keccak, RLP, signer recovery, classification OK")
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
    k = sub.add_parser("ack")
    k.add_argument("--journal", required=True)
    k.add_argument("--hash", required=True)
    k.add_argument("--why", default="checked by the operator")
    g = sub.add_parser("gone")
    g.add_argument("--journal", required=True)
    g.add_argument("--hash", required=True)
    g.add_argument("--sender", required=True)
    g.add_argument("--network", default="sepolia")
    g.add_argument("--first")
    g.add_argument("--why", default="checked absent from the chain by the operator")
    sub.add_parser("acked").add_argument("--journal", required=True)
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
    elif a.cmd == "ack":
        if a.hash not in Journal(a.journal).txs():
            sys.exit(f"pubnet: {a.hash} is not in {a.journal}")
        Journal(a.journal).append({"event": "ack", "hash": a.hash, "why": a.why})
    elif a.cmd == "gone":
        jj = Journal(a.journal)
        t = jj.txs().get(a.hash)
        if t is None:
            sys.exit(f"pubnet: {a.hash} is not in {a.journal}")
        net = Net(a.network, first=a.first)
        try:
            why = absent(net, t, a.sender.lower())
        except Transient as e:
            sys.exit(f"pubnet: the chain could not be read ({e}): nothing marked")
        if not why:
            sys.exit(f"pubnet: {a.hash} may still be on the chain (an endpoint has its receipt, did not answer, or the"
                     " sender's nonce is above it): not marked gone")
        jj.append({"event": "gone", "hash": a.hash, "why": f"{a.why}: {why}"})
        print(f"pubnet: {a.hash} marked gone ({why}); it leaves the fence, the nonce floor and the done set")
    elif a.cmd == "acked":
        print(json.dumps([h for h, t in Journal(a.journal).txs().items() if t.get("acked")]))
    elif a.cmd == "rows":
        rows = [{"hash": t["hash"], "blockNumber": hex(t["block"]), "status": t.get("status"),
                 "gasUsed": t.get("gasUsed"), "to": t.get("to"), "contractAddress": t.get("contractAddress")}
                for t in Journal(a.journal).txs().values()
                if t.get("stage") == a.stage and t["state"] == "mined" and not t.get("acked")]
        print(json.dumps(rows))
    elif a.cmd == "classify":
        print(classify_failure(a.events, a.since, a.log))
    elif a.cmd == "selftest":
        return selftest()
    return 0


if __name__ == "__main__":
    sys.exit(main())
