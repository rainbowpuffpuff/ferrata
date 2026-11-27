#!/usr/bin/env python3
"""scripts/verify.py under test: its registry is honest, its command line behaves, and every check family fails
when its evidence is altered. Python 3.11+ standard library only, offline (the online run-record check runs against
a stand-in RPC on 127.0.0.1 that serves recorded blocks of our test network).

    python3 -I scripts/test_verify.py [-v] [-k PATTERN]

Each tamper test runs the real check (verifylib.core.run_check) on a copy of the repository in which one piece of
evidence is altered: the copy links every untouched path to this checkout and copies only what it changes, under
$RUNNER_TEMP or .scratch/ (never /tmp unless TMPDIR says so).
"""
from __future__ import annotations

import base64
import contextlib
import http.server
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import catalog, lez, runrecord  # noqa: E402
from verifylib.core import REGISTRY, Ctx, run_check  # noqa: E402
from verifylib.rows import status_rows  # noqa: E402

ROWS = catalog.load(ROOT)
SCRATCH = Path(os.environ.get("RUNNER_TEMP") or ROOT / ".scratch")


def overlay(edits: dict, copy=("scripts",)) -> Path:
    """A copy of the repository: each top-level entry a symlink to this checkout, except the directories on the way
    to an edited file (real directories of symlinks) and `copy` (copied whole: scripts that find the repository from
    their own path then find the copy). `edits` maps a path to a function of its bytes, or None to delete it."""
    SCRATCH.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="test-verify-", dir=SCRATCH))

    def materialise(rel: Path) -> Path:
        """Make tmp/rel a real directory whose entries link to ROOT/rel's (recursively along `rel`)."""
        cur_t, cur_r = tmp, ROOT
        for part in rel.parts:
            cur_t, cur_r = cur_t / part, cur_r / part
            if cur_t.is_symlink():
                cur_t.unlink()
            if not cur_t.exists():
                cur_t.mkdir()
                for e in cur_r.iterdir():
                    (cur_t / e.name).symlink_to(e)
        return cur_t

    for e in ROOT.iterdir():
        if e.name in (".git", ".scratch"):
            continue
        (tmp / e.name).symlink_to(e)
    (tmp / ".scratch").mkdir()
    for c in copy:
        materialise(Path(c).parent) if Path(c).parent != Path(".") else None
        t = tmp / c
        if t.is_symlink():
            t.unlink()
        shutil.copytree(ROOT / c, t, symlinks=False, ignore=shutil.ignore_patterns("__pycache__"))
    for rel, fn in edits.items():
        rel = Path(rel)
        d = materialise(rel.parent) if rel.parent != Path(".") else tmp
        f = d / rel.name
        data = (ROOT / rel).read_bytes()
        if f.is_symlink():
            f.unlink()
        if fn is not None:
            f.write_bytes(fn(data))
    return tmp


def sub(old: str, new: str, count: int = 1):
    """An edit replacing the first `count` occurrences of `old`, which must be there."""
    def fn(b: bytes) -> bytes:
        s = b.decode()
        if old not in s:
            raise AssertionError(f"{old!r} not in the file to alter")
        return s.replace(old, new, count).encode()
    return fn


def run(cid: str, root: Path, **kw):
    ctx = Ctx(root=root, cache=root / ".scratch" / "cache", git_dir=ROOT if root != ROOT else None, **kw)
    return run_check(REGISTRY[cid], ctx)


class TamperCase(unittest.TestCase):
    def assertPasses(self, cid, root=ROOT, **kw):
        r = run(cid, root, **kw)
        self.assertEqual(r.status, "PASS", f"{cid} on {root}: {r.status}\n" + "\n".join(r.lines))
        return r

    def assertFails(self, cid, root, contains=None, **kw):
        r = run(cid, root, **kw)
        self.assertEqual(r.status, "FAIL", f"{cid} should fail on the altered copy: {r.status}\n" + "\n".join(r.lines))
        if contains:
            self.assertTrue(any(contains in ln for ln in r.lines), f"{cid}: no line names {contains!r}:\n" +
                            "\n".join(r.lines))
        return r

    def tearDown(self):
        for p in SCRATCH.glob("test-verify-*"):
            shutil.rmtree(p, ignore_errors=True)


# ---- the registry -------------------------------------------------------------------------------------------------

class Registry(unittest.TestCase):
    def test_documents_state_the_counts(self):
        """The counts docs/cli-coverage.md and docs/VERIFY.md state are the registry's (review round 1, L4)."""
        words = {w: i for i, w in enumerate("zero one two three four five six seven eight nine ten eleven twelve"
                                            .split())}
        quick = [c for c in REGISTRY.values() if c.mode == "offline" and c.tier == "quick"]
        online = [c for c in REGISTRY.values() if c.mode == "online"]
        cov = (ROOT / "docs/cli-coverage.md").read_text()
        ver = (ROOT / "docs/VERIFY.md").read_text()
        found = 0
        for pattern, text, want in (
                (r"the (\d+) quick offline checks", cov, len(quick)),
                (r"The (\d+) online checks", cov, len(online)),
                (r"(\d+) quick offline checks", ver, len(quick)),
                (r"(\d+) online checks", ver, len(online)),
                (r"Node 20 or later for (\w+) of the checks", ver, sum("node" in c.needs for c in quick))):
            for m in re.finditer(pattern, text):
                n = int(m.group(1)) if m.group(1).isdigit() else words[m.group(1)]
                self.assertEqual(n, want, f"the documents say {m.group(0)!r}; the registry has {want}")
                found += 1
        self.assertGreaterEqual(found, 4, "the documents no longer state the counts this test reads")

    def test_rows_exist(self):
        keys = {r.key for r in ROWS}
        for c in REGISTRY.values():
            for row in c.rows:
                self.assertIn(row, keys, f"{c.id} names {row}, which docs/requirements-status.md does not have")

    def test_rows_cite_the_evidence(self):
        """A check backs a row only where the row's own Evidence cell cites one of its EVIDENCE rows or files."""
        by_key = {r.key: r for r in ROWS}
        bad = []
        for c in REGISTRY.values():
            if c.family == "test":
                continue  # the row-to-test map is its own source (scripts/ci/row-tests.toml)
            for row in c.rows:
                r = by_key[row]
                if not (set(r.evidence) & set(c.evidence) or any(i in r.evidence_text for i in c.inputs)):
                    bad.append(f"{c.id} -> {row} (check evidence {c.evidence}, row cites {r.evidence})")
        self.assertEqual(bad, [], "checks naming rows that do not cite them:\n" + "\n".join(bad))

    def test_every_check_is_described(self):
        for c in REGISTRY.values():
            self.assertTrue(c.title and c.what and c.against, f"{c.id}: title, what and against")
            self.assertIn(c.mode, ("offline", "online"))

    def test_online_checks_send_nothing(self):
        """The LEZ client refuses anything but reads; the Ethereum reads of checks_online allow four methods."""
        rpc = lez.Rpc("http://127.0.0.1:9/rpc")
        with self.assertRaises(lez.RpcError):
            rpc.call("sendTransaction", ["AA=="])
        from verifylib import checks_online
        with self.assertRaises(ValueError):
            checks_online._eth("http://127.0.0.1:9", "eth_sendRawTransaction", ["0x00"])

    def test_every_shown_row_has_a_check(self):
        cov = catalog.coverage(ROOT, ROWS)
        missing = [k for k, v in cov.items() if v["status"] == "shown" and not v["soft"]
                   and not (v["quick_offline"] or v["online"] or v["tests"])]
        self.assertEqual(missing, [], "shown hard rows with no check")

    def test_cli_coverage_never_calls_an_online_check_offline(self):
        """No Offline/online cell of docs/cli-coverage.md calls a check the registry holds online an offline one
        (review round 3, L11: the day-one row's cell read 'the online check: offline' for `day1-discover-online`,
        whose two stand-in tests are offline but which is itself registered online, read-only)."""
        cov = (ROOT / "docs/cli-coverage.md").read_text()
        for ln in cov.splitlines():
            cells = [c.strip() for c in ln.strip().strip("|").split("|")] if ln.startswith("|") else []
            if len(cells) < 4:
                continue
            self.assertNotRegex(cells[3], r"online check: offline", f"the cell {cells[3]!r} calls an online check offline")
        # and the day-one row now states what the discover check is: online, read-only, as the registry holds it
        row = next(ln for ln in cov.splitlines() if "day1-discover-online" in ln)
        self.assertIn("the discover check: online, read-only", row)
        self.assertEqual(REGISTRY["day1-discover-online"].mode, "online")


SEAL_DIRS = ("e2e/runs", "vault/test", "bridge/artifacts", "lez/results", "checkpoint/artifacts", "bench/report",
             "checkpoint/realblocks", "checkpoint/attestor/runs")  # where the seal catalog finds the recorded seals


def walkable(t: Path, rels=SEAL_DIRS) -> Path:
    """An overlay whose seal records a directory walk reaches (the seal catalog walks without following symlinked
    directories when the copy has no git): every directory on the way and below `rels` a real one, every file a
    symlink, except the files the overlay altered (real files, kept). Nothing is written through a symlink."""
    def real(out: Path, src: Path) -> None:
        if out.is_symlink():
            out.unlink()
        if not out.exists():
            out.mkdir()
            for e in src.iterdir():
                (out / e.name).symlink_to(e)

    for rel in rels:
        cur, src = t, ROOT
        for part in Path(rel).parts:
            cur, src = cur / part, src / part
            real(cur, src)
        for d, _dirs, names in os.walk(ROOT / rel):
            out = t / Path(d).relative_to(ROOT)
            real(out, Path(d))
            for n in names:
                if not (out / n).exists() and not (out / n).is_symlink():
                    (out / n).symlink_to(Path(d) / n)
    return t


class EvidenceRowsFinal(TamperCase):
    """evidence-rows-final: EVIDENCE rows 275-281 and RFP-021 R.9, R.10, RFP-022 F.7, F.13, held to their records. Each
    altered document or record must fail on the figure altered."""

    def test_passes_on_the_tree(self):
        self.assertPasses("evidence-rows-final")

    def test_a_document_figure_altered(self):
        cases = (
            ("docs/EVIDENCE.md", "A3's ratio 0.0770 / 0.0222 / 0.0853 / 0.0358", "A3's ratio 0.0770 / 0.0222 / 0.0835 / 0.0358",
             "EVIDENCE row 276"),
            ("docs/EVIDENCE.md", "honest-1's fee share 0.522 / 0.521", "honest-1's fee share 0.522 / 0.512", "EVIDENCE row 275"),
            ("docs/requirements-status.md", "but 23 and 15 jobs abandoned", "but 24 and 15 jobs abandoned", "RFP-021 R.9"),
            ("docs/requirements-status.md", "all gas 1.022–1.088 times", "all gas 1.022–1.089 times", "RFP-021 R.9"),
            ("docs/requirements-status.md", "0.530–0.617 with it", "0.530–0.618 with it", "RFP-021 R.10"),
            ("docs/requirements-status.md", "275–341 s at window 30", "275–342 s at window 30", "RFP-021 R.10"),
            ("docs/requirements-status.md", "(6 and 6; 3 and 3 before ADR 0046)", "(6 and 5; 3 and 3 before ADR 0046)",
             "RFP-022 F.13"),
            ("docs/requirements-status.md", "301.9 s and 260.4 s", "301.9 s and 206.4 s", "RFP-022 F.13"),
            ("docs/requirements-status.md", "each released once for 99,900,000 base units",
             "each released once for 99,000,000 base units", "RFP-022 F.7"),
            ("docs/EVIDENCE.md", "(n = 5, t = 4)", "(n = 5, t = 3)", "EVIDENCE row 280"),
            # Review r1 (Kimi) M2: the proving times and the replays the rows state are held too.
            ("docs/EVIDENCE.md", "the prover's own wall time 92.2 s for the checkpoint", "the prover's own wall time 1.0 s for the checkpoint",
             "EVIDENCE row 279"),
            ("docs/EVIDENCE.md", "the client's waits 193.8 s, 14.4 s, 18.9 s", "the client's waits 1 s, 2 s, 3 s", "EVIDENCE row 280"),
            ("docs/EVIDENCE.md", "each replay refused (`NullifierAlreadyUsed`, `ParentNotFinal`)", "each replay refused",
             "EVIDENCE row 279"),
        )
        for path, old, new, where in cases:
            with self.subTest(old):
                self.assertFails("evidence-rows-final", overlay({path: sub(old, new)}), where)

    def test_a_record_altered(self):
        """The records, not the documents: an overlap gate, a payment and a test renamed each fail the documents."""
        t = overlay({"lez/updater/results/gate-overlap.txt": sub("\t0.0383\t(20771459", "\t0.0483\t(20771459")})
        self.assertFails("evidence-rows-final", t, "RFP-021 R.9")
        t = overlay({"e2e/runs/zone-bridge-rc3-erc20/vault-checks.json": sub('"paidWei": "99900000"', '"paidWei": "99800000"', 2)})
        self.assertFails("evidence-rows-final", t, "RFP-022 F.7")
        t = overlay({"e2e/runs/zone-bridge-rc3-erc20/vault-checks.json": sub('"refused": "ParentNotFinal"', '"refused": "none"')})
        self.assertFails("evidence-rows-final", t, "the replays refused")
        t = overlay({"e2e/runs/zone-bridge-rc3-rotation/prover/done/0-checkpoint.json": sub('"seconds": 193.8', '"seconds": 93.8')})
        self.assertFails("evidence-rows-final", t, "EVIDENCE row 280")
        t = overlay({"lez/results/f13/prove-bridge.md": sub("execute_and_prove 301.9 s", "execute_and_prove 310.9 s")})
        self.assertFails("evidence-rows-final", t, "RFP-022 F.13")
        t = overlay({"cli/src/lez.rs": sub("fn the_claim_takes_the_layout_of_the_program_the_zone_runs",
                                           "fn the_claim_layout")})
        self.assertFails("evidence-rows-final", t, "EVIDENCE row 281")
        t = overlay({"e2e/both-halves/results/f13/fees.md": sub("| 1 | 0 | 1000000000000000 | 6 | 5 |",
                                                                "| 1 | 0 | 1000000000000000 | 5 | 5 |")})
        self.assertFails("evidence-rows-final", t, "the sets before any claim differ")


class EvidenceRows(TamperCase):
    """evidence-rows-integ: EVIDENCE rows 271-274 and the rows citing them, held to what the checks recompute (review
    round 1 of the integration, M5 and L2: these figures were labels no check read). Each altered copy must fail on
    the figure altered, not on another (the copies are walkable, so the seal counts are the tree's)."""

    def test_a_walkable_copy_passes(self):
        self.assertPasses("evidence-rows-integ", walkable(overlay({})))

    def test_passes_on_the_tree(self):
        self.assertPasses("evidence-rows-integ")

    def test_row_271_wrong_count(self):
        t = walkable(overlay({"docs/EVIDENCE.md": sub(": 672 updates applied, every one by", ": 673 updates applied, every one by")}))
        self.assertFails("evidence-rows-integ", t, "672 updates applied")

    def test_row_273_wrong_count(self):
        t = walkable(overlay({"docs/EVIDENCE.md": sub("22 of 22 of 3.1.0", "23 of 23 of 3.1.0")}))
        self.assertFails("evidence-rows-integ", t, "does not state '79 of 79 seals of 3.0.0 and 22 of 22 of 3.1.0'")

    def test_row_274_wrong_count(self):
        t = walkable(overlay({"docs/EVIDENCE.md": sub("14 of 14 steps ok; CI's check stage passes", "15 of 15 steps ok; CI's check stage passes")}))
        self.assertFails("evidence-rows-integ", t, "does not state '14 of 14 steps ok'")

    def test_row_272_wrong_total(self):
        row = next(ln for ln in (ROOT / "docs/EVIDENCE.md").read_text().splitlines() if ln.startswith("| 272 | "))
        total = re.search(r"\| (\d+) checks: ", row).group(1)
        t = walkable(overlay({"docs/EVIDENCE.md": sub(f"| {total} checks: ", f"| {int(total) + 1} checks: ")}))
        self.assertFails("evidence-rows-integ", t, "EVIDENCE row 272")

    def test_status_rows_and_verify_md(self):
        t = walkable(overlay({"docs/requirements-status.md": sub("the longest interval 20 min 40 s", "the longest interval 20 min 41 s")}))
        self.assertFails("evidence-rows-integ", t, "RFP-021 S.1")
        t = walkable(overlay({"docs/requirements-status.md": sub("(`--coverage`: 127 of 127)", "(`--coverage`: 126 of 127)")}))
        self.assertFails("evidence-rows-integ", t, "RFP-022 R.11")
        t = walkable(overlay({"docs/VERIFY.md": sub("39 run only on a", "37 run only on a")}))
        self.assertFails("evidence-rows-integ", t, "docs/VERIFY.md")

    def test_a_missing_row_or_citation(self):
        """Row 274 renumbered away; RFP-022 F.7 no longer citing row 273: each fails."""
        t = walkable(overlay({"docs/EVIDENCE.md": sub("| 274 | ", "| 299 | ")}))
        self.assertFails("evidence-rows-integ", t, "EVIDENCE row 274: not found")
        t = walkable(overlay({"docs/requirements-status.md": sub("206, 266, 273, 279, 280; `vault/test/real/ZoneBridgeRc3Multi.t.sol`",
                                                                 "206, 266, 279, 280; `vault/test/real/ZoneBridgeRc3Multi.t.sol`")}))
        self.assertFails("evidence-rows-integ", t, "does not cite row 273")

    def test_review_round_2_figures(self):
        """Review round 2 (M3, L6): a digit put before a figure, and each figure only one guard reads, altered alone."""
        cases = (
            ("docs/EVIDENCE.md", "672 updates applied", "1672 updates applied", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "as of LEZ block 25,776 (", "as of LEZ block 25,776.5 (", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", ", 0 over 1 h", ", 10 over 1 h", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "224 updates on each full UTC day", "225 updates on each full UTC day", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "1 min 23 s", "1 min 24 s", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "463–494 of 512", "463–495 of 512", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "from block 75 (", "from block 74 (", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "673 = 1 + 672", "674 = 1 + 673", "EVIDENCE row 271"),
            ("docs/EVIDENCE.md", "22 more of 3.0.0", "21 more of 3.0.0", "EVIDENCE row 273"),
            ("docs/requirements-status.md", "672 updates to 26 Nov 2026", "673 updates to 26 Nov 2026", "RFP-022 S.1"),
            ("docs/requirements-status.md", "14 of 14 steps (row 274", "15 of 15 steps (row 274", "RFP-022 R.10"),
            ("docs/requirements-status.md", "66 of the 68 run in CI (56 of 59", "67 of the 68 run in CI (57 of 59",
             "RFP-022 R.11"),
            ("docs/VERIFY.md", "39 run only on a", "139 run only on a", "docs/VERIFY.md"))
        for rel, old_, new_, where in cases:
            with self.subTest(figure=old_):
                t = walkable(overlay({rel: sub(old_, new_)}))
                self.assertFails("evidence-rows-integ", t, where)

    def test_the_record_moves_the_check(self):
        """A step of the rc3 round trip marked failed: the row's 14 of 14 no longer holds."""
        t = walkable(overlay({"e2e/runs/rc3-roundtrip-1/run/steps.tsv": sub("\tok\t", "\tfailed\t")}))
        self.assertFails("evidence-rows-integ", t, "EVIDENCE row 274")


# ---- the command line ---------------------------------------------------------------------------------------------

def cli(*args, root=ROOT):
    p = subprocess.run([sys.executable, "-I", str(root / "scripts/verify.py"), *args], capture_output=True, text=True,
                       cwd=root, timeout=900)
    return p.returncode, p.stdout + p.stderr


class CommandLine(TamperCase):
    def test_list(self):
        rc, out = cli("--list")
        self.assertEqual(rc, 0)
        self.assertIn("run-record  [run-record, offline, quick]", out)

    def test_list_json(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        f = SCRATCH / "test-verify-list.json"
        try:
            rc, out = cli("run-record", "family:signature", "--list", "--json", str(f))
            self.assertEqual(rc, 0, out)
            got = json.loads(f.read_text())
            self.assertEqual({c["id"] for c in got}, {c.id for c in REGISTRY.values()
                                                      if c.id == "run-record" or c.family == "signature"})
            self.assertTrue(all(c["what"] and c["against"] for c in got))
        finally:
            f.unlink(missing_ok=True)

    def test_row_selector(self):
        for sel in ("RFP-021:F.14", "021/F.14", "F.14@021", "RFP-021 F.14"):
            rc, out = cli(sel, "--list")
            self.assertEqual(rc, 0, out)
            self.assertIn("lab-roots", out, sel)

    def test_ambiguous_and_unknown(self):
        rc, out = cli("F.4")
        self.assertNotEqual(rc, 0)
        self.assertIn("give the RFP", out)
        rc, out = cli("no-such-check")
        self.assertNotEqual(rc, 0)

    def test_exit_status(self):
        rc, out = cli("acc-leaf-vectors")
        self.assertEqual(rc, 0, out)
        t = overlay({"vectors/acc-leaf.json": sub('"', "'", 1)})
        rc, out = cli("acc-leaf-vectors", root=t)
        self.assertEqual(rc, 1, out)
        self.assertIn("FAIL  acc-leaf-vectors", out)

    def test_coverage(self):
        rc, out = cli("--coverage")
        self.assertEqual(rc, 0)
        self.assertIn("RFP-021 hard rows", out)


# ---- tamper tests, family by family ---------------------------------------------------------------------------------

RR = runrecord.RECORD_DIR


class RunRecordOffline(TamperCase):
    def test_unaltered(self):
        self.assertPasses("run-record")

    def test_summary_number(self):
        s = json.loads((ROOT / RR / "summary.json").read_text())
        inst = next(iter(s["instances"]))
        n = s["instances"][inst]["updates_applied"]
        t = overlay({f"{RR}/summary.json": sub(f'"updates_applied": {n}', f'"updates_applied": {n + 1}')})
        self.assertFails("run-record", t, "updates_applied")

    def test_a_time_moved(self):
        rows = (ROOT / RR / "transactions.tsv").read_text().splitlines()
        line = next(ln for ln in rows[1:] if "\tfinish\tapplied\t" in ln)
        t_old = line.split("\t")[1]
        t_new = t_old[:11] + ("23" if t_old[11:13] != "23" else "22") + t_old[13:]
        t = overlay({f"{RR}/transactions.tsv": sub(line, line.replace(t_old, t_new))})
        self.assertFails("run-record", t)

    def test_a_finish_removed(self):
        rows = (ROOT / RR / "transactions.tsv").read_text().splitlines()
        line = next(ln for ln in rows[1:] if "\tfinish\tapplied\t" in ln)
        t = overlay({f"{RR}/transactions.tsv": sub(line + "\n", "")})
        self.assertFails("run-record", t)

    def test_record_md(self):
        t = overlay({f"{RR}/RECORD.md": sub("0 over 3 h", "1 over 3 h")})
        self.assertFails("run-record", t, "RECORD.md")

    def forged(self, chain_edit=None):
        """A copy whose chain.json is edited and whose summary.json and RECORD.md are regenerated from it with the
        record's own functions, so that only the consistency checks can catch it (review round 1, H1)."""
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        rows = runrecord.read_tsv(ROOT / RR / "transactions.tsv")
        if chain_edit:
            chain_edit(chain)
        summary = json.loads(json.dumps(runrecord.summarize(rows, chain)))
        return overlay({f"{RR}/chain.json": lambda _b: (json.dumps(chain, indent=1, sort_keys=True) + "\n").encode(),
                        f"{RR}/summary.json": lambda _b: (json.dumps(summary, indent=1, sort_keys=True) + "\n").encode(),
                        f"{RR}/RECORD.md": lambda _b: runrecord.render(summary).encode()})

    def test_forging_helper_is_faithful(self):
        self.assertPasses("run-record", self.forged())

    def test_chain_facts_forged_together(self):
        """Review round 1, H1's mutation: as-of block, hash and time, blocks, pauses, read time and counters all
        replaced, the derived files regenerated."""
        def edit(c):
            c["as_of"] = {"block": 999999, "hash": "0" * 64, "time_utc": "1970-01-01T00:00:00.000Z"}
            c["blocks"] = 999999
            c["block_gaps_over_60s"] = []
            c["read_at_utc"] = "1970-01-01T00:00:00Z"
            c["instance_counters_read"] = {}
        r = self.assertFails("run-record", self.forged(edit), "chain.json")
        self.assertFalse(any("recomputed and linked" in ln for ln in r.lines), r.lines)

    def test_blocks_count_forged(self):
        self.assertFails("run-record", self.forged(lambda c: c.update(blocks=c["blocks"] + 1)), "blocks")

    def test_a_counter_forged(self):
        def edit(c):
            inst = next(iter(c["instance_counters_read"]))
            c["instance_counters_read"][inst]["applied_by_as_of"] += 1
        self.assertFails("run-record", self.forged(edit), "counter")

    def test_a_counter_forged_consistently(self):
        """The counter read at the as-of block and the one by it, both one more: only the finishes can tell."""
        def edit(c):
            inst = next(iter(c["instance_counters_read"]))
            c["instance_counters_read"][inst]["applied_by_as_of"] += 1
            c["instance_counters_read"][inst]["applied"] += 1
        self.assertFails("run-record", self.forged(edit), "finishes give")

    def test_as_of_time_before_the_lines(self):
        """The as-of block's time moved before the newest line's, nothing else changed."""
        rows = runrecord.read_tsv(ROOT / RR / "transactions.tsv")

        def edit(c):
            c["as_of"]["time_utc"] = rows[-1]["time_utc"][:11] + "00:00:00.000Z"
        self.assertFails("run-record", self.forged(edit), "before the newest line")

    def test_a_pause_forged(self):
        def edit(c):
            c["block_gaps_over_60s"][0]["seconds"] += 10
        self.assertFails("run-record", self.forged(edit), "pause")

    def test_an_instance_configuration_forged(self):
        def edit(c):
            inst = next(iter(c["instances"]))
            c["instances"][inst]["sync_committee_size"] = 256
        self.assertFails("run-record", self.forged(edit), "Init line")

    def test_as_of_hash_not_the_lines(self):
        """A line in the as-of block names that block's hash: chain.json's must be the same."""
        rows = runrecord.read_tsv(ROOT / RR / "transactions.tsv")
        last = rows[-1]

        def edit(c):
            c["as_of"] = {"block": int(last["block"]), "hash": "1" * 64, "time_utc": last["time_utc"]}
            c["blocks"] = int(last["block"])
            for v in c["instance_counters_read"].values():
                v["head"] = int(last["block"])
        self.assertFails("run-record", self.forged(edit), "as-of block's hash")

    def test_an_interval_hidden(self):
        """Dropping an update shortens nothing silently: the intervals and counts recomputed differ."""
        rows = (ROOT / RR / "transactions.tsv").read_text().splitlines()
        fin = [ln for ln in rows[1:] if "\tfinish\tapplied\t" in ln]
        t = overlay({f"{RR}/transactions.tsv": sub(fin[len(fin) // 2] + "\n", "")})
        self.assertFails("run-record", t, "summary.json")


class ScannerState(unittest.TestCase):
    """runrecord.scan's attribution state changes only with what the chain applied (review round 1, M5)."""

    def test_a_refused_duplicate_open_keeps_the_open_time(self):
        """Open applied at t, Finish at t + 350 s, a refused duplicate Open at the Finish's time (round 2, L1)."""
        base = {c: "-" for c in runrecord.COLUMNS}
        inst, job = "I" * 44, "J" * 44

        def line(block, t, step, outcome):
            return {**base, "block": str(block), "time_utc": t, "block_hash": "0" * 64, "index": "0",
                    "tx": f"{block:064x}", "step": step, "outcome": outcome, "instance": inst, "job": job,
                    "sender": "S", "finalized_slot": "64", "finalized_root": "1" * 64}
        rows = [line(10, "2026-11-23T10:00:00.000Z", "open", "applied"),
                line(45, "2026-11-23T10:05:50.000Z", "finish", "applied"),
                line(46, "2026-11-23T10:05:50.000Z", "open", "reverted")]
        chain = {"as_of": {"block": 50, "hash": "0" * 64, "time_utc": "2026-11-23T10:06:30.000Z"}, "blocks": 50,
                 "first_block_with_time": None, "block_gaps_over_60s": [], "instances": {inst: {}}}
        v = runrecord.summarize(rows, chain)["instances"][inst]
        self.assertEqual(v["open_to_finish_s"], {"median": 350.0, "max": 350.0})
        self.assertEqual(v["jobs_opened_not_finished"], [])

    def test_a_refused_init_changes_nothing(self):
        program, inst, job, signer = b"\x01" * 32, b"\x02" * 32, b"\x03" * 32, b"\x09" * 32
        good = {"chain_id": 11155111, "sync_committee_size": 512, "adoption_parts": 2}
        hostile = {"chain_id": 666, "sync_committee_size": 1, "adoption_parts": 9}

        class FakeIdl:
            def instruction(self, msg):
                return {b"I1": ("init", {"config": good}), b"I2": ("init", {"config": hostile})}[msg]

        def block(i, msg, reverted):
            tx = {"root": {"to": {"program_account_id": program, "account_id": inst}, "message": msg},
                  "hash": bytes([i]) * 32, "signers": [signer], "nonces": [],
                  "public_context": {"actors": [{"account_id": inst}, {"account_id": job}]}}
            return {"header": {"block_id": i, "timestamp": 1_000_000 + 10_000 * i, "hash": bytes([i]) * 32,
                               "prev_block_hash": bytes([i - 1]) * 32}, "hash_ok": True,
                    "reverted": [0] if reverted else [], "transactions": [tx]}

        real = runrecord.lez.decode_block
        runrecord.lez.decode_block = lambda raw: raw
        try:
            lines, chain, probs = runrecord.scan([block(1, b"I1", False), block(2, b"I2", True)], program, FakeIdl())
        finally:
            runrecord.lez.decode_block = real
        self.assertEqual(probs, [])
        self.assertEqual([ln["outcome"] for ln in lines], ["applied", "reverted"])
        self.assertEqual(chain["instances"][lez.b58encode(inst)]["sync_committee_size"], 512)
        self.assertEqual(chain["instances"][lez.b58encode(inst)]["chain_id"], 11155111)


class StandInRpc:
    """A JSON-RPC stand-in on 127.0.0.1 serving recorded blocks: getLastBlockId, getBlockRange, getBlock,
    getTransaction, and getAccountView for the accounts it is given."""

    def __init__(self, blocks: dict, files: dict | None = None, accounts: dict | None = None,
                 balances: dict | None = None):
        self.blocks = blocks
        self.balances = balances or {}  # account, base58 -> native balance
        self.files = files or {}  # GET path -> JSON
        self.accounts = accounts or {}  # (account, program), base58 -> the actor state's bytes
        self.txs = {}
        for b64 in blocks.values():
            for tx in lez.decode_block(base64.b64decode(b64))["transactions"]:
                self.txs[tx["hash"].hex()] = base64.b64encode(tx["raw"]).decode()
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_GET(self):
                body = json.dumps(outer.files.get(self.path)).encode()
                self.send_response(200 if self.path in outer.files else 404)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                m, p = req["method"], req.get("params", [])
                if m == "getLastBlockId":
                    res = max(outer.blocks)
                elif m == "getBlockRange":
                    res = [outer.blocks[i] for i in range(p[0], p[1] + 1) if i in outer.blocks]
                elif m == "getBlock":
                    res = outer.blocks.get(p[0])
                elif m == "getTransaction":
                    res = outer.txs.get(p[0])
                elif m == "getAccountBalance":
                    res = outer.balances.get(p[0])
                elif m == "getAccountView":
                    key = (p[0]["account_id"], p[0]["program_account_id"])
                    res = ({"nonce": 0, "data": {"actor_states": {key[1]: list(outer.accounts[key])}}}
                           if key in outer.accounts else None)
                else:
                    res = None
                body = json.dumps({"jsonrpc": "2.0", "id": req.get("id"), "result": res}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()  # joins the request threads and closes the listening socket


FIXTURE = ROOT / "scripts/verifylib/testdata/lez-rc3-blocks-25190-25260.json"


def attestation_program_files(program: str):
    """What the stand-in serves for the network's attestation program: /programs.json naming it, and its immutable
    loader header naming the attestation image (as LEZ's loader writes it)."""
    files = {"/programs.json": {"programs": {"ferrata_attestation": {"program_account": program,
                                                                    "image_id": "0x" + runrecord.ATTESTATION_IMAGE}}}}
    header = bytes([0]) + bytes.fromhex(runrecord.ATTESTATION_IMAGE) + bytes(32) + bytes([1])
    return files, {(program, lez.b58encode(runrecord.LOADER)): header}


class RunRecordOnline(TamperCase):
    """run-record-online against a stand-in RPC serving 71 recorded blocks of the test network, on a record of
    those blocks made by the same code the maintainer's build uses. The fixture cannot start at the chain's first
    block, so these runs set Ctx.record_from_block (the command line never does) and take the instances' Init
    configuration from the record; everything after that is the production path."""

    def setUp(self):
        fx = json.loads(FIXTURE.read_text())
        self.blocks = {fx["first"] + i: b for i, b in enumerate(fx["blocks"])}
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        program = lez.b58decode(chain["program_account"])
        lines, ch, probs = runrecord.scan((base64.b64decode(self.blocks[i]) for i in sorted(self.blocks)), program,
                                          runrecord.lc.Idl.load(ROOT / "idl/ferrata_attestation.idl.json"),
                                          first=fx["first"], instances=chain["instances"])
        self.assertEqual(probs, [])
        self.assertGreaterEqual(sum(1 for ln in lines if ln["step"] == "finish"), 2)
        ch.update(program_account=chain["program_account"], first_block=fx["first"])
        self.base = Path(tempfile.mkdtemp(prefix="test-verify-rr-", dir=SCRATCH))
        runrecord.write_tsv(lines, self.base / "transactions.tsv")
        (self.base / "chain.json").write_text(json.dumps(ch, indent=1, sort_keys=True) + "\n")
        self.first = fx["first"]
        self.program = chain["program_account"]
        files, accounts = attestation_program_files(self.program)
        self.rpc = StandInRpc(dict(self.blocks), files, accounts)

    def tearDown(self):
        self.rpc.close()
        shutil.rmtree(self.base, ignore_errors=True)
        super().tearDown()

    def copy(self, tsv=None, chain=None):
        edits = {}
        for name, fn in (("transactions.tsv", tsv), ("chain.json", chain)):
            data = (self.base / name).read_bytes()
            edits[f"{RR}/{name}"] = (lambda _d, _fn=fn, _data=data: (_fn(_data) if _fn else _data))
        return overlay(edits)

    def online(self, root):
        return run("run-record-online", root, lez_url=self.rpc.url, record_from_block=self.first)

    def test_unaltered(self):
        r = self.online(self.copy())
        self.assertEqual(r.status, "PASS", "\n".join(r.lines))

    def _first(self, step):
        return next(ln for ln in (self.base / "transactions.tsv").read_text().splitlines()[1:]
                    if f"\t{step}\t" in ln)

    def test_tx_hash(self):
        line = self._first("finish")
        tx = line.split("\t")[4]
        bad = tx[:-1] + ("0" if tx[-1] != "0" else "1")
        r = self.online(self.copy(tsv=sub(line, line.replace(tx, bad))))
        self.assertEqual(r.status, "FAIL")
        self.assertTrue(any("not in the chain" in ln for ln in r.lines), r.lines)

    def test_a_field(self):
        line = self._first("open")
        cols = line.split("\t")
        i = runrecord.COLUMNS.index("participants")
        cols[i] = str(int(cols[i]) + 1)
        r = self.online(self.copy(tsv=sub(line, "\t".join(cols))))
        self.assertEqual(r.status, "FAIL")
        self.assertTrue(any("differs in participants" in ln for ln in r.lines), r.lines)

    def test_a_line_missing(self):
        line = self._first("miller")
        r = self.online(self.copy(tsv=sub(line + "\n", "")))
        self.assertEqual(r.status, "FAIL")
        self.assertTrue(any("missing from the record" in ln for ln in r.lines), r.lines)

    def test_as_of_hash(self):
        ch = json.loads((self.base / "chain.json").read_text())
        h = ch["as_of"]["hash"]
        r = self.online(self.copy(chain=sub(h, h[::-1])))
        self.assertEqual(r.status, "FAIL")

    def test_a_block_byte(self):
        """The stand-in serves a block with one byte of a transaction changed: its hash no longer holds."""
        bid = int(self._first("open").split("\t")[0])
        raw = bytearray(base64.b64decode(self.rpc.blocks[bid]))
        raw[len(raw) // 2] ^= 1
        self.rpc.blocks[bid] = base64.b64encode(bytes(raw)).decode()
        r = self.online(self.copy())
        self.assertEqual(r.status, "FAIL", "\n".join(r.lines))


class RunRecordOnlineMore(RunRecordOnline):
    """run-record-online, run-record-counters and run-record-beacon on the same stand-ins (review round 1, M4)."""

    def test_unaltered(self):
        pass  # RunRecordOnline's

    def test_the_program_the_record_names(self):
        other = lez.b58encode(b"\x07" * 32)
        r = self.online(self.copy(chain=sub(self.program, other)))
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("the network's attestation program" in ln for ln in r.lines), r.lines)

    def test_an_empty_list(self):
        header = (self.base / "transactions.tsv").read_text().splitlines()[0] + "\n"
        r = self.online(self.copy(tsv=lambda _b: header.encode()))
        self.assertEqual(r.status, "FAIL", r.lines)

    def test_counters_with_no_instance_on_chain(self):
        """The stand-in's chain holds no Init: no counter can be read, and that is a failure, not a pass."""
        r = run("run-record-counters", self.copy(), lez_url=self.rpc.url)
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("no applied Init" in ln for ln in r.lines), r.lines)

    def beacon(self, roots: dict):
        return StandInRpc({}, {f"/eth/v1/beacon/headers/{slot}": {"data": {"root": root, "canonical": True}}
                               for slot, root in roots.items()})

    def test_beacon_against_sepolia_golden(self):
        """The record's applied headers against a stand-in serving the roots Sepolia's beacon API served."""
        golden = json.loads(GOLDEN.read_text())["updates"]
        b = self.beacon({u["finalized_slot"]: u["finalized_root"] for u in golden})
        try:
            r = run("run-record-beacon", self.copy(), beacon=[b.url])
            self.assertEqual(r.status, "PASS", r.lines)
            u = golden[0]
            b.files[f"/eth/v1/beacon/headers/{u['finalized_slot']}"]["data"]["root"] = "0x" + "ab" * 32
            r = run("run-record-beacon", self.copy(), beacon=[b.url])
            self.assertEqual(r.status, "FAIL", r.lines)
        finally:
            b.close()

    def test_beacon_with_no_update(self):
        header = (self.base / "transactions.tsv").read_text().splitlines()[0] + "\n"
        r = run("run-record-beacon", self.copy(tsv=lambda _b: header.encode()), beacon=["http://127.0.0.1:9"])
        self.assertEqual(r.status, "FAIL", r.lines)


GOLDEN = ROOT / "scripts/verifylib/testdata/sepolia-beacon-golden.json"
BLOCK_9421 = ROOT / "scripts/verifylib/testdata/lez-rc3-block-9421.json"


def _merkle_ok(leaf: bytes, branch: list, gindex: int, root: bytes) -> bool:
    """The consensus specs' is_valid_merkle_branch, written here apart from the decoders."""
    import hashlib
    depth, index, node = gindex.bit_length() - 1, gindex - (1 << (gindex.bit_length() - 1)), leaf
    if len(branch) != depth:
        return False
    for i, sib in enumerate(branch):
        node = hashlib.sha256(sib + node if (index >> i) & 1 else node + sib).digest()
    return node == root


class Decoders(unittest.TestCase):
    """The light-client decoders (verifylib/lc.py, lez.py) against values that do not come from them: Sepolia's
    beacon API (GOLDEN), the Merkle branches each update carries to its attested state root, and Ethereum's own
    SSZ updates of mainnet (lez/lab/), and malformed inputs (review round 1, M1 and M2)."""

    FINALIZED_ROOT_GINDEX = {"Electra": 169, "Gloas": 735}
    NEXT_SYNC_COMMITTEE_GINDEX = {"Electra": 87, "Gloas": 2946}

    @classmethod
    def setUpClass(cls):
        cls.idl = runrecord.lc.Idl.load(ROOT / "idl/ferrata_attestation.idl.json")
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        program = lez.b58decode(chain["program_account"])
        cls.raw_blocks, cls.opens = [], {}
        for f in (FIXTURE, BLOCK_9421):
            for b64 in json.loads(f.read_text())["blocks"]:
                raw = base64.b64decode(b64)
                cls.raw_blocks.append(raw)
                for tx in lez.decode_block(raw)["transactions"]:
                    if tx["root"] and tx["root"]["to"]["program_account_id"] == program:
                        name, args = cls.idl.instruction(tx["root"]["message"])
                        if name == "open":
                            cls.opens[tx["hash"].hex()] = (tx["root"]["message"], args["update"])
        cls.golden = json.loads(GOLDEN.read_text())["updates"]

    def test_every_golden_update_is_in_the_fixtures(self):
        self.assertEqual({u["tx"] for u in self.golden} - set(self.opens), set())

    def test_against_sepolia(self):
        for g in self.golden:
            u = runrecord.lc.compact_update(self.opens[g["tx"]][1])
            bits = bytes.fromhex(g["sync_committee_bits"][2:])
            with self.subTest(block=g["block"]):
                self.assertEqual((u["attested_slot"], u["finalized_slot"], u["signature_slot"]),
                                 (g["attested_slot"], g["finalized_slot"], g["signature_slot"]))
                self.assertEqual("0x" + u["finalized_root"].hex(), g["finalized_root"])
                self.assertEqual("0x" + u["attested_root"].hex(), g["attested_root"])
                self.assertEqual(u["sync_committee_bits"], bits)
                self.assertEqual("0x" + u["sync_committee_signature"].hex(), g["sync_committee_signature"])
                self.assertEqual(u["participants"], int.from_bytes(bits, "little").bit_count())

    def test_branches_to_the_attested_state(self):
        for g in self.golden:
            u = runrecord.lc.compact_update(self.opens[g["tx"]][1])
            with self.subTest(block=g["block"]):
                self.assertTrue(_merkle_ok(u["finalized_root"], u["finality_branch"],
                                           self.FINALIZED_ROOT_GINDEX[u["fork"]], u["attested_state_root"]))
                if u["next_committee"]:
                    import hashlib
                    s = u["next_committee_summary"]
                    agg = hashlib.sha256(s[32:80] + bytes(16)).digest()
                    leaf = hashlib.sha256(s[:32] + agg).digest()
                    self.assertTrue(_merkle_ok(leaf, u["next_committee_branch"],
                                               self.NEXT_SYNC_COMMITTEE_GINDEX[u["fork"]], u["attested_state_root"]))
        self.assertTrue(any(runrecord.lc.compact_update(self.opens[g["tx"]][1])["next_committee"] for g in self.golden))

    def test_idl_as_the_explorer_reads_it(self):
        """Signed integers, the 32-byte aliases, and a vector that cannot fit what is left (review round 2, L3)."""
        idl = runrecord.lc.Idl({
            "types": [{"name": "E", "type": {"kind": "struct", "fields": []}},
                      {"name": "S", "type": {"kind": "struct", "fields": [{"name": "a", "type": "i16"},
                                                                         {"name": "b", "type": "[u8; 32]"}]}}],
            "instructions": [{"name": "x", "discriminator": [0], "args": [{"name": "v", "type": {"vec": {"defined": "E"}}}]},
                             {"name": "y", "discriminator": [1], "args": [{"name": "s", "type": {"defined": "S"}}]}]})
        with self.assertRaises(lez.DecodeError):
            idl.instruction(bytes([0, 5, 0, 0, 0]))
        self.assertEqual(idl.instruction(bytes([1]) + (65535).to_bytes(2, "little") + bytes(32))[1]["s"]["a"], -1)

    def test_a_block_rehashed_but_not_linked(self):
        """Its own hash recomputed after its previous hash was changed: it decodes, but does not follow its parent."""
        b1, b2 = self.raw_blocks[0], bytearray(self.raw_blocks[1])
        b2[8] ^= 1
        d = lez.decode_block(bytes(b2))
        b2[40:72] = d["computed_hash"]
        self.assertTrue(lez.decode_block(bytes(b2))["hash_ok"])
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        _, _, probs = runrecord.scan([b1, bytes(b2)], lez.b58decode(chain["program_account"]), self.idl,
                                     first=1, instances=chain["instances"])
        self.assertTrue(any("previous hash" in p for p in probs), probs)

    def test_trailing_bytes(self):
        raw = self.raw_blocks[0]
        with self.assertRaises(lez.DecodeError):
            lez.decode_block(raw + b"\0")
        msg, upd = self.opens[self.golden[0]["tx"]]
        with self.assertRaises(lez.DecodeError):
            self.idl.instruction(msg + b"\0")
        with self.assertRaises(lez.DecodeError):
            runrecord.lc.compact_update(upd + b"\0")

    def _with_attested(self, upd: bytes, header: bytes) -> bytes:
        n = int.from_bytes(upd[1:5], "little")
        return upd[:1] + len(header).to_bytes(4, "little") + header + upd[5 + n:]

    def test_a_malformed_header(self):
        """Review round 1, M1: bytes appended inside the attested header's vector (its length adjusted)."""
        upd = self.opens[self.golden[0]["tx"]][1]
        n = int.from_bytes(upd[1:5], "little")
        with self.assertRaises(lez.DecodeError):
            runrecord.lc.compact_update(self._with_attested(upd, upd[5:5 + n] + b"junk"))
        with self.assertRaises(lez.DecodeError):
            runrecord.lc.compact_update(self._with_attested(upd, upd[5:5 + n - 32]))

    def _with_bits(self, upd: bytes, bits: bytes) -> bytes:
        tail = 64 + 96 + 8  # the 512 bits, the signature, the signature slot
        return upd[:-tail] + bits + upd[-tail + 64:]

    def test_small_committees(self):
        """Ferrata accepts any power-of-two committee: the bitvector is n bits rounded up to bytes, padding zero."""
        upd = self.opens[self.golden[0]["tx"]][1]
        for n, bits in ((1, b"\x01"), (2, b"\x03"), (4, b"\x0f"), (8, b"\xff"), (16, b"\xff\x7f")):
            with self.subTest(n=n):
                u = runrecord.lc.compact_update(self._with_bits(upd, bits), n)
                self.assertEqual(u["participants"], int.from_bytes(bits, "little").bit_count())
        with self.assertRaises(lez.DecodeError):
            runrecord.lc.compact_update(self._with_bits(upd, b"\x1f"), 4)  # a padding bit set
        with self.assertRaises(lez.DecodeError):
            runrecord.lc.compact_update(self._with_bits(upd, b"\x0f\x00"), 4)  # one byte too many

    def test_electra_from_mainnet_ssz(self):
        """A pre-Gloas layout: Ethereum's own SSZ light-client updates of mainnet (lez/lab/), re-encoded as a
        CompactUpdate here; the decoded finalized header must be the one the update's finality branch proves."""
        import hashlib
        d = ROOT / "lez/lab/results/record/net/mainnet/updates"

        def split(b, fields):
            fixed = sum(4 if f is None else f for f in fields)
            parts, offs, pos = [], [], 0
            for i, f in enumerate(fields):
                if f is None:
                    offs.append((i, int.from_bytes(b[pos:pos + 4], "little")))
                    parts.append(None)
                    pos += 4
                else:
                    parts.append(b[pos:pos + f])
                    pos += f
            for k, (i, o) in enumerate(offs):
                parts[i] = b[o:offs[k + 1][1] if k + 1 < len(offs) else len(b)]
            self.assertEqual(offs[0][1], fixed)
            return parts

        # LightClientUpdate (Electra): attested, next committee, its branch (6), finalized, finality branch (7),
        # sync aggregate, signature slot; LightClientFinalityUpdate: the same without the committee and its branch
        period = split((d / "00001.period.electra.ssz").read_bytes(), [None, 512 * 48 + 48, 6 * 32, None, 7 * 32, 160, 8])
        final = split((d / "00002.finality.electra.ssz").read_bytes(), [None, None, 7 * 32, 160, 8])
        cases = [("period", period[0], period[1], period[2], period[3], period[4], period[5], period[6]),
                 ("finality", final[0], None, bytes(6 * 32), final[1], final[2], final[3], final[4])]
        for name, att, committee, nbranch, fin, fbranch, agg, sslot in cases:
            summary = b""
            if committee is not None:
                leaves = [hashlib.sha256(committee[i:i + 48] + bytes(16)).digest() for i in range(0, 512 * 48, 48)]
                while len(leaves) > 1:
                    leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
                summary = leaves[0] + committee[512 * 48:]
            enc = (bytes([3]) + len(att).to_bytes(4, "little") + att + bytes([committee is not None]) + summary + nbranch
                   + len(fin).to_bytes(4, "little") + fin + fbranch + agg + sslot)
            u = runrecord.lc.compact_update(enc)
            with self.subTest(update=name):
                self.assertEqual(u["fork"], "Electra")
                self.assertEqual(u["finalized_slot"], int.from_bytes(fin[:8], "little"))
                self.assertEqual(u["attested_slot"], int.from_bytes(att[:8], "little"))
                self.assertEqual(u["signature_slot"], int.from_bytes(sslot, "little"))
                self.assertEqual(u["participants"], int.from_bytes(agg[:64], "little").bit_count())
                self.assertTrue(_merkle_ok(u["finalized_root"], u["finality_branch"], 169, u["attested_state_root"]))
                if committee is not None:
                    agg_root = hashlib.sha256(summary[32:80] + bytes(16)).digest()
                    self.assertTrue(_merkle_ok(hashlib.sha256(summary[:32] + agg_root).digest(),
                                               u["next_committee_branch"], 87, u["attested_state_root"]))
                with self.assertRaises(lez.DecodeError):  # the Electra header has a payload: Gloas's layout refuses it
                    runrecord.lc.compact_update(bytes([4]) + enc[1:])


class Faucet(unittest.TestCase):
    """faucet-terms-online against a stand-in gateway (review round 1, M8)."""

    ACCT = "ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs"

    def faucet(self, published: int, on_chain: int, amount: int = 10**10, hourly: int = 120):
        terms = {"faucet_account": self.ACCT, "balance": published, "amount": str(amount),
                 "limits": {"global_per_hour": hourly}}
        rpc = StandInRpc({}, {"/faucet": terms}, balances={self.ACCT: on_chain})
        try:
            return run("faucet-terms-online", ROOT, lez_url=rpc.url)
        finally:
            rpc.close()

    def test_published_and_chain_agree(self):
        self.assertEqual(self.faucet(10**15, 10**15).status, "PASS")
        self.assertEqual(self.faucet(10**15, 10**15 - 3 * 10**10).status, "PASS")  # three grants in between

    def test_a_published_balance_the_chain_does_not_hold(self):
        r = self.faucet(10**18, 10**15)
        self.assertEqual(r.status, "FAIL", r.lines)

    def test_under_a_day_of_grants(self):
        r = self.faucet(10**12, 10**12)  # 100 grants, under 24 x 120
        self.assertEqual(r.status, "FAIL", r.lines)


class StandInEth:
    """A JSON-RPC stand-in for an Ethereum chain on 127.0.0.1: eth_chainId, eth_blockNumber, and eth_call answered by
    `call(to, data)`."""

    def __init__(self, chain_id: int, call):
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_POST(self):
                req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                m, prm = req["method"], req.get("params", [])
                res = {"eth_chainId": hex(chain_id), "eth_blockNumber": "0x100"}.get(m)
                if m == "eth_call":
                    res = call(prm[0]["to"].lower(), prm[0]["data"])
                body = json.dumps({"jsonrpc": "2.0", "id": req.get("id"), "result": res}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()


class Families(TamperCase):
    """One tamper test for each family whose checks wrap a script or read an endpoint: the check must fail on an
    altered copy, or on a stand-in endpoint that answers what the documents say is not so."""

    def test_docs_a_broken_link(self):
        self.assertPasses("links")
        t = overlay({"docs/VERIFY.md": lambda b: b + b"\n[a page that is not there](not-there.md)\n"})
        self.assertFails("links", t)

    def test_unit_an_unpinned_action(self):
        self.assertPasses("unit-actions-pinned")
        t = overlay({".github/workflows/verify-online.yml": sub(
            "uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4", "uses: actions/checkout@v4")})
        self.assertFails("unit-actions-pinned", t)

    def test_sepolia_a_vault_reopened(self):
        from verifylib import checks_online
        reopened = next(iter(checks_online.VAULTS.values())).lower()
        for addr, want in ((None, "PASS"), (reopened, "FAIL")):
            eth = StandInEth(11155111, lambda to, data, a=addr: "0x" + ("0" * 63 + "1" if to == a else "0" * 64) * 3)
            try:
                r = run("sepolia-vaults-closed", ROOT, sepolia_rpc=[eth.url])
            finally:
                eth.close()
            self.assertEqual(r.status, want, r.lines)

    def test_github_a_conclusion_not_the_documents(self):
        from verifylib import checks_online
        runs = {f"/repos/{repo}/actions/runs/{run_id}": {"path": f".github/workflows/{wf}", "conclusion": want,
                                                          "head_sha": (sha or "f" * 8) + "0" * 32}
                for repo, run_id, wf, want, sha, _ in checks_online.GITHUB_RUNS}
        api = StandInRpc({}, runs)
        try:
            self.assertEqual(run("github-ci-runs", ROOT, github_api=api.url).status, "PASS")
            failed = next(k for k, v in runs.items() if v["conclusion"] == "failure")
            api.files[failed]["conclusion"] = "success"  # a recorded failure reported as green
            r = run("github-ci-runs", ROOT, github_api=api.url)
            self.assertEqual(r.status, "FAIL", r.lines)
        finally:
            api.close()


class OnlineWrappers(unittest.TestCase):
    """The online checks that wrap a repository script, against a stand-in network and the script's own output."""

    def test_day1_discover_against_a_stand_in_network(self):
        """The day-one kit's probe of a network that answers: it passes; one that answers nothing skips."""
        fx = json.loads(FIXTURE.read_text())
        blocks = {fx["first"] + i: b for i, b in enumerate(fx["blocks"])}
        rpc = StandInRpc(dict(blocks), {})
        try:
            r = run("day1-discover-online", ROOT, lez_url=rpc.url)
            self.assertEqual(r.status, "PASS", "\n".join(r.lines))
        finally:
            rpc.close()
        rpc = StandInRpc({}, {})
        try:
            r = run("day1-discover-online", ROOT, lez_url=rpc.url)
            self.assertEqual(r.status, "SKIP", "\n".join(r.lines))
        finally:
            rpc.close()

    def test_sourcify_rows_from_the_scripts_output(self):
        """What `sourcify.sh status` prints, one row per contract: the match word, or none when unreadable."""
        from verifylib import checks_online
        lines = ["vault            0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c  exact_match  "
                 "src/FerrataVault.sol:FerrataVault",
                 "registry         0xa75e653BdaD23d198Ea357277b715aD8A9687673  match  other",
                 "token            0x6b479b044501C8764a326EaBb64b5f441F96C3CB  none  ",
                 "emergencyStop    0x1dB3fa8041E8178f1608c2F7dF627d7EC16C8221  unreadable",
                 "groth16Verifier  0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0  exact_match  v"]
        self.assertEqual([m for _, _, m in checks_online._sourcify_rows(lines)],
                         ["exact_match", "match", "none", None, "exact_match"])
        self.assertEqual(checks_online._sourcify_rows(["check-unaudited.sh: every file is marked"]), [])

    def test_sourcify_status_requires_all_five_recorded_contracts(self):
        """`sourcify-status-online` offline, its script's output mocked: all five fields, each once, at the
        deployment record's own addresses, each an exact match (review round 3, M4: one exact-match row used to
        pass it). A reply sourcify.dev cannot be read for is still a skip, never a pass."""
        from verifylib import checks_online
        deploy = json.loads((ROOT / "e2e/runs/crown-live-3/deploy.json").read_text())
        row = lambda f: f"{f:<16} {deploy[f]}  exact_match  src/X.sol:X"
        real, it = checks_online._run, None

        def mock(lines):
            def _run(ctx, argv, timeout=1800, env=None):
                return 0, list(lines)
            return _run

        def outcome(lines):
            checks_online._run = mock(lines)
            try:
                return run("sourcify-status-online", ROOT, lez_url="http://127.0.0.1:9")
            finally:
                checks_online._run = real

        self.assertEqual(outcome([row(f) for f in checks_online.SOURCIFY_FIELDS]).status, "PASS")
        self.assertEqual(outcome([row("vault")]).status, "FAIL")  # the review's one-row reproduction
        self.assertEqual(outcome([row(f) for f in checks_online.SOURCIFY_FIELDS] + [row("vault")]).status, "FAIL")
        self.assertEqual(outcome([row(f) for f in checks_online.SOURCIFY_FIELDS[:4]]).status, "FAIL")
        wrong = f"{'vault':<16} 0x0000000000000000000000000000000000000000  exact_match  src/X.sol:X"
        self.assertEqual(outcome([wrong] + [row(f) for f in checks_online.SOURCIFY_FIELDS[1:]]).status, "FAIL")
        not_exact = [row(f) for f in checks_online.SOURCIFY_FIELDS]
        not_exact[2] = f"{'token':<16} {deploy['token']}  match  src/X.sol:X"
        self.assertEqual(outcome(not_exact).status, "FAIL")
        unreadable = f"{'vault':<16} {deploy['vault']}  unreadable"
        self.assertEqual(outcome([unreadable] + [row(f) for f in checks_online.SOURCIFY_FIELDS[1:]]).status, "SKIP")


class GasDrop(unittest.TestCase):
    """gasdrop-terms-online against a stand-in gateway (review round 2, M4)."""

    IMAGE = "0xce3ad533d2c72b38c9779f1c692dd784e3fd754612808696e1362da49c7f8125"  # docs/image-ids.md's gas-drop row
    RELAYER = "07" * 32

    def terms(self, amount, balance=100):
        t = {"imageId": self.IMAGE, "relayer": "0x" + self.RELAYER, "amount": amount, "covers": ["claim", "burn"]}
        rpc = StandInRpc({}, {"/relayer/gas-drop": t}, balances={lez.b58encode(bytes.fromhex(self.RELAYER)): balance})
        try:
            return run("gasdrop-terms-online", ROOT, lez_url=rpc.url)
        finally:
            rpc.close()

    def test_a_drop_the_relayer_can_pay(self):
        self.assertEqual(self.terms(10).status, "PASS")
        self.assertEqual(self.terms(100).status, "PASS")  # exactly one drop

    def test_no_drop(self):
        for amount in (0, -1, "ten", None):
            with self.subTest(amount=amount):
                self.assertEqual(self.terms(amount).status, "FAIL")
        self.assertEqual(self.terms(101).status, "FAIL")  # more than the relayer holds


class Dates(unittest.TestCase):
    def test_utc_whatever_the_time_zone(self):
        """runrecord.parse_iso in a zone with daylight saving, in July and November (review round 2, L2)."""
        code = ("import sys; sys.path.insert(0, 'scripts'); from verifylib.runrecord import parse_iso; "
                "print(parse_iso('2026-07-26T10:17:22.807Z'), parse_iso('2026-11-26T10:17:22.807Z'))")
        for tz in ("America/New_York", "Europe/Prague", "UTC"):
            p = subprocess.run([sys.executable, "-I", "-c", code], cwd=ROOT, capture_output=True, text=True,
                               env={**os.environ, "TZ": tz}, timeout=60)
            self.assertEqual(p.stdout.split(), ["1785061042807", "1795688242807"], (tz, p.stderr))


class SiteManifestOutcomes(unittest.TestCase):
    """site-manifest-online's reading of site-manifest.py's verdicts (review round 2, M1): only transport failures,
    twice, make a skip; a definite answer such as HTTP 404 is a failure."""

    PASS_OUT = (0, ["232 of 232 files equal the manifest", "VERDICT: PASS"])

    def outcome(self, *outputs):
        from verifylib import checks_online
        real, it = checks_online._run, iter(outputs)
        checks_online._run = lambda ctx, argv, timeout=1800, env=None: next(it)
        try:
            return run("site-manifest-online", ROOT, lez_url="http://127.0.0.1:9").status
        finally:
            checks_online._run = real

    def failing(self, *lines):
        return (1, list(lines) + ["VERDICT: FAIL (1)"])

    def test_outcomes(self):
        gone = self.failing("FAIL  /x: cannot fetch (HTTPError: 404)")
        slow = self.failing("FAIL  /x: cannot fetch (TimeoutError: The read operation timed out)")
        differs = self.failing("FAIL  /x: served 0123456789abcdef…, manifest fedcba9876543210…")
        self.assertEqual(self.outcome(gone, gone), "FAIL")
        self.assertEqual(self.outcome(slow, slow), "SKIP")
        self.assertEqual(self.outcome(slow, self.PASS_OUT), "PASS")
        self.assertEqual(self.outcome(slow, gone), "FAIL")
        self.assertEqual(self.outcome(differs), "FAIL")
        self.assertEqual(self.outcome(self.failing("FAIL  cannot read the served manifest /m.json: HTTPError: 410")),
                         "FAIL")
        self.assertEqual(self.outcome(self.failing("FAIL  /x: cannot fetch (HTTPError: 503)"),
                                      self.failing("FAIL  /x: cannot fetch (HTTPError: 503)")), "SKIP")


class Runner(unittest.TestCase):
    """The runner's reading of a command's success, and the named tests it must never run (review round 2, M2, M6)."""

    def command(self, code: str):
        from verifylib.core import Check
        c = Check(id="t", title="t", family="unit", mode="offline", argv=("python3", "-I", "-c", code))
        return run_check(c, Ctx(root=ROOT))

    def test_a_test_run_that_ran_nothing_is_a_skip(self):
        skipping = ("import unittest\nclass T(unittest.TestCase):\n    @classmethod\n    def setUpClass(cls):\n"
                    "        raise unittest.SkipTest('no artifact')\n    def test_x(self):\n        pass\n"
                    "unittest.main()")
        self.assertEqual(self.command(skipping).status, "SKIP")
        running = "import unittest\nclass T(unittest.TestCase):\n    def test_x(self):\n        pass\nunittest.main()"
        self.assertEqual(self.command(running).status, "PASS")
        from verifylib.core import ran_nothing
        self.assertIsNone(ran_nothing("# tests 67\n# pass 65\n# fail 0\n# skipped 2\n"))
        self.assertTrue(ran_nothing("# tests 2\n# pass 0\n# skipped 2\n"))

    def test_recipes_are_not_run(self):
        from verifylib import tests_map
        self.assertTrue(tests_map.manual_reason("cd lez && cargo run --bin ferrata-attest -- status --send", {"box"}))
        self.assertTrue(tests_map.manual_reason("modules/scripts/catalog-install.sh RELEASE_DIR LOGOSCTL WORK_DIR",
                                                {"box"}))
        self.assertIsNone(tests_map.manual_reason("cd cli && cargo test --locked -p ferrata-cli --lib", {"box"}))
        self.assertIsNone(tests_map.manual_reason("python3 -I scripts/test_m0.py", {"links"}))
        hand = [c for c in REGISTRY.values() if c.tier == "tests" and c.title.endswith("[by hand]")]
        self.assertTrue(hand)
        for c in hand:
            self.assertIsNone(c.shell)
            self.assertIsNone(c.argv)
            r = run_check(c, Ctx(root=ROOT))
            self.assertEqual(r.status, "SKIP", c.id)
            self.assertIn("The recipe:", r.lines[0])
        sends = [c for c in REGISTRY.values() if c.tier == "tests" and c.shell and tests_map._SENDS.search(c.shell)]
        self.assertEqual(sends, [], "a named test that can send is runnable")


class LezRead(unittest.TestCase):
    """scripts/lez-read.py, the explorer's reads from a terminal, against the stand-in RPC and recorded blocks."""

    def setUp(self):
        fx = json.loads(FIXTURE.read_text())
        self.blocks = {fx["first"] + i: b for i, b in enumerate(fx["blocks"])}
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        progs = {"programs": {"ferrata_attestation": {"program_account": chain["program_account"],
                                                      "image_id": "0x" + runrecord.ATTESTATION_IMAGE}}}
        self.rpc = StandInRpc(dict(self.blocks), {"/programs.json": progs})
        lines, _, _ = runrecord.scan((base64.b64decode(self.blocks[i]) for i in sorted(self.blocks)),
                                     lez.b58decode(chain["program_account"]),
                                     runrecord.lc.Idl.load(ROOT / "idl/ferrata_attestation.idl.json"),
                                     first=fx["first"], instances=chain["instances"])
        self.open = next(ln for ln in lines if ln["step"] == "open")

    def tearDown(self):
        self.rpc.close()

    def read(self, *args):
        p = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/lez-read.py"), "--lez", self.rpc.url, "--json",
                            *args], capture_output=True, text=True, timeout=120)
        return p.returncode, (json.loads(p.stdout) if p.stdout.strip() else None), p.stderr

    def test_block_with_an_update(self):
        rc, out, err = self.read("block", str(self.open["block"]))
        self.assertEqual(rc, 0, err)
        self.assertTrue(out["hash_ok"])
        call = next(t["call"] for t in out["transactions"] if t["call"]["program_name"] == "ferrata_attestation")
        self.assertEqual(call["instruction"], "open")
        self.assertEqual(call["update"]["finalized_slot"], self.open["finalized_slot"])
        self.assertEqual(call["update"]["participants"], self.open["participants"])

    def test_a_scalar_account(self):
        """The names IDL's `Sequence` account is a bare u64 (review round 1, L1)."""
        names, acct = lez.b58encode(b"\x05" * 32), lez.b58encode(b"\x06" * 32)
        rpc = StandInRpc({}, {"/programs.json": {"programs": {"ferrata_names": {"program_account": names,
                                                                                 "image_id": "0x" + "00" * 32}}}},
                         {(acct, names): (7).to_bytes(8, "little")})
        try:
            p = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/lez-read.py"), "--lez", rpc.url, "--json",
                                "account", acct, names], capture_output=True, text=True, timeout=120)
        finally:
            rpc.close()
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(json.loads(p.stdout)["Sequence"], 7, p.stdout)

    def test_transaction(self):
        rc, out, err = self.read("tx", self.open["tx"])
        self.assertEqual(rc, 0, err)
        self.assertTrue(out["hash_ok"])
        self.assertEqual(out["signers"], [self.open["sender"]])

    def test_a_block_that_does_not_hash(self):
        bid = self.open["block"]
        raw = bytearray(base64.b64decode(self.rpc.blocks[bid]))
        raw[85] ^= 1  # a byte of the body: the block's hash no longer covers it
        self.rpc.blocks[bid] = base64.b64encode(bytes(raw)).decode()
        rc, out, err = self.read("block", str(bid))
        self.assertEqual(rc, 1, (out, err))

    # ---- `instance ACCOUNT --alarms`: the status page's rules from a terminal --------------------------

    GENESIS = 1655733600  # Sepolia's genesis, as the public network's instances configure it
    WS_EPOCHS = 3532      # Sepolia's weak-subjectivity period in epochs, as they configure it

    def instance_state(self, slot, next_committee=True, active=True, ws_period_epochs=WS_EPOCHS, genesis_time=GENESIS,
                       seconds_per_slot=12, slots_per_epoch=32, epochs_per_period=256):
        """An InstanceState of the attestation program, borsh-encoded as the program stores it (the IDL's layout:
        id, Config, active, the store, applied, root and parent ids): the Sepolia configuration the public network's
        instances use, a finalized beacon header at `slot`, and the next committee known or not, as given."""
        forks = b"".join((0x90000000 + i).to_bytes(4, "little") for i in range(9))
        cfg = (struct.pack("<QIQQQ", 11155111, 512, slots_per_epoch, epochs_per_period, 1) + forks
               + b"".join((50 + 1000 * i).to_bytes(8, "little") for i in range(9)) + bytes(range(32))
               + struct.pack("<QQ", genesis_time, seconds_per_slot) + bytes([4, 2])
               + struct.pack("<Q", ws_period_epochs))
        header = (slot.to_bytes(8, "little") + (1).to_bytes(8, "little")
                  + bytes.fromhex("02" * 32) + bytes.fromhex("03" * 32) + bytes.fromhex("04" * 32))
        # CompactStore (ferrata.js `decodeCompactStore`): fork byte, the header, the current committee's summary,
        # a flag, the next committee's summary when the flag is set.
        store = (bytes([4]) + header + bytes(32) + bytes(48) + bytes([1 if next_committee else 0])
                 + (bytes(32) + bytes(48) if next_committee else b""))
        assert len(cfg) == 202 and len(header) == 112 and len(store) in (194, 274)
        return (bytes(range(1, 33)) + cfg + bytes([1 if active else 0]) + struct.pack("<I", len(store)) + store
                + struct.pack("<Q", 673) + bytes(range(1, 33)) + bytes(range(2, 34)))

    def instance_rpc(self, state):
        """A stand-in network serving one instance: its /programs.json, and the instance's state at its account."""
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        program, account = chain["program_account"], lez.b58encode(bytes(range(65, 97)))
        files = {"/programs.json": {"programs": {"ferrata_attestation": {
            "program_account": program, "image_id": "0x" + runrecord.ATTESTATION_IMAGE}}}}
        return StandInRpc({}, files, {(account, program): state}), account

    def read_instance(self, state, *extra):
        """`lez-read instance ACCOUNT [--alarms ...] --json` against a stand-in network serving `state`."""
        rpc, account = self.instance_rpc(state)
        try:
            p = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/lez-read.py"), "--lez", rpc.url,
                                "--json", "instance", account, *extra], capture_output=True, text=True, timeout=120)
        finally:
            rpc.close()
        return p.returncode, (json.loads(p.stdout) if p.stdout.strip() else None), p.stderr

    def slot_of(self, seconds_ago, genesis_time=GENESIS):
        """The slot whose time is `seconds_ago` before now, as the wall clock counts it."""
        return (int(time.time()) - seconds_ago - genesis_time) // 12

    def test_instance(self):
        from verifylib import lc
        slot = self.slot_of(6 * 3600)
        rc, out, err = self.read_instance(self.instance_state(slot))
        self.assertEqual(rc, 0, err)
        self.assertEqual(out["chain_id"], 11155111)
        self.assertTrue(out["active"])
        self.assertTrue(out["next_committee"])
        self.assertEqual(out["finalized_slot"], slot)
        self.assertEqual(out["ws_period_epochs"], self.WS_EPOCHS)
        header = (slot.to_bytes(8, "little") + (1).to_bytes(8, "little")
                  + bytes.fromhex("02" * 32) + bytes.fromhex("03" * 32) + bytes.fromhex("04" * 32))
        self.assertEqual(out["finalized_root"], "0x" + lc.beacon_root(lc._beacon(header)).hex())

    def test_no_alarms_when_healthy(self):
        """An instance an epoch behind the wall clock: the one alarm is the ok one, and the exit is 0."""
        rc, out, err = self.read_instance(self.instance_state(self.slot_of(20 * 60)), "--alarms")
        self.assertEqual(rc, 0, (err, out))
        self.assertEqual([a["level"] for a in out["alarms"]], ["ok"], out["alarms"])
        self.assertEqual(out["alarms"][0]["lag_epochs"], 1, out["alarms"])

    def test_lag_warns(self):
        """Twelve epochs behind: a warning, and the exit is still 0 (behind, not wrong)."""
        rc, out, err = self.read_instance(self.instance_state(self.slot_of((12 * 32 + 64) * 12 + 60)), "--alarms")
        self.assertEqual(rc, 0, (err, out))
        self.assertTrue(any(a["level"] == "warn" and "12 epochs behind" in a["text"] for a in out["alarms"]),
                        out["alarms"])

    def test_lag_alarms(self):
        """A hundred epochs behind: a bad alarm, and the exit is 1."""
        rc, out, err = self.read_instance(self.instance_state(self.slot_of((100 * 32 + 64) * 12 + 60)), "--alarms")
        self.assertEqual(rc, 1, (err, out))
        self.assertTrue(any(a["level"] == "bad" and "100 epochs behind" in a["text"] for a in out["alarms"]),
                        out["alarms"])

    def test_lag_at_the_warn_threshold_exactly(self):
        """Four epochs behind is the warning's exact threshold: warn (and three epochs: ok, five: warn), with the
        lag displayed (review round 3, M7: a `>` for the `>=` made only the exact case wrong)."""
        for epochs, level, rc in ((3, "ok", 0), (4, "warn", 0), (5, "warn", 0)):
            with self.subTest(epochs=epochs):
                rc_, out, err = self.read_instance(
                    self.instance_state(self.slot_of((epochs * 32 + 64) * 12 + 60)), "--alarms")
                self.assertEqual(rc_, rc, (err, out))
                self.assertEqual([a["level"] for a in out["alarms"]], [level], out["alarms"])
                self.assertEqual(out["alarms"][0]["lag_epochs"], epochs, out["alarms"])
                if level == "warn":
                    self.assertIn(f"{epochs} epochs behind", out["alarms"][0]["text"], out["alarms"])

    def test_lag_at_the_bad_threshold_exactly(self):
        """Sixty-four epochs behind is the bad alarm's exact threshold: bad, exit 1 (and sixty-three: warn)."""
        for epochs, level, rc in ((63, "warn", 0), (64, "bad", 1), (65, "bad", 1)):
            with self.subTest(epochs=epochs):
                rc_, out, err = self.read_instance(
                    self.instance_state(self.slot_of((epochs * 32 + 64) * 12 + 60)), "--alarms")
                self.assertEqual(rc_, rc, (err, out))
                self.assertTrue(any(a["level"] == level and f"{epochs} epochs behind" in a["text"]
                                    for a in out["alarms"]), out["alarms"])
                self.assertEqual(out["alarms"][0]["lag_epochs"], epochs, out["alarms"])

    def test_a_negative_lag_truncates_toward_zero(self):
        """A checkpoint behind the header: the lag is negative, and divides as the page's BigInt does, truncating
        toward zero (review round 3, M7: floor division showed -2 for -33/32, not -1)."""
        slot = self.slot_of(20 * 60)
        for behind, shown in ((1, 0), (31, 0), (32, -1), (33, -1)):
            with self.subTest(behind=behind):
                rc, out, err = self.read_instance(self.instance_state(slot), "--alarms",
                                                  "--finalized-slot", str(slot - behind))
                self.assertEqual(rc, 0, (err, out))
                self.assertEqual([a["level"] for a in out["alarms"]], ["ok"], out["alarms"])
                self.assertEqual(out["alarms"][0]["lag_epochs"], shown, out["alarms"])
                self.assertEqual(out["alarms"][0]["lag_slots"], -behind, out["alarms"])

    def test_lag_by_a_given_finalized_slot(self):
        """With --finalized-slot the lag is judged by Ethereum's checkpoint, as the page judges it by the APIs'."""
        slot = self.slot_of(4000)
        rc, out, err = self.read_instance(self.instance_state(slot), "--alarms", "--finalized-slot", str(slot + 32))
        self.assertEqual(rc, 0, (err, out))
        self.assertEqual([a["level"] for a in out["alarms"]], ["ok"], out["alarms"])
        rc, out, err = self.read_instance(self.instance_state(slot), "--alarms",
                                          "--finalized-slot", str(slot + 32 * 100))
        self.assertEqual(rc, 1, (err, out))
        self.assertTrue(any("100 epochs behind" in a["text"] for a in out["alarms"]), out["alarms"])

    def test_a_stale_finalized_slot_warns(self):
        """A checkpoint far behind the wall clock: the page's warning that Ethereum's finality looks stalled."""
        rc, out, err = self.read_instance(self.instance_state(self.slot_of(20 * 60)), "--alarms",
                                          "--finalized-slot", str(self.slot_of(32 * 200 * 12)))
        self.assertEqual(rc, 0, (err, out))
        self.assertTrue(any(a["level"] == "warn" and "200 epochs behind the wall clock" in a["text"]
                             for a in out["alarms"]), out["alarms"])

    def test_an_unknown_next_committee_near_the_period_end(self):
        """The next committee unknown with the period ending in minutes: a warning; past the end: a bad alarm."""
        period_end = ((self.slot_of(0) // 8192) + 1) * 8192
        # a genesis that puts the period ending now + 6 min (the header 64 slots inside it, so its own lag is none)
        genesis = int(time.time()) + 360 - period_end * 12
        slot = period_end - 64
        state = self.instance_state(slot, next_committee=False, genesis_time=genesis)
        rc, out, err = self.read_instance(state, "--alarms")
        self.assertEqual(rc, 0, (err, out))
        self.assertTrue(any(a["level"] == "warn" and "next sync committee is unknown and the period ends" in a["text"]
                            for a in out["alarms"]), out["alarms"])
        past = self.instance_state(slot, next_committee=False, genesis_time=genesis - 7 * 60)
        rc, out, err = self.read_instance(past, "--alarms")
        self.assertEqual(rc, 1, (err, out))
        self.assertTrue(any(a["level"] == "bad" and "period has ended" in a["text"] for a in out["alarms"]),
                        out["alarms"])

    def test_a_known_next_committee_near_the_period_end(self):
        """The same instance with the next committee known: no period alarm at all."""
        period_end = ((self.slot_of(0) // 8192) + 1) * 8192
        genesis = int(time.time()) + 360 - period_end * 12
        rc, out, err = self.read_instance(self.instance_state(period_end - 64, genesis_time=genesis), "--alarms")
        self.assertEqual(rc, 0, (err, out))
        self.assertEqual([a["level"] for a in out["alarms"]], ["ok"], out["alarms"])

    def test_the_period_end_boundary_exactly(self):
        """The next committee unknown with the period ending exactly now: still a warning, not the bad alarm
        (the page warns at `hours < 12`, alarms at `hours < 0`); one slot later: bad."""
        import datetime
        import importlib.util
        spec = importlib.util.spec_from_file_location("lez_read", ROOT / "scripts/lez-read.py")
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
        cfg = {"slots_per_epoch": 32, "epochs_per_sync_committee_period": 256, "genesis_time": 1655733600,
               "seconds_per_slot": 12, "ws_period_epochs": 3532}
        period_end_ms = (cfg["genesis_time"] + 8192 * 12) * 1000  # the end of genesis period 0
        for at, level in ((period_end_ms - 60_000, "warn"), (period_end_ms, "warn"), (period_end_ms + 1, "bad")):
            with self.subTest(at=at):
                out = m.alarms(cfg, 0, active=True, next_committee=False, now_ms=at)
                self.assertTrue(any(a["level"] == level and "next sync committee is unknown" in a["text"]
                                    for a in out), out)
        # the exact boundary through the tool as well: the period ends within the half-minute, so the warning
        # still reads "0.0 h" (a second earlier or later is the same verdict, as the direct calls above show)
        period_end = ((self.slot_of(0) // 8192) + 1) * 8192
        genesis = int(time.time()) + 30 - period_end * 12  # the period ends in half a minute
        state = self.instance_state(period_end - 64, next_committee=False, genesis_time=genesis)
        rc, out, err = self.read_instance(state, "--alarms")
        self.assertEqual(rc, 0, (err, out))
        self.assertTrue(any(a["level"] == "warn" and "period ends in 0.0 h" in a["text"] for a in out["alarms"]),
                        out["alarms"])

    def test_a_pre_genesis_slot_ends_its_period_as_bigint_divides(self):
        """A pre-genesis slot (the library's arithmetic, not a chain record: the tool decodes the real slot as
        unsigned): the page's BigInt division truncates toward zero, so slot -1's period ends at slot 8192, in
        27.3 h, not at genesis, in 0.0 h (review round 3, L9)."""
        import datetime
        import importlib.util
        spec = importlib.util.spec_from_file_location("lez_read", ROOT / "scripts/lez-read.py")
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
        cfg = {"slots_per_epoch": 32, "epochs_per_sync_committee_period": 256, "genesis_time": 0,
               "seconds_per_slot": 12, "ws_period_epochs": 3532}
        out = m.alarms(cfg, -1, active=True, next_committee=False, now_ms=0)
        self.assertEqual([a["level"] for a in out], ["ok"], out)  # JavaScript: a single ok, periodEndMs 98304000
        self.assertEqual(out[0]["period_end_utc"], "1970-01-02T03:18:24Z", out)  # 98,304,000 ms after genesis

    def test_weak_subjectivity_warns(self):
        """The weak-subjectivity period ending in two hours: a warning (the same header is a whole period old, so
        the lag alarms beside it, as the page shows both)."""
        slot = self.slot_of((self.WS_EPOCHS * 32 - 600) * 12)
        rc, out, err = self.read_instance(self.instance_state(slot), "--alarms")
        self.assertEqual(rc, 1, (err, out))
        self.assertTrue(any(a["level"] == "warn" and "weak-subjectivity period of the finalized header ends in"
                            in a["text"] for a in out["alarms"]), out["alarms"])

    def test_weak_subjectivity_alarms(self):
        """A finalized header older than the weak-subjectivity period: a bad alarm, and the exit is 1."""
        slot = self.slot_of((self.WS_EPOCHS * 32 + 100) * 12)
        rc, out, err = self.read_instance(self.instance_state(slot), "--alarms")
        self.assertEqual(rc, 1, (err, out))
        self.assertTrue(any(a["level"] == "bad" and "older than the weak-subjectivity period" in a["text"]
                             for a in out["alarms"]), out["alarms"])

    def test_an_inactive_instance_warns(self):
        rc, out, err = self.read_instance(self.instance_state(self.slot_of(20 * 60), active=False), "--alarms")
        self.assertEqual(rc, 0, (err, out))
        self.assertTrue(any(a["level"] == "warn" and "Not yet active" in a["text"] for a in out["alarms"]),
                        out["alarms"])

    def test_an_account_with_no_state(self):
        """The page's `missing` alarm is this tool's read failing: no state, exit 1."""
        rpc, account = self.instance_rpc(b"")
        try:
            rpc.accounts.pop((account, json.loads((ROOT / RR / "chain.json").read_text())["program_account"]))
            p = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/lez-read.py"), "--lez", rpc.url, "--json",
                                "instance", account, "--alarms"], capture_output=True, text=True, timeout=120)
        finally:
            rpc.close()
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("holds no InstanceState", p.stderr)

    def test_the_page_rules_on_the_page_tests_own_cases(self):
        """The Python rules give the JS rules' verdicts on the status page's own test cases
        (deploy/testnet/web/explorer/test/alarms.test.mjs, read as data; Node is not run): the same configuration,
        the same fixed now, the same instances and checkpoints. Not ported: the cases whose rules need the public
        APIs a browser calls per provider (a beacon API's root at our slot, an execution RPC's block for the
        record) and the missing-record warning, which this terminal does not compute."""
        import datetime
        import importlib.util
        spec = importlib.util.spec_from_file_location("lez_read", ROOT / "scripts/lez-read.py")
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
        cfg = {"slots_per_epoch": 32, "epochs_per_sync_committee_period": 256, "genesis_time": 1655733600,
               "seconds_per_slot": 12, "ws_period_epochs": 3532}
        now = int(datetime.datetime(2026, 11, 23, 12, 0, 0, tzinfo=datetime.timezone.utc)
                  .timestamp() * 1000)  # Date.UTC(2026, 10, 23, 12, 0, 0)
        now_slot = (now // 1000 - cfg["genesis_time"]) // cfg["seconds_per_slot"]

        def alarms_of(behind=96, next_=True, active=True, ref=None, slot=None, at=now):
            return m.alarms(cfg, now_slot - behind if slot is None else slot, active=active, next_committee=next_,
                            now_ms=at, network_finalized=ref)

        def has(a, level, pattern):
            return any(x["level"] == level and re.search(pattern, x["text"]) for x in a)

        self.assertEqual(",".join(x["level"] for x in alarms_of()), "ok",
                         "an instance 1 epoch behind Ethereum's finality: healthy")
        self.assertTrue(all(x["level"] != "bad" for x in alarms_of(ref=now_slot - 64 + 200)),
                        "a provider ahead of the wall clock is no alarm")
        self.assertTrue(has(alarms_of(behind=64 + 32 * 10, ref=now_slot - 64), "warn", r"10 epochs behind"),
                        "10 epochs behind: warning")
        self.assertTrue(has(alarms_of(behind=64 + 32 * 100, ref=now_slot - 64), "bad", r"100 epochs behind"),
                        "100 epochs behind: alarm")
        # the exact thresholds by a supplied checkpoint (review round 3, M7: a `>` for the `>=` escaped every
        # case above; only the exact boundary tells them apart), and the displayed lag either side of each
        self.assertTrue(has(alarms_of(behind=64 + 32 * 3, ref=now_slot - 64), "ok", r"Following Ethereum"),
                        "exactly three epochs behind: no alarm")
        four = alarms_of(behind=64 + 32 * 4, ref=now_slot - 64)
        self.assertTrue(has(four, "warn", r"4 epochs behind") and all(x["level"] != "bad" for x in four),
                        "exactly four epochs behind: warning, the threshold itself")
        self.assertEqual(four[0]["lag_epochs"], 4, four)
        sixty3 = alarms_of(behind=64 + 32 * 63, ref=now_slot - 64)
        self.assertTrue(has(sixty3, "warn", r"63 epochs behind") and all(x["level"] != "bad" for x in sixty3),
                        "exactly sixty-three epochs behind: warning")
        self.assertEqual(sixty3[0]["lag_epochs"], 63, sixty3)
        sixty4 = alarms_of(behind=64 + 32 * 64, ref=now_slot - 64)
        self.assertTrue(has(sixty4, "bad", r"64 epochs behind"), "exactly sixty-four epochs behind: alarm")
        self.assertEqual(sixty4[0]["lag_epochs"], 64, sixty4)
        no_provider = alarms_of(ref=None)
        self.assertFalse(has(no_provider, "bad", r"epochs behind"),
                        "no checkpoint: the lag is the wall clock's, a warning at most")
        self.assertTrue(has(alarms_of(ref=now_slot - 32 * 200), "warn", r"200 epochs behind the wall clock"),
                        "a checkpoint 200 epochs behind the wall clock: warning")
        period_end = ((now_slot // 8192) + 1) * 8192
        period_end_ms = (cfg["genesis_time"] + period_end * 12) * 1000
        near = alarms_of(next_=False, slot=period_end - 1 - 32, ref=period_end - 1,
                         at=period_end_ms - 6.4 * 60000)
        self.assertTrue(has(near, "warn", r"next sync committee is unknown and the period ends"),
                        "next committee unknown, period ends soon: warning")
        self.assertTrue(has(alarms_of(next_=False, slot=period_end - 1 - 32, ref=period_end - 1,
                                       at=period_end_ms + 60000), "bad", r"period has ended"),
                        "next committee unknown after the period end: alarm")
        self.assertTrue(has(alarms_of(behind=3532 * 32 + 100, ref=now_slot - 64), "bad",
                            r"older than the weak-subjectivity"),
                        "header older than the weak-subjectivity period: alarm")
        self.assertTrue(has(alarms_of(behind=3532 * 32 - 600, ref=now_slot - 64), "warn",
                            r"weak-subjectivity period of the finalized header ends in"),
                        "weak-subjectivity end within a day: warning")
        self.assertTrue(has(alarms_of(active=False), "warn", r"Not yet active"), "inactive instance: warning")


class Records(TamperCase):
    def test_perf_tracking(self):
        self.assertPasses("perf-tracking")
        text = (ROOT / "lez/results/perf-tracking.md").read_text()
        num = re.search(r"\d{1,3},\d{3},\d{3}", text).group(0)
        t = overlay({"lez/results/perf-tracking.md": sub(num, num[:-1] + str((int(num[-1]) + 1) % 10))})
        self.assertFails("perf-tracking", t)

    def test_perf_projections(self):
        """The generator is copied into the copy (an edit leaving it as is), so it reads the copy's records."""
        self.assertPasses("perf-projections")
        gen = {"bench/perf/projections.py": lambda b: b}
        t = overlay({**gen, "lez/results/perf-state.md": sub("| header(id, slot) record | 365 | 421 | 453 |",
                                                             "| header(id, slot) record | 365 | 421 | 454 |")})
        self.assertFails("perf-projections", t, "not what bench/perf/projections.py derives")
        t = overlay({**gen, "lez/results/perf-projections.md": sub("| 9,087 | 4.12 MB | 20.58 MB |",
                                                                   "| 9,087 | 4.12 MB | 20.68 MB |")})
        self.assertFails("perf-projections", t, "not what bench/perf/projections.py derives")
        t = overlay({**gen, "docs/requirements-status.md": sub("(4.12 MB and 20.58 MB)", "(4.12 MB and 20.68 MB)")})
        self.assertFails("perf-projections", t, "no longer says")
        t = overlay({**gen, "docs/requirements-status.md": sub("one slot per release and per new depositor",
                                                               "eight slots per release and per new depositor")})
        self.assertFails("perf-projections", t, "no longer says")  # review round 2, M5

    def test_rc3_roundtrip_record(self):
        self.assertPasses("rc3-roundtrip-record")
        rt = "e2e/runs/rc3-roundtrip-1/run"
        t = overlay({f"{rt}/steps.tsv": sub("burn\tok\t", "burn\tFAILED\t")})
        self.assertFails("rc3-roundtrip-record", t)
        t = overlay({f"{rt}/facts.json": sub('"0xcad2ae02"', '"0xcad2ae03"')})
        self.assertFails("rc3-roundtrip-record", t)
        t = overlay({f"{rt}/facts.json": sub('"burnRootAccepted": true', '"burnRootAccepted": false')})
        self.assertFails("rc3-roundtrip-record", t)  # review round 2, a surviving mutant

    def test_lab_roots(self):
        self.assertPasses("lab-roots")
        f = next((ROOT / "lez/lab/results/sessions").glob("*/fetch-sepolia.json"))
        rel = str(f.relative_to(ROOT))
        root = json.loads(f.read_text())["second_provider_finalized_root"]
        bad = root[:-1] + ("0" if root[-1] != "0" else "1")
        t = overlay({rel: sub(root, bad)})
        self.assertFails("lab-roots", t, "differs")

    def test_m0(self):
        self.assertPasses("m0")
        t = overlay({"lez/results/v0.4.0-rc3/gas-drop/summary.json": sub('"passed": true', '"passed": false')})
        self.assertFails("m0", t)


class Seals(TamperCase):
    """verifylib/groth16.py on recorded seals, and the seal checks on altered evidence."""

    @classmethod
    def setUpClass(cls):
        from verifylib import checks_seals, groth16
        cls.g = groth16
        cls.live = checks_seals._live_proofs(ROOT)

    def test_a_live_proof_verifies_and_alterations_fail(self):
        e = self.live[1]  # the release proof of the test network's live round trip (3.1.0)
        self.assertEqual(self.g.verify_seal(e["seal"], e["image"], e["digest"])[:2], (True, "3.1.0"))
        bad = bytearray(e["seal"])
        bad[100] ^= 1
        self.assertFalse(self.g.verify_seal(bytes(bad), e["image"], e["digest"])[0], "a seal byte")
        self.assertFalse(self.g.verify_seal(e["seal"], e["image"][:-1] + b"\x00", e["digest"])[0], "the image id")
        self.assertFalse(self.g.verify_seal(e["seal"], e["image"], bytes(32))[0], "the journal")
        other = bytes.fromhex(self.g.PARAMS["3.0.0"]["selector"]) + e["seal"][4:]
        self.assertFalse(self.g.verify_seal(other, e["image"], e["digest"])[0], "the other release's selector")
        self.assertFalse(self.g.verify_seal(e["seal"][:-1], e["image"], e["digest"])[0], "a short seal")
        big = e["seal"][:4] + self.g.P.to_bytes(32, "big") + e["seal"][36:]
        self.assertFalse(self.g.verify_seal(big, e["image"], e["digest"])[0], "a coordinate not below p")

    def test_points_the_contract_refuses(self):
        """Infinity, negated, swapped and mis-ordered points, another image, trailing bytes: all refused."""
        e = self.live[1]
        s, P = e["seal"], self.g.P
        w = [int.from_bytes(s[4 + 32 * i: 36 + 32 * i], "big") for i in range(8)]

        def mk(ws):
            return s[:4] + b"".join(v.to_bytes(32, "big") for v in ws)
        cases = {"A at infinity": mk([0, 0] + w[2:]), "C at infinity": mk(w[:6] + [0, 0]),
                 "B at infinity": mk(w[:2] + [0] * 4 + w[6:]), "A and C swapped": mk(w[6:8] + w[2:6] + w[0:2]),
                 "A negated": mk([w[0], (P - w[1]) % P] + w[2:]), "C negated": mk(w[:6] + [w[6], (P - w[7]) % P]),
                 "B's x real and imaginary swapped": mk(w[:2] + [w[3], w[2], w[4], w[5]] + w[6:]),
                 "a trailing byte (the contract's abi.decode would ignore it; refused here)": s + b"\0"}
        for name, seal in cases.items():
            with self.subTest(case=name):
                self.assertFalse(self.g.verify_seal(seal, e["image"], e["digest"])[0])
        other = next(x for x in self.live if x["image"] != e["image"])
        self.assertFalse(self.g.verify_seal(s, other["image"], e["digest"])[0], "another image id")

    def test_a_twist_point_outside_the_subgroup_and_an_off_curve_point(self):
        """Each refused for its own reason, not for a later pairing failure (review round 2, surviving mutants)."""
        g, P = self.g, self.g.P

        def sqrt2(a):  # a square root in Fp2 = Fp[u]/(u^2 + 1), p = 3 mod 4 (Adj, Rodriguez-Henriquez), or None
            a1 = g.f2_pow(a, (P - 3) // 4)
            alpha = g.f2_mul(g.f2_mul(a1, a1), a)
            x0 = g.f2_mul(a1, a)
            if g.f2_mul(g.f2_pow(alpha, P), alpha) == ((P - 1) % P, 0):
                return None
            if alpha == ((P - 1) % P, 0):
                return g.f2_mul((0, 1), x0)
            return g.f2_mul(g.f2_pow(g.f2_add((1, 0), alpha), (P - 1) // 2), x0)
        pt = None
        for k in range(1, 200):
            x = (k, 1)
            y = sqrt2(g.f2_add(g.f2_mul(g.f2_mul(x, x), x), g.B_TWIST))
            if y is not None and g.f2_mul(y, y) == g.f2_add(g.f2_mul(g.f2_mul(x, x), x), g.B_TWIST):
                pt = (x, y)
                break
        self.assertIsNotNone(pt)
        self.assertTrue(g.on_twist(pt))
        self.assertIsNotNone(g.g2_mul(pt, g.R), "the point found is in the subgroup: pick another")
        (xr, xi), (yr, yi) = pt
        e = self.live[1]
        s = e["seal"]
        w = [int.from_bytes(s[4 + 32 * i: 36 + 32 * i], "big") for i in range(8)]
        bad_b = s[:4] + b"".join(v.to_bytes(32, "big") for v in w[:2] + [xi, xr, yi, yr] + w[6:])
        ok, _, why = g.verify_seal(bad_b, e["image"], e["digest"])
        self.assertFalse(ok)
        self.assertIn("not in the order-r subgroup", why)
        off = s[:4] + b"".join(v.to_bytes(32, "big") for v in [w[0], (w[1] + 1) % P] + w[2:])
        ok, _, why = g.verify_seal(off, e["image"], e["digest"])
        self.assertFalse(ok)
        self.assertIn("not on the curve", why)

    def test_the_check_fails_on_an_altered_proof(self):
        self.assertPasses("seals-v3.1")
        from verifylib.checks_seals import LIVE_PROOFS
        seal = json.loads((ROOT / LIVE_PROOFS).read_text())["release"]["seal"]
        flipped = seal[:-1] + ("0" if seal[-1] != "0" else "1")
        t = overlay({LIVE_PROOFS: sub(seal, flipped)})
        self.assertFails("seals-v3.1", t, "does not verify")


class Signatures(TamperCase):
    """verifylib/sigs.py on RFC 8032's vectors, and the release-sums check on altered evidence."""
    DL = "deploy/testnet/web/basecamp/dl"

    def test_each_refusal_for_its_reason(self):
        """SSHSIG's namespace and signer binding, canonical point decoding, minisign's global signature: each refused
        by its own condition (review round 2, surviving mutants)."""
        from verifylib import sigs
        dl = ROOT / self.DL
        key = sigs.ssh_ed25519_key((dl / "keys/ferrata-release.pub").read_text())
        sums = (dl / "SHA256SUMS").read_bytes()
        arm = (dl / "SHA256SUMS.sig").read_text()
        ok, why = sigs.sshsig_verify(arm, sums, key, "git")
        self.assertEqual((ok, why.split(" ")[0]), (False, "namespace"))
        other = bytes([key[0] ^ 1]) + key[1:]
        self.assertEqual(sigs.sshsig_verify(arm, sums, other, "file"), (False, "signed by another key"))
        self.assertIsNone(sigs._decode(sigs.Q.to_bytes(32, "little")), "y = q is not canonical")
        self.assertIsNone(sigs._decode((sigs.Q + 1).to_bytes(32, "little")), "y = q + 1 (one) is not canonical")
        self.assertIsNone(sigs._decode((1 | 1 << 255).to_bytes(32, "little")), "x = 0 with the sign bit set")
        ms = (dl / "SHA256SUMS.minisig").read_text().strip().splitlines()
        tampered = "\n".join([ms[0], ms[1], ms[2] + " (edited)", ms[3]]) + "\n"
        ok, why = sigs.minisign_verify(tampered, sums, (dl / "keys/minisign.pub").read_text())
        self.assertFalse(ok)
        self.assertIn("global signature", why)

    def test_small_order_keys_refused(self):
        """Under a small-order public key a signature [s]B || s verifies for any message (cofactorless check): it is
        refused, as ed25519-dalek's verify_strict refuses it (review round 2 of the integration, L1)."""
        from verifylib import sigs
        identity = bytes([1]) + bytes(31)
        s = 12345
        forged = sigs._encode(sigs._mul(s, sigs._BASE)) + s.to_bytes(32, "little")
        for key in (identity, (sigs.Q - 1).to_bytes(32, "little")):  # the identity; the point of order 2, y = -1
            self.assertIsNotNone(sigs._decode(key))
            for message in (b"", b"any message", b"another"):
                self.assertFalse(sigs.ed25519_verify(key, message, forged), (key.hex(), message))
        # a small-order R under a real key: refused
        self.assertFalse(sigs.ed25519_verify(sigs._encode(sigs._mul(7, sigs._BASE)), b"m", identity + bytes(32)))

    def test_rfc8032_vectors(self):
        from verifylib import sigs
        pub = bytes.fromhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
        sig = bytes.fromhex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e3970"
                            "1cf9b46bd25bf5f0595bbe24655141438e7a100b")
        self.assertTrue(sigs.ed25519_verify(pub, b"", sig))
        self.assertFalse(sigs.ed25519_verify(pub, b"\x00", sig))
        high_s = sig[:32] + (int.from_bytes(sig[32:], "little") + sigs.L).to_bytes(32, "little")
        self.assertFalse(sigs.ed25519_verify(pub, b"", high_s), "S not below L is refused (malleability)")

    def test_unaltered(self):
        self.assertPasses("basecamp-sums-signed")

    def test_sums_altered(self):
        line = (ROOT / self.DL / "SHA256SUMS").read_text().splitlines()[0]
        t = overlay({f"{self.DL}/SHA256SUMS": sub(line[:8], line[:7] + ("0" if line[7] != "0" else "1"))})
        self.assertFails("basecamp-sums-signed", t, "does not verify")

    def test_signature_altered(self):
        text = (ROOT / self.DL / "SHA256SUMS.minisig").read_text().splitlines()[1]
        t = overlay({f"{self.DL}/SHA256SUMS.minisig": sub(text[20:24], text[20:24][::-1] if text[20:24] != text[20:24][::-1] else "AAAA")})
        self.assertFails("basecamp-sums-signed", t, "minisig")

    def test_another_key(self):
        other = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINdamAGCsQq31Uv+08lkBzoO4XLz2qYjJa8CGmj3B1Ea"  # RFC 8032 test 1's key
        line = next(ln for ln in (ROOT / self.DL / "keys/allowed_signers").read_text().splitlines() if "ssh-ed25519" in ln)
        new = line.split("ssh-ed25519")[0] + other
        t = overlay({f"{self.DL}/keys/allowed_signers": sub(line, new)})
        self.assertFails("basecamp-sums-signed", t)


class Hashes(TamperCase):
    def test_site_manifest(self):
        self.assertPasses("site-manifest-git")
        m = json.loads((ROOT / "deploy/testnet/web/trust/bridge/verify/manifest.json").read_text())
        h = m["files"][3]["sha256"]
        t = overlay({"deploy/testnet/web/trust/bridge/verify/manifest.json": sub(h, h[::-1])})
        self.assertFails("site-manifest-git", t, m["files"][3]["path"])

    def test_image_ids(self):
        self.assertPasses("image-ids-agree")
        dep = json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text())
        img = dep["programs"][0]["image_id"].lower()
        t = overlay({"docs/image-ids.md": sub(img[2:], img[2:][::-1], count=100)})
        self.assertFails("image-ids-agree", t, dep["programs"][0]["name"])

    def test_acc_leaf(self):
        self.assertPasses("acc-leaf-vectors")
        text = (ROOT / "vectors/acc-leaf.json").read_text()
        h = next(m.group(0) for m in re.finditer(r"[0-9a-f]{64}", text) if len(set(m.group(0))) > 4)
        t = overlay({"vectors/acc-leaf.json": sub(h, h[:-1] + ("0" if h[-1] != "0" else "1"))})
        self.assertFails("acc-leaf-vectors", t)


if __name__ == "__main__":
    unittest.main()
