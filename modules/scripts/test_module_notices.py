"""module_notices.py's render (review L10): every licence a crate is used under has its text in the notices, from the
crate's own files or a standard text in licenses/; render stops otherwise. And the release's notices carry the standard
texts of the two licences the network session's crates brought (CDLA-Permissive-2.0: webpki-roots; Unicode-3.0:
unicode-normalization).   python3 -I modules/scripts/test_module_notices.py   (scratch under $TMPDIR, else beside it)"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import module_notices  # noqa: E402

NOTICES = os.path.join(HERE, "..", "..", "deploy", "testnet", "web", "basecamp", "dl", "notices")
MIT = "Permission is hereby granted, free of charge, to any person obtaining a copy of this software"
CDLA = "# Community Data License Agreement - Permissive - Version 2.0\n\nThis is the Community Data License Agreement"
UNICODE = "UNICODE LICENSE V3\n\nCOPYRIGHT AND PERMISSION NOTICE"


class Render(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="notices-test-", dir=os.environ.get("TMPDIR") or HERE)

    def tearDown(self):
        shutil.rmtree(self.dir)

    def render(self, packages, standard):
        """packages: {"name version": (licence, {file: text})}; standard: the files of licenses/. (exit, crates.md)"""
        run = tempfile.mkdtemp(dir=self.dir)
        out, dest = os.path.join(run, "out"), os.path.join(run, "dest")
        os.makedirs(os.path.join(out, "files"))
        os.makedirs(os.path.join(dest, "licenses"))
        for f in standard:
            open(os.path.join(dest, "licenses", f), "w").write(f + "\n")
        rec = {"module": "m", "targets": {"linux-amd64": sorted(packages)}, "packages": {}}
        for key, (lic, files) in packages.items():
            n, v = key.split(" ")
            os.makedirs(os.path.join(out, "files", f"{n}-{v}"), exist_ok=True)
            for f, text in files.items():
                open(os.path.join(out, "files", f"{n}-{v}", f), "w").write(text)
            rec["packages"][key] = {"license": lic, "source": "registry+https://github.com/rust-lang/crates.io-index", "authors": ["A"],
                                    "files": sorted(files), "local": False, "kind": "registry", "where": "crates.io"}
        json.dump(rec, open(os.path.join(out, "m.json"), "w"))
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "module_notices.py"), "render", out, dest], capture_output=True, text=True)
        md = os.path.join(dest, "crates.md")
        return r, (open(md).read() if os.path.exists(md) else "")

    def test_a_licence_without_its_text_stops_the_notices(self):
        r, _ = self.render({"webpki-roots 1.0.12": ("CDLA-Permissive-2.0", {})}, ["MIT.txt"])
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("no text for these licences", r.stderr)
        self.assertIn("webpki-roots 1.0.12 (CDLA-Permissive-2.0): CDLA-Permissive-2.0", r.stderr)

    def test_the_crates_own_file_carries_it(self):
        r, md = self.render({"webpki-roots 1.0.12": ("CDLA-Permissive-2.0", {"LICENSE": CDLA})}, ["MIT.txt"])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Community Data License Agreement - Permissive", md)

    def test_every_part_of_an_and_needs_its_text(self):
        pkgs = {"unicode-normalization 0.1.26": ("(MIT OR Apache-2.0) AND Unicode-3.0", {"LICENSE-MIT": MIT})}
        r, _ = self.render(pkgs, ["MIT.txt"])
        self.assertIn("unicode-normalization 0.1.26 ((MIT OR Apache-2.0) AND Unicode-3.0): Unicode-3.0", r.stderr)
        r, md = self.render(pkgs, ["MIT.txt", "Unicode-3.0.txt"])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("- `licenses/Unicode-3.0.txt`: unicode-normalization 0.1.26", md)

    def test_what_a_crate_is_used_under(self):
        self.assertEqual(module_notices.spdx_used("(MIT OR Apache-2.0) AND Unicode-3.0"), ["MIT", "Unicode-3.0"])
        self.assertEqual(module_notices.spdx_used("Apache-2.0 AND ISC"), ["Apache-2.0", "ISC"])
        self.assertEqual(module_notices.spdx_used("MIT/Apache-2.0"), ["MIT"])
        self.assertEqual(module_notices.spdx_used("Apache-2.0 WITH LLVM-exception OR MIT"), ["Apache-2.0"])
        self.assertEqual(module_notices.spdx_used("CC0-1.0"), ["CC0-1.0"])


class Published(unittest.TestCase):
    def test_the_release_notices_carry_cdla_and_unicode_texts(self):
        for f, words in [("CDLA-Permissive-2.0.txt", "community data license agreement - permissive - version 2.0"),
                         ("Unicode-3.0.txt", "unicode license v3")]:
            path = os.path.join(NOTICES, "licenses", f)
            self.assertTrue(os.path.isfile(path), path)
            self.assertIn(words, " ".join(open(path, encoding="utf-8").read().lower().split()))


if __name__ == "__main__":
    unittest.main()
