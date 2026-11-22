#!/usr/bin/env python3
"""A beacon node's light-client API replaying a recorded chain in the lab's layout
(lez/lab/results/record/net/<chain>/: bootstrap.ssz, checkpoint, updates/NNNNN.<period|finality>.<layout>.ssz),
for bench/perf/gate-sequencer.sh. Loopback only; nothing is fetched from any network.

Entries "happen" over time: the first --start at once, then one more every --step seconds, up to --last. At any
moment it serves what a beacon node served after the entries that have happened (as lez/updater's Recorded source
does): the newest finality update among them at /eth/v1/beacon/light_client/finality_update, and per period the
newest period update at /eth/v1/beacon/light_client/updates. One port per client (--ports), so that the bytes each
client downloads are counted apart. On SIGTERM or SIGINT it writes --stats (JSON) and exits.

  python3 -I beacon_replay.py --dir DIR --ports 9601,9602 --start 2 --step 90 --last 40 \
      --reveals reveals.tsv --stats stats.json
"""

import argparse
import json
import os
import signal
import struct
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SLOTS_PER_PERIOD = 32 * 256


def u32(b, at):
    return struct.unpack_from("<I", b, at)[0]


def u64(b, at):
    return struct.unpack_from("<Q", b, at)[0]


def slots(kind, b):
    """(attested slot, finalized slot, signature slot) of an Electra-layout update or finality update."""
    attested = u64(b, u32(b, 0))
    if kind == "finality":
        finalized = u64(b, u32(b, 4))
        signature = u64(b, 4 + 4 + 224 + 160)
    else:
        fin_off_at = 4 + 24624 + 192
        finalized = u64(b, u32(b, fin_off_at))
        signature = u64(b, fin_off_at + 4 + 224 + 160)
    if not (finalized <= attested < signature):
        raise ValueError(f"implausible slots {attested} {finalized} {signature}")
    return attested, finalized, signature


class Replay:
    def __init__(self, a):
        d = Path(a.dir)
        self.bootstrap = (d / "bootstrap.ssz").read_bytes()
        self.checkpoint = (d / "checkpoint").read_text().strip().lower().removeprefix("0x")
        self.entries = []
        for p in sorted((d / "updates").glob("*.ssz")):
            _, kind, layout, _ = p.name.split(".")
            if layout != "electra":
                break  # the slot parser knows the Electra layout only
            b = p.read_bytes()
            att, fin, sig = slots(kind, b)
            self.entries.append(dict(name=p.name, kind=kind, layout=layout, ssz=b, attested=att, finalized=fin,
                                     signature=sig, period=att // SLOTS_PER_PERIOD))
        self.last = min(a.last or len(self.entries), len(self.entries))
        self.start, self.step = a.start, a.step
        self.t0 = time.time()
        self.lock = threading.Lock()
        self.stats = {}
        self.revealed = 0
        self.reveals = open(a.reveals, "w") if a.reveals else None
        if self.reveals:
            self.reveals.write("entry\tname\tkind\tattested_slot\tfinalized_slot\tsignature_slot\trevealed_utc\trevealed_unix\n")
        self.tick()

    def now(self):
        n = self.start + int((time.time() - self.t0) // self.step) if self.step > 0 else self.last
        return min(n, self.last)

    def tick(self):
        n = self.now()
        with self.lock:
            while self.revealed < n:
                e = self.entries[self.revealed]
                t = time.time()
                if self.reveals:
                    self.reveals.write(f"{self.revealed + 1}\t{e['name']}\t{e['kind']}\t{e['attested']}\t{e['finalized']}\t"
                                       f"{e['signature']}\t{time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(t))}\t{t:.3f}\n")
                    self.reveals.flush()
                self.revealed += 1
        return n

    def finality_update(self):
        n = self.tick()
        for e in reversed(self.entries[:n]):
            if e["kind"] == "finality":
                return e
        return None

    def period_update(self, period):
        n = self.tick()
        for e in reversed(self.entries[:n]):
            if e["kind"] == "period" and e["period"] == period:
                return e
        return None

    def count(self, port, what, nbytes):
        with self.lock:
            s = self.stats.setdefault(str(port), {})
            r = s.setdefault(what, {"requests": 0, "bytes": 0})
            r["requests"] += 1
            r["bytes"] += nbytes


def handler(replay, port):
    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def send(self, code, body, ctype, what, version=None):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            if version:
                self.send_header("Eth-Consensus-Version", version)
            self.end_headers()
            self.wfile.write(body)
            # Bytes on the wire for this answer: status line and headers as sent, and the body.
            replay.count(port, what, len(body) + 200)

        def do_GET(self):
            path, _, query = self.path.partition("?")
            q = dict(kv.split("=", 1) for kv in query.split("&") if "=" in kv)
            if path == "/eth/v1/node/version":
                body = json.dumps({"data": {"version": "Lodestar/v1.36.0/ferrata-replay"}}).encode()
                return self.send(200, body, "application/json", "version")
            if path.startswith("/eth/v1/beacon/light_client/bootstrap/"):
                root = path.rsplit("/", 1)[1].lower().removeprefix("0x")
                if root != replay.checkpoint:
                    return self.send(404, b'{"code":404,"message":"unknown root"}', "application/json", "bootstrap")
                return self.send(200, replay.bootstrap, "application/octet-stream", "bootstrap", "electra")
            if path == "/eth/v1/beacon/light_client/finality_update":
                e = replay.finality_update()
                if e is None:
                    return self.send(404, b'{"code":404,"message":"none yet"}', "application/json", "finality_update")
                return self.send(200, e["ssz"], "application/octet-stream", "finality_update", e["layout"])
            if path == "/eth/v1/beacon/light_client/updates":
                start, count = int(q.get("start_period", 0)), int(q.get("count", 1))
                out = b""
                # Every period of the range the record holds, an absent one skipped (as lez/updater's Recorded
                # source does): the updater asks for the period before its store's, which a record that starts at
                # the store's period does not hold.
                for p in range(start, start + min(count, 128)):
                    e = replay.period_update(p)
                    if e is not None:
                        out += struct.pack("<Q", len(e["ssz"]) + 4) + b"\x00\x00\x00\x00" + e["ssz"]
                return self.send(200, out, "application/octet-stream", "updates")
            return self.send(404, b'{"code":404}', "application/json", "other")

    return H


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--ports", required=True)
    ap.add_argument("--start", type=int, default=1)
    ap.add_argument("--step", type=float, default=60.0)
    ap.add_argument("--last", type=int, default=0)
    ap.add_argument("--reveals")
    ap.add_argument("--stats", required=True)
    a = ap.parse_args()
    replay = Replay(a)
    servers = []
    for port in [int(p) for p in a.ports.split(",")]:
        s = ThreadingHTTPServer(("127.0.0.1", port), handler(replay, port))
        threading.Thread(target=s.serve_forever, daemon=True).start()
        servers.append(s)
    print(f"replaying {len(replay.entries)} entries (serving up to {replay.last}) on {a.ports}", flush=True)

    def stop(*_):
        Path(a.stats).write_text(json.dumps({"revealed": replay.revealed, "last": replay.last,
                                             "per_port": replay.stats}, indent=1))
        os._exit(0)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    while True:
        replay.tick()
        time.sleep(1)


if __name__ == "__main__":
    sys.exit(main())
