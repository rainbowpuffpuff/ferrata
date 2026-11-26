#!/usr/bin/env python3
"""Repository-invariant tests for scripts/audit_scope.py (standard library)."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/audit_scope.py"
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("audit_scope", SCRIPT)
assert SPEC and SPEC.loader
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class AuditScopeOutputTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.output = subprocess.run(
            [sys.executable, "-I", str(SCRIPT)],
            cwd=ROOT,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout
        cls.tracked = set(AUDIT.tracked_files())
        cls.rows = {}
        for line in cls.output.splitlines():
            fields = [field.strip() for field in line.strip("|").split("|")]
            if len(fields) == 7 and fields[0].endswith((".rs", ".sol")):
                path, lang, code, comments, blank, total, description = fields
                cls.rows[path] = (lang, int(code), int(comments), int(blank), int(total))

    def test_every_listed_file_is_tracked_source(self) -> None:
        self.assertGreater(len(self.rows), 0)
        for path, (_, _, _, _, _) in self.rows.items():
            with self.subTest(path=path):
                self.assertIn(path, self.tracked)
                self.assertTrue((ROOT / path).is_file())
                self.assertNotRegex(path, r"(^|/)(test|tests|examples?|benches?)(/|\.|$)")

    def test_each_row_and_both_totals_add_up(self) -> None:
        seen_totals = 0
        for path, (_, code, comments, blank, total) in self.rows.items():
            with self.subTest(path=path):
                self.assertEqual(code + comments + blank, total)
                self.assertGreater(total, 0)
        for line in self.output.splitlines():
            if not line.startswith("| **TOTAL ("):
                continue
            seen_totals += 1
            match = re.fullmatch(
                r"\| \*\*TOTAL \((\d+) files\)\*\* \| \| \*\*(\d+)\*\* \| "
                r"\*\*(\d+)\*\* \| \*\*(\d+)\*\* \| \*\*(\d+)\*\* \| \|",
                line,
            )
            self.assertIsNotNone(match, line)
            files, code, comments, blank, total = map(int, match.groups())
            self.assertEqual(code + comments + blank, total)
            section = self.output[: self.output.index(line)]
            section = section[section.rfind("RFP-") :]
            body = [
                row for row in section.splitlines()
                if row.startswith("| ") and row.split("|")[1].strip().endswith((".rs", ".sol"))
            ]
            self.assertEqual(files, len(body))
            columns = [[int(cell.strip()) for cell in row.split("|")[3:7]] for row in body]
            self.assertEqual([code, comments, blank], [sum(col[i] for col in columns) for i in range(3)])
        self.assertEqual(seen_totals, 2)

    def test_physical_line_counts_match(self) -> None:
        for path, (lang, code, comments, blank, total) in self.rows.items():
            with self.subTest(path=path):
                self.assertEqual(AUDIT.language(path), lang)
                self.assertEqual(AUDIT.count_lines(path), (code, comments, blank))
                self.assertEqual(total, len((ROOT / path).read_text(encoding="utf-8").splitlines()))

    def test_nonproduction_and_unrelated_programs_are_excluded(self) -> None:
        excluded = {
            "crates/ferrata-core/tests/fuzz_regressions.rs",
            "vault/test/Roles.t.sol",
            "vault/src/standin/StandInCheckpointVerifier.sol",
            "lez/guests/consumer/src/main.rs",
            "bridge/guests/src/bin/claim.rs",
            "bridge/guests/src/bin/gasdrop.rs",
        }
        self.assertTrue(excluded <= self.tracked)
        self.assertTrue(excluded.isdisjoint(self.rows))

    def test_expected_audits_and_reused_versions_are_present(self) -> None:
        self.assertIn("RFP-021 AUDIT: attestation program", self.output)
        self.assertIn("RFP-022 AUDIT: contracts and bridge/checkpoint programs", self.output)
        self.assertIn("lez/guests/program/src/main.rs", self.rows)
        self.assertIn("lez/guests/bridge/src/main.rs", self.rows)
        self.assertIn("vault/src/FerrataVault.sol", self.rows)
        for pin in ("0.3.17", "v0.3.16-risczero.0", "3.0.5", "3.1.0", "v5.4.0", "v0.4.0-rc1"):
            self.assertIn(pin, self.output)
        self.assertIn("both *_0_3_5_31 guests", self.output)
        for lockfile in (
            "Cargo.lock",
            "lez/guests/program/Cargo.lock",
            "lez/guests/bridge/Cargo.lock",
            "checkpoint/guests/Cargo.lock",
            "bridge/guests/Cargo.lock",
            "vault/lib.lock",
        ):
            self.assertIn(lockfile, self.tracked)


    def test_the_document_carries_exactly_the_generated_tables(self) -> None:
        """docs/security/audit-scope.md embeds every table line the generator prints, so a source change that moves
        a count, or a lockfile that moves a version, fails here until the document is regenerated."""
        doc = (ROOT / "docs/security/audit-scope.md").read_text(encoding="utf-8").splitlines()
        missing = [line for line in self.output.splitlines() if line.startswith("|") and line not in doc]
        self.assertEqual(missing, [], "regenerate the tables of docs/security/audit-scope.md")


if __name__ == "__main__":
    unittest.main()
