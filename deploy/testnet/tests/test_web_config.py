#!/usr/bin/env python3
"""deploy/testnet/web-config.py (install-web.sh's step 5) on a stand-in web root: the messaging module (a messages v2
client) uses v2, v1 named as legacy, and is left off without v2, never pointed at v1. The wallet's inbox and names
panels use the program their bundled client is written for: the wallet as this repository bundles it (v1 clients) gets
v1, also where v2 is deployed and a v2 config was served before (the fault found on the live network: the panels
pointed at programs their clients do not speak); a wallet with v2 clients gets v2 (v1 as legacy) and is left off
without it; an unrecognised client is left off. The explorer's catalogue keeps v1; what the host set in a served config
and the bundle leaves empty is kept, the live file's values first (a hand-set lezReady false stays false) and the copy
served after the last good run filling only what the live file lacks or leaves null, also when the last run stopped
after installing the bundle's raw configs; the module's rpc is the site's own; files, /AGENTS.md included, are written
whole (a new file renamed over the old); the program streams' AGENTS sections and the host's limits are in /AGENTS.md
once, also after a second run.
Run: python3 -I deploy/testnet/tests/test_web_config.py"""
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE.parent / "web-config.py"
BUNDLE = HERE.parent / "web" / "wallet"  # the wallet as this repository bundles it
CLIENTS = ("vendor/names.js", "msg/ferrata-msg.js")
STAND_IN = {"v1": ("const SEED_DOMAIN = enc.encode('ferrata-names/v1/name/');\n", "const SALT = utf8('ferrata-msg/v1');\n"),
            "v2": ("const SEED_DOMAIN = enc.encode('ferrata-names/v2/name/');\n",
                   "const SALT = utf8('ferrata-msg/v1');\nconst PDA_DOMAIN = utf8('ferrata-msg/v2/');\n")}
FAILED = []
MSG1, MSG2, NAMES1, NAMES2 = "ExZx" + "1" * 40, "Gnqr" + "2" * 40, "4EBU" + "3" * 40, "JCx9" + "4" * 40
ATT, BRIDGE = "6f9z" + "5" * 40, "36iq" + "6" * 40
RAW_WALLET = {"network": "bundle", "rpc": "/rpc", "msg": {"program": None}, "names": {"program": None},
              "bridge": {"program": None, "vault": None, "evmChainId": 11155111}}


def check(name, ok):
    print(("ok   " if ok else "FAIL ") + name)
    if not ok:
        FAILED.append(name)


def site(root, v2, clients="bundle"):
    """A web root as install-web.sh leaves it before step 5; `clients`: the wallet's own ("bundle"), stand-ins written
    for v1 or v2, or none (a wallet whose clients are not recognised)."""
    www, stage, here, served = root / "www", root / "stage", root / "here", root / "served"
    for d in (www / "msg", www / "wallet", stage / "msg", stage / "names", served, here / "host" / "www"):
        d.mkdir(parents=True, exist_ok=True)
    for i, rel in enumerate(CLIENTS):
        dst = www / "wallet" / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        if clients == "bundle":
            shutil.copyfile(BUNDLE / rel, dst)
        elif clients in STAND_IN:
            dst.write_text(STAND_IN[clients][i])
    progs = {"ferrata_attestation": {"program_account": ATT}, "ferrata_bridge_b1": {"program_account": BRIDGE},
             "ferrata_msg": {"program_account": MSG1}, "ferrata_names": {"program_account": NAMES1}}
    if v2:
        progs |= {"ferrata_msg_v2": {"program_account": MSG2}, "ferrata_names_v2": {"program_account": NAMES2}}
    (www / "programs.json").write_text(json.dumps({"programs": progs, "instances": {"sepolia": {"instance_id": "0x" + "ab" * 32}}}))
    (www / "config.json").write_text(json.dumps({"programs": [{"key": "msg", "account": ""}, {"key": "names", "account": ""},
                                                              {"key": "attestation", "account": ""}]}))
    (www / "msg" / "config.json").write_text(json.dumps({"rpc": None, "program": None, "legacy_program": MSG1, "note": "n"}))
    (www / "wallet" / "config.json").write_text(json.dumps(RAW_WALLET))
    # What the live host served before the fix: the wallet's panels on v2.
    (served / "wallet-config.json").write_text(json.dumps(
        {"bridge": {"program": BRIDGE, "vault": "0x1949", "token": {"symbol": "TUSD"}},
         "msg": {"program": MSG2, "legacyProgram": MSG1}, "names": {"program": NAMES2}}))
    (served / "msg-config.json").write_text(json.dumps({"rpc": "https://old-name.example/rpc", "program": MSG1}))
    (www / "AGENTS.md").write_text("# AGENTS.md\n\nat most 100 blocks per call on {{HOST}}\n\n## Names (`ferrata_names` v2) — for agents\n\nv2\n")
    (stage / "names" / "AGENTS.md").write_text("## Names (`ferrata_names` v2) — for agents\n\nv2\n")
    (stage / "msg" / "AGENTS-section.md").write_text("## Encrypted messages (`ferrata_msg_v2`)\n\nv2 on {{HOST}}\n")
    (here / "host" / "www" / "AGENTS-limits.md").write_text("## Limits of the public endpoints\n\nlimits\n")
    return www, stage, here, served


def run(www, stage, here, served):
    return subprocess.run([sys.executable, "-I", str(SCRIPT), str(www), "net.example", str(here), "50", str(stage), str(served)],
                          capture_output=True, text=True, timeout=60)


def configs(v2, clients):
    """(the messaging module's config, the wallet's) after a run on a fresh stand-in site."""
    with tempfile.TemporaryDirectory() as d:
        www, stage, here, served = site(pathlib.Path(d), v2, clients)
        r = run(www, stage, here, served)
        if r.returncode != 0:
            return {}, {"error": r.stderr[-300:]}
        return json.loads((www / "msg" / "config.json").read_text()), json.loads((www / "wallet" / "config.json").read_text())


with tempfile.TemporaryDirectory() as d:
    www, stage, here, served = site(pathlib.Path(d), v2=True)
    agents_inode = (www / "AGENTS.md").stat().st_ino
    r = run(www, stage, here, served)
    check("runs", r.returncode == 0)
    check("/AGENTS.md written whole: a new file renamed over the old, not rewritten in place",
          (www / "AGENTS.md").stat().st_ino != agents_inode)
    m = json.loads((www / "msg" / "config.json").read_text())
    check("messaging module: the v2 program, v1 as its legacy program, the site's own rpc (not a name served before)",
          m["program"] == MSG2 and m["legacy_program"] == MSG1 and m["rpc"] == "https://net.example/rpc" and m["note"] == "n")
    w = json.loads((www / "wallet" / "config.json").read_text())
    check("wallet as this repository bundles it (v1 clients, by their seed domains): messages and names v1, no legacy "
          "entry, with v2 deployed and over the v2 config served before", w["msg"] == {"program": MSG1} and w["names"] == {"program": NAMES1})
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
    run(www, stage, here, served)
    check("a second run changes nothing", (www / "AGENTS.md").read_text() == a and json.loads((www / "wallet" / "config.json").read_text()) == w)
    check("each file written whole: no temporary file left", not list(www.rglob("*.tmp")))
    # A run stopped after installing the bundle's raw configs (step 3), before step 5: the configs last served, kept
    # outside the stage, still hold the host's values, and the next run restores them.
    (www / "wallet" / "config.json").write_text(json.dumps(RAW_WALLET))
    (served / "wallet-config.json").write_text(json.dumps(w))
    run(www, stage, here, served)
    check("after a run stopped half way, the next one restores the host's values from the configs last served",
          json.loads((www / "wallet" / "config.json").read_text()) == w)

m, w = configs(v2=True, clients="v2")
check("a wallet with v2 clients: messages and names v2, messages v1 as legacy",
      w.get("msg") == {"program": MSG2, "legacyProgram": MSG1} and w.get("names") == {"program": NAMES2})
m, w = configs(v2=False, clients="bundle")
check("v1 alone: the messaging module left off (program null, also over a v1 program served before), v1 kept as legacy; "
      "the wallet as bundled (v1 clients) on v1",
      m.get("program") is None and m.get("legacy_program") == MSG1 and m.get("rpc") == "https://net.example/rpc"
      and w.get("msg") == {"program": MSG1} and w.get("names") == {"program": NAMES1})
m, w = configs(v2=False, clients="v2")
check("v1 alone, a wallet with v2 clients: its panels left off, never pointed at v1",
      (w.get("msg") or {}).get("program", "") is None and (w.get("names") or {}).get("program", "") is None)
m, w = configs(v2=True, clients="none")
check("a wallet whose clients are not recognised: its panels left off, whatever was served before",
      w.get("msg") == {"program": None} and w.get("names") == {"program": None})
m, w = configs(v2=True, clients="v1")
check("stand-in v1 clients read as v1 (the markers, not the bundle's other text)",
      w.get("msg") == {"program": MSG1} and w.get("names") == {"program": NAMES1})
with tempfile.TemporaryDirectory() as d:
    www, stage, here, served = site(pathlib.Path(d), v2=True)
    # The live file as this run found it (install-web.sh copies it into the stage before step 3): the operator closed
    # deposits and moved fromBlock after the last good run, whose served copy still says otherwise.
    (stage / "live-configs").mkdir()
    (stage / "live-configs" / "wallet-config.json").write_text(json.dumps(
        {"bridge": {"program": BRIDGE, "vault": "0x1949", "lezReady": False, "fromBlock": 12200000, "configId": None}}))
    (served / "wallet-config.json").write_text(json.dumps(
        {"bridge": {"program": BRIDGE, "vault": "0x1111", "lezReady": True, "fromBlock": 12196188, "configId": "0x37",
                    "token": {"symbol": "TUSD"}}}))
    run(www, stage, here, served)
    b = json.loads((www / "wallet" / "config.json").read_text())["bridge"]
    check("the live file's values win over the copy served after the last good run (lezReady false stays false, the "
          "vault and fromBlock as set live); that copy fills only what the live file lacks or leaves null (token, configId)",
          (b["lezReady"], b["fromBlock"], b["vault"], b["configId"], b["token"]) == (False, 12200000, "0x1949", "0x37", {"symbol": "TUSD"}))
print("all passed" if not FAILED else f"FAILED {len(FAILED)}")
sys.exit(1 if FAILED else 0)
