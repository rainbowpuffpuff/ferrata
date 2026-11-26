#!/usr/bin/env python3
"""ferrata-bridge-checkpoint with more than one attestor on the host (RFP-022 F.7 on the test network:
deploy/testnet/MULTIKEY.md), against a stand-in node API (its channel's accredited keys, an empty chain) and a stand-in
realblocks-crown that records the attestations it is given:
- one key (the network as deployed): the newest tip of the one attestor, its attestation alone (as before);
- four keys, t(4) = 3: the newest tip signed by at least three of the four attestors, every attestation of that tip
  handed to realblocks-crown (attestation.json and attestation-<k>.json), none of another tip;
- a tip signed by two of four is passed over for an older one signed by three;
- no tip signed by t of them: no build;
- the node's channel unreadable: no tip, no build (fails closed).
Run: python3 -I deploy/testnet/tests/test_checkpoint_attestors.py"""
import http.server
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import threading

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = pathlib.Path(os.environ.get("CHECKPOINT_UNDER_TEST") or HERE.parent / "host" / "bin" / "ferrata-bridge-checkpoint")
GENESIS = "0x" + "47" * 32
CHANNEL = "01" * 32
FAILED = []
NODE = {"keys": 1, "up": True}


class Api(http.server.BaseHTTPRequestHandler):
    """The vault's finalState over JSON-RPC (POST) and the node's API (GET)."""

    def answer(self, code, body):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        q = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.answer(200, {"jsonrpc": "2.0", "id": q["id"], "result": GENESIS})

    def do_GET(self):
        if not NODE["up"]:
            self.answer(503, {})
        elif self.path == f"/channel/{CHANNEL}":
            self.answer(200, {"accredited_keys": [f"{k:064x}" for k in range(NODE["keys"])]})
        elif self.path == "/cryptarchia/info":
            self.answer(200, {"cryptarchia_info": {"lib_slot": 0}})
        elif self.path.startswith("/cryptarchia/blocks"):
            self.answer(200, [])
        else:
            self.answer(404, {})

    def log_message(self, *a):
        pass


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f": {detail}" if detail and not ok else ""))
    if not ok:
        FAILED.append(name)


srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Api)
threading.Thread(target=srv.serve_forever, daemon=True).start()
url = f"http://127.0.0.1:{srv.server_address[1]}"
with tempfile.TemporaryDirectory(dir=HERE) as d:
    b = pathlib.Path(d) / "bridge"
    (b / "checkpoint").mkdir(parents=True)
    (b / "params.json").write_text(json.dumps({"genesisState": GENESIS}))
    (b / "config.json").write_text(json.dumps({"vault": "0x" + "19" * 20, "configId": "0x" + "37" * 32}))
    (b / "identity.json").write_text(json.dumps({"tip": {"id": 100}, "bridge": {"header": "0x" + "1f" * 32}}))
    env_file = pathlib.Path(d) / "bridge.env"
    env_file.write_text(f"FERRATA_ETH_RPC={url}\nFERRATA_VAULT=0x{'19' * 20}\n")
    log = pathlib.Path(d) / "rc-args.txt"
    stub = pathlib.Path(d) / "realblocks-crown"
    # The stand-in records the --att list and the files' contents, then writes the outputs a build summarizes.
    stub.write_text("""#!/usr/bin/env bash
prev=""; for a in "$@"; do case "$prev" in --out) out="$a" ;; --name) name="$a" ;; --att) att="$a" ;; --tip) tip="$a" ;; esac; prev="$a"; done
{ echo "tip $tip"; for f in ${att//,/ }; do echo "att $(basename "$f") $(cat "$f")"; done; } > "$RC_LOG"
echo '{"journal": "aa", "tip": '"$tip"', "parent_state": "'"$GENESIS_STATE"'", "new_state": "0x01"}' > "$out/$name.json"
echo '{"parent_tip": 100, "burns": []}' > "$out/burns.json"
echo '{"tip_id": '"$tip"', "digest": "0x01"}' > "$out/state.json"
""")
    stub.chmod(0o755)
    env = {**os.environ, "FERRATA_BRIDGE_DIR": str(b), "FERRATA_BRIDGE_ENV": str(env_file), "FERRATA_NODE_API": url,
           "FERRATA_REALBLOCKS_CROWN": str(stub), "RC_LOG": str(log), "GENESIS_STATE": GENESIS,
           "PYTHONDONTWRITEBYTECODE": "1"}

    def attestors(rounds):
        """rounds: {directory name: [tips]}; each tip file names its attestor and tip."""
        for p in b.glob("attestor*"):
            shutil.rmtree(p)
        for name, tips in rounds.items():
            (b / name / "rounds").mkdir(parents=True)
            for t in tips:
                (b / name / "rounds" / f"tip-{t}.json").write_text(f"{name}:{t}")

    def run():
        log.unlink(missing_ok=True)
        for z in (b / "checkpoint").glob("zone-*"):
            shutil.rmtree(z)
        r = subprocess.run(["bash", str(SCRIPT)], env=env, capture_output=True, text=True, timeout=120)
        given = log.read_text().split("\n") if log.exists() else []
        return r.returncode, r.stdout + r.stderr, given

    NODE.update(keys=1, up=True)
    attestors({"attestor": [120, 130]})
    rc, out, given = run()
    check("one key: the one attestor's newest tip, its attestation alone",
          rc == 0 and given[:2] == ["tip 130", "att attestation.json attestor:130"] and len([g for g in given if g.startswith("att ")]) == 1,
          out[-300:] + repr(given))

    NODE.update(keys=4)
    attestors({"attestor": [120, 130], "attestor-1": [120, 130], "attestor-2": [120], "attestor-3": [120, 130]})
    rc, out, given = run()
    atts = sorted(g.split(" ", 2)[2] for g in given if g.startswith("att "))
    check("four keys, t = 3: the newest tip three attestors signed, and all its attestations",
          rc == 0 and given[0] == "tip 130" and atts == ["attestor-1:130", "attestor-3:130", "attestor:130"]
          and (b / "checkpoint" / "zone-100-130").is_dir(), out[-300:] + repr(given))

    attestors({"attestor": [120, 130], "attestor-1": [120, 130], "attestor-2": [120], "attestor-3": [120]})
    rc, out, given = run()
    atts = sorted(g.split(" ", 2)[2] for g in given if g.startswith("att "))
    check("a tip two of four signed is passed over for the older one four signed",
          rc == 0 and given[0] == "tip 120" and atts == ["attestor-1:120", "attestor-2:120", "attestor-3:120", "attestor:120"],
          out[-300:] + repr(given))

    attestors({"attestor": [120, 130], "attestor-1": [130], "attestor-2": [], "attestor-3": [120]})
    rc, out, given = run()
    check("no tip signed by t of them: no build", rc != 0 and not given and not list((b / "checkpoint").glob("zone-*")),
          out[-300:] + repr(given))

    NODE.update(up=False)
    attestors({"attestor": [120, 130], "attestor-1": [120, 130], "attestor-2": [120, 130], "attestor-3": [120, 130]})
    rc, out, given = run()
    check("the node's channel unreadable: no tip, no build (fails closed)",
          rc != 0 and not given and "could not be read" in out and not list((b / "checkpoint").glob("zone-*")),
          out[-300:] + repr(given))
srv.shutdown()
print(f"{'FAILED ' + str(len(FAILED)) if FAILED else 'all passed'}")
raise SystemExit(1 if FAILED else 0)
