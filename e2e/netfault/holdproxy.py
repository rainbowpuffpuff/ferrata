#!/usr/bin/env python3
"""holdproxy: one HTTP endpoint in front of another (a LEZ sequencer's JSON-RPC, the mini-app devnet's /rpc), whose
faults the test chooses as it goes, for the off-chain components' fault scenarios (RFP-022 R.06, R.07; e2e/NETWORK.md).

  holdproxy.py --upstream http://127.0.0.1:<port> --port-file F --ctl C --log L

Listens on 127.0.0.1 (the port written to F). Before each request it reads the control file C (one line):

  pass                  forward (also when C is missing or empty)
  outage                every request answered 503, or the connection closed (as a provider that is down)
  hold METHOD SECS      a request whose JSON body names METHOD ("method") is forwarded, and its answer held SECS seconds
                        before it is returned (the test kills the client meanwhile: the call reached the server, the
                        client never learnt it); then C is set back to `pass`
  lose METHOD           such a request is forwarded and the connection closed without the answer; then `pass`
  trap METHOD           such a request is not forwarded: the connection is held open until the client goes away (the
                        test kills the client: it had built and proven what it was about to send, which never left);
                        then `pass`
  fail METHOD           such a request is answered 503 without being forwarded (the endpoint failing that one call: it
                        never reached the server); then `pass`

Each one-shot fault writes a line to C + ".hit" when it fires (the test waits for it). Every request and what was done
with it goes to L (JSONL: time, method, action). Local only: the upstream must be a loopback address.
"""
import argparse
import json
import os
import threading
import time
import urllib.error
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
    a = ap.parse_args()
    host = urllib.parse.urlparse(a.upstream).hostname
    if host not in ("127.0.0.1", "localhost", "::1"):
        raise SystemExit(f"holdproxy: the upstream must be a loopback address, not {host}")

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
        with LOCK:
            with open(a.ctl, "w") as f:
                f.write("pass\n")
            with open(a.ctl + ".hit", "a") as f:
                f.write(json.dumps({"t": time.time(), "fired": what}) + "\n")

    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def forward(self, body):
            req = urllib.request.Request(a.upstream + self.path, data=body, method="POST",
                                         headers={"Content-Type": self.headers.get("Content-Type", "application/json")})
            try:
                with urllib.request.urlopen(req, timeout=600) as r:
                    return r.status, r.read(), r.headers.get("Content-Type", "application/json")
            except urllib.error.HTTPError as e:
                return e.code, e.read(), e.headers.get("Content-Type", "application/json")

        def reply(self, status, data, ctype="application/json"):
            self.send_response(status)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def close_now(self):
            self.close_connection = True
            try:
                self.connection.shutdown(2)
            except OSError:
                pass

        def do_GET(self):
            req = urllib.request.Request(a.upstream + self.path)
            try:
                with urllib.request.urlopen(req, timeout=60) as r:
                    self.reply(r.status, r.read(), r.headers.get("Content-Type", "text/plain"))
            except Exception as e:  # noqa: BLE001
                self.reply(502, str(e).encode(), "text/plain")

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get("content-length", 0)))
            try:
                method = json.loads(body).get("method", "")
            except (ValueError, AttributeError):
                method = ""
            ctl = control()
            mode = ctl[0]
            target = ctl[1] if len(ctl) > 1 else None
            if mode == "outage":
                log(method, "outage")
                if int(time.time() * 1000) % 3:
                    return self.reply(503, b"service unavailable", "text/plain")
                return self.close_now()
            if mode == "fail" and method == target:
                fired(f"fail {method}")
                log(method, "failed (503, not forwarded)")
                return self.reply(503, b"service unavailable", "text/plain")
            if mode in ("hold", "lose", "trap") and method == target:
                if mode == "trap":
                    fired(f"trap {method}")
                    log(method, "trapped (not forwarded)")
                    # Held open, never forwarded, until the client is gone (or ten minutes).
                    self.connection.settimeout(1.0)
                    t0 = time.time()
                    while time.time() - t0 < 600:
                        try:
                            if not self.connection.recv(1, 0x02):  # MSG_PEEK: b"" once the client closed
                                break
                        except TimeoutError:
                            continue
                        except OSError:
                            break
                    log(method, "trapped request's client gone")
                    return self.close_now()
                status, data, ctype = self.forward(body)
                if mode == "lose":
                    fired(f"lose {method}")
                    log(method, "forwarded, answer lost", status=status)
                    return self.close_now()
                secs = float(ctl[2]) if len(ctl) > 2 else 10.0
                fired(f"hold {method} {secs}")
                log(method, f"forwarded, answer held {secs} s", status=status)
                time.sleep(secs)
                try:
                    return self.reply(status, data, ctype)
                except OSError:
                    log(method, "held answer not delivered (client gone)")
                    return None
            status, data, ctype = self.forward(body)
            log(method, "forwarded", status=status)
            try:
                self.reply(status, data, ctype)
            except OSError:
                pass

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    srv.daemon_threads = True
    with open(a.port_file + ".part", "w") as f:
        f.write(f"{srv.server_address[1]}\n")
    os.replace(a.port_file + ".part", a.port_file)
    srv.serve_forever()


if __name__ == "__main__":
    main()
