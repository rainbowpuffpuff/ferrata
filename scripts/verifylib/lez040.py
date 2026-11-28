"""LEZ v0.4.0's blocks, transactions and receipts, read as its sequencer's RPC serves them; the network's version.

A library beside verifylib/lez.py (LEZ v0.4.0-rc3's layouts, unchanged) for scripts/lez-read.py: the same reader,
hashes and polite read-only client, with v0.4.0's layouts (logos-execution-zone v0.4.0, a255b42c). What changed from
rc3 (deploy/testnet/web/wallet/lez040.js reads the same, both tested on vectors LEZ v0.4.0's own crates made:
deploy/testnet/web/wallet/test/fixtures/lez-v040-vectors.json):
- the public context: each actor carries an Authority byte (0 None, 1 Signer; LEZ #1144), and private signers follow
  the promotions; the fee declaration moved from the execution to the message, after the nonces (LEZ #1080);
- admission evidence names an account by its initial key state (33 bytes: key type 0 Schnorr, commitment); the witness
  set is a list of entries {auth_data: Vec<u8>, next: key state} (LEZ #1135): the signers are the nonce map's accounts;
- the privacy-preserving execution: Delivery {envelope, grants: Vec<Actor>, pda_seeds}, publications Clear | Sealed,
  ProvenFee {refund_to, tip, fallback {nullifier, root, commitment, ciphertext, boundary}};
- the block body: `receipts` (one TransactionReceipt {success, fee {gas_exec, gas_stor, fee_base, tip, refund}} per
  user transaction; the trailing fee and clock transactions have none) in place of `reverted` (LEZ #1107).
The block hash is computed as on rc3 (SHA-256 of the block prefix, the header without its hash, and the body).
Read-only: Rpc040 sends nothing but reads.
"""
from __future__ import annotations

import json
import urllib.error
import urllib.request

from verifylib.lez import BEDROCK_STATUS, DecodeError, Reader, Rpc, RpcError, for_public_pda, pad32, sha256

AUTHORITY = ("None", "Signer")
# v0.4.0's system programs (lee/state_machine/core/src/program/mod.rs, lez/programs/*/core, signing_key.rs;
# cross-zone messaging and its programs were removed, LEZ #1147). The upgrader is its own fixed account; each other
# program is at the system_upgrader's PDA for its name (LEZ #946).
SYSTEM_PROGRAMS = ("system_upgrader", "clock", "fee", "bridge", "sequencer_stake", "signature")
SYSTEM_UPGRADER = bytes([0xFD]) * 32


def system_program_id(name: str) -> bytes:
    """lee_core's well-known system program account: system_upgrader itself is 0xfd x 32; every other name uses
    `AccountId::from_system_program_name`, its PDA with seed SHA-256(`/LEE/SystemUpgrader/v1/Name` zero-padded to
    32 || the name zero-padded to 32)."""
    if name == "system_upgrader":
        return SYSTEM_UPGRADER
    return for_public_pda(SYSTEM_UPGRADER, sha256(pad32("/LEE/SystemUpgrader/v1/Name"), pad32(name)))
_BOUNDARY = ("PrivateToPublic", "PublicToPrivate", "EndPrivateSubtree", "EndPublicSubtree")


def _actor(r):
    return {"account_id": r.take(32), "program_account_id": r.take(32)}


def _key_state(r):
    t = r.u8()
    if t != 0:
        raise DecodeError(f"KeyType {t}")
    return bytes([t]) + r.take(32)


def _authority(r):
    t = r.u8()
    if t > 1:
        raise DecodeError(f"Authority tag {t}")
    return AUTHORITY[t]


def _public_context(r):
    return {"actors": r.vec(lambda r: {**_actor(r), "authority": _authority(r)}),
            "authorized_accounts": r.vec(lambda r: r.take(32)),
            "promotions": r.vec(lambda r: r.u64()),
            "private_signers": r.vec(_actor)}


def _root_call(r):
    return {"to": _actor(r), "message": r.bytes_vec()}


def _nonces(r):
    return r.vec(lambda r: {"account": r.take(32), "nonce": r.u128()})


def _fee_declaration(r):
    return {"payer": r.take(32), "gas_limit": r.u64(), "tip": r.u64(), "max_fee": r.u128()}


def _evidence(r):
    t = r.u8()
    if t == 0:
        return {"kind": "key", "initial_key": _key_state(r)}
    if t == 1:
        return {"kind": "pda", "program": r.take(32), "seed": r.take(32)}
    raise DecodeError(f"PublicAccountEvidence tag {t}")


def _entries(r):
    return r.vec(lambda r: {"auth_data": r.bytes_vec(), "next": _key_state(r)})


def _note(r):
    return {"ciphertext": r.bytes_vec(), "epk": r.bytes_vec()}


def _envelope(r):
    return {"from": _actor(r), "to": _actor(r), "message": r.bytes_vec()}


def _delivery(r):
    return {"envelope": _envelope(r), "grants": r.vec(_actor), "pda_seeds": r.vec(lambda r: r.take(32))}


def _boundary_step(r):
    t = r.u8()
    if t > 3:
        raise DecodeError(f"BoundaryStep tag {t}")
    return {"kind": _BOUNDARY[t], "delivery": _delivery(r)} if t < 2 else {"kind": _BOUNDARY[t]}


def _publication(r):
    t = r.u8()
    if t == 0:
        return {"kind": "Clear", "body": _envelope(r)}
    if t == 1:
        return {"kind": "Sealed", "commitment": r.take(32), "note": _note(r)}
    raise DecodeError(f"Publication tag {t}")


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


def _fallback(r):
    return {"nullifier": r.take(32), "root": r.take(32), "commitment": r.take(32), "ciphertext": r.bytes_vec(),
            "boundary": r.vec(_boundary_step)}


def _proven_fee(r):
    return {"refund_to": r.option(lambda r: r.take(32)), "tip": r.u64(), "fallback": r.option(_fallback)}


def _proven_execution(r):
    return {"boundary": r.vec(_boundary_step), "async_messages": r.vec(_publication),
            "recovery_bindings": r.vec(lambda r: {"address": r.take(32), "note": _note(r)}),
            "public_root": r.option(_root_call), "private_actions": r.vec(_private_action),
            "validity": {"blocks": _window(r), "timestamps": _window(r)},
            "program_image_claims": r.vec(_image_claim), "proven_fee": r.option(_proven_fee)}


def read_transaction(r: Reader) -> dict:
    tag_at = r.pos
    tag = r.u8()
    start = r.pos
    if tag == 0:
        tx = {"kind": "Public", "public_context": _public_context(r)}
        tx["root"] = _root_call(r)
        tx["nonces"] = _nonces(r)
        tx["fee"] = r.option(_fee_declaration)
        tx["admission_evidence"] = r.vec(_evidence)
        tx["entries"] = _entries(r)
    elif tag == 1:
        tx = {"kind": "PrivacyPreserving", "public_context": _public_context(r)}
        tx["execution"] = _proven_execution(r)
        tx["root"] = tx["execution"]["public_root"]
        tx["nonces"] = _nonces(r)
        tx["fee"] = r.option(_fee_declaration)
        tx["admission_evidence"] = r.vec(_evidence)
        tx["entries"] = _entries(r)
        tx["proof"] = r.bytes_vec()
    else:
        raise DecodeError(f"LeeTransaction tag {tag}")
    tx["size"] = r.pos - start
    tx["hash"] = sha256(r.buf[start:r.pos])
    tx["raw"] = r.buf[tag_at:r.pos]
    # From v0.4.0 a witness entry names no account: the signers are the nonce map's accounts, in its order.
    tx["signers"] = [n["account"] for n in tx["nonces"]]
    return tx


def decode_transaction(raw: bytes) -> dict:
    r = Reader(raw)
    tx = read_transaction(r)
    r.finish("LeeTransaction")
    return tx


def _receipt(r):
    s = r.u8()
    if s > 1:
        raise DecodeError(f"receipt success {s}")
    return {"success": s == 1, "gas_exec": r.u64(), "gas_stor": r.u64(), "fee_base": r.u128(), "tip": r.u128(),
            "refund": r.u128()}


def decode_block(raw: bytes) -> dict:
    """A v0.4.0 block as `getBlock` serves it. `reverted` lists the positions whose receipt failed, as rc3's list
    did; a transaction past the last receipt (the block's trailing fee and clock transactions) has none."""
    r = Reader(raw)
    header = {"block_id": r.u64(), "prev_block_hash": r.take(32), "hash": r.take(32), "timestamp": r.u64()}
    body_start = r.pos
    txs = r.vec(read_transaction)
    receipts = r.vec(_receipt)
    body_end = r.pos
    st = r.u8()
    if st > 2:
        raise DecodeError(f"bedrock_status {st}")
    r.finish("Block")
    if len(receipts) > len(txs):
        raise DecodeError(f"{len(receipts)} receipts for {len(txs)} transactions")
    computed = sha256(pad32("/LEE/v0.3/Message/Block/"), raw[0:40], raw[72:80], raw[body_start:body_end])
    return {"header": header, "transactions": txs, "receipts": receipts,
            "reverted": [i for i, x in enumerate(receipts) if not x["success"]], "bedrock_status": BEDROCK_STATUS[st],
            "size": len(raw), "computed_hash": computed, "hash_ok": computed == header["hash"]}


def outcome(block: dict, index: int) -> str:
    """'applied', 'reverted' (its fee charged), or 'system' (no receipt: the block's fee and clock transactions)."""
    if index >= len(block["receipts"]):
        return "system"
    return "applied" if block["receipts"][index]["success"] else "reverted"


class Rpc040(Rpc):
    """lez.Rpc with v0.4.0's two reads (getSequencerStatus, getTransactionReceipt); still nothing but reads."""

    ALLOWED = Rpc.ALLOWED | frozenset({"getSequencerStatus", "getTransactionReceipt"})


def classify(status: int, body: bytes) -> str:
    """The network's LEZ from its answer to getSequencerStatus: 'v0.4.0' when it serves a status (or answers v0.4.0's
    -31902, not online yet), 'before v0.4.0' on -32601 (method not found: an rc3 sequencer, or the test network's
    gateway, which answers a method it does not forward so, with HTTP 403). Anything else raises RpcError."""
    try:
        reply = json.loads(body)
    except ValueError as e:
        raise RpcError(f"getSequencerStatus: HTTP {status} and no JSON-RPC reply") from e
    err = reply.get("error") if isinstance(reply, dict) else None
    if isinstance(err, dict) and isinstance(err.get("code"), int):
        if err["code"] == -32601:
            return "before v0.4.0"
        if err["code"] == -31902:
            return "v0.4.0"
        raise RpcError(f"getSequencerStatus: error {err['code']} ({err.get('message')})")
    # v0.4.0's SequencerStatus is tagged by `state` (lez/sequencer/service/protocol/src/lib.rs); any other result (a
    # stand-in's null) says nothing of the version.
    result = reply.get("result") if isinstance(reply, dict) else None
    if 200 <= status < 300 and isinstance(result, dict) and result.get("state") in ("online", "bootstrapping"):
        return "v0.4.0"
    raise RpcError(f"getSequencerStatus: HTTP {status} without a sequencer status or an error")


def probe(url: str, timeout: float = 30.0, user_agent: str = "ferrata-verify/1") -> str:
    """Asks the endpoint for getSequencerStatus once (a read) and classifies the answer."""
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getSequencerStatus", "params": []}).encode()
    req = urllib.request.Request(url, data=body, headers={
        "Content-Type": "application/json", "Accept": "application/json", "User-Agent": user_agent})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return classify(resp.status, resp.read())
    except urllib.error.HTTPError as e:
        with e:
            return classify(e.code, e.read())
