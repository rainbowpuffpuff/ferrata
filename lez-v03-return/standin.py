#!/usr/bin/env python3
"""The stand-in attestor for the public LEZ testnet's return leg (lez-v03-return/README.md). Standard library only.

A TESTNET STAND-IN, not part of Ferrata's design: it signs a checkpoint journal with keys the operator of the run
holds, in place of the zero-knowledge checkpoint proof that cannot exist for a LEZ v0.3.1 zone whose sequencer runs
no attestor. Never for production.

  standin.py selftest
  standin.py keygen --dir KEYS [--n 3]                      fresh secp256k1 keys, one file each, mode 600
  standin.py signers --dir KEYS                             their addresses, ascending (nothing secret is printed)
  standin.py tip --rpc URL [--block N]                      the newest block the node reports Finalized (or block N)
  standin.py genesis --rpc URL --signers A,B,C [--block N]  the vault's genesisState at that final block
  standin.py native-burns --identity ID.json --recipient 0x.. --out DIR
                                                            the rehearsal's two burns of a labelled test wallet
  standin.py wallet-burns --identity ID.json --seed-text TEXT --recipient 0x.. [--indices 0,1] [--kinds public,private]
        [--check DRIVER-NOTES.json] --out DIR               the burns of lez-v03-bridge's wallet, from its seed text
  standin.py burns-from-notes --identity ID.json --notes NOTES.json --out BURNS.json
                                                            burns.json from the LEZ wallet's notes (INTERFACE.md)
  standin.py layout --program ID --config-id 0x.. [--imported N] --out LAYOUT.json
                                                            where lez-v03-bridge keeps its burn accumulator
  standin.py checkpoint --identity ID.json --burns BURNS.json --parent-state 0x.. --signers A,B,C --threshold T
        --rpc URL --layout LAYOUT.json [--block N] [--imported N] [--deposit-root 0x..] --out CHECKPOINT.json
                                                            the vault's 17-word checkpoint journal for the burn root,
                                                            only after the node's bridge state matched the burns and
                                                            the tip was Finalized on the node (no option skips either)
  standin.py sign --checkpoint CHECKPOINT.json --verifier 0x.. --keys KEYS [--count K] --out CANDIDATE.json
                                                            the seal: K signatures (default: the threshold); refuses a
                                                            checkpoint whose source does not record both checks
  standin.py check-seal --candidate CANDIDATE.json          recovers the signers of a seal (no key needed)

Every digest, tree and encoding here is recomputed by other code before anything depends on it: the burn leaves
and the root by the Rust bridge core (`ferrata-e2e crown-release`), the journal digest by the vault, the signed
digest by the stand-in verifier contract (`attestationDigest`).
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import hmac
import json
import os
import secrets
import stat
import struct
import sys
import time
import urllib.request

# ---- Keccak-256 (Ethereum's), secp256k1 -------------------------------------------------------------------------

_RC = [
    0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000, 0x000000000000808B,
    0x0000000080000001, 0x8000000080008081, 0x8000000000008009, 0x000000000000008A, 0x0000000000000088,
    0x0000000080008009, 0x000000008000000A, 0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
    0x8000000000008003, 0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
    0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008,
]
_ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]
_M64 = (1 << 64) - 1


def _rol(x: int, n: int) -> int:
    return ((x << n) | (x >> (64 - n))) & _M64 if n else x


def _keccak_f(a: list) -> None:
    for rc in _RC:
        c = [a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4] for x in range(5)]
        d = [c[(x - 1) % 5] ^ _rol(c[(x + 1) % 5], 1) for x in range(5)]
        for x in range(5):
            for y in range(5):
                a[x][y] ^= d[x]
        b = [[0] * 5 for _ in range(5)]
        for x in range(5):
            for y in range(5):
                b[y][(2 * x + 3 * y) % 5] = _rol(a[x][y], _ROT[x][y])
        for x in range(5):
            for y in range(5):
                a[x][y] = b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y])
        a[0][0] ^= rc


def keccak256(data: bytes) -> bytes:
    rate = 136
    fill = rate - len(data) % rate
    padded = bytes(data) + (b"\x81" if fill == 1 else b"\x01" + bytes(fill - 2) + b"\x80")
    a = [[0] * 5 for _ in range(5)]
    for off in range(0, len(padded), rate):
        for i in range(rate // 8):
            a[i % 5][i // 5] ^= int.from_bytes(padded[off + 8 * i:off + 8 * i + 8], "little")
        _keccak_f(a)
    return b"".join(a[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
G = (0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
     0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8)


def _add(p, q):
    if p is None:
        return q
    if q is None:
        return p
    if p[0] == q[0]:
        if (p[1] + q[1]) % P == 0:
            return None
        m = 3 * p[0] * p[0] * pow(2 * p[1], -1, P) % P
    else:
        m = (q[1] - p[1]) * pow(q[0] - p[0], -1, P) % P
    x = (m * m - p[0] - q[0]) % P
    return x, (m * (p[0] - x) - p[1]) % P


def _mul(k: int, p):
    out = None
    while k:
        if k & 1:
            out = _add(out, p)
        p = _add(p, p)
        k >>= 1
    return out


def address_of_point(point) -> bytes:
    return keccak256(point[0].to_bytes(32, "big") + point[1].to_bytes(32, "big"))[12:]


def address_of_key(key: int) -> bytes:
    return address_of_point(_mul(key, G))


def sign(key: int, digest: bytes) -> bytes:
    """r(32) || s(32) || v(1): RFC 6979 nonce, s in the lower half of the order, v = 27 or 28 (as ecrecover takes
    it). Not constant-time: for throwaway testnet keys only."""
    z = int.from_bytes(digest, "big")
    x = key.to_bytes(32, "big")
    v_, k_ = b"\x01" * 32, b"\x00" * 32
    k_ = hmac.new(k_, v_ + b"\x00" + x + digest, hashlib.sha256).digest()
    v_ = hmac.new(k_, v_, hashlib.sha256).digest()
    k_ = hmac.new(k_, v_ + b"\x01" + x + digest, hashlib.sha256).digest()
    v_ = hmac.new(k_, v_, hashlib.sha256).digest()
    while True:
        v_ = hmac.new(k_, v_, hashlib.sha256).digest()
        k = int.from_bytes(v_, "big")
        if 1 <= k < N:
            point = _mul(k, G)
            r = point[0] % N
            s = pow(k, -1, N) * (z + r * key) % N
            if r and s:
                rec = (point[1] & 1) ^ (1 if point[0] >= N else 0)
                if s > N // 2:
                    s, rec = N - s, rec ^ 1
                return r.to_bytes(32, "big") + s.to_bytes(32, "big") + bytes([27 + rec])
        k_ = hmac.new(k_, v_ + b"\x00", hashlib.sha256).digest()
        v_ = hmac.new(k_, v_, hashlib.sha256).digest()


def recover(digest: bytes, sig: bytes) -> bytes:
    """The address that signed `digest`, with the checks of OpenZeppelin's ECDSA.tryRecover (low s, v 27 or 28)."""
    if len(sig) != 65:
        raise ValueError("a signature is 65 bytes")
    r, s, v = int.from_bytes(sig[:32], "big"), int.from_bytes(sig[32:64], "big"), sig[64]
    if not (1 <= r < N and 1 <= s <= N // 2 and v in (27, 28)):
        raise ValueError("invalid signature (r, s or v out of range)")
    y2 = (pow(r, 3, P) + 7) % P
    y = pow(y2, (P + 1) // 4, P)
    if y * y % P != y2:
        raise ValueError("invalid signature (r is not on the curve)")
    if (y & 1) != (v - 27):
        y = P - y
    z = int.from_bytes(digest, "big")
    rinv = pow(r, -1, N)
    point = _add(_mul(s * rinv % N, (r, y)), _mul((-z * rinv) % N, G))
    if point is None:
        raise ValueError("invalid signature")
    return address_of_point(point)


# ---- small encodings ----------------------------------------------------------------------------------------------

def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def unhex(text: str, length: int | None = None) -> bytes:
    out = bytes.fromhex(text.lower().removeprefix("0x"))
    if length is not None and len(out) != length:
        raise SystemExit(f"expected {length} bytes: {text}")
    return out


def hx(data: bytes) -> str:
    return "0x" + data.hex()


def word(value: int) -> bytes:
    return value.to_bytes(32, "big")


_B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58decode(text: str) -> bytes:
    n = 0
    for ch in text:
        n = n * 58 + _B58.index(ch)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return bytes(len(text) - len(text.lstrip("1"))) + body


def b58encode(data: bytes) -> str:
    n, out = int.from_bytes(data, "big"), ""
    while n:
        n, r = divmod(n, 58)
        out = _B58[r] + out
    return "1" * (len(data) - len(data.lstrip(b"\x00"))) + out


def account_id(text: str) -> str:
    """An account id as LEZ's RPC takes it (base58 of 32 bytes), from base58 or 0x-hex."""
    raw = unhex(text, 32) if text.startswith("0x") else b58decode(text)
    if len(raw) != 32:
        raise SystemExit(f"not a 32-byte account id: {text}")
    return b58encode(raw)


# ---- the bridge core's trees, burns and checkpoint state (bridge/crates/ferrata-bridge-core, checkpoint-core) ---

TREE_DEPTH = 32
BURN_TAG = sha256(b"ferrata/burn/leaf/v2")
CHECKPOINT_DOMAIN = sha256(b"ferrata/vault/checkpoint/v3")
RELEASE_DOMAIN = sha256(b"ferrata/vault/release/v2")
STAND_IN_IMAGE_ID = sha256(b"ferrata/stand-in/checkpoint-attestation/v1")
EIP712_NAME = b"Ferrata stand-in checkpoint attestation (testnet only)"
DOMAIN_TYPEHASH = keccak256(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
ATTESTATION_TYPEHASH = keccak256(b"CheckpointAttestation(bytes32 imageId,bytes32 journalDigest)")


def node(left: bytes, right: bytes) -> bytes:
    return sha256(b"\x01" + left + right)


def zero_hashes() -> list:
    zero = [bytes(32)]
    for _ in range(TREE_DEPTH):
        zero.append(node(zero[-1], zero[-1]))
    return zero


def tree_root(leaves: list) -> bytes:
    """Root of the depth-32 tree whose first leaves are `leaves` (bridge core `Tree::root`)."""
    zero, level = zero_hashes(), list(leaves)
    for h in range(TREE_DEPTH):
        level = [node(level[i], level[i + 1] if i + 1 < len(level) else zero[h]) for i in range(0, len(level), 2)]
    return level[0] if level else zero[TREE_DEPTH]


def tree_frontier(leaves: list) -> list:
    """The canonical frontier (bridge core `IncrementalTree::frontier`): f[h] is the completed subtree of height h on
    the left edge where bit h of the leaf count is 1, zero bytes elsewhere."""
    zero = zero_hashes()
    branch, count = [bytes(32)] * TREE_DEPTH, 0
    for leaf in leaves:
        running, saved = leaf, False
        for h in range(TREE_DEPTH):
            if (count >> h) & 1:
                running = node(branch[h], running)
            else:
                if not saved:
                    branch[h], saved = running, True
                running = node(running, zero[h])
        count += 1
    return [branch[h] if (count >> h) & 1 else bytes(32) for h in range(TREE_DEPTH)]


def root_from_frontier(count: int, frontier: list) -> bytes:
    zero, running = zero_hashes(), bytes(32)
    for h in range(TREE_DEPTH):
        if (count >> h) & 1 == 0 and frontier[h] != bytes(32):
            raise ValueError("the frontier is not canonical for this count")
        running = node(frontier[h], running) if (count >> h) & 1 else node(running, zero[h])
    return running


def burn_leaf(config_id: bytes, index: int, token: bytes, amount: bytes, c_out: bytes, r: bytes) -> bytes:
    return sha256(b"\x02" + BURN_TAG + config_id + index.to_bytes(8, "big") + token + amount + c_out + r)


def prf(key: bytes, message: bytes) -> bytes:
    return hmac.new(key, message, hashlib.sha256).digest()


def burn_note(seed: bytes, config_id: bytes, i: int, chain_id: int, vault: bytes, recipient: bytes, token: bytes,
              amount: int) -> dict:
    """The wallet's i-th burn note under `config_id` (bridge core `BurnNote::create`) with its release secret."""
    k_auth, k_view = prf(seed, b"ferrata/auth/v1"), prf(seed, b"ferrata/view/v1")
    r = prf(k_view, b"ferrata/burn-tag/v1" + config_id + i.to_bytes(8, "big"))
    s_out = prf(k_auth, b"ferrata/s-out/v1" + config_id + r)
    c_out = sha256(b"ferrata/Cout/v1" + s_out + word(chain_id) + vault + recipient)
    pad = prf(k_view, b"ferrata/memo/v1" + r)[:20]
    memo = bytes(a ^ b for a, b in zip(recipient, pad))
    mac = prf(k_view, b"ferrata/burn-mac/v2" + r + memo + token + word(amount) + c_out)[:16]
    return {"i": i, "r": r, "memo": memo, "mac": mac, "token": token, "amount": word(amount), "c_out": c_out,
            "s_out": s_out, "recipient": recipient, "nullifier": sha256(b"ferrata/nf-out/v2" + s_out)}


def key_set_hash(keys: list, timeframe: int = 0) -> bytes:
    """checkpoint-core / attestor-core `key_set_hash` (v2): the keys are 32-byte words."""
    return sha256(b"ferrata/keyset/v2" + len(keys).to_bytes(4, "big") + timeframe.to_bytes(4, "big") + b"".join(keys))


def signer_words(signers: list) -> list:
    """The stand-in key set as the state's key set: each address left-padded to 32 bytes, ascending."""
    raw = sorted(unhex(s, 20) for s in signers)
    if len(set(raw)) != len(raw) or not raw:
        raise SystemExit("the signer set must be distinct addresses")
    return [bytes(12) + a for a in raw]


def lez_state(tip_id: int, tip_hash: bytes, keys_hash: bytes, config_tip_hash: bytes, burn_count: int,
              frontier: list, imported: int, sunset: bool) -> bytes:
    frontier_hash = sha256(b"ferrata/burn-frontier/v1" + b"".join(frontier))
    return sha256(b"ferrata/lez-state/v1" + tip_id.to_bytes(8, "big") + tip_hash + keys_hash + config_tip_hash
                  + burn_count.to_bytes(8, "big") + frontier_hash + imported.to_bytes(8, "big") + bytes([sunset]))


def checkpoint_journal(c: dict) -> bytes:
    """The vault's 17-word checkpoint journal (FerrataVault._checkpointDigest hashes exactly these bytes)."""
    return b"".join([
        CHECKPOINT_DOMAIN, word(c["chainId"]), bytes(12) + unhex(c["vault"], 20), unhex(c["lezConfigId"], 32),
        word(c["nMin"]), word(c["tMin"]), unhex(c["parentState"], 32), unhex(c["newState"], 32),
        unhex(c["burnRoot"], 32), word(c["burnCount"]), word(c["importedLeafCount"]), word(int(c["sunset"])),
        unhex(c["echoNonce"], 32), word(c["tipId"]), unhex(c["tipHash"], 32), word(c["n"]), word(c["t"]),
    ])


def attestation_digest(chain_id: int, verifier: bytes, journal_digest: bytes) -> bytes:
    """What a stand-in key signs (StandInCheckpointVerifier.attestationDigest): EIP-712, the verifier's domain."""
    domain = keccak256(DOMAIN_TYPEHASH + keccak256(EIP712_NAME) + keccak256(b"1") + word(chain_id)
                       + bytes(12) + verifier)
    struct_hash = keccak256(ATTESTATION_TYPEHASH + STAND_IN_IMAGE_ID + journal_digest)
    return keccak256(b"\x19\x01" + domain + struct_hash)


# ---- LEZ v0.3.1 over its sequencer's JSON-RPC (read-only) ----------------------------------------------------------

BLOCK_PREFIX = b"/LEE/v0.3/Message/Block/" + bytes(8)  # lez/common/src/block.rs HashableBlockData::compute_hash
STATUS = {0: "Pending", 1: "Safe", 2: "Finalized"}  # lez/common/src/block.rs BedrockStatus (borsh variant index)


class Lez:
    """LEZ v0.3.1's sequencer RPC (lez/sequencer/service/rpc/src/lib.rs): reads only. Refuses any method that is not
    a `get*` read, so this tool cannot send a transaction."""

    def __init__(self, url: str):
        if not (url.startswith("https://") or url.startswith("http://127.0.0.1") or url.startswith("http://localhost")):
            raise SystemExit("the LEZ RPC must be https, or a local endpoint")
        self.url = url

    def call(self, method: str, params: list):
        if not method.startswith("get"):
            raise SystemExit(f"refusing {method}: this tool only reads")
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        req = urllib.request.Request(self.url, data=body, headers={"Content-Type": "application/json",
                                                                   "User-Agent": "curl/8"})
        last = None
        for attempt in range(4):
            try:
                with urllib.request.urlopen(req, timeout=40) as reply:
                    out = json.load(reply)
                if "error" in out:
                    raise SystemExit(f"{method}: {out['error']}")
                return out["result"]
            except OSError as e:  # network: try again
                last = e
                time.sleep(2 + 3 * attempt)
        raise SystemExit(f"{method}: {last}")

    def head(self) -> int:
        return int(self.call("getLastBlockId", []))

    def block(self, block_id: int) -> dict | None:
        raw = self.call("getBlock", [block_id])
        return None if raw is None else decode_block(base64.b64decode(raw))

    def shard(self, account: str, program: str) -> bytes:
        """The account's data under `program` (v0.3.1 keeps one shard of data per program in every account)."""
        got = self.call("getAccount", [account])
        return bytes(got["data"]["shards"].get(program, []))


def decode_block(raw: bytes) -> dict:
    """borsh(Block) of v0.3.1: header {block_id u64, prev_block_hash [32], hash [32], timestamp u64 (ms)}, body
    {transactions: Vec<LeeTransaction>}, bedrock_status u8. The hash is recomputed here from the block's own bytes:
    sha256(PREFIX || borsh(block_id, prev_block_hash, timestamp, transactions)); the transactions are the bytes
    between the header and the status byte, so they need no parsing for this."""
    if len(raw) < 85:
        raise SystemExit("a block of fewer than 85 bytes")
    block_id, = struct.unpack_from("<Q", raw, 0)
    timestamp, = struct.unpack_from("<Q", raw, 72)
    tx_count, = struct.unpack_from("<I", raw, 80)
    recomputed = sha256(BLOCK_PREFIX + raw[0:40] + raw[72:80] + raw[80:-1])
    if raw[-1] not in STATUS:
        raise SystemExit(f"block {block_id}: unknown bedrock status {raw[-1]}")
    return {"id": block_id, "prev": raw[8:40], "hash": raw[40:72], "timestamp_ms": timestamp, "tx_count": tx_count,
            "status": STATUS[raw[-1]], "recomputed_hash": recomputed, "bytes": len(raw),
            "sha256": sha256(raw).hex(), "raw": raw}


class Reader:
    def __init__(self, data: bytes):
        self.data, self.pos = data, 0

    def take(self, n: int) -> bytes:
        if self.pos + n > len(self.data):
            raise ValueError(f"truncated at {self.pos}")
        out = self.data[self.pos:self.pos + n]
        self.pos += n
        return out

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]


def parse_block(raw: bytes) -> list:
    """borsh(Block) of LEZ v0.3.1, every byte accounted for. Public transactions (tag 0) are read in full:
    Message {program_account_id, shard_selectors: [(account_id, program_account_id)], nonces, instruction_data,
    fee: Option<FeeDeclaration>} and WitnessSet {(signature, public key)*} (lee/state_machine/src/public_transaction/
    message.rs, lee/state_machine/core/src/account.rs ProgramShardSelector). A privacy-preserving transaction (tag 1)
    stops the walk with ValueError: its layout is not read here, so whoever needs every transaction refuses."""
    r = Reader(raw)
    r.take(80)
    txs = []
    for index in range(r.u32()):
        start, tag = r.pos, r.take(1)[0]
        if tag != 0:
            raise ValueError(f"transaction {index}: tag {tag}, not a public transaction")
        program = r.take(32)
        shards = [(r.take(32), r.take(32)) for _ in range(r.u32())]
        nonces = [int.from_bytes(r.take(16), "little") for _ in range(r.u32())]
        data = r.take(r.u32())
        fee_tag = r.take(1)[0]
        if fee_tag > 1:
            raise ValueError(f"transaction {index}: fee option tag {fee_tag}")
        fee = None
        if fee_tag == 1:
            fee = {"payer": r.take(32), "gas_limit": struct.unpack("<Q", r.take(8))[0],
                   "tip": struct.unpack("<Q", r.take(8))[0], "max_fee": int.from_bytes(r.take(16), "little")}
        witnesses = [(r.take(64), r.take(32)) for _ in range(r.u32())]
        txs.append({"program": program, "shards": shards, "nonces": nonces, "data": data, "fee": fee,
                    "witnesses": witnesses, "bytes": r.pos - start})
    if r.pos != len(raw) - 1:
        raise ValueError(f"{len(raw) - 1 - r.pos} bytes between the last transaction and the status byte")
    return txs


def checked_block(lez: Lez, block_id: int) -> dict:
    b = lez.block(block_id)
    if b is None:
        raise SystemExit(f"the node has no block {block_id}")
    if b["id"] != block_id:
        raise SystemExit(f"asked for block {block_id}, got {b['id']}")
    if b["recomputed_hash"] != b["hash"]:
        raise SystemExit(f"block {block_id}: the hash recomputed from its contents is not its header's")
    return b


WALK = 64  # at most this many blocks are walked: down from the head to a Finalized one, or between a tip and a read


def newest_final(lez: Lez) -> dict:
    """The newest block the node reports Finalized. Finality is monotone in the block id (the sequencer marks every
    block up to the finalized one, lez/sequencer/actors/storage/src/actor.rs:246-266): walked down from the head for
    up to WALK blocks (on the public testnet the flag is a few blocks behind the head), then a binary search below."""
    head = lez.head()
    for block_id in range(head, max(head - WALK, 0), -1):
        b = checked_block(lez, block_id)
        if b["status"] == "Finalized":
            return b
    lo, hi = 1, head - WALK
    if hi < 1 or checked_block(lez, lo)["status"] != "Finalized":
        raise SystemExit("no block of this node is Finalized")
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if checked_block(lez, mid)["status"] == "Finalized":
            lo = mid
        else:
            hi = mid - 1
    return checked_block(lez, lo)


def final_tip(lez: Lez, block_id: int | None, show_only: bool = False) -> dict:
    """The tip a checkpoint is cut at. `block_id` None: the newest Finalized block. A block the node does not report
    Finalized is refused: on the public testnet such blocks are replaced (block 83006 of 18 November 2026, 08:50 UTC,
    was gone from the node two hours later). `show_only` (the `tip` command) returns it for display; nothing that
    signs passes it."""
    tip = newest_final(lez) if block_id is None else checked_block(lez, block_id)
    if tip["status"] != "Finalized" and not show_only:
        raise SystemExit(f"block {tip['id']} is {tip['status']} on the node, not Finalized: nothing is attested")
    parent = checked_block(lez, tip["id"] - 1) if tip["id"] > 1 else None
    if parent is not None and parent["hash"] != tip["prev"]:
        raise SystemExit(f"block {tip['id']} does not link to block {tip['id'] - 1}")
    return tip


def read_field(lez: Lez, layout: dict, name: str, cache: dict) -> bytes | None:
    """One field of the bridge's state as lez-v03-return/INTERFACE.md describes it: {account, program, offset,
    length} (account and program default to the layout's)."""
    spec = layout.get(name)
    if spec is None:
        return None
    account = account_id(spec.get("account", layout.get("account", "")))
    program = account_id(spec.get("program", layout.get("program", "")))
    if (account, program) not in cache:
        cache[(account, program)] = lez.shard(account, program)
    data, off, length = cache[(account, program)], int(spec["offset"]), int(spec["length"])
    if off + length > len(data):
        raise SystemExit(f"{name}: the shard of {account} under {program} has {len(data)} bytes, "
                         f"the layout reads {off}..{off + length}")
    return data[off:off + length]


def read_accumulator(lez: Lez, layout: dict) -> dict:
    """The burn accumulator as the node holds it now, read between two reads of the head that agree (so it is the
    state after that block)."""
    endian = layout.get("count_endian", "little")
    for _ in range(6):
        before, cache = lez.head(), {}
        count = int.from_bytes(read_field(lez, layout, "burn_count", cache), endian)
        root = read_field(lez, layout, "burn_root", cache)
        flat = read_field(lez, layout, "burn_frontier", cache)
        imported = read_field(lez, layout, "imported_leaf_count", cache)
        imported_root = read_field(lez, layout, "imported_root", cache)
        config_id = read_field(lez, layout, "config_id", cache)
        constants = [(e, read_field(lez, {**layout, "_": {**e, "length": len(unhex(e["hex"]))}}, "_", cache))
                     for e in layout.get("expect", [])]
        if lez.head() == before:
            for e, got in constants:
                if got != unhex(e["hex"]):
                    raise SystemExit(f"the bridge's state is not laid out as the layout says: {e.get('what', e)}")
            frontier = None if flat is None else [flat[32 * h:32 * h + 32] for h in range(TREE_DEPTH)]
            return {"head": before, "burn_count": count, "burn_root": root, "burn_frontier": frontier,
                    "imported_leaf_count": None if imported is None else int.from_bytes(imported, endian),
                    "imported_root": imported_root, "config_id": config_id}
        time.sleep(2)
    raise SystemExit("the head moved during six reads of the accumulator")


def read_state(lez: Lez, layout: dict) -> tuple:
    """The accumulator and the block it is the state after: the head block is read before and after the accumulator
    and must be the same block both times (same id, same hash), so a head replaced during the read is not taken."""
    for _ in range(6):
        before = checked_block(lez, lez.head())
        chain = read_accumulator(lez, layout)
        if chain["head"] == before["id"] and checked_block(lez, chain["head"])["hash"] == before["hash"]:
            return chain, before
        time.sleep(2)
    raise SystemExit("the head block changed during six reads of the accumulator")


def bridge_watch(layout: dict) -> tuple:
    """The ids a transaction that can change the accumulator names at its root: the bridge program, and every account
    the layout reads."""
    if not layout.get("program"):
        raise SystemExit("the layout names no bridge program")
    accounts = {b58decode(account_id(layout["account"]))} if layout.get("account") else set()
    for v in list(layout.values()) + list(layout.get("expect", [])):
        if isinstance(v, dict) and v.get("account"):
            accounts.add(b58decode(account_id(v["account"])))
    return b58decode(account_id(layout["program"])), accounts


def blocks_between(lez: Lez, low: dict, high: dict) -> list:
    """The blocks after `low` up to `high` as the node holds them now: each hash recomputed from its contents, each
    linked to the one before, the first to `low`, and the last one `high` itself (so none was replaced meanwhile)."""
    if not 0 < high["id"] - low["id"] <= WALK:
        raise SystemExit(f"blocks {low['id']} to {high['id']}: more than {WALK} blocks apart, or not in order")
    blocks, prev = [], low
    for block_id in range(low["id"] + 1, high["id"] + 1):
        b = checked_block(lez, block_id)
        if b["prev"] != prev["hash"]:
            raise SystemExit(f"block {block_id} does not link to block {block_id - 1}")
        blocks.append(b)
        prev = b
    if prev["hash"] != high["hash"]:
        raise SystemExit(f"block {high['id']} changed on the node while the blocks were read")
    return blocks


def no_bridge_transaction(blocks: list, program: bytes, accounts: set) -> list:
    """Refuses unless every transaction of `blocks` is one this tool reads in full (a public transaction) and none
    names the bridge program or an account of the accumulator, at its root or in a shard selector. Why that is
    enough (lez-v03-return/README.md, "A tip below the head"): in v0.3.1 a program's shard of an account is written
    only by that program's own apply; lez-v03-bridge burns and imports only as a transaction's root call (it refuses a
    caller: guests/bridge/src/main.rs:125, 237), and a public transaction's root call is its message's
    program_account_id. A privacy-preserving transaction is not read here, so one of those refuses as well."""
    out = []
    for b in blocks:
        try:
            txs = parse_block(b["raw"])
        except ValueError as e:
            raise SystemExit(f"block {b['id']}: a transaction this tool does not read ({e}); it could change the "
                             "accumulator, so only a tip at the read head is attested")
        for k, tx in enumerate(txs):
            named = {tx["program"]} | {acc for acc, _ in tx["shards"]} | {prog for _, prog in tx["shards"]}
            if program in named or accounts & named:
                raise SystemExit(f"block {b['id']} carries a transaction of the bridge program (transaction {k}): the "
                                 "accumulator may differ between the tip and the read")
        out.append({"id": b["id"], "hash": hx(b["hash"]), "status": b["status"], "transactions": len(txs),
                    "root_programs": sorted({b58encode(t["program"]) for t in txs})})
    return out


# ---- the layout of lez-v03-bridge's state (its core crate, read on 18 November 2026) -------------------------------

PDA_PREFIX = b"/LEE/v0.2/AccountId/PDA/" + bytes(8)  # lee/state_machine/core/src/program/mod.rs:242-257 (v0.3.1)
BRIDGE_DOMAIN = b"ferrata-lez/v1/bridge/"


def bridge_pda(program: bytes, tag: bytes, *parts: bytes) -> bytes:
    """A public PDA of the bridge program: LEZ v0.3.1's `AccountId::for_public_pda` over the bridge's seed
    sha256("ferrata-lez/v1/bridge/" || tag || parts)."""
    return sha256(PDA_PREFIX + program + sha256(BRIDGE_DOMAIN + tag + b"".join(parts)))


def bridge_layout(program: bytes, config_id: bytes, imported: int | None) -> dict:
    """INTERFACE.md's layout for `lez-v03-bridge`: `burns(cid)` holds borsh(BridgeState::Burns(BurnTree {config_id,
    count: u64, frontier: Vec<[u8; 32]>})), variant byte 5; `root(cid, n)` holds borsh(BridgeState::Root(RootState
    {config_id, evm_chain_id: u64, vault: [u8; 20], leaf_count: u64, root, ..})), variant byte 1."""
    burns = b58encode(bridge_pda(program, b"burns", config_id))
    layout = {
        "what": "lez-v03-bridge's state as its core crate lays it out (borsh); derived, to be checked against the node",
        "program": b58encode(program), "account": burns, "count_endian": "little",
        "changes_only_in_root_calls": "lez-v03-bridge writes burns(cid) only in Burn and root(cid, n) only in "
                                      "ImportRoot, and both refuse a caller (guests/bridge/src/main.rs:125, 237)",
        "config_id": {"offset": 1, "length": 32}, "burn_count": {"offset": 33, "length": 8},
        "burn_frontier": {"offset": 45, "length": 1024},
        "expect": [{"offset": 0, "hex": "05", "what": "burns(cid) holds BridgeState::Burns (variant 5)"},
                   {"offset": 41, "hex": "20000000", "what": "the frontier has 32 nodes"}],
    }
    if imported is not None:
        root = b58encode(bridge_pda(program, b"root", config_id, imported.to_bytes(8, "little")))
        layout["imported_leaf_count"] = {"account": root, "offset": 61, "length": 8}
        layout["imported_root"] = {"account": root, "offset": 69, "length": 32}
        layout["expect"] += [
            {"account": root, "offset": 0, "hex": "01", "what": f"root(cid, {imported}) holds BridgeState::Root"},
            {"account": root, "offset": 1, "hex": config_id.hex(), "what": "root(cid, n) is of this configuration"}]
    return layout


# ---- commands -----------------------------------------------------------------------------------------------------

def load_keys(directory: str) -> list:
    """[(address, key)] ascending by address; refuses a key file that others can read."""
    out = []
    for name in sorted(os.listdir(directory)):
        if not name.endswith(".key"):
            continue
        path = os.path.join(directory, name)
        if stat.S_IMODE(os.stat(path).st_mode) & 0o077:
            raise SystemExit(f"{path}: a key file must be mode 600")
        key = int(open(path).read().strip(), 16)
        if not 1 <= key < N:
            raise SystemExit(f"{path}: not a secp256k1 key")
        out.append((address_of_key(key), key))
    if not out:
        raise SystemExit(f"no *.key file in {directory}")
    return sorted(out)


def cmd_keygen(a) -> None:
    os.makedirs(a.dir, mode=0o700, exist_ok=True)
    if any(n.endswith(".key") for n in os.listdir(a.dir)):
        raise SystemExit(f"{a.dir} already holds keys: nothing written")
    for i in range(a.n):
        key = 0
        while not 1 <= key < N:
            key = int.from_bytes(secrets.token_bytes(32), "big")
        fd = os.open(os.path.join(a.dir, f"signer-{i}.key"), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as f:
            f.write(f"{key:064x}\n")
    cmd_signers(a)


def cmd_signers(a) -> None:
    print(json.dumps({"signers": [hx(addr) for addr, _ in load_keys(a.dir)]}))


def tip_record(tip: dict) -> dict:
    return {"id": tip["id"], "hash": hx(tip["hash"]), "prev": hx(tip["prev"]), "timestamp_ms": tip["timestamp_ms"],
            "status": tip["status"], "tx_count": tip["tx_count"], "bytes": tip["bytes"], "sha256": tip["sha256"],
            "hash_recomputed_from_contents": True}


def cmd_tip(a) -> None:
    lez = Lez(a.rpc)
    tip = final_tip(lez, a.block, show_only=True)
    print(json.dumps({"rpc": a.rpc, "head": lez.head(), "tip": tip_record(tip)}))


def cmd_genesis(a) -> None:
    lez = Lez(a.rpc)
    tip = final_tip(lez, a.block)
    keys = signer_words(a.signers.split(","))
    state = lez_state(tip["id"], tip["hash"], key_set_hash(keys), bytes(32), 0, [bytes(32)] * TREE_DEPTH, 0, False)
    print(json.dumps({"genesisState": hx(state), "tip": tip_record(tip), "rpc": a.rpc,
                      "signers": [hx(k[12:]) for k in keys], "keySetHash": hx(key_set_hash(keys)),
                      "configTipHash": hx(bytes(32)), "what": "LezState at a block the public LEZ node reports "
                      "Finalized, no burns, no imports; the key set is the STAND-IN signer set"}))


def identity(path: str) -> dict:
    d = json.load(open(path))
    return {"chainId": int(d["chainId"]), "vault": unhex(d["vault"], 20), "configId": unhex(d["configId"], 32)}


REHEARSAL_SEED_TEXT = "ferrata testnet-return rehearsal: bridge seed (test value)"


def wallet_burns(ident: dict, seed_text: str, recipient: bytes, amount: int, indices: list, kinds: list, origin: str,
                 label: str, out: str) -> dict:
    """The burns of the wallet whose bridge seed is sha256(seed_text) (lez-v03-bridge's `deposits::keys`), in the
    order given (the accumulator's order): burns.json and burn-notes.json as `ferrata-e2e crown-release` reads them."""
    seed = sha256(seed_text.encode())
    burns, notes, leaves = [], [], []
    for k, (i, kind) in enumerate(zip(indices, kinds)):
        n = burn_note(seed, ident["configId"], i, ident["chainId"], ident["vault"], recipient, bytes(20), amount)
        leaf = burn_leaf(ident["configId"], k, n["token"], n["amount"], n["c_out"], n["r"])
        leaves.append(leaf)
        burns.append({"burn_index": k, "block": None, "origin": origin, "r": hx(n["r"]), "memo": hx(n["memo"]),
                      "mac": hx(n["mac"]), "token": hx(n["token"]), "amount": hx(n["amount"]), "c_out": hx(n["c_out"]),
                      "leaf": hx(leaf)})
        notes.append({"i": i, "kind": kind, "r": hx(n["r"]), "sOut": hx(n["s_out"]), "cOut": hx(n["c_out"]),
                      "recipient": hx(recipient), "token": hx(n["token"]), "amount": str(amount), "denomIndex": 0})
    os.makedirs(out, exist_ok=True)
    write(os.path.join(out, "burns.json"), {"label": label, "config_id": hx(ident["configId"]),
          "burn_count": len(burns), "burn_root": hx(tree_root(leaves)), "burns": burns})
    write(os.path.join(out, "burn-notes.json"), notes)
    return {"burn_root": hx(tree_root(leaves)), "burn_count": len(burns), "burns": burns}


def cmd_native_burns(a) -> None:
    """Two burns of a labelled test wallet, computed natively: the fork rehearsal's burn tree. NOT from LEZ blocks."""
    got = wallet_burns(identity(a.identity), REHEARSAL_SEED_TEXT, unhex(a.recipient, 20), int(a.amount), [0, 1],
                       ["public", "private"], "STAND-IN (native test wallet, not a LEZ block)",
                       "STAND-IN: two burns of a labelled test wallet computed natively (fork rehearsal), not from "
                       "LEZ blocks", a.out)
    print(json.dumps({"burn_root": got["burn_root"], "burn_count": got["burn_count"]}))


def cmd_wallet_burns(a) -> None:
    """The burns a wallet made on LEZ, from the seed text its driver was given, in accumulator order. `--check`: the
    bridge driver's record of the notes it sent ({step: {r, c_out, ..}}) must hold exactly these."""
    indices = [int(x) for x in a.indices.split(",")]
    kinds = a.kinds.split(",")
    if len(indices) != len(kinds):
        raise SystemExit("--indices and --kinds must have the same length")
    got = wallet_burns(identity(a.identity), a.seed_text, unhex(a.recipient, 20), int(a.amount), indices, kinds,
                       "the wallet's note (recomputed from its seed)",
                       "the wallet's burns recomputed from its seed, in the order given (to be compared with the "
                       "node's accumulator)", a.out)
    if a.check:
        sent = json.load(open(a.check))
        sent = sent.get("notes", sent)
        want = {(unhex(n["r"], 32), unhex(n["c_out"], 32)) for n in sent.values()}
        have = {(unhex(b["r"], 32), unhex(b["c_out"], 32)) for b in got["burns"]}
        if want != have:
            raise SystemExit("the notes recomputed from the seed are not the notes the bridge driver recorded")
    print(json.dumps({"burn_root": got["burn_root"], "burn_count": got["burn_count"], "checked": bool(a.check)}))


def cmd_burns_from_notes(a) -> None:
    """burns.json from the LEZ wallet's own burn notes, for an accumulator that holds exactly these burns in this
    order (INTERFACE.md): `checkpoint --layout` then compares the root and the count with the node's."""
    ident = identity(a.identity)
    notes = json.load(open(a.notes))
    burns, leaves = [], []
    for k, n in enumerate(notes):
        amount = n["amount"] if str(n["amount"]).startswith("0x") else hx(word(int(n["amount"])))
        leaf = burn_leaf(ident["configId"], k, unhex(n["token"], 20), unhex(amount, 32), unhex(n["cOut"], 32),
                         unhex(n["r"], 32))
        leaves.append(leaf)
        burns.append({"burn_index": k, "block": n.get("block"), "origin": n.get("origin", "the LEZ wallet's note"),
                      "r": n["r"], "token": n["token"], "amount": amount, "c_out": n["cOut"], "leaf": hx(leaf)})
    write(a.out, {"label": "the burns of the LEZ wallet's notes, in the order given (to be compared with the node's "
                           "accumulator)", "config_id": hx(ident["configId"]), "burn_count": len(burns),
                  "burn_root": hx(tree_root(leaves)), "burns": burns})
    print(json.dumps({"burn_root": hx(tree_root(leaves)), "burn_count": len(burns)}))


def write(path: str, value) -> None:
    with open(path, "w") as f:
        json.dump(value, f, indent=1)
        f.write("\n")


def cmd_checkpoint(a) -> None:
    ident = identity(a.identity)
    burns = json.load(open(a.burns))
    if unhex(burns["config_id"], 32) != ident["configId"]:
        raise SystemExit("burns.json is for another configuration")
    leaves = []
    for k, b in enumerate(burns["burns"]):
        if int(b["burn_index"]) != k:
            raise SystemExit(f"burn {k}: burn_index")
        leaf = burn_leaf(ident["configId"], k, unhex(b["token"], 20), unhex(b["amount"], 32), unhex(b["c_out"], 32),
                         unhex(b["r"], 32))
        if "leaf" in b and unhex(b["leaf"], 32) != leaf:
            raise SystemExit(f"burn {k}: the recomputed leaf is not the listed one")
        leaves.append(leaf)
    root, frontier, count = tree_root(leaves), tree_frontier(leaves), len(leaves)
    if root_from_frontier(count, frontier) != root or ("burn_root" in burns and unhex(burns["burn_root"], 32) != root):
        raise SystemExit("the burn root recomputed from the leaves is not the listed one")
    imported, lez = a.imported, Lez(a.rpc)
    layout = json.load(open(a.layout))
    chain, read_at = read_state(lez, layout)
    if chain["burn_count"] != count:
        raise SystemExit(f"the node's accumulator holds {chain['burn_count']} burns, burns.json lists {count}")
    if chain["burn_root"] is not None and chain["burn_root"] != root:
        raise SystemExit("the node's burn root is not the root of the listed burns")
    if chain["burn_frontier"] is not None and chain["burn_frontier"] != frontier:
        raise SystemExit("the node's burn frontier is not the frontier of the listed burns")
    if chain["burn_root"] is None and chain["burn_frontier"] is None:
        raise SystemExit("the layout names neither burn_root nor burn_frontier")
    if chain["config_id"] is not None and chain["config_id"] != ident["configId"]:
        raise SystemExit("the node's bridge configuration is not the vault's lezConfigId")
    if chain["imported_leaf_count"] is not None:
        if imported is not None and imported != chain["imported_leaf_count"]:
            raise SystemExit("--imported differs from the node's imported leaf count")
        imported = chain["imported_leaf_count"]
    if a.deposit_root is not None:
        if chain["imported_root"] is None:
            raise SystemExit("--deposit-root given, but the layout names no imported_root")
        if chain["imported_root"] != unhex(a.deposit_root, 32):
            raise SystemExit("the deposit root LEZ imported is not the vault's")
    # The tip: the block named, or the newest the node reports Finalized; a block not Finalized is refused. The state
    # was read after block `read_at`: a tip elsewhere is taken only if no block between the two can have changed it.
    tip, between = final_tip(lez, a.block), []
    if tip["id"] == read_at["id"]:
        if tip["hash"] != read_at["hash"]:
            raise SystemExit(f"block {tip['id']} changed on the node after the accumulator was read")
    else:
        if not layout.get("changes_only_in_root_calls"):
            raise SystemExit(f"the accumulator was read after block {read_at['id']}, the tip is block {tip['id']}, and "
                             "the layout does not say the bridge changes it only in root calls "
                             "(changes_only_in_root_calls): only a tip at the read head is attested")
        program, accounts = bridge_watch(layout)
        low, high = (tip, read_at) if tip["id"] < read_at["id"] else (read_at, tip)
        between = no_bridge_transaction(blocks_between(lez, low, high), program, accounts)
    source = {"kind": "LEZ node", "rpc": a.rpc, "tip": tip_record(tip), "final_on_node": tip["status"] == "Finalized",
              "accumulator_checked_against_node": True, "accumulator_read_at_head": read_at["id"],
              "accumulator_read_at_hash": hx(read_at["hash"]),
              "blocks_between_tip_and_read": between,
              "imported_root_is_the_vaults": a.deposit_root is not None}
    if between:
        source["why_the_read_is_the_tips"] = (
            "no transaction in the blocks between the tip and the read names the bridge program or an account of the "
            "accumulator, every one of them is read in full, and the bridge changes the accumulator only in a root "
            "call: the accumulator after the tip is the one read (lez-v03-return/README.md)")
    tip_id, tip_hash = tip["id"], tip["hash"]
    if imported is None:
        raise SystemExit("the imported leaf count is unknown: --imported, or imported_leaf_count in the layout")
    keys = signer_words(a.signers.split(","))
    if not 1 <= a.threshold <= len(keys):
        raise SystemExit("threshold")
    new_state = lez_state(tip_id, tip_hash, key_set_hash(keys), bytes(32), count, frontier, imported, False)
    c = {"chainId": ident["chainId"], "vault": hx(ident["vault"]), "lezConfigId": hx(ident["configId"]),
         "nMin": len(keys), "tMin": a.threshold, "parentState": hx(unhex(a.parent_state, 32)), "newState": hx(new_state),
         "burnRoot": hx(root), "burnCount": count, "importedLeafCount": imported, "sunset": False,
         "echoNonce": hx(bytes(32)), "tipId": tip_id, "tipHash": hx(tip_hash), "n": len(keys), "t": a.threshold}
    journal = checkpoint_journal(c)
    write(a.out, {"what": "STAND-IN checkpoint: the vault's journal for a burn root that stand-in keys attest; no "
                          "zero-knowledge proof of LEZ blocks stands behind it", "checkpoint": c,
                  "journal": hx(journal), "journalDigest": hx(sha256(journal)),
                  "signers": [hx(k[12:]) for k in keys], "threshold": a.threshold, "source": source})
    print(json.dumps({"burnRoot": hx(root), "burnCount": count, "tipId": tip_id, "newState": hx(new_state),
                      "journalDigest": hx(sha256(journal)), "final_on_node": source["final_on_node"],
                      "accumulator_read_at_head": read_at["id"], "blocks_between": len(between)}))


def cmd_layout(a) -> None:
    program = unhex(a.program, 32) if a.program.startswith("0x") else b58decode(a.program)
    layout = bridge_layout(program, unhex(a.config_id, 32), a.imported)
    write(a.out, layout)
    print(json.dumps({"burns_account": layout["account"],
                      "root_account": layout.get("imported_leaf_count", {}).get("account")}))


def cmd_sign(a) -> None:
    cp = json.load(open(a.checkpoint))
    c = cp["checkpoint"]
    src = cp.get("source") or {}
    if not (src.get("kind") == "LEZ node" and src.get("final_on_node") is True
            and src.get("accumulator_checked_against_node") is True):
        raise SystemExit("the checkpoint's source does not record both checks (its tip Finalized on the LEZ node, the "
                         "bridge's accumulator on that node equal to the burns): nothing is signed")
    if int(src["tip"]["id"]) != int(c["tipId"]) or src["tip"]["hash"].lower() != c["tipHash"].lower():
        raise SystemExit("the checkpoint's tip is not the tip its source checked: nothing is signed")
    journal = checkpoint_journal(c)
    if hx(journal) != cp["journal"]:
        raise SystemExit("the journal in the file is not the encoding of its checkpoint")
    verifier = unhex(a.verifier, 20)
    digest = attestation_digest(c["chainId"], verifier, sha256(journal))
    keys = load_keys(a.keys)
    allowed = {unhex(s, 20) for s in cp["signers"]}
    mine = [(addr, key) for addr, key in keys if addr in allowed]
    count = a.count if a.count is not None else cp["threshold"]
    if count > len(mine):
        raise SystemExit(f"{len(mine)} of the checkpoint's signers have a key in {a.keys}; {count} asked")
    chosen = mine[:count]
    seal = b"".join(sign(key, digest) for _, key in chosen)
    for i, (addr, _) in enumerate(chosen):
        if recover(digest, seal[65 * i:65 * i + 65]) != addr:
            raise SystemExit("a signature does not recover to its key")
    write(a.out, {"what": cp["what"], "chainId": c["chainId"], "vault": c["vault"], "verifier": hx(verifier),
                  "standInImageId": hx(STAND_IN_IMAGE_ID), "checkpoint": c, "journal": cp["journal"],
                  "journalDigest": cp["journalDigest"], "attestationDigest": hx(digest), "seal": hx(seal),
                  "signedBy": [hx(addr) for addr, _ in chosen], "threshold": cp["threshold"],
                  "signers": cp["signers"], "source": cp["source"]})
    print(json.dumps({"attestationDigest": hx(digest), "signedBy": [hx(addr) for addr, _ in chosen],
                      "seal_bytes": len(seal)}))


def seal_signers(candidate: dict) -> list:
    journal = unhex(candidate["journal"])
    digest = attestation_digest(int(candidate["chainId"]), unhex(candidate["verifier"], 20), sha256(journal))
    seal = unhex(candidate["seal"])
    if len(seal) % 65:
        raise SystemExit("the seal is not a whole number of signatures")
    return [recover(digest, seal[i:i + 65]) for i in range(0, len(seal), 65)]


def cmd_check_seal(a) -> None:
    cand = json.load(open(a.candidate))
    got = seal_signers(cand)
    allowed = {unhex(s, 20) for s in cand["signers"]}
    ok = (len(got) >= cand["threshold"] and all(g in allowed for g in got) and got == sorted(set(got)))
    print(json.dumps({"recovered": [hx(g) for g in got], "threshold": cand["threshold"], "accepted": ok}))
    sys.exit(0 if ok else 1)


def cmd_selftest(_a) -> None:
    assert keccak256(b"").hex() == "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    assert keccak256(b"abc").hex() == "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
    # one byte of padding (135 bytes) and a full padding block (136 bytes): the Solidity type hashes below
    assert hx(DOMAIN_TYPEHASH) == "0x8b73c3c69bb8fe3d512ecc4cf759cc79239f7b179b0ffacaa9a75d522b39400f"
    assert keccak256(b"1").hex() == "c89efdaa54c0f20c7adf612882df0950f5a951637e0307cdcb4c672f298b8bc6"
    assert hx(address_of_key(1)) == "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
    digest = sha256(b"selftest")
    for key in (1, 2, 0xC0FFEE, N - 1):
        sig = sign(key, digest)
        assert recover(digest, sig) == address_of_key(key) and int.from_bytes(sig[32:64], "big") <= N // 2
    assert b58encode(b58decode("6iArKUXxhUJqS7kCaPNhwMWt3ro71PDyBj7jwAyE2VQV")) == \
        "6iArKUXxhUJqS7kCaPNhwMWt3ro71PDyBj7jwAyE2VQV" and b58encode(bytes(32)) == "1" * 32
    assert hx(CHECKPOINT_DOMAIN) == "0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7"
    assert hx(RELEASE_DOMAIN) == "0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51"
    assert hx(BURN_TAG) == "0x93474c5ad2f57f5fc610ad22e3f6d50e51b98a4c0eb611dde536a7317c0e8cf2"
    leaves = [sha256(bytes([i])) for i in range(5)]
    for k in range(6):
        assert root_from_frontier(k, tree_frontier(leaves[:k])) == tree_root(leaves[:k])
    print("selftest: ok")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    p = sub.add_parser("keygen"); p.add_argument("--dir", required=True); p.add_argument("--n", type=int, default=3)
    p = sub.add_parser("signers"); p.add_argument("--dir", required=True)
    p = sub.add_parser("tip"); p.add_argument("--rpc", required=True); p.add_argument("--block", type=int)
    p = sub.add_parser("genesis"); p.add_argument("--rpc", required=True); p.add_argument("--signers", required=True)
    p.add_argument("--block", type=int)
    p = sub.add_parser("native-burns"); p.add_argument("--identity", required=True)
    p.add_argument("--recipient", required=True); p.add_argument("--amount", default="1000000000000000")
    p.add_argument("--out", required=True)
    p = sub.add_parser("wallet-burns"); p.add_argument("--identity", required=True)
    p.add_argument("--seed-text", required=True); p.add_argument("--recipient", required=True)
    p.add_argument("--amount", default="1000000000000000"); p.add_argument("--indices", default="0,1")
    p.add_argument("--kinds", default="public,private"); p.add_argument("--check"); p.add_argument("--out", required=True)
    p = sub.add_parser("burns-from-notes"); p.add_argument("--identity", required=True)
    p.add_argument("--notes", required=True); p.add_argument("--out", required=True)
    p = sub.add_parser("checkpoint"); p.add_argument("--identity", required=True); p.add_argument("--burns", required=True)
    p.add_argument("--parent-state", required=True); p.add_argument("--signers", required=True)
    p.add_argument("--threshold", type=int, required=True); p.add_argument("--rpc", required=True)
    p.add_argument("--layout", required=True); p.add_argument("--block", type=int); p.add_argument("--imported", type=int)
    p.add_argument("--deposit-root"); p.add_argument("--out", required=True)
    p = sub.add_parser("layout"); p.add_argument("--program", required=True); p.add_argument("--config-id", required=True)
    p.add_argument("--imported", type=int); p.add_argument("--out", required=True)
    p = sub.add_parser("sign"); p.add_argument("--checkpoint", required=True); p.add_argument("--verifier", required=True)
    p.add_argument("--keys", required=True); p.add_argument("--count", type=int); p.add_argument("--out", required=True)
    p = sub.add_parser("check-seal"); p.add_argument("--candidate", required=True)
    a = ap.parse_args()
    {"selftest": cmd_selftest, "keygen": cmd_keygen, "signers": cmd_signers, "tip": cmd_tip, "genesis": cmd_genesis,
     "native-burns": cmd_native_burns, "wallet-burns": cmd_wallet_burns, "burns-from-notes": cmd_burns_from_notes,
     "checkpoint": cmd_checkpoint,
     "layout": cmd_layout, "sign": cmd_sign,
     "check-seal": cmd_check_seal}[a.cmd](a)


if __name__ == "__main__":
    main()
