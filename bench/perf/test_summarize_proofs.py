#!/usr/bin/env python3
"""The tests of bench/perf/summarize_proofs.py (RFP-021 P.2's proof records): `python3 -I bench/perf/test_summarize_proofs.py`.

- The committed records (`bench/perf/results/p2-c`, `p2-b`, `p2-a`) pass every check, and the summary is the first
  part of `lez/results/perf-proofs.md`, byte for byte (the hand-written notes follow it).
- A log filed under the wrong transition is refused: `prove-040.log` (`VerifyAll`, 9,049,362 cycles) copied over
  `prove-041.log` (`ApplyVerified`, 58,184), whose image id is the same program's.
- A proof on a sibling image is refused: transition 44 (`Read`) moved, in index.tsv and in its log alike, to the
  software-Keccak build of the attestation program (`ferrata_lez_keccak_soft`, a row of docs/image-ids.md's first
  table, but not the guest `Read` runs in); and a stage's SHA256SUMS that does not list a guest it proved.
- A private run of another program is refused.

Each refusal exits 1 and prints nothing. The records are copied to a temporary directory and changed there.
Standard library only."""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "bench/perf/summarize_proofs.py"
DIRS = ["bench/perf/results/p2-c", "bench/perf/results/p2-b", "bench/perf/results/p2-a"]
PROGRAM = "816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df"
CONSUMER = "8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96"
KECCAK_SOFT = "efe9c58caa0eacd4c837815d7dbb3c003011b478f9cc5b85fca083b6f0a5cb3e"


def summarize(dirs, cwd=ROOT):
    return subprocess.run([sys.executable, "-I", str(SCRIPT), *map(str, dirs)], cwd=cwd, capture_output=True,
                          text=True, timeout=120)


class Records(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dirs = [Path(self.tmp.name) / Path(d).name for d in DIRS]
        for d, copy in zip(DIRS, self.dirs):
            shutil.copytree(ROOT / d, copy)

    def tearDown(self):
        self.tmp.cleanup()

    def refused(self, *names):
        r = summarize(self.dirs)
        self.assertEqual(r.returncode, 1, r.stderr)
        self.assertEqual(r.stdout, "", "a refused record leaves no summary")
        for name in names:
            self.assertIn(name, r.stderr)
        return r.stderr

    def test_the_committed_records_pass_and_are_the_results_file(self):
        r = summarize(DIRS)
        self.assertEqual(r.returncode, 0, r.stderr)
        doc = (ROOT / "lez/results/perf-proofs.md").read_text()
        self.assertTrue(doc.startswith(r.stdout), "lez/results/perf-proofs.md starts with the summary, byte for byte")
        self.assertTrue(doc[len(r.stdout):].startswith("\n## Notes on the records"), "then the hand-written notes")
        self.assertIn(f"| `0x{PROGRAM[:8]}…{PROGRAM[-4:]}` | `ferrata_lez` | equal |", r.stdout)
        self.assertIn(f"| `0x{CONSUMER[:8]}…{CONSUMER[-4:]}` | `ferrata_consumer` | equal |", r.stdout)
        self.assertNotIn("not proven", r.stdout)

    def test_a_log_filed_under_another_transition_is_refused(self):
        c = self.dirs[0]
        shutil.copy(c / "prove-040.log", c / "prove-041.log")
        err = self.refused("prove-041.log")
        self.assertIn("RESULT of transition 40, 9049362 user cycles, handler VerifyAll", err)
        self.assertIn("index.tsv row 41: 58184, ApplyVerified", err)

    def test_a_proof_on_a_sibling_image_is_refused(self):
        c = self.dirs[0]
        index = (c / "index.tsv").read_text().splitlines()
        row = index[44].split("\t")
        self.assertEqual((row[0], row[5], row[6]), ("44", "Read", PROGRAM))
        row[6] = KECCAK_SOFT
        index[44] = "\t".join(row)
        (c / "index.tsv").write_text("\n".join(index) + "\n")
        log = c / "prove-044.log"
        log.write_text(log.read_text().replace(f"image {PROGRAM} (Read)", f"image {KECCAK_SOFT} (Read)"))
        err = self.refused("prove-044.log")
        self.assertIn("proven on ferrata_lez_keccak_soft, not ferrata_lez", err)

    def test_a_guest_missing_from_the_stage_sums_is_refused(self):
        c = self.dirs[0]
        sums = (c / "SHA256SUMS").read_text().splitlines()
        kept = [l for l in sums if f"elf-{CONSUMER}.bin" not in l]
        self.assertEqual(len(kept), len(sums) - 1)
        (c / "SHA256SUMS").write_text("\n".join(kept) + "\n")
        self.refused(CONSUMER)

    def test_a_private_run_of_another_program_is_refused(self):
        a = self.dirs[2]
        log = a / "private-1.log"
        text = log.read_text()
        self.assertIn(f"program image {PROGRAM} ", text)
        log.write_text(text.replace(f"program image {PROGRAM} ", f"program image {CONSUMER} ", 1))
        self.refused("private-1.log", "is not ferrata_lez's")


if __name__ == "__main__":
    unittest.main(verbosity=2)
