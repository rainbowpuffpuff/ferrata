#!/usr/bin/env python3
"""Offline checks of the trust pages: python3 -I deploy/testnet/web/trust/test_trust.py (exit 1 on a failure)."""
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
fails = []


def check(ok, msg):
    if not ok:
        fails.append(msg)
        print("FAIL", msg)


# 1. the committed pages are what build.py makes
r = subprocess.run([sys.executable, "-I", str(HERE / "build.py"), "--check"], capture_output=True, text=True)
check(r.returncode == 0, "build.py --check: " + r.stdout.strip())
# 2. the served registry is the reproduction list, byte for byte in content
check(json.loads((HERE / "apps/registry.json").read_text()) == json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text()),
      "apps/registry.json differs from scripts/reproduce/deployed-programs.json")
reg = json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text())
for p in reg["programs"]:
    check(re.fullmatch(r"0x[0-9a-f]{64}", p["image_id"]) and re.fullmatch(r"[0-9a-f]{64}", p["binary_sha256"]), f"{p['name']}: id format")
    check(p["source"] in {s["id"] for s in reg["sources"]}, f"{p['name']}: unknown source")
# 3. every recorded rebuild row equals the list
run = ROOT / "scripts/reproduce/runs/2026-11-23-deployed-testnet"
rows = {}
for t in run.glob("*.tsv"):
    lines = t.read_text().splitlines()
    for line in lines[1:]:
        c = dict(zip(lines[0].split("\t"), line.split("\t")))
        rows[(t.stem, c["name"])] = c
for p in reg["programs"]:
    c = rows.get((p["source"], p["guest"]))
    check(c is not None and c["image_id"].removeprefix("0x") == p["image_id"][2:] and c["sha256_bin"] == p["binary_sha256"]
          and c["r0vm_id"] == "= id", f"{p['name']}: the recorded rebuild does not equal the list")
res = json.loads((run / "result.json").read_text())
check([q["name"] for q in res["programs"]] == [p["name"] for p in reg["programs"]], "result.json: not the list's programs")
check(all(q.get("ok") and q.get("immutable") is True and q.get("r0vm") == "= id" for q in res["programs"]),
      "result.json: a program is not ok (rebuilt, r0vm, published and immutable on chain)")
check(res.get("not_in_list") == [], "result.json: deployed programs missing from the list")
check(json.loads((HERE / "apps/rebuild.json").read_text())["summary"].startswith(f"{len(reg['programs'])} of {len(reg['programs'])} "),
      "apps/rebuild.json does not say every program was rebuilt")
# 4. the manifest: well formed, no duplicates, every trust page listed with its committed bytes
m = json.loads((HERE / "bridge/verify/manifest.json").read_text())
paths = [f["path"] for f in m["files"]]
check(len(paths) == len(set(paths)), "manifest: duplicate paths")
for f in m["files"]:
    check(re.fullmatch(r"[0-9a-f]{64}", f["sha256"]) and re.fullmatch(r"[0-9a-f]{40}", f["commit"]), f"manifest: {f['path']} malformed")
for f in m["files"]:
    if f["repo_path"].startswith("deploy/testnet/web/trust/") and f["path"] != "/bridge/verify/manifest.json":
        local = HERE / f["path"].lstrip("/")
        check(local.exists() and hashlib.sha256(local.read_bytes()).hexdigest() == f["sha256"], f"manifest: {f['path']} is not this tree's file")
check("/bridge/verify/manifest.json" not in paths, "the manifest lists itself")
listed = {f["path"]: f["sha256"] for f in m["files"]}
for d in ("trust", "bridge", "apps", "accessibility"):
    for local in sorted((HERE / d).rglob("*")):
        rel = "/" + str(local.relative_to(HERE))
        if local.is_file() and rel != "/bridge/verify/manifest.json":
            check(listed.get(rel) == hashlib.sha256(local.read_bytes()).hexdigest(), f"manifest: {rel} missing or stale")
check(all("public" in f for f in m["files"]), "manifest: an entry without its public flag")
# 4c. the accessibility record covers every page the check lists, every trust page among them
sys.path.insert(0, str(ROOT / "scripts/a11y"))
from check import PAGES  # noqa: E402  (the module imports Playwright only when run)
a11y = json.loads((HERE / "accessibility/results.json").read_text())
check([p["path"] for p in a11y["pages"]] == PAGES, "accessibility/results.json: not the pages scripts/a11y/check.py lists")
for p in a11y["pages"]:
    if p["owner"] == "trust pages":
        check(not p["violations"] and p["keyboard"].get("skip_link") and p["keyboard"].get("trap") == "none",
              f"accessibility: {p['path']} fails a rule or the keyboard pass")
# 5. every page: language, one h1, main landmark, skip link first, CSP meta, no inline script or style, no private names
for page in sorted(HERE.glob("*/**/index.html")) + sorted(HERE.glob("*/index.html")):
    t = page.read_text()
    rel = page.relative_to(HERE)
    check('<html lang="en">' in t, f"{rel}: lang")
    check(t.count("<h1>") == 1, f"{rel}: one h1")
    check('<main id="main"' in t and 'class="skip" href="#main"' in t, f"{rel}: main and skip link")
    check("Content-Security-Policy" in t and "script-src 'self'" in t, f"{rel}: CSP meta")
    check(not re.search(r"<script>(?!</script>)|<style>|style=\"| on[a-z]+=\"", t), f"{rel}: inline script or style")
WORDS = [w for w in os.environ.get("TRUST_PRIVATE_WORDS", "").split(",") if w]  # kept out of the repository
if not WORDS:
    print("SKIPPED: the private-word check (set TRUST_PRIVATE_WORDS=word,word to run it); machine paths and e-mail "
          "addresses are still checked")
for f in [x for x in [*HERE.rglob("*.html"), *HERE.rglob("*.json"), *HERE.rglob("*.js"), *HERE.rglob("*.py")] if x.name != "test_trust.py"]:
    t = f.read_text()
    words = WORDS
    check(not re.search(r"/home/|/Users/|[\w.+-]+@[\w-]+\.[a-z]{2,}", t) and not any(w in t for w in words),
          f"{f.relative_to(HERE)}: machine path, e-mail or private word")
print("ok" if not fails else f"{len(fails)} failed")
sys.exit(1 if fails else 0)
