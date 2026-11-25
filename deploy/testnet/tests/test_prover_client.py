#!/usr/bin/env python3
"""host/bin/ferrata-bridge-prover under faults (RFP-022 R.6): its client killed mid-proof and started again, its connection
to the prover cut mid-proof, the prover dying mid-proof or answering that it failed, the prover's service down, a restart after the job was delivered, a request that cannot succeed. The
prover is the stand-in of standin_modal/ (a local server with Modal's call model: a spawned call outlives its client);
the client is the deployed script, unchanged. Each case: the proofs the prover ran, the jobs the relayer's inbox got,
the request's end. Standard library only; run from anywhere: python3 -I deploy/testnet/tests/test_prover_client.py"""
import hashlib
import json
import os
import pathlib
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

HERE = pathlib.Path(__file__).resolve().parent
CLIENT = HERE.parent / "host" / "bin" / "ferrata-bridge-prover"
STANDIN = HERE / "standin_modal"
JOURNAL = "ab" * 32


def wait_for(pred, secs, what):
    t = time.time() + secs
    while time.time() < t:
        v = pred()
        if v:
            return v
        time.sleep(0.1)
    raise AssertionError(f"timed out after {secs} s waiting for {what}")


class Fwd:
    """A TCP forwarder in front of the stand-in service, cut (every connection closed) and started again on its port."""

    def __init__(self, up):
        self.up, self.port, self.socks, self.srv = up, 0, [], None

    def start(self):
        s = socket.socket()
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind(("127.0.0.1", self.port))
        s.listen(64)
        self.port, self.srv = s.getsockname()[1], s
        threading.Thread(target=self.accept, args=(s,), daemon=True).start()

    def accept(self, s):
        while True:
            try:
                c, _ = s.accept()
            except OSError:
                return
            try:
                u = socket.create_connection(("127.0.0.1", self.up))
            except OSError:
                c.close()
                continue
            self.socks += [c, u]
            for a, b in ((c, u), (u, c)):
                threading.Thread(target=self.pipe, args=(a, b), daemon=True).start()

    @staticmethod
    def pipe(a, b):
        try:
            while True:
                d = a.recv(65536)
                if not d:
                    break
                b.sendall(d)
        except OSError:
            pass
        for x in (a, b):
            try:
                x.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

    def cut(self):
        for x in [self.srv] + self.socks:
            if x is None:
                continue
            try:
                x.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            x.close()
        self.socks = []


class Rig:
    """A stand-in prover service, a client, a requests directory and a relayer inbox, in one fresh directory."""

    def __init__(self, delay=0.0):
        self.d = pathlib.Path(tempfile.mkdtemp(prefix="prover-client-"))
        self.req, self.inbox, self.progs = self.d / "requests", self.d / "inbox", self.d / "programs"
        for p in (self.req, self.inbox, self.progs):
            p.mkdir()
        prog = b"stand-in program"
        (self.progs / "checkpoint.bin").write_bytes(prog)
        pins = {"checkpoint-b": ["checkpoint.bin", "0x" + "11" * 32, hashlib.sha256(prog).hexdigest(), "checkpoint"]}
        (self.d / "pins.json").write_text(json.dumps(pins))
        (self.d / "input.bin").write_bytes(b"stand-in input")
        self.ctl, self.log = self.d / "ctl", self.d / "prover.jsonl"
        self.ctl.write_text("pass\n")
        self.delay, self.server, self.client, self.out = delay, None, None, self.d / "client.out"

    def start_server(self, port=None):
        pf = self.d / "port"
        pf.unlink(missing_ok=True)
        cmd = [sys.executable, "-I", str(STANDIN / "server.py"), "--port-file", str(pf), "--log", str(self.log),
               "--work", str(self.d / "work"), "--prove-cmd", f"{sys.executable} -I {STANDIN / 'prove_echo.py'}",
               "--ctl", str(self.ctl)]
        self.server = subprocess.Popen(cmd, env=dict(os.environ, STANDIN_DELAY=str(self.delay)))
        self.port = int(wait_for(lambda: pf.exists() and pf.read_text(), 10, "the server's port"))

    def stop_server(self):
        self.server.kill()
        self.server.wait()

    def env(self, port):
        return dict(os.environ, PYTHONPATH=str(STANDIN), FERRATA_STANDIN_MODAL_URL=f"http://127.0.0.1:{port}",
                    FERRATA_PROVER_PROGRAMS_JSON=str(self.d / "pins.json"), FERRATA_PROGRAMS_31=str(self.progs),
                    FERRATA_PROVE_LIMIT_S="60")

    def start_client(self, port):
        with open(self.out, "a") as f:
            self.client = subprocess.Popen([sys.executable, str(CLIENT), "watch", "--requests", str(self.req), "--inbox",
                                            str(self.inbox), "--poll", "1"], stdout=f, stderr=subprocess.STDOUT,
                                           env=self.env(port))

    def kill_client(self, sig=signal.SIGKILL):
        self.client.send_signal(sig)
        return self.client.wait(timeout=20)

    def request(self, name, journal=JOURNAL):
        q = {"kind": "checkpoint-b", "input": str(self.d / "input.bin"), "journal": "0x" + journal}
        (self.req / f".{name}.tmp").write_text(json.dumps(q))
        os.replace(self.req / f".{name}.tmp", self.req / f"{name}.json")

    def proofs(self, name):
        if not self.log.exists():
            return []
        return [json.loads(x) for x in self.log.read_text().splitlines() if json.loads(x).get("name") == name]

    def done(self, name, where="done"):
        p = self.req / where / f"{name}.json"
        return json.loads(p.read_text()) if p.exists() else None

    def lines(self):
        return [json.loads(x) for x in self.out.read_text().splitlines() if x.startswith("{")] if self.out.exists() else []

    def close(self):
        for p in (self.client, self.server):
            if p and p.poll() is None:
                p.kill()
                p.wait()
        shutil.rmtree(self.d, ignore_errors=True)


class ProverClient(unittest.TestCase):
    def setUp(self):
        self.rigs = []

    def tearDown(self):
        for r in self.rigs:
            r.close()

    def rig(self, delay=0.0):
        r = Rig(delay)
        self.rigs.append(r)
        return r

    def test_client_killed_mid_proof_waits_for_the_same_call(self):
        r = self.rig(delay=4)
        r.start_server()
        r.start_client(r.port)
        r.request("cp-1")
        wait_for(lambda: any(p["event"] == "start" for p in r.proofs("cp-1")), 15, "the proof to start")
        time.sleep(1)
        r.kill_client()
        self.assertTrue((r.req / "cp-1.json").exists(), "the request stays")
        call = json.loads((r.req / "calls" / "cp-1.json").read_text())
        r.start_client(r.port)
        d = wait_for(lambda: r.done("cp-1"), 30, "the request done")
        self.assertTrue(d["ok"] and d["resumed"], d)
        self.assertEqual(d["call_id"], call["call_id"], "the same call waited for")
        self.assertEqual([p["event"] for p in r.proofs("cp-1")], ["start", "end"], "one proof, not two")
        self.assertEqual(json.loads((r.inbox / "cp-1.json").read_text()),
                         {"kind": "checkpoint", "journal": "0x" + JOURNAL, "seal": "0x5ea15ea1"})
        self.assertEqual(sorted(p.name for p in r.inbox.iterdir()), ["cp-1.json"])
        self.assertFalse((r.req / "calls" / "cp-1.json").exists() or (r.req / "failed" / "cp-1.json").exists())
        self.assertEqual(r.kill_client(signal.SIGTERM), 0, "a clean stop on SIGTERM")
        self.assertEqual(r.lines()[-1], {"event": "stop", "reason": "SIGTERM"})

    def test_prover_dying_mid_proof_is_proven_again(self):
        r = self.rig(delay=3)
        r.start_server()
        r.ctl.write_text("die-next 1\n")
        r.start_client(r.port)
        r.request("cp-2")
        d = wait_for(lambda: r.done("cp-2"), 40, "the request done")
        ends = [p for p in r.proofs("cp-2") if p["event"] == "end"]
        self.assertEqual([(e["ok"], e.get("died", False)) for e in ends], [(False, True), (True, False)], ends)
        self.assertTrue(d["ok"] and d["attempts"] == 2, d)
        self.assertTrue(any(x.get("event") == "retry" for x in r.lines()), "the failure logged as a retry")
        self.assertEqual(sorted(p.name for p in r.inbox.iterdir()), ["cp-2.json"])

    def test_a_cut_connection_mid_proof_waits_for_the_same_call(self):
        # The client's connection to the service cut for 4 s while the spawned call runs: it waits for the same call
        # again (no attempt counted), instead of starting a second proof.
        r = self.rig(delay=8)
        r.start_server()
        f = Fwd(r.port)
        f.start()
        self.addCleanup(f.cut)
        r.start_client(f.port)
        r.request("cp-5")
        wait_for(lambda: any(p["event"] == "start" for p in r.proofs("cp-5")), 15, "the proof to start")
        time.sleep(2)
        f.cut()
        time.sleep(4)
        f.start()
        d = wait_for(lambda: r.done("cp-5") or r.done("cp-5", "failed"), 60, "the request finished")
        self.assertTrue(d["ok"] and d["attempts"] == 1, d)
        self.assertEqual([p["event"] for p in r.proofs("cp-5")], ["start", "end"], "one proof, not two")
        self.assertTrue(any(x.get("event") == "reconnect" for x in r.lines()), "the cut logged as a reconnect")
        self.assertFalse(any(x.get("event") == "retry" for x in r.lines()), "no attempt counted")
        self.assertEqual(sorted(p.name for p in r.inbox.iterdir()), ["cp-5.json"])

    def test_a_failed_prover_answer_is_proven_again(self):
        # The service answers a prover that failed (a crash, out of memory, its own limit) as Modal's prove_bytes does:
        # ok false with its exit code. The client proves again, within its attempts.
        r = self.rig(delay=1)
        r.start_server()
        r.ctl.write_text("fail-next 137\n")
        r.start_client(r.port)
        r.request("cp-6")
        d = wait_for(lambda: r.done("cp-6") or r.done("cp-6", "failed"), 40, "the request finished")
        ends = [p for p in r.proofs("cp-6") if p["event"] == "end"]
        self.assertEqual([(e["ok"], e.get("exit")) for e in ends], [(False, 137), (True, None)], ends)
        self.assertTrue(d["ok"] and d["attempts"] == 2, d)
        self.assertTrue(any(x.get("event") == "retry" and "exit 137" in x.get("error", "") for x in r.lines()))
        self.assertEqual(sorted(p.name for p in r.inbox.iterdir()), ["cp-6.json"])

    def test_service_down_keeps_the_request_until_it_is_back(self):
        r = self.rig()
        r.start_server()
        port = r.port
        r.stop_server()
        r.start_client(port)
        r.request("cp-3")
        wait_for(lambda: sum(x.get("event") == "retry" for x in r.lines()) >= 3, 20, "three failed attempts")
        self.assertTrue((r.req / "cp-3.json").exists() and not (r.req / "failed" / "cp-3.json").exists())
        self.assertEqual(r.proofs("cp-3"), [], "nothing reached the prover")
        # The service back on another port: the client is started again with it (its unit's environment).
        r.kill_client()
        r.start_server()
        r.start_client(r.port)
        d = wait_for(lambda: r.done("cp-3"), 30, "the request done")
        self.assertTrue(d["ok"] and d["attempts"] == 1, d)
        self.assertEqual([p["event"] for p in r.proofs("cp-3")], ["start", "end"])

    def test_restart_after_delivery_writes_no_second_job(self):
        r = self.rig()
        r.start_server()
        r.start_client(r.port)
        r.request("cp-4")
        wait_for(lambda: r.done("cp-4"), 20, "the first request done")
        r.kill_client()
        # A client killed after the job was delivered and before the request was finished: its record as it was then;
        # the relayer has taken the job from the inbox meanwhile.
        first = r.done("cp-4")
        (r.req / "done" / "cp-4.json").unlink()
        (r.inbox / "cp-4.json").unlink()
        r.request("cp-4")
        rec = {"call_id": first["call_id"], "attempts": 1, "delivered": True,
               "result": {"ok": True, "journal_hex": JOURNAL, "seal_hex": "5ea15ea1"}}
        (r.req / "calls" / "cp-4.json").write_text(json.dumps(rec))
        r.start_client(r.port)
        d = wait_for(lambda: r.done("cp-4"), 20, "the request finished")
        self.assertTrue(d["ok"] and d.get("delivered_before"), d)
        self.assertEqual(list(r.inbox.iterdir()), [], "no second job")
        self.assertEqual(len([p for p in r.proofs("cp-4") if p["event"] == "start"]), 1, "no second proof")

    def test_a_proof_of_another_journal_fails_at_once(self):
        r = self.rig()
        r.start_server()
        r.start_client(r.port)
        r.request("wrong-1")
        d = wait_for(lambda: r.done("wrong-1", "failed"), 20, "the request failed")
        self.assertIn("journal is not the one asked for", d["error"])
        self.assertEqual(len([p for p in r.proofs("wrong-1") if p["event"] == "start"]), 1, "not tried again")
        self.assertEqual(list(r.inbox.iterdir()), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
