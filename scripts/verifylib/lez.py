"""LEZ v0.4.0-rc3's blocks and transactions, read as its sequencer's RPC serves them, and a polite JSON-RPC client.

A port of the test network explorer's decoder (deploy/testnet/web/explorer/assets/lez.js, borsh.js, idl.js) to the
Python standard library, for scripts/verify.py. Layouts from logos-execution-zone v0.4.0-rc3 (lez/common/src/
{block,transaction}.rs, lee/state_machine/src/...): a block is `header {block_id u64, prev_block_hash [32], hash [32],
timestamp u64 (ms)}`, `body {transactions Vec<LeeTransaction>, reverted Vec<u64>}`, then `bedrock_status u8`, outside
the hash. Block hash = SHA-256(`/LEE/v0.3/Message/Block/` zero-padded to 32 ‖ bytes 0..40 ‖ bytes 72..80 ‖ body).
A transaction's hash is SHA-256 of its borsh without the first (variant) byte.
"""
from __future__ import annotations

import base64
import hashlib
import json
import random
import time
import urllib.error
import urllib.request
from pathlib import Path

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
BEDROCK_STATUS = ("Pending", "Safe", "Finalized")


class DecodeError(ValueError):
    pass


def b58encode(b: bytes) -> str:
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def b58decode(s: str) -> bytes:
    n = 0
    for c in s:
        i = B58.find(c)
        if i < 0:
            raise ValueError(f"not base58: {s!r}")
        n = n * 58 + i
    body = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return b"\0" * (len(s) - len(s.lstrip("1"))) + body


def pad32(s: str) -> bytes:
    b = s.encode()
    if len(b) > 32:
        raise ValueError(s)
    return b.ljust(32, b"\0")


def sha256(*parts: bytes) -> bytes:
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def account_from_public_key(pk: bytes) -> bytes:
    """`AccountId::from(&PublicKey)`: the public account a witness key signs for."""
    return sha256(pad32("/LEE/v0.3/AccountId/Public/"), pk)


def for_public_pda(program: bytes, seed: bytes) -> bytes:
    return sha256(pad32("/LEE/v0.2/AccountId/PDA/"), program, seed)


def ferrata_pda(program: bytes, tag: str, *parts: bytes, domain: str = "ferrata-lez/v1/") -> bytes:
    """Ferrata's PDA rule: `for_public_pda(program, SHA-256(domain ‖ tag ‖ parts))` (u64 parts little-endian)."""
    enc = [p if isinstance(p, bytes) else int(p).to_bytes(8, "little") for p in parts]
    return for_public_pda(program, sha256(domain.encode(), tag.encode(), *enc))


class Reader:
    """A strict borsh reader: little-endian integers, u32-prefixed vectors, one-byte options and bools."""

    def __init__(self, buf: bytes, pos: int = 0):
        self.buf = buf
        self.pos = pos

    def take(self, n: int) -> bytes:
        if n < 0 or self.pos + n > len(self.buf):
            raise DecodeError(f"need {n} bytes at {self.pos}, have {len(self.buf) - self.pos}")
        b = self.buf[self.pos:self.pos + n]
        self.pos += n
        return b

    def uint(self, w: int) -> int:
        return int.from_bytes(self.take(w), "little")

    def u8(self) -> int:
        return self.uint(1)

    def u16(self) -> int:
        return self.uint(2)

    def u32(self) -> int:
        return self.uint(4)

    def u64(self) -> int:
        return self.uint(8)

    def u128(self) -> int:
        return self.uint(16)

    def flag(self) -> bool:
        v = self.u8()
        if v > 1:
            raise DecodeError(f"bool {v}")
        return v == 1

    def bytes_vec(self) -> bytes:
        return self.take(self.u32())

    def vec(self, item):
        n = self.u32()
        if n > len(self.buf):
            raise DecodeError(f"vector of {n}")
        return [item(self) for _ in range(n)]

    def option(self, item):
        return item(self) if self.flag() else None

    def finish(self, what: str) -> None:
        if self.pos != len(self.buf):
            raise DecodeError(f"{what}: {len(self.buf) - self.pos} bytes left over")


# ---- transactions -----------------------------------------------------------------------------------------------

def _actor(r):
    return {"account_id": r.take(32), "program_account_id": r.take(32)}


def _root_call(r):
    return {"to": _actor(r), "message": r.bytes_vec()}


def _public_context(r):
    return {"actors": r.vec(_actor), "authorized_accounts": r.vec(lambda r: r.take(32)),
            "promotions": r.vec(lambda r: r.u64())}


def _nonces(r):
    return r.vec(lambda r: {"account": r.take(32), "nonce": r.u128()})


def _evidence(r):
    t = r.u8()
    if t == 0:
        return {"kind": "key", "public_key": r.take(32)}
    if t == 1:
        return {"kind": "pda", "program": r.take(32), "seed": r.take(32)}
    raise DecodeError(f"PublicAccountEvidence tag {t}")


def _witnesses(r):
    return r.vec(lambda r: {"signature": r.take(64), "public_key": r.take(32)})


def _fee_declaration(r):
    return {"payer": r.take(32), "gas_limit": r.u64(), "tip": r.u64(), "max_fee": r.u128()}


def _note(r):
    return {"ciphertext": r.bytes_vec(), "epk": r.bytes_vec()}


def _delivery(r):
    return {"envelope": {"from": _actor(r), "to": _actor(r), "message": r.bytes_vec()},
            "inherited_authorizations": r.vec(lambda r: r.take(32)), "inherits_entry_authorizations": r.flag(),
            "pda_seeds": r.vec(lambda r: r.take(32))}


_BOUNDARY = ("PrivateToPublic", "PublicToPrivate", "EndPrivateSubtree", "EndPublicSubtree")


def _boundary_step(r):
    t = r.u8()
    if t > 3:
        raise DecodeError(f"BoundaryStep tag {t}")
    return {"kind": _BOUNDARY[t], "delivery": _delivery(r)} if t < 2 else {"kind": _BOUNDARY[t]}


def _private_action(r):
    return {"nullifier": r.take(32), "root": r.take(32), "commitment": r.take(32), "encrypted_post_state": _note(r)}


def _window(r):
    return {"from": r.option(lambda r: r.u64()), "to": r.option(lambda r: r.u64())}


def _image_claim(r):
    t = r.u8()
    if t == 0:
        return {"kind": "Disclosed", "account_id": r.take(32), "image_id": r.take(32)}
    if t == 1:
        return {"kind": "Undisclosed", "root": r.take(32)}
    raise DecodeError(f"ProgramImageClaim tag {t}")


def _proven_fee(r):
    return {"refund_to": r.option(lambda r: r.take(32)),
            "fallback": r.option(lambda r: {"action": _private_action(r), "boundary": r.vec(_boundary_step)})}


def _proven_execution(r):
    return {"boundary": r.vec(_boundary_step),
            "async_messages": r.vec(lambda r: {"commitment": r.take(32), "note": _note(r)}),
            "recovery_bindings": r.vec(lambda r: {"address": r.take(32), "note": _note(r)}),
            "public_root": r.option(_root_call), "private_actions": r.vec(_private_action),
            "validity": {"blocks": _window(r), "timestamps": _window(r)},
            "program_image_claims": r.vec(_image_claim), "fee": r.option(_proven_fee)}


def read_transaction(r: Reader) -> dict:
    tag_at = r.pos
    tag = r.u8()
    start = r.pos
    if tag == 0:
        tx = {"kind": "Public", "public_context": _public_context(r)}
        tx["root"] = _root_call(r)
        tx["fee"] = r.option(_fee_declaration)
        tx["nonces"] = _nonces(r)
        tx["admission_evidence"] = r.vec(_evidence)
        tx["witnesses"] = _witnesses(r)
    elif tag == 1:
        tx = {"kind": "PrivacyPreserving", "public_context": _public_context(r)}
        tx["execution"] = _proven_execution(r)
        tx["root"] = tx["execution"]["public_root"]
        tx["fee"] = None
        tx["nonces"] = _nonces(r)
        tx["admission_evidence"] = r.vec(_evidence)
        tx["witnesses"] = _witnesses(r)
        tx["proof"] = r.bytes_vec()
    else:
        raise DecodeError(f"LeeTransaction tag {tag}")
    tx["size"] = r.pos - start
    tx["hash"] = sha256(r.buf[start:r.pos])
    tx["raw"] = r.buf[tag_at:r.pos]
    tx["signers"] = [account_from_public_key(w["public_key"]) for w in tx["witnesses"]]
    return tx


def decode_transaction(raw: bytes) -> dict:
    r = Reader(raw)
    tx = read_transaction(r)
    r.finish("LeeTransaction")
    return tx


def decode_block(raw: bytes) -> dict:
    """A block as `getBlock` serves it; `hash_ok` says whether the recomputed hash equals the header's."""
    r = Reader(raw)
    header = {"block_id": r.u64(), "prev_block_hash": r.take(32), "hash": r.take(32), "timestamp": r.u64()}
    body_start = r.pos
    txs = r.vec(read_transaction)
    reverted = r.vec(lambda r: r.u64())
    body_end = r.pos
    st = r.u8()
    if st > 2:
        raise DecodeError(f"bedrock_status {st}")
    r.finish("Block")
    computed = sha256(pad32("/LEE/v0.3/Message/Block/"), raw[0:40], raw[72:80], raw[body_start:body_end])
    return {"header": header, "transactions": txs, "reverted": reverted, "bedrock_status": BEDROCK_STATUS[st],
            "size": len(raw), "computed_hash": computed, "hash_ok": computed == header["hash"]}


# ---- JSON-RPC ---------------------------------------------------------------------------------------------------

class RpcError(Exception):
    pass


class Rpc:
    """JSON-RPC 2.0 over HTTPS, read-only by construction (`ALLOWED`), polite to a rate-limited gateway: one request
    at a time, the server's Retry-After honoured on 429, transient errors retried with a backoff within `budget_s`."""

    ALLOWED = frozenset({"getLastBlockId", "getBlock", "getBlockRange", "getTransaction", "getAccountView",
                         "getAccount", "getProgramIds", "getChannelId", "checkHealth", "getAccountsNonces",
                         "getAccountBalance"})

    def __init__(self, url: str, timeout: float = 60.0, budget_s: float = 600.0, user_agent: str = "ferrata-verify/1"):
        self.url = url
        self.timeout = timeout
        self.budget_s = budget_s
        self.ua = user_agent
        self.requests = 0
        self.bytes = 0

    def call(self, method: str, params=()):
        if method not in self.ALLOWED:
            raise RpcError(f"{method}: not a read this tool sends")
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
        deadline = time.monotonic() + self.budget_s
        delay = 1.0
        while True:
            req = urllib.request.Request(self.url, data=body, headers={
                "Content-Type": "application/json", "Accept": "application/json", "User-Agent": self.ua})
            try:
                with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                    data = resp.read()
                self.requests += 1
                self.bytes += len(data)
                out = json.loads(data)
                if "error" in out:
                    raise RpcError(f"{method}{list(params)}: {out['error']}")
                return out.get("result")
            except urllib.error.HTTPError as e:
                wait = None
                if e.code == 429:
                    try:
                        wait = float(e.headers.get("Retry-After") or 0) or None
                    except ValueError:
                        wait = None
                if e.code not in (429, 500, 502, 503, 504):
                    raise RpcError(f"{method}: HTTP {e.code}") from e
            except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
                wait = None
                last = e  # noqa: F841 - kept for the message below
            pause = wait if wait is not None else delay * (1 + random.random() / 2)
            if time.monotonic() + pause > deadline:
                raise RpcError(f"{method}: no answer within {self.budget_s:.0f} s")
            time.sleep(min(pause, 120.0))
            delay = min(delay * 2, 60.0)

    def get_json(self, url: str):
        req = urllib.request.Request(url, headers={"Accept": "application/json", "User-Agent": self.ua})
        with urllib.request.urlopen(req, timeout=self.timeout) as resp:
            data = resp.read()
        self.bytes += len(data)
        return json.loads(data)


def fetch_blocks(rpc: Rpc, first: int, last: int, cache: Path, step: int = 50, progress=None) -> None:
    """Every block from `first` to `last` into `cache/blocks-<start>.json` (a list of base64 strings), `step` at a
    time; batches already cached are kept. The cache is the raw answer, so anything derived from it can be redone."""
    cache.mkdir(parents=True, exist_ok=True)
    start = first - (first - 1) % step
    while start <= last:
        end = start + step - 1
        f = cache / f"blocks-{start:08d}.json"
        have = json.loads(f.read_text()) if f.exists() else []
        if len(have) < step and start + len(have) <= last:
            got = rpc.call("getBlockRange", [start, min(end, last) if end > last else end])
            if not isinstance(got, list):
                raise RpcError(f"getBlockRange [{start}, {end}]: {type(got).__name__}")
            if len(got) >= len(have):
                tmp = f.with_suffix(".part")
                tmp.write_text(json.dumps(got))
                tmp.replace(f)
            if progress:
                progress(start, end, rpc)
        start += step


def cached_blocks(cache: Path):
    """The cached blocks in order, each as (raw bytes)."""
    for f in sorted(cache.glob("blocks-*.json")):
        for b64 in json.loads(f.read_text()):
            yield base64.b64decode(b64)
