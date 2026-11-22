"""Tests of the release scripts (review-code-ops L3, M1; review-fix-supply M-1, M-2, L-11):
`python3 -I scripts/test_release_scripts.py`.

- `scripts/release.sh commit`: "+dirty" for a modified tracked file and for an untracked one (cargo and the build box
  use both), not for an ignored one; FERRATA_COMMIT wins; "unknown" outside git, with nothing on stderr.
- `scripts/reproduce-image-ids.sh build`: refuses an upstream LEZ tree that is missing, not a clone, or at another commit
  than v0.4.0-rc1's, before anything is built.
- `scripts/release.sh build`: a corrupted cached native archive stops it before anything is built (run with a PATH
  that has no cargo or forge, so nothing heavy could start even if the check failed open).
- The release path end to end (`build`, `package`, `verify`) in a copy of the scripts, with stub cargo, forge, rustc,
  r0vm, nm and guest builds, two tiny native archives (file:// URLs) and a small crate graph:
  - a good build writes BUILD-OK; `package` and `verify --strict` pass; the release carries the binary check-build
    passed, its record, the crate notices and the standard library's notices; an inherited CARGO_TARGET_DIR is ignored;
  - the review's case (review-fix-supply M-2): a CLI build that fails, or whose build script linked native code from
    its own download, leaves no BUILD-OK, no build/cli/ferrata and no native lines in toolchains.txt; `package`
    refuses it, and refuses a build directory whose cli/ferrata is another file even beside an earlier BUILD-OK; a
    failed build into the same directory removes the earlier BUILD-OK;
  - `verify --strict` fails a release whose CLI was swapped (SHA256SUMS and manifest rewritten to match), whose crate
    notices lost a crate, or whose binary has symbols of a crate the notices do not list; without r0vm, `verify` says
    the image ids were not recomputed and `--strict` fails.
Standard library only; bash, git, tar and gzip needed; files go to a temporary directory (TMPDIR).
"""

import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
LEZ_COMMIT = "1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def minimal_path(t, extra=()):
    """A PATH with only the commands the scripts' checks use: no cargo, forge, rustc, r0vm or nm."""
    b = os.path.join(t, "minbin")
    os.makedirs(b, exist_ok=True)
    for c in ("bash", "sh", "git", "python3", "sha256sum", "date", "cat", "tee", "mkdir", "dirname", "cut", "grep", "sed",
              "find", "sort", "xargs", "tail", "head", "ls", "tr", "wc", "env", "rm", "cp", "mv", "tar", "gzip", "uname",
              "nproc", "chmod", *extra):
        p = shutil.which(c)
        if p and not os.path.exists(os.path.join(b, c)):
            os.symlink(p, os.path.join(b, c))
    return b


def git(cwd, *args):
    return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false",
                           *args], cwd=cwd, capture_output=True, text=True, check=True).stdout.strip()


def sha(b):
    return hashlib.sha256(b).hexdigest()


def wr(p, data, mode=None):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb" if isinstance(data, bytes) else "w") as f:
        f.write(data)
    if mode:
        os.chmod(p, mode)


def rd(p, mode="r"):
    with open(p, mode) as f:
        return f.read()


class Commit(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.r = os.path.join(self.tmp.name, "tree")
        os.makedirs(os.path.join(self.r, "scripts"))
        shutil.copy(os.path.join(HERE, "release.sh"), os.path.join(self.r, "scripts", "release.sh"))
        with open(os.path.join(self.r, ".gitignore"), "w") as f:
            f.write("target/\n")
        git(self.r, "init", "-q")
        git(self.r, "add", ".")
        git(self.r, "commit", "-q", "-m", "x")
        self.head = git(self.r, "rev-parse", "HEAD")

    def tearDown(self):
        self.tmp.cleanup()

    def commit(self, env=None):
        e = {k: v for k, v in os.environ.items() if k != "FERRATA_COMMIT"}
        e.update(env or {})
        return subprocess.run(["bash", os.path.join(self.r, "scripts", "release.sh"), "commit"], capture_output=True,
                              text=True, env=e)

    def test_clean_modified_untracked_ignored(self):
        self.assertEqual(self.commit().stdout.strip(), self.head)
        with open(os.path.join(self.r, ".gitignore"), "a") as f:
            f.write("# changed\n")
        self.assertEqual(self.commit().stdout.strip(), self.head + "+dirty")
        git(self.r, "checkout", "--", ".gitignore")
        os.makedirs(os.path.join(self.r, "src", "bin"))
        with open(os.path.join(self.r, "src", "bin", "extra.rs"), "w") as f:
            f.write("fn main() {}\n")          # untracked: cargo would build it
        self.assertEqual(self.commit().stdout.strip(), self.head + "+dirty")
        shutil.rmtree(os.path.join(self.r, "src"))
        os.makedirs(os.path.join(self.r, "target"))
        with open(os.path.join(self.r, "target", "x"), "w") as f:
            f.write("ignored\n")
        self.assertEqual(self.commit().stdout.strip(), self.head)

    def test_ferrata_commit_and_no_git(self):
        self.assertEqual(self.commit({"FERRATA_COMMIT": "abc+dirty"}).stdout.strip(), "abc+dirty")
        shutil.rmtree(os.path.join(self.r, ".git"))
        r = self.commit({"GIT_CEILING_DIRECTORIES": self.tmp.name})
        self.assertEqual(r.stdout.strip(), "unknown")
        self.assertEqual(r.stderr, "")                       # no "fatal: not a git repository" (review nit)


class ReproduceUpstream(unittest.TestCase):
    """reproduce-image-ids.sh build in a copy laid out as tree/ beside upstream/ and risc0-home-lez/."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = self.t = self.tmp.name
        os.makedirs(os.path.join(t, "tree", "scripts"))
        shutil.copy(os.path.join(HERE, "reproduce-image-ids.sh"), os.path.join(t, "tree", "scripts"))
        os.makedirs(os.path.join(t, "risc0-home-lez"))
        self.path = minimal_path(t)

    def tearDown(self):
        self.tmp.cleanup()

    def build(self):
        return subprocess.run(["bash", os.path.join(self.t, "tree", "scripts", "reproduce-image-ids.sh"), "build", "--out",
                               os.path.join(self.t, "out")], capture_output=True, text=True,
                              env={"PATH": self.path, "HOME": self.t, "GIT_CEILING_DIRECTORIES": self.t})

    def test_missing_upstream(self):
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("missing", r.stdout)

    def test_upstream_not_a_clone(self):
        os.makedirs(os.path.join(self.t, "upstream", "lez-v0.4.0-rc1"))
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("cannot read the commit", r.stdout)

    def test_upstream_at_another_commit(self):
        u = os.path.join(self.t, "upstream", "lez-v0.4.0-rc1")
        os.makedirs(u)
        git(u, "init", "-q")
        with open(os.path.join(u, "README.md"), "w") as f:
            f.write("not LEZ v0.4.0-rc1\n")
        git(u, "add", ".")
        git(u, "commit", "-q", "-m", "x")
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn(f"is at {git(u, 'rev-parse', 'HEAD')}, not {LEZ_COMMIT}", r.stdout)
        self.assertFalse(os.path.exists(os.path.join(self.t, "out", "logs", "lez-methods.log")))  # nothing was built


class ReleaseBuildNative(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.t = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def test_corrupted_cached_archive_stops_the_build(self):
        fn = load("fetch_native", os.path.join(HERE, "fetch_native.py"))
        pins = fn.load_pins(fn.PINS)
        cache = os.path.join(self.t, "native")
        arc = fn.archive_path(cache, pins["archives"][0])
        os.makedirs(os.path.dirname(arc))
        with open(arc, "wb") as f:              # the pinned size, other bytes: a replaced asset in the cache
            f.write(b"\0" * pins["archives"][0]["bytes"])
        out = os.path.join(self.t, "rel")
        r = subprocess.run(["bash", os.path.join(HERE, "release.sh"), "build", out], capture_output=True, text=True,
                           env={"PATH": minimal_path(self.t), "HOME": self.t, "FERRATA_NATIVE_DIR": cache,
                                "FERRATA_COMMIT": "test"}, timeout=120)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("corrupted or was replaced; it was not used", r.stderr)
        with open(os.path.join(out, "build", "logs", "steps.txt")) as f:
            steps = f.read()
        self.assertIn("native archives FAILED", steps)
        for log in ("guests.log", "vault.log", "cli.log"):     # nothing was started
            self.assertFalse(os.path.exists(os.path.join(out, "build", "logs", log)), log)


# ---------------------------------------------------------------------------------------------------------------
# the release path end to end, with stubs
# ---------------------------------------------------------------------------------------------------------------
STUB_CARGO = r'''#!{python}
# stub cargo for scripts/test_release_scripts.py
import json, os, sys
F = os.environ["STUB_FIXTURE"]
args = sys.argv[1:]
if args[:1] == ["tree"]:
    sys.stdout.write(open(os.path.join(F, "tree.txt")).read()); sys.exit(0)
if args[:1] == ["metadata"]:
    sys.stdout.write(open(os.path.join(F, "metadata.json")).read()); sys.exit(0)
if args[:1] != ["build"]:
    sys.exit("stub cargo: " + " ".join(args))
mode = os.environ.get("STUB_CARGO", "ok")
if mode == "fail":
    print("error[E0308]: mismatched types (stub)", file=sys.stderr)
    print(json.dumps({{"reason": "build-finished", "success": False}}))
    sys.exit(101)
target = os.path.join(os.environ.get("CARGO_TARGET_DIR") or os.path.join(os.getcwd(), "target"), "release")
def bse(pkg, paths, libs):
    out = os.path.join(target, "build", pkg + "-1", "out")
    os.makedirs(out, exist_ok=True)
    return {{"reason": "build-script-executed", "package_id": "git+https://example.invalid/x#" + pkg + "@0.1.0",
            "linked_paths": ["native=" + p for p in paths], "linked_libs": libs, "cfgs": [], "env": [], "out_dir": out}}
rs = os.environ.get("RAPIDSNARK_LIB_DIR", "")
if mode == "unchecked":           # the build script fetched its own copy: the variables were not honoured
    rs = os.path.join(target, "build", "rust-rapidsnark-1", "out")
lbc = os.environ.get("LBC_ROOT_DIR", "")
msgs = [bse("rust-rapidsnark", [rs], ["static=rapidsnark", "static=gmp", "stdc++"]),
        bse("circuits-pol-sys", [os.path.join(lbc, "pol"), os.path.join(lbc, "lib")], ["static=pol", "static=gmp"])]
meta = json.load(open(os.path.join(F, "metadata.json")))
for p in meta["packages"]:
    kinds = p["targets"][0]["kind"] if p["targets"] else ["lib"]
    if "bin" not in kinds:
        msgs.append({{"reason": "compiler-artifact", "package_id": p["id"], "target": {{"kind": kinds, "name": p["name"]}}}})
exe = os.path.join(target, "ferrata")
os.makedirs(target, exist_ok=True)
open(exe, "wb").write(os.environ.get("STUB_BINARY", "\x7fELF stub ferrata").encode())
os.chmod(exe, 0o755)
msgs.append({{"reason": "compiler-artifact", "package_id": meta["packages"][0]["id"],
             "target": {{"kind": ["bin"], "name": "ferrata"}}, "executable": exe}})
msgs.append({{"reason": "build-finished", "success": True}})
for m in msgs:
    print(json.dumps(m))
'''

STUB_FORGE = r'''#!{python}
import json, os, sys
args = sys.argv[1:]
if args[:1] == ["--version"]:
    print("forge 0.0.0-stub"); sys.exit(0)
out = args[args.index("--out") + 1]
art = {{"bytecode": {{"object": "0x6001600055", "linkReferences": {{}}}}, "deployedBytecode": {{"object": "0x600160"}},
       "metadata": {{"compiler": {{"version": "0.8.26"}}, "settings": {{"evmVersion": "cancun"}}}}}}
for n in ("FerrataVault", "FerrataRegistry"):
    os.makedirs(os.path.join(out, n + ".sol"), exist_ok=True)
    json.dump(art, open(os.path.join(out, n + ".sol", n + ".json"), "w"))
'''

STUB_RUSTC = r'''#!/bin/sh
case "$1" in
  --version) echo "rustc 0.0.0 (stub)" ;;
  --print) echo "{sysroot}" ;;
  *) exit 2 ;;
esac
'''

STUB_R0VM = r'''#!{python}
import hashlib, sys
a = sys.argv[1:]
if a[:1] == ["--version"]:
    print("risc0-r0vm 3.0.5 (stub)"); sys.exit(0)
print("0x" + hashlib.sha256(open(a[a.index("--elf") + 1], "rb").read()).hexdigest())
'''

STUB_NM = r'''#!/bin/sh
printf '%s\n' '0000000000001000 T core::fmt::write' '0000000000001010 T std::rt::lang_start' \
  '0000000000001020 T <serde::de::Foo as core::fmt::Debug>::fmt' '0000000000001030 T ferrata::main'
[ -n "$STUB_NM_EXTRA" ] && printf '%s\n' "0000000000001040 T $STUB_NM_EXTRA"
exit 0
'''

STUB_GIT = r'''#!/bin/sh
case "$*" in
  *lez-v0.4.0-rc1*rev-parse*) echo "{lez}"; exit 0 ;;
esac
exec "{git}" "$@"
'''

STUB_REPRODUCE = r'''#!{python}
# stub scripts/reproduce-image-ids.sh build --out DIR: six programs, their ids the sha256 of their bytes (as stub r0vm)
import hashlib, json, os, sys
out = sys.argv[sys.argv.index("--out") + 1]
os.makedirs(os.path.join(out, "programs"), exist_ok=True)
os.makedirs(os.path.join(out, "r0bf"), exist_ok=True)
h = lambda b: hashlib.sha256(b).hexdigest()
ids = []
for f in ("ferrata_lez.bin", "ferrata_bridge.bin"):
    b = ("lez program " + f).encode(); open(os.path.join(out, "programs", f), "wb").write(b)
    ids.append(f"{{f}} {{h(b)}} {{h(b)}}")
open(os.path.join(out, "ids.txt"), "w").write("\n".join(ids) + "\n")
with open(os.path.join(out, "r0bf", "ids.jsonl"), "w") as j:
    for f in ("checkpoint.bin", "checkpoint_b.bin", "segment.bin", "release.bin"):
        b = ("r0bf program " + f).encode(); open(os.path.join(out, "r0bf", f), "wb").write(b)
        j.write(json.dumps({{"program": f, "image_id": "0x" + h(b), "sha256": h(b)}}) + "\n")
'''


class ReleasePath(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tfn = load("test_fetch_native", os.path.join(HERE, "test_fetch_native.py"))
        cls.trm = load("test_release_manifest", os.path.join(HERE, "test_release_manifest.py"))

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        t = self.t = os.path.realpath(self.tmp.name)
        tree = self.tree = os.path.join(t, "tree")
        for f in ("release.sh", "fetch_native.py", "release_manifest.py", "crate_notices.py", "vault_libs.py"):
            wr(os.path.join(tree, "scripts", f), rd(os.path.join(HERE, f), "rb"), 0o755)
        py = sys.executable
        wr(os.path.join(tree, "scripts", "reproduce-image-ids.sh"), STUB_REPRODUCE.format(python=py), 0o755)
        # Two tiny native archives at file:// URLs, their pins, the notices and texts that cover them.
        rs = {"rs/lib/librapidsnark.a": b"rapidsnark test", "rs/lib/libgmp.a": b"gmp test"}
        lbc = {"lbc/lib/libgmp.a": b"gmp 6.2.1 test", "lbc/pol/libpol.a": b"pol test"}
        origin = os.path.join(t, "origin")
        rs_b, lbc_b, src_b = self.tfn.make_zip(rs), self.tfn.make_tgz(lbc), b"source tarball test\n"
        for name, b in (("rs.zip", rs_b), ("lbc.tar.gz", lbc_b), ("src-1.tar.xz", src_b)):
            wr(os.path.join(origin, name), b)
        comp = lambda n, lic, text: {"name": n, "version": "1", "licence": lic, "in": "lib", "source": "x", "texts": [text]}
        pins = {"archives": [
            {"name": "rapidsnark", "version": "1", "url": "file://" + os.path.join(origin, "rs.zip"), "bytes": len(rs_b),
             "sha256": sha(rs_b), "format": "zip", "env": {"RAPIDSNARK_LIB_DIR": "rs/lib"},
             "build_packages": ["rust-rapidsnark"], "static_libs": ["rapidsnark", "gmp"],
             "files": {k: sha(v) for k, v in rs.items()}, "components": [comp("test rapidsnark", "LGPL-3.0-only", "LGPL-3.0.txt")]},
            {"name": "circuits", "version": "1", "url": "file://" + os.path.join(origin, "lbc.tar.gz"), "bytes": len(lbc_b),
             "sha256": sha(lbc_b), "format": "tar.gz", "env": {"LBC_ROOT_DIR": "lbc"}, "build_packages": ["circuits-pol-sys"],
             "static_libs": ["pol", "gmp"], "files": {k: sha(v) for k, v in lbc.items()},
             "components": [comp("test circuits", "MIT", "MIT-test.txt")]}],
            "compiled_from_crate_sources": {"cli": {}},
            "sources": [{"name": "src-1", "url": "file://" + os.path.join(origin, "src-1.tar.xz"), "bytes": len(src_b),
                         "sha256": sha(src_b)}]}
        wr(os.path.join(tree, "scripts", "native-deps.json"), json.dumps(pins))
        wr(os.path.join(tree, "releases", "third-party", "THIRD-PARTY-NOTICES.md"),
           f"# test notices\n\n`rs.zip` {sha(rs_b)}: test rapidsnark `LGPL-3.0-only` licenses/LGPL-3.0.txt\n"
           f"`lbc.tar.gz` {sha(lbc_b)}: test circuits `MIT` licenses/MIT-test.txt\n")
        wr(os.path.join(tree, "releases/third-party/licenses/LGPL-3.0.txt"),
           rd(os.path.join(ROOT, "releases/third-party/licenses/LGPL-3.0.txt"), "rb"))
        wr(os.path.join(tree, "releases/third-party/licenses/MIT-test.txt"), "MIT (test)\n")
        for f in ("LICENSE-MIT", "LICENSE-APACHE"):
            shutil.copy(os.path.join(ROOT, f), os.path.join(tree, f))
        wr(os.path.join(tree, "vault", "lib.lock"), "# no libraries in the test\n")
        for f in ("vault/script/DeployVault.s.sol", "vault/script/deploy.sh", "vault/script/params/test.json"):
            wr(os.path.join(tree, f), "{}\n")
        # The crate graph: cli/Cargo.lock, and what the stub cargo answers.
        crates_mod = load("crate_notices_tree", os.path.join(tree, "scripts", "crate_notices.py"))
        meta, tree_text, lock = self.trm.crate_graph(t, os.path.join(tree, "cli", "Cargo.toml"))
        self.assertTrue(crates_mod.is_own(meta["packages"][0]))
        wr(os.path.join(tree, "cli", "Cargo.lock"), lock)
        wr(os.path.join(tree, "cli", "Cargo.toml"), "[package]\nname = \"ferrata-cli\"\n")
        fixture = os.path.join(t, "fixture")
        wr(os.path.join(fixture, "metadata.json"), json.dumps(meta))
        wr(os.path.join(fixture, "tree.txt"), tree_text)
        sysroot = os.path.join(t, "sysroot")
        wr(os.path.join(sysroot, "share", "doc", "rust", "COPYRIGHT-library.html"), self.trm.STD_HTML)
        os.makedirs(os.path.join(t, "upstream", "lez-v0.4.0-rc1"))
        b = self.bin = os.path.join(t, "bin")
        wr(os.path.join(b, "cargo"), STUB_CARGO.format(python=py), 0o755)
        wr(os.path.join(b, "forge"), STUB_FORGE.format(python=py), 0o755)
        wr(os.path.join(b, "rustc"), STUB_RUSTC.format(sysroot=sysroot), 0o755)
        wr(os.path.join(b, "r0vm"), STUB_R0VM.format(python=py), 0o755)
        wr(os.path.join(b, "nm"), STUB_NM, 0o755)
        wr(os.path.join(b, "git"), STUB_GIT.format(lez=LEZ_COMMIT, git=shutil.which("git")), 0o755)
        self.env = {"PATH": b + os.pathsep + minimal_path(t), "HOME": t, "FERRATA_NATIVE_DIR": os.path.join(t, "native"),
                    "FERRATA_COMMIT": "abc123", "STUB_FIXTURE": fixture, "GIT_CEILING_DIRECTORIES": t}
        self.out = os.path.join(t, "out")

    def rel(self, *args, **env):
        e = dict(self.env)
        e.update(env)
        return subprocess.run(["bash", os.path.join(self.tree, "scripts", "release.sh"), *args], capture_output=True,
                              text=True, env=e, timeout=120)

    def build(self, **env):
        return self.rel("build", self.out, **env)

    def package(self, ver="9.9.9"):
        return self.rel("package", self.out, ver)

    def verify(self, *extra, ver="9.9.9", **env):
        return self.rel("verify", os.path.join(self.out, f"ferrata-{ver}"), *extra, **env)

    def b(self, *rel):
        return os.path.join(self.out, "build", *rel)

    def test_good_release(self):
        r = self.build(CARGO_TARGET_DIR=os.path.join(self.t, "elsewhere"))      # L-11: an inherited target dir
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr + rd(self.b("logs", "cli.log")))
        self.assertIn("BUILD-OK", rd(self.b("logs", "steps.txt")))
        self.assertTrue(os.path.isfile(os.path.join(self.tree, "cli", "target", "release", "ferrata")))
        self.assertFalse(os.path.exists(os.path.join(self.t, "elsewhere")))
        self.assertIn("native_rapidsnark 1 sha256", rd(self.b("toolchains.txt")))
        r = self.package()
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        d = os.path.join(self.out, "ferrata-9.9.9")
        for f in ("cli/ferrata", "cli/check-build.json", "cli/crates.tsv", "cli/Cargo.lock", "THIRD-PARTY-CRATES.md",
                  "THIRD-PARTY-NOTICES.md", "licenses/rust-std-COPYRIGHT-library.html", "MANIFEST.json"):
            self.assertTrue(os.path.isfile(os.path.join(d, f)), f)
        m = json.loads(rd(os.path.join(d, "MANIFEST.json")))
        self.assertEqual(m["build"]["cli_sha256"], sha(b"\x7fELF stub ferrata"))
        self.assertEqual(m["commit"], "abc123")
        r = self.verify("--strict")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("image ids recomputed with r0vm: match", r.stdout)
        self.assertIn("is the binary check-build passed", r.stdout)
        r = self.verify(PATH=os.pathsep.join(p for p in self.env["PATH"].split(os.pathsep) if p != self.bin)
                        + os.pathsep + self.no_r0vm_bin())
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("image ids NOT recomputed", r.stdout)
        r = self.verify("--strict", PATH=self.no_r0vm_bin() + os.pathsep + minimal_path(self.t))
        self.assertEqual(r.returncode, 1)
        self.assertIn("r0vm not found", r.stderr)

    def no_r0vm_bin(self):
        """The stubs without r0vm (nm stays)."""
        d = os.path.join(self.t, "bin-no-r0vm")
        if not os.path.isdir(d):
            os.makedirs(d)
            shutil.copy(os.path.join(self.bin, "nm"), os.path.join(d, "nm"))
        return d

    def assert_not_released(self, r, why):
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        steps = rd(self.b("logs", "steps.txt"))
        self.assertIn("cli FAILED", steps)
        self.assertIn("no BUILD-OK", steps)
        self.assertIn(why, rd(self.b("logs", "cli.log")))
        self.assertFalse(os.path.exists(self.b("BUILD-OK")))
        self.assertFalse(os.path.exists(self.b("cli", "ferrata")))
        self.assertNotIn("native_", rd(self.b("toolchains.txt")))
        p = self.package()
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("has no BUILD-OK", p.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.out, "ferrata-9.9.9")))

    def test_failed_cli_build_is_not_released(self):
        self.assert_not_released(self.build(STUB_CARGO="fail"), "mismatched types")

    def test_unchecked_native_link_is_not_released(self):
        self.assert_not_released(self.build(STUB_CARGO="unchecked"), "rust-rapidsnark: links from")

    def test_the_reviewers_case(self):
        """review-fix-supply M-2: steps.txt says "cli FAILED", cli/ferrata is another file; with an earlier BUILD-OK."""
        self.assertEqual(self.build().returncode, 0)
        good = rd(self.b("BUILD-OK"))
        self.assertEqual(self.build(STUB_CARGO="fail").returncode, 1)
        self.assertFalse(os.path.exists(self.b("BUILD-OK")))            # the earlier BUILD-OK went with the new build
        wr(self.b("cli", "ferrata"), b"not the checked binary\n")
        wr(self.b("BUILD-OK"), good)                                     # put back by hand
        p = self.package()
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("is not the one the build checked", p.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.out, "ferrata-9.9.9")))

    def tampered(self, rel, data=None, edit=None, manifest_key=None):
        """A good release with one file changed, its SHA256SUMS line and manifest entry rewritten to match."""
        self.assertEqual(self.build().returncode, 0)
        self.assertEqual(self.package().returncode, 0)
        d = os.path.join(self.out, "ferrata-9.9.9")
        p = os.path.join(d, rel)
        wr(p, data if data is not None else edit(rd(p)))
        new = sha(rd(p, "rb"))
        sums = os.path.join(d, "SHA256SUMS")
        wr(sums, "".join(f"{new}  {rel}\n" if l.endswith(f"  {rel}\n") else l for l in rd(sums).splitlines(True)))
        if manifest_key:
            mp = os.path.join(d, "MANIFEST.json")
            m = json.loads(rd(mp))
            e = m[manifest_key] if manifest_key in m else m["third_party"][manifest_key]
            e["bytes"], e["sha256"] = os.path.getsize(p), new
            wr(mp, json.dumps(m))
            msha = sha(rd(mp, "rb"))
            wr(sums, "".join(f"{msha}  MANIFEST.json\n" if l.endswith("  MANIFEST.json\n") else l
                             for l in rd(sums).splitlines(True)))
        return self.verify("--strict")

    def test_verify_refuses_a_swapped_cli(self):
        r = self.tampered("cli/ferrata", data=b"\x7fELF another binary", manifest_key="cli")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("is not the binary check-build passed", r.stdout)

    def test_verify_refuses_notices_without_a_crate(self):
        r = self.tampered("THIRD-PARTY-CRATES.md", manifest_key="crates_notices",
                          edit=lambda s: "".join(l for l in s.splitlines(True) if not l.startswith("| mitonly |")))
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("does not list mitonly 0.1.0", r.stdout)

    def test_verify_refuses_a_binary_with_an_unlisted_crate(self):
        self.assertEqual(self.build().returncode, 0)
        self.assertEqual(self.package().returncode, 0)
        r = self.verify("--strict", STUB_NM_EXTRA="notlinked::f")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("symbols of crate notlinked", r.stderr)


if __name__ == "__main__":
    unittest.main()
