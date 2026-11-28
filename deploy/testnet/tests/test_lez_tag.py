#!/usr/bin/env python3
# Tests for deploy/testnet/lez-tag.sh. Run with:
#   python3 -I deploy/testnet/tests/test_lez_tag.py
# Reference files are found relative to this test, never downloaded: LEZ's trees beside the repository
# (upstream/lez-<tag>, scripts/fetch-upstream.sh) or copies in .scratch/upstream-lez-<tag>. Tests that need one skip when
# it is absent. Temporary files stay in this repository's ignored .scratch/ directory.
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
TESTNET = HERE.parent
ROOT = HERE.parents[2]
LIB = TESTNET / "lez-tag.sh"
SCRATCH = ROOT / ".scratch"
REFERENCES = ROOT / ".scratch"

TAGS = {
    "v0.4.0-rc3": {
        "commit": "bafd07be0701398f9bc435f73ab05e9a7936c301",
        "r0vm": "3.0.5",
        "bedrock": "0.3.5-rc.1",
        "import": "--private-key",
    },
    "v0.4.0": {
        "commit": "a255b42c0fc80d1cb219454a3a1f5ec3b5d92115",
        "r0vm": "3.1.0",
        "bedrock": "0.4.0-rc.2",
        "import": "--secret",
    },
}


def lez_shell(tag, script, *args, default=False, source=None, record=None):
    """Source lez-tag.sh in a fresh bash and run script with the supplied positional arguments. RECORD is the host's tag
    record (/etc/ferrata/lez-tag on a host), None for a host without one."""
    env = os.environ.copy()
    env.pop("FERRATA_LEZ_TAG", None)
    env.pop("FERRATA_LEZ_SRC", None)
    env["LEZ_TAG_LIB"] = str(LIB)
    env["FERRATA_LEZ_TAG_FILE"] = host_record(record)
    if not default:
        env["FERRATA_LEZ_TAG"] = tag
    if source is not None:
        env["FERRATA_LEZ_SRC"] = source
    return subprocess.run(
        ["bash", "-c", 'set -euo pipefail; . "$LEZ_TAG_LIB"; ' + script, "test-lez-tag", *args],
        capture_output=True,
        text=True,
        env=env,
        timeout=20,
    )


def host_record(record):
    """A host tag record file holding RECORD (or a path that does not exist, for None)."""
    SCRATCH.mkdir(exist_ok=True)
    path = SCRATCH / ("test-lez-tag-record-" + str(os.getpid()))
    if record is None:
        path.unlink(missing_ok=True)
    else:
        path.write_text(record + "\n")
    return str(path)


def public_keys(path):
    config = json.loads(path.read_text())
    return [
        action["supply_account"]["recipient"]["identified"]["key"]
        for action in config["genesis"]["actions"]
        if "supply_account" in action
    ]


class ReferenceTest(unittest.TestCase):
    def reference(self, tag, relative):
        # LEZ's tree beside the repository (scripts/fetch-upstream.sh), else a copy in .scratch/upstream-lez-<tag>
        for path in (ROOT.parent / "upstream" / ("lez-" + tag) / relative, REFERENCES / ("upstream-lez-" + tag) / relative):
            if path.is_file():
                return path
        self.skipTest("reference copy is absent: " + str(path))


class TemporaryTest(ReferenceTest):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="test-lez-tag-", dir=SCRATCH)
        self.directory = pathlib.Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()


class TagValues(ReferenceTest):
    def values(self, tag=None, source=None):
        result = lez_shell(
            tag,
            'printf "%s\\n" "$LEZ_TAG" "$LEZ_COMMIT" "$LEZ_SRC" "$R0VM_VERSION" '
            '"$BEDROCK_RELEASE" "$WALLET_IMPORT_FLAG"',
            default=tag is None,
            source=source,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.splitlines()

    def test_both_tags(self):
        for tag, expected in TAGS.items():
            with self.subTest(tag=tag):
                self.assertEqual(
                    self.values(tag),
                    [
                        tag,
                        expected["commit"],
                        "/opt/ferrata/src/upstream/lez-" + tag,
                        expected["r0vm"],
                        expected["bedrock"],
                        expected["import"],
                    ],
                )

    def test_unset_is_exactly_todays_rc3_values(self):
        self.assertEqual(
            self.values(),
            [
                "v0.4.0-rc3",
                "bafd07be0701398f9bc435f73ab05e9a7936c301",
                "/opt/ferrata/src/upstream/lez-v0.4.0-rc3",
                "3.0.5",
                "0.3.5-rc.1",
                "--private-key",
            ],
        )

    def test_source_override(self):
        for tag in TAGS:
            with self.subTest(tag=tag):
                self.assertEqual(self.values(tag, "/a/source/tree")[2], "/a/source/tree")

    def test_empty_is_unset(self):
        # as the host's tools have always read it (${FERRATA_LEZ_TAG:-...})
        self.assertEqual(self.values(""), self.values())

    def test_host_record(self):
        # install.sh records v0.4.0 on the host it moves; a command run there by hand without the variable takes it, and
        # a variable that disagrees with it is refused
        r = lez_shell(None, 'printf "%s\\n" "$LEZ_TAG" "$LEZ_COMMIT"', default=True, record="v0.4.0")
        self.assertEqual((r.returncode, r.stdout.split()), (0, ["v0.4.0", TAGS["v0.4.0"]["commit"]]), r.stderr)
        self.assertEqual(lez_shell("v0.4.0", 'echo "$LEZ_TAG"', record="v0.4.0").stdout.strip(), "v0.4.0")
        r = lez_shell("v0.4.0-rc3", ":", record="v0.4.0")
        self.assertEqual(r.returncode, 2)
        self.assertIn("this host runs LEZ v0.4.0", r.stderr)
        for bad in ("", "garbage"):   # a record that exists must name a tag: never taken for rc3
            r = lez_shell(None, 'echo "$LEZ_TAG"', default=True, record=bad)
            self.assertEqual((r.returncode, r.stdout), (2, ""), bad)
            self.assertIn("not v0.4.0-rc3 or v0.4.0", r.stderr)
        with open(TESTNET / "install.sh") as f:
            self.assertIn("echo v0.4.0 > /etc/ferrata/lez-tag", f.read())

    def test_bad_tag_is_an_error(self):
        for bad in ("v0.4.0-rc4", "v0.4.1", "v0.4.0 "):
            with self.subTest(tag=bad):
                result = lez_shell(bad, ":")
                self.assertEqual(result.returncode, 2)
                self.assertIn("FERRATA_LEZ_TAG", result.stderr)
                self.assertIn("v0.4.0-rc3 or v0.4.0", result.stderr)


class HostTagCopies(unittest.TestCase):
    """The host's tools read the tag as lez-tag.sh does (each carries its own copy: they are installed on their own):
    the variable, else the host's record, else rc3; an empty variable is unset; a disagreement is refused."""
    # (variable, record, the tag; or None and the refusal's words)
    CASES = ((None, None, "v0.4.0-rc3"), ("", None, "v0.4.0-rc3"), ("v0.4.0", None, "v0.4.0"), (None, "v0.4.0", "v0.4.0"),
             ("", "v0.4.0", "v0.4.0"), ("v0.4.0", "v0.4.0", "v0.4.0"), (None, "v0.4.0-rc3", "v0.4.0-rc3"),
             ("v0.4.0-rc3", "v0.4.0", None, "this host runs LEZ v0.4.0"),
             ("v0.4.1", None, None, "must be v0.4.0-rc3 or v0.4.0"),           # no unknown tag taken for rc3
             (None, "", None, "not v0.4.0-rc3 or v0.4.0"),                     # an empty record is not rc3
             (None, "garbage", None, "not v0.4.0-rc3 or v0.4.0"))

    def env(self, tag, record):
        env = {k: v for k, v in os.environ.items() if k != "FERRATA_LEZ_TAG"}
        env["FERRATA_LEZ_TAG_FILE"] = host_record(record)
        if tag is not None:
            env["FERRATA_LEZ_TAG"] = tag
        return env

    def test_bash_copies(self):
        for tool in ("host/bin/ferrata-bridge-configure", "host/bin/ferrata-sweep-published"):
            text = (TESTNET / tool).read_text()
            m = re.search(r"^lez_host_tag\(\) \{\n.*?^\}\n", text, re.MULTILINE | re.DOTALL)
            self.assertIsNotNone(m, tool)
            for tag, record, want, *said in self.CASES:
                with self.subTest(tool=tool, tag=tag, record=record):
                    r = subprocess.run(["bash", "-c", m.group(0) + "lez_host_tag"], capture_output=True, text=True,
                                       env=self.env(tag, record), timeout=20)
                    if want:
                        self.assertEqual((r.returncode, r.stdout.strip()), (0, want), r.stderr)
                    else:
                        self.assertEqual((r.returncode, r.stdout.strip()), (2, ""))
                        self.assertIn(said[0], r.stderr)

    def test_python_copies(self):
        probe = ("import importlib.machinery, importlib.util, sys\n"
                 "loader = importlib.machinery.SourceFileLoader('tool_under_test', sys.argv[1])\n"
                 "spec = importlib.util.spec_from_loader(loader.name, loader)\n"
                 "m = importlib.util.module_from_spec(spec); loader.exec_module(m)\n"
                 "print(m.lez_host_tag())\n")
        for tool in ("host/bin/ferrata-bridge-identity", "web-config.py", "host/bin/ferrata-gateway"):
            for tag, record, want, *said in self.CASES:
                with self.subTest(tool=tool, tag=tag, record=record):
                    r = subprocess.run([sys.executable, "-I", "-B", "-c", probe, str(TESTNET / tool)], capture_output=True,
                                       text=True, env=self.env(tag, record), timeout=20)
                    if want:
                        self.assertEqual((r.returncode, r.stdout.strip()), (0, want), r.stderr)
                    else:
                        self.assertNotEqual(r.returncode, 0)
                        self.assertIn(said[0], r.stderr)


class NodeConfig(TemporaryTest):
    def test_real_configs_keep_allowed_hosts_untouched(self):
        for tag in TAGS:
            with self.subTest(tag=tag):
                source = self.reference(tag, "bedrock/node-config.yaml")
                target = self.directory / (tag + ".yaml")
                original = source.read_text()
                target.write_text(original)
                result = lez_shell(tag, 'lez_node_config "$1" 18080', str(target))
                self.assertEqual(result.returncode, 0, result.stderr)
                expected = original.replace(
                    "listen_address: 0.0.0.0:18080", "listen_address: 127.0.0.1:18080"
                ).replace("    swarm:\n", "    swarm:\n      host: 127.0.0.1\n")
                self.assertEqual(target.read_text(), expected)
                before_hosts = re.findall(r"^\s*allowed_hosts:.*$", original, re.MULTILINE)
                after_hosts = re.findall(r"^\s*allowed_hosts:.*$", target.read_text(), re.MULTILINE)
                self.assertEqual(after_hosts, before_hosts)
                for line in after_hosts:
                    self.assertIsNone(re.search(r"(?<![0-9])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![0-9])", line))
                if tag == "v0.4.0":
                    self.assertEqual(after_hosts, ["    allowed_hosts: [logos-blockchain-node-0]"])

    def test_port_argument(self):
        target = self.directory / "custom.yaml"
        target.write_text("api:\n  backend:\n    listen_address: 0.0.0.0:18555\n")
        for tag in TAGS:
            with self.subTest(tag=tag):
                target.write_text("api:\n  backend:\n    listen_address: 0.0.0.0:18555\n")
                result = lez_shell(tag, 'lez_node_config "$1" 18555', str(target))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("listen_address: 127.0.0.1:18555", target.read_text())


class DeploymentSettings(ReferenceTest):
    def test_real_files_and_mismatches(self):
        for tag, expected in (("v0.4.0-rc3", "V1"), ("v0.4.0", "V2")):
            with self.subTest(tag=tag):
                path = self.reference(tag, "bedrock/deployment-settings.yaml")
                self.assertIn("  0: !" + expected + "\n", path.read_text())
                result = lez_shell(tag, 'lez_check_deployment "$1"', str(path))
                self.assertEqual(result.returncode, 0, result.stderr)
                other = "v0.4.0" if tag == "v0.4.0-rc3" else "v0.4.0-rc3"
                mismatch = lez_shell(other, 'lez_check_deployment "$1"', str(path))
                self.assertNotEqual(mismatch.returncode, 0)
                self.assertIn("expected 0: !", mismatch.stderr)


class DeploymentSettingsSpelling(TemporaryTest):
    def test_comment_and_indentation(self):
        for tag, era in (("v0.4.0-rc3", "V1"), ("v0.4.0", "V2")):
            path = self.directory / f"{tag}.yaml"
            path.write_text("eras:\n    0: !" + era + "   # the first era\n")
            self.assertEqual(lez_shell(tag, 'lez_check_deployment "$1"', str(path)).returncode, 0, tag)
            path.write_text("eras:\n  10: !" + era + "\n")
            self.assertNotEqual(lez_shell(tag, 'lez_check_deployment "$1"', str(path)).returncode, 0, tag)


class DeployForwarding(unittest.TestCase):
    def test_deploy_sh_forwards_the_tag(self):
        # deploy.sh runs the host's scripts over SSM: the chain stage (systemd-run), `run` and `programs` must carry the
        # tag, or the host builds and installs rc3 whatever the workstation asked for
        text = (TESTNET / "deploy.sh").read_text()
        self.assertIn('. "$HERE/lez-tag.sh"', text)
        self.assertIn("-E FERRATA_LEZ_TAG=$LEZ_TAG", text)
        lines = text.splitlines()
        start = next(i for i, l in enumerate(lines) if l.lstrip().startswith("run)"))
        run = "\n".join(lines[start:next(i for i in range(start, len(lines)) if lines[i].rstrip().endswith(";;")) + 1])
        ssms = [l for l in run.splitlines() if l.lstrip().startswith('ssm "')]
        self.assertEqual(len(ssms), 2, run)
        v040, ssm = ssms   # the v0.4.0 command (stops at a failed stage), then rc3's (as it was)
        self.assertEqual(ssm.count("FERRATA_LEZ_TAG=$LEZ_TAG"), 2, ssm)
        self.assertTrue(v040.lstrip().startswith('ssm "set -o pipefail; ${pre}'), v040)
        self.assertIn("| tail -n 40 || exit 1; done", v040)
        self.assertNotIn("pipefail", ssm)
        # on v0.4.0 an rc3 host's installed provision.sh (which predates lez-tag.sh) is replaced before fetch runs again
        self.assertIn('if [ "$LEZ_TAG" = v0.4.0 ]; then', run)
        self.assertIn("install -m 755 /opt/ferrata/src/ferrata/deploy/testnet/provision.sh /opt/ferrata/provision.sh", run)
        self.assertIn("install -m 644 /opt/ferrata/src/ferrata/deploy/testnet/lez-tag.sh /opt/ferrata/lez-tag.sh", run)
        self.assertTrue(ssm.lstrip().startswith('ssm "${pre}FERRATA_LEZ_TAG=$LEZ_TAG /opt/ferrata/provision.sh fetch'), ssm)
        programs = next(l for l in text.splitlines() if l.lstrip().startswith("programs)"))
        self.assertIn("FERRATA_LEZ_TAG=$LEZ_TAG /opt/ferrata/src/ferrata/deploy/testnet/deploy-programs.sh", programs)


class DeploySsmStatus(TemporaryTest):
    def run_sh(self, status):
        # deploy.sh sh against a stand-in `aws` whose SSM command ends with `status`
        bindir = self.directory / "bin"
        bindir.mkdir(exist_ok=True)
        aws = bindir / "aws"
        aws.write_text(
            "#!/bin/sh\n"
            'case "$*" in\n'
            '  *describe-stacks*) echo i-0abc ;;\n'
            '  *send-command*) echo cmd-1 ;;\n'
            '  *"--query Status"*) echo "$FAKE_STATUS" ;;\n'
            '  *get-command-invocation*) printf "%s\\tout\\terr\\n" "$FAKE_STATUS" ;;\n'
            "esac\n")
        aws.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if not k.startswith("FERRATA_")}
        env.update({"PATH": f"{bindir}:{env.get('PATH', '/usr/bin:/bin')}", "FAKE_STATUS": status})
        return subprocess.run(["bash", str(TESTNET / "deploy.sh"), "sh", "true"], capture_output=True, text=True,
                              env=env, timeout=60)

    def test_success_and_failure(self):
        self.assertEqual(self.run_sh("Success").returncode, 0)
        for status in ("Failed", "Cancelled", "TimedOut"):
            r = self.run_sh(status)
            self.assertNotEqual(r.returncode, 0, status)
            self.assertIn("the host's command ended " + status, r.stderr)


class GenesisKeys(TemporaryTest):
    def test_real_keys_and_cross_tag_refusal(self):
        real = {tag: public_keys(self.reference(tag, "lez/sequencer/service/networks/local.json")) for tag in TAGS}
        self.assertTrue(all(len(keys) >= 2 for keys in real.values()))
        for tag, keys in real.items():
            for key in keys:
                with self.subTest(tag=tag, key=key):
                    expected_length = 64 if tag == "v0.4.0-rc3" else 66
                    self.assertEqual(len(key), expected_length)
                    if tag == "v0.4.0":
                        self.assertTrue(key.startswith("00"))
                    result = lez_shell(tag, 'lez_genesis_key_ok "$1"', key)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    for other_tag, other_keys in real.items():
                        if other_tag != tag:
                            for other_key in other_keys:
                                refused = lez_shell(tag, 'lez_genesis_key_ok "$1"', other_key)
                                self.assertNotEqual(refused.returncode, 0)

    def test_malformed_keys_are_refused(self):
        for tag in TAGS:
            for bad in ("", "00", "gg" * 32, "ab" * 31, "ab" * 33):
                with self.subTest(tag=tag, key=bad):
                    self.assertNotEqual(lez_shell(tag, 'lez_genesis_key_ok "$1"', bad).returncode, 0)
            if tag == "v0.4.0":
                wrong_type = "01" + "ab" * 32
                with self.subTest(tag=tag, key=wrong_type):
                    self.assertNotEqual(lez_shell(tag, 'lez_genesis_key_ok "$1"', wrong_type).returncode, 0)


class R0vm(TemporaryTest):
    def make_r0vm(self, home, version, target="x86_64-unknown-linux-gnu"):
        binary = home / "extensions" / ("v" + version + "-cargo-risczero-" + target) / "r0vm"
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text("#!/bin/sh\n")
        binary.chmod(0o755)
        return binary

    def test_version_and_first_home(self):
        for tag, expected in TAGS.items():
            with self.subTest(tag=tag):
                first = self.directory / (tag + "-first")
                second = self.directory / (tag + "-second")
                other_version = "3.1.0" if expected["r0vm"] == "3.0.5" else "3.0.5"
                self.make_r0vm(first, other_version)
                wanted = self.make_r0vm(second, expected["r0vm"])
                result = lez_shell(tag, 'lez_find_r0vm "$1" "$2"', str(first), str(second))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), str(wanted))
                also = self.make_r0vm(first, expected["r0vm"], "another-target")
                result = lez_shell(tag, 'lez_find_r0vm "$1" "$2"', str(first), str(second))
                self.assertEqual(result.stdout.strip(), str(also))
                missing = lez_shell(tag, 'lez_find_r0vm "$1"', str(self.directory / "missing"))
                self.assertEqual(missing.returncode, 1)
                self.assertEqual(missing.stdout, "")


class DeploymentSources(ReferenceTest):
    def test_program_loader_deploy_flags_did_not_change(self):
        source = "lez/wallet/src/cli/programs/program_loader.rs"
        rc3 = self.reference("v0.4.0-rc3", source).read_bytes()
        v040 = self.reference("v0.4.0", source).read_bytes()
        self.assertEqual(v040, rc3)
        for flag in ("elf", "header", "segments", "immutable", "payer"):
            self.assertRegex(v040.decode(), r"#\[arg\(long[^]]*\)\]\s+" + flag + ":")

    def test_no_active_rc3_path_or_commit_outside_shared_library(self):
        literal = re.compile(r"lez-v0\.4\.0-rc3|bafd07be0701398f9bc435f73ab05e9a7936c301")
        failures = []
        # Executable deployment code only. Historical generated evidence and test fixtures are data, not
        # configuration used by the tag-selected deployment path. Their comments may also name old pins.
        for path in TESTNET.rglob("*"):
            if not path.is_file() or path == LIB or "tests" in path.parts or "test" in path.parts:
                continue
            is_script = path.suffix in (".sh", ".py") or path.parent == TESTNET / "host" / "bin"
            if not is_script:
                continue
            for number, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
                if line.lstrip().startswith("#"):
                    continue
                if literal.search(line):
                    failures.append(str(path.relative_to(ROOT)) + ":" + str(number))
        self.assertEqual(failures, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
