#!/usr/bin/env python3
"""The live test network's record, checked again from its files: `python3 -I modules/scripts/test_live_testnet.py`.

For every `modules/results/live-testnet-*/` record:
- `live_testnet.py verify`: every figure and check of its `summary.json` (the components' CPU and memory from the
  process samples, the tools' GNU time, each daemon's traffic from its relay, the module calls, the journal, the
  /health probes, the instance at the end) computed again from the record's own files, equal; none of its checks
  failed;
- `lez/updater/results/live-sepolia/check.py` on its session: every journal line has its manifest row (block, result,
  execution gas, fee = the payer's debit), the jobs have the shape Open, Miller, [Adopt 1/2, Adopt 2/2,] Finish in
  increasing blocks, and every applied job's update is in the module's log;
- `scripts/scrub_records.py --check-only`: no machine path in it.
Offline: nothing is read from any network (check.py's `--beacon` re-check is the record README's, by hand)."""
import glob
import json
import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
RECORDS = sorted(glob.glob(os.path.join(REPO, "modules", "results", "live-testnet-*")))


def run(*args):
    return subprocess.run([sys.executable, "-I", *args], capture_output=True, text=True, cwd=REPO)


class TestLiveTestnetRecords(unittest.TestCase):
    def test_there_is_a_record(self):
        self.assertTrue(RECORDS, "no modules/results/live-testnet-* record")

    def test_summary_recomputes_and_every_check_passes(self):
        for rec in RECORDS:
            with self.subTest(record=os.path.basename(rec)):
                r = run(os.path.join(HERE, "live_testnet.py"), "verify", rec)
                self.assertEqual(r.returncode, 0, r.stdout[-3000:] + r.stderr[-3000:])
                s = json.load(open(os.path.join(rec, "summary.json")))
                failed = [c["what"] for c in s["checks"] if not c["ok"]]
                self.assertEqual(failed, [], "checks that failed in the record")

    def test_journal_against_the_manifest(self):
        """check.py's checks of the journal against the backend's manifest and of the jobs' shapes all pass. Its check
        that every applied job's update is in the log (finalized slot, root, sources) needs the daemon's own job lines,
        which the module prints only when started with `quiet: false`: the session's first start used the module's
        default (quiet), its restart did not; the roots of every job are checked instead by attest-monitor's two
        providers (summary.json's check, session/monitor/seen.json)."""
        wanted = ["journal lines have their manifest row", "included transactions: charged fee = the payer's balance fell by",
                  "transactions alone in their block carry the block's own gas totals", "transactions settle in increasing blocks",
                  "jobs applied, each Open, Miller, [Adopt 1/2, Adopt 2/2,] Finish in increasing blocks"]
        for rec in RECORDS:
            with self.subTest(record=os.path.basename(rec)):
                r = run(os.path.join(REPO, "lez", "updater", "results", "live-sepolia", "check.py"), os.path.join(rec, "session"))
                lines = r.stdout.splitlines()
                for w in wanted:
                    self.assertTrue(any(l.startswith("PASS ") and w in l for l in lines), f"check.py: no PASS line for {w!r}\n{r.stdout[-3000:]}")
                fails = [l for l in lines if l.startswith("FAIL ")]
                self.assertTrue(all("every applied job's update is in updater.log" in l for l in fails), "\n".join(fails))

    def test_no_machine_paths(self):
        for rec in RECORDS:
            with self.subTest(record=os.path.basename(rec)):
                r = run(os.path.join(REPO, "scripts", "scrub_records.py"), "--check-only", rec)
                self.assertEqual(r.returncode, 0, r.stdout[-3000:] + r.stderr[-3000:])


if __name__ == "__main__":
    unittest.main()
