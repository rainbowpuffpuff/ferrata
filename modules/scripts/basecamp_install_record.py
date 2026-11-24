#!/usr/bin/env python3
"""The Basecamp page's install-test record, from a run of modules/scripts/basecamp-install-test.sh (a headless Logos
Core with signature_policy require and the release key as its only trusted signer).

    python3 -I modules/scripts/basecamp_install_record.py RECORD_DIR OUT_DIR [HOST_BASE]

RECORD_DIR is the run's WORK/record: steps.tsv (name, expected, exit code, ms, pass, output), out/<step>.txt, req/,
live-*.json, versions.txt, roots.env (the run's absolute paths). Writes OUT_DIR/install-test.json (what the page shows)
and OUT_DIR/install-test/ (the record), with every absolute path of roots.env replaced by its name (WORK/, REL/,
UNSIGNED/) and HOST_BASE by https://<this host>; it refuses to write a record that still holds a home directory, a
machine path or the host's name. The release's commit is read from OUT_DIR/../release.json. Standard library only.
"""
import json
import os
import re
import shutil
import sys

if len(sys.argv) not in (3, 4):
    sys.exit("usage: basecamp_install_record.py RECORD_DIR OUT_DIR [HOST_BASE]")
rec, out = sys.argv[1], sys.argv[2]
host = sys.argv[3].rstrip("/") if len(sys.argv) == 4 else ""
RELEASE = json.load(open(os.path.join(out, "..", "release.json"), encoding="utf-8"))
SOURCE = RELEASE["source_commit"]
# A release re-signed in part gives each package its source_commit: each module must report its own.
COMMIT_OF = {p["name"]: p.get("source_commit", SOURCE) for p in RELEASE["packages"] if p["platform"] == "linux-amd64"}
MODULES = ["ferrata_attest_module", "ferrata_bridge_module", "ferrata_relayer_module", "ferrata_updater_module",
           "ferrata_attestor_module", "ferrata_monitor_module", "ferrata_bridge_ui"]
roots = dict(line.rstrip("\n").split("=", 1) for line in open(os.path.join(rec, "roots.env"), encoding="utf-8") if "=" in line)
# Longest first, so that a root inside another (LOGOSCTL in WORK) is replaced whole.
SUBS = sorted(((v, k) for k, v in roots.items() if k != "LOGOSCTL" and v), key=lambda kv: -len(kv[0]))


def scrub(text):
    for path, name in SUBS:
        text = text.replace(path, name)
    if host:
        text = text.replace(host, "https://<this host>")
        bare = re.sub(r"^https?://", "", host)
        if bare in text:
            raise SystemExit(f"basecamp_install_record.py: the host's name is still in the record")
    left = re.search(r"/h[o]me/|/Users/|/r[o]ot/|sp[i]ke/tmp|Down[l]oads/", text)  # (bracketed so that a grep for machine paths skips this line)
    if left:
        raise SystemExit(f"basecamp_install_record.py: a machine path is left in the record: …{text[max(0, left.start() - 40):left.end() + 40]}…")
    return text


def reply(step):
    """The module's reply: logosctl's JSON with the module's own JSON as a string in `result`."""
    raw = json.load(open(os.path.join(rec, "out", step + ".txt")))
    return json.loads(raw["result"]) if isinstance(raw.get("result"), str) else raw.get("result")


steps = {}
for line in open(os.path.join(rec, "steps.tsv"), encoding="utf-8"):
    name, expect, rc, ms, ok, _ = line.rstrip("\n").split("\t", 5)
    steps[name] = {"expect": expect, "exit": int(rc), "ms": int(ms), "pass": ok == "1"}

packages = []
for m in MODULES:
    row = {"name": m, "install": steps[f"install-{m}"]["pass"], "install_ms": steps[f"install-{m}"]["ms"], "calls": []}
    if m == "ferrata_bridge_ui":
        row["load"] = None
        row["calls"].append({"method": "package ls", "note": "listed as an installed ui module (Basecamp opens it; logosctl loads core modules only)"})
    else:
        row["load"] = steps[f"load-{m}"]["pass"]
        text = json.dumps(reply(f"version-{m}"))
        src = COMMIT_OF.get(m, SOURCE)
        ok = steps[f"version-{m}"]["pass"] and src in set(re.findall(r"\b[0-9a-f]{40}\b", text))
        row["calls"].append({"method": "version", "ok": ok, "note": f"reports the source commit {src[:8]}" if ok else "answered without the source commit"})
    packages.append(row)
by = {p["name"]: p for p in packages}

want = json.load(open(os.path.join(rec, "req", "deposit_vector.expect.json")))["expect_commitment"]
got = reply("deposit-vector")["commitment"]
by["ferrata_bridge_module"]["calls"].append({"method": "deposit", "ok": got == want,
    "note": "the specification's first deposit vector: commitment " + ("equal to the vector's" if got == want else f"{got}, NOT the vector's {want}")})

live = {}
if "live-read-instance" in steps:
    when = open(os.path.join(rec, "live-read-utc.txt"), encoding="utf-8").read().strip()
    r = reply("live-read-instance")
    status = json.load(open(os.path.join(rec, "live-status.json")))
    mon = status.get("attestation", {}).get("monitor", {})
    bridge = json.load(open(os.path.join(rec, "live-wallet-config.json"))).get("bridge", {})
    live = {"read_utc": when, "instance": r["id"], "finalized_slot": r["store"]["finalized_slot"], "applied": r["applied"],
            "fork": r["store"].get("fork"), "active": r["active"],
            "status_utc": status.get("utc"), "status_finalized_slot": mon.get("finalized_slot"),
            "status_applied": mon.get("applied_jobs"),
            "bridge": {k: bridge.get(k) for k in ("program", "configId", "vault", "evmChainId", "lezReady")}}
    by["ferrata_attest_module"]["calls"].append({"method": "read_instance", "ok": steps["live-read-instance"]["pass"],
        "note": f"this network's Sepolia light-client instance, read from its RPC by the test script at {when} and "
                f"decoded by the module: finalized slot {live['finalized_slot']:,} after {live['applied']} applied updates "
                f"(the status page's snapshot of {live['status_utc']}: {live['status_finalized_slot']:,} after "
                f"{live['status_applied']})"})
    dv = reply("deposit-live-vault")
    by["ferrata_bridge_module"]["calls"].append({"method": "deposit", "ok": steps["deposit-live-vault"]["pass"],
        "note": f"the deposit call for the vault this network's wallet configuration names (chain {bridge.get('evmChainId')}): "
                f"{dv['vault_call']['function']} to {dv['vault_call']['to']} (built, not sent)"})

refusals = []
for step, case in (("refuse-unsigned", "the same package unsigned"),
                   ("refuse-tampered", "the signed package with one bit of its plugin flipped"),
                   ("refuse-other-signer", "the package signed by another key under the release key's signer name")):
    said = json.load(open(os.path.join(rec, "out", step + ".txt"))).get("error", "")
    refusals.append({"case": case, "refused": steps[step]["pass"], "expected": steps[step]["expect"], "said": scrub(said)[:110]})

versions = open(os.path.join(rec, "versions.txt"), encoding="utf-8").read().splitlines()
result = {
    "driver": "modules/scripts/basecamp-install-test.sh",
    "steps": len(steps), "passed": sum(s["pass"] for s in steps.values()),
    "logosctl": "logosctl " + re.search(r"version (\S+)", versions[0]).group(1) + " (logos-logoscore-cli, its portable build)",
    "where": versions[1] + ", " + versions[2],
    "policy": "require", "packages": packages, "refusals": refusals, "live": live,
    "record": "deploy/testnet/web/basecamp/tests/install-test/",
}
texts = {}
for root, _, files in os.walk(rec):
    for name in files:
        src = os.path.join(root, name)
        texts[os.path.relpath(src, rec)] = scrub(open(src, encoding="utf-8").read())
os.makedirs(out, exist_ok=True)
with open(os.path.join(out, "install-test.json"), "w", encoding="utf-8") as f:
    f.write(scrub(json.dumps(result, indent=1)) + "\n")
dst = os.path.join(out, "install-test")
shutil.rmtree(dst, ignore_errors=True)
for rel, text in sorted(texts.items()):
    os.makedirs(os.path.dirname(os.path.join(dst, rel)), exist_ok=True)
    with open(os.path.join(dst, rel), "w", encoding="utf-8") as b:
        b.write(text)
print(f"install-test.json: {result['passed']} of {result['steps']} steps as expected; {len(packages)} packages")
