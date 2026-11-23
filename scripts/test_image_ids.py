"""The image ids the tools resolve, offline: `python3 -I scripts/test_image_ids.py`.

docs/image-ids.md names each program once and its first table lists the 14 guests this tree builds; the live runs'
records are held to the bridge program they deployed (`ferrata_bridge`, under "Superseded") and a run of the bridge program since milestone B1 to `ferrata_bridge_b1`, by
scripts/verify-sepolia.py's "image ids" check, run here on the committed records of crown-live-3 and crown-live-4 with
every socket refused and an RPC stand-in that answers only the chain's id and head; scripts/reproduce/compare_ids.py
holds this tree's rebuilt bridge program to `ferrata_bridge_b1`, as the crown runner and the crown driver hold a
bundle's; and every pin of B1's program in the tree is that row's. Standard library only; scratch in a temporary directory under $TMPDIR.
"""

import contextlib
import importlib.util
import io
import json
import os
import re
import shutil
import socket
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DOC = os.path.join(ROOT, "docs", "image-ids.md")
ROW = re.compile(r"^\|[^|]*`(\w+)`\s*\|\s*`(0x[0-9a-f]{64})`\s*\|\s*`([0-9a-f]{64})`", re.M)


def _refuse(*_a, **_k):
    raise OSError("network disabled by scripts/test_image_ids.py")


socket.socket.connect = _refuse  # type: ignore[method-assign]
socket.create_connection = _refuse  # type: ignore[assignment]


def load(name: str, path: str):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def rows() -> list:
    return ROW.findall(open(DOC, encoding="utf-8").read())


def entry(name: str) -> tuple:
    found = [(i, sha) for n, i, sha in rows() if n == name]
    assert len(found) == 1, f"docs/image-ids.md: {len(found)} rows for {name}"
    return found[0]


def image_ids_check(run_dir: str) -> str:
    """verify-sepolia.py's "image ids" line for a run's records, offline (a fresh copy of the module per call)."""
    vs = load("verify_sepolia_offline", os.path.join(HERE, "verify-sepolia.py"))

    class OfflineRpc:
        def __init__(self, url):
            self.url = url

        def __call__(self, method, *params):
            if method == "eth_chainId":
                return hex(vs.CHAIN_ID)
            if method == "eth_blockNumber":
                return hex(10_000_000)
            if method == "eth_getBlockByNumber" and params and params[0] == "finalized":
                return {"number": hex(9_999_900)}
            raise vs.RpcUnavailable(f"{method}: offline")

        def __getattr__(self, name):  # every other read (call, logs, ...): unavailable
            def offline(*_a, **_k):
                raise vs.RpcUnavailable(f"{name}: offline")
            return offline

    vs.Rpc = OfflineRpc
    argv, out = sys.argv, io.StringIO()
    sys.argv = ["verify-sepolia.py", run_dir, "--rpc", "http://offline.invalid", "--image-ids", DOC]
    try:
        with contextlib.redirect_stdout(out):
            try:
                vs.main()
            except SystemExit:
                pass
    finally:
        sys.argv = argv
    lines = [l for l in out.getvalue().splitlines() if l.split(" ", 1)[-1].lstrip().startswith("image ids:")]
    assert len(lines) == 1, out.getvalue()[-2000:]
    return lines[0]


class ImageIds(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="test-image-ids-")
        self.deployed = entry("ferrata_bridge")[0]
        self.b1 = entry("ferrata_bridge_b1")[0]

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_copy(self, run: str, bridge: str) -> str:
        """A copy of a committed run whose LEZ session A deployed `bridge` instead (the other records unchanged)."""
        d = os.path.join(self.tmp, f"{run}-{bridge[2:10]}")
        shutil.copytree(os.path.join(ROOT, "e2e", "runs", run), d)
        p = os.path.join(d, "lez", "session-a", "session-a.json")
        s = open(p, encoding="utf-8").read()
        self.assertIn(self.deployed[2:], s)
        open(p, "w", encoding="utf-8").write(s.replace(self.deployed[2:], bridge[2:]))
        return d

    def test_every_program_has_one_row(self):
        names = [n for n, _, _ in rows()]
        self.assertEqual(sorted(n for n in set(names) if names.count(n) > 1), [])
        self.assertNotEqual(self.deployed, self.b1)

    def test_the_live_runs_keep_the_bridge_they_deployed(self):
        for run in ("crown-live-3", "crown-live-4"):
            sa = json.load(open(os.path.join(ROOT, "e2e", "runs", run, "lez", "session-a", "session-a.json")))
            self.assertEqual(sa["bridge"]["imageId"].lower(), self.deployed, run)
            line = image_ids_check(os.path.join(ROOT, "e2e", "runs", run))
            self.assertTrue(line.startswith("PASS"), line)

    def test_a_run_of_b1_is_held_to_its_row(self):
        line = image_ids_check(self.run_copy("crown-live-4", self.b1))
        self.assertTrue(line.startswith("PASS") and "since milestone B1" in line, line)
        line = image_ids_check(self.run_copy("crown-live-4", "0x" + "11" * 32))
        self.assertTrue(line.startswith("FAIL"), line)
        # crown-live-3's vault is pinned: it holds the program it was deployed with, not B1's.
        line = image_ids_check(self.run_copy("crown-live-3", self.b1))
        self.assertTrue(line.startswith("FAIL") and "crown-live-3's vault holds" in line, line)

    def compare(self, bridge: str, release: str = "") -> tuple:
        """compare_ids.py on the outputs of a rebuild of this tree whose bridge program is `bridge`."""
        out = os.path.join(self.tmp, f"out-{bridge[2:10]}-{release[2:6]}")
        os.makedirs(os.path.join(out, "r0bf"))
        cmp = load("compare_ids_offline", os.path.join(HERE, "reproduce", "compare_ids.py"))
        first = re.split(r"\n## ", open(DOC, encoding="utf-8").read(), maxsplit=1)[0]  # as compare_ids.py reads it
        ids = {cmp.GUEST.get(n, n): i for n, i, _ in ROW.findall(first)}  # the names a rebuild gives its guests
        self.assertEqual(len(ids), 14)
        ids["ferrata_bridge"] = bridge
        if release:
            ids["release"] = release
        with open(os.path.join(out, "ids.tsv"), "w") as f:
            f.write("workspace\tname\timage_id\tsha256_bin\tbytes_bin\tsha256_user_elf\tr0vm_id\n")
            f.writelines(f"x\t{n}\t{i}\t-\t0\t-\t= id\n" for n, i in sorted(ids.items()))
        with open(os.path.join(out, "ids.txt"), "w") as f:
            f.write(f"ferrata_lez.bin x {ids['ferrata_lez'][2:]}\nferrata_bridge.bin x {bridge[2:]}\n")
        with open(os.path.join(out, "r0bf", "ids.jsonl"), "w") as f:
            f.writelines(json.dumps({"program": f"{n}.bin", "image_id": ids[n], "sha256": "x"}) + "\n"
                         for n in ("checkpoint_b", "checkpoint", "segment", "release"))
        argv, text = sys.argv, io.StringIO()
        sys.argv = ["compare_ids.py", "--out", out]
        try:
            with contextlib.redirect_stdout(text):
                rc = cmp.main()
        finally:
            sys.argv = argv
        return rc, text.getvalue()

    def test_compare_holds_this_trees_bridge_to_b1(self):
        rc, text = self.compare(self.b1)
        self.assertEqual(rc, 0, text)
        # The crown run's attestation program is this tree's; its bridge (before B1) and its four Ethereum-side programs
        # (RISC Zero 3.0.5) are not: this tree builds `ferrata_bridge_b1` and the `*_31` rows.
        self.assertIn("1 of 6 ids of the crown run's programs equal the recorded ones; bridge: this tree's own", text)
        for k in ("bridge", "checkpoint_b", "checkpoint", "segment", "release"):
            self.assertIn(f"{k}: this tree's own (docs/image-ids.md), equal", text)
        self.assertIn("14 of 14 guests equal docs/image-ids.md", text)
        rc, text = self.compare(self.deployed)  # the program before B1 is not what this tree builds
        self.assertEqual(rc, 1, text)
        rc, text = self.compare(self.b1, release="0x" + "22" * 32)
        self.assertEqual(rc, 1, text)

    def test_every_pin_of_b1_is_its_row(self):
        b1, sha = entry("ferrata_bridge_b1")
        app = open(os.path.join(ROOT, "e2e", "crown", "cloud", "app.py"), encoding="utf-8").read()
        self.assertIn(f'"bridge": "{b1}"', app)
        seqnet = open(os.path.join(ROOT, "e2e", "seqnet", "drive.sh"), encoding="utf-8").read()
        self.assertIn(f"BRIDGE_ID={b1} ", seqnet)
        day1 = open(os.path.join(ROOT, "lez", "testnet04", "day1.sh"), encoding="utf-8").read()
        self.assertIn(f'BRIDGE_SHA256="{sha}"', day1)
        self.assertIn(f'BRIDGE_IMAGE="{b1[2:]}"', day1)
        real = open(os.path.join(HERE, "privacy", "real.sh"), encoding="utf-8").read()
        self.assertIn("BRIDGE_ENTRY=ferrata_bridge_b1", real)
        crown = open(os.path.join(ROOT, "lez", "crown", "driver", "src", "crown.rs"), encoding="utf-8").read()
        self.assertIn(f'pub const BRIDGE_IMAGE_ID: &str = "{b1[2:]}";', crown)

    def test_the_crown_runner_wants_this_trees_lez_programs(self):
        """e2e/crown/cloud/drive.py's pre-flight: the bundle's LEZ programs must be docs/image-ids.md's `ferrata_lez` and
        `ferrata_bridge_b1` (its function run alone: the module itself starts a run when imported)."""
        import ast
        src = open(os.path.join(ROOT, "e2e", "crown", "cloud", "drive.py"), encoding="utf-8").read()
        fn = next(n for n in ast.parse(src).body if isinstance(n, ast.FunctionDef) and n.name == "tree_lez_programs")
        ns = {"REPO": ROOT, "re": re}

        def die(msg):
            raise AssertionError(msg)

        ns["die"] = die
        exec(compile(ast.Module(body=[fn], type_ignores=[]), "drive.py", "exec"), ns)
        self.assertEqual(ns["tree_lez_programs"](), {"ferrata_lez.bin": entry("ferrata_lez")[0],
                                                     "ferrata_bridge.bin": self.b1})
        sets = json.load(open(os.path.join(ROOT, "e2e", "crown", "cloud", "inputs.json"), encoding="utf-8"))
        self.assertEqual(sets["rc3-b1"]["programs"], sets["rc3"]["programs"])


if __name__ == "__main__":
    unittest.main()
