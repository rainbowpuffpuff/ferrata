"""Tests of scripts/check-decisions.py: `python3 -I scripts/test_check_decisions.py`.

Standard library only; nothing is written to disk. The records, the index and the files they cite are built here in
memory; the last test runs the check on the repository's own log.
"""

import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("check_decisions", os.path.join(HERE, "check-decisions.py"))
cd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cd)

SPEC = "# changes\n\n## D1. §1 Something\n\ntext\n\n## D2. §2 Something else\n\ntext\n"
FILES = {
    "docs/design/note.md": "# Note\n\n## 4. Paths\n\n### 4.2 Two paths\n\nThe proven path needs a prover; the public job "
                           "does not.\n",
    "docs/design/spec/bridge-spec-v3-changes.md": SPEC,
    "src/core.rs": "pub fn evaluate() {}\n",
    "vault/test/EmergencyStop.t.sol": "contract EmergencyStopTest {}\n",
}

GOOD = """# ADR 0001: One core for every path

- **Date:** 10 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 F.1, S.4

## Context

The same verdict must come from the program, the consumer's proof, the tool and the module, with the same codes.

## Options considered

1. Only a proven path, with a prover for every update. *Recorded:* docs/design/note.md §4.2 ("The proven path needs a
   prover").
2. A settable configuration that an admin may change. *Ruled out:* RFP-021 F.13 (no instruction rewrites it).
3. Two implementations, one for the program and one for the tools. *Reconstructed for contrast* (not recorded).
4. One core, `evaluate`, called by every path.

## Decision

The fourth.

## Rationale and trade-offs

Verdicts and codes cannot differ between the paths, which makes dry runs meaningful; the cost is one crate to audit.

## Sources

- docs/design/note.md §4
- src/core.rs (`evaluate`)
- docs/design/spec/bridge-spec-v3-changes.md D1, D2
- EVIDENCE row 7
"""

INDEX = """# Log

| ADR | Decision | Date | Status | Requirements |
|---|---|---|---|---|
| [0001](0001-one-core.md) | One core for every path | 10 November 2026 | accepted; built | RFP-021 F.1, S.4 |
"""


def repo(records, readme=INDEX, files=None):
    files = dict(FILES if files is None else files)
    ids = {"RFP-021": {"F.1", "F.13", "S.4"}, "RFP-022": {"Sec.9"}}
    return cd.Repo(records, readme, set(files), ids, {7}, SPEC, lambda p: files.get(p, ""))


def findings(records, **kw):
    return cd.check(repo(records, **kw))[0]


class Records(unittest.TestCase):
    def test_a_complete_record_passes(self):
        self.assertEqual(findings({"0001-one-core.md": GOOD}), [])

    def test_an_empty_record(self):
        out = findings({"0001-one-core.md": "# ADR 0001: One core for every path\n"})
        for what in ("no '- **Date:**' line", "no '- **Status:**' line", "no '- **Requirements:**' line",
                     "no '## Context' section", "no '## Options considered' section", "no '## Sources' section",
                     "0 option(s) considered"):
            self.assertTrue(any(what in f for f in out), (what, out))

    def test_the_reviews_probe_record(self):
        # A record an internal review wrote to show that the first version of the check passed anything.
        probe = """# ADR 0001: Probe

- **Date:** someday
- **Status:** accepted; built (branch nowhere), deployed on the moon
- **Requirements:** RFP-021 F.1; Security #99, trust

## Context

x

## Options considered

1. a
2. b

## Decision

b

## Rationale and trade-offs

D1 D2 D3 D4 D5 D6 D7 D8 D9 D10 D11 D12 D13

## Sources

- README.md
- branch nowhere, commit deadbeef: docs/not/there.rs
"""
        index = INDEX.replace("| One core for every path | 10 November 2026 | accepted; built | RFP-021 F.1, S.4 |",
                              "| Something else | 1 January 2020 | accepted; built (branch nowhere), deployed on the moon "
                              "| RFP-021 F.2 |").replace("0001-one-core.md", "0001-probe.md")
        out = findings({"0001-probe.md": probe}, readme=index)
        for what in ("names no day, month and year", "status cites a branch", "'Security' is not a requirement ID",
                     "'#99' is not a requirement ID", "'trust' is not a requirement ID", "'Context' says too little",
                     "option 1 says too little",
                     "option 1 is neither recorded", "README.md is not a tracked file", "- README.md - branch nowhere, commit deadbeef",
                     "docs/not/there.rs is not a tracked file", "title 'Something else' differs",
                     "date '1 January 2020' differs", "requirements 'RFP-021 F.2' differs",
                     "D1 is cited in no record's Sources"):  # "D1 … D13" in the rationale no longer counts
            self.assertTrue(any(what in f for f in out), (what, out))

    def test_an_option_whose_source_does_not_say_it(self):
        bad = GOOD.replace('("The proven path needs a\n   prover")', '("The proven path is free")')
        self.assertTrue(any('does not contain "The proven path is free"' in f for f in findings({"0001-one-core.md": bad})))

    def test_a_section_the_file_does_not_have(self):
        bad = GOOD.replace("docs/design/note.md §4.2", "docs/design/note.md §4.7")
        self.assertTrue(any("docs/design/note.md has no §4.7" in f for f in findings({"0001-one-core.md": bad})))

    def test_a_name_the_file_does_not_have(self):
        bad = GOOD.replace("src/core.rs (`evaluate`)", "src/core.rs (`verify_all`)")
        self.assertTrue(any("src/core.rs does not contain `verify_all`" in f for f in findings({"0001-one-core.md": bad})))

    def test_a_ruled_out_option_with_no_clause(self):
        bad = GOOD.replace("*Ruled out:* RFP-021 F.13 (no instruction rewrites it)", "*Ruled out:* by common sense")
        self.assertTrue(any("ruled out by no RFP requirement" in f for f in findings({"0001-one-core.md": bad})))

    def test_an_option_that_records_no_alternative(self):
        bad = GOOD.replace("3. Two implementations, one for the program and one for the tools. *Reconstructed for contrast* "
                           "(not recorded).", "3. The source records no alternative; the obvious one is to do nothing.")
        out = findings({"0001-one-core.md": bad})
        self.assertTrue(any("records no alternative" in f for f in out), out)
        self.assertTrue(any("option 3 is neither recorded" in f for f in out), out)

    def test_a_path_in_the_text_that_is_not_tracked(self):
        bad = GOOD.replace("the cost is one crate to audit.", "shown by `vault/test/MigrationFailsafe.t.sol`.")
        self.assertTrue(any("vault/test/MigrationFailsafe.t.sol, which is not a tracked file" in f
                            for f in findings({"0001-one-core.md": bad})))

    def test_an_evidence_row_that_does_not_exist(self):
        bad = GOOD.replace("EVIDENCE row 7", "EVIDENCE row 8")
        self.assertTrue(any("EVIDENCE row 8 does not exist" in f for f in findings({"0001-one-core.md": bad})))

    def test_a_requirement_of_the_other_rfp(self):
        bad = GOOD.replace("RFP-021 F.1, S.4", "RFP-021 Sec.9").replace("| RFP-021 F.1, S.4 |", "| RFP-021 Sec.9 |")
        self.assertTrue(any("RFP-021 Sec.9 is not a row" in f for f in findings({"0001-one-core.md": bad},
                                                                                readme=INDEX.replace("RFP-021 F.1, S.4", "RFP-021 Sec.9"))))

    def test_an_adr_that_is_not_in_the_log(self):
        # 0013 named ADR 0029 after 0029 was held back (found by an internal review).
        bad = GOOD.replace("the cost is one crate to audit.", "the cost is one crate to audit; ADR 0029 adds the rest, "
                           "and ADRs 0001 and 0002 the context.")
        out = findings({"0001-one-core.md": bad}, readme=INDEX + "\nRecords 0027–0031 wait: ADRs 0001–0003.\n")
        self.assertIn("0001-one-core.md: names ADR 0029, which is not in the log", out)
        self.assertIn("0001-one-core.md: names ADR 0002, which is not in the log", out)
        self.assertIn("README: names ADR 0002, which is not in the log", out)
        self.assertIn("README: names ADR 0003, which is not in the log", out)
        self.assertFalse(any("ADR 0001," in f for f in out), out)
        self.assertEqual(findings({"0001-one-core.md": GOOD.replace("the cost is", "ADR 0001: the cost is")}), [])

    def test_a_gap_in_the_numbers_and_an_unlisted_record(self):
        out = findings({"0001-one-core.md": GOOD, "0003-other.md": GOOD.replace("ADR 0001", "ADR 0003")})
        self.assertTrue(any("without a gap" in f for f in out), out)
        self.assertTrue(any("ADR 0003 is linked 0 times" in f for f in out), out)


class Strictness(unittest.TestCase):
    """An internal review mutated one record thirteen ways; nine passed the check then. Each of the eight that a check
    can catch (a false statement in the record's own words cannot be) fails now."""

    def flags(self, bad, what, files=None):
        out = findings({"0001-one-core.md": bad}, files=files)
        self.assertTrue(any(what in f for f in out), (what, out))

    def test_a_path_at_the_end_of_a_sentence(self):
        # The full stop is not part of the path: a tracked file passes, a missing one under an unknown directory fails.
        end = "the cost is one crate to audit."
        self.assertEqual(findings({"0001-one-core.md": GOOD.replace(end, end[:-1] + ", in src/core.rs.")}), [])
        self.flags(GOOD.replace(end, end[:-1] + ", in zzz/missing.md."), "names zzz/missing.md, which is not a tracked")

    def test_a_path_under_a_directory_the_repository_lacks(self):
        self.flags(GOOD.replace("- src/core.rs (`evaluate`)", "- src/core.rs (`evaluate`), `lib/Nope.sol`"),
                   "lib/Nope.sol is not a tracked file")
        self.flags(GOOD.replace("one crate to audit.", "one crate to audit, `lib/Nope.sol`."),
                   "names lib/Nope.sol, which is not a tracked file")

    def test_a_line_past_the_end_of_its_file(self):
        self.flags(GOOD.replace("- src/core.rs (`evaluate`)", "- src/core.rs:999999 (`evaluate`)"),
                   "src/core.rs:999999 is not within its 1 lines")
        self.flags(GOOD.replace("one crate to audit.", "one crate to audit (`core.rs:2-3`)."),
                   "src/core.rs:2-3 is not within its 1 lines")
        self.assertEqual(findings({"0001-one-core.md": GOOD.replace("- src/core.rs (", "- src/core.rs:1 (")}), [])

    def test_a_bare_file_name_no_tracked_file_has(self):
        self.flags(GOOD.replace("one crate to audit.", "one crate to audit, in `NotThere.rs`."),
                   "names NotThere.rs, which no tracked file is called")
        self.assertEqual(findings({"0001-one-core.md": GOOD.replace("one crate to audit.", "one crate, `core.rs`.")}),
                         [])

    def test_a_branch_anywhere_in_the_record(self):
        self.flags(GOOD.replace("The fourth.", "The fourth, built on branch rfp-softs and not merged yet."),
                   "names a branch")
        self.flags(GOOD.replace("one crate to audit.", "one crate to audit; on the branches it is the same."),
                   "names a branch")
        chain = GOOD.replace("one crate to audit.", "one crate to audit; the canonical branch and the finality "
                             "branches are a chain's, not git's.")
        self.assertEqual(findings({"0001-one-core.md": chain}), [])

    def test_a_link_to_a_branch_on_github(self):
        self.flags(GOOD.replace("one crate to audit.", "one crate to audit: https://github.com/o/r/tree/rfp-softs/lez."),
                   "links to GitHub at 'rfp-softs', not at a commit")
        self.assertEqual(findings({"0001-one-core.md": GOOD.replace(
            "one crate to audit.", "one crate: https://github.com/o/r/blob/0123456789abcdef0123456789abcdef01234567/x.")}),
            [])

    def test_a_section_of_a_file_that_is_not_markdown(self):
        self.flags(GOOD.replace("- src/core.rs (`evaluate`)", "- src/core.rs §99 (`evaluate`)"),
                   "a § of src/core.rs, which is not Markdown")

    def test_an_ellipsis_quote_whose_parts_are_apart_or_out_of_order(self):
        files = dict(FILES, **{"docs/design/long.md": "# Long\n\nalpha beta " + "filler " * 60 + "gamma delta.\n"})
        near = GOOD.replace('("The proven path needs a\n   prover")', '("The proven path … the public job does")')
        self.assertEqual(findings({"0001-one-core.md": near}, files=files), [])
        apart = GOOD.replace("- src/core.rs (`evaluate`)", '- docs/design/long.md ("alpha beta … gamma delta")')
        self.flags(apart, 'does not contain "alpha beta … gamma delta" (its parts in order, near)', files=files)
        joined = GOOD.replace("- src/core.rs (`evaluate`)", '- docs/design/long.md ("alpha beta" … "gamma delta")')
        self.flags(joined, "(its parts in order, near)", files=files)
        backwards = GOOD.replace('("The proven path needs a\n   prover")', '("the public job … The proven path")')
        self.flags(backwards, "(its parts in order, near)", files=files)

    def test_a_name_that_is_only_part_of_a_word(self):
        self.flags(GOOD.replace("src/core.rs (`evaluate`)", "src/core.rs (`valuate`)"),
                   "src/core.rs does not contain `valuate` (as a whole word)")


class RepositoryLog(unittest.TestCase):
    def test_the_repository_log_passes(self):
        out, n, changes = cd.check(cd.load())
        self.assertEqual(out, [])
        self.assertGreater(n, 0)
        self.assertGreater(changes, 0)


if __name__ == "__main__":
    unittest.main()
