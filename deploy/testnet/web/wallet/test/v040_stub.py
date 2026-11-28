#!/usr/bin/env python3
"""A small, stateful LEZ v0.4.0 sequencer stand-in for the browser wallet test.

    python3 test/v040_stub.py --port 18700 --log OUT/wires.jsonl

It binds only to loopback and uses only the Python standard library.  The funded payer is account /0 of BIP-39's public
all-`abandon` test phrase.  sendTransaction logs every wire, decodes a complete public transaction before answering its
hash, and applies a decodable native transfer to the in-memory balances and nonces.  Nothing leaves this process.
"""
import argparse
import base64
import binascii
import hashlib
import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# Computed from the public phrase with ../lez040.js (the page test recomputes it rather than trusting this constant).
PAYER = "E6Mb7gptmMnFDmL7r4fxeAvYwcWf6hpBFDVi5xUd9ddC"
INITIAL_BALANCE = 1_000_000_000_000
B58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58encode(raw):
    """AccountId's ordinary Bitcoin-base58 representation (no checksum)."""
    zeroes = len(raw) - len(raw.lstrip(b"\0"))
    n = int.from_bytes(raw, "big")
    chars = []
    while n:
        n, digit = divmod(n, 58)
        chars.append(B58_ALPHABET[digit])
    return "1" * zeroes + "".join(reversed(chars))


class DecodeError(ValueError):
    pass


class Reader:
    """The v0.4.0 Borsh primitives needed by a public transaction."""
    def __init__(self, raw):
        self.raw, self.offset = raw, 0

    def take(self, n):
        end = self.offset + n
        if n < 0 or end > len(self.raw):
            raise DecodeError("truncated transaction")
        out = self.raw[self.offset:end]
        self.offset = end
        return out

    def uint(self, n):
        return int.from_bytes(self.take(n), "little")

    def u8(self):
        return self.uint(1)

    def u32(self):
        return self.uint(4)

    def u64(self):
        return self.uint(8)

    def u128(self):
        return self.uint(16)

    def byte_vec(self):
        return self.take(self.u32())

    def vec(self, read):
        # Do not permit a malicious count to turn into an unbounded loop: each item consumes at least one byte here.
        count = self.u32()
        if count > len(self.raw) - self.offset:
            raise DecodeError("impossible vector length")
        return [read() for _ in range(count)]

    def actor(self):
        return {"account": self.take(32), "program": self.take(32)}

    def end(self):
        return self.offset == len(self.raw)


def decode_public(wire):
    """Decode every field of a public LeeTransaction and return its hash and native transfer, if any.

    This deliberately does not pretend to execute programs or validate cryptography.  It establishes the complete wire
    layout, identifies an exact native Transfer, and lets the caller update only that simple state transition.
    """
    try:
        raw = base64.b64decode(wire, validate=True)
    except (binascii.Error, ValueError) as e:
        raise DecodeError("wire is not base64") from e
    r = Reader(raw)
    if r.u8() != 0:
        raise DecodeError("not a public transaction")
    tx_start = r.offset
    actors = r.vec(lambda: (r.actor(), r.u8()))           # actor map and authorities (1 = Signer)
    r.vec(lambda: r.take(32))                             # authorized accounts
    r.vec(r.u64)                                           # promotions
    r.vec(r.actor)                                         # private signers
    target = r.actor()
    data = r.byte_vec()
    nonces = r.vec(lambda: {"account": r.take(32), "nonce": r.u128()})
    has_fee = r.u8()
    if has_fee not in (0, 1):
        raise DecodeError("invalid fee option")
    if has_fee:
        fee = {"payer": r.take(32), "gas_limit": r.u64(), "tip": r.u64(), "max_fee": r.u128()}
    else:
        fee = None

    def evidence():
        kind = r.u8()
        if kind == 0:
            return {"key": r.take(33)}
        if kind == 1:
            return {"pda": r.actor()}                    # two 32-byte fields: program and seed
        raise DecodeError("unknown admission evidence")

    r.vec(evidence)
    message_end = r.offset

    def witness():
        auth_data = r.byte_vec()
        state = r.take(33)
        if state[0] != 0:
            raise DecodeError("unknown signing key type")
        return {"auth_data": auth_data, "next": state}

    witnesses = r.vec(witness)
    if not r.end():
        raise DecodeError("trailing bytes after a public transaction")
    tx_raw = raw[tx_start:]
    transfer = None
    if target["program"] == bytes(32) and len(data) == 49 and data[0] == 0:
        transfer = {
            "from": target["account"],
            "to": data[1:33],
            "amount": int.from_bytes(data[33:49], "little"),
            "message": raw[tx_start:message_end],
            "nonces": nonces,
            "fee": fee,
            "witnesses": witnesses,
            "actors": actors,
        }
    return {"hash": hashlib.sha256(tx_raw).hexdigest(), "transfer": transfer}


class State:
    def __init__(self, log_path):
        self.lock = threading.Lock()
        self.height = 1
        self.accounts = {PAYER: {"balance": INITIAL_BALANCE, "nonce": 0}}
        self.transactions = {}
        self.log_path = os.path.abspath(log_path)
        os.makedirs(os.path.dirname(self.log_path), exist_ok=True)
        self.log = open(self.log_path, "a", encoding="utf-8")

    def account(self, account_id):
        value = self.accounts.get(account_id)
        if value is None:
            return None                         # v0.4.0: an unknown account is absent
        return {"account_id": account_id, "nonce": value["nonce"], "balance": value["balance"],
                "data": {"actor_states": {}}}

    def send(self, wire):
        """Log first, then refuse a wire whose full public transaction cannot be decoded."""
        with self.lock:
            item = {"height": self.height + 1, "wire": wire}
            try:
                decoded = decode_public(wire)
            except (DecodeError, IndexError, TypeError) as e:
                item["error"] = str(e)
                self._write(item)
                raise DecodeError(str(e)) from e
            tx_hash = decoded["hash"]
            item["hash"] = tx_hash
            transfer = decoded["transfer"]
            if transfer is not None:
                from_id, to_id = b58encode(transfer["from"]), b58encode(transfer["to"])
                sender = self.accounts.get(from_id)
                sender_nonce = next((n["nonce"] for n in transfer["nonces"] if n["account"] == transfer["from"]), None)
                error = None
                signers = [actor["account"] for actor, authority in transfer["actors"] if authority == 1]
                if any(actor["program"] != bytes(32) for actor, _ in transfer["actors"]):
                    error = "a native transfer declares only native-token actors"
                elif signers != [transfer["from"]] or len(transfer["witnesses"]) != 1:
                    error = "a native transfer is signed by its payer alone, with one witness"
                elif sender is None or sender_nonce is None or sender_nonce != sender["nonce"]:
                    error = "native transfer has no current payer nonce"
                elif sender["balance"] < transfer["amount"]:
                    error = "native transfer exceeds the payer balance"
                if error:
                    item["error"] = error
                    self._write(item)
                    raise DecodeError(error)
                sender["balance"] -= transfer["amount"]
                sender["nonce"] += 1
                recipient = self.accounts.setdefault(to_id, {"balance": 0, "nonce": 0})
                recipient["balance"] += transfer["amount"]
                item["applied"] = {"from": from_id, "to": to_id, "amount": str(transfer["amount"])}
            self.height += 1
            item["height"] = self.height
            self.transactions[tx_hash] = {"block": self.height, "receipt": {
                "success": True,
                "fee": {"gas_exec": 0, "gas_stor": 0, "fee_base": 0, "tip": 0, "refund": 0},
            }}
            self._write(item)
            return tx_hash

    def _write(self, item):
        self.log.write(json.dumps(item, separators=(",", ":")) + "\n")
        self.log.flush()


class Server(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True


class Handler(BaseHTTPRequestHandler):
    state = None

    def log_message(self, fmt, *args):
        pass

    def reply(self, body, status=200):
        raw = json.dumps(body, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(raw)

    def do_POST(self):
        if self.path != "/rpc":
            return self.reply({"error": "not found"}, 404)
        request_id = None
        try:
            length = int(self.headers.get("Content-Length") or 0)
            request = json.loads(self.rfile.read(length))
            if not isinstance(request, dict):
                raise ValueError("one JSON-RPC request object is required")
            request_id = request.get("id")
            method, params = request.get("method"), request.get("params", [])
            if not isinstance(params, list):
                raise ValueError("params must be an array")
            if method == "sendTransaction":
                result = self.state.send(params[0])
                return self.reply({"jsonrpc": "2.0", "id": request_id, "result": result})
            with self.state.lock:
                height = self.state.height
                if method == "getSequencerStatus":
                    result = {"state": "online", "height": height, "isOurTurn": True}
                elif method == "getLastBlockId":
                    result = height
                elif method == "getFeeState":
                    result = {"base_fee_exec": 1, "base_fee_stor": 1}
                elif method == "getAccount":
                    result = self.state.account(params[0])
                elif method == "getAccountBalance":
                    value = self.state.accounts.get(params[0])
                    result = value["balance"] if value is not None else None
                elif method == "getAccountsNonces":
                    result = [self.state.accounts.get(account_id, {"nonce": 0})["nonce"] for account_id in params[0]]
                elif method == "getTransaction":
                    found = self.state.transactions.get(params[0].removeprefix("0x"))
                    result = [params[0], found["block"]] if found else None
                elif method == "getTransactionReceipt":
                    found = self.state.transactions.get(params[0].removeprefix("0x"))
                    result = found["receipt"] if found else None
                elif method == "getProgramIds":
                    result = {}                         # no optional programs; Settings can still print the LEZ version
                else:
                    return self.reply({"jsonrpc": "2.0", "id": request_id,
                                       "error": {"code": -32601, "message": "Method not found"}})
            return self.reply({"jsonrpc": "2.0", "id": request_id, "result": result})
        except DecodeError as e:
            return self.reply({"jsonrpc": "2.0", "id": request_id,
                               "error": {"code": -32602, "message": f"Invalid transaction: {e}"}})
        except (ValueError, TypeError, KeyError, IndexError, json.JSONDecodeError) as e:
            return self.reply({"jsonrpc": "2.0", "id": request_id,
                               "error": {"code": -32602, "message": f"Invalid params: {e}"}})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--log", required=True)
    args = parser.parse_args()
    Handler.state = State(args.log)
    print(f"LEZ v0.4.0 stub on http://127.0.0.1:{args.port}/rpc; payer {PAYER}", flush=True)
    Server(("127.0.0.1", args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
