#!/usr/bin/env python3
"""Tests of checkpoint/attestor/resources.py (RFP-022 P.6: the attestor's resources from a run's records).

    python3 -I checkpoint/attestor/test_resources.py
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.join(HERE, "resources.py")


def run(*args):
    return subprocess.run([sys.executable, "-I", TOOL, *args], capture_output=True, text=True)


class Resources(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="resources-", dir=os.environ.get("TEST_TMPDIR") or HERE)
        self.addCleanup(shutil.rmtree, self.d)

    def write(self, attestor, lines):
        p = os.path.join(self.d, "zone", attestor)
        os.makedirs(p, exist_ok=True)
        with open(os.path.join(p, "resources.tsv"), "w") as f:
            f.write("".join(line + "\n" for line in lines))

    def test_figures_per_attestor_and_for_all(self):
        # wall, user, sys, peak RSS KiB, bytes read, exit
        self.write("attestor-0", ["2026-11-26T10:00:00Z\t1.00\t0.20\t0.05\t20480\t1000\t0",
                                  "2026-11-26T10:01:00Z\t3.00\t0.40\t0.10\t30720\t3000\t0",
                                  "2026-11-26T10:02:00Z\t2.00\t0.30\t0.05\t25600\t2000\t1"])
        self.write("attestor-1", ["2026-11-26T10:00:00Z\t4.00\t1.00\t0.00\t40960\t6000\t0"])
        r = run(self.d, "--json")
        self.assertEqual(r.returncode, 0, r.stderr)
        s = json.loads(r.stdout)
        a0 = s["attestor-0"]
        self.assertEqual((a0["rounds"], a0["signed"]), (3, 2))
        self.assertEqual((a0["wall_s_median"], a0["wall_s_max"]), (2.0, 3.0))
        self.assertEqual((a0["cpu_s_median"], a0["cpu_s_max"]), (0.35, 0.5))
        self.assertEqual((a0["rss_mib_median"], a0["rss_mib_max"]), (25.0, 30.0))
        self.assertEqual((a0["bytes_median"], a0["bytes_max"]), (2000, 3000))
        # mean CPU 0.3667 s a round, one round a minute: 0.00611 of a core; mean 2000 B a round: 2.88 MB a day
        self.assertEqual(a0["core_share_at_cadence"], 0.00611)
        self.assertEqual(a0["mib_per_day_at_cadence"], round(2000 * 1440 / 2**20, 1))
        every = s["all attestors"]
        self.assertEqual((every["rounds"], every["signed"]), (4, 3))
        self.assertEqual(every["bytes_max"], 6000)
        # another cadence scales the derived figures only
        r2 = json.loads(run(self.d, "--json", "--round-s", "120").stdout)
        self.assertEqual(r2["attestor-0"]["core_share_at_cadence"], 0.00306)
        self.assertEqual(r2["attestor-0"]["wall_s_median"], 2.0)
        # the table names every attestor and the total
        t = run(self.d)
        self.assertEqual(t.returncode, 0)
        self.assertIn("| attestor-0 | 3 (2) |", t.stdout)
        self.assertIn("| all attestors | 4 (3) |", t.stdout)

    def test_no_records_or_a_malformed_row_fails(self):
        self.assertEqual(run(self.d).returncode, 1)
        self.write("attestor-0", ["2026-11-26T10:00:00Z\t1.00\t0.20\t20480\t1000\t0"])  # a field missing
        r = run(self.d)
        self.assertEqual(r.returncode, 1)
        self.assertIn("6 fields, want 7", r.stderr)
        self.assertEqual(run(self.d, "--round-s", "0").returncode, 2)


if __name__ == "__main__":
    unittest.main()
