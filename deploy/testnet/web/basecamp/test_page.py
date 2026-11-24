#!/usr/bin/env python3
"""Checks the Basecamp page against its release record.

    python3 -I deploy/testnet/web/basecamp/test_page.py [--packages DIR]

- index.html is build.py's output for the committed release.json and tests/ (`build.py --check`: rendered and
  compared, nothing written);
- every package in release.json is on the page with its SHA-256 and size, and SHA256SUMS lists exactly those files
  with those values; the signature files and the public keys it links are in dl/ (the .lgx files themselves only with
  --packages DIR, a directory holding <platform>/*.lgx: the release directory, or the packages downloaded from the
  host's /basecamp/dl/<platform>/);
- the licence notices the page links are in dl/notices/, every text they name is there, and the GMP sources match
  scripts/native-deps.json;
- the install-test record holds no machine path, and the page's verify steps ask for lgx's trust line;
- the key is one key in every form: the SSH public key, the DID's x, the minisign key and Basecamp's form agree;
- with ssh-keygen on PATH: SHA256SUMS.sig and each package's .sig verify against dl/keys/allowed_signers (each .sig
  only with --packages);
- the page loads nothing from elsewhere: a CSP, no inline script or style, and no absolute URL but the repository's
  and the https://<this host> placeholder the page's script fills in.
Standard library (and ssh-keygen when present). Runs every check, then exits 1 if any failed.
"""
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DL = os.path.join(HERE, "dl")
fails = 0


def check(ok, what):
    global fails
    print(("ok   " if ok else "FAIL ") + what)
    if not ok:
        fails += 1


def main():
    packages_dir = None
    if len(sys.argv) == 3 and sys.argv[1] == "--packages":
        packages_dir = os.path.abspath(sys.argv[2])
    rel = json.load(open(os.path.join(HERE, "release.json"), encoding="utf-8"))
    page = open(os.path.join(HERE, "index.html"), encoding="utf-8").read()

    # Fresh: build.py's output for this release.json and tests/, compared without writing.
    r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "build.py"), "--check"], capture_output=True, text=True)
    check(r.returncode == 0, "index.html is build.py's output for the committed release.json and tests/")

    # The notices the page links, their texts and the sources they offer.
    notices = os.path.join(DL, "notices")
    check('href="dl/notices/NOTICES.md"' in page and os.path.isfile(os.path.join(notices, "NOTICES.md")),
          "the page links dl/notices/NOTICES.md and it is there")
    if os.path.isfile(os.path.join(notices, "NOTICES.md")):
        text = open(os.path.join(notices, "NOTICES.md"), encoding="utf-8").read()
        named = sorted(set(re.findall(r"\]\((licenses/[^)\s]+|crates\.md)\)", text)))
        missing = [n for n in named if not os.path.isfile(os.path.join(notices, n))]
        check(bool(named) and not missing, f"every file NOTICES.md names is in dl/notices/ ({len(named)} files)"
              + (f"; missing {missing}" if missing else ""))
        deps = json.load(open(os.path.join(HERE, "..", "..", "..", "..", "scripts", "native-deps.json"), encoding="utf-8"))
        for src in deps["sources"]:
            name = src["url"].rsplit("/", 1)[1]
            check(f"sources/{name}" in text and src["sha256"] in text,
                  f"NOTICES.md offers {name} with the sha256 scripts/native-deps.json pins")
            path = os.path.join(notices, "sources", name)
            if os.path.isfile(path):  # served with the packages, not committed (dl/.gitignore)
                check(hashlib.sha256(open(path, "rb").read()).hexdigest() == src["sha256"], f"dl/notices/sources/{name} matches it")

    # The record and the verify steps.
    left = []
    for root, _, files in os.walk(os.path.join(HERE, "tests")):
        for f in files:
            t = open(os.path.join(root, f), encoding="utf-8", errors="replace").read()
            if re.search(r"/h[o]me/|/Users/|sp[i]ke/tmp|Down[l]oads/", t):  # (bracketed so that a grep for machine paths skips this line)
                left.append(os.path.relpath(os.path.join(root, f), HERE))
    check(not left, "no machine path in tests/" + (f": {left[:3]}" if left else ""))
    check("Signer is trusted: ferrata" in page and "Signature is valid" in page, "the page's lgx check asks for lgx's trust line")

    sums = {}
    for line in open(os.path.join(DL, "SHA256SUMS"), encoding="ascii"):
        h, f = line.split()
        sums[f] = h
    files = {p["file"]: p for p in rel["packages"]}
    check(set(sums) == set(files), f"SHA256SUMS lists exactly the {len(files)} packages of release.json")
    for f, p in sorted(files.items()):
        check(sums.get(f) == p["sha256"], f"{f}: SHA256SUMS = release.json")
        check(p["sha256"] in page and f'href="dl/{f}"' in page, f"{f}: on the page with its SHA-256 and link")
        shown = f"{p['size'] / 1e6:.1f} MB" if p["size"] >= 1e6 else f"{p['size'] / 1e3:.1f} kB"
        check(shown in page and f'title="{p["size"]:,} bytes"' in page, f"{f}: its size on the page ({shown})")
        for ext in (".sig", ".minisig"):
            check(os.path.exists(os.path.join(DL, f + ext)), f"{f}{ext} is in dl/")
        if packages_dir:
            path = os.path.join(packages_dir, f)
            if os.path.exists(path):
                h = hashlib.sha256(open(path, "rb").read()).hexdigest()
                check(h == p["sha256"] and os.path.getsize(path) == p["size"], f"{f}: the package's bytes match")
            else:
                check(False, f"{f}: in {packages_dir}")

    # One key in every form.
    k = rel["key"]
    ssh = open(os.path.join(DL, "keys", "ferrata-release.pub"), encoding="ascii").read().split()
    pub = base64.b64decode(ssh[1])[-32:]
    did = open(os.path.join(DL, "keys", "ferrata-release.did"), encoding="ascii").read().strip()
    x = json.loads(base64.urlsafe_b64decode(did[8:] + "=" * (-len(did[8:]) % 4)))["x"]
    mini = base64.b64decode(open(os.path.join(DL, "keys", "minisign.pub"), encoding="ascii").read().splitlines()[1])
    bc = hashlib.sha256(pub).digest()[:8].hex().upper()
    check(base64.urlsafe_b64decode(x + "=" * (-len(x) % 4)) == pub, "the DID's key is the SSH key")
    check(mini[10:] == pub and mini[:2] == b"Ed", "the minisign key is the SSH key")
    check(k["did"] == did and k["public_key_hex"] == pub.hex(), "release.json names that key")
    check(k["basecamp_key"] == " ".join(bc[i:i + 4] for i in range(0, 16, 4)), "Basecamp's form of the key")
    for v in (k["ssh_fingerprint"], k["did"], k["minisign_public"], k["basecamp_key"]):
        check(v in page, f"the page shows {v[:24]}...")
    allowed = open(os.path.join(DL, "keys", "allowed_signers"), encoding="ascii").read().split()
    check(allowed[0] == "ferrata-release" and allowed[-1] == ssh[1], "allowed_signers names that key")

    if shutil.which("ssh-keygen"):
        def verify(path, sig):
            with open(path, "rb") as f:
                r = subprocess.run(["ssh-keygen", "-Y", "verify", "-f", os.path.join(DL, "keys", "allowed_signers"),
                                    "-I", "ferrata-release", "-n", "file", "-s", sig], stdin=f, capture_output=True)
            return r.returncode == 0
        check(verify(os.path.join(DL, "SHA256SUMS"), os.path.join(DL, "SHA256SUMS.sig")), "SHA256SUMS.sig verifies (ssh-keygen)")
        if packages_dir:
            for f in sorted(files):
                check(verify(os.path.join(packages_dir, f), os.path.join(DL, f + ".sig")), f"{f}.sig verifies (ssh-keygen)")

    # Nothing from elsewhere.
    check("Content-Security-Policy" in page, "a CSP")
    check(not re.search(r"<script(?![^>]*\bsrc=)", page), "no inline script")
    check(" style=" not in page and "<style" not in page, "no inline style")
    urls = set(re.findall(r"https?://[^\s\"'<>)]+", page))
    allowed_prefixes = ("https://github.com/rainbowpuffpuff/ferrata", "https://&lt;this")
    stray = sorted(u for u in urls if not u.startswith(allowed_prefixes))
    check(not stray, "no other absolute URL" + (f": {stray[:3]}" if stray else ""))
    print(f"{'ok' if not fails else 'FAILED'}: {fails} failures")
    sys.exit(1 if fails else 0)


main()
