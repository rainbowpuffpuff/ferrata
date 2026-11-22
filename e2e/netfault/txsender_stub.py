#!/usr/bin/env python3
"""txsender_stub: a stand-in for Logos Core's `tx_sender_module` over one local anvil, so that the relayer daemon's
`--send-via logos` route can be killed and restarted as a process (cli/tests/relayer_crash.rs part 5; RFP-022 R.06),
and the monitor's fallback run against what the module cannot account for (monitor/tests/logos-fallback.sh).
Tests only: the real route runs inside ferrata_relayer_module with Logos Core's own modules
(modules/scripts/relayer-module.sh; modules/results/send-stack-2026-11-14/).

  txsender_stub.py --upstream http://127.0.0.1:<anvil> --port-file F --ctl C --log L [--history H]

One HTTP endpoint on 127.0.0.1 (its port written to F): POST {"module": "tx_sender_module", "method": M, "args": [...]}
answers what the module's method M answers (logos-evm-tx-sender-module f1754c3, rust-lib/src/glue.rs; the subset the
daemon calls):
  send(request_json)       reserves the account's next nonce (one ledger, as the module's), or the request's pinned
                           `nonce` (refused while a request not broadcast yet holds it), and answers
                           {ok, pending: true, requestId}; nothing is signed or broadcast
  send_status(requestId)   the broadcast (the module has no background advancer: this call IS the broadcast): the
                           history row written first (as the module's "write ahead"), then the call sent with its
                           reserved nonce from the request's account (anvil signs: the test impersonates it) ->
                           {ok, status: "broadcast", final: true, hashes: [h], hash: h, legs}; asked again for a request
                           already broadcast, the same answer; for a request it does not hold (made before a restart):
                           {ok: false, final: true}, as the module
  cancel_send(requestId)   withdraws a request not broadcast yet
  live_sends()             the requests not broadcast yet: {ok, sends: [{requestId, chainId, from, status, purpose}]}
  history(address, chain)  the durable rows (H, JSONL, read back at start; the newest row per request):
                           {ok, transactions: [{requestId, leg, nonce, hash, status, chainId, from, txInput, purpose}]}
A restarted stand-in with the same H is the module restarted: its requests are gone (in memory), its history and the
nonces it holds (burnt: every row's) are not. Request ids carry the process's own tag, so a restarted stand-in never
reuses one. Approval is immediate (as under a custodian's unlock without confirmation). Faults, one-shot, read from C
before each call (one line; `pass` or missing: none):
  hold send SECS           `send` reserves the request, then holds its answer SECS (the test kills the daemon meanwhile:
                           it never learns the request's id)
  stall send_status        `send_status` holds the connection, broadcasting nothing, until the client goes away (the
                           daemon killed after its request was made, before any broadcast)
  hold send_status SECS    `send_status` broadcasts, then holds its answer SECS (killed: it never learns the hash)
  stuck send_status        `send_status` writes the row and broadcasts, and the request is `stuck` from then on (no
                           hash: as a broadcast whose RPC never answered): {ok, status: "stuck", final: true, hashes: [],
                           legs: [{nonce, left: true}]}
  stuck-unsent send_status the same, the call never sent
  fail send_status         `send_status` writes the row, and the broadcast answers an error, nothing sent: as the module
                           when its node refuses or cannot be reached (glue.rs `advance_send`: the leg had begun to
                           leave, the row is left `unknown`, the number stays burnt and is never handed out again) ->
                           {ok, status: "failed", final: true, hashes: [], legs: [{nonce, left: true}], reason}
Each fault that fires appends a line to C + ".hit"; every call goes to L (JSONL). The upstream must be a loopback
address.
"""
import argparse
import json
import os
import threading
import time
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--port-file", required=True)
    ap.add_argument("--ctl", required=True)
    ap.add_argument("--log", required=True)
    ap.add_argument("--history")
    a = ap.parse_args()
    host = urllib.parse.urlparse(a.upstream).hostname
    if host not in ("127.0.0.1", "localhost", "::1"):
        raise SystemExit(f"txsender_stub: the upstream must be a loopback address, not {host}")
    requests = {}  # id -> {from, chainId, to, data, label, purpose, nonce, status, hash}
    ledger = {}  # (chainId, from) -> next nonce reserved
    counter = [0]
    tag = os.urandom(3).hex()
    rows = {}  # requestId -> its newest history row
    if a.history and os.path.exists(a.history):
        with open(a.history) as f:
            for line in f:
                if line.strip():
                    r = json.loads(line)
                    rows[r["requestId"]] = r
        # As the module's restart: the numbers its history holds are burnt (never taken again by an unpinned send).
        for r in rows.values():
            key = (r["chainId"], r["from"].lower())
            ledger[key] = max(ledger.get(key, 0), r["nonce"] + 1)

    def write_row(rid, r, hash_, status):
        row = {"requestId": rid, "leg": 0, "nonce": r["nonce"], "hash": hash_ or "", "status": status,
               "chainId": r["chainId"], "from": r["from"], "to": r["to"], "txInput": r["data"],
               "purpose": r["purpose"]}
        rows[rid] = row
        if a.history:
            with open(a.history, "a") as f:
                f.write(json.dumps(row) + "\n")
                f.flush()
                os.fsync(f.fileno())

    def log(method, action, **kw):
        with LOCK, open(a.log, "a") as f:
            f.write(json.dumps({"t": time.time(), "method": method, "action": action, **kw}) + "\n")

    def control():
        try:
            words = open(a.ctl).read().split()
        except FileNotFoundError:
            return ["pass"]
        return words or ["pass"]

    def fired(what):
        with open(a.ctl, "w") as f:
            f.write("pass\n")
        with open(a.ctl + ".hit", "a") as f:
            f.write(json.dumps({"t": time.time(), "fired": what}) + "\n")

    def rpc(method, params):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        req = urllib.request.Request(a.upstream, data=body, method="POST", headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=60) as r:
            v = json.loads(r.read())
        if "error" in v:
            raise RuntimeError(json.dumps(v["error"]))
        return v["result"]

    def status_reply(rid, r):
        leg = {"to": r["to"], "nonce": r["nonce"], "label": r["label"],
               "left": r["status"] in ("broadcast", "stuck") or r.get("left", False)}
        if r["status"] == "stuck":
            return {"ok": True, "requestId": rid, "status": "stuck", "final": True, "hashes": [], "legs": [leg],
                    "reason": "the broadcast has not answered"}
        if r["status"] == "broadcast":
            leg["hash"] = r["hash"]
            return {"ok": True, "requestId": rid, "status": "broadcast", "final": True, "hashes": [r["hash"]],
                    "hash": r["hash"], "legs": [leg]}
        if r["status"] == "failed":
            return {"ok": True, "requestId": rid, "status": "failed", "final": True, "hashes": [], "legs": [leg],
                    "reason": r.get("reason", "")}
        return {"ok": True, "requestId": rid, "status": r["status"], "final": r["status"] != "awaitingApproval",
                "hashes": [], "legs": [leg]}

    def send(args):
        req = json.loads(args[0])
        call = req["calls"][0]
        key = (req["chainId"], req["from"].lower())
        with LOCK:
            pinned = req.get("nonce")
            if pinned is not None:
                held = [rid for rid, r in requests.items() if (r["chainId"], r["from"].lower()) == key
                        and r["nonce"] == pinned and r["status"] == "awaitingApproval"]
                if held:
                    return None, {"ok": False, "error": f"nonce {pinned} is already held by a send that has not been "
                                                        "broadcast; a nonce may only be pinned to replace a "
                                                        "transaction that has already left"}
                nonce = int(pinned)
                ledger[key] = max(ledger.get(key, 0), nonce + 1)
            else:
                chain_next = int(rpc("eth_getTransactionCount", [req["from"], "pending"]), 16)
                nonce = max(ledger.get(key, 0), chain_next)
                ledger[key] = nonce + 1
            counter[0] += 1
            rid = f"r{counter[0]}-{tag}"
            requests[rid] = {"from": req["from"], "chainId": req["chainId"], "to": call["to"], "data": call["data"],
                             "label": call.get("label", ""), "purpose": req.get("purpose", ""), "nonce": nonce,
                             "status": "awaitingApproval", "hash": None}
        return rid, {"ok": True, "pending": True, "requestId": rid, "handle": f"h-{rid}"}

    def broadcast(rid, stuck=None, fail=False):
        with LOCK:
            r = requests.get(rid)
            if r is None:
                return {"ok": False, "error": f"no send with id '{rid}'", "final": True}
            if r["status"] == "awaitingApproval":
                write_row(rid, r, None, "intent")  # written ahead: a row means it may have left
                if fail:
                    # The module's `leave_unknown`, then `Failed`: no hash, the leg left, its nonce spent.
                    r["status"], r["left"] = "failed", True
                    r["reason"] = "the node refused the transaction (the stand-in's fault: nothing was sent)"
                    write_row(rid, r, None, "unknown")
                    return status_reply(rid, r)
                tx = {"from": r["from"], "to": r["to"], "data": r["data"], "nonce": hex(r["nonce"])}
                try:
                    if stuck != "unsent":
                        h = rpc("eth_sendTransaction", [tx])
                        if stuck is None:
                            r["hash"] = h
                            write_row(rid, r, h, "pending")
                    r["status"] = "stuck" if stuck else "broadcast"
                except Exception as e:  # noqa: BLE001
                    r["status"], r["reason"] = "failed", str(e)
            return status_reply(rid, r)

    class H(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def reply(self, v):
            data = json.dumps(v).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def until_client_gone(self, secs):
            self.connection.settimeout(1.0)
            t0 = time.time()
            while time.time() - t0 < secs:
                try:
                    if not self.connection.recv(1, 0x02):  # MSG_PEEK: b"" once the client closed
                        return True
                except TimeoutError:
                    continue
                except OSError:
                    return True
            return False

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
            method, args = body.get("method", ""), body.get("args", [])
            ctl = control()
            fault = ctl[0] if len(ctl) > 1 and ctl[1] == method else None
            if body.get("module") != "tx_sender_module":
                log(method, "refused (not tx_sender_module)", module=body.get("module"))
                return self.reply({"ok": False, "error": f"no module {body.get('module')} here"})
            if method == "send":
                rid, v = send(args)
                if rid is None:
                    log(method, "refused", reason=v.get("error"))
                    return self.reply(v)
                if fault == "hold":
                    secs = float(ctl[2]) if len(ctl) > 2 else 20.0
                    fired(f"hold send {secs} ({rid})")
                    log(method, f"request {rid} made, answer held {secs} s", requestId=rid)
                    if self.until_client_gone(secs):
                        log(method, f"held answer for {rid} not delivered (client gone)", requestId=rid)
                        return None
                log(method, "request made", requestId=rid, nonce=requests[rid]["nonce"], label=requests[rid]["label"],
                    pinned=json.loads(args[0]).get("nonce") is not None)
                return self.reply(v)
            if method == "send_status":
                rid = args[0] if args else ""
                if fault == "stall":
                    fired(f"stall send_status ({rid})")
                    log(method, f"stalled, nothing broadcast for {rid}", requestId=rid)
                    self.until_client_gone(600)
                    log(method, f"stalled call's client gone; {rid} still open", requestId=rid)
                    return None
                stuck = None
                if fault in ("stuck", "stuck-unsent"):
                    stuck = "unsent" if fault == "stuck-unsent" else "sent"
                    fired(f"{fault} send_status ({rid})")
                if fault == "fail":
                    fired(f"fail send_status ({rid})")
                v = broadcast(rid, stuck, fault == "fail")
                if fault == "hold":
                    secs = float(ctl[2]) if len(ctl) > 2 else 20.0
                    fired(f"hold send_status {secs} ({rid})")
                    log(method, f"{rid} {v.get('status')}, answer held {secs} s", requestId=rid, hash=v.get("hash"))
                    if self.until_client_gone(secs):
                        log(method, f"held answer for {rid} not delivered (client gone)", requestId=rid)
                        return None
                log(method, v.get("status", "refused"), requestId=rid, hash=v.get("hash"))
                return self.reply(v)
            if method == "cancel_send":
                rid = args[0] if args else ""
                with LOCK:
                    r = requests.get(rid)
                    ok = r is not None and r["status"] == "awaitingApproval"
                    if ok:
                        r["status"] = "cancelled"
                        key = (r["chainId"], r["from"].lower())
                        if ledger.get(key) == r["nonce"] + 1:
                            ledger[key] = r["nonce"]
                log(method, "cancelled" if ok else "not cancellable", requestId=rid)
                return self.reply({"ok": ok} if ok else {"ok": False, "error": f"{rid} is not awaiting approval"})
            if method == "history":
                address = (args[0] if args else "").lower()
                chain = int(args[1]) if len(args) > 1 else 0
                with LOCK:
                    txs = [r for r in rows.values() if r["from"].lower() == address
                           and (chain == 0 or r["chainId"] == chain)]
                log(method, f"{len(txs)} rows")
                return self.reply({"ok": True, "chainId": chain, "address": args[0] if args else "",
                                   "transactions": txs})
            if method == "live_sends":
                with LOCK:
                    sends = [{"requestId": rid, "handle": f"h-{rid}", "chainId": r["chainId"], "from": r["from"],
                              "status": r["status"], "origin": "ferrata_relayer_module", "purpose": r["purpose"]}
                             for rid, r in requests.items() if r["status"] == "awaitingApproval"]
                log(method, f"{len(sends)} open")
                return self.reply({"ok": True, "sends": sends})
            log(method, "unknown method")
            return self.reply({"ok": False, "error": f"{method}: not in this stand-in"})

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    srv.daemon_threads = True
    with open(a.port_file + ".part", "w") as f:
        f.write(f"{srv.server_address[1]}\n")
    os.replace(a.port_file + ".part", a.port_file)
    srv.serve_forever()


if __name__ == "__main__":
    main()
