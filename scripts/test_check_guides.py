"""Tests of scripts/check-guides.py's checks 2 (the mini-app's privacy table), 4 (no guide, and no index pointing at
the guides, still calls a guide a first draft) and 5 (keyless retirement as the vault counts it):
`python3 -I scripts/test_check_guides.py`.

Standard library only; nothing is written to disk. The texts are built here; the last test reads the repository.
"""

import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("check_guides", os.path.join(HERE, "check-guides.py"))
cg = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cg)


class FirstDrafts(unittest.TestCase):
    def test_a_guide_that_calls_itself_a_first_draft(self):
        out = cg.first_draft_findings({"docs/guides/user-guide.md": "# User guide\n\n**First draft, 11 November.**"})
        self.assertEqual(len(out), 1, out)

    def test_a_guide_that_says_what_it_replaces(self):
        text = "Revised for delivery on 21 November 2026; it replaces the first draft of 11 November."
        self.assertEqual(cg.first_draft_findings({"docs/guides/user-guide.md": text}), [])

    def test_the_readme_calling_the_guides_first_drafts(self):
        # The line the review found (README.md:193 before the fix), and the docs index's heading.
        old = "The guides are first drafts of 11 November 2026; every statement cites the file it rests on."
        self.assertEqual(len(cg.first_draft_findings({"README.md": old})), 1)
        heading = "## Guides, trust and performance (first drafts, 11 November 2026)"
        self.assertEqual(len(cg.first_draft_findings({"docs/README.md": heading})), 1)

    def test_a_note_calling_the_consumer_guide_a_first_draft(self):
        old = "**Not built**: the final version of the consumer guide (the first draft is `consumer-guide.md`)."
        self.assertEqual(len(cg.first_draft_findings({"docs/design/attestation.md": old})), 1)

    def test_an_index_may_say_it_of_the_performance_document(self):
        text = "The guides were revised on 21 November 2026 (`docs/performance.md` is still the first draft of 11 November)."
        self.assertEqual(cg.first_draft_findings({"README.md": text}), [])

    def test_the_repository_has_none(self):
        root = os.path.dirname(HERE)
        guides = os.path.join(root, "docs", "guides")
        paths = [os.path.join("docs", "guides", n) for n in sorted(os.listdir(guides)) if n.endswith(".md")]
        paths += list(cg.DOCS) + list(cg.POINTERS)
        texts = {}
        for p in paths:
            with open(os.path.join(root, p), encoding="utf-8") as f:
                texts[p] = f.read()
        self.assertEqual(cg.first_draft_findings(texts), [])


class MiniAppTable(unittest.TestCase):
    """Check 2's second half: the mini-app's privacy.js against cli/src/privacy.rs."""

    STEPS = {"Deposit (Ethereum)": {"public_eth": ["your depositor address", "the token"], "public_lez": [],
                                    "hidden": ["the LEZ account"]}}
    JS = ('var STEPS = {\n    deposit: {\n        title: "Deposit (Ethereum)",\n'
          '        publicEth: ["your depositor address", "the token"],\n        publicLez: [],\n'
          '        hidden: ["the LEZ account"]\n    }\n};\n')

    def test_a_word_changed_in_the_mini_app(self):
        out = cg.js_findings(self.JS.replace('"the token"', '"the amount"'), self.STEPS)
        self.assertTrue(any("public_eth differs" in f for f in out), out)

    def test_a_step_the_tool_does_not_have(self):
        out = cg.js_findings(self.JS.replace("Deposit (Ethereum)", "Deposit (LEZ)"), self.STEPS)
        self.assertTrue(any("is not a step of cli/src/privacy.rs" in f for f in out), out)

    def test_the_repository_agrees(self):
        root = os.path.dirname(HERE)
        with open(os.path.join(root, "modules/ferrata_bridge_ui/qml/js/privacy.js"), encoding="utf-8") as f:
            self.assertEqual(cg.js_findings(f.read(), cg.rs_steps()), [])


class KeylessRetirement(unittest.TestCase):
    ROOT = os.path.dirname(HERE)
    # The sentences the third review found (H3-M1), as they stood at cc6b0fa3.
    OLD_USER = ("default). Anyone may retire once no checkpoint has been finalised for 30 days plus one candidate\n"
                "  wait, time under a checkpoint freeze or a watchers' pause not counting (`retireOpensAt()` on the\n"
                "  vault).")
    OLD_TRUST = ("| freezes lapse; anyone retires after 30 days without checkpoint progress (time under a freeze does not "
                 "count); the admin replaces it in 14 days |\n| retire after 30 days without checkpoint progress (no "
                 "submission or finalisation; freezes and pauses do not count), or once a sunset checkpoint |")
    NEW_USER = ("Anyone may retire once 30 days plus one candidate wait have passed since the last\n  checkpoint progress: "
                "the deployment, the last candidate submitted, the last checkpoint finalised or\n  the end of the last "
                "checkpoint freeze or watchers' pause, whichever is latest (`retireOpensAt()`).")
    NEW_OPERATOR = ("The freeze authority, or **anyone 30 days\nplus one candidate wait after the last checkpoint progress** "
                    "(the deployment, the last candidate submitted or\ncheckpoint finalised, or the end of the last "
                    "checkpoint freeze or watchers' pause, whichever is latest:\n`retireOpensAt()`), calls `retire()`")

    def vault(self):
        with open(os.path.join(self.ROOT, cg.VAULT), encoding="utf-8") as f:
            return f.read()

    def test_the_old_guide_sentence_is_caught(self):
        out = cg.retirement_findings({"docs/guides/user-guide.md": self.OLD_USER})
        self.assertTrue(any("not stated as the vault counts it" in f for f in out), out)
        self.assertTrue(any("finalisations only" in f for f in out), out)
        self.assertTrue(any("the time before it does not count either" in f for f in out), out)

    def test_the_old_trust_cells_are_caught(self):
        out = cg.retirement_findings({"docs/trust.md": self.OLD_TRUST})
        self.assertEqual(len(out), 2, out)

    def test_the_new_wordings_pass(self):
        texts = {"docs/guides/user-guide.md": self.NEW_USER, "docs/guides/operator-guide.md": self.NEW_OPERATOR,
                 "docs/guides/deployer-guide.md": self.NEW_OPERATOR.replace("**", "")}
        self.assertEqual(cg.retirement_findings(texts), [])

    def test_a_guide_that_leaves_out_a_start_is_caught(self):
        text = self.NEW_USER.replace("the last candidate submitted, ", "")
        self.assertEqual(len(cg.retirement_findings({"docs/guides/user-guide.md": text})), 1)

    def test_d13s_text_beside_the_rule_as_built_passes(self):
        text = "permissionless once no checkpoint has finalised for 30 days (as built: 30 days plus one candidate wait"
        self.assertEqual(cg.retirement_findings({"vault/README.md": text}), [])

    def test_the_vault_still_counts_so(self):
        self.assertEqual(cg.vault_rule_findings(self.vault()), [])

    def test_a_vault_whose_count_changed_is_caught(self):
        src = self.vault()
        submit = src.index("function submitCandidate(")
        reset = src.index("keylessFrom = uint64(block.timestamp);", submit)
        no_submit = src[:reset] + src[reset:].replace("keylessFrom = uint64(block.timestamp);", "", 1)
        self.assertEqual(len(cg.vault_rule_findings(no_submit)), 1)
        no_freeze = src.replace("if (checkpointsFrozenUntil > from) from = checkpointsFrozenUntil;", "", 1)
        self.assertEqual(len(cg.vault_rule_findings(no_freeze)), 1)
        longer = src.replace("KEYLESS_RETIREMENT = 30 days;", "KEYLESS_RETIREMENT = 31 days;", 1)
        self.assertEqual(len(cg.vault_rule_findings(longer)), 1)

    def test_the_repository_agrees(self):
        paths = [os.path.join("docs/guides", n) for n in sorted(os.listdir(os.path.join(self.ROOT, "docs/guides")))
                 if n.endswith(".md")]
        paths += list(cg.DOCS) + list(cg.RETIREMENT_DOCS)
        paths += [os.path.join("docs/decisions", n) for n in sorted(os.listdir(os.path.join(self.ROOT, "docs/decisions")))
                  if n.endswith(".md")]
        texts = {}
        for p in paths:
            with open(os.path.join(self.ROOT, p), encoding="utf-8") as f:
                texts[p] = f.read()
        self.assertEqual(cg.retirement_findings(texts), [])


if __name__ == "__main__":
    unittest.main()
