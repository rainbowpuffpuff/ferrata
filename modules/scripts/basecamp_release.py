#!/usr/bin/env python3
"""The Basecamp release's record (release.json), from a signed release directory (basecamp-release.sh sign).

    python3 -I modules/scripts/basecamp_release.py assemble REL

REL holds <platform>/*.lgx (signed), keys/ (the public key in its forms), SHA256SUMS, and one
build-<platform>.json per platform: basecamp-release.sh build's build.json, or for packages built elsewhere (the
macos workflow's artifact) a file of the same shape with a "built_by" object. Every value is read from those files or
from the packages themselves (`lgx manifest --json`, `lgx signature`, which must be on PATH); release.json is
written into REL and printed in short. Standard library only.
"""
import base64
import hashlib
import json
import os
import subprocess
import sys
import time


def die(msg):
    sys.exit(f"basecamp_release.py: {msg}")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def lgx(*args):
    return subprocess.run(["lgx", *args], check=True, capture_output=True, text=True).stdout


def key_record(rel):
    k = os.path.join(rel, "keys")
    ssh_pub = open(os.path.join(k, "ferrata-release.pub"), encoding="ascii").read().split()
    raw = base64.b64decode(ssh_pub[1])
    # ssh-ed25519 public blob: string "ssh-ed25519", string key (32 bytes)
    if len(raw) != 51 or raw[4:15] != b"ssh-ed25519":
        die("keys/ferrata-release.pub is not an Ed25519 key")
    pub = raw[-32:]
    did = open(os.path.join(k, "ferrata-release.did"), encoding="ascii").read().strip()
    jwk = json.loads(base64.urlsafe_b64decode(did[len("did:jwk:"):] + "=" * (-len(did[len("did:jwk:"):]) % 4)))
    x = base64.urlsafe_b64decode(jwk["x"] + "=" * (-len(jwk["x"]) % 4))
    if x != pub:
        die("the DID and the SSH key are not the same key")
    mini = open(os.path.join(k, "minisign.pub"), encoding="ascii").read().splitlines()
    mraw = base64.b64decode(mini[1])
    if mraw[:2] != b"Ed" or mraw[10:] != pub:
        die("minisign.pub is not the same key")
    fpr = open(os.path.join(k, "fingerprint.txt"), encoding="ascii").read().split()[1]
    bc = hashlib.sha256(pub).digest()[:8].hex().upper()
    return {
        "name": "ferrata-release",
        "signer_name": "Ferrata release (test network)",
        "public_key_hex": pub.hex(),
        "ssh_public": " ".join(ssh_pub[:2]),
        "ssh_fingerprint": fpr,
        "did": did,
        "minisign_public": mini[1],
        "minisign_key_id": mraw[2:10][::-1].hex().upper(),
        # Basecamp's form (logos-basecamp app/utils/InstallTrust.cpp, fingerprint()): SHA-256 of the 32-byte public
        # key, its first 8 bytes in upper-case hex, in groups of four.
        "basecamp_key": " ".join(bc[i:i + 4] for i in range(0, 16, 4)),
    }


def assemble(rel):
    rel = os.path.abspath(rel)
    key = key_record(rel)
    sums = {}
    for line in open(os.path.join(rel, "SHA256SUMS"), encoding="ascii"):
        h, f = line.split()
        sums[f] = h
    platforms, packages = {}, []
    for name in sorted(os.listdir(rel)):
        if not (name.startswith("build-") and name.endswith(".json")):
            continue
        b = json.load(open(os.path.join(rel, name), encoding="utf-8"))
        pid = b["platform"]
        platforms[pid] = {k: v for k, v in b.items() if k != "packages"}
        for p in b["packages"]:
            path = os.path.join(rel, p["file"])
            if not os.path.exists(path):
                die(f"{p['file']} is in {name} but not in {rel}")
            manifest = json.loads(lgx("manifest", path, "--json"))
            sig = json.loads(lgx("signature", path) or "null") or {}
            digest = sha256(path)
            if sums.get(p["file"]) != digest:
                die(f"{p['file']}: SHA256SUMS does not match the file")
            if manifest.get("hashes", {}).get("root") != p["rootHash"]:
                die(f"{p['file']}: signing changed the rootHash ({p['rootHash']} -> {manifest.get('hashes', {}).get('root')})")
            if sig.get("did") != key["did"]:
                die(f"{p['file']}: signed by {sig.get('did')}, not the release key")
            packages.append({
                "name": p["name"], "version": p["version"], "platform": pid, "file": p["file"],
                "display_name": manifest.get("display_name", ""), "type": manifest.get("type", ""),
                "category": manifest.get("category", ""),
                "size": os.path.getsize(path), "sha256": digest, "rootHash": p["rootHash"],
                "unsigned_size": p["size"], "unsigned_sha256": p["sha256"],
                "signer_did": sig.get("did"), "signer_name": (sig.get("signer") or {}).get("name") or sig.get("name"),
                "export_commit": p.get("export_commit", ""), "export_tree": p.get("export_tree", ""),
                **({"origin": p["origin"]} if p.get("origin") else {}),
            })
    commits = {v.get("source_commit") for v in platforms.values()}
    if len(commits) != 1:
        die(f"the platforms were built from different commits: {sorted(commits)}")
    out = {
        "source_commit": commits.pop(),
        "signed_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(os.path.getmtime(os.path.join(rel, "SHA256SUMS")))),
        "key": key,
        "platforms": platforms,
        "packages": sorted(packages, key=lambda p: (p["platform"], p["name"])),
    }
    with open(os.path.join(rel, "release.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=1)
        f.write("\n")
    print(json.dumps({"packages": len(packages), "platforms": sorted(platforms), "key": key["ssh_fingerprint"],
                      "basecamp_key": key["basecamp_key"]}))


if len(sys.argv) != 3 or sys.argv[1] != "assemble":
    die("usage: basecamp_release.py assemble REL")
assemble(sys.argv[2])
