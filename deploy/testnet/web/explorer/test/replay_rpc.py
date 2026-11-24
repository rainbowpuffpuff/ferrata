#!/usr/bin/env python3
"""Serve the explorer bundle with a replay of LEZ v0.4.0-rc3's sequencer JSON-RPC over test/out/world.json.

    python3 -I deploy/testnet/web/explorer/test/replay_rpc.py [--port 18750] [--world PATH] [--config PATH]

Loopback only. Serves the bundle's files at /, the test world's config.json at /config.json, a read-only
JSON-RPC at /rpc with the rc3 methods the pages use (getBlock, getBlockRange, getLastBlockId, getTransaction,
getAccount, getAccountView, getAccountBalance, getProgramIds, getChannelId, getFeeState, checkHealth; batches too),
answering as the sequencer does (blocks and transactions as base64 of borsh, account ids base58, hashes hex, u128 as
bare JSON numbers), a mock beacon API at /beacon-mock/<chain>/eth/v1/beacon/headers/<slot|finalized> and a mock
execution JSON-RPC at /eth-mock/<chain>. Every request is logged to stdout. Standard library only; writes nothing.
"""
import argparse
import http.server
import json
import mimetypes
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BUNDLE = HERE.parent
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58decode(text):
    n = 0
    for ch in text:
        n = n * 58 + B58.index(ch)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return bytes(len(text) - len(text.lstrip("1"))) + body


class RpcError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def load(world_path):
    w = json.loads(Path(world_path).read_text())
    w["blocks"] = {int(k): v for k, v in w["blocks"].items()}
    return w


def handle(world, method, params):
    p = params if isinstance(params, list) else []
    head = world["head"] - world.get("held", 0)
    if method == "checkHealth":
        return None
    if method == "getLastBlockId":
        return head
    if method == "getBlock":
        return world["blocks"].get(int(p[0])) if int(p[0]) <= head else None
    if method == "getBlockRange":
        a, b = int(p[0]), int(p[1])
        if b < a:
            raise RpcError(-32602, "Invalid params")
        if b - a + 1 > 50:  # as the public gateway
            raise RpcError(-32602, "at most 50 blocks per getBlockRange")
        return [world["blocks"][i] for i in range(a, min(b, head) + 1) if i in world["blocks"]]
    if method == "getCrossZoneDeadLetters":
        return {"total_retired": 0, "retained": []}
    if method == "getTransaction":
        t = world["txs"].get(str(p[0]).lower())
        return t
    if method == "getAccount":
        if len(b58decode(p[0])) != 32:
            raise RpcError(-32602, "Invalid params")
        return world["accounts"].get(p[0])
    if method == "getAccountView":
        sel = p[0]
        a = world["accounts"].get(sel["account_id"])
        if a is None:
            return None
        st = a["data"]["actor_states"].get(sel["program_account_id"])
        return {"nonce": a["nonce"], "data": {"actor_states": {sel["program_account_id"]: st} if st is not None else {}}}
    if method == "getAccountBalance":
        a = world["accounts"].get(p[0])
        st = a and a["data"]["actor_states"].get("11111111111111111111111111111111")
        return int.from_bytes(bytes(st), "little") if st else 0
    if method == "getProgramIds":
        return world["program_ids"]
    if method == "getChannelId":
        return world["channel_id"]
    if method == "getFeeState":
        return world["fee_state"]
    raise RpcError(-32601, "Method not found")


class Handler(http.server.BaseHTTPRequestHandler):
    world = None
    config_path = None

    def log_message(self, fmt, *args):
        sys.stdout.write("%s %s\n" % (self.command, fmt % args))
        sys.stdout.flush()

    def send(self, code, body, ctype="application/json"):
        data = body if isinstance(body, bytes) else body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
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
        n = int(self.headers.get("Content-Length") or 0)
        try:
            req = json.loads(self.rfile.read(n) or b"null")
        except ValueError:
            return self.send(400, json.dumps({"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}}))
        if self.path == "/__advance":  # test only: reveal one held-back block
            self.world["held"] = max(0, self.world.get("held", 0) - 1)
            return self.send(200, json.dumps({"head": self.world["head"] - self.world["held"]}))
        if self.path == "/faucet":  # a stand-in faucet: records the request, answers as the host's gateway does
            self.world.setdefault("faucet_requests", []).append(req)
            if not isinstance(req, dict) or not req.get("account"):
                return self.send(400, json.dumps({"ok": False, "error": "account is required"}))
            return self.send(200, json.dumps({"ok": True, "hash": self.world.get("faucet_hash", "00" * 32), "credited": 10000000000}))
        if self.path.startswith("/eth-mock/"):
            chain = self.path.split("/")[2]
            return self.send(200, json.dumps(self.eth(chain, req)))
        if self.path != "/rpc":
            return self.send(404, "{}")
        reqs = req if isinstance(req, list) else [req]
        out = []
        for r in reqs:
            try:
                out.append({"jsonrpc": "2.0", "id": r.get("id"), "result": handle(self.world, r.get("method"), r.get("params"))})
            except RpcError as e:
                out.append({"jsonrpc": "2.0", "id": r.get("id"), "error": {"code": e.code, "message": str(e)}})
            except (KeyError, IndexError, TypeError, ValueError) as e:
                out.append({"jsonrpc": "2.0", "id": r.get("id"), "error": {"code": -32602, "message": f"Invalid params: {e}"}})
        self.send(200, json.dumps(out if isinstance(req, list) else out[0]))

    def eth(self, chain, req):
        blocks = self.world["eth"].get(chain, {}).get("blocks", {})
        method, params = req.get("method"), req.get("params") or []
        res = None
        if method == "eth_getBlockByNumber":
            res = blocks.get(params[0])
        elif method == "eth_getBlockByHash":
            res = next((b for b in blocks.values() if b["hash"] == params[0]), None)
        elif method == "eth_chainId":
            res = hex(int(chain))
        return {"jsonrpc": "2.0", "id": req.get("id"), "result": res}

    def do_GET(self):
        path = self.path.split("?")[0].split("#")[0]
        if path.startswith("/beacon-mock/"):
            parts = path.split("/")
            chain, which = parts[2], parts[-1]
            b = self.world["beacon"].get(chain, {})
            body = b.get("finalized") if which in ("finalized", "head") else b.get("headers", {}).get(which)
            return self.send(200, json.dumps(body)) if body else self.send(404, json.dumps({"code": 404, "message": "NOT_FOUND"}))
        if path == "/faucet":
            return self.send(200, json.dumps({"faucet_account": "ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs", "balance": 999990000000000, "amount": "10000000000", "unit": "1 LGO = 10^9", "limits": {"per_account_seconds": 86400, "per_ip_per_day": 3, "global_per_hour": 120}, "how": "POST /faucet"}))
        if path == "/__faucet_requests":
            return self.send(200, json.dumps(self.world.get("faucet_requests", [])))
        if path == "/config.json":
            return self.send(200, Path(self.config_path).read_bytes())
        if path in ("/status/status.json", "/status/history.json"):
            f = Path(self.config_path).parent / "status" / path.rsplit("/", 1)[1]
            return self.send(200, f.read_bytes()) if f.exists() else self.send(404, "{}")
        if path == "/programs.json":
            f = Path(self.config_path).with_name("programs.json")
            # as the deployment serves it before anything is deployed
            return self.send(200, f.read_bytes() if f.exists() else b'{"status": "being deployed"}')
        if path == "/AGENTS.md":
            f = BUNDLE / "AGENTS.md"
            return self.send(200, f.read_bytes(), "text/markdown; charset=utf-8") if f.exists() else self.send(404, "")
        if path == "/network":  # the live host serves the landing page there (config.landing)
            path = "/index.html"
        rel = path.lstrip("/")
        f = (BUNDLE / rel).resolve()
        if f.is_dir():
            f = f / "index.html"
        if BUNDLE not in f.parents and f != BUNDLE or not f.is_file() or "test" in f.relative_to(BUNDLE).parts[:1]:
            return self.send(404, "not found", "text/plain")
        ctype = mimetypes.guess_type(f.name)[0] or "application/octet-stream"
        if f.suffix == ".js":
            ctype = "text/javascript"
        self.send(200, f.read_bytes(), ctype)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=18750)
    ap.add_argument("--world", default=str(HERE / "out" / "world.json"))
    ap.add_argument("--config", default=str(HERE / "out" / "config.json"))
    ap.add_argument("--hold", type=int, default=0, help="serve the head this many blocks early; POST /__advance reveals one")
    a = ap.parse_args()
    Handler.world = load(a.world)
    Handler.world["held"] = a.hold
    snap_faucet = [h for h, (b64, bid) in Handler.world["txs"].items() if bid == Handler.world.get("snapshot_blocks", [0])[0]]
    Handler.world["faucet_hash"] = snap_faucet[0] if snap_faucet else "00" * 32
    Handler.config_path = a.config
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", a.port), Handler)
    print(f"replay RPC on http://127.0.0.1:{a.port}/ (head {Handler.world['head']})", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
