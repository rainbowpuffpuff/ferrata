#!/usr/bin/env python3
"""Tests of the zone scripts' tag logic, without a node or a sequencer (python3 -I checkpoint/attestor/test_zone_scripts.py).

Each script is run against fake LEZ trees and bundles made under this tree's ignored .scratch/ (never /tmp), and must
refuse what does not belong together before it starts or writes anything: zone.sh (a v0.4.0 tree with an rc3 bundle or
one of no tag; an rc3 bundle with rc1's devnet configuration still accepted, as the F.7 runs used), zone-bridge.sh (no
defaults on v0.4.0: ZB_BUNDLE, ZB_PROGRAMS, ZB_CHECKPOINT_A, ZB_CHECKPOINT_B; nothing written under e2e/runs/),
zone-attest.sh and zone-v040.sh (a tree of no known tag, or not v0.4.0).
"""
import os
import shutil
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
CHECKPOINT = os.path.dirname(HERE)
REPO = os.path.dirname(CHECKPOINT)
SCRATCH = os.path.join(REPO, ".scratch")


def have(tool):
    return shutil.which(tool) is not None


class Scripts(unittest.TestCase):
    def setUp(self):
        os.makedirs(SCRATCH, exist_ok=True)
        self.d = tempfile.mkdtemp(prefix="test-zone-scripts-", dir=SCRATCH)
        self.tmpdir = os.path.join(self.d, "tmpdir")
        os.makedirs(self.tmpdir)

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def tree(self, tag):
        t = os.path.join(self.d, "upstream", f"lez-{tag}")
        os.makedirs(os.path.join(t, "lez", "configs", "docker-all-in-one", "devnet"), exist_ok=True)
        os.makedirs(os.path.join(t, "bedrock"), exist_ok=True)
        return t

    def bundle(self, tag_line):
        b = os.path.join(self.d, "bundle-src")
        shutil.rmtree(b, ignore_errors=True)
        os.makedirs(os.path.join(b, "lez-config"))
        os.makedirs(os.path.join(b, "bin"))
        os.makedirs(os.path.join(b, "node"))
        if tag_line is not None:
            with open(os.path.join(b, "lez-config", "lez-tag.txt"), "w") as f:
                f.write(tag_line + "\n")
        with open(os.path.join(b, "node", "logos-blockchain-node"), "w") as f:
            f.write("#!/bin/sh\nexit 3\n")
        with open(os.path.join(b, "bin", "sequencer_service"), "w") as f:
            f.write("#!/bin/sh\nexit 3\n")
        subprocess.run(["bash", "-c", 'cd "$1" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS', "x", b], check=True)
        out = os.path.join(self.d, "bundle.tar.zst")
        subprocess.run(["bash", "-c", 'tar -C "$1" -cf - . | zstd -q -f -o "$2"', "x", b, out], check=True)
        return out

    def run_script(self, args, env, cwd=CHECKPOINT):
        e = dict(os.environ, TMPDIR=self.tmpdir, **env)
        return subprocess.run(["bash", *args], cwd=cwd, env=e, capture_output=True, text=True, timeout=120)

    # ---- zone.sh ---------------------------------------------------------------------------------------------------
    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_zone_v040_tree_needs_v040_bundle(self):
        t = self.tree("v0.4.0")
        for line in ("v0.4.0-rc3 bafd07be0701398f9bc435f73ab05e9a7936c301", None):
            p = self.run_script(["attestor/zone.sh", self.bundle(line), os.path.join(self.d, "out"), "1"], {"ZONE_LEZ": t})
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
            self.assertIn("zone.sh: the bundle is LEZ", p.stdout + p.stderr)

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_zone_v040_bundle_needs_v040_tree(self):
        t = self.tree("v0.4.0-rc3")
        p = self.run_script(["attestor/zone.sh", self.bundle("v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115"),
                             os.path.join(self.d, "out"), "1"], {"ZONE_LEZ": t})
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        self.assertIn("the bundle is LEZ v0.4.0, the tree v0.4.0-rc3", p.stdout + p.stderr)

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_zone_rc1_tree_with_rc3_bundle_passes_the_tag_check(self):
        # the F.7 runs: rc1's devnet configuration (the same as rc3's) with an rc3 bundle. The tag check passes; the run
        # then stops later, at the missing Bedrock configuration of the fake tree (not with exit 2 from the tag check).
        t = self.tree("v0.4.0-rc1")
        p = self.run_script(["attestor/zone.sh", self.bundle("v0.4.0-rc3 bafd07be0701398f9bc435f73ab05e9a7936c301"),
                             os.path.join(self.d, "out"), "1"], {"ZONE_LEZ": t})
        self.assertNotIn("the bundle is LEZ", p.stdout + p.stderr)
        self.assertIn("node-config.yaml", p.stdout + p.stderr)

    def test_zone_unknown_tag(self):
        t = self.tree("v0.4.1")
        p = self.run_script(["attestor/zone.sh", os.path.join(self.d, "nothing.tar.zst"), os.path.join(self.d, "out")], {"ZONE_LEZ": t})
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        self.assertIn("not a LEZ tree of a known tag", p.stdout + p.stderr)

    def test_peer_topology_of_the_generated_configurations(self):
        # zone.sh's four sequencers each list the three others (npeers 4); zone-bridge.sh's leave room for the fifth key
        # its rotation stakes (npeers 5); zl_seq_cfg's own test shows what npeers does to the peer list
        import re
        def calls(path):
            with open(path) as f:
                return re.findall(r'zl_seq_cfg "\$(?:dev|DEV)/sequencer_config\.json" (\S+) "\$(?:work|W)/cfg-\$(?:i|1)\.json" "\$(?:api|API)" "\$(?:i|1)" (\d+) "\$TAG"', f.read())
        self.assertEqual(calls(os.path.join(HERE, "zone.sh")), [("-", "4")])
        self.assertEqual(calls(os.path.join(HERE, "zone-bridge.sh")), [('"$LEZ/lez/sequencer/service/networks/local.json"', "5")])

    # ---- zone-bridge.sh --------------------------------------------------------------------------------------------
    def test_zone_bridge_v040_has_no_defaults(self):
        t = self.tree("v0.4.0")
        name = "test-zone-scripts-never-" + os.path.basename(self.d)
        run = os.path.join("e2e", "runs", name)
        full = {"ZB_BUNDLE": "b.tar.zst", "ZB_PROGRAMS": "p", "ZB_CHECKPOINT_A": "a", "ZB_CHECKPOINT_B": "b"}
        try:
            for missing in full:
                env = {k: v for k, v in full.items() if k != missing}
                env["ZB_LEZ"] = t
                for k in full:  # an inherited value would hide the missing one
                    os.environ.pop(k, None)
                for stage in ("build", "zone", "input", "prove", "releases", "vault", "fixture", "verify", "gpu-jobs",
                              "gpu-seals", "groth16"):
                    p = self.run_script(["checkpoint/attestor/zone-bridge.sh", stage, run], env, cwd=REPO)
                    self.assertEqual(p.returncode, 2, (missing, stage, p.stdout + p.stderr))
                    self.assertIn(f"no default {missing}", p.stdout + p.stderr)
                    self.assertFalse(os.path.exists(os.path.join(REPO, run)), "the refused stage wrote its run directory")
        finally:
            shutil.rmtree(os.path.join(REPO, run), ignore_errors=True)

    def test_zone_bridge_unknown_tag(self):
        t = self.tree("v0.5.0")
        run = os.path.join("e2e", "runs", "test-zone-scripts-never-" + os.path.basename(self.d))
        try:
            p = self.run_script(["checkpoint/attestor/zone-bridge.sh", "zone", run], {"ZB_LEZ": t}, cwd=REPO)
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
            self.assertIn("not a LEZ tree of a known tag", p.stdout + p.stderr)
            self.assertFalse(os.path.exists(os.path.join(REPO, run)))
        finally:
            shutil.rmtree(os.path.join(REPO, run), ignore_errors=True)

    # ---- zone-bundle.sh add-programs ---------------------------------------------------------------------------------
    def programs(self, good=True, drop=None, prefix="", extra=()):
        d = os.path.join(self.d, "programs")
        os.makedirs(d, exist_ok=True)
        import hashlib, json
        lines = []
        for name, body in (("ferrata_lez", b"lez-elf"), ("ferrata_bridge", b"bridge-elf")):
            with open(os.path.join(d, name + ".bin"), "wb") as f:
                f.write(body)
            sha = hashlib.sha256(body if good or name == "ferrata_lez" else b"other").hexdigest()
            if name != drop:
                lines.append(json.dumps({"program": prefix + name + ".bin", "image_id": "0x" + "00" * 32, "sha256": sha}))
        lines += [json.dumps(e) for e in extra]
        with open(os.path.join(d, "ids.jsonl"), "w") as f:
            f.write("\n".join(lines) + "\n")
        fx = os.path.join(self.d, "fixture")
        os.makedirs(fx, exist_ok=True)
        for n in ("bootstrap.ssz", "update_1409.ssz", "beacon_header_11550000_second.json", "SOURCE.md", "unrelated.txt"):
            with open(os.path.join(fx, n), "w") as f:
                f.write(n)
        return d, fx

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_add_programs(self):
        src = self.bundle("v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115")
        progs, fx = self.programs()
        new = os.path.join(self.d, "new.tar.zst")
        p = self.run_script(["checkpoint/attestor/zone-bundle.sh", "add-programs", src, progs, fx, new], {}, cwd=REPO)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        x = os.path.join(self.d, "x")
        os.makedirs(x)
        subprocess.run(["bash", "-c", 'zstd -q -d -c "$1" | tar -C "$2" -xf - && cd "$2" && sha256sum -c --quiet SHA256SUMS', "x", new, x], check=True)
        self.assertEqual(sorted(os.listdir(os.path.join(x, "program"))), ["ferrata_bridge.bin", "ferrata_lez.bin"])
        self.assertEqual(sorted(os.listdir(os.path.join(x, "fixture"))),
                         ["SOURCE.md", "beacon_header_11550000_second.json", "bootstrap.ssz", "update_1409.ssz"])
        with open(os.path.join(x, "lez-config", "lez-tag.txt")) as f:
            self.assertTrue(f.read().startswith("v0.4.0 "))          # the rest of the bundle kept
        self.assertFalse(any(n.startswith(".add-programs") for n in os.listdir(self.d)))

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_add_programs_refuses_a_program_ids_do_not_name(self):
        src = self.bundle("v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115")
        progs, fx = self.programs(good=False)
        new = os.path.join(self.d, "new.tar.zst")
        p = self.run_script(["checkpoint/attestor/zone-bundle.sh", "add-programs", src, progs, fx, new], {}, cwd=REPO)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("ferrata_bridge.bin is not the sha256 ids.jsonl names", p.stderr)
        self.assertFalse(os.path.exists(new))

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_add_programs_needs_an_entry_per_program(self):
        src = self.bundle("v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115")
        progs, fx = self.programs(drop="ferrata_bridge")
        new = os.path.join(self.d, "new.tar.zst")
        p = self.run_script(["checkpoint/attestor/zone-bundle.sh", "add-programs", src, progs, fx, new], {}, cwd=REPO)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("ids.jsonl names no single sha256 for ferrata_bridge.bin", p.stderr)
        self.assertFalse(os.path.exists(new))

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_add_programs_refuses_conflicting_entries(self):
        # a second entry for the same program with another sha256: which build is meant is unknown, so neither is taken
        src = self.bundle("v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115")
        progs, fx = self.programs(extra=[{"program": "other/ferrata_bridge.bin", "image_id": "0x" + "11" * 32,
                                          "sha256": "ab" * 32}])
        new = os.path.join(self.d, "new.tar.zst")
        p = self.run_script(["checkpoint/attestor/zone-bundle.sh", "add-programs", src, progs, fx, new], {}, cwd=REPO)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("ids.jsonl names no single sha256 for ferrata_bridge.bin", p.stderr)
        self.assertFalse(os.path.exists(new))

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_add_programs_takes_path_shaped_entries(self):
        src = self.bundle("v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115")
        progs, fx = self.programs(prefix="build/programs/")
        new = os.path.join(self.d, "new.tar.zst")
        p = self.run_script(["checkpoint/attestor/zone-bundle.sh", "add-programs", src, progs, fx, new], {}, cwd=REPO)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    # ---- zone-attest.sh, zone-v040.sh ------------------------------------------------------------------------------
    def test_zone_attest_unknown_tag(self):
        t = self.tree("v9")
        z = os.path.join(self.d, "zone")
        os.makedirs(z)
        p = self.run_script(["attestor/zone-attest.sh", z, os.path.join(self.d, "out")], {"ZONE_LEZ": t})
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        self.assertIn("not a LEZ tree of a known tag", p.stdout + p.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.tmpdir, "attestor-zone-attest")))

    def test_zone_v040_refuses_other_tags(self):
        for tag in ("v0.4.0-rc3", "v0.4.0-rc1"):
            t = self.tree(tag)
            p = self.run_script(["attestor/zone-v040.sh", "standalone", os.path.join(self.d, "b.tar.zst"), os.path.join(self.d, "out")],
                                {"ZONE_LEZ": t})
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
            self.assertIn("is not LEZ v0.4.0", p.stdout + p.stderr)

    @unittest.skipUnless(have("zstd"), "zstd is not installed")
    def test_zone_v040_refuses_an_rc3_bundle(self):
        t = self.tree("v0.4.0")
        p = self.run_script(["attestor/zone-v040.sh", "vote", self.bundle("v0.4.0-rc3 bafd07be0701398f9bc435f73ab05e9a7936c301"),
                             os.path.join(self.d, "out")], {"ZONE_LEZ": t})
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        self.assertIn("the bundle is not LEZ v0.4.0", p.stdout + p.stderr)

    # ---- zone-v040.sh's EXIT trap: the records scrubbed, and a scrub that refuses fails the run ---------------------
    def cleanup_rc(self, run_rc, scrub_rc, last_log_line=None):
        """zone-v040.sh's cleanup() alone (taken from the script), after a run that exits RUN_RC, with a stand-in
        scripts/scrub_records.py that exits SCRUB_RC; (the trap's exit status, the records it left)."""
        fake = os.path.join(self.d, "repo")
        os.makedirs(os.path.join(fake, "scripts"), exist_ok=True)
        with open(os.path.join(fake, "scripts", "scrub_records.py"), "w") as f:
            f.write(f"import sys\nprint('stand-in scrub')\nsys.exit({scrub_rc})\n")
        work, recs = os.path.join(self.d, "work"), os.path.join(self.d, "recs")
        os.makedirs(os.path.join(work, "logs"), exist_ok=True)
        with open(os.path.join(work, "logs", "seq.log"), "w") as f:
            f.write("".join(f"line {i}\n" for i in range(500)))
        if last_log_line is not None:
            os.makedirs(recs, exist_ok=True)
            with open(os.path.join(recs, "run.log"), "w") as f:
                f.write(last_log_line + "\n")
        facts = os.path.join(work, "facts.jsonl")
        with open(facts, "w") as f:
            f.write('{"a": 1}\n{"b": "2"}\n')
        script = os.path.join(HERE, "zone-v040.sh")
        body = (f'sleep() {{ :; }}; PIDS=(); REPO="$1"; RECS="$2"; work="$3"; FACTS="$4"; SPIKE="$5"\n'
                f'eval "$(sed -n "/^cleanup() {{/,/^}}$/p" "$6")"\n'
                f'trap cleanup EXIT\n(exit {run_rc})\n')
        p = subprocess.run(["bash", "-c", "set -uo pipefail\n" + body, "x", fake, recs, work, facts, self.d, script],
                           capture_output=True, text=True, timeout=60)
        return p, recs

    def test_cleanup_keeps_a_passing_run_when_the_scrub_passes(self):
        p, recs = self.cleanup_rc(0, 0)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        import json
        with open(os.path.join(recs, "facts.json")) as f:
            self.assertEqual(json.load(f), {"a": 1, "b": "2"})
        with open(os.path.join(recs, "logs", "seq.log")) as f:
            lines = f.read().splitlines()
        self.assertEqual((len(lines), lines[0], lines[-1]), (400, "line 100", "line 499"))   # the last 400 lines
        with open(os.path.join(recs, "scrub.txt")) as f:
            self.assertIn("stand-in scrub", f.read())

    def test_cleanup_fails_a_passing_run_whose_scrub_refuses(self):
        p, _ = self.cleanup_rc(0, 1)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("the records name this machine", p.stderr)

    def test_cleanup_keeps_a_failed_run_s_status(self):
        for scrub in (0, 1):
            p, _ = self.cleanup_rc(3, scrub)
            self.assertEqual(p.returncode, 3, (scrub, p.stdout + p.stderr))

    def test_cleanup_says_why_a_silent_failure_stopped(self):
        # a command that failed under set -e left no STOP line: the trap writes one; a run that said why gets no second
        recs = os.path.join(self.d, "recs")
        shutil.rmtree(recs, ignore_errors=True)
        self.cleanup_rc(3, 0)
        with open(os.path.join(recs, "run.log")) as f:
            self.assertIn("STOP: exit 3 without a STOP line", f.read())
        shutil.rmtree(recs)
        self.cleanup_rc(1, 0, last_log_line="[01:02:03] STOP: unstake: refused")
        with open(os.path.join(recs, "run.log")) as f:
            self.assertEqual(f.read().splitlines(), ["[01:02:03] STOP: unstake: refused"])
        shutil.rmtree(recs)
        self.cleanup_rc(0, 0)
        self.assertFalse(os.path.exists(os.path.join(recs, "run.log")))


if __name__ == "__main__":
    unittest.main(verbosity=1)
