#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_lez040 (standard library only, no network).

Each offline check passes on the real tree and on a minimal copy holding only its declared inputs.
One thing is then altered per case and must FAIL with a line naming the difference. The online
check's two transport functions are replaced: no test performs a network request. Copies stay
under .scratch/verify-lez040-tests/, never the system temporary directory.
"""
from __future__ import annotations

import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_lez040  # noqa: E402
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-lez040-tests"
OFFLINE = ("lez040-selector", "lez040-spel", "lez040-catalogue")
ALL = OFFLINE + ("lez040-selector-online",)
SELECTOR = "lez/scripts/lez-select.sh"
RECORD = "docs/port-v0.4.0/lez-tags.json"
V040_LOG = "idl/generated/spel-v2-f739d8a-lez-a255b42c.txt"
V040_IDL = "idl/generated/at-f739d8a-lez-a255b42c/ferrata_attestation.idl-v2.json"
PORT = "docs/port-v0.4.0.md"
V040_COMMIT = "a255b42c0fc80d1cb219454a3a1f5ec3b5d92115"
CARGO_TOML = """[workspace.dependencies]
risc0-zkvm = { version = "=3.1.0" }
risc0-build = { version = "=3.0.7" }
risc0-zkos-v1compat = { version = "=2.2.3" }
"""


def _replace(path: Path, old: str, new: str, count: int = 1) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != count:
        raise AssertionError(f"{path}: expected {count} of {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def _catalogue_row(root: Path, row_id: str, status: str, evidence: str) -> None:
    path = root / PORT
    lines = path.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines):
        if line.startswith(f"| {row_id} |"):
            cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
            status_col, evidence_col = (5, 6) if row_id.startswith("C") else (3, 4)
            cells[status_col], cells[evidence_col] = status, evidence
            lines[i] = "| " + " | ".join(cells) + " |"
            path.write_text("\n".join(lines) + "\n", encoding="utf-8")
            return
    raise AssertionError(f"no catalogue row {row_id}")


class Lez040VerifyTamperTests(unittest.TestCase):
    maxDiff = 5000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in ALL}
        cls.record = json.loads((ROOT / RECORD).read_text(encoding="utf-8"))
        cls.listing = "\n".join(cls.record["ls_remote"]) + "\n"

    def _copy(self, cid: str, destination: Path) -> Path:
        rels = list(self.checks[cid].inputs)
        if cid == "lez040-catalogue":  # the paths a "tested on v0.4.0" row's commands name, which the check requires
            import re
            text = (ROOT / PORT).read_text(encoding="utf-8")
            for _, status, evidence in checks_lez040._catalogue_rows(text):
                if status == "tested on v0.4.0":
                    for command in re.findall(r"`((?:python3|node|cargo|bash|just|forge) [^`]*)`", evidence):
                        rels += [t for t in command.split() if "/" in t and (ROOT / t).exists()]
        for rel in dict.fromkeys(rels):
            source = ROOT / rel
            target = destination / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                shutil.copy2(source, target)
        return destination

    def _run(self, cid: str, root: Path = ROOT):
        return run_check(self.checks[cid], Ctx(root=root, git_dir=ROOT))

    def _altered(self, cid: str, edit, contains: str):
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as temporary:
            root = self._copy(cid, Path(temporary))
            edit(root)
            result = self._run(cid, root)
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect the alteration: {result.lines}")
        self.assertTrue(any(contains.lower() in line.lower() for line in result.lines),
                        f"{cid}: no line names {contains!r}: {result.lines}")

    def _replace_and_fail(self, cid: str, rel: str, old: str, new: str, contains: str, count: int = 1):
        self._altered(cid, lambda root: _replace(root / rel, old, new, count), contains)

    def _online_result(self, listing=None, manifest=CARGO_TOML, ls_error=None, fetch_error=None):
        calls = []

        def fetch(url):
            calls.append(url)
            if fetch_error:
                raise fetch_error
            return manifest

        def ls_remote(_url):
            if ls_error:
                raise ls_error
            return self.listing if listing is None else listing

        with mock.patch.object(checks_lez040, "_ls_remote", ls_remote), \
                mock.patch.object(checks_lez040, "_fetch", fetch):
            result = self._run("lez040-selector-online")
        self.transport_urls = calls
        return result

    def test_01_every_check_passes_on_the_real_tree(self):
        for cid in OFFLINE:
            with self.subTest(check=cid):
                result = self._run(cid)
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")
        result = self._online_result()
        self.assertEqual(result.status, "PASS", result.lines)
        self.assertEqual(self.transport_urls,
                         [f"https://raw.githubusercontent.com/logos-blockchain/logos-execution-zone/{V040_COMMIT}/Cargo.toml"])

    def test_02_minimal_input_copies_pass(self):
        for cid in OFFLINE:
            with self.subTest(check=cid), tempfile.TemporaryDirectory(prefix=cid + "-clean-", dir=SCRATCH) as temporary:
                root = self._copy(cid, Path(temporary))
                result = self._run(cid, root)
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")

    def test_03b_each_port_evidence_row_selects_its_checks(self):
        """`verify.py evidence:N` for rows 301 to 305 runs the checks each row names (none selects nothing)."""
        from verifylib import catalog
        catalog.load(ROOT)
        want = {301: {"lez040-spel"}, 302: {"lez040-image-ids", "lez040-image-ids-rebuild"},
                303: {"lez040-run-standalone", "lez040-run-four-keys", "lez040-run-vote-1", "lez040-run-vote-2"},
                304: {"lez040-run-clients"}, 305: {"lez040-run-cli-e2e"}}
        evidence = (ROOT / "docs/EVIDENCE.md").read_text()
        for n, ids in want.items():
            with self.subTest(row=n):
                self.assertEqual({c.id for c in REGISTRY.values() if n in c.evidence}, ids)
                line = next(ln for ln in evidence.splitlines() if ln.startswith(f"| {n} |"))
                for cid in ids - {"lez040-image-ids-rebuild"}:  # row 302 names the rebuild by its script
                    self.assertIn(cid, line)

    def test_03_metadata_is_complete_and_inputs_exist(self):
        self.assertEqual(set(self.checks), set(ALL))
        for cid, registered in self.checks.items():
            with self.subTest(check=cid):
                if cid == "lez040-spel":  # SPEL on v0.4.0 backs both U.4 rows through EVIDENCE row 301
                    self.assertEqual((registered.rows, registered.evidence), (("RFP-021 U.4", "RFP-022 U.4"), (301,)))
                else:
                    self.assertEqual((registered.rows, registered.evidence), ((), ()))
                self.assertTrue(registered.title and registered.what and registered.against)
                self.assertTrue(registered.inputs)
                self.assertIsNotNone(registered.fn)
                for rel in registered.inputs:
                    path = Path(rel)
                    self.assertFalse(path.is_absolute(), rel)
                    self.assertNotIn("..", path.parts, rel)
                    self.assertTrue((ROOT / rel).exists(), f"{cid}: missing input {rel}")
        self.assertEqual(self.checks["lez040-selector-online"].needs, ("python3", "git", "network"))

    # Selector: each alteration is independent and made on a copy of only its inputs.
    def test_10_selector_commit_differs(self):
        self._replace_and_fail("lez040-selector", SELECTOR,
                               f"v0.4.0) echo {V040_COMMIT}", f"v0.4.0) echo b{V040_COMMIT[1:]}", "selector commit")

    def test_11_record_commit_differs(self):
        self._replace_and_fail("lez040-selector", RECORD,
                               f'"commit": "{V040_COMMIT}"', f'"commit": "c{V040_COMMIT[1:]}"', "record")

    def test_12_selector_risc0_pin_differs(self):
        self._replace_and_fail("lez040-selector", SELECTOR,
                               "3.1.0 3.0.7 2.2.3", "3.1.1 3.0.7 2.2.3", "RISC Zero triple")

    def test_13_manifest_lez040_feature_removed(self):
        self._replace_and_fail("lez040-selector", "lez/harness/Cargo.toml",
                               'lez-040 = ["lez-rc3"]', "lez-040 = []", 'lez-040 does not include "lez-rc3"')

    def test_14_lockfile_patch_names_another_file(self):
        self._replace_and_fail("lez040-selector", "lez/lockfiles/v0.4.0.patch",
                               "--- a/Cargo.lock\n+++ b/Cargo.lock", "--- a/Cargo.toml\n+++ b/Cargo.lock", "Cargo.lock")

    def test_15_committed_source_line_names_the_twin(self):
        self._replace_and_fail("lez040-selector", "lez/guests/bridge/Cargo.toml",
                               'path = "src/main.rs" # lez-select', 'path = "src/main_lez040.rs" # lez-select',
                               "names the v0.4.0 twin")

    def test_16_a_twin_missing(self):
        self._altered("lez040-selector", lambda root: (root / "lez/guests/msg/src/main_lez040.rs").unlink(),
                      "twin src/main_lez040.rs is missing")

    # Online: remote data, not transport, is altered. A transport error is a SKIP, never a FAIL.
    def test_20_online_moved_tag_fails(self):
        moved = self.listing.replace(V040_COMMIT, "1" * 40, 1)
        result = self._online_result(listing=moved)
        self.assertEqual(result.status, "FAIL", result.lines)
        self.assertTrue(any("live commit" in line for line in result.lines), result.lines)

    def test_21_online_different_pin_fails(self):
        result = self._online_result(manifest=CARGO_TOML.replace("3.0.7", "3.0.8"))
        self.assertEqual(result.status, "FAIL", result.lines)
        self.assertTrue(any("risc0-build" in line and "differs" in line for line in result.lines), result.lines)

    def test_22_online_network_error_skips(self):
        for kwargs in ({"ls_error": OSError("network down")}, {"fetch_error": OSError("network down")}):
            with self.subTest(**kwargs):
                result = self._online_result(**kwargs)
                self.assertEqual(result.status, "SKIP", result.lines)
                self.assertTrue(any("network down" in line for line in result.lines), result.lines)

    def test_23_online_newer_tag_is_information_only(self):
        listing = self.listing + "f" * 40 + "\trefs/tags/v0.4.0-rc6\n"
        result = self._online_result(listing=listing)
        self.assertEqual(result.status, "PASS", result.lines)
        self.assertTrue(any("information" in line and "v0.4.0-rc6" in line for line in result.lines), result.lines)

    # SPEL: generated bytes, each count fact, and the second-line commit are independently checked.
    def test_30_spel_one_generated_byte_fails(self):
        self._replace_and_fail("lez040-spel", V040_IDL, '"version": "2"', '"version": "3"', "byte for byte")

    def test_31_spel_zero_differences_fails(self):
        self._replace_and_fail("lez040-spel", V040_LOG, "33 messages and types compared, 0 difference(s)",
                               "33 messages and types compared, 1 difference(s)", "0 difference(s)")

    def test_32_spel_decoded_count_fails(self):
        self._replace_and_fail("lez040-spel", V040_LOG, "decoded 21 of 21", "decoded 20 of 21", "decoded counts")

    def test_33_spel_header_commit_fails(self):
        self._replace_and_fail("lez040-spel", V040_LOG, V040_COMMIT, "d" + V040_COMMIT[1:], "second-line LEZ commit")

    def _both_generated(self, program: str, old: str, new: str, contains: str):
        """The same change in the v0.4.0 and the rc3 generated IDL (so they stay byte for byte equal): only the
        comparison with the hand-written IDL can see it (review round 1 of this branch, M2)."""
        def edit(root):
            for tree in ("lez-a255b42c", "lez-bafd07be"):
                _replace(root / f"idl/generated/at-f739d8a-{tree}/{program}.idl-v2.json", old, new)
        self._altered("lez040-spel", edit, contains)

    def test_34_spel_a_message_renamed_in_both_runs(self):
        self._both_generated("ferrata_attestation", '"name": "Init"', '"name": "CorruptedInit"',
                             "the hand-written instruction there is")

    def test_35_spel_a_field_type_changed_in_both_runs(self):
        def edit(root):
            for tree in ("lez-a255b42c", "lez-bafd07be"):
                path = root / f"idl/generated/at-f739d8a-{tree}/ferrata_bridge.idl-v2.json"
                idl = json.loads(path.read_text(encoding="utf-8"))
                idl["messages"][0]["fields"][2]["type"] = {"array": ["u8", 21]}  # Burn's memo, [u8; 20]
                path.write_text(json.dumps(idl, indent=2), encoding="utf-8")
        self._altered("lez040-spel", edit, "differ from the hand-written arguments")

    def test_36_spel_a_type_renamed_in_both_runs(self):
        self._both_generated("ferrata_attestation", '"name": "Hop"', '"name": "Hops"', "is not generated")

    # Catalogue: use C3 (currently not started) and S2 (the real recorded run) as isolated fixtures.
    def test_40_catalogue_unknown_status_fails(self):
        self._altered("lez040-catalogue", lambda root: _catalogue_row(root, "C3", "finished", ""), "unknown status")

    def test_41_catalogue_tested_without_command_fails(self):
        self._altered("lez040-catalogue",
                      lambda root: _catalogue_row(root, "C3", "tested on v0.4.0", "ran locally"), "no backticked command")

    def test_42_catalogue_recorded_path_missing_fails(self):
        self._altered("lez040-catalogue",
                      lambda root: _catalogue_row(root, "S2", "recorded run", "`idl/generated/not-there.txt`"),
                      "does not exist")

    def test_43_catalogue_recorded_path_no_recheck_fails(self):
        def edit(root):
            _catalogue_row(root, "S2", "recorded run", "`e2e/runs/zone-v040-not-read/README.md`")
            target = root / "e2e/runs/zone-v040-not-read/README.md"
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("a run's record, but no lez040 check's input\n", encoding="utf-8")
        self._altered("lez040-catalogue", edit, "no other lez040 check's inputs")

    def test_50_catalogue_rows_must_be_there(self):
        """review round 2, M4: a renamed header left 0 rows to check, and the check passed."""
        self._replace_and_fail("lez040-catalogue", PORT, "| Status | Evidence |", "| State | Evidence |",
                               "no row C1", count=2)

    def test_51_catalogue_recorded_run_must_be_a_run(self):
        """review round 2, M4: GitHub's tag listing, read by lez040-selector, is no run's record."""
        def edit(root):
            target = root / "docs/port-v0.4.0/lez-tags.json"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "docs/port-v0.4.0/lez-tags.json", target)
            _catalogue_row(root, "S2", "recorded run", "`docs/port-v0.4.0/lez-tags.json`")
        self._altered("lez040-catalogue", edit, "is not a run's record")

    def test_53_catalogue_a_marker_in_a_comment_is_not_v040(self):
        """review (Kimi 17): `# v0.4.0` after a CI test passed the v0.4.0 rule."""
        def edit(root):
            target = root / "lez/scripts/test_lez_select.py"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "lez/scripts/test_lez_select.py", target)
            _catalogue_row(root, "C3", "tested on v0.4.0", "`python3 -I lez/scripts/test_lez_select.py # v0.4.0`")
        self._altered("lez040-catalogue", edit, "no command is about v0.4.0")

    def test_52_catalogue_tested_needs_a_v040_command(self):
        """review round 2, M4: any test CI runs passed for "tested on v0.4.0"."""
        def edit(root):
            target = root / "lez/scripts/test_lez_select.py"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "lez/scripts/test_lez_select.py", target)
            _catalogue_row(root, "C3", "tested on v0.4.0", "`python3 -I lez/scripts/test_lez_select.py`")
        self._altered("lez040-catalogue", edit, "no command is about v0.4.0")

    def test_44_catalogue_building_commit_missing_fails(self):
        missing = "1111111111111111111111111111111111111111"
        self._altered("lez040-catalogue",
                      lambda root: _catalogue_row(root, "C2", "building", f"branch `{missing}`"), "not a commit")

    def test_46_catalogue_unpublished_commit_listed_passes(self):
        """The port's unpublished ids (docs/port-v0.4.0/unpublished-commits.txt) count while 6a539b7f is in the clone,
        so GitHub's clone, which has none of them, passes; an id in neither still fails (test_44)."""
        fake = "2222222222222222222222222222222222222222"
        with tempfile.TemporaryDirectory(prefix="lez040-catalogue-", dir=SCRATCH) as temporary:
            root = self._copy("lez040-catalogue", Path(temporary))
            with open(root / checks_lez040.UNPUBLISHED, "a", encoding="utf-8") as f:
                f.write(f"{fake} lez-040-core\n")
            _catalogue_row(root, "C2", "building", f"branch `{fake}`")
            result = self._run("lez040-catalogue", root)
        self.assertEqual(result.status, "PASS", result.lines)

    def test_47_catalogue_unpublished_commit_needs_the_published_one(self):
        """A listed unpublished id does not count when the published commit is absent from the clone."""
        fake = "2222222222222222222222222222222222222222"
        with tempfile.TemporaryDirectory(prefix="lez040-catalogue-", dir=SCRATCH) as temporary:
            root = self._copy("lez040-catalogue", Path(temporary))
            with open(root / checks_lez040.UNPUBLISHED, "a", encoding="utf-8") as f:
                f.write(f"{fake} lez-040-core\n")
            _catalogue_row(root, "C2", "building", f"branch `{fake}`")
            with mock.patch.object(checks_lez040, "PUBLISHED_PORT", "3333333333333333333333333333333333333333"):
                result = self._run("lez040-catalogue", root)
        self.assertEqual(result.status, "FAIL", result.lines)
        self.assertTrue(any("not a commit" in line for line in result.lines), result.lines)

    def test_45_catalogue_tested_command_paths_are_checked(self):
        def edit(root):
            _catalogue_row(root, "C3", "tested on v0.4.0", "`python3 -I scripts/no-such-check.py`")
        self._altered("lez040-catalogue", edit, "command path")

    def test_48_catalogue_tested_command_must_run_somewhere(self):
        """review round 1 of this branch, M1: a command that runs nowhere is not evidence."""
        for evidence, contains in (
                ("`python3 -I -c definitely-not-a-test`", "is no registered check"),
                ("`python3 -I scripts/verify.py no-such-check`", "is no registered check"),
                ("`python3 -I scripts/reproduce/compare_lez040_ids.py`", "is no registered check")):
            with self.subTest(evidence=evidence):
                def edit(root, evidence=evidence):
                    _catalogue_row(root, "C3", "tested on v0.4.0", evidence)
                    if "compare_lez040_ids" in evidence:
                        target = root / "scripts/reproduce/compare_lez040_ids.py"
                        target.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(ROOT / "scripts/reproduce/compare_lez040_ids.py", target)
                self._altered("lez040-catalogue", edit, contains)

    def test_49_catalogue_tested_by_a_ci_test_passes(self):
        with tempfile.TemporaryDirectory(prefix="catalogue-ci-", dir=SCRATCH) as temporary:
            root = self._copy("lez040-catalogue", Path(temporary))
            target = root / "scripts/test_lez040_decode.py"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "scripts/test_lez040_decode.py", target)
            _catalogue_row(root, "C3", "tested on v0.4.0", "`python3 -I scripts/test_lez040_decode.py`")
            result = self._run("lez040-catalogue", root)
            self.assertEqual(result.status, "PASS", result.lines)

    def test_46_catalogue_recorded_run_directory_read_by_a_check_passes(self):
        """A run's directory cited, the files a check reads inside it: re-checked (lez040-spel reads the generated
        IDLs in this directory), with or without the trailing slash."""
        for cited in ("idl/generated/at-f739d8a-lez-a255b42c/", "idl/generated/at-f739d8a-lez-a255b42c"):
            with self.subTest(cited=cited), tempfile.TemporaryDirectory(prefix="catalogue-dir-", dir=SCRATCH) as t:
                root = self._copy("lez040-catalogue", Path(t))
                shutil.copytree(ROOT / "idl/generated/at-f739d8a-lez-a255b42c", root / "idl/generated/at-f739d8a-lez-a255b42c")
                _catalogue_row(root, "S2", "recorded run", f"`{cited}`")
                result = self._run("lez040-catalogue", root)
                self.assertEqual(result.status, "PASS", result.lines)

    def test_47_catalogue_sibling_prefix_is_not_read(self):
        """A path that only shares a prefix with an input (`...-lez-a255b42c-other`) is not read by it."""
        def edit(root):
            other = root / "idl/generated/at-f739d8a-lez-a255b42c-other"
            other.mkdir(parents=True)
            (other / "x.json").write_text("{}", encoding="utf-8")
            _catalogue_row(root, "S2", "recorded run", "`idl/generated/at-f739d8a-lez-a255b42c-other/`")
        self._altered("lez040-catalogue", edit, "no other lez040 check's inputs")


if __name__ == "__main__":
    unittest.main(verbosity=2)
