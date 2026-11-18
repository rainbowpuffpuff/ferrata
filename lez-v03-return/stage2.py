#!/usr/bin/env python3
"""How far a real checkpoint gets on the public LEZ testnet's own data (lez-v03-return/STAGE2.md). Standard library
only; reads only.

  stage2.py check [FIXTURES]     offline, over the committed fixtures: real LEZ v0.3.1 blocks (the node's RPC) and the
                                 transactions that inscribed them on the public Logos Blockchain testnet (its
                                 explorer's API). Everything a checkpoint guest checks about an inscription is
                                 recomputed natively: the Mantle transaction hash, the signer's Ed25519 signature
                                 over it, the message chain, the channel's key set, the block's hash and link, and
                                 every transaction of the block parsed field by field.
  stage2.py fetch FIXTURES       reads the fixtures again from the two public endpoints (GET and `getBlock` only)
  stage2.py depth                how many Logos Blockchain blocks lie above the inscription of the newest block the
                                 LEZ node reports Finalized, and above the next one

Layouts: Mantle transactions as `checkpoint/crates/checkpoint-core/src/mantle.rs` reads them (logos-blockchain
d480efc6); LEZ v0.3.1 blocks and transactions from the tag's source (STAGE2.md gives file and line).
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import struct
import sys
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import standin as s  # noqa: E402

LEZ_RPC = "https://testnet.lez.logos.co"
EXPLORER = "https://testnet.blockchain.logos.co/web/explorer/api/v1"
CHANNEL = "01" * 32
BLOCKS = (83000, 83001, 83002, 83003)

# ---- Ed25519 verification (RFC 8032), for the inscriptions' signatures --------------------------------------------

Q = 2 ** 255 - 19
L = 2 ** 252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, -1, Q) % Q
I = pow(2, (Q - 1) // 4, Q)


def _recover_x(y: int, sign: int) -> int | None:
    xx = (y * y - 1) * pow(D * y * y + 1, -1, Q) % Q
    x = pow(xx, (Q + 3) // 8, Q)
    if (x * x - xx) % Q:
        x = x * I % Q
    if (x * x - xx) % Q:
        return None
    if x == 0 and sign:
        return None
    return Q - x if (x & 1) != sign else x


def _point(raw: bytes):
    y = int.from_bytes(raw, "little")
    sign, y = y >> 255, y & ((1 << 255) - 1)
    if y >= Q:
        return None
    x = _recover_x(y, sign)
    return None if x is None else (x, y, 1, x * y % Q)


def _padd(p, q):
    a, b = (p[1] - p[0]) * (q[1] - q[0]) % Q, (p[1] + p[0]) * (q[1] + q[0]) % Q
    c, d = 2 * p[3] * q[3] * D % Q, 2 * p[2] * q[2] % Q
    e, f, g, h = b - a, d - c, d + c, b + a
    return e * f % Q, g * h % Q, f * g % Q, e * h % Q


def _pmul(k: int, p):
    out = (0, 1, 1, 0)
    while k:
        if k & 1:
            out = _padd(out, p)
        p = _padd(p, p)
        k >>= 1
    return out


def _peq(p, q) -> bool:
    return (p[0] * q[2] - q[0] * p[2]) % Q == 0 and (p[1] * q[2] - q[1] * p[2]) % Q == 0


_BY = 4 * pow(5, -1, Q) % Q
BASE = (_recover_x(_BY, 0), _BY, 1, _recover_x(_BY, 0) * _BY % Q)


def ed25519_verify(key: bytes, message: bytes, signature: bytes) -> bool:
    """[S]B == R + [SHA-512(R ‖ A ‖ M)]A, with S below the group order and canonical encodings of A and R."""
    if len(key) != 32 or len(signature) != 64:
        return False
    a, r = _point(key), _point(signature[:32])
    sig_s = int.from_bytes(signature[32:], "little")
    if a is None or r is None or sig_s >= L:
        return False
    k = int.from_bytes(hashlib.sha512(signature[:32] + key + message).digest(), "little") % L
    return _peq(_pmul(sig_s, BASE), _padd(r, _pmul(k, a)))


# ---- Mantle transactions as the explorer shows them, in d480efc6's canonical encoding -----------------------------

def blake2b256(*parts: bytes) -> bytes:
    h = hashlib.blake2b(digest_size=32)
    for p in parts:
        h.update(p)
    return h.digest()


def encode_op(op: dict) -> bytes:
    c = op["content"]
    if c["type"] == "ChannelInscribe":  # opcode 0x11: channel ‖ u32 len ‖ inscription ‖ parent MsgId ‖ signer
        ins = bytes.fromhex(c["inscription"])
        return (b"\x11" + bytes.fromhex(c["channel_id"]) + struct.pack("<I", len(ins)) + ins
                + bytes.fromhex(c["parent"]) + bytes.fromhex(c["signer"]))
    if c["type"] == "LedgerTransfer":  # opcode 0x00: u8 n ‖ NoteId·n ‖ u8 m ‖ (value u64 ‖ pk)·m
        return (b"\x00" + bytes([len(c["inputs"])]) + b"".join(bytes.fromhex(i) for i in c["inputs"])
                + bytes([len(c["outputs"])])
                + b"".join(struct.pack("<Q", o["value"]) + bytes.fromhex(o["public_key"]) for o in c["outputs"]))
    raise ValueError(f"an operation this reader does not encode: {c['type']}")


def tx_hash(tx: dict) -> bytes:
    ops = [encode_op(op) for op in tx["operations"]]
    return blake2b256(b"MANTLE_TXHASH_V1", bytes([len(ops)]), *ops)


def inscription_of(tx: dict) -> dict:
    return next(op for op in tx["operations"] if op["content"]["type"] == "ChannelInscribe")


def msg_id(content: dict) -> bytes:
    ins = bytes.fromhex(content["inscription"])
    return blake2b256(bytes.fromhex(content["channel_id"]), struct.pack("<I", len(ins)), ins,
                      bytes.fromhex(content["parent"]), bytes.fromhex(content["signer"]))


# ---- LEZ v0.3.1 blocks: every transaction, field by field ----------------------------------------------------------

# The parser lives in standin.py (the attestor reads the blocks between its tip and its read with it).
Reader = s.Reader
parse_block = s.parse_block


# ---- the offline check ----------------------------------------------------------------------------------------------

def jl(path: str):
    with open(path) as f:
        return json.load(f)


def check(fixtures: str) -> list:
    """Returns one line per fact established; raises on the first that does not hold."""
    out = []
    config = jl(os.path.join(fixtures, "l1-channel-config.json"))["content"]
    keys = config["keys"]
    assert config["channel"] == CHANNEL and config["parent"] == "00" * 32
    out.append(f"channel configuration (the channel's first operation): {len(keys)} accredited keys, posting_timeframe "
               f"{config['posting_timeframe']}, configuration_threshold {config['configuration_threshold']}; "
               f"t(n) = {max(1, min(-(-2 * len(keys) // 3), len(keys) - 1))} of {len(keys)}")
    previous, previous_block = None, None
    for block_id in BLOCKS:
        tx = jl(os.path.join(fixtures, f"l1-inscription-{block_id}.json"))
        op = inscription_of(tx)
        c = op["content"]
        payload = bytes.fromhex(c["inscription"])
        # 1. the transaction hash, recomputed from the operations' fields
        digest = tx_hash(tx)
        assert digest.hex() == tx["hash"], f"{block_id}: the recomputed Mantle hash is not the explorer's"
        # 2. the signer is an accredited key and signed that hash
        assert c["signer"] in keys, f"{block_id}: the signer is not in the channel's key set"
        assert op["proof"]["type"] == "Ed25519"
        assert ed25519_verify(bytes.fromhex(c["signer"]), digest, bytes.fromhex(op["proof"]["signature"])), \
            f"{block_id}: the signature does not verify"
        tampered = bytearray(digest)
        tampered[0] ^= 1
        assert not ed25519_verify(bytes.fromhex(c["signer"]), bytes(tampered), bytes.fromhex(op["proof"]["signature"]))
        # 3. the message chain: this inscription's parent is the previous inscription's MsgId
        if previous is not None:
            assert c["parent"] == previous.hex(), f"{block_id}: the parent is not the previous inscription"
        previous = msg_id(c)
        # 4. the payload is the node's block: the same bytes but the status byte, hash recomputed, linked
        node = base64.b64decode(jl(os.path.join(fixtures, f"testnet-block-{block_id}.json"))["result"])
        assert payload[:-1] == node[:-1], f"{block_id}: the inscribed block is not the node's"
        assert payload[-1] == 0, "an inscribed block carries status Pending"
        block = s.decode_block(payload)
        assert block["id"] == block_id and block["recomputed_hash"] == block["hash"]
        if previous_block is not None:
            assert block["prev"] == previous_block, f"{block_id}: does not link to the previous block"
        previous_block = block["hash"]
        # 5. every transaction parsed
        txs = parse_block(payload)
        fees = sum(1 for t in txs if t["fee"])
        out.append(f"LEZ block {block_id}: inscribed in L1 transaction {tx['hash'][:16]}… by key {c['signer'][:8]}…; "
                   f"Mantle hash recomputed, Ed25519 signature verified, parent = the previous inscription's MsgId, "
                   f"payload = the node's block (status {s.STATUS[node[-1]]} there), block hash recomputed and "
                   f"linked; {len(payload)} bytes, {len(txs)} public transactions ({fees} with a fee, "
                   f"{len(txs) - fees} system), all bytes parsed")
    return out


# ---- the network (reads only) ---------------------------------------------------------------------------------------

def get(path: str):
    req = urllib.request.Request(EXPLORER + path, headers={"User-Agent": "curl/8"})
    with urllib.request.urlopen(req, timeout=60) as reply:
        return json.load(reply)


def channel_page(page: int, size: int = 50) -> list:
    return get(f"/channels/{CHANNEL}?page={page}&page-size={size}")["operations"]


def find_inscriptions(wanted: set) -> dict:
    """The channel operations that inscribe the wanted LEZ blocks, searched from the newest page back."""
    count = get(f"/channels/{CHANNEL}?page=0&page-size=1")["op_count"]
    found, page = {}, (count - 1) // 50
    while wanted - set(found) and page >= 0:
        for op in channel_page(page):
            c = op["content"]
            if c["type"] == "ChannelInscribe" and len(c["inscription"]) >= 16:
                block_id = struct.unpack_from("<Q", bytes.fromhex(c["inscription"][:16]))[0]
                if block_id in wanted:
                    found[block_id] = op
        page -= 1
    return found


def cmd_fetch(fixtures: str) -> None:
    os.makedirs(fixtures, exist_ok=True)
    first = channel_page(0, 1)[0]
    s.write(os.path.join(fixtures, "l1-channel-config.json"), first)
    lez = s.Lez(LEZ_RPC)
    ops = find_inscriptions(set(BLOCKS))
    for block_id in BLOCKS:
        s.write(os.path.join(fixtures, f"l1-inscription-{block_id}.json"), get("/transactions/" + ops[block_id]["transaction_hash"]))
        with open(os.path.join(fixtures, f"testnet-block-{block_id}.json"), "w") as f:
            json.dump({"jsonrpc": "2.0", "id": 1, "result": lez.call("getBlock", [block_id])}, f)
    print(f"wrote the channel's configuration and {len(BLOCKS)} inscriptions with their blocks to {fixtures}")


def cmd_depth() -> None:
    lez = s.Lez(LEZ_RPC)
    final = s.newest_final(lez)
    head = get("/blocks/list?page=0&page-size=1")["blocks"][0]
    ops = find_inscriptions({final["id"], final["id"] + 1})
    print(json.dumps({
        "lez_head": lez.head(), "lez_newest_final": final["id"], "l1_head_height": head["height"],
        "l1_head_slot": head["slot"],
        "final_block_inscribed_at": ops[final["id"]]["height"],
        "final_block_depth": head["height"] - ops[final["id"]]["height"],
        "next_block_inscribed_at": ops.get(final["id"] + 1, {}).get("height"),
        "next_block_depth": None if final["id"] + 1 not in ops else head["height"] - ops[final["id"] + 1]["height"]}))


def main() -> None:
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    if cmd == "check":
        for line in check(sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, "fixtures")):
            print("PASS", line)
    elif cmd == "fetch":
        cmd_fetch(sys.argv[2])
    elif cmd == "depth":
        cmd_depth()
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
