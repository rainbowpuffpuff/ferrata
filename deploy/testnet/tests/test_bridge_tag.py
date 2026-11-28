#!/usr/bin/env python3
"""Tag-selected bridge pins and labels. Run: python3 -I deploy/testnet/tests/test_bridge_tag.py."""
import base64
import contextlib
import hashlib
import importlib.machinery
import importlib.util
import io
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest
import warnings
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
TESTNET = HERE.parent
ROOT = HERE.parents[2]
SCRATCH = ROOT / ".scratch"
INSTALLER = TESTNET / "install-bridge.sh"
CONFIGURE = TESTNET / "host" / "bin" / "ferrata-bridge-configure"
IDENTITY = TESTNET / "host" / "bin" / "ferrata-bridge-identity"
BINARIES = ("ferrata-relayer", "monitor", "crown-driver", "realblocks-crown", "ferrata_attestor")
CHECKPOINT = "checkpoint_b_0_4_0.bin"
PROGRAMS = ("release.bin", CHECKPOINT)


class BridgeTemporaryTest(unittest.TestCase):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="test-bridge-tag-", dir=SCRATCH)
        self.directory = pathlib.Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()


class InstallerHarness(BridgeTemporaryTest):
    def setUp(self):
        super().setUp()
        self.bin_dir = self.directory / "bin"
        self.programs_dir = self.directory / "programs"
        self.bin_dir.mkdir()
        self.programs_dir.mkdir()
        fake_bin = self.directory / "fake-bin"
        fake_bin.mkdir()
        self.install_log = self.directory / "install.log"
        # All seven v0.4.0 checks precede the first install. Stop there, before foundry/curl or any host path is used.
        install = fake_bin / "install"
        install.write_text("#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$FAKE_INSTALL_LOG\"\nexit 93\n")
        install.chmod(0o755)
        self.path = str(fake_bin) + os.pathsep + os.environ.get("PATH", os.defpath)
        for name in BINARIES:
            (self.bin_dir / name).write_bytes(("v0.4.0 binary payload: " + name + "\n").encode())
        for name in PROGRAMS:
            (self.programs_dir / name).write_bytes(("v0.4.0 program payload: " + name + "\n").encode())

    def pin_line(self, name):
        path = self.bin_dir / name if name in BINARIES else self.programs_dir / name
        return hashlib.sha256(path.read_bytes()).hexdigest() + "  " + name

    def write_pins(self, omit=None, duplicate=None):
        names = [PROGRAMS[1], *BINARIES[:2], PROGRAMS[0], *BINARIES[2:]]
        lines = [self.pin_line(name) for name in names if name != omit]
        if duplicate is not None:
            lines.append(self.pin_line(duplicate))
        path = self.directory / "SHA256SUMS"
        path.write_text("\n".join(lines) + "\n")
        return path

    def run_installer(self, tag="v0.4.0", pins=None, checkpoint=CHECKPOINT, image_b="0x" + "bb" * 32, release_id="0x" + "cc" * 32,
                      image_a="0x" + "aa" * 32):
        env = os.environ.copy()
        for k in ("FERRATA_LEZ_TAG", "FERRATA_BRIDGE_PINS", "FERRATA_BRIDGE_CHECKPOINT_B", "FERRATA_CHECKPOINT_IMAGE_B",
                  "FERRATA_RELEASE_IMAGE_ID", "FERRATA_CHECKPOINT_IMAGE_A"):
            env.pop(k, None)
        env["FERRATA_LEZ_TAG_FILE"] = str(self.directory / "no-host-tag")
        if image_a is not None:
            env["FERRATA_CHECKPOINT_IMAGE_A"] = image_a
        if image_b is not None:
            env["FERRATA_CHECKPOINT_IMAGE_B"] = image_b
        if release_id is not None:
            env["FERRATA_RELEASE_IMAGE_ID"] = release_id
        env.update({"PATH": self.path, "FAKE_INSTALL_LOG": str(self.install_log), "TMPDIR": str(self.directory)})
        if tag is not None:
            env["FERRATA_LEZ_TAG"] = tag
        if pins is not None:
            env["FERRATA_BRIDGE_PINS"] = str(pins)
        if checkpoint is not None:
            env["FERRATA_BRIDGE_CHECKPOINT_B"] = checkpoint
        return subprocess.run(
            [str(INSTALLER), str(self.bin_dir), str(self.programs_dir), "0x" + "11" * 20],
            cwd=ROOT, env=env, capture_output=True, text=True, timeout=20,
        )


class InstallerPins(InstallerHarness):
    def test_rc3_pins_are_the_base_s(self):
        # the seven rc3 pins as the base fb27c1e2 has them (the sha256 of their lines in order, so the check needs no
        # history: CI's checkout is shallow)
        pin = re.compile(
            r"^[0-9a-f]{64}  (?:ferrata-relayer|monitor|crown-driver|realblocks-crown|ferrata_attestor)$"
            r"|(?:release\.bin|checkpoint_b_0_3_5\.bin):[0-9a-f]{64}", re.MULTILINE,
        )
        after = pin.findall(INSTALLER.read_text())
        self.assertEqual(len(after), 7, after)
        self.assertEqual(hashlib.sha256(("\n".join(after) + "\n").encode()).hexdigest(),
                         "395d238c2b630115ea1ec74b7ed26cfb253d2feb28e3b7f1121be87879984c80")

    def test_v040_requires_a_readable_pins_file(self):
        for pins in (None, self.directory / "absent", self.directory):
            with self.subTest(pins=pins):
                result = self.run_installer(pins=pins)
                self.assertEqual(result.returncode, 2, result)
                self.assertIn("FERRATA_BRIDGE_PINS", result.stderr)
                self.assertIn("readable", result.stderr)

    def test_v040_requires_the_rule_b_filename(self):
        result = self.run_installer(pins=self.write_pins(), checkpoint=None)
        self.assertEqual(result.returncode, 2, result)
        self.assertIn("FERRATA_BRIDGE_CHECKPOINT_B", result.stderr)

    def test_v040_refuses_missing_and_duplicate_names(self):
        for omit, duplicate, expected in (
            ("monitor", None, "'monitor' exactly once"),
            (None, "monitor", "'monitor' exactly once"),
            ("release.bin", None, "'release.bin' exactly once"),
        ):
            with self.subTest(omit=omit, duplicate=duplicate):
                result = self.run_installer(pins=self.write_pins(omit=omit, duplicate=duplicate))
                self.assertEqual(result.returncode, 2, result)
                self.assertIn(expected, result.stderr)

    def test_v040_refuses_malformed_or_unexpected_entries(self):
        for line, message in (("not a checksum", "sha256sum format"), ("00" * 32 + "  other.bin", "unexpected artifact")):
            with self.subTest(line=line):
                pins = self.write_pins()
                with pins.open("a") as f:
                    f.write(line + "\n")
                result = self.run_installer(pins=pins)
                self.assertEqual(result.returncode, 2, result)
                self.assertIn(message, result.stderr)

    def test_good_v040_pins_are_checked_before_install(self):
        result = self.run_installer(pins=self.write_pins())
        self.assertEqual(result.returncode, 93, result.stdout + result.stderr)
        output = result.stdout + result.stderr
        for name in (*BINARIES, *PROGRAMS):
            with self.subTest(name=name):
                self.assertEqual(output.count(name + ": OK"), 1, output)
        self.assertNotIn("checkpoint_b_0_3_5.bin: OK", output)
        commands = self.install_log.read_text().splitlines()
        self.assertEqual(len(commands), 1, commands)
        self.assertTrue(commands[0].startswith("-m 755 "), commands)
        for name in BINARIES:
            self.assertIn(str(self.bin_dir / name), commands[0])


class InstallerProgramIds(InstallerHarness):
    def test_v040_needs_the_rule_b_image_id(self):
        pins = self.write_pins()
        for bad in (None, "", "0x1234", "bb" * 32):
            result = self.run_installer(pins=pins, image_b=bad)
            self.assertEqual(result.returncode, 2, (bad, result.stderr))
            self.assertIn("FERRATA_CHECKPOINT_IMAGE_B", result.stderr)
            self.assertFalse(self.install_log.exists() and self.install_log.read_text(), "installed before refusing")

    def test_v040_needs_the_rule_a_image_id_before_installing(self):
        # ferrata-bridge-configure, run at the installer's end, needs it: refused before the first install
        pins = self.write_pins()
        for bad in (None, "0x1234"):
            result = self.run_installer(pins=pins, image_a=bad)
            self.assertEqual(result.returncode, 2, (bad, result.stderr))
            self.assertIn("FERRATA_CHECKPOINT_IMAGE_A", result.stderr)
            self.assertFalse(self.install_log.exists() and self.install_log.read_text(), "installed before refusing")

    def test_v040_release_id_needed_unless_rc3s_release(self):
        pins = self.write_pins()
        result = self.run_installer(pins=pins, release_id=None)   # the fake release.bin is not rc3's
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("FERRATA_RELEASE_IMAGE_ID", result.stderr)

    def test_prover_programs_table(self):
        # the function the installer writes the prover's table with, run on its own with the validated values
        text = INSTALLER.read_text()
        m = re.search(r"^bridge_prover_programs\(\) \{\n.*?^\}\n", text, re.MULTILINE | re.DOTALL)
        self.assertIsNotNone(m)
        out = self.directory / "programs.json"
        env = dict(os.environ, REL_ID="0x" + "cc" * 32, REL_SHA="AB" * 32, BRIDGE_CHECKPOINT_B="checkpoint_b_0_4_0.bin",
                   CKB_ID="0x" + "bb" * 32, CKB_SHA="cd" * 32)
        result = subprocess.run(["bash", "-c", m.group(0) + '\nbridge_prover_programs "$1"', "x", str(out)],
                                capture_output=True, text=True, env=env, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        table = json.loads(out.read_text())
        self.assertEqual(table, {"release": ["release.bin", "0x" + "cc" * 32, "ab" * 32, "release"],
                                 "checkpoint-b": ["checkpoint_b_0_4_0.bin", "0x" + "bb" * 32, "cd" * 32, "checkpoint"]})
        # the prover reads exactly this shape (kind -> [file, image id, sha256, journal kind])
        prover = (TESTNET / "host" / "bin" / "ferrata-bridge-prover").read_text()
        self.assertIn('KINDS = {k: tuple(v) for k, v in json.loads(pathlib.Path(os.environ["FERRATA_PROVER_PROGRAMS_JSON"]).read_text()).items()}', prover)
        self.assertIn("Environment=FERRATA_PROVER_PROGRAMS_JSON=/etc/ferrata/bridge-prover-programs.json", text)


class RunbookCommand(InstallerHarness):
    """V040-REDEPLOY.md §3.7's install-bridge.sh command names every input the v0.4.0 installer requires: its
    variables, each placeholder given a test value, take the installer as far as the good pins do."""

    def test_runbook_command_reaches_the_checksums(self):
        text = (TESTNET / "V040-REDEPLOY.md").read_text()
        lines = [l for l in text.splitlines() if "/install-bridge.sh /opt/ferrata/bridge-v040-bin" in l]
        self.assertEqual(len(lines), 1, lines)
        given = dict(re.findall(r"(FERRATA_[A-Z0-9_]+)=(0x<[^>]*>|<[^>]*>|\S+)", lines[0].split("install-bridge.sh")[0]))
        self.assertEqual(given.get("FERRATA_LEZ_TAG"), "v0.4.0")
        test_values = {"FERRATA_BRIDGE_PINS": None, "FERRATA_BRIDGE_CHECKPOINT_B": CHECKPOINT,
                       "FERRATA_CHECKPOINT_IMAGE_B": "0x" + "bb" * 32, "FERRATA_RELEASE_IMAGE_ID": "0x" + "cc" * 32,
                       "FERRATA_CHECKPOINT_IMAGE_A": "0x" + "aa" * 32}
        self.assertLessEqual(set(given) - {"FERRATA_LEZ_TAG"}, set(test_values), given)
        result = self.run_installer(pins=self.write_pins() if "FERRATA_BRIDGE_PINS" in given else None,
                                    checkpoint=test_values["FERRATA_BRIDGE_CHECKPOINT_B"] if "FERRATA_BRIDGE_CHECKPOINT_B" in given else None,
                                    image_b=test_values["FERRATA_CHECKPOINT_IMAGE_B"] if "FERRATA_CHECKPOINT_IMAGE_B" in given else None,
                                    release_id=test_values["FERRATA_RELEASE_IMAGE_ID"] if "FERRATA_RELEASE_IMAGE_ID" in given else None,
                                    image_a=test_values["FERRATA_CHECKPOINT_IMAGE_A"] if "FERRATA_CHECKPOINT_IMAGE_A" in given else None)
        self.assertEqual(result.returncode, 93, result.stdout + result.stderr)   # as test_good_v040_pins_are_checked_before_install


class GeneratedLabels(BridgeTemporaryTest):
    def configure_source(self, tag=None):
        text = CONFIGURE.read_text()
        self.assertIn('"$TAG" "$IMG_A" "$IMG_B" "$REL_ID"', text)
        match = re.search(r"python3 -I - .*?<<'EOF' > \"\$B/params\.json\"\n(.*?)\nEOF\n", text, re.DOTALL)
        self.assertIsNotNone(match)
        identity = self.directory / "identity.json"
        config = self.directory / "config.json"
        identity.write_text(json.dumps({
            "tip": {"id": 7},
            "vaultParams": {"genesisState": "0x00", "nMin": 1, "tMin": 1, "postingTimeframe": 60},
        }))
        config.write_text(json.dumps({"configId": "0x1234"}))
        env = os.environ.copy()
        env.pop("FERRATA_LEZ_TAG", None)
        env["PYTHON"] = sys.executable
        if tag is not None:
            env["FERRATA_LEZ_TAG"] = tag
        ids = self.rule_ids(tag or "v0.4.0-rc3", table=True)
        self.assertEqual(ids.returncode, 0, ids.stderr)
        img_a, img_b, rel_id = ids.stdout.split()
        result = subprocess.run(
            ["bash", "-c", '"$PYTHON" -I - "$1" "$2" "$3" "$4" "$5" "$6" "$7"',
             "configure-snippet", str(identity), str(config), "0x" + "11" * 20, tag or "v0.4.0-rc3", img_a, img_b, rel_id],
            input=match.group(1), capture_output=True, text=True, env=env, cwd=ROOT, timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def configure_functions(self):
        text = CONFIGURE.read_text()
        fns = [re.search(r"^%s\(\) \{\n.*?^\}\n" % name, text, re.MULTILINE | re.DOTALL) for name in ("lez_host_tag", "bridge_rule_ids")]
        self.assertTrue(all(fns))
        return "".join(m.group(0) for m in fns)

    def rule_ids(self, tag, table=True, image_a="0x" + "aa" * 32, image_b="0x" + "bb" * 32, table_b=None, table_rel="0x" + "cc" * 32,
                 rows=None):
        """configure's bridge_rule_ids TAG with the operator's ids and the prover's table (as install-bridge.sh writes it;
        ROWS replaces it whole)."""
        path = self.directory / "prover-programs.json"
        if table:
            path.write_text(json.dumps(rows if rows is not None else {
                "release": ["release.bin", table_rel, "ab" * 32, "release"],
                "checkpoint-b": [CHECKPOINT, table_b or image_b, "cd" * 32, "checkpoint"]}))
        elif path.exists():
            path.unlink()
        env = {k: v for k, v in os.environ.items() if not k.startswith(("FERRATA_CHECKPOINT_IMAGE", "FERRATA_LEZ_TAG"))}
        env.update({"FERRATA_PROVER_PROGRAMS_JSON": str(path)})
        for k, v in (("FERRATA_CHECKPOINT_IMAGE_A", image_a), ("FERRATA_CHECKPOINT_IMAGE_B", image_b)):
            if v is not None:
                env[k] = v
        return subprocess.run(["bash", "-c", self.configure_functions() + 'bridge_rule_ids "$1"', "x", tag],
                              capture_output=True, text=True, env=env, timeout=20)

    def test_rule_ids_rc3(self):
        # rc3's three programs (docs/image-ids.md), whatever the operator's variables say
        for extra in ({}, {"image_a": None, "image_b": None, "table": False}):
            r = self.rule_ids("v0.4.0-rc3", **extra)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(r.stdout.split(), ["0x9140ec7479debc9b40529b0805095d4cfdef93a1e0500c6687a1d7ec4d7c42de",
                                                "0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04",
                                                "0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd"])

    def test_rule_ids_v040_take_the_prover_table(self):
        r = self.rule_ids("v0.4.0", table_rel="0x" + "dd" * 32)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.split(), ["0x" + "aa" * 32, "0x" + "bb" * 32, "0x" + "dd" * 32])
        good_b = [CHECKPOINT, "0x" + "bb" * 32, "cd" * 32, "checkpoint"]
        for kw, said in (({"table": False}, "program table"), ({"table_b": "0x" + "ee" * 32}, "is not the rule (B) program"),
                         ({"table_rel": "rc3"}, "release's image id"), ({"image_a": None}, "FERRATA_CHECKPOINT_IMAGE_A"),
                         ({"rows": {"release": ["", "0x" + "cc" * 32, "ab" * 32, "release"], "checkpoint-b": good_b}}, "release's file"),
                         ({"rows": {"release": ["release.bin"], "checkpoint-b": good_b}}, "is not [file, image id"),
                         ({"rows": {"release": ["release.bin", "0x" + "cc" * 32, "ab" * 32, "checkpoint"], "checkpoint-b": good_b}},
                          "journal kind"),
                         ({"rows": {"checkpoint-b": good_b}}, "not the two programs"), ({"rows": []}, "not the two programs")):
            with self.subTest(kw=kw):
                r = self.rule_ids("v0.4.0", **kw)
                self.assertNotEqual(r.returncode, 0, r.stdout)
                self.assertIn(said, r.stderr)
                self.assertNotIn("Traceback", r.stderr)

    def test_host_tag(self):
        record = self.directory / "lez-tag"
        def tag(env_tag, recorded):
            if recorded is None:
                record.unlink(missing_ok=True)
            else:
                record.write_text(recorded + "\n")
            env = {k: v for k, v in os.environ.items() if k != "FERRATA_LEZ_TAG"}
            env["FERRATA_LEZ_TAG_FILE"] = str(record)
            if env_tag is not None:
                env["FERRATA_LEZ_TAG"] = env_tag
            r = subprocess.run(["bash", "-c", self.configure_functions() + "lez_host_tag"], capture_output=True, text=True,
                               env=env, timeout=20)
            return r.returncode, r.stdout.strip()
        self.assertEqual(tag(None, None), (0, "v0.4.0-rc3"))
        self.assertEqual(tag("", None), (0, "v0.4.0-rc3"))
        self.assertEqual(tag(None, "v0.4.0"), (0, "v0.4.0"))          # run by hand on a v0.4.0 host
        self.assertEqual(tag("", "v0.4.0"), (0, "v0.4.0"))
        self.assertEqual(tag("v0.4.0", "v0.4.0"), (0, "v0.4.0"))
        self.assertEqual(tag("v0.4.0", None), (0, "v0.4.0"))
        self.assertEqual(tag("v0.4.0-rc3", "v0.4.0")[0], 2)            # disagrees with the host's record

    def test_configure_source_names_the_selected_tag(self):
        expected = {
            None: "v0.4.0-rc3",
            "v0.4.0-rc3": "v0.4.0-rc3",
            "v0.4.0": "v0.4.0",
        }
        baseline = None
        for tag, selected in expected.items():
            with self.subTest(tag=tag):
                params = self.configure_source(tag)
                self.assertIn("ferrata-testnet (AWS, LEZ " + selected + "): ferrata-bridge-configure", params.pop("_source"))
                if selected == "v0.4.0":
                    self.assertEqual((params.pop("checkpointImageIdA"), params.pop("checkpointImageIdB"), params.pop("releaseImageId")),
                                     ("0x" + "aa" * 32, "0x" + "bb" * 32, "0x" + "cc" * 32))
                else:
                    self.assertEqual((params.pop("checkpointImageIdA"), params.pop("checkpointImageIdB"), params.pop("releaseImageId")),
                                     ("0x9140ec7479debc9b40529b0805095d4cfdef93a1e0500c6687a1d7ec4d7c42de",
                                      "0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04",
                                      "0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd"))
                if baseline is None:
                    baseline = params
                self.assertEqual(params, baseline)

    def test_configure_refuses_v040_without_its_rule_programs(self):
        env = {k: v for k, v in os.environ.items() if not k.startswith(("FERRATA_CHECKPOINT_IMAGE", "FERRATA_LEZ_TAG"))}
        env.update({"FERRATA_LEZ_TAG": "v0.4.0", "FERRATA_LEZ_TAG_FILE": str(self.directory / "no-host-tag"),
                    "FERRATA_PROVER_PROGRAMS_JSON": str(self.directory / "no-table")})
        for extra in ({}, {"FERRATA_CHECKPOINT_IMAGE_A": "0x" + "aa" * 32}, {"FERRATA_CHECKPOINT_IMAGE_A": "0x" + "aa" * 32,
                      "FERRATA_CHECKPOINT_IMAGE_B": "0xnothex"}):
            result = subprocess.run(["bash", str(CONFIGURE), "--vault", "0x" + "11" * 20], capture_output=True, text=True,
                                    env=dict(env, **extra), cwd=ROOT, timeout=20)
            self.assertEqual(result.returncode, 2, (extra, result.stderr))
            self.assertIn("FERRATA_CHECKPOINT_IMAGE_A and FERRATA_CHECKPOINT_IMAGE_B", result.stderr)

    def identity_label(self, tag=None):
        loader = importlib.machinery.SourceFileLoader("bridge_identity_under_test", str(IDENTITY))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        module = importlib.util.module_from_spec(spec)
        loader.exec_module(module)
        programs = self.directory / "programs.json"
        programs.write_text(json.dumps({
            "programs": {
                "ferrata_attestation": {"program_account": "1" * 44, "image_id": "0x11"},
                "ferrata_bridge_b1": {"program_account": "2" * 44, "image_id": "0x22"},
            },
            "instances": {"sepolia": {"instance_id": "33" * 32, "checkpoint": "44" * 32}},
        }))
        channel = {
            "accredited_keys": ["55" * 32], "posting_timeframe": 60,
            "config_tip_hash": "66" * 32, "configuration_threshold": 1,
        }

        def get(path):
            return channel if path.startswith("/channel/") else {"fork_digest": "77" * 32}

        def rpc(method, params):
            if method == "getLastBlockId":
                return 7
            self.assertEqual(method, "getBlock")
            raw = bytearray(73)
            raw[:8] = (7).to_bytes(8, "little")
            raw[40:72] = bytes.fromhex("88" * 32)
            raw[-1] = 2
            return base64.b64encode(raw).decode()

        out = self.directory / "generated-identity.json"
        env = {key: value for key, value in os.environ.items() if key != "FERRATA_LEZ_TAG"}
        if tag is not None:
            env["FERRATA_LEZ_TAG"] = tag
        with warnings.catch_warnings(), \
                mock.patch.dict(os.environ, env, clear=True), \
                mock.patch.object(module, "get", side_effect=get), \
                mock.patch.object(module, "rpc", side_effect=rpc), \
                mock.patch.object(module, "PROGRAMS", str(programs)), \
                mock.patch.object(sys, "argv", [str(IDENTITY), "--out", str(out)]), \
                contextlib.redirect_stdout(io.StringIO()):
            warnings.simplefilter("ignore", ResourceWarning)
            module.main()
        return json.loads(out.read_text())

    def test_identity_label_names_the_selected_tag(self):
        for tag, selected in ((None, "v0.4.0-rc3"), ("v0.4.0-rc3", "v0.4.0-rc3"), ("v0.4.0", "v0.4.0")):
            with self.subTest(tag=tag):
                identity = self.identity_label(tag)
                bedrock = "0.4.0-rc.2" if selected == "v0.4.0" else "0.3.5-rc.1"
                self.assertEqual(
                    identity["label"],
                    "ferrata-testnet: LEZ " + selected + " on Logos Blockchain " + bedrock + ", one sequencer (AWS)",
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
