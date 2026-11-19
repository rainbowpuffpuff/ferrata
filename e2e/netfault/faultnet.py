#!/usr/bin/env python3
"""faultnet: three bad public endpoints in front of one local anvil, for e2e/netfault/run.sh (e2e/NETWORK.md).

  faultnet.py --upstream http://127.0.0.1:<anvil> --seed S --port-file F --log L

Starts three JSON-RPC endpoints on 127.0.0.1 (ports written to F, one per line) whose faults are drawn from a random
generator seeded with S (one per endpoint), so a seed replays the same kinds and rates of fault:

  flaky    "header not found" / "block not found" on reads that name a block, 429, 502/503, a timeout (an answer
           later than the client waits), and a head that goes backwards (eth_blockNumber, and "latest", 1-4 blocks
           behind now and then);
  lagging  k blocks behind (k from the seed, 2..6): "latest" is head - k, a block above that is "header not found", a
           receipt or transaction in a block above it is null, the pending nonce is the lagged latest one; and 429s;
  sends    eth_sendRawTransaction dropped (the hash answered, nothing forwarded), duplicated (forwarded twice), or
           forwarded with the answer lost (the connection closed); plus "block not found" now and then.

and a fourth, from the seed (seed % 4): a liar that answers wrong but valid-looking values
  0 frozen    contract state (eth_call, getStorageAt, getCode, estimateGas, getProof) at any block >= F answered at
              block F, F written by run.sh to <ctl> after the deployment (it "forgets" every deposit, submit and
              release), while nonces and balances are honest: the independent review's selective liar;
  1 receipts  a receipt (status 1, block 1,000,000,000) for any transaction without one;
  2 nonce     the sender's nonce one lower than the chain's, latest and pending alike;
  3 steady    no liar: an honest endpoint without faults;
and, shared by all, outages: 0 to 2 windows (from the seed: 5-150 s after the start, 15-60 s long) in which every
endpoint answers 503 or nothing, as when a provider or the machine's network is down.

Which personality each port has is also drawn from the seed. Every fault is counted in the log (JSONL).
"""
import argparse
import json
import os
import random
import sys
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "scripts"))
from pubnet import keccak256  # noqa: E402

BLOCK_ARG = {"eth_call": 1, "eth_estimateGas": 1, "eth_getBalance": 1, "eth_getCode": 1, "eth_getStorageAt": 2,
             "eth_getProof": 2, "eth_getTransactionCount": 1, "eth_getBlockByNumber": 0, "eth_createAccessList": 1}


class Upstream:
    def __init__(self, url):
        self.url = url
        self._head, self._t = 0, 0.0
        self.lock = threading.Lock()

    def call(self, method, params):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        req = urllib.request.Request(self.url, data=body, headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read())

    def head(self):
        with self.lock:
            if time.monotonic() - self._t > 0.2:
                self._head, self._t = int(self.call("eth_blockNumber", [])["result"], 16), time.monotonic()
            return self._head


OUTAGES = []  # (start, end) in monotonic time, shared by the endpoints
SENDER = ""


STATE = {"eth_call": 1, "eth_getStorageAt": 2, "eth_getCode": 1, "eth_estimateGas": 1, "eth_getProof": 2}


class Endpoint:
    def __init__(self, name, kind, rng, up, log, lag=0, ctl=""):
        self.name, self.kind, self.rng, self.up, self.lag, self.ctl = name, kind, rng, up, lag, ctl
        self.logf, self.lock = log, threading.Lock()
        r = rng.random
        self.p = {  # rates drawn once per seed
            "notfound": 0.04 + 0.08 * r() if kind in ("flaky", "sends") else 0.0,
            "r429": 0.03 + 0.05 * r() if kind in ("flaky", "lagging") else 0.0,
            "r5xx": 0.01 + 0.03 * r() if kind == "flaky" else 0.0,
            "timeout": 0.005 + 0.015 * r() if kind == "flaky" else 0.0,
            "backwards": 0.05 + 0.1 * r() if kind == "flaky" else 0.0,
            "drop": 0.15 + 0.25 * r() if kind == "sends" else 0.0,
            "dup": 0.15 + 0.25 * r() if kind == "sends" else 0.0,
            "lost": 0.05 + 0.15 * r() if kind == "sends" else 0.0,
        }

    def roll(self, what):
        with self.lock:
            return self.rng.random() < self.p[what]

    def count(self, fault, method):
        with self.lock, open(self.logf, "a") as f:
            f.write(json.dumps({"t": time.time(), "ep": self.name, "kind": self.kind, "fault": fault,
                                "method": method}) + "\n")

    def visible(self):
        """This node's head: the chain's minus its lag, minus 1-4 now and then (a head that goes backwards)."""
        h = self.up.head() - self.lag
        return h - self.rng.randint(1, 4) if self.p["backwards"] and self.roll("backwards") else h

    def lie(self, req):
        """The liar's answer, or None to answer honestly."""
        method, params, rid = req.get("method", ""), list(req.get("params") or []), req.get("id")
        if self.kind == "liar-frozen" and method in STATE and self.ctl and os.path.exists(self.ctl):
            f = int(open(self.ctl).read().strip() or 0)
            k = STATE[method]
            if f and len(params) > k and isinstance(params[k], str) and params[k].startswith("0x") \
                    and int(params[k], 16) >= f:
                params[k] = hex(f)
                self.count("lie-frozen", method)
                r = self.up.call(method, params)
                r["id"] = rid
                return 200, r, 0
        if self.kind == "liar-receipts" and method == "eth_getTransactionReceipt":
            r = self.up.call(method, params)
            if r.get("result") is None:
                self.count("lie-receipt", method)
                r["result"] = {"transactionHash": params[0], "blockNumber": hex(1_000_000_000),
                               "blockHash": "0x" + "ab" * 32, "status": "0x1", "from": SENDER or "0x" + "00" * 20,
                               "to": None, "gasUsed": "0x5208", "cumulativeGasUsed": "0x5208", "contractAddress": None,
                               "logs": [], "logsBloom": "0x" + "00" * 256, "transactionIndex": "0x0",
                               "effectiveGasPrice": "0x1", "type": "0x2"}
            r["id"] = rid
            return 200, r, 0
        if self.kind == "liar-nonce" and method == "eth_getTransactionCount":
            r = self.up.call(method, params)
            if "result" in r and int(r["result"], 16) > 0:
                self.count("lie-nonce", method)
                r["result"] = hex(int(r["result"], 16) - 1)
            r["id"] = rid
            return 200, r, 0
        return None

    def answer(self, req):
        """(status, body or None for a closed connection, delay before answering)."""
        method, params = req.get("method", ""), list(req.get("params") or [])
        rid = req.get("id")
        err = lambda code, msg: (200, {"jsonrpc": "2.0", "id": rid, "error": {"code": code, "message": msg}}, 0)  # noqa
        now = time.monotonic()
        if any(a <= now < b for a, b in OUTAGES):
            self.count("outage", method)
            return (503, b"service unavailable", 0) if self.rng.random() < 0.7 else (0, None, 0)
        if self.kind.startswith("liar") or self.kind == "steady":
            lied = self.lie(req) if self.kind.startswith("liar") else None
            if lied is not None:
                return lied
            r = self.up.call(method, params)
            r["id"] = rid
            return 200, r, 0
        if self.roll("r429"):
            self.count("429", method)
            return 429, {"jsonrpc": "2.0", "id": rid, "error": {"code": -32005, "message": "Too Many Requests"}}, 0
        if self.roll("r5xx"):
            self.count("5xx", method)
            return self.rng.choice((502, 503)), b"<html>bad gateway</html>", 0
        if self.roll("timeout"):
            self.count("timeout", method)
            return 504, b"upstream timed out", 6.0  # later than the client's 3 s
        vis = self.visible()
        if method == "eth_blockNumber":
            return 200, {"jsonrpc": "2.0", "id": rid, "result": hex(vis)}, 0
        if method in BLOCK_ARG:
            k = BLOCK_ARG[method]
            p = params + [None] * (k + 1 - len(params)) if len(params) <= k else params
            tag = p[k]
            if tag in (None, "latest") or (tag == "pending" and self.lag):
                p[k] = hex(vis)
            elif isinstance(tag, str) and tag.startswith("0x"):
                if int(tag, 16) > vis:
                    self.count("ahead-of-node", method)
                    if method == "eth_getBlockByNumber":
                        return 200, {"jsonrpc": "2.0", "id": rid, "result": None}, 0
                    return err(-32000, "header not found")
            if isinstance(p[k], str) and p[k].startswith("0x") and self.roll("notfound"):
                self.count("notfound", method)
                return err(-32001, self.rng.choice(("block not found", "header not found", "missing trie node 1f2e")))
            params = p
        if method in ("eth_getTransactionReceipt", "eth_getTransactionByHash"):
            r = self.up.call(method, params)
            res = r.get("result")
            if res and res.get("blockNumber") and int(res["blockNumber"], 16) > vis:
                self.count("receipt-lag", method)
                r["result"] = None
            r["id"] = rid
            return 200, r, 0
        if method == "eth_getLogs" and params and isinstance(params[0], dict):
            f = dict(params[0])
            if f.get("toBlock") in (None, "latest") or (isinstance(f.get("toBlock"), str)
                                                         and f["toBlock"].startswith("0x")
                                                         and int(f["toBlock"], 16) > vis):
                f["toBlock"] = hex(vis)
            params = [f]
        if method == "eth_sendRawTransaction":
            h = "0x" + keccak256(bytes.fromhex(params[0][2:])).hex()
            if self.roll("drop"):
                self.count("drop", method)
                return 200, {"jsonrpc": "2.0", "id": rid, "result": h}, 0
            r = self.up.call(method, params)
            if self.roll("dup"):
                self.count("dup", method)
                self.up.call(method, params)
            if self.roll("lost"):
                self.count("lost", method)
                return 0, None, 0
            r["id"] = rid
            return 200, r, 0
        r = self.up.call(method, params)
        r["id"] = rid
        return 200, r, 0


def serve(ep, port_out):
    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *a):
            pass

        def do_POST(self):
            req = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
            if isinstance(req, list):
                outs = [ep.answer(r) for r in req]
                status, body, delay = 200, [o[1] for o in outs], max(o[2] for o in outs)
                if any(o[1] is None or o[0] != 200 for o in outs):
                    status, body, delay = outs[0]
            else:
                status, body, delay = ep.answer(req)
            if delay:
                time.sleep(delay)
            if body is None:
                self.close_connection = True
                self.connection.shutdown(2)
                return
            data = body if isinstance(body, bytes) else json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    srv.daemon_threads = True
    port_out.append(srv.server_address[1])
    threading.Thread(target=srv.serve_forever, daemon=True).start()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--port-file", required=True)
    ap.add_argument("--log", required=True)
    ap.add_argument("--ctl", default="", help="the frozen liar's block F (a file run.sh writes after the deployment)")
    ap.add_argument("--sender", default="")
    a = ap.parse_args()
    global SENDER
    SENDER = a.sender.lower()
    rng = random.Random(a.seed)
    kinds = ["flaky", "lagging", "sends"]
    rng.shuffle(kinds)
    t0 = time.monotonic()
    for _ in range(rng.choice((0, 1, 1, 2))):
        start = rng.uniform(5, 150)
        OUTAGES.append((t0 + start, t0 + start + rng.uniform(15, 60)))
    up = Upstream(a.upstream)
    ports, desc = [], []
    kinds.append(("liar-frozen", "liar-receipts", "liar-nonce", "steady")[a.seed % 4])
    for i, kind in enumerate(kinds):
        lag = rng.randint(2, 6) if kind == "lagging" else 0
        ep = Endpoint(f"ep{i}", kind, random.Random(a.seed * 1000 + i), up, a.log, lag, a.ctl)
        serve(ep, ports)
        desc.append({"ep": f"ep{i}", "port": ports[-1], "kind": kind, "lag": lag,
                     "rates": {k: round(v, 3) for k, v in ep.p.items() if v}})
    with open(a.port_file + ".part", "w") as f:
        f.write("\n".join(map(str, ports)) + "\n")
    os.replace(a.port_file + ".part", a.port_file)
    print(json.dumps({"seed": a.seed, "endpoints": desc,
                      "outages_s": [[round(x - t0), round(y - t0)] for x, y in OUTAGES]}), flush=True)
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    main()
