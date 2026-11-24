#!/usr/bin/env python3
"""deploy/testnet/install-web.sh, its real shell steps, on a scratch copy: the script's absolute paths moved into a
temporary tree beside this file, curl (the instance metadata), caddy and systemctl stubbed, nothing outside the tree
touched. The host's values in the wallet's served config survive both orders:
- after a good run the operator closes deposits by hand (bridge.lezReady false, a later fromBlock): the next install
  keeps both (the live file's values win over the copy served after the last good run; the fault review round 2 found);
- a run stopped after step 3 (the bundle's raw config, bridge settings null, in place), then a good run: the host's
  values come back from the copy served after the last good run, the hand edits included.
The messaging module's hand-set values are kept too; the bundle's and web-config.py's own values are written as before.
Run: python3 -I deploy/testnet/tests/test_install_web.py"""
import io
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
SRC = HERE.parent  # deploy/testnet
REPO = SRC.parent.parent
WALLET = SRC / "web" / "wallet"
FAILED = []
BRIDGE, MSG1, MSG2, NAMES1, NAMES2, ATT = ("36iq" + "6" * 40, "ExZx" + "1" * 40, "Gnqr" + "2" * 40, "4EBU" + "3" * 40,
                                           "JCx9" + "4" * 40, "6f9z" + "5" * 40)
LIVE_WALLET = {"network": "Ferrata LEZ testnet (rc3)", "rpc": "/rpc", "faucet": "/faucet", "explorer": "/explorer/",
               "token": {"symbol": "LGO", "decimals": 9}, "gasLimitTransfer": 100000,
               "msg": {"program": MSG1}, "names": {"program": NAMES1},
               "bridge": {"program": BRIDGE, "configId": "0x" + "37" * 32, "vault": "0x" + "19" * 20, "evmChainId": 11155111,
                          "lezReady": True, "fromBlock": 12196188, "token": {"symbol": "ETH", "decimals": 18}}}
LIVE_MSG = {"rpc": "https://net.example/rpc", "program": MSG2, "legacy_program": MSG1, "note": "set by the operator"}


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f": {detail}" if detail and not ok else ""))
    if not ok:
        FAILED.append(name)


def tree(t):
    """The scratch host: the script (paths moved), web-config.py, the files steps 4 and 6 read, stubs, a web tarball."""
    d = t / "deploy" / "testnet"
    (d / "host" / "www").mkdir(parents=True)
    for p in ("idl", "www/wallet", "www/msg", "status", "caddy", "bin", "web"):
        (t / p).mkdir(parents=True)
    text = (SRC / "install-web.sh").read_text()
    for a, b in (("/etc/ferrata/site.env", f"{t}/site.env"), ("/srv/ferrata/www", f"{t}/www"), ("/opt/ferrata/web-stage", f"{t}/stage"),
                 ("/var/lib/ferrata/web-served", f"{t}/served"), ("/var/lib/ferrata/status/", f"{t}/status/"),
                 ("/etc/caddy/", f"{t}/caddy/"), ("-o ferrata -g ferrata ", "")):
        if a not in text:
            raise SystemExit(f"install-web.sh no longer has {a!r}: this test's copy is out of date")
        text = text.replace(a, b)
    left = re.findall(r"(?<![\w.-])/(?:srv|opt|etc|var)/\S*", text.replace(str(t), "<tree>"))
    if left:
        raise SystemExit(f"install-web.sh names paths outside the scratch tree: {left}")
    (d / "install-web.sh").write_text(text)
    shutil.copy(SRC / "web-config.py", d / "web-config.py")
    for f in ("status.html", "health.js", "style.css", "robots.txt", "AGENTS-limits.md"):
        shutil.copy(SRC / "host" / "www" / f, d / "host" / "www" / f)
    shutil.copy(SRC / "host" / "Caddyfile", d / "host" / "Caddyfile")
    for f in ("ferrata_attestation.idl.json", "ferrata_bridge.idl.json"):
        shutil.copy(REPO / "idl" / f, t / "idl" / f)
    for name, body in (("curl", 'case "$*" in *api/token*) echo tok;; *public-ipv4*) echo 192.0.2.10;; esac\n'),
                       ("caddy", "exit 0\n"), ("systemctl", "exit 0\n")):
        (t / "bin" / name).write_text("#!/bin/sh\n" + body)
        (t / "bin" / name).chmod(0o755)
    (t / "status" / "programs.json").write_text(json.dumps({"programs": {
        "ferrata_attestation": {"program_account": ATT}, "ferrata_bridge_b1": {"program_account": BRIDGE},
        "ferrata_msg": {"program_account": MSG1}, "ferrata_names": {"program_account": NAMES1},
        "ferrata_msg_v2": {"program_account": MSG2}, "ferrata_names_v2": {"program_account": NAMES2}}, "instances": {}}))
    files = {"build/index.html": b"<p>build</p>", "explorer/index.html": b"<p>explorer</p>",
             "explorer/config.json": json.dumps({"programs": []}).encode(),
             "wallet/index.html": b"<p>wallet</p>", "wallet/config.json": (WALLET / "config.json").read_bytes(),
             "wallet/vendor/names.js": (WALLET / "vendor" / "names.js").read_bytes(),
             "wallet/msg/ferrata-msg.js": (WALLET / "msg" / "ferrata-msg.js").read_bytes(),
             "msg/config.json": json.dumps({"rpc": None, "program": None, "legacy_program": None}).encode()}
    with tarfile.open(t / "web.tgz", "w:gz") as tf:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    (t / "www" / "wallet" / "config.json").write_text(json.dumps(LIVE_WALLET, indent=2))
    (t / "www" / "msg" / "config.json").write_text(json.dumps(LIVE_MSG, indent=2))


with tempfile.TemporaryDirectory(dir=HERE) as d:
    t = pathlib.Path(d)
    tree(t)
    env = {**os.environ, "PATH": f"{t / 'bin'}:{os.environ['PATH']}"}

    def run():
        r = subprocess.run(["bash", str(t / "deploy" / "testnet" / "install-web.sh"), str(t / "web.tgz")], env=env,
                           capture_output=True, text=True, timeout=120)
        return r.returncode, (r.stdout + r.stderr)[-400:]

    def wallet():
        return json.loads((t / "www" / "wallet" / "config.json").read_text())

    def served():
        return json.loads((t / "served" / "wallet-config.json").read_text())

    rc, out = run()
    w = wallet()
    check("a good run: completes", rc == 0, out)
    check("a good run: the host's bridge settings kept where the bundle leaves them empty, the program from programs.json",
          {k: w["bridge"].get(k) for k in ("vault", "configId", "lezReady", "fromBlock", "token", "program")}
          == {k: LIVE_WALLET["bridge"][k] for k in ("vault", "configId", "lezReady", "fromBlock", "token", "program")})
    check("a good run: the copy served after it is the file served", served() == w)

    w["bridge"]["lezReady"], w["bridge"]["fromBlock"] = False, 12200000  # the operator closes deposits by hand
    (t / "www" / "wallet" / "config.json").write_text(json.dumps(w, indent=2))
    m = json.loads((t / "www" / "msg" / "config.json").read_text())
    m["note"] = "edited by the operator"
    (t / "www" / "msg" / "config.json").write_text(json.dumps(m, indent=2))
    rc, out = run()
    w2 = wallet()
    check("hand edits after a good run, then a good run: lezReady false and the later fromBlock stay (the live file's "
          "values win over the copy served after the last good run)",
          rc == 0 and w2["bridge"]["lezReady"] is False and w2["bridge"]["fromBlock"] == 12200000, out)
    check("... and the messaging module's hand-set value stays",
          json.loads((t / "www" / "msg" / "config.json").read_text()).get("note") == "edited by the operator")

    status = t / "deploy" / "testnet" / "host" / "www" / "status.html"
    status.rename(status.with_suffix(".away"))  # step 4 fails: the run stops after step 3
    rc, out = run()
    raw = wallet()
    check("a run stopped after step 3: fails, the bundle's raw config in place (bridge settings null), the copy served "
          "after the last good run untouched",
          rc != 0 and raw["bridge"].get("vault") is None and "lezReady" not in raw["bridge"]
          and served()["bridge"]["lezReady"] is False, out)
    status.with_suffix(".away").rename(status)
    rc, out = run()
    w3 = wallet()
    check("then a good run: the host's values come back from the copy served after the last good run, hand edits included",
          rc == 0 and {k: w3["bridge"].get(k) for k in ("vault", "configId", "lezReady", "fromBlock", "token")}
          == {"vault": LIVE_WALLET["bridge"]["vault"], "configId": LIVE_WALLET["bridge"]["configId"], "lezReady": False,
              "fromBlock": 12200000, "token": LIVE_WALLET["bridge"]["token"]}, out)
    check("... and the messaging module's", json.loads((t / "www" / "msg" / "config.json").read_text()).get("note") == "edited by the operator")
    check("the wallet's panels on the programs its bundled clients speak (v1), whatever was set",
          (w3["msg"], w3["names"]) == ({"program": MSG1}, {"program": NAMES1}))
print("all passed" if not FAILED else f"FAILED {len(FAILED)}")
sys.exit(1 if FAILED else 0)
