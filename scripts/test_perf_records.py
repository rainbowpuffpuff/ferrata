"""Recomputes the documented performance figures of RFP-022 P.5 and P.6 from the committed records:
`python3 -I scripts/test_perf_records.py`.

RFP-022 P.5 (deposit to mint, burn to release) is documented in `lez/results/perf-latency-bridge.md` and
`docs/performance.md` §P.5; every figure of it is derived from `e2e/runs/crown-live-3/`'s own records —
`stages.done` (each stage's end and duration), `captures/capture-2.json` with `captures/capture-2/block.json`
(the finalized slot the capture used and the captured block's timestamp) and `lez/session-b/logs/driver.log`
(each LEZ transaction's send-to-final time, each real proof's time) — plus the Sepolia constants the documents
name (beacon genesis, 12 s slots, finality two epochs after the checkpoint's slot; the last is the one *derived*
figure, marked so in both documents). RFP-022 P.6 is documented in `docs/performance.md` §P.6 with the records it
cites: the daemons' CPU, memory and bandwidth from `bench/perf/results/daemons-anvil/` (GNU time per daemon, the
5 s CPU/RSS samples, the counting proxies' JSON-RPC byte totals, the phase marks), the checkpoint prover's GPU
times and memory from `gpu-prover/results/*/run.json`, the laptop proof and its peak memory from
`checkpoint/results/prove.jsonl` and the committed GNU-time logs, the Groth16 wrap from
`checkpoint/results/groth16.jsonl`, and the live run's L40S checkpoint from `e2e/runs/crown-live-3/stages.done`,
`checkpoint/exec.txt`, `checkpoint/gpu/run.json` and the driver log.

Each test parses the raw files — never the prose of the documents — recomputes a figure and asserts it equals the
documented value at the document's own rounding, and asserts the document states it (the exact string, so a
document that drifts from its records fails). The derivations follow what the documents say each figure is.
Figures of P.6 that cannot be recomputed from committed data are asserted nowhere and named in comments at their
place: the GPU cost row's "385 L4-seconds billed" and "about 0.09 USD" (Modal's own billing statement of
10 November 2026; only the 366 s the functions themselves ran, which `gpu-prover/RESULTS.md` also states,
recomputes from the committed `run.json` files), the Groth16 wrap's "7.3 GiB" of prover memory (the Docker
container's `VmHWM`, polled live while the wrap ran; the committed `groth16.jsonl` records wall times only), and
the wrap row's "recursion 18.7 s on 16 cores + 14 s Groth16" (an end-to-end run cited to `docs/design/bridge.md`
§18.7, whose raw per-step timings are not committed). Standard library only."""

import json
import re
import statistics
import unittest
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The Sepolia constants both documents name for the one *derived* P.5 figure (finality two epochs after
# the checkpoint's slot; everything else is read from records). lez/results/perf-latency-bridge.md.
SEPOLIA_GENESIS = 1655733600  # beacon chain genesis time (Sepolia)
SLOT_S, EPOCH_SLOTS, FINALITY_EPOCHS = 12, 32, 2


def rd(p):
    return (ROOT / p).read_text()


def unix(s):
    return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()


def clock(t):
    return datetime.fromtimestamp(t, timezone.utc).strftime("%H:%M:%SZ")


def minutes(s):
    """The documents give the latency figures in minutes at one decimal (32.9 min, 71.0 min)."""
    return round(s / 60, 1)


def gnu_time(text):
    """The fields bench/perf/summarize_gate.py's `gnu_time` reads from a GNU time -v record."""
    out = {}
    for key, pat in [("user", r"User time \(seconds\): ([\d.]+)"), ("sys", r"System time \(seconds\): ([\d.]+)"),
                     ("rss_kb", r"Maximum resident set size \(kbytes\): (\d+)"),
                     ("wall", r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([\d:.]+)")]:
        m = re.search(pat, text)
        if m:
            out[key] = (sum(float(p) * 60 ** i for i, p in enumerate(reversed(m.group(1).split(":"))))
                        if key == "wall" else float(m.group(1)))
    return out


class Stated:
    """Asserts a document states a figure, matching its own characters (en dashes, commas, bold)."""

    def __init__(self, path):
        self.path, self.text = path, rd(path)

    def has(self, needle):
        if needle not in self.text:
            raise AssertionError(f"{self.path} does not state '{needle}'")
        return self


def load_stages():
    """stages.done: 'name end duration', the runner's own stamp of every stage (end time UTC, duration s)."""
    stages = {}
    for line in rd("e2e/runs/crown-live-3/stages.done").splitlines():
        name, end, dur = line.split()
        stages[name] = (unix(end), float(dur.rstrip("s")))
    return stages


def load_session_b():
    """Session B's driver log: every transaction's send-to-final time, every real proof's time, in order.

    The line format is the runner's (`bench/perf/latency_bridge.py` reads the same records): a proof line
    '[crown-b] LABEL: executed, proven and sent in N s' precedes its transaction's 'applied ... +Ns' line.
    """
    txs, proofs = [], {}
    for line in rd("e2e/runs/crown-live-3/lez/session-b/logs/driver.log").splitlines():
        m = re.search(r"\[crown-b\] (.*?): executed, proven and sent in ([\d.]+) s", line)
        if m:
            proofs[m.group(1)] = float(m.group(2))
            continue
        m = re.search(r"\[crown-b\] (.*?)\s+applied\s+gas\s+(\d+)\s+block\s+(\d+)\s+\+\s*([\d.]+)s", line)
        if m:
            label = m.group(1).strip()
            txs.append(dict(label=label, block=int(m.group(3)), final_s=float(m.group(4)),
                            proof_s=next((v for k, v in proofs.items() if label.startswith(k)), 0.0)))
    return txs


class TestP5Latency(unittest.TestCase):
    """Deposit to mint and burn to release (RFP-022 P.5), from crown-live-3's own records."""

    @classmethod
    def setUpClass(cls):
        cls.bridge_doc = Stated("lez/results/perf-latency-bridge.md")
        cls.perf_doc = Stated("docs/performance.md")
        cls.stages = load_stages()
        cap2 = json.loads(rd("e2e/runs/crown-live-3/captures/capture-2.json"))
        block2 = json.loads(rd("e2e/runs/crown-live-3/captures/capture-2/block.json"))["result"]
        cls.finalized_slot = cap2["finalized_slot"]
        cls.captured_block = cap2["block"]
        cls.block_time = int(block2["timestamp"], 16)
        cls.txs = txs = load_session_b()
        # The mint's time is not stamped anywhere: session B's stage end minus its duration gives its start,
        # and the transactions follow one another (the proof, then the send-to-final, each); whatever the
        # runner did before the first transaction (zone start, wallet sync) is the stage's remainder. This is
        # the derivation perf-latency-bridge.md describes, on the records alone.
        b_end, b_dur = cls.stages["lez-b"]
        busy = sum(t["final_s"] + t["proof_s"] for t in txs)
        t = b_end - b_dur + (b_dur - busy)
        for x in txs:
            t += x["proof_s"] + x["final_s"]
            x["at"] = t
        cls.claim1 = next(x for x in txs if x["label"].startswith("bridge ClaimPrivately"))
        cls.upd2 = [x for x in txs if x["label"].startswith("stage 2 finality update")]
        cls.imp2 = next(x for x in txs if x["label"].startswith("bridge ImportRoot n=2"))
        cls.burn = txs[-1]
        cls.d1_end = cls.stages["deposit1"][0]

    def test_the_records_themselves(self):
        """The stamps the documents quote: the deposit, the capture's finalized slot and its block."""
        self.assertEqual(clock(self.d1_end), "14:39:01Z")  # "deposit 1 included | 14:39:01Z"
        self.assertEqual(self.finalized_slot, 11563424)  # "finalized slot 11,563,424"
        self.assertEqual(self.captured_block, 12119519)  # "captured block 12,119,519 at 14:44:36Z"
        self.assertEqual(clock(self.block_time), "14:44:36Z")
        self.assertTrue(self.burn["label"].startswith("bridge Burn (private)"))
        self.assertEqual(self.burn["block"], 90)  # "block 90, the last of session B"

    def test_inbound_deposit_to_mint(self):
        """32.9 min from deposit 1 to its private claim applied; its parts 25.2, 18.6 and 6.6 min; 29.8 min steady."""
        total = self.claim1["at"] - self.d1_end
        self.assertEqual(minutes(total), 32.9)
        self.assertEqual(minutes(self.stages["capture2"][0] - self.d1_end), 25.2)  # captured
        # The one *derived* figure: Ethereum finalizes the checkpoint two epochs (768 s) after its slot.
        fin = SEPOLIA_GENESIS + self.finalized_slot * SLOT_S + FINALITY_EPOCHS * EPOCH_SLOTS * SLOT_S
        self.assertEqual(clock(fin), "14:57:36Z")  # "Ethereum finalizes it (*derived*) | 14:57:36Z"
        self.assertEqual(minutes(fin - self.d1_end), 18.6)  # source-chain finality wait
        self.assertEqual(minutes(self.stages["capture2"][0] - fin), 6.6)  # the update served and read, captured
        steady = ((fin - self.d1_end) + (self.stages["capture2"][0] - fin)
                  + sum(x["final_s"] for x in self.upd2) + self.imp2["final_s"]
                  + self.claim1["proof_s"] + self.claim1["final_s"])  # without the runner's other work
        self.assertEqual(minutes(steady), 29.8)
        self.bridge_doc.has("As it ran: **32.9 min** from the deposit to the mint").has("**29.8 min**")
        self.perf_doc.has("**32.9 min**").has("18.6 min").has("**29.8 min**")

    def test_inbound_with_the_clients_default_hold(self):
        """About 1.1–12.1 h with the client's default 1–12 h claim hold (derived; counted from the deposit's block)."""
        after_capture = (self.claim1["at"] - self.d1_end) - (self.stages["capture2"][0] - self.d1_end)
        self.assertEqual(minutes(after_capture), 7.7)  # "what followed the capture in this run (7.7 min)"
        # The shortest hold (1 h) outlasts finality and the capture (25.2 min < 60 min), so both bounds add it.
        self.assertLess(self.stages["capture2"][0] - self.d1_end, 3600)
        self.assertEqual(round((3600 + after_capture) / 3600, 1), 1.1)
        self.assertEqual(round((12 * 3600 + after_capture) / 3600, 1), 12.1)
        self.bridge_doc.has("the inbound is about 1.1 h to 12.1 h")
        self.perf_doc.has("about 1.1–12.1 h")

    def test_inbound_components(self):
        """The claim's proof 231.9 s, its send-to-final 8.3 s, the LEZ update 29.6 s, the import 10.0 s (39.6 s)."""
        self.assertEqual(round(self.claim1["proof_s"], 1), 231.9)
        self.assertEqual(round(self.claim1["final_s"], 1), 8.3)
        lez_update = sum(x["final_s"] for x in self.upd2)
        self.assertEqual(round(lez_update, 1), 29.6)
        self.assertEqual(round(self.imp2["final_s"], 1), 10.0)
        self.assertEqual(round(lez_update + self.imp2["final_s"], 1), 39.6)  # "LEZ steps before the claim"
        self.bridge_doc.has("proven in 231.9 s, then sent and final in 8.3 s")
        self.perf_doc.has("231.9 s").has("39.6 s").has("(7.7 min)")

    def test_outbound_burn_to_release(self):
        """71.0 min from the burn's proving, 66.3 from its sending, 66.2 from it final on LEZ."""
        r0_end = self.stages["release0"][0]
        b_end = self.stages["lez-b"][0]
        t_sent = b_end - self.burn["final_s"]
        t_proving = t_sent - self.burn["proof_s"]
        self.assertEqual(clock(b_end), "15:16:52Z")  # "block 90, the last of session B, 15:16:52Z"
        self.assertEqual(minutes(r0_end - t_proving), 71.0)
        self.assertEqual(minutes(r0_end - t_sent), 66.3)
        self.assertEqual(minutes(r0_end - b_end), 66.2)
        self.assertEqual(round(self.burn["proof_s"], 1), 282.8)
        self.assertEqual(round(self.burn["final_s"], 1), 7.0)
        self.assertEqual(clock(self.stages["release0"][0]), "16:23:01Z")  # the release stage's stamp
        self.bridge_doc.has("**71.0 min**; from its sending 66.3 min; from it final on LEZ 66.2 min")
        self.bridge_doc.has("proven in 282.8 s and final on LEZ in 7.0 s (block 90")
        self.perf_doc.has("**71.0 min** from the burn's proving (66.3 from its sending, 66.2 from it final on LEZ)")
        self.perf_doc.has("282.8 s")

    def test_outbound_w_release_delay_and_verification(self):
        """W 3,607 s (the row's 60.1 min), the release stage 156 s, verification 37 s, the checkpoint proofs 167 s."""
        w = self.stages["wait"][0] - self.stages["submit"][0]
        self.assertEqual(w, 3607.0)
        self.assertEqual(minutes(w), 60.1)  # the row: "of which W 60.1 min"
        self.assertEqual(self.stages["release0"][1], 156.0)  # "the release delay (120 s), then `release`"
        self.assertEqual(self.stages["submit"][1] + self.stages["finalize"][1], 37.0)  # submitCandidate + finalizeAttested
        self.assertEqual(self.stages["checkpoint-prove"][1] + self.stages["checkpoint-wrap"][1], 167.0)
        self.assertEqual(self.stages["checkpoint-prove"][1], 119.0)  # the L40S stage: "119 s with the queue"
        self.bridge_doc.has("| 3607 | 85% |")
        self.perf_doc.has("3,607 s (W = 1 h of block time)").has("156 s (120 s delay and the release)")
        self.perf_doc.has("37 s (`submitCandidate`, `finalizeAttested`)")
        self.perf_doc.has("+ 167 s (checkpoint on one L40S and the Groth16 wrap)")


class TestP6Daemons(unittest.TestCase):
    """`monitor` and `ferrata-relayer` (RFP-022 P.6): CPU, memory and bandwidth, from the anvil run's raw records."""

    @classmethod
    def setUpClass(cls):
        cls.doc = Stated("lez/results/perf-daemons.md")
        cls.perf = Stated("docs/performance.md")
        cls.times = {n: gnu_time(rd(f"bench/perf/results/daemons-anvil/time-{n}.txt")) for n in ("monitor", "relayer")}
        cls.rpc = {n: json.loads(rd(f"bench/perf/results/daemons-anvil/rpc-{n}.json")) for n in ("monitor", "relayer")}

    def daemon(self, name, cpu_s, wall_s, share_5, share_4, peak_mib, up, down, per_day_cpu, per_day_mb):
        t = self.times[name]
        cpu = t["user"] + t["sys"]
        self.assertEqual(round(cpu, 2), cpu_s)
        self.assertEqual(round(t["wall"]), wall_s)
        self.assertEqual(round(cpu / t["wall"], 5), share_5)  # perf-daemons.md's table, five decimals
        self.assertEqual(round(cpu / t["wall"], 4), share_4)  # performance.md and the row, four
        self.assertEqual(round(t["rss_kb"] / 1024, 1), peak_mib)  # peak RSS, MiB
        self.assertEqual((self.rpc[name]["up"], self.rpc[name]["down"]), (up, down))
        day = 86400 / t["wall"]
        self.assertEqual(round(cpu * day), per_day_cpu)  # CPU s per day
        self.assertEqual(round((self.rpc[name]["up"] + self.rpc[name]["down"]) * day / 1e6, 1), per_day_mb)  # MB per day

    def test_monitor(self):
        """0.53 CPU-s in 1,284 s (0.0004 of a core), 14.0 MiB peak, 86 KB up / 254 KB down (about 23 MB a day)."""
        self.daemon("monitor", 0.53, 1284, 0.00041, 0.0004, 14.0, 86265, 254218, 36, 22.9)
        self.assertEqual(round((86265 + 254218) * 86400 / self.times["monitor"]["wall"] / 1e6), 23)
        self.doc.has("| monitor | 1284 | 0.53 | 0.00041 | 14.0 | 14.0 | 86,265 / 254,218 | 36 / 22.9 |")
        self.perf.has("0.53 CPU-seconds in 1,284 s (0.0004 of a core)").has("**14.0 MiB peak**")
        self.perf.has("JSON-RPC 86 KB up, 254 KB down (about 23 MB a day, *derived*)")

    def test_relayer(self):
        """0.87 CPU-s in 1,284 s (0.0007 of a core), 20.3 MiB peak, 408 KB up / 976 KB down (about 93 MB a day)."""
        self.daemon("relayer", 0.87, 1284, 0.00068, 0.0007, 20.3, 408005, 975682, 59, 93.1)
        self.assertEqual(round((408005 + 975682) * 86400 / self.times["relayer"]["wall"] / 1e6), 93)
        self.doc.has("| relayer | 1284 | 0.87 | 0.00068 | 20.3 | 20.0 | 408,005 / 975,682 | 59 / 93.1 |")
        self.perf.has("0.87 CPU-seconds in 1,284 s (0.0007 of a core)").has("**20.3 MiB peak**")
        self.perf.has("JSON-RPC 408 KB up, 976 KB down (about 93 MB a day, *derived*)")

    def test_samples_and_phases(self):
        """The 5 s samples and the phase marks: the medians and the per-phase CPU the record's tables state."""
        samples = {}
        for line in rd("bench/perf/results/daemons-anvil/samples.tsv").splitlines()[1:]:
            f = line.split("\t")
            samples.setdefault(f[1], []).append((float(f[0]), float(f[3]), int(f[4])))  # unix, cpu_s, rss_kb
        phases = [(float(l.split("\t")[0]), l.split("\t", 1)[1]) for l in
                  rd("bench/perf/results/daemons-anvil/phases.tsv").splitlines()]
        self.assertEqual([round(t - phases[0][0]) for t, _ in phases[1:]], [600, 681, 1281])
        self.doc.has("activity start at +600 s").has("idle-2 end at +1281 s")
        medians = {n: round(statistics.median(x[2] for x in v) / 1024, 1) for n, v in samples.items()}
        self.assertEqual(medians, {"monitor": 14.0, "relayer": 20.0})  # "median RSS MiB"
        # Per phase, the CPU seconds between the phase marks, from the samples, and the share of one core.
        cells = {("monitor", 0): (0.16, 595, 0.0003), ("monitor", 1): (0.03, 80, 0.0004),
                 ("monitor", 2): (0.06, 595, 0.0001), ("relayer", 0): (0.15, 595, 0.0003),
                 ("relayer", 1): (0.07, 80, 0.0009), ("relayer", 2): (0.18, 595, 0.0003)}
        for name in samples:
            s = samples[name]
            for i, ((t0, _), (t1, _)) in enumerate(zip(phases, phases[1:])):
                inside = [x for x in s if t0 <= x[0] <= t1]
                cpu, span = inside[-1][1] - inside[0][1], inside[-1][0] - inside[0][0]
                want_cpu, want_span, want_share = cells[(name, i)]
                self.assertEqual((round(cpu, 2), round(span), round(cpu / span, 4)), (want_cpu, want_span, want_share))
        self.doc.has("| monitor | 0.16 s over 595 s (0.0003 of a core) | 0.03 s over 80 s (0.0004 of a core) |"
                     " 0.06 s over 595 s (0.0001 of a core) |")
        self.doc.has("| relayer | 0.15 s over 595 s (0.0003 of a core) | 0.07 s over 80 s (0.0009 of a core) |"
                     " 0.18 s over 595 s (0.0003 of a core) |")


class TestP6Provers(unittest.TestCase):
    """The checkpoint prover's times and memory (RFP-022 P.6), from the committed run records."""

    @classmethod
    def setUpClass(cls):
        cls.perf = Stated("docs/performance.md")
        cls.gpu = Stated("gpu-prover/RESULTS.md")
        cls.ck = Stated("checkpoint/RESULTS.md")
        cls.l4 = {n: json.loads(rd(f"gpu-prover/results/gpu-l4/{n}/run.json"))
                  for n in ("cp-empty-1", "cp-empty-360", "cp-pp5-360", "cp-pp50-360")}
        cls.rb = json.loads(rd("gpu-prover/results/ruleb-l4b/rb-empty-360-n4/run.json"))
        cls.laptop = [json.loads(l) for l in rd("checkpoint/results/prove.jsonl").splitlines()]
        cls.wraps = [json.loads(l) for l in rd("checkpoint/results/groth16.jsonl").splitlines()]
        cls.live = json.loads(rd("e2e/runs/crown-live-3/checkpoint/gpu/run.json"))

    def test_one_l4_checkpoint_prover(self):
        """14.0 / 24.1 / 41.4 / 219.3 s; GPU memory about 10.9 GiB, host 0.76–1.08 GiB."""
        totals = {n: round(self.l4[n]["prove"]["total_s"], 1) for n in self.l4}
        self.assertEqual(totals, {"cp-empty-1": 14.0, "cp-empty-360": 24.1, "cp-pp5-360": 41.4, "cp-pp50-360": 219.3})
        self.assertEqual({n: self.l4[n]["prove"]["segments"] for n in self.l4},
                         {"cp-empty-1": 4, "cp-empty-360": 7, "cp-pp5-360": 12, "cp-pp50-360": 60})
        gpu_mib = sorted(self.l4[n]["gpu_stats"]["gpu_memory_used_mib_peak"] for n in self.l4)
        self.assertEqual(gpu_mib, [11076, 11140, 11158, 11159])  # "11,076–11,159 MiB (10.8–10.9 GiB)"
        self.assertEqual((round(gpu_mib[0] / 1024, 1), round(gpu_mib[-1] / 1024, 1)), (10.8, 10.9))
        host_gib = sorted(round(int(self.l4[n]["gnu_time"]["max_rss_kib"]) / 1024 / 1024, 2) for n in self.l4)
        self.assertEqual((host_gib[0], host_gib[-1]), (0.76, 1.08))  # "the host process peaks at 0.76–1.08 GiB"
        self.perf.has("14.0 s (fixed part), 24.1 s (idle hour), 41.4 s (5 private tx), 219.3 s (50 private tx)")
        self.perf.has("about 10.9 GiB GPU, 0.76–1.08 GiB host")
        self.gpu.has("GPU memory peaks at 11,076–11,159 MiB (10.8–10.9 GiB)")
        self.gpu.has("host process peaks at\n0.76–1.08 GiB (GNU time)") if "host process peaks at\n0.76–1.08 GiB (GNU time)" in self.gpu.text \
            else self.gpu.has("0.76–1.08 GiB (GNU time)")
        for n, total in totals.items():
            self.gpu.has(f"**{total} s**")

    def test_the_same_on_a_laptop_cpu(self):
        """1,157.5 s starting quiet, 1,758.9 s under load, 9.62 GB peak; the extrapolations the documents label."""
        walls = sorted(round(r["wall_s"], 1) for r in self.laptop if r["name"] == "cp-empty-1")
        self.assertEqual(walls, [1157.5, 1758.9])  # "1,157.5–1,758.9 s fixed part"
        quiet = gnu_time(rd("checkpoint/results/logs/20261110T055305Z-prove-cp-empty-1.log"))  # the quiet-start run
        loaded = gnu_time(rd("checkpoint/results/logs/20261110T023312Z-prove-cp-empty-1.log"))  # the run under load
        for log in (quiet, loaded):
            self.assertEqual(round(log["rss_kb"] / 1e6, 2), 9.62)  # "peak 9.62 GB (host + r0vm)"
        # The documents' own extrapolation at 289 s per segment (1,157.5 s / 4):
        per_segment = round(1157.5 / 4)
        self.assertEqual(per_segment, 289)
        self.assertEqual(round(7 * per_segment / 60), 34)  # "≈ 34 min per idle hour"
        self.assertEqual(round(60 * per_segment / 3600, 1), 4.8)  # "≈ 4.8 h per busy hour"
        self.assertEqual(7 * per_segment, 2023)  # gpu-prover's "≈ 2,023 s"
        self.perf.has("9.62 GB peak")
        self.perf.has("1,157.5–1,758.9 s fixed part; ≈ 34 min per idle hour and ≈ 4.8 h per busy hour (extrapolated)")
        self.gpu.has("1,157.5 s quiet start / 1,758.9 s loaded")
        self.ck.has("1,157.5 s").has("1,758.9 s").has("9.62 GB (host + r0vm)")

    def test_rule_b_checkpoint_on_one_l4(self):
        """31.9 s for the idle hour's rule (B) checkpoint (n = 4, 9 segments)."""
        self.assertEqual(round(self.rb["prove"]["total_s"], 1), 31.9)
        self.assertEqual(self.rb["prove"]["segments"], 9)
        self.assertTrue(self.rb["name"].endswith("-n4"))  # the fixture's own name: n = 4
        self.perf.has("31.9 s (idle hour, `n` = 4, 9 segments)")

    def test_the_live_runs_checkpoint_on_one_l40s(self):
        """crown-live-3's rule (B) checkpoint: 31.9 s prover wall (119 s with queue and transfers), 18,499,940 cycles."""
        self.assertEqual(round(self.live["prover_wall_s"], 1), 31.9)
        self.assertEqual(self.live["prove"]["user_cycles"], 18499940)  # exec.txt holds the same, executed natively
        self.assertEqual(self.live["prove"]["segments"], 20)
        exec_txt = json.loads(rd("e2e/runs/crown-live-3/checkpoint/exec.txt"))
        self.assertEqual((exec_txt["user_cycles"], exec_txt["segments"], exec_txt["journal_equals_native"]),
                         (18499940, 20, True))
        self.assertEqual(load_stages()["checkpoint-prove"][1], 119.0)  # the stage: queue and transfers included
        driver = rd("e2e/runs/crown-live-3/lez/session-b/logs/driver.log")
        self.assertIn("blocks 25..=90", driver)
        self.assertIn("2 burn note(s) written", driver)
        self.perf.has("crown-live-3`: LEZ blocks 25..=90, 2 burns, 18,499,940 user cycles, 20 segments")
        self.perf.has("31.9 s prover wall (119 s with the queue and transfers)")

    def test_the_groth16_wrap_times(self):
        """195.1 s and 215.2 s on the laptop. Two figures of the wrap row are NOT recomputable: see the comments."""
        walls = sorted(round(r["wall_s"], 1) for r in self.wraps)
        self.assertEqual(walls, [195.1, 215.2])
        for r in self.wraps:  # what the committed record of the wrap holds (times, not memory):
            self.assertEqual((r["name"], r["seal_bytes"], r["selector"]), ("cp-empty-1", 260, "0x73c457ba"))
        self.perf.has("195.1–215.2 s (laptop)")
        self.ck.has("Groth16 wrap 215.2 s / 195.1 s under load")
        # NOT recomputable from committed data, asserted nowhere: performance.md's wrap row states the prover
        # container's "7.3 GiB" of memory (the `VmHWM` inside the Docker container, polled live while the wrap
        # ran — checkpoint/RESULTS.md, "Real proof"; the committed groth16.jsonl records wall times only, and
        # the GNU-time logs of the wrap show the host's 1.44 GB, not the container's peak), and the same row's
        # "recursion 18.7 s on 16 cores + 14 s Groth16" of an end-to-end run, cited to docs/design/bridge.md
        # §18.7, whose raw per-step timings are not committed.

    def test_the_gpu_cost_row(self):
        """Only the 366 s the functions ran recomputes; the 385 L4-seconds billed do not (Modal's statement)."""
        container_s = sum(self.l4[n]["container_s_until_done"] for n in self.l4)
        container_s += json.loads(rd("gpu-prover/results/gpu-l4/lez-update/run.json"))["container_s_until_done"]
        self.assertEqual(round(container_s), 366)  # "The functions themselves ran 366 s"
        self.gpu.has("The functions themselves ran 366 s")
        # NOT recomputable from committed data, asserted nowhere: performance.md's cost row states "385
        # L4-seconds billed ... about 0.09 USD of GPU", from Modal's own billing statement of 10 November 2026
        # (gpu-prover/RESULTS.md, "Cost"); container start-up, which the committed run records do not measure,
        # makes up the difference between the billed 385 s and the 366 s above.


if __name__ == "__main__":
    unittest.main()
