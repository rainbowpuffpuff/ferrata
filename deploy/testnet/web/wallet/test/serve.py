#!/usr/bin/env python3
"""The wallet as the test network serves it, for local tests: the bundle's files, the sequencer's RPC at /rpc and a
stand-in faucet at /faucet, all on one origin (as the public deployment does: /wallet, /rpc, /faucet).

    python3 -I serve.py --port 18650 --rpc http://127.0.0.1:3040 [--faucet-wallet BIN --faucet-from ACCOUNT_ID]

The faucet stand-in sends with LEZ's own wallet CLI (BIN, its home and password from the environment:
LEE_WALLET_HOME_DIR, LEE_WALLET_PASSWORD) from a funded account of a local network. It checks that the public key the
page sends belongs to the account id it sends, names the key (`--to-pk`) when the account is new on LEZ, and allows one
request per account every --faucet-interval seconds. Loopback only; standard library only.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SERVED = {"/": "index.html"}
for name in ("index.html", "app.js", "lez.js", "keystore.js", "history.js", "prove.js", "bridge.js", "private.js",
             "eth.js", "style.css", "config.json", "vendor/noble.js"):
    SERVED["/" + name] = name
TYPES = {".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8",
         ".json": "application/json"}
CSP = ("default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; "
       "manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58encode(b: bytes) -> str:
    n = int.from_bytes(b, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + out


def account_id_of(pk: bytes) -> str:
    prefix = b"/LEE/v0.3/AccountId/Public/".ljust(32, b"\0")
    return b58encode(hashlib.sha256(prefix + pk).digest())


class Handler(BaseHTTPRequestHandler):
    server_version = "wallet-test"
    sys_version = ""

    def log_message(self, fmt, *args):
        if self.server.args.verbose:
            super().log_message(fmt, *args)

    def send(self, code, body: bytes, ctype="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Content-Security-Policy", CSP)
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("X-Frame-Options", "DENY")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        name = SERVED.get(path)
        if not name:
            return self.send(404, b'{"error":"not found"}')
        with open(os.path.join(ROOT, name), "rb") as f:
            body = f.read()
        if name == "config.json" and self.server.args.config:
            with open(self.server.args.config, "rb") as f:
                body = f.read()
        self.send(200, body, TYPES[os.path.splitext(name)[1]])

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        if length > 4 << 20:
            return self.send(413, b'{"error":"too large"}')
        body = self.rfile.read(length)
        if self.path == "/rpc":
            req = urllib.request.Request(self.server.args.rpc, data=body, headers={"Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    return self.send(r.status, r.read())
            except Exception as e:  # noqa: BLE001
                return self.send(502, json.dumps({"error": f"sequencer: {e}"}).encode())
        if self.path == "/faucet":
            return self.faucet(body)
        return self.send(404, b'{"error":"not found"}')

    def faucet(self, body: bytes):
        a = self.server.args
        if not a.faucet_wallet:
            return self.send(503, b'{"error":"no faucet on this server"}')
        try:
            j = json.loads(body)
            account, pk_hex = str(j["account_id"]), str(j["public_key"]).lower()
            if not re.fullmatch(r"[0-9a-f]{64}", pk_hex):
                raise ValueError("public_key: 64 hex digits")
            if account_id_of(bytes.fromhex(pk_hex)) != account:
                raise ValueError("the public key is not this account's")
        except (ValueError, KeyError, json.JSONDecodeError) as e:
            return self.send(400, json.dumps({"error": f"bad request: {e}"}).encode())
        now = time.time()
        with self.server.lock:
            last = self.server.recent.get(account, 0)
            if now - last < a.faucet_interval:
                return self.send(429, json.dumps({"error": f"one request per {a.faucet_interval} s per account"}).encode())
            self.server.recent[account] = now
            rpc = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccount", "params": [account]}).encode()
            with urllib.request.urlopen(urllib.request.Request(a.rpc, data=rpc, headers={"Content-Type": "application/json"}), timeout=10) as r:
                exists = json.loads(r.read()).get("result") is not None
            to = ["--to", f"Public/{account}"] if exists else ["--to-pk", pk_hex]
            cmd = [a.faucet_wallet, "auth-transfer", "send", "--from", f"Public/{a.faucet_from}", *to, "--amount", str(a.faucet_amount)]
            p = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
        out = p.stdout + p.stderr
        self.server.faucet_log.append({"account": account, "new": not exists, "exit": p.returncode, "output": out[-2000:]})
        m = re.findall(r"\b[0-9a-f]{64}\b", out)
        if p.returncode != 0 or not m:
            return self.send(502, json.dumps({"error": "the faucet's send failed", "detail": out[-400:]}).encode())
        return self.send(200, json.dumps({"tx_hash": m[0], "amount": str(a.faucet_amount)}).encode())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=18650)
    ap.add_argument("--rpc", default="http://127.0.0.1:3040")
    ap.add_argument("--config", help="serve this file as config.json")
    ap.add_argument("--faucet-wallet")
    ap.add_argument("--faucet-from")
    ap.add_argument("--faucet-amount", type=int, default=5_000_000_000)
    ap.add_argument("--faucet-interval", type=int, default=20)
    ap.add_argument("--faucet-log", help="write the faucet's sends here (JSON) at exit")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()
    srv = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    srv.args, srv.lock, srv.recent, srv.faucet_log = args, threading.Lock(), {}, []
    print(f"serving {ROOT} on http://127.0.0.1:{args.port}/ (rpc -> {args.rpc})", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        if args.faucet_log:
            with open(args.faucet_log, "w") as f:
                json.dump(srv.faucet_log, f, indent=1)


if __name__ == "__main__":
    import signal
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    main()
