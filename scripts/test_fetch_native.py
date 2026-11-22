"""Tests of scripts/fetch_native.py (review-code-ops M1): `python3 -I scripts/test_fetch_native.py`.

Synthetic archives served from file:// URLs, with pins written for them: a good fetch and its cache; a corrupted
cached archive (refused, nothing exported; --refetch replaces it); a wrong hash or size at the origin; members that
would leave the directory, links, missing pinned files; a modified unpacked file; and check-build on cargo
build-script messages (the archives linked from where they were checked, a build script's own download refused, a
library path outside the build refused, a new static library refused). Standard library only; files go to a temporary
directory (TMPDIR).
"""

import hashlib
import importlib.util
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "fetch_native.py")
_spec = importlib.util.spec_from_file_location("fetch_native", SCRIPT)
fn = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fn)

RS_FILES = {"rs-1/lib/librapidsnark.a": b"rapidsnark archive", "rs-1/lib/libgmp.a": b"gmp 6.3.0 archive"}
LBC_FILES = {"lbc-1/lib/libgmp.a": b"gmp 6.2.1 archive", "lbc-1/pol/libpol.a": b"pol witness generator",
             "lbc-1/pol/proving_key.zkey": b"zkey bytes"}


def sha(b):
    return hashlib.sha256(b).hexdigest()


def make_zip(files):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, data in files.items():
            z.writestr(name, data)
    return buf.getvalue()


def make_tgz(files, extra=None):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as t:
        for name, data in files.items():
            ti = tarfile.TarInfo(name)
            ti.size = len(data)
            t.addfile(ti, io.BytesIO(data))
        for ti in extra or []:
            t.addfile(ti, io.BytesIO(b"x" * ti.size) if ti.isfile() else None)
    return buf.getvalue()


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.t = self.tmp.name
        self.origin = os.path.join(self.t, "origin")
        self.cache = os.path.join(self.t, "cache")
        os.makedirs(self.origin)
        self.publish(make_zip(RS_FILES), make_tgz(LBC_FILES))

    def tearDown(self):
        self.tmp.cleanup()

    def publish(self, rs_bytes, lbc_bytes, pin_rs=None, pin_lbc=None, rs_files=None, lbc_files=None):
        """Put the archives at the origin and write pins for them (pin_*: the bytes the pins are computed from)."""
        for name, data in (("rs.zip", rs_bytes), ("lbc.tar.gz", lbc_bytes)):
            with open(os.path.join(self.origin, name), "wb") as f:
                f.write(data)
        pin_rs, pin_lbc = pin_rs or rs_bytes, pin_lbc or lbc_bytes
        self.pins = {"archives": [
            {"name": "rapidsnark", "version": "1", "url": "file://" + os.path.join(self.origin, "rs.zip"),
             "bytes": len(pin_rs), "sha256": sha(pin_rs), "format": "zip", "env": {"RAPIDSNARK_LIB_DIR": "rs-1/lib"},
             "build_packages": ["rust-rapidsnark"], "static_libs": ["rapidsnark", "gmp"],
             "files": {k: sha(v) for k, v in (rs_files or RS_FILES).items()}},
            {"name": "circuits", "version": "1", "url": "file://" + os.path.join(self.origin, "lbc.tar.gz"),
             "bytes": len(pin_lbc), "sha256": sha(pin_lbc), "format": "tar.gz", "env": {"LBC_ROOT_DIR": "lbc-1"},
             "build_packages": ["circuits-pol-sys"], "static_libs": ["pol", "gmp"],
             "files": {k: sha(v) for k, v in (lbc_files or LBC_FILES).items()}}],
            "sources": [{"name": "src", "url": "file://" + os.path.join(self.origin, "rs.zip"), "bytes": len(rs_bytes),
                         "sha256": sha(rs_bytes)}]}
        self.pins_path = os.path.join(self.t, "pins.json")
        with open(self.pins_path, "w") as f:
            json.dump(self.pins, f)

    def run_fn(self, *args):
        return subprocess.run([sys.executable, "-I", SCRIPT, *args, "--dir", self.cache, "--pins", self.pins_path],
                              capture_output=True, text=True, timeout=60)

    def exports(self, stdout):
        return dict(l.removeprefix("export ").split("=", 1) for l in stdout.splitlines() if l.startswith("export "))


class Fetch(Base):
    def test_good_fetch_exports_checked_dirs_and_reuses_the_cache(self):
        r = self.run_fn("env")
        self.assertEqual(r.returncode, 0, r.stderr)
        env = self.exports(r.stdout)
        self.assertEqual(sorted(env), ["LBC_ROOT_DIR", "RAPIDSNARK_LIB_DIR"])
        self.assertTrue(os.path.isfile(os.path.join(env["RAPIDSNARK_LIB_DIR"], "librapidsnark.a")))
        self.assertTrue(os.path.isfile(os.path.join(env["LBC_ROOT_DIR"], "pol", "libpol.a")))
        for f in os.listdir(self.origin):          # the second run must not download again
            os.unlink(os.path.join(self.origin, f))
        r2 = self.run_fn("env")
        self.assertEqual(r2.returncode, 0, r2.stderr)
        self.assertEqual(self.exports(r2.stdout), env)
        self.assertNotIn("downloading", r2.stderr)

    def test_corrupted_cached_archive_is_refused(self):
        self.assertEqual(self.run_fn("fetch").returncode, 0)
        arc = fn.archive_path(self.cache, self.pins["archives"][1])
        with open(arc, "r+b") as f:                # one byte changed, same size
            f.seek(100)
            b = f.read(1)
            f.seek(100)
            f.write(bytes([b[0] ^ 1]))
        r = self.run_fn("env")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("corrupted or was replaced; it was not used", r.stderr)
        self.assertEqual(self.exports(r.stdout), {})
        self.assertTrue(os.path.exists(arc))      # left for inspection, not silently replaced
        r = self.run_fn("env", "--refetch")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(fn.sha256_file(arc), self.pins["archives"][1]["sha256"])

    def test_wrong_hash_at_the_origin(self):
        good = make_tgz(LBC_FILES)
        bad = bytearray(good)
        bad[len(bad) // 2] ^= 1                    # a replaced asset of the same size
        self.publish(make_zip(RS_FILES), bytes(bad), pin_lbc=good)
        r = self.run_fn("fetch")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertRegex(r.stderr, r"circuits 1 \(lbc.tar.gz\): sha256 [0-9a-f]{64} from .*, pinned " + sha(good))
        arc = fn.archive_path(self.cache, self.pins["archives"][1])
        self.assertFalse(os.path.exists(arc))
        self.assertFalse(os.path.exists(fn.unpacked_path(self.cache, self.pins["archives"][1])))
        self.assertEqual([x for x in os.listdir(os.path.dirname(arc)) if x.startswith(".partial")], [])

    def test_longer_file_at_the_origin(self):
        good = make_tgz(LBC_FILES)
        self.publish(make_zip(RS_FILES), good + b"trailing", pin_lbc=good)
        r = self.run_fn("fetch")
        self.assertEqual(r.returncode, 1)
        self.assertIn("more than the pinned", r.stderr)

    def test_member_leaving_the_directory(self):
        evil = tarfile.TarInfo("../evil.a")
        evil.size = 4
        data = make_tgz(LBC_FILES, extra=[evil])
        self.publish(make_zip(RS_FILES), data)
        r = self.run_fn("fetch")
        self.assertEqual(r.returncode, 1)
        self.assertIn("refused member '../evil.a'", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.cache, "evil.a")))

    def test_link_member(self):
        link = tarfile.TarInfo("lbc-1/lib/libssl.a")
        link.type, link.linkname = tarfile.SYMTYPE, "/usr/lib/libssl.a"
        self.publish(make_zip(RS_FILES), make_tgz(LBC_FILES, extra=[link]))
        r = self.run_fn("fetch")
        self.assertEqual(r.returncode, 1)
        self.assertIn("refused member 'lbc-1/lib/libssl.a'", r.stderr)

    def test_pinned_file_missing(self):
        self.publish(make_zip(RS_FILES), make_tgz(LBC_FILES), lbc_files={**LBC_FILES, "lbc-1/poq/libpoq.a": b"q"})
        r = self.run_fn("fetch")
        self.assertEqual(r.returncode, 1)
        self.assertIn("lbc-1/poq/libpoq.a missing", r.stderr)
        self.assertFalse(os.path.exists(fn.unpacked_path(self.cache, self.pins["archives"][1])))

    def test_modified_unpacked_file_is_restored_from_the_archive(self):
        self.assertEqual(self.run_fn("fetch").returncode, 0)
        p = os.path.join(fn.unpacked_path(self.cache, self.pins["archives"][0]), "rs-1/lib/libgmp.a")
        with open(p, "wb") as f:
            f.write(b"something else")
        r = self.run_fn("fetch")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("unpacking again", r.stdout)
        with open(p, "rb") as f:
            self.assertEqual(f.read(), RS_FILES["rs-1/lib/libgmp.a"])

    def test_sources(self):
        out = os.path.join(self.t, "src")
        r = subprocess.run([sys.executable, "-I", SCRIPT, "sources", "--out", out, "--pins", self.pins_path],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(os.path.isfile(os.path.join(out, "rs.zip")))
        with open(os.path.join(out, "rs.zip"), "ab") as f:
            f.write(b"x")
        r = subprocess.run([sys.executable, "-I", SCRIPT, "sources", "--out", out, "--pins", self.pins_path],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 1)


class CheckBuild(Base):
    def setUp(self):
        super().setUp()
        self.assertEqual(self.run_fn("fetch").returncode, 0)
        self.rs = os.path.join(fn.unpacked_path(self.cache, self.pins["archives"][0]), "rs-1/lib")
        self.lbc = fn.unpacked_path(self.cache, self.pins["archives"][1])
        self.target = os.path.join(self.t, "target")
        self.out_blst = os.path.join(self.target, "release/build/blst-1/out")
        os.makedirs(self.out_blst)

    def msg(self, pkg, paths, libs, out_dir=None):
        return {"reason": "build-script-executed", "package_id": f"git+https://example.org/x#{pkg}@0.1.0",
                "linked_paths": [f"native={p}" for p in paths], "linked_libs": libs, "cfgs": [], "env": [],
                "out_dir": out_dir or os.path.join(self.target, f"release/build/{pkg}-1/out")}

    def good(self):
        return [self.msg("rust-rapidsnark", [self.rs], ["static=rapidsnark", "static=gmp", "stdc++"]),
                self.msg("circuits-pol-sys", [os.path.join(self.lbc, "lbc-1/pol"), os.path.join(self.lbc, "lbc-1/lib")],
                         ["static=pol", "stdc++", "static=gmp"]),
                self.msg("blst", [self.out_blst], ["static=blst"], self.out_blst),
                self.msg("pcsc-sys", ["/usr/lib/x86_64-linux-gnu"], ["pcsclite"])]

    def check(self, msgs, pins=None, record=None):
        p = os.path.join(self.t, "messages.jsonl")
        with open(p, "w") as f:
            f.write("not json: a compiler line\n")
            for m in msgs:
                f.write(json.dumps(m) + "\n")
            f.write(json.dumps({"reason": "compiler-artifact", "package_id": "x"}) + "\n")
        if pins is not None:
            with open(self.pins_path, "w") as f:
                json.dump(pins, f)
        return self.run_fn("check-build", p, "--target-dir", self.target, *(["--record", record] if record else []))

    def test_good_build(self):
        r = self.check(self.good())
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_rapidsnark_own_download_refused(self):
        m = self.good()
        own = os.path.join(self.target, "release/build/rust-rapidsnark-1/out/rapidsnark/x86_64")
        m[0] = self.msg("rust-rapidsnark", [own], ["static=rapidsnark", "static=gmp"])
        r = self.check(m)
        self.assertEqual(r.returncode, 1)
        self.assertIn("rust-rapidsnark: links from", r.stderr)
        self.assertIn("RAPIDSNARK_LIB_DIR", r.stderr)

    def test_circuits_cache_download_refused(self):
        m = self.good()
        m[1] = self.msg("circuits-pol-sys", [os.path.expanduser("~/.cache/logos/blockchain/x/pol")], ["static=pol"])
        r = self.check(m)
        self.assertEqual(r.returncode, 1)
        self.assertIn("LBC_ROOT_DIR", r.stderr)

    def test_missing_build_script_message(self):
        r = self.check(self.good()[1:])
        self.assertEqual(r.returncode, 1)
        self.assertIn("rust-rapidsnark: its build script is not in this build's messages", r.stderr)

    def test_unexpected_static_lib_from_an_archive(self):
        m = self.good()
        m[0]["linked_libs"].append("static=ssl")
        r = self.check(m)
        self.assertEqual(r.returncode, 1)
        self.assertIn("links static ssl", r.stderr)

    def test_path_outside_the_build(self):
        m = self.good() + [self.msg("some-sys", ["/opt/vendor/lib"], ["static=vendor"])]
        r = self.check(m)
        self.assertEqual(r.returncode, 1)
        self.assertIn("some-sys: links from /opt/vendor/lib", r.stderr)

    def test_new_compiled_native_package_against_the_record(self):
        pins = dict(self.pins, compiled_from_crate_sources={"cli": {"blst": "blst's C"}})
        self.assertEqual(self.check(self.good(), pins, record="cli").returncode, 0)
        zstd = self.out_blst.replace("blst", "zstd")
        os.makedirs(zstd)
        r = self.check(self.good() + [self.msg("zstd-sys", [zstd], ["static=zstd"], zstd)], pins, record="cli")
        self.assertEqual(r.returncode, 1)
        self.assertIn("zstd-sys: links static ['zstd'], and compiled_from_crate_sources['cli'] does not list it", r.stderr)
        r = self.check(self.good()[:2], pins, record="cli")       # blst no longer linked: the record is stale
        self.assertEqual(r.returncode, 1)
        self.assertIn("blst: listed in compiled_from_crate_sources['cli'] but links no static library", r.stderr)
        self.assertEqual(self.check(self.good(), pins).returncode, 0)   # no --record: no ratchet

    def test_no_messages(self):
        r = self.check([])
        self.assertEqual(r.returncode, 1)
        self.assertIn("no build-script-executed messages", r.stderr)

    def release_check(self, msgs, *extra):
        """check-build as scripts/release.sh build runs it: --binary, --copy-binary, --out-json (review-fix-supply M-2)."""
        p = os.path.join(self.t, "messages.jsonl")
        with open(p, "w") as f:
            for m in msgs:
                f.write(json.dumps(m) + "\n")
        self.copy, self.rec = os.path.join(self.t, "copied-ferrata"), os.path.join(self.t, "check-build.json")
        for x in (self.copy, self.rec):
            if os.path.exists(x):
                os.unlink(x)
        return self.run_fn("check-build", p, "--target-dir", self.target, "--binary", "ferrata", "--copy-binary",
                           self.copy, "--out-json", self.rec, *extra)

    def binary(self, where=None):
        exe = where or os.path.join(self.target, "release", "ferrata")
        os.makedirs(os.path.dirname(exe), exist_ok=True)
        with open(exe, "wb") as f:
            f.write(b"\x7fELF the checked binary")
        return [{"reason": "compiler-artifact", "package_id": "path+file:///w/cli#ferrata-cli@0.1.0",
                 "target": {"kind": ["bin"], "name": "ferrata"}, "executable": exe},
                {"reason": "build-finished", "success": True}]

    def test_release_record_and_copy(self):
        r = self.release_check(self.good() + self.binary())
        self.assertEqual(r.returncode, 0, r.stderr)
        with open(self.rec) as f:
            rec = json.load(f)
        with open(self.copy, "rb") as f:
            self.assertEqual(rec["binary"]["sha256"], hashlib.sha256(f.read()).hexdigest())
        self.assertEqual(rec["result"], "pass")
        self.assertEqual([a["sha256"] for a in rec["archives"]], [a["sha256"] for a in self.pins["archives"]])
        # No machine paths (the record goes into the release); the test's pins have file:// URLs, the real ones GitHub's.
        self.assertNotIn(self.t, json.dumps({**rec, "archives": [{**a, "url": ""} for a in rec["archives"]]}))
        out = self.run_fn("record", "--check-build", self.rec)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(out.stdout.split(), " ".join(fn.record_lines(self.pins["archives"])).split())
        self.assertNotEqual(self.run_fn("record").returncode, 0)          # never the pins alone

    def test_release_check_refuses_a_failed_or_foreign_build(self):
        m = self.good() + self.binary()
        r = self.release_check(m[:-1] + [{"reason": "build-finished", "success": False}])
        self.assertEqual(r.returncode, 1)
        self.assertIn("did not report a successful build", r.stderr)
        self.assertFalse(os.path.exists(self.rec) or os.path.exists(self.copy))
        r = self.release_check(self.good() + [{"reason": "build-finished", "success": True}])
        self.assertEqual(r.returncode, 1)
        self.assertIn("name no executable 'ferrata'", r.stderr)
        r = self.release_check(self.good() + self.binary(os.path.join(self.t, "elsewhere", "release", "ferrata")))
        self.assertEqual(r.returncode, 1)
        self.assertIn("outside the target directory", r.stderr)

    def test_unpacked_file_changed_during_the_build(self):
        lib = os.path.join(self.rs, "libgmp.a")
        with open(lib, "ab") as f:
            f.write(b"changed after the fetch")
        r = self.release_check(self.good() + self.binary())
        self.assertEqual(r.returncode, 1)
        self.assertIn("after the build:", r.stderr)


class Names(unittest.TestCase):
    def test_package_name(self):
        self.assertEqual(fn.package_name("git+https://github.com/l/r.git?rev=e9#rust-rapidsnark@0.1.3"), "rust-rapidsnark")
        self.assertEqual(fn.package_name("path+file:///w/cli#ferrata-cli@0.1.0"), "ferrata-cli")
        self.assertEqual(fn.package_name("path+file:///w/blst#0.3.15"), "blst")
        self.assertEqual(fn.package_name("blst 0.3.15 (registry+https://github.com/rust-lang/crates.io-index)"), "blst")
        self.assertEqual(fn.lib_name("static:+whole-archive=foo"), ("static", "foo"))
        self.assertEqual(fn.lib_name("stdc++"), ("default", "stdc++"))


class Pins(unittest.TestCase):
    def test_repository_pins_are_complete(self):
        pins = fn.load_pins(fn.PINS)
        for a in pins["archives"]:
            for k in ("name", "version", "url", "bytes", "sha256", "format", "env", "build_packages", "static_libs",
                      "files", "components", "confirmed"):
                self.assertIn(k, a, f"{a.get('name')}: {k}")
            self.assertRegex(a["sha256"], r"^[0-9a-f]{64}$")
            self.assertTrue(a["url"].startswith("https://"))
            for c in a["components"]:
                self.assertTrue(c.get("licence") and c.get("texts"), c["name"])
        for s in pins["sources"]:
            self.assertRegex(s["sha256"], r"^[0-9a-f]{64}$")


if __name__ == "__main__":
    unittest.main()
