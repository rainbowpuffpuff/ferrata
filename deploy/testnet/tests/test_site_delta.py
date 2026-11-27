#!/usr/bin/env python3
"""deploy/testnet/site-delta.py: the plan against a host's manifest, and the package's install.sh on a scratch web root.

    python3 -I deploy/testnet/tests/test_site_delta.py

The manifests are built here from the repository at HEAD (every file the host would serve from the build page's
bundle and the wallet, as served now), then changed the way a host drifts: one file older, one missing, one that HEAD
no longer has. install.sh runs for real (bash, sha256sum, install), as the current user, under a scratch directory
beside this file. Standard library only.
"""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("site_delta", HERE.parent / "site-delta.py")
sd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sd)
HEAD = sd.git("rev-parse", "HEAD").decode().strip()
ME = f"{os.getuid()}:{os.getgid()}"


def served_now():
    """A manifest equal to HEAD: the build page's bundle and the wallet's files, as site-manifest.py lists them."""
    head = sd.blobs(HEAD, [f"{sd.BUILD}/build.json", f"{sd.WALLET}/sw.js"])
    paths = {"/" + f: f"{sd.BUILD}/{f}" for f in json.loads(head[f"{sd.BUILD}/build.json"])["files"]}
    for f in sd.sw_files(head[f"{sd.WALLET}/sw.js"].decode()) + ["sw.js"]:
        paths["/wallet/" + f] = f"{sd.WALLET}/{f}"
    data = sd.blobs(HEAD, paths.values())
    files = [{"path": p, "sha256": hashlib.sha256(data[r]).hexdigest(), "repo_path": r} for p, r in sorted(paths.items())]
    return {"made": "test", "files": files}, data, paths


class Plan(unittest.TestCase):
    def test_a_host_equal_to_head_needs_nothing(self):
        m, _, _ = served_now()
        p = sd.plan(m, HEAD, set(sd.PARTS))
        self.assertEqual((p["install"], p["left_out"], p["gone"]), ([], [], []))

    def test_drift_is_found_and_sorted_by_part(self):
        m, data, paths = served_now()
        by = {e["path"]: e for e in m["files"]}
        by["/index.html"]["sha256"] = "0" * 64                       # older on the host
        by["/wallet/bridge.js"]["sha256"] = "1" * 64
        m["files"] = [e for e in m["files"] if e["path"] != "/testers/index.html"]   # not served yet
        m["files"].append({"path": "/old.html", "sha256": "2" * 64, "repo_path": "deploy/testnet/web/build/old.html"})
        m["files"].append({"path": "/network.html", "sha256": "3" * 64, "repo_path": "deploy/testnet/web/explorer/index.html"})
        p = sd.plan(m, HEAD, {"build", "wallet"})
        got = {(path, old, part) for path, old, _d, part, _r in p["install"]}
        self.assertEqual(got, {("/index.html", "0" * 64, "build"), ("/wallet/bridge.js", "1" * 64, "wallet"),
                               ("/testers/index.html", None, "build")})
        self.assertEqual([x[0] for x in p["left_out"]], ["/network.html"])
        self.assertEqual(p["gone"], [("/old.html", "deploy/testnet/web/build/old.html")])
        for path, _old, d, _part, repo_path in p["install"]:
            self.assertEqual(d, data[repo_path], path)                 # the bytes HEAD has
        self.assertEqual([x[0] for x in sd.plan(m, HEAD, set(sd.PARTS))["left_out"]], [])

    def test_a_manifest_hash_that_is_not_hex_is_refused(self):
        m, _, _ = served_now()
        by = {e["path"]: e for e in m["files"]}
        by["/index.html"]["sha256"] = "0" * 63 + "\n"
        with tempfile.TemporaryDirectory(dir=HERE) as d, self.assertRaises(SystemExit):
            sd.pack(sd.plan(m, HEAD, {"build"}), HEAD, Path(d))

    def test_a_path_install_sh_could_misread_is_refused(self):
        m, _, _ = served_now()
        m["files"].append({"path": "/a b.html", "sha256": "4" * 64, "repo_path": f"{sd.BUILD}/index.html"})
        with self.assertRaises(SystemExit):
            sd.plan(m, HEAD, {"build", "wallet", "other"})


class Pack(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir=HERE)
        self.d = Path(self.tmp.name)
        m, self.data, self.paths = served_now()
        self.old = {"/index.html": b"<p>the page the host serves now</p>\n", "/wallet/bridge.js": b"// old\n"}
        by = {e["path"]: e for e in m["files"]}
        for path, body in self.old.items():
            by[path]["sha256"] = hashlib.sha256(body).hexdigest()
        m["files"] = [e for e in m["files"] if e["path"] != "/testers/index.html"]
        self.plan = sd.plan(m, HEAD, {"build", "wallet"})
        self.tar = sd.pack(self.plan, HEAD, self.d / "out")
        self.pkg = self.d / "pkg"
        with tarfile.open(self.tar) as t:
            self.names = sorted(t.getnames())
            t.extractall(self.pkg, filter="data")
        self.www = self.d / "www"
        for path, body in self.old.items():
            (self.www / path.lstrip("/")).parent.mkdir(parents=True, exist_ok=True)
            (self.www / path.lstrip("/")).write_bytes(body)

    def tearDown(self):
        self.tmp.cleanup()

    def install(self, path_prefix=None, **extra):
        env = dict(os.environ, WWW=str(self.www), OWNER=ME, BACKUP=str(self.d / "backup"), **extra)
        if path_prefix:
            env["PATH"] = f"{path_prefix}:{env['PATH']}"
        return subprocess.run(["bash", str(self.pkg / "install.sh")], env=env, capture_output=True, text=True)

    def test_the_package_holds_exactly_the_plan(self):
        want = sorted(["www/index.html", "www/wallet/bridge.js", "www/testers/index.html", "SHA256SUMS", "EXPECT", "install.sh",
                       "rollback.sh"])
        self.assertEqual(self.names, want)
        self.assertEqual(sorted((self.pkg / "EXPECT").read_text().splitlines()), sorted([
            f"{hashlib.sha256(self.old['/index.html']).hexdigest()} index.html",
            f"{hashlib.sha256(self.old['/wallet/bridge.js']).hexdigest()} wallet/bridge.js", "absent testers/index.html"]))

    def test_install_replaces_all_and_keeps_the_old_ones(self):
        r = self.install()
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        for path in ("/index.html", "/wallet/bridge.js", "/testers/index.html"):
            self.assertEqual((self.www / path.lstrip("/")).read_bytes(), self.data[self.paths[path]], path)
            self.assertEqual(oct((self.www / path.lstrip("/")).stat().st_mode & 0o777), "0o644")
        self.assertEqual((self.d / "backup/index.html").read_bytes(), self.old["/index.html"])
        self.assertFalse(list(self.www.rglob("*.site-delta-new")))
        self.assertEqual(self.install().returncode, 1)               # a second run finds the host moved: nothing

    def test_a_host_that_moved_gets_nothing(self):
        (self.www / "wallet/bridge.js").write_bytes(b"// changed on the host meanwhile\n")
        r = self.install()
        self.assertEqual(r.returncode, 1)
        self.assertIn("not what the manifest says", r.stdout)
        self.assertIn("nothing installed", r.stdout)
        self.assertEqual((self.www / "index.html").read_bytes(), self.old["/index.html"])
        self.assertFalse((self.www / "testers/index.html").exists())

    def test_a_target_that_should_be_absent(self):
        (self.www / "testers").mkdir()
        (self.www / "testers/index.html").write_text("someone's page")
        r = self.install()
        self.assertEqual(r.returncode, 1)
        self.assertIn("present, the manifest says absent: testers/index.html", r.stdout)

    def test_a_failure_while_staging_changes_nothing(self):
        (self.www / "testers").mkdir()
        (self.www / "testers").chmod(0o555)                          # the third file cannot be staged
        try:
            r = self.install()
        finally:
            (self.www / "testers").chmod(0o755)
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("staging failed: nothing installed", r.stdout)
        self.assertEqual((self.www / "index.html").read_bytes(), self.old["/index.html"])
        self.assertEqual((self.www / "wallet/bridge.js").read_bytes(), self.old["/wallet/bridge.js"])
        self.assertFalse(list(self.www.rglob("*.site-delta-new")))

    def test_a_rename_that_fails_puts_back_the_ones_before_it(self):
        shim = self.d / "shim"
        shim.mkdir()
        real = subprocess.run(["bash", "-c", "command -v mv"], capture_output=True, text=True).stdout.strip()
        (shim / "mv").write_text(  # the third rename fails, after index.html and the new testers/index.html moved
            "#!/bin/bash\nn=$(( $(cat \"$MV_COUNT\" 2>/dev/null || echo 0) + 1 )); echo $n > \"$MV_COUNT\"\n"
            f"[ \"$n\" = 3 ] && {{ echo 'mv: injected failure' >&2; exit 1; }}\nexec {real} \"$@\"\n")
        (shim / "mv").chmod(0o755)
        r = self.install(path_prefix=str(shim), MV_COUNT=str(self.d / "mv-count"))
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("a rename failed: the files already renamed were put back; nothing installed", r.stdout)
        self.assertEqual((self.www / "index.html").read_bytes(), self.old["/index.html"])        # renamed, then put back
        self.assertEqual((self.www / "wallet/bridge.js").read_bytes(), self.old["/wallet/bridge.js"])
        self.assertFalse((self.www / "testers/index.html").exists())
        self.assertFalse([p for p in self.www.rglob("*") if p.name.endswith((".site-delta-new", ".site-delta-old"))])

    def test_rollback_puts_back_what_install_replaced(self):
        self.assertEqual(self.install().returncode, 0)
        env = dict(os.environ, WWW=str(self.www), OWNER=ME)
        r = subprocess.run(["bash", str(self.pkg / "rollback.sh"), str(self.d / "backup")], env=env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("put back 3 file(s)", r.stdout)
        for path, body in self.old.items():
            self.assertEqual((self.www / path.lstrip("/")).read_bytes(), body, path)
        self.assertFalse((self.www / "testers/index.html").exists())
        r = subprocess.run(["bash", str(self.pkg / "rollback.sh"), str(self.d / "no-such-backup")], env=env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 1)

    def test_a_damaged_package_installs_nothing(self):
        p = self.pkg / "www/index.html"
        p.write_bytes(p.read_bytes() + b" ")
        r = self.install()
        self.assertEqual(r.returncode, 1)
        self.assertIn("not whole", r.stdout)
        self.assertEqual((self.www / "index.html").read_bytes(), self.old["/index.html"])


class PackGuard(unittest.TestCase):
    def test_pack_refuses_a_stale_bundle_unless_told(self):
        with tempfile.TemporaryDirectory(dir=HERE) as d:
            m, _, _ = served_now()
            m["files"][0]["sha256"] = "6" * 64
            f, out = Path(d) / "manifest.json", Path(d) / "out"
            f.write_text(json.dumps(m))
            saved = sd.bundle_fresh
            try:
                sd.bundle_fresh = lambda commit: (False, "stale, as the test says")
                with self.assertRaises(SystemExit) as cm, contextlib.redirect_stdout(io.StringIO()):
                    sd.main(["pack", "--manifest", str(f), "--out", str(out)])
                self.assertIn("refusing to pack: stale, as the test says", str(cm.exception))
                self.assertFalse(out.exists())
                with contextlib.redirect_stdout(io.StringIO()):
                    sd.main(["pack", "--manifest", str(f), "--out", str(out), "--allow-stale"])
                self.assertEqual(len(list(out.glob("site-*.tar"))), 1)
            finally:
                sd.bundle_fresh = saved

    def test_a_dirty_tree_and_a_stale_bundle_are_refused(self):
        """bundle_fresh's own checks, in a scratch worktree of HEAD: a changed tracked file, then a bundle file that
        differs from a fresh build while git sees a clean tree."""
        saved = sd.ROOT
        with tempfile.TemporaryDirectory(dir=HERE) as d:
            wt = Path(d) / "wt"
            sd.git("worktree", "add", "--detach", "--quiet", str(wt), "HEAD")
            try:
                sd.ROOT = wt
                (wt / "README.md").write_text((wt / "README.md").read_text() + "\nchanged\n")
                self.assertIn("the working tree has changes", sd.bundle_fresh(HEAD)[1])
                sd.git("checkout", "--", "README.md")
                bj = f"{sd.BUILD}/build.json"
                sd.git("update-index", "--assume-unchanged", bj)
                (wt / bj).write_text((wt / bj).read_text() + " ")
                self.assertIn("site/build.py --check", sd.bundle_fresh(HEAD)[1])
            finally:
                sd.ROOT = saved
                sd.git("worktree", "remove", "--force", str(wt))

    def test_only_the_checked_out_commit(self):
        ok, why = sd.bundle_fresh(sd.git("rev-parse", "HEAD~1").decode().strip())
        self.assertFalse(ok)
        self.assertIn("is not the checked-out commit", why)


class Cli(unittest.TestCase):
    def test_plan_from_a_manifest_file(self):
        with tempfile.TemporaryDirectory(dir=HERE) as d:
            m, _, _ = served_now()
            m["files"][0]["sha256"] = "5" * 64
            f = Path(d) / "manifest.json"
            f.write_text(json.dumps(m))
            r = subprocess.run(["python3", "-I", str(HERE.parent / "site-delta.py"), "plan", "--manifest", str(f), "--parts", "all"],
                               capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertIn("install: 1", r.stdout)
            self.assertIn(m["files"][0]["path"], r.stdout)


if __name__ == "__main__":
    unittest.main()
