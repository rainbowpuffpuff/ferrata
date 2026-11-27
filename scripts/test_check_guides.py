"""Tests of scripts/check-guides.py's checks 2 (the mini-app's privacy table), 4 (no guide, and no index pointing at
the guides, still calls a guide a first draft), 5 (keyless retirement as the vault counts it), 6 (the counts a
terminal user relies on) and 7 (README.md's four links for reviewers, their targets and numbers):
`python3 -I scripts/test_check_guides.py`.

Standard library only; nothing is written to disk. The texts are built here; the last test of each class reads the
repository.
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
        # README.md:193 before the fix, and the docs index's heading.
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
    # The sentences as they stood at cc6b0fa3, before the fix of 22 November 2026.
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


class MethodCounts(unittest.TestCase):
    """Check 6's first half: the stated method counts against each module's trait."""

    ROOT = os.path.dirname(HERE)
    # The five counts as they stand: every path the check reads, in step with the traits below.
    COUNTS = {
        "modules/README.md": "`ferrata_attest_module` (eleven methods: dry runs),\n"
                             "`ferrata_bridge_module` (twelve: deposit, claim), and\n",
        "docs/guides/consumer-guide.md": "Logos core module (eleven methods, `ferrata_attest_module`), so a\n",
        "docs/guides/user-guide.md": "twelve methods a Logos ui module or a client calls; "
                                     "The twelve methods natively\n",
    }
    # The traits the counts must match: 11 attest methods, 12 bridge methods.
    TRAITS = {"ferrata_attest_module": ["version"] * 11, "ferrata_bridge_module": ["version"] * 12}
    # The shapes as they stood before the fix of 26 November 2026, against the traits as they are.
    OLD_MODULES_README = ("# Logos modules\n\n`ferrata_attest_module` (eight methods: dry runs, checkpoint checks),\n"
                          "`ferrata_bridge_module` (eleven: deposit, claim, burn), and the daemons.\n")
    TRAIT = ("/// The module's methods.\npub trait FerrataBridgeModule: Send + 'static {\n"
             "    fn version(&mut self) -> String;\n    fn deposit(&mut self, request: String) -> String;\n"
             "    fn claim(&mut self, request: String) -> String;\n"
             "    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}\n}\n")

    def texts(self, **planted):
        texts = dict(self.COUNTS)
        texts.update(planted)
        return texts

    def test_the_traits_methods_are_counted_without_the_hook(self):
        self.assertEqual(cg.trait_methods(self.TRAIT), ["version", "deposit", "claim"])

    def test_a_file_without_a_trait_is_refused(self):
        self.assertIsNone(cg.trait_methods("// no trait here\n"))

    def test_the_old_counts_are_caught(self):
        out = cg.method_count_findings(self.texts(**{"modules/README.md": self.OLD_MODULES_README}), self.TRAITS)
        self.assertEqual(len(out), 2, out)
        self.assertIn("says eight methods of ferrata_attest_module, its trait has 11", out[0])
        self.assertIn("says eleven methods of ferrata_bridge_module, its trait has 12", out[1])

    def test_a_sentence_reworded_away_from_a_number_is_caught(self):
        text = "`ferrata_attest_module` (some methods: dry runs),\n`ferrata_bridge_module` (twelve: deposit), and\n"
        out = cg.method_count_findings(self.texts(**{"modules/README.md": text}), self.TRAITS)
        self.assertEqual(len(out), 1, out)
        self.assertIn("no method count of ferrata_attest_module", out[0])

    def test_an_unreadable_trait_is_caught(self):
        out = cg.method_count_findings(self.texts(), {"ferrata_attest_module": None,
                                                      "ferrata_bridge_module": None})
        self.assertEqual(len(out), 5, out)
        self.assertTrue(all("no `pub trait` block to count the methods of" in f for f in out), out)

    def test_a_method_left_out_of_the_table_is_caught(self):
        readme = "| Module | Directory | Methods |\n|---|---|---|\n" \
                 "| `ferrata_bridge_module` | `ferrata_bridge_module/` | `version`, `deposit`, `claim` |\n"
        out = cg.method_list_findings(readme, {"ferrata_bridge_module": ["version", "deposit", "claim", "admin"]})
        self.assertEqual(out, ["modules/README.md: ferrata_bridge_module's trait has `admin`, "
                               "its table row does not name it"])

    def test_a_removed_method_left_in_the_table_is_caught(self):
        """The other direction (review round 3, M8): a row may not name a callable the trait no longer has, for a
        module with no numeric count shape to catch it either."""
        readme = ("| Module | Directory | Methods |\n|---|---|---|\n"
                  "| `ferrata_monitor_module` | `ferrata_monitor_module/` | `version`, `check`, `removed_method` |\n")
        out = cg.method_list_findings(readme, {"ferrata_monitor_module": ["version", "check"]})
        self.assertEqual(out, ["modules/README.md: ferrata_monitor_module's table row names `removed_method`, "
                               "its trait has no such method"])

    def test_the_rows_descriptive_references_still_pass(self):
        """A row's notes may name other modules' parts (`keystore_module`, `tx_sender_module`) in backticks."""
        readme = ("| Module | Directory | Methods |\n|---|---|---|\n"
                  "| `ferrata_monitor_module` | `ferrata_monitor_module/` | `version`, `run`; `custody` (the watcher "
                  "key into Logos Core's `keystore_module`, unlocked for `tx_sender_module`) |\n")
        self.assertEqual(cg.method_list_findings(readme, {"ferrata_monitor_module": ["version", "run", "custody"]}), [])

    def test_the_repository_agrees(self):
        methods = {}
        for module, path in cg.MODULES.items():
            with open(os.path.join(self.ROOT, path), encoding="utf-8") as f:
                methods[module] = cg.trait_methods(f.read())
        texts = {}
        for path in dict.fromkeys(p for p, _, _ in cg.METHOD_COUNTS):
            with open(os.path.join(self.ROOT, path), encoding="utf-8") as f:
                texts[path] = f.read()
        self.assertEqual(cg.method_count_findings(texts, methods), [])
        with open(os.path.join(self.ROOT, "modules", "README.md"), encoding="utf-8") as f:
            self.assertEqual(cg.method_list_findings(f.read(), methods), [])


class TestCounts(unittest.TestCase):
    """Check 6's second half: the web wallet's README and the mini-app's, against what they describe."""

    ROOT = os.path.dirname(HERE)
    WALLET_README = ("```sh\nnode --test test/*.test.mjs                       # 48: LEZ's pinned vectors\n```\n"
                     "\n- `test/fork_bridge.py`: the bridge tab against the deployed Sepolia vault on an anvil fork "
                     "of Sepolia (nothing reaches Sepolia; `--fork-url`, `--deployment vault/deployments/x.json`, "
                     "`--config-id`, and `--out`, the directory the record is written to; all four are required):\n"
                     "  deposits refused.\n- `test/public_read.py`: read-only against the public network.\n")
    TESTS = {f"test/{n}.test.mjs": "test('a', () => {});\ntest('b', () => {});\n" for n in ("one", "two")}
    FORK = ('    ap.add_argument("--fork-url", required=True)\n'
            '    ap.add_argument("--deployment", required=True)\n'
            '    ap.add_argument("--out", required=True)\n'
            '    ap.add_argument("--config-id", required=True)\n')
    UI_README = "the view's 27 test\nfunctions pass, 27 of 27 on 23 November\n"
    UI_RESULTS = "On the Modal build box: `scripts/box-test.sh` ran the view's 27 test functions, **27 passed in 40.5 s**\n"

    def test_the_wallets_test_count_is_read_from_the_files(self):
        out = cg.wallet_test_count_findings(self.WALLET_README, self.TESTS)
        self.assertIn("says 48 Node tests, the files declare 4", out[0])

    def test_the_missing_out_argument_is_caught(self):
        with open(os.path.join(self.ROOT, cg.FORK_BRIDGE), encoding="utf-8") as f:
            script = f.read()
        readme = self.WALLET_README.replace("`--config-id`, and `--out`, the directory the record is written to; "
                                            "all four are required", "`--config-id`")
        out = cg.fork_bridge_args_findings(readme, script)
        self.assertEqual(out, [f"{cg.WALLET_README}: `test/fork_bridge.py` requires --out, "
                               "its bullet does not name it"])

    def test_the_mini_apps_count_against_its_results(self):
        out = cg.ui_count_findings(self.UI_README.replace("27", "17"), self.UI_RESULTS)
        self.assertEqual(len(out), 1, out)
        self.assertIn("says 17 of 17", out[0])

    def test_a_results_file_this_check_cannot_read_is_caught(self):
        self.assertEqual(len(cg.ui_count_findings(self.UI_README, "nothing to read here")), 1)

    def test_the_repository_agrees(self):
        with open(os.path.join(self.ROOT, cg.WALLET_README), encoding="utf-8") as f:
            readme = f.read()
        tests = {}
        for name in sorted(os.listdir(os.path.join(self.ROOT, cg.WALLET_TESTS))):
            if name.endswith(".test.mjs"):
                with open(os.path.join(self.ROOT, cg.WALLET_TESTS, name), encoding="utf-8") as f:
                    tests[f"test/{name}"] = f.read()
        self.assertEqual(cg.wallet_test_count_findings(readme, tests), [])
        with open(os.path.join(self.ROOT, cg.UI_README), encoding="utf-8") as f:
            ui = f.read()
        with open(os.path.join(self.ROOT, cg.UI_RESULTS), encoding="utf-8") as f:
            results = f.read()
        self.assertEqual(cg.ui_count_findings(ui, results), [])


class ReviewerLinks(unittest.TestCase):
    """Check 7: README.md's four links for reviewers, against the documents that hold their numbers and targets."""

    ROOT = os.path.dirname(HERE)
    NET = "https://1-2-3-4.sslip.io"
    STATUS = ("| RFP | shown | in part | not yet | waits on Logos | total |\n|---|---:|---:|---:|---:|---:|\n"
              "| RFP-021 | 49 | 6 | 1 | 3 | **59** |\n| RFP-022 | 47 | 18 | 1 | 2 | **68** |\n")
    VERIFY = ("python3 -I scripts/verify.py --all   # the 119 quick offline checks: about 6 minutes\n"
              "python3 -I scripts/verify.py --all --online   # the 22 online checks: about 20 minutes\n")
    README = ("# X\n\n## For reviewers: four links\n\n"
              "1. **The status, with counts:** [docs/requirements-status.md](docs/requirements-status.md). Of the 59 hard\n"
              "   requirements of RFP-021 and the 68 of RFP-022, **49 and 47 are shown** under our strict rule; 6 and 18 in part,\n"
              "   1 and 1 not yet, 3 and 2 wait on Logos (27 November 2026).\n"
              "2. **Verify it in ten minutes:** [docs/VERIFY.md](docs/VERIFY.md). Clone, then `verify.py --all`\n"
              "   runs the 119 quick offline checks; then `--online` runs the 22 online checks, read-only.\n"
              "3. **Try it on our test network:** [the testers' page](https://1-2-3-4.sslip.io/testers/): the faucet.\n"
              "4. **The run records:** [docs/EVIDENCE.md](docs/EVIDENCE.md): every run.\n\n## What it is\n")

    def run_on(self, readme=None, status=None, verify=None, net=None):
        return cg.reviewer_findings(self.README if readme is None else readme, status or self.STATUS,
                                    verify or self.VERIFY, net or self.NET)

    def test_the_example_passes(self):
        self.assertEqual(self.run_on(), [])

    def test_a_count_the_status_document_changed_is_caught(self):
        out = self.run_on(status=self.STATUS.replace("| 49 | 6 |", "| 50 | 5 |"))
        self.assertEqual(len(out), 1, out)
        self.assertIn("docs/requirements-status.md (59, 68, 50, 47, 5, 18", out[0])

    def test_a_count_changed_in_the_readme_is_caught(self):
        self.assertEqual(len(self.run_on(readme=self.README.replace("3 and 2 wait", "3 and 3 wait"))), 1)

    def test_counts_reworded_away_are_caught(self):
        out = self.run_on(readme=self.README.replace("**49 and 47 are shown**", "49 and 47 are shown"))
        self.assertIn("no longer states the counts", out[0])

    def test_a_check_count_is_held_to_verify_md(self):
        out = self.run_on(verify=self.VERIFY.replace("the 119 quick", "the 120 quick"))
        self.assertEqual(out, ["README.md: the reviewer block says ['119'] quick offline checks, docs/VERIFY.md ['120']"])

    def test_the_testers_page_on_another_host_is_caught(self):
        self.assertEqual(len(self.run_on(net="https://5-6-7-8.sslip.io")), 1)

    def test_links_out_of_order_are_caught(self):
        readme = self.README.replace("1. **The status", "0. **The status")
        self.assertIn("the reviewer links are", self.run_on(readme=readme)[0])

    def test_a_missing_section_is_caught(self):
        self.assertEqual(self.run_on(readme="# X\n\n## Start here\n"), ["README.md: no `## For reviewers: four links` section"])

    def test_the_repository_agrees(self):
        import json
        texts = {}
        for p in ("README.md", cg.STATUS_DOC, cg.VERIFY_DOC, cg.NETWORK_DOC):
            with open(os.path.join(self.ROOT, p), encoding="utf-8") as f:
                texts[p] = f.read()
        out = cg.reviewer_findings(texts["README.md"], texts[cg.STATUS_DOC], texts[cg.VERIFY_DOC],
                                   json.loads(texts[cg.NETWORK_DOC])["network"])
        self.assertEqual(out, [])


if __name__ == "__main__":
    unittest.main()
