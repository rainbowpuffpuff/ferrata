#!/usr/bin/env python3
"""Offline tests of lez/scripts/lez-select.sh: run it on a copy of lez/'s manifests and lockfiles, beside stand-in LEZ
trees, through every tag and back.

    python3 -I lez/scripts/test_lez_select.py

Checks, for v0.4.0-rc1, rc2, rc3 and v0.4.0: the LEZ paths of every listed manifest (the messages guests on rc3 but
under v0.4.0), the path of `program_loader_core` (moved in v0.4.0, LEZ #946), the RISC Zero pins (3.1.0 on v0.4.0,
LEZ #1124), the `# lez-select` default features and that `lez-040` implies `lez-rc3`, the lockfile patches (applied
and reverted without leftovers, the committed lockfiles back byte for byte), the refusals (an unknown tag, a LEZ
checkout at another commit), that every lez/ manifest with a LEZ path is listed (a crate left on another tree breaks
the graph: cargo's "package collision"), and that the tags' commits agree with scripts/fetch-upstream.sh and
lez/live/env.sh. Nothing is built or fetched; scratch goes under the repository's .scratch/.
"""
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

LEZ = Path(__file__).resolve().parents[1]
REPO = LEZ.parent
SCRIPT = LEZ / "scripts" / "lez-select.sh"
TAGS = ["v0.4.0-rc1", "v0.4.0-rc2", "v0.4.0-rc3", "v0.4.0"]
# lez/ manifests with a LEZ path that the selector leaves alone, each on purpose: the lab driver and the first names
# wire (kept as the record of v0.4.0-rc1; neither is in a graph the selector moves).
UNLISTED = {"lab/driver", "names-wire-v1"}


def bash_array(name: str) -> list[str]:
    text = SCRIPT.read_text()
    m = re.search(rf"^{name}=\((.*?)\)", text, re.S | re.M)
    assert m, f"no array {name} in lez-select.sh"
    return m.group(1).split()


def commits_in_script() -> dict[str, str]:
    return dict(re.findall(r"^\s+(v0\.4\.0(?:-rc\d)?)\) echo ([0-9a-f]{40}) ;;$", SCRIPT.read_text(), re.M))


class LezSelect(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifests = bash_array("manifests")
        cls.pinned = bash_array("rc3_pinned")
        cls.methods = bash_array("methods")
        cls.featured = bash_array("featured")
        cls.sourced_040 = bash_array("sourced_040")
        scratch = REPO / ".scratch"
        scratch.mkdir(exist_ok=True)
        cls.tmp = Path(tempfile.mkdtemp(prefix="test-lez-select-", dir=scratch))
        cls.work = cls.tmp / "work"
        cls.lez = cls.work / "ferrata" / "lez"
        (cls.lez / "scripts").mkdir(parents=True)
        shutil.copy2(SCRIPT, cls.lez / "scripts" / "lez-select.sh")
        shutil.copytree(LEZ / "lockfiles", cls.lez / "lockfiles")
        cls.files = ["Cargo.lock"]
        for d in sorted(set(cls.manifests + cls.pinned + cls.methods + cls.featured + cls.sourced_040)):
            for f in ("Cargo.toml", "Cargo.lock"):
                if (LEZ / d / f).exists():
                    cls.files.append(f"{d}/{f}")
        for f in cls.files:
            (cls.lez / f).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(LEZ / f, cls.lez / f)
        for tag in TAGS:  # stand-in LEZ trees: no .git, so the commit check is skipped
            (cls.work / "upstream" / f"lez-{tag}").mkdir(parents=True)
            (cls.work / "upstream" / f"lez-{tag}" / "Cargo.toml").write_text("[workspace]\n")

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp)

    def run_select(self, *args, ok=True):
        p = subprocess.run(["bash", str(self.lez / "scripts" / "lez-select.sh"), *args], capture_output=True,
                           text=True, env={"PATH": os.environ["PATH"]})
        if ok:
            self.assertEqual(p.returncode, 0, p.stderr)
        return p

    def snapshot(self) -> dict[str, bytes]:
        return {f: (self.lez / f).read_bytes() for f in self.files}

    def leftovers(self) -> list[str]:
        return [str(p) for p in self.lez.rglob("*") if p.suffix in (".orig", ".rej")]

    def check_tag(self, tag: str):
        self.assertEqual(self.run_select("--show").stdout.strip(), tag)
        zkvm, build, v1compat = ("3.1.0", "3.0.7", "2.2.3") if tag == "v0.4.0" else ("3.0.5", "3.0.5", "2.2.2")
        for d, want in [(d, tag) for d in self.manifests] + [(d, "v0.4.0" if tag == "v0.4.0" else "v0.4.0-rc3")
                                                              for d in self.pinned]:
            text = (self.lez / d / "Cargo.toml").read_text()
            trees = set(re.findall(r"upstream/lez-([A-Za-z0-9.+-]+)/", text))
            self.assertEqual(trees, {want}, f"{d} on {tag}")
            for line in re.findall(r'^risc0-zkvm = \{ version = "=([0-9.]+)"', text, re.M):
                self.assertEqual(line, zkvm, f"{d} risc0-zkvm on {tag}")
            for line in re.findall(r'^risc0-zkos-v1compat = "=([0-9.]+)"', text, re.M):
                self.assertEqual(line, v1compat, f"{d} risc0-zkos-v1compat on {tag}")
            loaders = re.findall(r'upstream/lez-[^/]+/([a-z_/]+program_loader[a-z_/]*)"', text)
            for path in loaders:
                self.assertEqual(path, "lee/program_loader" if tag == "v0.4.0" else "lez/programs/program_loader/core",
                                 f"{d} program_loader_core on {tag}")
        for d in self.methods:
            pins = re.findall(r'^risc0-build = \{ version = "=([0-9.]+)"', (self.lez / d / "Cargo.toml").read_text(),
                              re.M)
            self.assertEqual(pins, [build], f"{d} risc0-build on {tag}")
        feature = {"v0.4.0-rc1": "", "v0.4.0-rc2": '"lez-rc2"', "v0.4.0-rc3": '"lez-rc3"', "v0.4.0": '"lez-040"'}[tag]
        for d in self.featured:
            text = (self.lez / d / "Cargo.toml").read_text()
            self.assertIn(f"default = [{feature}] # lez-select\n", text, d)
            self.assertIn('lez-040 = ["lez-rc3"]\n', text, d)
            self.assertIn('lez-rc3 = ["lez-rc2"]\n', text, d)
        for d in self.sourced_040:  # the guests: their v0.4.0 twins on v0.4.0 only, the originals otherwise
            text = (self.lez / d / "Cargo.toml").read_text()
            paths = re.findall(r'^path = "([^"]+)" # lez-select$', text, re.M)
            self.assertEqual(len(paths), 1, d)
            twin = re.search(r"_lez040\.rs$", paths[0]) is not None
            self.assertEqual(twin, tag == "v0.4.0", f"{d} builds {paths[0]} on {tag}")
            self.assertNotIn("lez-040", text, f"{d}: a guest's feature would change its release-candidate build")
        self.assertEqual(self.leftovers(), [])

    def test_every_tag_and_back(self):
        start = self.run_select("--show").stdout.strip()
        if start != "v0.4.0-rc1":
            self.run_select("v0.4.0-rc1")
        self.check_tag("v0.4.0-rc1")
        rc1 = self.snapshot()
        seen = {}
        for tag in ["v0.4.0", "v0.4.0-rc3", "v0.4.0-rc2", "v0.4.0-rc1", "v0.4.0-rc3", "v0.4.0", "v0.4.0-rc2",
                    "v0.4.0", "v0.4.0-rc1"]:
            out = self.run_select(tag).stdout
            self.assertIn(f"-> {tag} (", out)
            self.check_tag(tag)
            snap = self.snapshot()
            if tag in seen:  # the same tag from another tag gives the same files
                self.assertEqual(snap, seen[tag], f"{tag} differs when selected again")
            seen[tag] = snap
            self.assertIn("already", self.run_select(tag).stdout)
            self.assertEqual(self.snapshot(), snap)
        self.assertEqual(self.snapshot(), rc1)
        changed = [f for f in self.files if f.endswith(".lock") and seen["v0.4.0"][f] != rc1[f]]
        self.assertIn("Cargo.lock", changed)
        self.assertIn("guests/program/Cargo.lock", changed)  # risc0-zkvm 3.1.0 in the guests' own lockfiles
        if start != "v0.4.0-rc1":
            self.run_select(start)

    def test_refusals(self):
        p = self.run_select("v0.3.9", ok=False)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("unknown LEZ tag v0.3.9", p.stderr)
        before = self.snapshot()
        tree = self.work / "upstream" / "lez-v0.4.0"
        try:
            subprocess.run(["git", "init", "-q", str(tree)], check=True)
            subprocess.run(["git", "-C", str(tree), "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q",
                            "--allow-empty", "-m", "x"], check=True)
            p = self.run_select("v0.4.0", ok=False)
            self.assertNotEqual(p.returncode, 0)
            self.assertIn("expected a255b42c0fc80d1cb219454a3a1f5ec3b5d92115 (v0.4.0)", p.stderr)
        finally:
            shutil.rmtree(tree / ".git")
        self.assertEqual(self.snapshot(), before, "a refused selection wrote files")

    def test_every_lez_manifest_listed(self):
        listed = set(self.manifests + self.pinned)
        tracked = subprocess.run(["git", "-C", str(LEZ), "ls-files", "--", "*Cargo.toml"], capture_output=True,
                                 text=True, check=True).stdout.split()
        for f in tracked:
            d = str(Path(f).parent)
            if re.search(r"upstream/lez-v0\.4\.0", (LEZ / f).read_text()) and d not in UNLISTED:
                self.assertIn(d, listed, f"lez/{f} has a LEZ path but lez-select.sh does not list it")

    def test_shared_sources_switch_alike(self):
        # A guest that builds another guest's source (`[[bin]] path = "../X/src/main.rs"`) must switch to that
        # source's v0.4.0 twin too, or its v0.4.0 build takes the release candidates' code.
        for d in self.manifests + self.pinned:
            m = re.search(r'^path = "\.\./([a-z0-9-]+)/src/main\.rs"', (LEZ / d / "Cargo.toml").read_text(), re.M)
            if m and f"guests/{m.group(1)}" in self.sourced_040:
                self.assertIn(d, self.sourced_040, f"lez/{d} builds guests/{m.group(1)}'s source")

    def test_every_twin_exists_and_is_current(self):
        for d in self.sourced_040:
            path = re.search(r'^path = "([^"]+)" # lez-select$', (LEZ / d / "Cargo.toml").read_text(), re.M).group(1)
            original = (LEZ / d / path).resolve()
            self.assertTrue(original.exists(), f"lez/{d}: {path}")
            self.assertTrue(original.with_name(f"{original.stem}_lez040.rs").exists(), f"lez/{d}: no v0.4.0 twin")
        p = subprocess.run(["python3", "-I", str(LEZ / "scripts" / "lez040-sources.py"), "--check"],
                           capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_commits_agree(self):
        commits = commits_in_script()
        self.assertEqual(sorted(commits), sorted(TAGS))
        fetch = (REPO / "scripts" / "fetch-upstream.sh").read_text()
        env = (LEZ / "live" / "env.sh").read_text()
        for tag, commit in commits.items():
            self.assertIn(f"{tag}:{commit}", fetch, f"scripts/fetch-upstream.sh: {tag}")
            self.assertIn(f'{tag}) LEZ_COMMIT="${{LEZ_COMMIT:-{commit}}}" ;;', env, f"lez/live/env.sh: {tag}")


if __name__ == "__main__":
    unittest.main()
