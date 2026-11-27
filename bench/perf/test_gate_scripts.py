#!/usr/bin/env python3
"""Offline tests of bench/perf/gate-sequencer.sh's parts: beacon_replay.py's copies (`--copies`: each update with its
signature slot moved by one, within its sync-committee period), gate-hook.sh's defaults and summarize_gate.py's
per-daemon lines. Standard library only: `python3 -I bench/perf/test_gate_scripts.py` (CI `links`). Scratch in the
repository's `.scratch/` (git-ignored)."""

import csv
import importlib.util
import json
import os
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.request
from pathlib import Path


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
RECORD = REPO / "lez/lab/results/record/net/mainnet"
SCRATCH = REPO / ".scratch" / "test_gate_scripts"


def load_beacon_replay():
    spec = importlib.util.spec_from_file_location("beacon_replay_under_test", HERE / "beacon_replay.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CopyUpdateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.replay = load_beacon_replay()

    def test_every_recorded_electra_update(self):
        paths = sorted((RECORD / "updates").glob("*.electra.ssz"))
        self.assertTrue(paths, "the recorded mainnet updates are required")
        for path in paths:
            with self.subTest(update=path.name):
                kind = path.name.split(".")[1]
                original = path.read_bytes()
                changed, changed_sig = self.replay.copy_update(kind, original)
                attested, finalized, signature = self.replay.slots(kind, original)
                copy_slots = self.replay.slots(kind, changed)
                period = signature // self.replay.SLOTS_PER_PERIOD
                plus, minus = signature + 1, signature - 1
                if plus // self.replay.SLOTS_PER_PERIOD == period and plus > attested:
                    expected_sig = plus
                elif minus // self.replay.SLOTS_PER_PERIOD == period and minus > attested:
                    expected_sig = minus
                else:
                    expected_sig = signature
                self.assertEqual(changed_sig, expected_sig)
                self.assertEqual(copy_slots, (attested, finalized, changed_sig))
                self.assertEqual(len(changed), len(original))
                self.assertEqual(changed_sig // self.replay.SLOTS_PER_PERIOD, period)
                if changed_sig == signature:
                    self.assertFalse(plus // self.replay.SLOTS_PER_PERIOD == period and plus > attested)
                    self.assertFalse(minus // self.replay.SLOTS_PER_PERIOD == period and minus > attested)
                else:
                    self.assertEqual(abs(changed_sig - signature), 1)
                    self.assertGreater(changed_sig, attested)
                restored = bytearray(changed)
                struct.pack_into("<Q", restored, self.replay.signature_offset(kind), signature)
                self.assertEqual(bytes(restored), original, "bytes other than signature_slot changed")

    def test_the_replayed_updates_all_have_a_copy(self):
        """The sequencer gate replays the first 106 recorded updates: each one's copy differs (no entry falls back to
        its original bytes), so a run with GATE_COPIES=1 serves honest-2 a copy of every update."""
        paths = sorted((RECORD / "updates").glob("*.electra.ssz"))[:106]
        self.assertEqual(len(paths), 106)
        for path in paths:
            kind = path.name.split(".")[1]
            original = path.read_bytes()
            changed, sig = self.replay.copy_update(kind, original)
            self.assertNotEqual(changed, original, path.name)
            self.assertNotEqual(sig, self.replay.slots(kind, original)[2], path.name)

    def test_copies_must_name_served_ports(self):
        r = subprocess.run([sys.executable, "-I", str(HERE / "beacon_replay.py"), "--dir", str(RECORD), "--ports", "1,2",
                            "--copies", "3", "--stats", "x.json"], capture_output=True, text=True)
        self.assertEqual(r.returncode, 2)
        self.assertIn("every --copies port must also be in --ports", r.stderr)


class BeaconReplayHTTPTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.replay = load_beacon_replay()

    def free_ports(self):
        sockets = []
        try:
            for _ in range(2):
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sockets.append(s)
                s.bind(("127.0.0.1", 0))
            return [s.getsockname()[1] for s in sockets]
        except OSError as exc:
            if os.environ.get("CI"):
                raise
            self.skipTest(f"cannot bind a loopback port in this sandbox: {exc}")
        finally:
            for s in sockets:
                s.close()

    def test_original_and_copy_ports_endpoints_stats_and_reveals(self):
        ports = self.free_ports()
        SCRATCH.mkdir(parents=True, exist_ok=True)
        paths = sorted((RECORD / "updates").glob("*.electra.ssz"))[:3]
        period_entry = next(p for p in reversed(paths) if ".period." in p.name)
        finality_entry = next(p for p in reversed(paths) if ".finality." in p.name)
        period_original = period_entry.read_bytes()
        finality_original = finality_entry.read_bytes()
        period_copy = self.replay.copy_update("period", period_original)[0]
        finality_copy = self.replay.copy_update("finality", finality_original)[0]
        period = self.replay.slots("period", period_original)[0] // self.replay.SLOTS_PER_PERIOD
        expected_period = {}
        for label, raw in (("original", period_original), ("copy", period_copy)):
            expected_period[label] = struct.pack("<Q", len(raw) + 4) + b"\0\0\0\0" + raw

        with tempfile.TemporaryDirectory(prefix="gate-test-", dir=SCRATCH) as tmp:
            tmp = Path(tmp)
            log_path = tmp / "beacon.log"
            reveals_path = tmp / "reveals.tsv"
            stats_path = tmp / "stats.json"
            with log_path.open("w+") as log:
                process = subprocess.Popen(
                    [sys.executable, "-I", str(HERE / "beacon_replay.py"), "--dir", str(RECORD),
                     "--ports", ",".join(map(str, ports)), "--copies", str(ports[1]),
                     "--step", "1", "--start", "1", "--last", "3",
                     "--reveals", str(reveals_path), "--stats", str(stats_path)],
                    stdout=log, stderr=subprocess.STDOUT)
                try:
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        if process.poll() is not None:
                            log.flush()
                            message = log_path.read_text()
                            if "Address already in use" in message or "Permission denied" in message:
                                self.skipTest(f"beacon replay could not bind loopback ports: {message.strip()}")
                            self.fail(f"beacon replay exited before serving requests:\n{message}")
                        log.flush()
                        if "replaying " in log_path.read_text():
                            break
                        time.sleep(0.02)
                    else:
                        self.fail("beacon replay did not start within 10 seconds")

                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        if reveals_path.exists() and len(reveals_path.read_text().splitlines()) == 4:
                            break
                        if process.poll() is not None:
                            self.fail("beacon replay exited before revealing three entries")
                        time.sleep(0.02)
                    else:
                        self.fail("beacon replay did not reveal three entries within 10 seconds")

                    responses = {"finality": {}, "period": {}}
                    for label, port in zip(("original", "copy"), ports):
                        base = f"http://127.0.0.1:{port}/eth/v1/beacon/light_client"
                        with urllib.request.urlopen(base + "/finality_update", timeout=3) as response:
                            responses["finality"][label] = response.read()
                        with urllib.request.urlopen(base + f"/updates?start_period={period}&count=1", timeout=3) as response:
                            responses["period"][label] = response.read()
                    self.assertEqual(responses["finality"]["original"], finality_original)
                    self.assertEqual(responses["period"]["original"], expected_period["original"])
                    self.assertEqual(responses["finality"]["copy"], finality_copy)
                    self.assertEqual(responses["period"]["copy"], expected_period["copy"])
                finally:
                    if process.poll() is None:
                        process.send_signal(signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
                        self.fail("beacon replay did not stop after SIGTERM")

            self.assertEqual(process.returncode, 0)
            stats = json.loads(stats_path.read_text())
            self.assertEqual(stats["revealed"], 3)
            self.assertEqual(stats["last"], 3)
            self.assertEqual(set(stats["per_port"]), {str(p) for p in ports})
            for label, port in zip(("original", "copy"), ports):
                per_port = stats["per_port"][str(port)]
                self.assertEqual(set(per_port), {"finality_update", "updates"})
                self.assertEqual(per_port["finality_update"]["requests"], 1)
                self.assertEqual(per_port["updates"]["requests"], 1)
                self.assertEqual(per_port["finality_update"]["bytes"],
                                 len(responses["finality"][label]) + 200)
                self.assertEqual(per_port["updates"]["bytes"], len(responses["period"][label]) + 200)

            with reveals_path.open(newline="") as f:
                reader = csv.DictReader(f, delimiter="\t")
                self.assertEqual(reader.fieldnames,
                                 ["entry", "name", "kind", "attested_slot", "finalized_slot", "signature_slot",
                                  "revealed_utc", "revealed_unix", "copy_signature_slot"])
                rows = list(reader)
            self.assertEqual(len(rows), 3)
            for path, row in zip(paths, rows):
                kind = path.name.split(".")[1]
                original = path.read_bytes()
                expected_copy_sig = self.replay.copy_update(kind, original)[1]
                self.assertEqual(row["name"], path.name)
                self.assertEqual(row["signature_slot"], str(self.replay.slots(kind, original)[2]))
                self.assertEqual(row["copy_signature_slot"], str(expected_copy_sig))


class GateHookDefaultTests(unittest.TestCase):
    def test_default_updater_toml_is_unchanged_by_text(self):
        """Static/text check: no zone or updater binary is needed to check the default config."""
        text = (HERE / "gate-hook.sh").read_text()
        for fragment in (
            'POLL="${GATE_POLL:-4}"',
            'POLLS="${GATE_POLLS:-$POLL,$POLL}"',
            'POLL1="${POLLS%%,*}"',
            'POLL2="${POLLS#*,}"',
            'COPIES="${GATE_COPIES:-0}"',
            'ADV_POLL="${GATE_ADV_POLL:-$(( POLL > 1 ? POLL / 2 : 1 ))}"',
            'polls=("$POLL1" "$POLL2"); joins=("" "")',
            'windows+=(1); polls+=("$ADV_POLL"); joins+=("join_wait_rounds = 0")',
            # honest-2's beacon port (9601 + 1) serves the copies, passed to the replay.
            'copy_args=(); if [ "$COPIES" = 1 ]; then copy_args=(--copies 9602); fi',
            'python3 -I "$HERE/beacon_replay.py" --dir "$REC" --ports "$ports" "${copy_args[@]}" --start 1',
            'names=(honest-1 honest-2)',
        ):
            self.assertIn(fragment, text)
        # An empty entry of `joins` (an honest daemon) adds no line to its configuration; the adversary's adds one.
        idiom = 'j=("" "x = 0"); for i in 0 1; do printf "%s|" a ${j[$i]:+"${j[$i]}"} b; done'
        out = subprocess.run(["bash", "-c", idiom], check=True, text=True, stdout=subprocess.PIPE).stdout
        self.assertEqual(out, "a|b|a|x = 0|b|")
        expected_config_lines = (
            """    '[daemon]' "name = \\"$name\\"" 'state_dir = "state"' "poll_secs = ${polls[$i]}" "listen = \\"127.0.0.1:$((9701 + i))\\"" 'ledger = "sequencer"' '' \\""",
            """    '[sequencer]' "rpc = \\"http://127.0.0.1:$((13041 + i))\\"" "program = \\"$PROGRAM\\"" 'wait_final = true' "log = \\"$ZONE_LOG/sequencer.log\\"" '' \\""",
            """    '[keys]' 'payer_key_file = "payer.key"' '' \\""",
            """    '[fees]' 'gas_limit = 4800000' 'max_fee_per_tx = 1000000000' 'budget = 1000000000000' '' \\""",
            """    '[policy]' "defer_window = ${windows[$i]}" 'max_lag_slots = 64' 'min_agreeing = 1' 'retry_base_ms = 1000' 'retry_max_ms = 60000' \\""",
            """    'max_attempts = 5' 'demote_rounds = 50' 'max_periods_per_request = 4' 'finish_own_jobs = true' ${joins[$i]:+"${joins[$i]}"} '' \\""",
            """    '[[chain]]' 'name = "mainnet"' 'preset = "mainnet"' "checkpoint = \\"$CHECKPOINT\\"" 'adoption_parts = 2' '' \\""",
            """    '  [[chain.source]]' '  name = "replay"' "  url = \\"http://127.0.0.1:$((9601 + i))\\"" '  timeout_secs = 30' > "$D/updater.toml\"""",
        )
        for line in expected_config_lines:
            self.assertIn(line, text)


class SummaryCompatibilityTests(unittest.TestCase):
    def test_gate_g10_existing_lines_and_new_accounting(self):
        result = subprocess.run(
            [sys.executable, "-I", str(HERE / "summarize_gate.py"), str(HERE / "results/gate-g10")],
            check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        lines = result.stdout.splitlines()
        expected_existing = (
            "GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0",
            "**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 64,487,822 / 756,437,607 = **0.0853** (fees: 0.0854; the gate: at most 0.10).",
            "**R.10:** honest-1's share of the honest daemons' fees = **0.541** (honest-2 0.459; the gate: 0.35 to 0.65).",
            "Jobs with an applied step: 77; steps applied: 245; applied more than once: 0.",
            "| honest-1 | 4 | 1042, 6.1 | 6399 | 429.0 | 0.0670 | 57.9 | 34.2 | 2,043 / 34,466,528 | 4,976,302 / 39,471,633 |",
            "| honest-2 | 4 | 1103, 5.8 | 6399 | 428.9 | 0.0670 | 57.9 | 34.4 | 2,155 / 36,169,000 | 4,601,177 / 37,478,622 |",
        )
        for expected in expected_existing:
            self.assertIn(expected, lines)
        # A run made before the per-daemon settings: its record stays as generated (no per-daemon lines).
        self.assertNotIn("Per-daemon applied Opens, fee share, and updates learned on chain:", lines)

    def test_different_polls_and_updates_learned_metric(self):
        source = HERE / "results/gate-g10"
        SCRATCH.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="gate-summary-", dir=SCRATCH) as tmp:
            run = Path(tmp) / "gate-test"
            run.mkdir()
            for path in source.iterdir():
                if path.name not in ("updater-honest-2.toml", "metrics-honest-1.txt", "machine.txt"):
                    (run / path.name).symlink_to(path)
            machine = (source / "machine.txt").read_text()
            (run / "machine.txt").write_text(machine.replace(
                "GATE_ADVERSARY=0\n", "GATE_ADVERSARY=0 GATE_POLLS=4,6 GATE_COPIES=0 GATE_ADV_POLL=2\n", 1))
            config = (source / "updater-honest-2.toml").read_text()
            self.assertIn("poll_secs = 4", config)
            (run / "updater-honest-2.toml").write_text(config.replace("poll_secs = 4", "poll_secs = 6", 1))
            (run / "metrics-honest-1.txt").write_text(
                'ferrata_updater_updates_from_chain_total{chain="mainnet"} 17\n')
            result = subprocess.run(
                [sys.executable, "-I", str(HERE / "summarize_gate.py"), str(run)],
                check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        lines = result.stdout.splitlines()
        poll_row = next(line for line in lines if line.startswith("| honest-2 | 6 |"))
        self.assertTrue(poll_row)
        self.assertIn("- honest-1: 37 applied Opens; fees / honest fees = 0.541; "
                      "updates learned from other parties' Opens: 17.", lines)
        self.assertIn("- honest-2: 39 applied Opens; fees / honest fees = 0.459.", lines)


if __name__ == "__main__":
    unittest.main(verbosity=2)
