#!/usr/bin/env python3
"""The stand-in Modal service for the prover client's fault tests (modal/ beside it is the client side).

  server.py --port-file F --log L --work DIR --prove-cmd CMD [--ctl C]

Each spawned call runs CMD in a thread (a shell command; PROGRAM, INPUT and META name files holding the call's three
arguments, NAME the request's name) and answers the last line of its standard output, a JSON object, as the call's
result. The calls outlive the client that spawned them, as Modal's do. The control file C (one line, read at each
spawn): `pass`; `die-next SECS`: the next call's prover dies SECS seconds in (its container gone: the call ends with an
error and no result), then `pass`. Every call's start and end go to L (JSONL). Loopback only."""
import argparse
import base64
import json
import os
import pathlib
import subprocess
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

LOCK = threading.Lock()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port-file", required=True)
    ap.add_argument("--log", required=True)
    ap.add_argument("--work", required=True)
    ap.add_argument("--prove-cmd", required=True)
    ap.add_argument("--ctl")
    a = ap.parse_args()
    work = pathlib.Path(a.work)
    work.mkdir(parents=True, exist_ok=True)
    calls = {}

    def log(**kw):
        with LOCK, open(a.log, "a") as f:
            f.write(json.dumps({"t": round(time.time(), 3), **kw}) + "\n")

    def control():
        try:
            line = pathlib.Path(a.ctl).read_text().split() if a.ctl else []
        except FileNotFoundError:
            line = []
        return line or ["pass"]

    def run(cid, args, die_after):
        d = work / cid
        d.mkdir()
        (d / "program").write_bytes(base64.b64decode(args[0]))
        (d / "input").write_bytes(base64.b64decode(args[1]))
        (d / "meta.json").write_text(json.dumps(args[2]))
        name = args[2].get("name", "")
        log(event="start", call_id=cid, name=name, die_after=die_after)
        env = dict(os.environ, PROGRAM=str(d / "program"), INPUT=str(d / "input"), META=str(d / "meta.json"), NAME=name)
        p = subprocess.Popen(["bash", "-c", a.prove_cmd], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, text=True)
        try:
            out, err = p.communicate(timeout=die_after)
        except subprocess.TimeoutExpired:
            p.kill()
            p.communicate()
            calls[cid] = {"error": "the prover's container stopped mid-proof (stand-in)"}
            log(event="end", call_id=cid, name=name, ok=False, died=True)
            return
        lines = [x for x in out.splitlines() if x.strip().startswith("{")]
        if p.returncode != 0 or not lines:
            calls[cid] = {"error": f"the prover exited {p.returncode}: {err[-300:]}"}
            log(event="end", call_id=cid, name=name, ok=False, exit=p.returncode)
            return
        calls[cid] = {"result": json.loads(lines[-1])}
        log(event="end", call_id=cid, name=name, ok=bool(calls[cid]["result"].get("ok")))

    class H(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def reply(self, status, obj):
            data = json.dumps(obj).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            u = urlparse(self.path)
            if u.path == "/health":
                return self.reply(200, {"ok": True})
            q = parse_qs(u.query)
            cid = q.get("id", [""])[0]
            wait = float(q.get("wait", ["0"])[0])
            t_end = time.time() + wait
            while True:
                if cid not in calls:
                    return self.reply(404, {"error": "unknown call"})
                c = calls[cid]
                if "result" in c:
                    return self.reply(200, {"result": c["result"]})
                if "error" in c:
                    return self.reply(500, {"error": c["error"]})
                if time.time() >= t_end:
                    return self.reply(202, {"pending": True})
                time.sleep(0.2)

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
            if urlparse(self.path).path != "/spawn":
                return self.reply(404, {"error": "no such path"})
            ctl = control()
            die_after = None
            if ctl[0] == "die-next":
                die_after = float(ctl[1]) if len(ctl) > 1 else 1.0
                pathlib.Path(a.ctl).write_text("pass\n")
            cid = "fc-" + uuid.uuid4().hex[:16]
            calls[cid] = {}
            threading.Thread(target=run, args=(cid, body["args"], die_after), daemon=True).start()
            self.reply(200, {"call_id": cid})

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    pathlib.Path(a.port_file).write_text(str(srv.server_address[1]))
    srv.serve_forever()


if __name__ == "__main__":
    main()
