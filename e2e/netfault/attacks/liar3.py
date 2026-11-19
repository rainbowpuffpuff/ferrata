#!/usr/bin/env python3
"""An endpoint in front of anvil whose lies are switched by files beside --ctl: the independent review's round 3
(attacks on `pubnet.py gone`), kept as regression cases (attacks.sh g2-g6):
  <ctl>.hide   eth_getTransactionReceipt answered null (a node with pruned history, or a liar)
  <ctl>.nlow   eth_getTransactionCount answered one lower than the truth (any block)
  <ctl>.stale  holds a block B: eth_call / getStorageAt / getCode / estimateGas at an explicit block >= B answered at B-1
  <ctl>.down   every request answered HTTP 503
Everything else is forwarded honestly."""
import argparse, http.client, json, os, threading, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
ap = argparse.ArgumentParser(); ap.add_argument("--upstream", required=True); ap.add_argument("--ctl", required=True)
ap.add_argument("--port-file", required=True); a = ap.parse_args(); U = urllib.parse.urlsplit(a.upstream)
STATE = {"eth_call": 1, "eth_getStorageAt": 2, "eth_getCode": 1, "eth_estimateGas": 1}
def up(m, p):
    c = http.client.HTTPConnection(U.hostname, U.port, timeout=30)
    c.request("POST", "/", json.dumps({"jsonrpc": "2.0", "id": 1, "method": m, "params": p}), {"Content-Type": "application/json"})
    r = json.loads(c.getresponse().read()); c.close(); return r
on = lambda k: os.path.exists(f"{a.ctl}.{k}")  # noqa: E731
def answer(q):
    m, p, rid = q.get("method"), list(q.get("params") or []), q.get("id")
    if on("stale") and m in STATE:
        b = int(open(f"{a.ctl}.stale").read()); k = STATE[m]
        if len(p) > k and isinstance(p[k], str) and p[k].startswith("0x") and int(p[k], 16) >= b:
            p[k] = hex(b - 1)
    r = up(m, p)
    if m == "eth_getTransactionReceipt" and on("hide"):
        r["result"] = None
    if m == "eth_getTransactionCount" and on("nlow") and "result" in r:
        r["result"] = hex(max(0, int(r["result"], 16) - 1))
    r["id"] = rid; return r
class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *x): pass
    def do_POST(self):
        q = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
        if on("down"):
            self.send_response(503); self.send_header("Content-Length", "0"); self.end_headers(); return
        body = json.dumps([answer(x) for x in q] if isinstance(q, list) else answer(q)).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
s = ThreadingHTTPServer(("127.0.0.1", 0), H); s.daemon_threads = True
open(a.port_file + ".part", "w").write(f"{s.server_address[1]}\n"); os.replace(a.port_file + ".part", a.port_file)
s.serve_forever()
