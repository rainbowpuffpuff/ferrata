#!/usr/bin/env python3
"""deploy/testnet/web-config.py (install-web.sh's step 5) on a stand-in web root: with messages and names v2 deployed
beside v1, the messaging module and the wallet use v2 (v1 named as legacy) and the explorer's catalogue keeps v1;
with v1 alone, v1; what the host set in a served config and the bundle leaves empty is kept; the program streams'
AGENTS sections and the host's limits are in /AGENTS.md once, also after a second run.
Run: python3 -I deploy/testnet/tests/test_web_config.py"""
import json
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE.parent / "web-config.py"
FAILED = []
MSG1, MSG2, NAMES1, NAMES2 = "ExZx" + "1" * 40, "Gnqr" + "2" * 40, "4EBU" + "3" * 40, "JCx9" + "4" * 40
ATT, BRIDGE = "6f9z" + "5" * 40, "36iq" + "6" * 40


def check(name, ok):
    print(("ok   " if ok else "FAIL ") + name)
    if not ok:
        FAILED.append(name)


def site(root, v2):
    www, stage, here = root / "www", root / "stage", root / "here"
    for d in (www / "msg", www / "wallet", stage / "msg", stage / "names", stage / "served-before", here / "host" / "www"):
        d.mkdir(parents=True, exist_ok=True)
    progs = {"ferrata_attestation": {"program_account": ATT}, "ferrata_bridge_b1": {"program_account": BRIDGE},
             "ferrata_msg": {"program_account": MSG1}, "ferrata_names": {"program_account": NAMES1}}
    if v2:
        progs |= {"ferrata_msg_v2": {"program_account": MSG2}, "ferrata_names_v2": {"program_account": NAMES2}}
    (www / "programs.json").write_text(json.dumps({"programs": progs, "instances": {"sepolia": {"instance_id": "0x" + "ab" * 32}}}))
    (www / "config.json").write_text(json.dumps({"programs": [{"key": "msg", "account": ""}, {"key": "names", "account": ""},
                                                              {"key": "attestation", "account": ""}]}))
    (www / "msg" / "config.json").write_text(json.dumps({"rpc": None, "program": None, "legacy_program": MSG1, "note": "n"}))
    (www / "wallet" / "config.json").write_text(json.dumps({"network": "bundle", "rpc": "/rpc", "msg": {"program": None},
                                                            "names": {"program": None},
                                                            "bridge": {"program": None, "vault": None, "evmChainId": 11155111}}))
    (stage / "served-before" / "wallet-config.json").write_text(json.dumps(
        {"bridge": {"program": BRIDGE, "vault": "0x1949", "token": {"symbol": "TUSD"}}, "msg": {"program": MSG1}}))
    (stage / "served-before" / "msg-config.json").write_text(json.dumps({"rpc": "https://host.example/rpc", "program": MSG1}))
    (www / "AGENTS.md").write_text("# AGENTS.md\n\nat most 100 blocks per call on {{HOST}}\n\n## Names (`ferrata_names` v2) — for agents\n\nv2\n")
    (stage / "names" / "AGENTS.md").write_text("## Names (`ferrata_names` v2) — for agents\n\nv2\n")
    (stage / "msg" / "AGENTS-section.md").write_text("## Encrypted messages (`ferrata_msg_v2`)\n\nv2 on {{HOST}}\n")
    (here / "host" / "www" / "AGENTS-limits.md").write_text("## Limits of the public endpoints\n\nlimits\n")
    return www, stage, here


def run(www, stage, here):
    return subprocess.run([sys.executable, "-I", str(SCRIPT), str(www), "net.example", str(here), "50", str(stage)],
                          capture_output=True, text=True, timeout=60)


with tempfile.TemporaryDirectory() as d:
    www, stage, here = site(pathlib.Path(d), v2=True)
    r = run(www, stage, here)
    check("runs", r.returncode == 0)
    m = json.loads((www / "msg" / "config.json").read_text())
    check("messaging module: the v2 program, v1 as its legacy program, the rpc the host had set", m["program"] == MSG2
          and m["legacy_program"] == MSG1 and m["rpc"] == "https://host.example/rpc" and m["note"] == "n")
    w = json.loads((www / "wallet" / "config.json").read_text())
    check("wallet: messages and names v2 (messages v1 as legacy)", w["msg"] == {"program": MSG2, "legacyProgram": MSG1}
          and w["names"] == {"program": NAMES2})
    check("wallet: the host's bridge settings kept where the bundle has none, the bundle's where it has one",
          w["bridge"]["vault"] == "0x1949" and w["bridge"]["token"] == {"symbol": "TUSD"} and w["bridge"]["evmChainId"] == 11155111
          and w["bridge"]["program"] == BRIDGE and w["attestation"]["program"] == ATT)
    e = json.loads((www / "config.json").read_text())
    acc = {p["key"]: p["account"] for p in e["programs"]}
    check("explorer: its catalogue keeps v1 (the IDL and image it reads)", acc["msg"] == MSG1 and acc["attestation"] == ATT)
    a = (www / "AGENTS.md").read_text()
    check("AGENTS.md: each section once (names v2 already there, messages v2 and the limits appended), the host named",
          a.count("## Names (`ferrata_names` v2)") == 1 and a.count("## Encrypted messages (`ferrata_msg_v2`)") == 1
          and a.count("## Limits of the public endpoints") == 1 and "v2 on net.example" in a and "at most 50 blocks per call" in a)
    run(www, stage, here)
    check("a second run changes nothing", (www / "AGENTS.md").read_text() == a and json.loads((www / "wallet" / "config.json").read_text()) == w)
with tempfile.TemporaryDirectory() as d:
    www, stage, here = site(pathlib.Path(d), v2=False)
    (stage / "served-before" / "msg-config.json").unlink()
    run(www, stage, here)
    m = json.loads((www / "msg" / "config.json").read_text())
    w = json.loads((www / "wallet" / "config.json").read_text())
    check("v1 alone: v1 everywhere; the module's rpc the site's own when the host had none", m["program"] == MSG1
          and m["rpc"] == "https://net.example/rpc" and w["msg"]["program"] == MSG1 and w["names"]["program"] == NAMES1
          and "legacyProgram" not in w["msg"])
print("all passed" if not FAILED else f"FAILED {len(FAILED)}")
sys.exit(1 if FAILED else 0)
