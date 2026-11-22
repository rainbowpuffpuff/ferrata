"""Tests of the release manifest's third-party checks (review-code-ops M2): `python3 -I scripts/test_release_manifest.py`.

A synthetic release directory laid out as `scripts/release.sh package` lays one out, with the repository's own
THIRD-PARTY-NOTICES.md, licence texts and scripts/native-deps.json: `make` and `check` pass; a missing or incomplete
notices file, a missing licence text, a toolchains.txt that does not record the pinned archives, a sources tarball
without the GMP sources, a manifest made before the notices, and a native component the notices do not list, each
fail. Standard library only; files go to a temporary directory (TMPDIR).
"""

import hashlib
import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCRIPT = os.path.join(HERE, "release_manifest.py")
_spec = importlib.util.spec_from_file_location("release_manifest", SCRIPT)
rm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rm)
_spec2 = importlib.util.spec_from_file_location("fetch_native", os.path.join(HERE, "fetch_native.py"))
fn = importlib.util.module_from_spec(_spec2)
_spec2.loader.exec_module(fn)
PINS = fn.load_pins(fn.PINS)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def rd(p, mode="r"):
    with open(p, mode) as f:
        return f.read()


def wr(p, text):
    with open(p, "w") as f:
        f.write(text)


class Release(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.out = self.tmp.name
        self.d = os.path.join(self.out, "ferrata-9.9.9")
        self.layout(self.d)
        self.tar = self.sources_tar(os.path.join(self.out, "ferrata-9.9.9-third-party-sources.tar"))

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, rel, data):
        p = os.path.join(self.d, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(data if isinstance(data, bytes) else data.encode())
        return data if isinstance(data, bytes) else data.encode()

    def layout(self, d):
        ids, jsonl = [], []
        for i, f in enumerate(["ferrata_lez.bin", "ferrata_bridge.bin"]):
            b = self.write(f"programs/{f}", f"lez program {i}")
            ids.append(f"{f} {sha(b)} {'%02x' % (i + 1) * 32}")
        for i, f in enumerate(["checkpoint.bin", "checkpoint_b.bin", "segment.bin", "release.bin"]):
            b = self.write(f"programs/{f}", f"r0bf program {i}")
            jsonl.append(json.dumps({"program": f, "image_id": "0x" + "%02x" % (i + 3) * 32, "sha256": sha(b)}))
        self.write("programs/ids.txt", "\n".join(ids) + "\n")
        self.write("programs/ids.jsonl", "\n".join(jsonl) + "\n")
        art = {"bytecode": {"object": "0x6001600055", "linkReferences": {}}, "deployedBytecode": {"object": "0x600160"},
               "metadata": {"compiler": {"version": "0.8.26"}, "settings": {"evmVersion": "cancun", "optimizer": {"enabled": True}}}}
        for n in ("FerrataVault", "FerrataRegistry"):
            self.write(f"vault/{n}.json", json.dumps(art))
        self.write("cli/ferrata", b"\x7fELF not really")
        self.write("cli/Cargo.lock", "# lock\n")
        self.write("toolchains.txt", "lez 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21\n" + "\n".join(fn.record_lines(PINS)) + "\n")
        for f in ("LICENSE-MIT", "LICENSE-APACHE"):
            shutil.copy(os.path.join(ROOT, f), os.path.join(d, f))
        shutil.copy(os.path.join(ROOT, "releases/third-party/THIRD-PARTY-NOTICES.md"), d)
        shutil.copytree(os.path.join(ROOT, "releases/third-party/licenses"), os.path.join(d, "licenses"))

    def sources_tar(self, path, names=None):
        with tarfile.open(path, "w") as t:
            for n in names or [os.path.basename(s["url"]) for s in PINS["sources"]]:
                ti = tarfile.TarInfo(f"ferrata-9.9.9-third-party-sources/{n}")
                ti.size = 4
                t.addfile(ti, io.BytesIO(b"gmp!"))
        return path

    def make(self, tar=None):
        return subprocess.run([sys.executable, "-I", SCRIPT, "make", self.d, "9.9.9", "abc123", tar or self.tar],
                              capture_output=True, text=True)

    def check(self):
        return subprocess.run([sys.executable, "-I", SCRIPT, "check", self.d], capture_output=True, text=True)

    def test_complete_release(self):
        r = self.make()
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("2 native archives covered by THIRD-PARTY-NOTICES.md", r.stdout)
        c = self.check()
        self.assertEqual(c.returncode, 0, c.stdout)
        self.assertIn("covers the 2 native archives and their 8 components", c.stdout)
        m = json.loads(rd(os.path.join(self.d, "MANIFEST.json")))
        self.assertEqual([a["sha256"] for a in m["native"]], [a["sha256"] for a in PINS["archives"]])
        self.assertEqual(m["third_party"]["sources"]["sha256"], sha(rd(self.tar, "rb")))
        self.assertIn("do not depend on where the tree is", m["image_ids_note"])     # L3: the old note was false
        self.assertIn("1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21", m["targets"]["lez"])

    def test_notices_removed_after_make(self):
        self.assertEqual(self.make().returncode, 0)
        os.unlink(os.path.join(self.d, "THIRD-PARTY-NOTICES.md"))
        c = self.check()
        self.assertEqual(c.returncode, 1)
        self.assertIn("THIRD-PARTY-NOTICES.md missing", c.stdout)

    def test_notices_without_an_archive(self):
        p = os.path.join(self.d, "THIRD-PARTY-NOTICES.md")
        wr(p, rd(p).replace(PINS["archives"][1]["sha256"], "0" * 64))
        r = self.make()
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("does not name logos-blockchain-circuits-v0.5.7-linux-x86_64.tar.gz with its sha256", r.stderr)

    def test_licence_text_missing(self):
        os.unlink(os.path.join(self.d, "licenses", "GPL-2.0.txt"))
        r = self.make()
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("licenses/GPL-2.0.txt missing", r.stderr)

    def test_toolchains_without_the_pinned_archive(self):
        p = os.path.join(self.d, "toolchains.txt")
        wr(p, rd(p).replace(PINS["archives"][0]["sha256"], "f" * 64))
        r = self.make()
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("does not record rapidsnark 0.0.8 with the pinned sha256", r.stderr)

    def test_sources_tar_without_gmp(self):
        tar = self.sources_tar(os.path.join(self.out, "short.tar"), names=["gmp-6.3.0.tar.xz"])
        r = self.make(tar)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("lacks ['gmp-6.2.1.tar.xz']", r.stderr)

    def test_no_sources_tar(self):
        r = subprocess.run([sys.executable, "-I", SCRIPT, "make", self.d, "9.9.9", "abc123"], capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("sources tarball", r.stderr)

    def test_manifest_made_before_the_notices(self):
        self.assertEqual(self.make().returncode, 0)
        p = os.path.join(self.d, "MANIFEST.json")
        m = json.loads(rd(p))
        for k in ("native", "third_party", "licence"):
            m.pop(k)
        wr(p, json.dumps(m))
        c = self.check()
        self.assertEqual(c.returncode, 1)
        self.assertIn("made before the third-party notices", c.stdout)

    def test_new_component_not_in_the_notices(self):
        # A native component added to scripts/native-deps.json without a line in the notices must be caught.
        pins = json.loads(json.dumps(PINS))
        pins["archives"][0]["components"].append({"name": "libsomething", "version": "1.0", "licence": "GPL-2.0-only",
                                                  "in": "lib/libsomething.a", "source": "x", "texts": ["GPL-2.0.txt"]})
        tool = {f"native_{a['name']}": f"{a['version']} sha256 {a['sha256']} {a['url']}" for a in pins["archives"]}
        probs = rm.notices_problems(self.d, {"native": rm.native_section(tool, pins)})
        self.assertTrue(any("libsomething" in p for p in probs), probs)
        self.assertEqual(rm.notices_problems(self.d, {"native": rm.native_section(tool, PINS)}), [])


if __name__ == "__main__":
    unittest.main()
