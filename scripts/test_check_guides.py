"""Tests of scripts/check-guides.py's check 4 (no guide, and no index pointing at the guides, still calls a guide a
first draft): `python3 -I scripts/test_check_guides.py`.

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


if __name__ == "__main__":
    unittest.main()
