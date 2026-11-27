#!/usr/bin/env python3
"""Offline tests of `scripts/site-manifest.py verify` against a stand-in host: a local http.server, on
127.0.0.1, serving a fixture tree this test builds from this clone's own git blobs (loopback only; no
network beyond it).

The pass case is a host serving exactly what the repository holds: the fixture's files are the blobs at HEAD
of paths this repository tracks, and the manifest the check reads lists them as the real one does. Then each
way a host can differ is planted in turn, and each must fail the check:

- an altered file: one served file's bytes changed, so its SHA-256 is no longer the manifest's;
- a missing file: a file the manifest lists removed from the host;
- an extra file: an entry the host's own manifest adds that the repository's copy does not have;
- a file the repository does not hold: served as the manifest lists it, its SHA-256 in no committed
  SHA256SUMS under deploy/testnet/web/;
- a served manifest that differs from the repository's copy while every file it lists matches.

Nothing is written outside the repository's .scratch/ (never the system's temporary directory).
Run: python3 -I scripts/test_site_manifest.py  (standard library only, git, Python 3.11+)"""
import hashlib
import http.server
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCRATCH = pathlib.Path(ROOT) / ".scratch" / "test-site-manifest"

# What the stand-in host serves: files this repository tracks, read from this clone's git at HEAD (a blob,
# not the working tree, so a dirty tree cannot make the fixture disagree with the git check the tool makes).
# One .html, so the Content-Security-Policy check on served pages runs too.
SERVED = ("deploy/testnet/web/basecamp/index.html",
          "deploy/testnet/web/explorer/registry/index.html",
          "deploy/testnet/web/wallet/package.json",
          "names/js/cli.mjs")

spec = importlib.util.spec_from_file_location("sm", os.path.join(HERE, "site-manifest.py"))
sm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sm)

npass = nfail = 0


def check(name, ok, detail=""):
    global npass, nfail
    print(("ok   " if ok else "FAIL ") + name + (f"\n       {detail}" if not ok and detail else ""))
    if ok:
        npass += 1
    else:
        nfail += 1


def blobs():
    """{repo path: the blob's bytes at HEAD} for the files the stand-in host serves."""
    return {p: subprocess.run(["git", "-C", ROOT, "show", f"HEAD:{p}"], capture_output=True,
                              check=True).stdout for p in SERVED}


def manifest_of(blobs, head):
    """The manifest the repository's copy stands in for: HEAD's blobs, as the real manifest lists files."""
    files = [{"path": "/" + p, "sha256": hashlib.sha256(b).hexdigest(), "bytes": len(b),
              "commit": head, "repo_path": p, "branch": "HEAD", "public": True} for p, b in blobs.items()]
    files.sort(key=lambda f: f["path"])
    return {"about": "stand-in manifest for the offline test of scripts/site-manifest.py",
            "site": "http://127.0.0.1", "made": "2026-11-26T00:00Z", "made_how": "the test's own fixture",
            "public_checked": {"utc": "2026-11-26T00:00Z", "refs": {}},
            "files": files, "generated": [], "unmatched": []}


class Host(http.server.SimpleHTTPRequestHandler):
    """The stand-in host: the fixture tree, and the headers the tool checks (a directory listing for `/`)."""

    def end_headers(self):
        self.send_header("Strict-Transport-Security", "max-age=31536000")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'self'; frame-ancestors 'none'")
        super().end_headers()

    def log_message(self, *_args):
        pass


def serve(tree):
    """(base URL, the server thread): a host on 127.0.0.1 serving `tree`, started and stopped by the caller."""
    handler = lambda *a, directory=tree: Host(*a, directory=directory)
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return f"http://127.0.0.1:{httpd.server_address[1]}", httpd


def run_verify(site, reference):
    """`site-manifest.py verify` against the stand-in host, with `reference` as the repository's copy."""
    p = subprocess.run([sys.executable, "-I", os.path.join(HERE, "site-manifest.py"), "verify",
                        "--site", site, "--manifest", str(reference), "--no-second"],
                       cwd=ROOT, capture_output=True, text=True, timeout=300)
    return p.returncode, (p.stdout + p.stderr).splitlines()


def write_tree(blobs, manifest, changes=()):
    """A fresh fixture tree under the scratch directory: every blob at its manifest path, the manifest at
    its served path, and each `changes` function applied to the tree afterwards."""
    tree = SCRATCH / next(iter(set(f"tree-{i}" for i in range(1000)) - {p.name for p in SCRATCH.iterdir()}))
    for p, b in blobs.items():
        (tree / p).parent.mkdir(parents=True, exist_ok=True)
        (tree / p).write_bytes(b)
    served = tree / sm.SERVED_PATH.lstrip("/")   # where the tool reads the host's own manifest
    served.parent.mkdir(parents=True, exist_ok=True)
    served.write_text(json.dumps(manifest, indent=1) + "\n")
    for change in changes:
        change(tree)
    return tree


def main():
    SCRATCH.mkdir(parents=True, exist_ok=True)
    blobs_ = blobs()
    head = subprocess.run(["git", "-C", ROOT, "rev-parse", "HEAD"], capture_output=True, text=True,
                          check=True).stdout.strip()
    manifest = manifest_of(blobs_, head)
    reference = SCRATCH / "manifest.json"
    reference.write_text(json.dumps(manifest, indent=1) + "\n")

    # 1. the pass case: the host serves exactly what the repository holds
    tree = write_tree(blobs_, manifest)
    site, httpd = serve(tree)
    rc, out = run_verify(site, reference)
    check("the fixture passes: every served file equals the manifest and the git blob it names",
          rc == 0 and any("VERDICT: PASS" in ln for ln in out), f"exit {rc}\n" + "\n".join(out[-12:]))
    httpd.shutdown()

    # 2. an altered file: one byte of one served file changed
    def alter(t):
        p = t / SERVED[0]
        b = bytearray(p.read_bytes())
        b[len(b) // 2] ^= 1
        p.write_bytes(bytes(b))
    tree = write_tree(blobs_, manifest, (alter,))
    site, httpd = serve(tree)
    rc, out = run_verify(site, reference)
    check("an altered file fails it (its SHA-256 is no longer the manifest's)",
          rc == 1 and any("FAIL" in ln and "served" in ln and "manifest" in ln for ln in out),
          f"exit {rc}\n" + "\n".join(out[-12:]))
    httpd.shutdown()

    # 3. a missing file: a file the manifest lists, removed from the host
    tree = write_tree(blobs_, manifest, (lambda t: (t / SERVED[-1]).unlink(),))
    site, httpd = serve(tree)
    rc, out = run_verify(site, reference)
    check("a missing file fails it (the host serves one the manifest lists no longer)",
          rc == 1 and any("cannot fetch" in ln for ln in out), f"exit {rc}\n" + "\n".join(out[-12:]))
    httpd.shutdown()

    # 4. an extra file: the host's own manifest names a file the repository's copy does not have
    extra = json.loads(json.dumps(manifest))
    p, b = SERVED[0], blobs_[SERVED[0]]
    extra["files"].insert(0, {"path": "/extra.html", "sha256": hashlib.sha256(b).hexdigest(), "bytes": len(b),
                              "commit": head, "repo_path": p, "branch": "HEAD", "public": True})
    tree = write_tree(blobs_, extra)
    (tree / "extra.html").write_bytes(b)
    site, httpd = serve(tree)
    rc, out = run_verify(site, reference)
    check("an extra file in the host's manifest fails it (the served manifest differs from the repository's)",
          rc == 1 and any("the served manifest differs from the repository's copy" in ln for ln in out),
          f"exit {rc}\n" + "\n".join(out[-12:]))
    httpd.shutdown()

    # 5. a file the repository does not hold: listed as `unmatched`, its SHA-256 in no committed SHA256SUMS
    blob = b"a file the repository does not hold, and no SHA256SUMS pins\n"
    unmatched = json.loads(json.dumps(manifest))
    unmatched["unmatched"] = [{"path": "/release.tar.gz", "sha256": hashlib.sha256(blob).hexdigest(),
                                "bytes": len(blob)}]
    reference2 = SCRATCH / "manifest-unmatched.json"
    reference2.write_text(json.dumps(unmatched, indent=1) + "\n")
    tree = write_tree(blobs_, unmatched, (lambda t: (t / "release.tar.gz").write_bytes(blob),))
    site, httpd = serve(tree)
    rc, out = run_verify(site, reference2)
    check("a served file no committed SHA256SUMS pins fails it, though the manifest lists it",
          rc == 1 and any("in no committed SHA256SUMS" in ln for ln in out), f"exit {rc}\n" + "\n".join(out[-12:]))
    httpd.shutdown()

    # 6. the host's manifest differs while every file it lists matches (a byte of the about text)
    other = json.loads(json.dumps(manifest))
    other["made"] = "2026-11-27T00:00Z"
    tree = write_tree(blobs_, other)
    site, httpd = serve(tree)
    rc, out = run_verify(site, reference)
    check("a served manifest that differs from the repository's copy fails it, files apart",
          rc == 1 and any("the served manifest differs from the repository's copy" in ln for ln in out),
          f"exit {rc}\n" + "\n".join(out[-12:]))
    httpd.shutdown()

    shutil.rmtree(SCRATCH, ignore_errors=True)
    print()
    print(f"test_site_manifest.py: {npass} passed, {nfail} failed")
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
