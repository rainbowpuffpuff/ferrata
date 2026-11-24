#!/usr/bin/env python3
"""ferrata-bridge-checkpoint's choice of parent and its refusal to stack candidates, against a stand-in Sepolia
endpoint that answers the vault's finalState() (no node, no prover: the runs stop where the node would be read):
- finalState = the configuration's genesisState: the parent is the configuration's tip T0 (zone-<T0>-<T>);
- finalState = a checkpoint's new state with its state.json here: the parent is that checkpoint's tip (zone-<tip>-<T>);
- finalState with no state.json here: refused, naming --rebuild;
- --prove while the last request's parent is still finalState (not final yet): refused before anything is built;
  once finalState moved on: allowed;
- the endpoint failing: no build and no request, with or without a pending request (the guard fails closed);
- --rebuild (with a stand-in realblocks-crown): the directory is copied whole first, under before-rebuild/ (never
  taken for a parent); the same journal is accepted and state.json written; another journal is refused, the copy named.
Run: python3 -I deploy/testnet/tests/test_checkpoint_parent.py"""
import http.server
import json
import os
import pathlib
import subprocess
import tempfile
import threading

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = pathlib.Path(os.environ.get("CHECKPOINT_UNDER_TEST") or HERE.parent / "host" / "bin" / "ferrata-bridge-checkpoint")
GENESIS = "0x" + "47" * 32
POSTED = "0x" + "66" * 32
FAILED = []
STATE = {"final": GENESIS}


class Rpc(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        q = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert q["method"] == "eth_call" and q["params"][0]["data"] == "0x22f3099c", q
        if STATE["final"] is None:  # the endpoint fails
            self.send_response(502)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        body = json.dumps({"jsonrpc": "2.0", "id": q["id"], "result": STATE["final"]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f": {detail}" if detail and not ok else ""))
    if not ok:
        FAILED.append(name)


srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Rpc)
threading.Thread(target=srv.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory(dir=HERE) as d:
    b = pathlib.Path(d) / "bridge"
    (b / "attestor" / "rounds").mkdir(parents=True)
    (b / "checkpoint" / "zone-1225-3767").mkdir(parents=True)
    (b / "params.json").write_text(json.dumps({"genesisState": GENESIS}))
    (b / "config.json").write_text(json.dumps({"vault": "0x" + "19" * 20, "configId": "0x" + "37" * 32}))
    (b / "identity.json").write_text(json.dumps({"tip": {"id": 1225}, "bridge": {"header": "0x" + "1f" * 32}}))
    (b / "checkpoint" / "zone-1225-3767" / "state.json").write_text(json.dumps({"tip_id": 3767, "digest": POSTED}))
    (b / "attestor" / "rounds" / "tip-3800.json").write_text("{}")
    env_file = pathlib.Path(d) / "bridge.env"
    env_file.write_text(f"FERRATA_ETH_RPC=http://127.0.0.1:{srv.server_address[1]}\nFERRATA_VAULT=0x{'19' * 20}\n")
    env = {**os.environ, "FERRATA_BRIDGE_DIR": str(b), "FERRATA_BRIDGE_ENV": str(env_file), "PYTHONDONTWRITEBYTECODE": "1"}

    def run(*args):
        # The node API is not there: a run that gets past the parent stops at its first read of the node.
        r = subprocess.run(["bash", str(SCRIPT), *args], env=env, capture_output=True, text=True, timeout=120)
        return r.returncode, r.stdout + r.stderr

    STATE["final"] = GENESIS
    rc, out = run("--tip", "3800")
    check("finalState = genesisState: the parent is T0 (zone-1225-3800)", (b / "checkpoint" / "zone-1225-3800").is_dir(), out[-300:])
    STATE["final"] = POSTED
    rc, out = run("--tip", "3800")
    check("finalState = a posted checkpoint's state: the parent is its tip (zone-3767-3800)",
          (b / "checkpoint" / "zone-3767-3800").is_dir(), out[-300:])
    STATE["final"] = "0x" + "99" * 32
    rc, out = run("--tip", "3800")
    check("finalState without a state.json here: refused, naming --rebuild", rc != 0 and "--rebuild" in out, out[-300:])
    (b / "checkpoint" / "requested.json").write_text(json.dumps({"name": "zone-1225-3767", "summary": {"parent_state": GENESIS}}))
    STATE["final"] = GENESIS
    rc, out = run("--tip", "3800", "--prove")
    check("--prove while the last request is not final: refused", rc != 0 and "not final yet" in out, out[-300:])
    (b / "checkpoint" / "requested.json").write_text(json.dumps({"name": "zone-1225-3767", "summary": {}}))
    rc, out = run("--tip", "3800", "--prove")
    check("--prove after a request from before parents were recorded (genesis assumed): refused while genesis is final",
          rc != 0 and "not final yet" in out, out[-300:])
    STATE["final"] = POSTED
    rc, out = run("--tip", "3800", "--prove")
    check("--prove once the last request is final: past the guard (stops at the node)",
          "not final yet" not in out and (b / "checkpoint" / "zone-3767-3800").is_dir(), out[-300:])
    import shutil
    for z in (b / "checkpoint").glob("zone-*-3800"):
        shutil.rmtree(z)
    STATE["final"] = None
    rc, out = run("--tip", "3800", "--prove")
    check("the endpoint failing with a request pending: --prove builds nothing (fails closed)",
          rc != 0 and "could not be read" in out and not list((b / "checkpoint").glob("zone-*-3800")), out[-300:])
    (b / "checkpoint" / "requested.json").unlink()
    rc, out = run("--tip", "3800")
    check("the endpoint failing, nothing pending: no parent, no build",
          rc != 0 and "could not be read" in out and not list((b / "checkpoint").glob("zone-*-3800")), out[-300:])
    # --rebuild of a checkpoint directory from before state files, with a stand-in realblocks-crown that writes the
    # outputs of the same span (its journal: STUB_JOURNAL) and touches the input, as the real one rewrites it.
    w = b / "checkpoint" / "zone-1225-3767"
    (w / "node" / "blocks").mkdir(parents=True)
    (w / "attestation.json").write_text("{}")
    (w / "zone-1225-3767.input").write_bytes(b"the proven input")
    (w / "burns.json").write_text(json.dumps({"parent_tip": 1225, "burns": [{"burn_index": 0, "block": 3749}]}))
    (w / "zone-1225-3767.json").write_text(json.dumps({"journal": "aa" * 4, "tip": 3767, "parent_state": GENESIS,
                                                       "new_state": POSTED, "blocks": 2542}))
    (w / "state.json").unlink()
    stub = pathlib.Path(d) / "realblocks-crown"
    stub.write_text("""#!/usr/bin/env bash
prev=""; for a in "$@"; do case "$prev" in --out) out="$a" ;; --name) name="$a" ;; esac; prev="$a"; done
printf 'rebuilt' > "$out/$name.input"
python3 -I - "$out" "$name" "$STUB_JOURNAL" <<'PY'
import json, sys
out, name, journal = sys.argv[1:4]
m = json.load(open(f"{out}/{name}.json")); m["journal"] = journal
json.dump(m, open(f"{out}/{name}.json", "w"))
json.dump({"tip_id": 3767, "digest": m["new_state"]}, open(f"{out}/state.json", "w"))
PY
""")
    stub.chmod(0o755)
    env["FERRATA_REALBLOCKS_CROWN"] = str(stub)
    env["STUB_JOURNAL"] = "aa" * 4
    STATE["final"] = GENESIS
    rc, out = run("--rebuild", str(w))
    kept = list((b / "checkpoint" / "before-rebuild").glob("zone-1225-3767-*"))
    check("--rebuild: the same journal is accepted, state.json written, the directory copied whole first (outside zone-*)",
          rc == 0 and "journal unchanged" in out and (w / "state.json").exists() and len(kept) == 1
          and (kept[0] / "zone-1225-3767.input").read_bytes() == b"the proven input"
          and not [k for k in (b / "checkpoint").glob("zone-*") if k.name != "zone-1225-3767"], out[-300:])
    # A copy with a state.json of its own (one rebuilt before): a parent is still chosen from zone-* alone.
    (kept[0] / "state.json").write_text(json.dumps({"tip_id": 9999, "digest": "0x" + "77" * 32}))
    STATE["final"] = "0x" + "77" * 32
    rc, out = run("--tip", "3800")
    check("a copy under before-rebuild/ is never taken for a parent (finalState = its digest: refused, nothing built)",
          rc != 0 and "--rebuild" in out and not list((b / "checkpoint").glob("zone-*-3800")), out[-300:])
    STATE["final"] = GENESIS
    env["STUB_JOURNAL"] = "bb" * 4
    rc, out = run("--rebuild", str(w))
    check("--rebuild: another journal is refused, naming the copy of the directory as it was",
          rc != 0 and "differs" in out and "before-rebuild/zone-1225-3767-" in out, out[-300:])
srv.shutdown()
print(f"{'FAILED ' + str(len(FAILED)) if FAILED else 'all passed'}")
raise SystemExit(1 if FAILED else 0)
