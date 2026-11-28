#!/usr/bin/env python3
"""Serve the explorer over a replay of LEZ v0.4.0's own recorded standalone sequencer.

    python3 deploy/testnet/web/explorer/test/replay040.py [--port 18751] [--run RUN]

RUN is cli/results/v040-standalone-1 by default. Its block-N.json files are the base64 Borsh answers getBlock returned
in that run, and are served unchanged. Transactions and receipts are indexed from those same bytes, not from the page
or from a second fixture. Only loopback is bound and only reads are served. Every HTTP request and RPC method is logged
to stdout; this program writes nothing. The standard library is sufficient.
"""
import argparse
import base64
import hashlib
import http.server
import json
import mimetypes
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BUNDLE = HERE.parent
REPO = HERE.parents[4]
DEFAULT_RUN = REPO / "cli" / "results" / "v040-standalone-1"
CHANNEL_ID = "01" * 32

# A v0.4.0-native config, without Ferrata deployments or Ethereum providers for a page to contact.
CONFIG = {
    "name": "Ferrata LEZ v0.4.0 replay (offline recorded run)",
    "lez": "v0.4.0",
    "rpc": "/rpc",
    "public_rpc": "",
    "site": "",
    "wallet": "",
    "faucet": "",
    "faucet_api": "",
    "lez_explorer": "",
    "status_api": "status/status.json",
    "status_history": "status/history.json",
    "programs_json": "",
    "repo": "",
    "bedrock": {"anchored": False, "channel_id": CHANNEL_ID},
    "bridge": {},
    "programs": [],
    "known_images": {},
    "instances": [],
    "accounts": {},
    "beacon_apis": {},
    "execution_rpcs": {},
    "scan_blocks": 300,
    "feed_blocks": 300,
    "range_limit": 50,
    "native": {"symbol": "LGO", "decimals": 9},
    "faucet_path": "faucet",
}


class RpcError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


class Reader:
    """The small, checked subset of Borsh needed to walk a v0.4.0 block."""

    def __init__(self, raw):
        self.raw, self.pos = raw, 0

    def take(self, n):
        if n < 0 or self.pos + n > len(self.raw):
            raise ValueError("truncated Borsh")
        out = self.raw[self.pos:self.pos + n]
        self.pos += n
        return out

    def uint(self, n):
        return int.from_bytes(self.take(n), "little")

    def u8(self):
        return self.uint(1)

    def u32(self):
        return self.uint(4)

    def u64(self):
        return self.uint(8)

    def vec(self, item):
        return [item() for _ in range(self.u32())]

    def option(self, item):
        tag = self.u8()
        if tag == 0:
            return None
        if tag == 1:
            return item()
        raise ValueError(f"invalid Option tag {tag}")

    def bytes_vec(self):
        return self.take(self.u32())

    def actor(self):
        self.take(64)

    def key_state(self):
        if self.u8() != 0:
            raise ValueError("unknown v0.4.0 signing-key type")
        self.take(32)

    def root_call(self):
        self.actor()
        self.bytes_vec()

    def note(self):
        self.bytes_vec()
        self.bytes_vec()

    def envelope(self):
        self.actor()
        self.actor()
        self.bytes_vec()

    def delivery(self):
        self.envelope()
        self.vec(self.actor)
        self.vec(lambda: self.take(32))

    def boundary(self):
        def step():
            tag = self.u8()
            if tag > 3:
                raise ValueError("unknown BoundaryStep")
            if tag < 2:
                self.delivery()
        self.vec(step)

    def skip_transaction(self):
        """Return the full LeeTransaction bytes and the hash (which excludes its enum tag)."""
        start = self.pos
        tag = self.u8()
        # PublicContext, common to public and private transactions.
        def context_actor():
            self.actor()
            if self.u8() > 1:
                raise ValueError("unknown Authority")
        self.vec(context_actor)
        self.vec(lambda: self.take(32))
        self.vec(self.u64)
        self.vec(self.actor)
        if tag == 0:
            self.root_call()
        elif tag == 1:
            # ProvenExecution: boundary, asynchronous messages, recovery bindings, public root, private actions,
            # validity windows and image claims. None of the recorded blocks uses this branch, but walking it keeps
            # the framing correct rather than depending on a transaction-size table.
            self.boundary()

            def publication():
                kind = self.u8()
                if kind == 0:
                    self.envelope()
                elif kind == 1:
                    self.take(32)
                    self.note()
                else:
                    raise ValueError("unknown Publication")
            self.vec(publication)
            self.vec(lambda: (self.take(32), self.note()))
            self.option(self.root_call)
            self.vec(lambda: (self.take(96), self.note()))
            for _ in range(4):
                self.option(self.u64)

            def image_claim():
                kind = self.u8()
                if kind > 1:
                    raise ValueError("unknown ProgramImageClaim")
                self.take(64)
            self.vec(image_claim)

            def proven_fee():
                self.option(lambda: self.take(32))
                self.u64()

                def fallback():
                    self.take(96)
                    self.bytes_vec()
                    self.boundary()
                self.option(fallback)
            self.option(proven_fee)
        else:
            raise ValueError(f"unknown LeeTransaction tag {tag}")
        self.vec(lambda: (self.take(32), self.uint(16)))  # nonces

        def fee_declaration():
            self.take(32)
            self.u64()
            self.u64()
            self.uint(16)
        self.option(fee_declaration)

        def evidence():
            kind = self.u8()
            if kind == 0:
                self.key_state()
            elif kind == 1:
                self.take(64)
            else:
                raise ValueError("unknown PublicAccountEvidence")
        self.vec(evidence)

        def witness():
            self.bytes_vec()
            self.key_state()
        self.vec(witness)
        if tag == 1:
            self.bytes_vec()  # proof
        end = self.pos
        raw = self.raw[start:end]
        return raw, hashlib.sha256(raw[1:]).hexdigest()

    def receipt(self):
        success = self.u8()
        if success > 1:
            raise ValueError("invalid receipt success byte")
        fee = {name: self.uint(width) for name, width in (
            ("gas_exec", 8), ("gas_stor", 8), ("fee_base", 16), ("tip", 16), ("refund", 16))}
        return {"success": bool(success), "fee": fee}


def decode_recorded_block(raw):
    r = Reader(raw)
    block_id = r.u64()
    r.take(64)  # previous block hash and this block's hash
    r.u64()  # timestamp
    transactions = r.vec(r.skip_transaction)
    receipts = r.vec(r.receipt)
    if r.u8() > 2 or r.pos != len(raw):
        raise ValueError("bad Bedrock status or trailing block bytes")
    if len(receipts) > len(transactions):
        raise ValueError("more receipts than transactions")
    return block_id, transactions, receipts


def load(run):
    run = Path(run)
    blocks, transactions, receipts = {}, {}, {}
    for path in sorted((run / "blocks").glob("block-*.json")):
        encoded = json.loads(path.read_text())
        if not isinstance(encoded, str):
            raise ValueError(f"{path}: expected the recorded base64 JSON string")
        block_id, txs, rs = decode_recorded_block(base64.b64decode(encoded, validate=True))
        if block_id != int(path.stem.split("-")[1]):
            raise ValueError(f"{path}: file name and block id disagree")
        blocks[block_id] = encoded
        for i, (raw, tx_hash) in enumerate(txs):
            transactions[tx_hash] = [base64.b64encode(raw).decode(), block_id]
            if i < len(rs):
                receipts[tx_hash] = rs[i]
    if not blocks:
        raise ValueError(f"{run}: no recorded blocks")
    facts_path = run / "facts.json"
    if facts_path.exists():
        facts = json.loads(facts_path.read_text())
        for send in facts.get("sends", []):
            h, block_id = send["hash"], send["block"]
            if h not in transactions or transactions[h][1] != block_id or receipts.get(h) != send["receipt"]:
                raise ValueError(f"{facts_path}: transfer {h} disagrees with its recorded block")
    head = max(blocks)
    return {"run": run, "head": head, "blocks": blocks, "txs": transactions, "receipts": receipts}


def handle(world, method, params):
    p = params if isinstance(params, list) else []
    if method == "getSequencerStatus":
        return {"state": "online", "height": world["head"], "isOurTurn": True}
    if method == "checkHealth":
        return None
    if method == "getLastBlockId":
        return world["head"]
    if method == "getBlock":
        return world["blocks"].get(int(p[0]))
    if method == "getBlockRange":
        first, last = int(p[0]), int(p[1])
        if last < first:
            raise RpcError(-32602, "Invalid params")
        if last - first + 1 > 50:
            raise RpcError(-32602, "at most 50 blocks per getBlockRange")
        return [world["blocks"][i] for i in range(first, min(last, world["head"]) + 1) if i in world["blocks"]]
    tx_hash = str(p[0]).removeprefix("0x").lower() if method in ("getTransaction", "getTransactionReceipt") and p else ""
    if method == "getTransaction":
        return world["txs"].get(tx_hash)
    if method == "getTransactionReceipt":
        return world["receipts"].get(tx_hash)
    if method == "getFeeState":
        # The run's recorder captured these rates at height 2; only the height advances in this static replay.
        return {
            "height": world["head"], "base_fee_exec": 8, "base_fee_stor": 8,
            "next_base_fee_exec_floor": 8, "next_base_fee_exec_ceiling": 9,
            "next_base_fee_stor_floor": 8, "next_base_fee_stor_ceiling": 9,
            "max_gas_exec": 10000000, "max_gas_stor": 1000000,
        }
    if method == "getProgramIds":
        return {}
    if method == "getChannelId":
        return CHANNEL_ID
    if method in ("getAccount", "getAccountView"):
        return None  # no account-state responses were recorded
    if method == "getAccountBalance":
        return 0
    # In particular getCrossZoneDeadLetters was removed with cross-zone messaging in v0.4.0. Other unknown methods
    # receive the same normal JSON-RPC method-not-found answer; pages probe some of them behind .catch(() => null).
    raise RpcError(-32601, "Method not found")


class Handler(http.server.BaseHTTPRequestHandler):
    world = None

    def log_message(self, fmt, *args):
        sys.stdout.write("%s %s\n" % (self.command, fmt % args))
        sys.stdout.flush()

    def send(self, code, body, content_type="application/json"):
        data = body if isinstance(body, bytes) else str(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(data)

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "content-type")
        self.end_headers()

    def do_POST(self):
        try:
            request = json.loads(self.rfile.read(int(self.headers.get("Content-Length") or 0)) or b"null")
        except ValueError:
            return self.send(400, json.dumps({"jsonrpc": "2.0", "id": None,
                                               "error": {"code": -32700, "message": "Parse error"}}))
        if self.path != "/rpc":
            return self.send(404, "{}")
        requests = request if isinstance(request, list) else [request]
        answers = []
        for req in requests:
            try:
                if not isinstance(req, dict):
                    raise RpcError(-32600, "Invalid Request")
                method = req.get("method")
                print(f"RPC {method} {json.dumps(req.get('params') or [], separators=(',', ':'))}", flush=True)
                result = handle(self.world, method, req.get("params"))
                answers.append({"jsonrpc": "2.0", "id": req.get("id"), "result": result})
            except RpcError as e:
                answers.append({"jsonrpc": "2.0", "id": req.get("id") if isinstance(req, dict) else None,
                                "error": {"code": e.code, "message": str(e)}})
            except (KeyError, IndexError, TypeError, ValueError) as e:
                answers.append({"jsonrpc": "2.0", "id": req.get("id") if isinstance(req, dict) else None,
                                "error": {"code": -32602, "message": f"Invalid params: {e}"}})
        self.send(200, json.dumps(answers if isinstance(request, list) else answers[0]))

    def do_GET(self):
        path = self.path.split("?")[0].split("#")[0]
        if path == "/config.json":
            return self.send(200, json.dumps(CONFIG).encode())
        if path == "/programs.json":
            return self.send(200, json.dumps({"network": CONFIG["name"], "lez": "v0.4.0",
                                              "programs": {}, "instances": {}}))
        if path == "/status/status.json":
            return self.send(200, "null")  # no host monitor was recorded
        if path == "/status/history.json":
            return self.send(200, "[]")
        if path == "/faucet":
            return self.send(200, "null")  # no faucet is part of this run
        if path == "/AGENTS.md":
            f = BUNDLE / "AGENTS.md"
            return self.send(200, f.read_bytes(), "text/markdown; charset=utf-8") if f.exists() else self.send(404, "")
        if path == "/network":
            path = "/index.html"
        f = (BUNDLE / path.lstrip("/")).resolve()
        if f.is_dir():
            f = f / "index.html"
        if (f != BUNDLE and BUNDLE not in f.parents) or not f.is_file() or "test" in f.relative_to(BUNDLE).parts[:1]:
            return self.send(404, "not found", "text/plain")
        content_type = "text/javascript" if f.suffix == ".js" else (mimetypes.guess_type(f.name)[0] or "application/octet-stream")
        self.send(200, f.read_bytes(), content_type)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=18751, help="127.0.0.1 port; 0 chooses a free one")
    parser.add_argument("--run", default=str(DEFAULT_RUN))
    args = parser.parse_args()
    Handler.world = load(args.run)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"replay v0.4.0 RPC on http://127.0.0.1:{server.server_address[1]}/ "
          f"(head {Handler.world['head']}, {len(Handler.world['blocks'])} recorded blocks)", flush=True)
    try:
        server.serve_forever()
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
