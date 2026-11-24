#!/usr/bin/env python3
"""The gateway against a stand-in sequencer: allowlist, batch, range and list caps, work-priced rate limits, the
network-wide send cap, response-size cap and byte budgets (reserved while answers are in flight; an answer built and
not sent charged to its IP, which waits for it before the sequencer builds another; small answers' own network-wide
budget), limits before the body, socket timeout, concurrency caps (per IP, checked first, and in all), CORS, the
faucet's input checks, per-account/IP/network limits, failures counted and not echoed, one claim per IP and one
transfer at a time, and whether a claim paid read from its own transaction (with a stand-in transfer tool that records
each transaction before sending it, and a stand-in sequencer with a pool and the faucet account's nonce): an answer
lost, a transfer applied after the gateway looked, another source paying the same amount, a claim interrupted after its
record, a transfer never sent (sent again), a transfer whose nonce another spent, a transfer that does not settle in
time (also in a ledger written before records).
Run: python3 -I deploy/testnet/tests/test_gateway.py"""
import hashlib
import http.client
import http.server
import importlib.machinery
import importlib.util
import json
import os
import pathlib
import socket
import sqlite3
import stat
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
GW = pathlib.Path(os.environ.get("GATEWAY_UNDER_TEST") or HERE.parent / "host" / "bin" / "ferrata-gateway")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Upstream(http.server.BaseHTTPRequestHandler):
    seen = []
    balances = {}  # getAccountBalance: 42 unless credited (by a faucet transfer applied, or "_credit": another source)
    # The faucet account's transfers: its nonce, the transactions applied (by hash), the pool (sent, not applied yet).
    chain = {"nonce": 0, "applied": {}, "pool": []}
    lock = threading.Lock()

    @classmethod
    def apply(cls, tx):
        """A transfer applied if its nonce is the account's (else it never can be: another spent its nonce)."""
        if tx["nonce"] != cls.chain["nonce"]:
            return False
        cls.chain["nonce"] += 1
        cls.chain["applied"][tx["hash"]] = tx
        cls.balances[tx["to"]] = cls.balances.get(tx["to"], 42) + tx["amount"]
        return True

    def log_message(self, *a):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        Upstream.seen.append(body)
        reqs = body if isinstance(body, list) else [body]
        fresh = "3Zz7e5YPLyxJzG5zHk9kqKDS8vN3yXVhFgk3wTKVcGd1"  # an account the stand-in has never seen
        big = {7777: "x" * 30000, 7778: "x" * 59000, 7779: "x" * 30000, 8888: "x" * 100000}  # getBlock answers
        slow = {5555: 1.5, 7778: 0.5, 7779: 0.5}  # and seconds before them

        def result(r):
            p0 = (r.get("params") or [None])[0]
            m = r.get("method")
            with Upstream.lock:
                if m == "_credit":
                    Upstream.balances[p0] = Upstream.balances.get(p0, 42) + int(r["params"][1])
                    return None
                if m == "_nonce":
                    return Upstream.chain["nonce"]
                if m == "_submit":  # (tx, "apply" now | "hold" in the pool | "lose" on the way)
                    mode = r["params"][1]
                    if p0["hash"] in Upstream.chain["applied"] or any(t["hash"] == p0["hash"] for t in Upstream.chain["pool"]):
                        return "known"
                    if mode == "hold":
                        Upstream.chain["pool"].append(p0)
                    elif mode == "apply":
                        Upstream.apply(p0)
                    return mode
                if m == "_mine":  # the pool goes into a block
                    pool, Upstream.chain["pool"] = Upstream.chain["pool"], []
                    return [Upstream.apply(tx) for tx in pool]
                if m == "_status":
                    return "applied" if p0 in Upstream.chain["applied"] else None
            if r.get("method") == "getAccountBalance":
                return Upstream.balances.get(p0, 42)
            if r.get("method") == "getAccount" and p0 == fresh:
                return None
            if r.get("method") == "getBlock" and p0 in slow:
                time.sleep(slow[p0])
            return big.get(p0, 42) if r.get("method") == "getBlock" else 42
        out = [{"jsonrpc": "2.0", "id": r.get("id"), "result": result(r)} for r in reqs]
        data = json.dumps(out if isinstance(body, list) else out[0]).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def post(url, obj, headers=None):
    req = urllib.request.Request(url, data=json.dumps(obj).encode(),
                                 headers={"Content-Type": "application/json", **(headers or {})})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, dict(r.headers), json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), json.loads(e.read())


def main():
    up_port, gw_port = free_port(), free_port()
    up = http.server.ThreadingHTTPServer(("127.0.0.1", up_port), Upstream)
    threading.Thread(target=up.serve_forever, daemon=True).start()
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="gw-test-", dir=os.environ.get("TEST_TMP")))
    tool = tmp / "tool"
    runs = tmp / "runs.log"  # every transfer the stand-in tool signs, by account
    calls = tmp / "calls.log"  # every run of the tool: its command and account or record
    # The stand-in of ferrata-day1 transfer / transfer-status / transfer-resend. A transfer takes the faucet account's
    # nonce, writes its record (hash, nonce, bytes) before it is sent, then sends it; the account's name says what goes
    # wrong on its first transfer (FaiL: on every one).
    tool.write_text(f"#!{sys.executable} -I\n" + f"RUNS, CALLS = {str(runs)!r}, {str(calls)!r}\n" + r'''
import hashlib, json, os, sys, time, urllib.request
cmd, rest = sys.argv[1], sys.argv[2:]
opt = {rest[i][2:]: rest[i + 1] for i in range(0, len(rest) - 1, 2)}
def rpc(method, *params):
    q = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
    req = urllib.request.Request(opt["rpc"], data=q, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=5) as r:
        return json.load(r)["result"]
with open(CALLS, "a") as f:
    f.write(f"{cmd} {opt.get('to') or opt.get('record')}\n")
if cmd == "transfer":
    to, amount = opt["to"], int(opt["amount"])
    first = to not in open(RUNS).read().split() if os.path.exists(RUNS) else True
    with open(RUNS, "a") as f:
        f.write(to + "\n")
    if "FaiL" in to:  # refused before anything is signed
        sys.exit("secret detail at /var/lib/ferrata/keys")
    if "SLoW" in to:
        time.sleep(2)
    tx = {"nonce": rpc("_nonce"), "to": to, "amount": amount, "salt": os.urandom(8).hex()}
    tx["hash"] = hashlib.sha256(json.dumps(tx, sort_keys=True).encode()).hexdigest()
    rec = {"hash": tx["hash"], "from": "FaucetAccount1111111111111111111111111111111", "nonce": str(tx["nonce"]),
           "tx": json.dumps(tx).encode().hex()}
    with open(opt["record"] + ".partial", "w") as f:
        json.dump(rec, f)
    os.rename(opt["record"] + ".partial", opt["record"])
    odd = first and next((k for k in ("LoST", "LaTE", "ThRD", "NeVeR", "TiMe") if k in to), None)
    if odd == "NeVeR":  # interrupted after its record, before it was sent
        sys.exit("killed")
    if odd == "ThRD":  # another source pays the account the same amount; this transfer is lost on the way
        rpc("_credit", to, amount)
        rpc("_submit", tx, "lose")
        sys.exit("no answer")
    rpc("_submit", tx, "hold" if odd in ("LaTE", "TiMe") else "apply")
    if odd in ("LoST", "LaTE"):  # sent, its answer lost
        sys.exit("the answer was lost")
    if odd == "TiMe":
        time.sleep(10)
    t0, state = time.monotonic(), "pending"
    while time.monotonic() - t0 < float(opt["wait"]):
        if rpc("_status", tx["hash"]) == "applied":
            state = "applied"
            break
        time.sleep(0.1)
    print(json.dumps({"hash": tx["hash"], "to": to, "amount": str(amount), "state": state, "credited": state == "applied",
                      "pk": "yes" if "to-pk" in opt else "no"}))
elif cmd == "transfer-status":
    rec = json.load(open(opt["record"]))
    if rpc("_status", rec["hash"]) == "applied":
        print(json.dumps({"hash": rec["hash"], "state": "applied", "block": 1}))
    else:
        print(json.dumps({"hash": rec["hash"], "state": "dropped" if rpc("_nonce") > int(rec["nonce"]) else "pending"}))
elif cmd == "transfer-resend":
    rec = json.load(open(opt["record"]))
    rpc("_submit", json.loads(bytes.fromhex(rec["tx"])), "apply")
    print(json.dumps({"hash": rec["hash"], "resent": True}))
else:
    sys.exit(f"unknown command {cmd}: transfer transfer-status transfer-resend")
''')
    tool.chmod(tool.stat().st_mode | stat.S_IEXEC)
    (tmp / "accounts.json").write_text(json.dumps({"payer": "FaucetAccount1111111111111111111111111111111"}))
    old_timeout = "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYig"
    (tmp / "faucet").mkdir()
    db = sqlite3.connect(tmp / "faucet" / "claims.sqlite")  # a ledger as written before timeouts had a status of their own
    db.execute("CREATE TABLE claims (t REAL, ip TEXT, account TEXT, ok INTEGER, hash TEXT, detail TEXT)")
    db.execute("INSERT INTO claims VALUES (?,'127.0.0.1',?,0,'','timeout')", (time.time() - 600, old_timeout))
    db.commit()
    db.close()
    os.environ.update({
        "GATEWAY_LISTEN": f"127.0.0.1:{gw_port}", "GATEWAY_UPSTREAM": f"http://127.0.0.1:{up_port}",
        "GATEWAY_READ_RATE": "0.001", "GATEWAY_READ_BURST": "30", "GATEWAY_SEND_RATE": "0.001", "GATEWAY_SEND_BURST": "2",
        "GATEWAY_SEND_GLOBAL_RATE": "0.001", "GATEWAY_SEND_GLOBAL_BURST": "4",
        "GATEWAY_BYTES_RATE": "1", "GATEWAY_BYTES_BURST": "50000", "GATEWAY_BYTES_GLOBAL_RATE": "1",
        "GATEWAY_BYTES_GLOBAL_BURST": "1000000000", "GATEWAY_MAX_RESPONSE": "60000",
        "GATEWAY_MAX_INFLIGHT": "3", "GATEWAY_PER_IP_INFLIGHT": "2", "GATEWAY_SOCKET_TIMEOUT": "2",
        "FAUCET_PER_IP_PER_DAY": "2", "FAUCET_PER_NET_PER_DAY": "3", "FAUCET_MAX_WAITING": "2",
        "FAUCET_WAIT": "1", "FAUCET_TOOL_GRACE": "3", "FAUCET_RESEND_AFTER": "1",
        "FAUCET_TOOL": str(tool), "FAUCET_STATE": str(tmp / "faucet"), "FAUCET_ACCOUNTS": str(tmp / "accounts.json"),
        "FAUCET_KEY": str(tmp / "nokey"),
    })
    loader = importlib.machinery.SourceFileLoader("gateway", str(GW))
    spec = importlib.util.spec_from_loader("gateway", loader)
    gw = importlib.util.module_from_spec(spec)
    loader.exec_module(gw)
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", gw_port), gw.Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{gw_port}"
    ok = 0

    def check(cond, what):
        nonlocal ok
        if not cond:
            raise SystemExit(f"FAIL: {what}")
        ok += 1
        print(f"ok {ok}: {what}")

    c, h, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 1, "method": "getLastBlockId", "params": []}, {"X-Real-IP": "198.51.100.7"})
    check(c == 200 and b["result"] == 42, "an allowed read is forwarded")
    check(h.get("Access-Control-Allow-Origin") == "*", "CORS header on the answer")
    c, _, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 2, "method": "requeueCrossZoneDeadLetter", "params": ["00"]})
    check(c == 403 and b["error"]["code"] == -32601, "an operator method is refused")
    c, _, b = post(base + "/rpc", [{"jsonrpc": "2.0", "id": i, "method": "getLastBlockId"} for i in range(21)])
    check(c == 400, "a batch over 20 is refused")
    c, _, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 3, "method": "getBlockRange", "params": [1, 500]})
    check(c == 400, "a getBlockRange over 50 blocks is refused")
    c, _, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 3, "method": "getBlockRange",
                                   "params": {"start_block_id": 1, "end_block_id": 52}})
    check(c == 400 and "50 blocks" in b["error"]["message"], "the same in the named-parameter form")
    c, _, b = post(base + "/rpc", [{"jsonrpc": "2.0", "id": 4, "method": "getBlock", "params": [1]},
                                   {"jsonrpc": "2.0", "id": 5, "method": "checkHealth"}])
    check(c == 200 and isinstance(b, list) and len(b) == 2, "a batch of allowed reads is forwarded whole")
    c, _, _ = post(base + "/", {"jsonrpc": "2.0", "id": 6, "method": "getLastBlockId"}, {"Host": "rpc.example.test"})
    check(c == 200, "POST / on the rpc. host is JSON-RPC")
    c, _, _ = post(base + "/", {"jsonrpc": "2.0", "id": 6, "method": "getLastBlockId"}, {"Host": "example.test"})
    check(c == 404, "POST / on the main host is not")
    req = urllib.request.Request(base + "/rpc", method="OPTIONS")
    with urllib.request.urlopen(req, timeout=5) as r:
        check(r.status == 204 and r.headers["Access-Control-Allow-Methods"].startswith("POST"), "CORS preflight")
    sends = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 7, "method": "sendTransaction", "params": [{}]},
                  {"X-Real-IP": "203.0.113.9"})[0] for _ in range(3)]
    check(sends[:2] == [200, 200] and sends[2] == 429, "sendTransaction is limited per IP")
    sends = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 7, "method": "sendTransaction", "params": [{}]},
                  {"X-Real-IP": "203.0.113.10"})[0] for _ in range(2)]
    c, h, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 7, "method": "sendTransaction", "params": [{}]},
                   {"X-Real-IP": "203.0.113.11"})
    check(sends == [200, 200] and c == 429 and "network-wide" in b["error"]["message"] and h.get("Retry-After"),
          "sendTransaction is capped network-wide: a third IP waits once the global budget is spent")
    rng = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 10, "method": "getBlockRange", "params": [1, 50]},
                {"X-Real-IP": "192.0.2.70"})[0] for _ in range(6)]
    check(rng == [200] * 5 + [429], "getBlockRange is charged by its span (50 blocks: 6 tokens; 5 fit a burst of 30)")
    ids = [f"Acc{i:029d}" for i in range(65)]
    c, _, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 11, "method": "getAccountsNonces", "params": [ids]},
                   {"X-Real-IP": "192.0.2.71"})
    check(c == 400 and "64 entries" in b["error"]["message"], "a list parameter over 64 entries is refused")
    c, _, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 11, "method": "getProofsAndRoot",
                                   "params": {"commitments": ids, "message_positions": []}}, {"X-Real-IP": "192.0.2.71"})
    check(c == 400, "the same in the named-parameter form")
    lst = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 12, "method": "getAccountsNonces", "params": [ids[:20]]},
                {"X-Real-IP": "192.0.2.72"})[0] for _ in range(3)]
    check(lst == [200, 200, 429], "list entries beyond 10 are charged (20 entries: 11 tokens)")
    big = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 13, "method": "getBlock", "params": [7777]},
                {"X-Real-IP": "192.0.2.73"}) for _ in range(3)]
    check([x[0] for x in big] == [200, 429, 429] and "budget" in big[1][2]["error"]["message"] and big[1][1].get("Retry-After"),
          "response bytes are budgeted per IP: a 30 KB answer is sent while the 50 KB budget holds it; the next waits")
    c, _, _ = post(base + "/rpc", {"jsonrpc": "2.0", "id": 13, "method": "getBlock", "params": [7777]}, {"X-Real-IP": "192.0.2.74"})
    check(c == 200, "another IP has its own byte budget")
    c, _, b = post(base + "/rpc", {"jsonrpc": "2.0", "id": 14, "method": "getBlock", "params": [8888]}, {"X-Real-IP": "192.0.2.75"})
    check(c == 400 and "exceed" in b["error"]["message"], "an answer over the response cap is not passed on")

    def builds(block):  # the answers the stand-in sequencer has built for getBlock(block)
        return sum(1 for q in Upstream.seen for r in (q if isinstance(q, list) else [q])
                   if r.get("method") == "getBlock" and (r.get("params") or [None])[0] == block)
    n0 = builds(7777)
    codes = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 24, "method": "getBlock", "params": [7777]},
                  {"X-Real-IP": "192.0.2.78"})[0] for _ in range(8)]
    n = builds(7777) - n0
    check(codes == [200] + [429] * 7 and n == 2, "an answer refused for its IP's byte budget is charged all the same: 8 "
          f"requests in a row for a 30 KB answer, a 50 KB budget: the sequencer builds 2 answers, not 8 (built: {n})")
    n0 = builds(8888)
    codes = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 25, "method": "getBlock", "params": [8888]},
                  {"X-Real-IP": "192.0.2.79"})[0] for _ in range(5)]
    n = builds(8888) - n0
    check(codes == [400] + [429] * 4 and n == 1, "an answer over the cap is charged to its IP for the bytes the sequencer "
          f"produced: 5 requests in a row, 1 answer built (built: {n})")
    saved = {k: gw.F[k] for k in ("GATEWAY_BYTES_RATE", "GATEWAY_BYTES_GLOBAL_RATE")}
    gw.F.update({"GATEWAY_BYTES_RATE": 20000, "GATEWAY_BYTES_GLOBAL_RATE": 1e6})  # an IP's budget refills at 20 KB/s
    n0, got, t0 = builds(7777), [], time.monotonic()
    while time.monotonic() - t0 < 8 and got.count(200) < 2:
        got.append(post(base + "/rpc", {"jsonrpc": "2.0", "id": 26, "method": "getBlock", "params": [7777]},
                        {"X-Real-IP": "192.0.2.80"})[0])
        time.sleep(0.2)
    gw.F.update(saved)
    n = builds(7777) - n0
    check(got.count(200) == 2 and got[0] == 200 and n == 3, "a client asking every 0.2 s for an answer its budget cannot "
          "pay gets it once the budget has refilled by its size; the sequencer builds nothing for the requests before "
          f"(built: {n}; answers: {got})")
    saved = {k: gw.F[k] for k in ("GATEWAY_SMALL_GLOBAL_BURST", "GATEWAY_SMALL_GLOBAL_RATE")}
    gw.F.update({"GATEWAY_SMALL_GLOBAL_BURST": 100, "GATEWAY_SMALL_GLOBAL_RATE": 0.001})
    gw.BUCKETS.state.pop(("bytes", "small"), None)
    small = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 27, "method": "checkHealth"}, {"X-Real-IP": f"192.0.2.{120 + i}"})
             for i in range(3)]
    gw.F.update(saved)
    gw.BUCKETS.state.pop(("bytes", "small"), None)
    check([x[0] for x in small] == [200, 200, 429] and "budget" in small[2][2]["error"]["message"],
          "small answers have a network-wide budget of their own: three clients' 42-byte answers, a 100-byte budget: "
          "the third waits")
    out = []
    th = [threading.Thread(target=lambda: out.append(post(base + "/rpc", {"jsonrpc": "2.0", "id": 18, "method": "getBlock",
                                                                          "params": [7778]}, {"X-Real-IP": "192.0.2.76"})))
          for _ in range(6)]
    for t in th:
        t.start()
    for t in th:
        t.join()
    served = sum(int(h["Content-Length"]) for c, h, _ in out if c == 200)
    check(len(out) == 6 and all(c in (200, 429) for c, _, _ in out) and 59000 < served <= 60000,
          "an answer larger than the budget (59 KB, 50 KB budget) goes alone, sent while the budget is full")
    # Answers larger than the reserve held while they are forwarded: two 30 KB answers in flight together (the IP's
    # 2 slots), a 4 KB reserve each, a 50 KB budget: the second does not fit once the first is sent.
    gw.F["GATEWAY_BYTES_RESERVE"] = 4000
    out = []
    th = [threading.Thread(target=lambda: out.append(post(base + "/rpc", {"jsonrpc": "2.0", "id": 19, "method": "getBlock",
                                                                          "params": [7779]}, {"X-Real-IP": "192.0.2.77"})))
          for _ in range(2)]
    for t in th:
        t.start()
    for t in th:
        t.join()
    gw.F["GATEWAY_BYTES_RESERVE"] = 262144
    served = sum(int(h["Content-Length"]) for c, h, _ in out if c == 200)
    check(len(out) == 2 and 29000 < served <= 50000 and sorted(c for c, _, _ in out) == [200, 429],
          "two answers larger than the reserve in flight at once to one IP: only what its budget holds is sent")
    # An IP's budget is a share (a quarter) of the network-wide one, and small answers pass when that is used up.
    saved = {k: gw.F[k] for k in ("GATEWAY_BYTES_GLOBAL_BURST", "GATEWAY_BYTES_GLOBAL_RATE")}
    # 60,100 B: two 30,042-byte answers fit, and less than a 42-byte answer is left after them.
    gw.F.update({"GATEWAY_BYTES_GLOBAL_BURST": 60100, "GATEWAY_BYTES_GLOBAL_RATE": 1})
    gw.BUCKETS.state.pop(("bytes", "*"), None)
    one = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 20, "method": "getBlock", "params": [7777]},
                {"X-Real-IP": "2001:db8:a::1"})[0] for _ in range(2)]
    check(one == [200, 429], "an IP's byte budget is a quarter of the network-wide one (15 KB of 60 KB: one 30 KB "
          "answer while full, then it waits)")
    c1 = post(base + "/rpc", {"jsonrpc": "2.0", "id": 21, "method": "getBlock", "params": [7777]}, {"X-Real-IP": "2001:db8:b::1"})[0]
    c2, h2, b2 = post(base + "/rpc", {"jsonrpc": "2.0", "id": 22, "method": "getBlock", "params": [7777]}, {"X-Real-IP": "2001:db8:c::1"})
    c3, _, b3 = post(base + "/rpc", {"jsonrpc": "2.0", "id": 23, "method": "checkHealth"}, {"X-Real-IP": "2001:db8:d::1"})
    check(c1 == 200 and c2 == 429 and "budget" in b2["error"]["message"] and h2.get("Retry-After") and c3 == 200 and b3["result"] == 42,
          "two clients use up the network-wide budget: a third client's large answer waits, a fourth's small call passes")
    gw.F.update(saved)
    gw.BUCKETS.state.pop(("bytes", "*"), None)
    codes = [post(base + "/rpc", {"jsonrpc": "2.0", "id": 8, "method": "checkHealth"}, {"X-Real-IP": "192.0.2.1"})[0]
             for _ in range(31)]
    check(codes[:30] == [200] * 30 and codes[30] == 429, "reads are limited per IP")
    c, _, _ = post(base + "/rpc", {"jsonrpc": "2.0", "id": 9, "method": "checkHealth"}, {"X-Real-IP": "192.0.2.2"})
    check(c == 200, "another IP has its own bucket")
    conn = http.client.HTTPConnection("127.0.0.1", gw_port, timeout=5)
    conn.putrequest("POST", "/rpc")
    for k, v in {"Content-Type": "application/json", "Content-Length": "500000", "X-Real-IP": "192.0.2.1"}.items():
        conn.putheader(k, v)
    conn.endheaders()  # and no body: a gateway that read the body first would wait
    r = conn.getresponse()
    check(r.status == 429, "a limited IP is refused before its body is read")
    conn.close()
    conn = http.client.HTTPConnection("127.0.0.1", gw_port, timeout=8)
    conn.putrequest("POST", "/rpc")
    for k, v in {"Content-Type": "application/json", "Content-Length": "100", "X-Real-IP": "192.0.2.3"}.items():
        conn.putheader(k, v)
    conn.endheaders()
    t0 = time.monotonic()
    try:
        r = conn.getresponse()
        closed = r.read() == b""
    except (http.client.RemoteDisconnected, ConnectionError):
        closed = True
    check(closed and time.monotonic() - t0 < 6, "a client that stops sending is dropped after the socket timeout")
    conn.close()
    slow = []
    th = [threading.Thread(target=lambda i=i: slow.append(post(base + "/rpc", {"jsonrpc": "2.0", "id": 15, "method": "getBlock",
                                                                             "params": [5555]}, {"X-Real-IP": f"192.0.2.{90 + i}"})[0]))
          for i in range(3)]
    for t in th:
        t.start()
    time.sleep(0.5)
    c, h, _ = post(base + "/rpc", {"jsonrpc": "2.0", "id": 16, "method": "checkHealth"}, {"X-Real-IP": "192.0.2.95"})
    for t in th:
        t.join()
    check(c == 503 and h.get("Retry-After") and slow == [200] * 3, "requests beyond the concurrency cap get 503 at once")
    health = {"jsonrpc": "2.0", "id": 17, "method": "checkHealth"}
    raw = json.dumps(health).encode()
    held = []
    for _ in range(2):  # bodies held back: the IP's two slots taken
        conn = http.client.HTTPConnection("127.0.0.1", gw_port, timeout=8)
        conn.putrequest("POST", "/rpc")
        for k, v in {"Content-Type": "application/json", "Content-Length": str(len(raw)), "X-Real-IP": "192.0.2.110"}.items():
            conn.putheader(k, v)
        conn.endheaders()
        held.append(conn)
    time.sleep(0.5)
    c, h, b = post(base + "/rpc", health, {"X-Real-IP": "192.0.2.110"})
    c2, _, _ = post(base + "/rpc", health, {"X-Real-IP": "192.0.2.111"})
    check(c == 429 and h.get("Retry-After") and "at once" in b["error"]["message"] and c2 == 200,
          "an IP holding its 2 slots gets 429 at once for a third, without a shared slot, and another IP is still served")
    codes = []
    for conn in held:
        conn.send(raw)
        r = conn.getresponse()
        codes.append(r.status)
        r.read()
        conn.close()
    c, _, _ = post(base + "/rpc", health, {"X-Real-IP": "192.0.2.110"})
    check(codes == [200, 200] and c == 200, "the held requests are answered once their bodies come, and their slots given back")
    c, _, b = post(base + "/faucet", {"account": "not an account!"})
    check(c == 400 and not b["ok"], "the faucet refuses a malformed account")
    acct = "Public/" + "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYib"
    old_tool, old_runs = tmp / "old-tool", tmp / "old-runs.log"  # a tool from before records: it ignores --record
    old_tool.write_text(f'#!/bin/sh\ncase "$1" in transfer) echo "$*" >> {old_runs}; echo \'{{"hash":"ab","credited":true}}\' ;; '
                        '*) echo "unknown command $1: keygen accounts sign send status target inspect verify fund transfer" >&2; '
                        'exit 1 ;; esac\n')
    old_tool.chmod(old_tool.stat().st_mode | stat.S_IEXEC)
    gw.CFG["FAUCET_TOOL"] = str(old_tool)
    c, h, b = post(base + "/faucet", {"account": acct}, {"X-Real-IP": "192.0.2.49"})
    gw.CFG["FAUCET_TOOL"] = str(tool)
    check(c == 503 and h.get("Retry-After") and not old_runs.exists(),
          "no claim is made with a tool that does not record its transfers (an older ferrata-day1)")
    c, _, b = post(base + "/faucet", {"account": acct}, {"X-Real-IP": "192.0.2.50"})
    check(c == 200 and b["ok"] and b["credited"] and b["hash"] in Upstream.chain["applied"],
          "the faucet sends through its tool, and the claim is paid by its own transaction")
    c, _, b = post(base + "/faucet", {"account": acct}, {"X-Real-IP": "192.0.2.51"})
    check(c == 429 and "recently" in b["error"], "one claim per account per day, whatever the IP")
    fresh = "3Zz7e5YPLyxJzG5zHk9kqKDS8vN3yXVhFgk3wTKVcGd1"
    c, _, b = post(base + "/faucet", {"account": fresh}, {"X-Real-IP": "192.0.2.60"})
    check(c == 400 and "public_key" in b["error"], "a never-used account without its public key is told what to send")
    c, _, b = post(base + "/faucet", {"account": fresh, "public_key": "zz"}, {"X-Real-IP": "192.0.2.60"})
    check(c == 400 and "64 hex" in b["error"], "a malformed public key is refused")
    c, _, b = post(base + "/faucet", {"account_id": fresh, "public_key": "ab" * 32}, {"X-Real-IP": "192.0.2.61"})
    check(c == 200 and b["ok"] and b["pk"] == "yes", "with its public key the claim goes through, the key passed to the tool")
    fail = "FaiL" * 9 + "1"
    c, _, b = post(base + "/faucet", {"account": fail}, {"X-Real-IP": "198.51.100.80"})
    check(c == 502 and not b["ok"] and "secret" not in json.dumps(b) and "/var/lib" not in json.dumps(b),
          "a failed transfer is reported without the tool's output")
    c, _, b = post(base + "/faucet", {"account": fail}, {"X-Real-IP": "198.51.100.80"})
    c2, _, b2 = post(base + "/faucet", {"account": "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYic"}, {"X-Real-IP": "198.51.100.80"})
    check(c == 502 and c2 == 429 and "today" in b2["error"], "failed claims count against the IP's daily claims")
    c, _, b = post(base + "/faucet", {"account": "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYid"}, {"X-Real-IP": "198.51.100.81"})
    c2, _, b2 = post(base + "/faucet", {"account": "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYie"}, {"X-Real-IP": "198.51.100.82"})
    check(c == 200 and c2 == 429 and "network" in b2["error"], "and against its /24 (3 a day here)")
    done = {}

    def claim(key, account, ip):
        done[key] = post(base + "/faucet", {"account": account}, {"X-Real-IP": ip})[0]
    first = threading.Thread(target=claim, args=("a", "SLoW" * 9 + "1", "203.0.113.40"))  # its transfer takes 2 s
    first.start()
    time.sleep(0.5)
    c, h, b = post(base + "/faucet", {"account": "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYif"}, {"X-Real-IP": "203.0.113.40"})
    check(c == 429 and h.get("Retry-After") and "in progress" in b["error"]["message"],
          "one claim at a time per IP: a second from the same IP gets 429 at once")
    ledger = sqlite3.connect(tmp / "faucet" / "claims.sqlite")
    pending = [ok for (ok,) in ledger.execute("SELECT ok FROM claims WHERE account=?", ("SLoW" * 9 + "1",))]
    ledger.close()
    check(pending == [3], "a claim is in the ledger (pending) while its transfer runs")
    second = threading.Thread(target=claim, args=("b", "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYif", "203.0.113.41"))
    second.start()
    time.sleep(0.5)
    c, h, b = post(base + "/faucet", {"account": "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYih"}, {"X-Real-IP": "203.0.113.42"})
    first.join()
    second.join()
    check(c == 429 and "busy" in b["error"]["message"] and done == {"a": 200, "b": 200},
          "so another IP's claim waits its turn and is paid; one transfer at a time, more than 2 waiting get 429 at once")
    amount = 10000000000

    def seq(method, *params):  # the stand-in sequencer, directly
        return post(f"http://127.0.0.1:{up_port}", {"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)})[2]["result"]

    def ledger_ok(account):
        db = sqlite3.connect(tmp / "faucet" / "claims.sqlite")
        try:
            return [ok for (ok,) in db.execute("SELECT ok FROM claims WHERE account=? ORDER BY t", (account,))]
        finally:
            db.close()

    def paid_to(account):  # the faucet's transfers to the account that were applied
        return sum(1 for tx in Upstream.chain["applied"].values() if tx["to"] == account)

    def claim_from(account, net):  # a claim from a network of its own (IPv6 /64)
        return post(base + "/faucet", {"account": account}, {"X-Real-IP": f"2001:db8:{net}::1"})
    slow_acct = "TiMe" * 9 + "1"  # sent into the pool, then the tool hangs: killed after FAUCET_WAIT + FAUCET_TOOL_GRACE
    t0 = time.monotonic()
    c, _, b = post(base + "/faucet", {"account": slow_acct}, {"X-Real-IP": "203.0.113.41"})
    check(c == 504 and not b["ok"] and time.monotonic() - t0 < 9 and ledger_ok(slow_acct) == [2],
          "a transfer not in a block in time gets 504 and is booked as sent, not settled")
    c, _, b = post(base + "/faucet", {"account": slow_acct}, {"X-Real-IP": "192.0.2.64"})
    check(c == 429 and "not settled" in b["error"] and "transfer-resend" in calls.read_text(),
          "it may still be applied, so it is the account's claim: refused from another network (its bytes sent again)")
    c, _, b = post(base + "/faucet", {"account": "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYij"}, {"X-Real-IP": "203.0.113.41"})
    check(c == 429 and "address" in b["error"], "and one of the IP's daily claims")
    c, _, b = post(base + "/faucet", {"account": old_timeout}, {"X-Real-IP": "192.0.2.65"})
    check(c == 429 and "not settled" in b["error"],
          "a timeout in a ledger written before records (ok=0, 'timeout') is migrated and counts too")
    check(ledger_ok("SLoW" * 9 + "1") == [1], "and the slow claim booked paid once its transfer answered")
    lost = "LoST" * 9 + "1"  # applied, the tool's answer lost
    c, _, b = claim_from(lost, 1)
    c2, _, b2 = claim_from(lost, 2)
    check(c == 200 and b["ok"] and b["credited"] and b["hash"] in Upstream.chain["applied"] and c2 == 429
          and "recently" in b2["error"], "a transfer whose answer is lost is found by its own hash: paid, and the account's claim")
    late = "LaTE" * 9 + "1"  # in the pool, the tool's answer lost: applied after the gateway looked
    c, _, b = claim_from(late, 3)
    pending_then = ledger_ok(late)
    seq("_mine")
    c2, _, b2 = claim_from(late, 4)
    check(c == 504 and pending_then == [2] and paid_to(late) == 1 and c2 == 429 and "recently" in b2["error"]
          and ledger_ok(late) == [1] and runs.read_text().split().count(late) == 1,
          "a transfer applied after the gateway looked (504) is found by its hash at the account's next claim: booked "
          "paid, no second transfer")
    third = "ThRD" * 9 + "1"  # its transfer lost on the way; another source pays the account the faucet's amount
    c, _, b = claim_from(third, 5)
    check(c == 504 and ledger_ok(third) == [2] and Upstream.balances[third] == 42 + amount and paid_to(third) == 0,
          "another source paying the account the faucet's amount is not taken for the faucet's transfer: still pending")
    # A claim interrupted after its record (the gateway stopped there): pending in the ledger, its transaction never sent.
    interrupted, unsigned = "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYik", "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYim"
    tx = {"nonce": Upstream.chain["nonce"], "to": interrupted, "amount": amount, "salt": "00"}
    tx["hash"] = hashlib.sha256(json.dumps(tx, sort_keys=True).encode()).hexdigest()
    rec = tmp / "faucet" / "records" / "interrupted.json"
    rec.write_text(json.dumps({"hash": tx["hash"], "from": "FaucetAccount1111111111111111111111111111111",
                               "nonce": str(tx["nonce"]), "tx": json.dumps(tx).encode().hex()}))
    db = sqlite3.connect(tmp / "faucet" / "claims.sqlite")
    db.execute("INSERT INTO claims VALUES (?,?,?,3,'',?)", (time.time() - 30, "2001:db8:20::1", interrupted,
                                                             json.dumps({"record": str(rec)})))
    db.execute("INSERT INTO claims VALUES (?,?,?,3,'',?)", (time.time() - 30, "2001:db8:21::1", unsigned,
                                                             json.dumps({"record": str(tmp / "faucet" / "records" / "none.json")})))
    db.commit()
    db.close()
    other = "2H49KF7DjgGJ2hq7QemPsm7FX7uSejtcQnf43gSnWYip"
    c, _, _ = claim_from(other, 6)  # another account's transfer spends the nonce the two pending ones were signed with
    c2, _, b2 = claim_from(third, 7)
    check(c == 200 and c2 == 200 and b2["credited"] and ledger_ok(third) == [0, 1] and paid_to(third) == 1
          and Upstream.balances[third] == 42 + 2 * amount,
          "once another transaction spent its nonce, that transfer never can be applied: the account is paid at its next claim, once")
    c, _, b = claim_from(interrupted, 8)
    check(c == 200 and b["credited"] and ledger_ok(interrupted) == [0, 1] and paid_to(interrupted) == 1,
          "a claim interrupted after its record does not hold the account once its nonce is spent: paid at its next claim")
    c, _, b = claim_from(unsigned, 9)
    check(c == 200 and ledger_ok(unsigned) == [0, 1] and paid_to(unsigned) == 1,
          "a claim interrupted before its record (nothing signed) does not hold the account either")
    c, _, b = claim_from(slow_acct, 10)
    check(c == 200 and ledger_ok(slow_acct) == [0, 1] and paid_to(slow_acct) == 1,
          "nor does the transfer that did not settle in time, once its nonce is spent")
    never = "NeVeR" * 7 + "1"  # the tool stopped after its record: the transaction never left
    c, _, _ = claim_from(never, 11)
    time.sleep(1.2)  # FAUCET_RESEND_AFTER: 1 s here
    c2, _, b2 = claim_from(never, 12)
    c3, _, b3 = claim_from(never, 13)
    check(c == 504 and c2 == 429 and "not settled" in b2["error"] and c3 == 429 and "recently" in b3["error"]
          and paid_to(never) == 1 and runs.read_text().split().count(never) == 1 and ledger_ok(never) == [1],
          "a transfer that never left, its nonce unspent, is sent again (its own bytes) at the account's next claim: "
          "paid once, one transfer signed")
    n0 = sum(1 for q in Upstream.seen for r in (q if isinstance(q, list) else [q]) if r.get("method") == "getAccountBalance")
    for _ in range(3):
        with urllib.request.urlopen(base + "/faucet", timeout=5) as r:
            info = json.loads(r.read())
    n1 = sum(1 for q in Upstream.seen for r in (q if isinstance(q, list) else [q]) if r.get("method") == "getAccountBalance")
    check(info["faucet_account"].startswith("Faucet") and info["amount"] == "10000000000"
          and info["network"]["public_send_per_second"] > 0, "the faucet's terms, with the network-wide send cap")
    check(n1 - n0 <= 1, "GET /faucet asks the sequencer at most once per 10 s")
    print(f"all {ok} checks passed")


if __name__ == "__main__":
    main()
