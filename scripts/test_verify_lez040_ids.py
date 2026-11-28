#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_lez040_ids (offline, standard library only).

    python3 -I scripts/test_verify_lez040_ids.py

`lez040-image-ids` runs on the real tree, then on a copy under .scratch/ holding only its inputs (a v0.4.0 image-id
record, the build's input files, LEZ's tags record, docs/image-ids.md), each copy with one thing altered: a guest's
source, a file added to or removed from the build's inputs, the selector's lockfile patch, the record's LEZ commit,
r0vm's cross-check, a v0.4.0 table in docs/image-ids.md that disagrees. Each must FAIL with a line naming what differs;
a record naming a commit this clone lacks makes the check SKIP, never PASS. The copies are read with this clone's git
objects (the record's commit is there).
"""
from __future__ import annotations

import json
import shutil
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_lez040_ids as M  # noqa: E402 - registers the module's checks
from verifylib.core import REGISTRY, Ctx, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-lez040-ids-tests"
CID = "lez040-image-ids"


def record() -> Path:
    recs = sorted((ROOT / M.RECORDS).glob("*/build.json"))
    if not recs:
        raise unittest.SkipTest(f"no record under {M.RECORDS}")
    return recs[0].parent


def run(root: Path):
    return run_check(REGISTRY[CID], Ctx(root=root, git_dir=ROOT, cache=root / ".scratch" / "cache"))


class ImageIdRecord(unittest.TestCase):
    def setUp(self):
        self.rec = record()
        self.rel = self.rec.relative_to(ROOT)
        self.copy = SCRATCH / self._testMethodName
        shutil.rmtree(self.copy, ignore_errors=True)
        crates = json.loads((self.rec / "build.json").read_text())["crates"]
        for rel in M._inputs(ROOT, crates) + [M.RECORDS, M.TAGS, M.DOC, M.OTHER_BUILD]:
            src = ROOT / rel
            if src.is_dir():
                shutil.copytree(src, self.copy / rel, ignore=shutil.ignore_patterns("target", ".scratch"))
            elif src.is_file():
                (self.copy / rel).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, self.copy / rel)

    def tearDown(self):
        shutil.rmtree(self.copy, ignore_errors=True)

    def assertStatus(self, status, contains=None):
        r = run(self.copy)
        self.assertEqual(r.status, status, "\n".join(r.lines))
        if contains:
            self.assertTrue(any(contains in ln for ln in r.lines), f"no line names {contains!r}:\n" + "\n".join(r.lines))
        return r

    def edit(self, rel: str, old: str, new: str):
        p = self.copy / rel
        s = p.read_text()
        self.assertIn(old, s, f"{rel}: the test's needle is gone")
        p.write_text(s.replace(old, new, 1))

    def test_passes_on_the_tree(self):
        r = run(ROOT)
        self.assertEqual(r.status, "PASS", "\n".join(r.lines))

    def test_the_copy_passes(self):
        self.assertStatus("PASS")

    def test_a_guest_source_changed(self):
        p = self.copy / "lez/guests/rebate/src/main.rs"
        p.write_bytes(p.read_bytes() + b"\n// one more line\n")
        self.assertStatus("FAIL", "lez/guests/rebate/src/main.rs differs")

    def test_a_shared_source_changed(self):
        p = self.copy / "crates/ferrata-core/Cargo.toml"
        p.write_bytes(p.read_bytes().replace(b"\n", b"\n\n", 1))
        self.assertStatus("FAIL", "crates/ferrata-core/Cargo.toml differs")

    def test_a_reached_crates_tests_are_no_build_input(self):
        """A dependency's integration tests are not compiled into a guest: a change there leaves the ids standing, a
        change in the same crate's src/ does not."""
        p = self.copy / "crates/ferrata-spectests/tests/helios_step_oracle.rs"
        p.write_bytes(p.read_bytes() + b"\n// one more line\n")
        self.assertStatus("PASS")
        q = self.copy / "crates/ferrata-spectests/src/lib.rs"
        q.write_bytes(q.read_bytes() + b"\n// one more line\n")
        self.assertStatus("FAIL", "crates/ferrata-spectests/src/lib.rs differs")

    def test_a_file_added(self):
        (self.copy / "lez/rebate-wire/src/extra.rs").write_text("pub const X: u8 = 1;\n")
        self.assertStatus("FAIL", "lez/rebate-wire/src/extra.rs is in this tree and was not")

    def test_a_file_removed(self):
        (self.copy / "lez/methods-rebate/build.rs").unlink()
        self.assertStatus("FAIL", "lez/methods-rebate/build.rs is in the build's source")

    def test_the_lockfile_patch_changed(self):
        p = self.copy / "lez/lockfiles/v0.4.0.patch"
        p.write_bytes(p.read_bytes() + b"\n")
        self.assertStatus("FAIL", "lez/lockfiles/v0.4.0.patch differs")

    def test_the_lez_commit_changed(self):
        self.edit(f"{self.rel}/build.json", "a255b42c0fc80d1cb219454a3a1f5ec3b5d92115",
                  "bafd07be0701398f9bc435f73ab05e9a7936c301")
        self.assertStatus("FAIL", "not v0.4.0")

    def test_r0vm_disagrees(self):
        p = self.copy / self.rel / "ids.tsv"
        lines = p.read_text().splitlines()
        cells = lines[1].split("\t")
        cells[6] = "0x" + "11" * 32
        lines[1] = "\t".join(cells)
        p.write_text("\n".join(lines) + "\n")
        self.assertStatus("FAIL", "r0vm computes")

    def test_a_malformed_id(self):
        p = self.copy / self.rel / "ids.tsv"
        lines = p.read_text().splitlines()
        cells = lines[1].split("\t")
        cells[2] = cells[2][:-2]
        lines[1] = "\t".join(cells)
        p.write_text("\n".join(lines) + "\n")
        self.assertStatus("FAIL", "malformed ids.tsv row")

    def _doc_row(self, name: str) -> str:
        line = next(ln for ln in (self.copy / M.DOC).read_text().splitlines() if f"`{name}_040`" in ln)
        return line

    def test_the_doc_table_disagrees(self):
        row = (self.rec / "ids.tsv").read_text().splitlines()[1].split("\t")
        line = self._doc_row(row[1])
        self.edit(M.DOC, line, line.replace(row[2], "0x" + "00" * 31 + "01"))
        self.assertStatus("FAIL", f"{M.DOC}: {row[1]}: its image id is 0x")

    def test_a_doc_row_no_record_names(self):
        row = (self.rec / "ids.tsv").read_text().splitlines()[1].split("\t")
        line = self._doc_row(row[1])
        self.edit(M.DOC, line, line.replace(f"`{row[1]}_040`", f"`{row[1]}_other_040`"))
        self.assertStatus("FAIL", "is in no build record")

    def test_the_doc_table_agrees(self):
        n = len((self.rec / "ids.tsv").read_text().splitlines()) - 1
        self.assertStatus("PASS", f"v0.4.0 section: {n} row(s)")

    def test_no_doc_section_passes_without_it(self):
        p = self.copy / M.DOC
        s = p.read_text()
        head, rest = s.split(M.DOC_SECTION, 1)
        p.write_text(head + "## " + rest.split("\n## ", 1)[1])
        r = self.assertStatus("PASS")
        self.assertFalse(any("v0.4.0 section" in ln for ln in r.lines), r.lines)

    def _tsv_cell(self, col: int, value: str):
        p = self.copy / self.rel / "ids.tsv"
        lines = p.read_text().splitlines()
        cells = lines[1].split("\t")
        cells[col] = value
        lines[1] = "\t".join(cells)
        p.write_text("\n".join(lines) + "\n")

    def test_a_size_that_is_not_a_number(self):
        self._tsv_cell(4, "not-a-size")
        self.assertStatus("FAIL", "malformed ids.tsv row")

    def test_a_user_elf_hash_that_is_not_one(self):
        self._tsv_cell(5, "not-a-sha256")
        self.assertStatus("FAIL", "malformed ids.tsv row")

    def test_a_guest_recorded_twice(self):
        p = self.copy / self.rel / "ids.tsv"
        lines = p.read_text().splitlines()
        p.write_text("\n".join(lines + [lines[1]]) + "\n")
        self.assertStatus("FAIL", "is recorded twice")

    def test_the_header_changed(self):
        p = self.copy / self.rel / "ids.tsv"
        lines = p.read_text().splitlines()
        lines[0] = lines[0].replace("sha256_bin", "sha_bin")
        p.write_text("\n".join(lines) + "\n")
        self.assertStatus("FAIL", "header")

    def test_each_doc_cell_held_to_the_record(self):
        row = (self.rec / "ids.tsv").read_text().splitlines()[1].split("\t")
        for col, what in ((3, "binary sha256"), (4, "bytes"), (5, "user ELF sha256")):
            with self.subTest(what=what):
                original = (self.copy / M.DOC).read_text()
                line = self._doc_row(row[1])
                if col == 4:
                    changed = line.replace(f"| {int(row[4]):,} |", f"| {int(row[4]) + 1:,} |")
                else:
                    changed = line.replace(row[col], ("1" if row[col][0] != "1" else "2") + row[col][1:])
                self.assertNotEqual(changed, line)
                self.edit(M.DOC, line, changed)
                self.assertStatus("FAIL", f"its {what} is")
                (self.copy / M.DOC).write_text(original)

    def test_the_other_machines_build_differs(self):
        rel = M.OTHER_BUILD
        if not (ROOT / rel).is_file():
            self.skipTest(f"no {rel}")
        p = self.copy / rel
        lines = p.read_text().splitlines()
        cells = lines[1].split("\t")
        cells[2] = "00" + cells[2][2:]
        lines[1] = "\t".join(cells)
        p.write_text("\n".join(lines) + "\n")
        self.assertStatus("FAIL", "differs from the build record")

    def test_a_commit_this_clone_lacks_skips(self):
        b = self.copy / self.rel / "build.json"
        d = json.loads(b.read_text())
        d["ferrata_commit"] = "f" * 40
        b.write_text(json.dumps(d))
        self.assertStatus("SKIP", "lacks commit")

    def test_no_record_fails(self):
        shutil.rmtree(self.copy / M.RECORDS)
        self.assertStatus("FAIL", "no record")


class CompareRebuild(unittest.TestCase):
    """scripts/reproduce/compare_lez040_ids.py, which lez040-image-ids-rebuild's script ends with."""

    HEAD = "workspace\tname\timage_id\tsha256_bin\tbytes_bin\tsha256_user_elf\tr0vm_id\n"

    def compare(self, recorded: list, rebuilt: list):
        import subprocess
        d = SCRATCH / "compare"
        d.mkdir(parents=True, exist_ok=True)
        (d / "rec.tsv").write_text(self.HEAD + "".join("\t".join(r) + "\n" for r in recorded))
        (d / "got.tsv").write_text(self.HEAD + "".join("\t".join(r) + "\n" for r in rebuilt))
        p = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/reproduce/compare_lez040_ids.py"),
                            str(d / "rec.tsv"), str(d / "got.tsv"), "t"], capture_output=True, text=True)
        shutil.rmtree(d, ignore_errors=True)
        return p.returncode, p.stdout

    def row(self, name="ferrata_rebate", img="0x" + "ab" * 32, sha="cd" * 32):
        return ["lez", name, img, sha, "1", "ef" * 32, "= id"]

    def test_equal(self):
        rc, out = self.compare([self.row()], [self.row()])
        self.assertEqual(rc, 0, out)
        self.assertIn("same", out)

    def test_another_id(self):
        rc, out = self.compare([self.row()], [self.row(img="0x" + "ac" * 32)])
        self.assertEqual(rc, 1, out)
        self.assertIn("DIFFERENT", out)

    def test_another_binary(self):
        rc, out = self.compare([self.row()], [self.row(sha="ce" * 32)])
        self.assertEqual(rc, 1, out)
        self.assertIn("binary sha256 different", out)

    def test_not_rebuilt(self):
        rc, out = self.compare([self.row(), self.row(name="ferrata_lez_class")], [self.row()])
        self.assertEqual(rc, 1, out)
        self.assertIn("ferrata_lez_class: recorded", out)
        self.assertIn("not rebuilt", out)

    def test_empty_record(self):
        rc, out = self.compare([], [self.row()])
        self.assertEqual(rc, 1, out)


if __name__ == "__main__":
    unittest.main()
