#!/usr/bin/env python3
"""A JSON-RPC endpoint in front of an upstream (anvil) that misbehaves on purpose: the independent review's
(review-rpc-robust, 18 Nov 2026) attacks on scripts/pubnet.py, kept as regression cases (attacks.sh).
  --mode honest          forwards everything
  --mode stale-state     while <ctl> exists (it holds a block number B): eth_call, eth_getStorageAt, eth_getCode,
                         eth_estimateGas at an explicit block >= B are answered at block B-1 (a valid-looking stale
                         answer); the nonce and balance are answered honestly (a selective liar)
  --mode stale-all       the same, the nonce and balance too (a consistent liar: one old block for everything)
  --mode nonce-stale     eth_getTransactionCount (latest and pending): upstream - 1 (a node one tx behind whose
                         pending view equals its latest)
  --mode reject-forward  eth_sendRawTransaction forwarded, but answered with an error a node could give
  --mode reject          eth_sendRawTransaction refused with that error, not forwarded
  --mode hang-send       eth_sendRawTransaction never forwarded, answered after 600 s
  --mode fake-receipt    eth_getTransactionReceipt of a hash upstream has no receipt for: a valid-looking receipt in
                         block 1,000,000,000 with status 1
"""
import argparse, http.client, json, os, sys, threading, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ap = argparse.ArgumentParser()
ap.add_argument("--upstream", required=True); ap.add_argument("--mode", default="honest")
ap.add_argument("--ctl", default=""); ap.add_argument("--port-file", required=True); ap.add_argument("--log", default="")
ap.add_argument("--sender", default="")
a = ap.parse_args()
U = urllib.parse.urlsplit(a.upstream)
lock = threading.Lock()


def up(method, params):
    c = http.client.HTTPConnection(U.hostname, U.port, timeout=30)
    c.request("POST", "/", json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
              {"Content-Type": "application/json"})
    r = json.loads(c.getresponse().read()); c.close()
    return r


def log(**kw):
    if a.log:
        with lock, open(a.log, "a") as f:
            f.write(json.dumps({"t": time.time(), **kw}) + "\n")


BLOCK_ARG = {"eth_call": 1, "eth_getStorageAt": 2, "eth_getCode": 1, "eth_estimateGas": 1, "eth_getAccountInfo": 1,
             "eth_getTransactionCount": 1, "eth_getBalance": 1}


def answer(req):
    m, p = req.get("method"), list(req.get("params") or [])
    rid = req.get("id")
    lie_on = a.ctl and os.path.exists(a.ctl)
    log(seen=m, params=[x if isinstance(x, str) and len(x) < 70 else "..." for x in p])
    if a.mode in ("stale-state", "stale-all") and lie_on and m in BLOCK_ARG:
        if a.mode == "stale-all" or m not in ("eth_getTransactionCount", "eth_getBalance", "eth_getAccountInfo"):
            k = BLOCK_ARG[m]
            b = int(open(a.ctl).read().strip())
            if len(p) > k and isinstance(p[k], str) and p[k].startswith("0x") and int(p[k], 16) >= b:
                log(lie=m, asked=p[k], answered=hex(b - 1)); p[k] = hex(b - 1)
    if a.mode == "nonce-stale" and m == "eth_getTransactionCount":
        r = up(m, p)
        if "result" in r:
            n = int(r["result"], 16)
            log(lie=m, honest=n, answered=max(0, n - 1)); r["result"] = hex(max(0, n - 1))
        r["id"] = rid; return r
    if m == "eth_sendRawTransaction" and a.mode in ("reject-forward", "reject", "hang-send"):
        if a.mode == "hang-send":
            log(hang=p[0][:20]); time.sleep(600)
        if a.mode == "reject-forward":
            r = up(m, p); log(forwarded=r)
        log(reject=p[0][:20])
        return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32000, "message": "transaction pool: duplicate entry"}}
    r = up(m, p)
    if a.mode == "fake-receipt" and m == "eth_getTransactionReceipt" and r.get("result") is None:
        h = p[0]
        r["result"] = {"transactionHash": h, "blockNumber": hex(1_000_000_000), "blockHash": "0x" + "ab" * 32,
                       "status": "0x1", "from": a.sender, "to": None, "gasUsed": "0x5208", "cumulativeGasUsed": "0x5208",
                       "contractAddress": None, "logs": [], "logsBloom": "0x" + "00" * 256, "transactionIndex": "0x0",
                       "effectiveGasPrice": "0x1", "type": "0x2"}
        log(fake_receipt=h)
    r["id"] = rid
    return r


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *x): pass
    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
        res = [answer(x) for x in req] if isinstance(req, list) else answer(req)
        body = json.dumps(res).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)


s = ThreadingHTTPServer(("127.0.0.1", 0), H); s.daemon_threads = True
open(a.port_file + ".part", "w").write(f"{s.server_address[1]}\n"); os.replace(a.port_file + ".part", a.port_file)
s.serve_forever()
