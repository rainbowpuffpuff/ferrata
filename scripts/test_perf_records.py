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

What is asserted, exactly:

- **The three generated records, byte for byte.** `lez/results/perf-latency-bridge.md` is
  `bench/perf/latency_bridge.py`'s output on the committed records, `lez/results/perf-daemons.md` is its
  hand-written header followed by `bench/perf/summarize_daemons.py`'s, and `lez/results/perf-gate-sequencer.md`
  (whence the §P.6 table's updater row takes its figures, RFP-021 P.6's) is `bench/perf/gate_report.py`'s output
  whole. Every *computed* figure of all three documents is thereby recomputed, at its generator's own rounding,
  and a stamp, slot or table cell that drifts fails; the figures the generators hard-code in their fixed prose
  ("6 slots", "121.6 min", the testnet and production parameters) are checked as text only, by the byte
  comparison.
- **`docs/performance.md` §P.5's table and §P.6's rows**, figure by figure from the raw records (each derivation
  follows what the document says the figure is), each with the document's exact string asserted — every figure
  the two rows state, and every number of the tables they cite that comes from these records. Each string is
  asserted with its count of occurrences, so a copy of it elsewhere (the updater row's figures also appear in
  RFP-021 §P.6) cannot mask the drift. Of the figures the rows' own status and Evidence lines state
  (`docs/requirements-status.md`, `docs/EVIDENCE.md` rows 174 and 175), the test asserts the headlines: the
  latencies and W of the P.5 line and the daemons' four figures, read as those files state them. The one
  *derived* figure (Ethereum finalizing two epochs after the checkpoint's slot) uses the Sepolia constants both
  documents name for it.

Figures that cannot be recomputed from committed data are asserted nowhere and named precisely where they belong:
the §P.6 cost row's "385 L4-seconds billed" and "about 0.09 USD" (Modal's own billing statement of 10 November
2026; only the 366 s the functions themselves ran recomputes, from the committed `run.json` files — the wrap's
"7.3 GiB" of prover memory *does* recompute, from the committed wrap logs' `VmHWM`); the §P.6 wrap row's
"recursion 18.7 s on 16 cores + 14 s Groth16" of an end-to-end run, cited to `docs/design/bridge.md` §18.7, whose
raw per-step timings are not committed; and, in §P.5, the production-parameter estimates (the design's rule
(A)/(B) values and the finality simulator's medians, cited to `bridge.md` §9, §11.7) and the closing sentence's
crown-live-1 "64.7 min" (the first run's own README's account; that run's `stages.done` stamps its release-0
stage at the runner's late re-check — "0s (already done)" — so the release block's time is not in its stamps).
Standard library only."""

import json
import re
import statistics
import subprocess
import sys
import unittest
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The Sepolia constants both documents name for the one *derived* P.5 figure (finality two epochs after
# the checkpoint's slot; everything else is read from records). lez/results/perf-latency-bridge.md.
SEPOLIA_GENESIS = 1655733600  # beacon chain genesis time (Sepolia)
SLOT_S, EPOCH_SLOTS, FINALITY_EPOCHS = 12, 32, 2


def rd(p):
    return (ROOT / p).read_text(encoding="utf-8")


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

    def has(self, needle, count=1):
        """The document states `needle` exactly `count` times (default once: a copy of it elsewhere in the
        same document cannot mask a drift of the row being asserted — RFP-021 §P.6 repeats the updater row)."""
        n = self.text.count(needle)
        if n != count:
            raise AssertionError(f"{self.path} states '{needle}' {n} time(s), not {count}")
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
        self.perf_doc.has("**32.9 min**").has("18.6 min (Ethereum: the deposit to its checkpoint's finalization")
        self.perf_doc.has("6.6 min (the covering finality update served and read").has("(25.2 min) elapse inside it")
        self.perf_doc.has("**29.8 min**")

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
        self.perf_doc.has("8.3 s (the claim, send to final)")

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
        self.perf_doc.has("282.8 s").has("7.0 s (LEZ: the burn sent to final on Bedrock")

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
        # The rows' own status and Evidence lines state the headlines; check they state what the records give.
        Stated("docs/requirements-status.md").has(
            "inbound 32.9 min, about 1.1–12.1 h with the client's default claim hold (derived); "
            "outbound 71.0 min from the burn's proving, of which W 60.1 min")
        Stated("docs/EVIDENCE.md").has(
            "inbound 32.9 min (finality 18.6, availability and capture 6.6, LEZ steps 0.7, proof 3.9, "
            "verification 0.1, the runner's other work 3.0); outbound 71.0 min from the burn's proving")
        Stated("docs/EVIDENCE.md").has("W 60.1, release 2.6")


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
        Stated("docs/requirements-status.md").has(
            "`monitor` (0.0004 of a core, 14.0 MiB) and the relayer (0.0007 of a core, 20.3 MiB) measured with their bandwidth")
        Stated("docs/EVIDENCE.md").has("`monitor` 0.53 CPU-s in 1,284 s (0.0004 of a core), 14.0 MiB peak, "
                                       "86,265 / 254,218 B; relayer 0.87 CPU-s (0.0007), 20.3 MiB, 408,005 / 975,682 B")

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
        self.gpu.has("0.76–1.08 GiB (GNU time)")
        for n, total in totals.items():
            self.gpu.has(f"**{total} s**")

    def test_the_same_on_a_laptop_cpu(self):
        """1,157.5 s starting quiet, 1,758.9 s under load, 9.85 GB peak; the extrapolations the documents label."""
        walls = sorted(round(r["wall_s"], 1) for r in self.laptop if r["name"] == "cp-empty-1")
        self.assertEqual(walls, [1157.5, 1758.9])  # "1,157.5–1,758.9 s fixed part"
        quiet = gnu_time(rd("checkpoint/results/logs/20261110T055305Z-prove-cp-empty-1.log"))  # the quiet-start run
        loaded = gnu_time(rd("checkpoint/results/logs/20261110T023312Z-prove-cp-empty-1.log"))  # the run under load
        for log in (quiet, loaded):
            # GNU time's "Maximum resident set size (kbytes)" is KiB: 9,617,860 KiB is 9.85 GB (9.17 GiB),
            # not the "9.62 GB" the documents said before this branch corrected them (a /1e6 misreading).
            self.assertEqual(round(log["rss_kb"] * 1024 / 1e9, 2), 9.85)
        # The documents' own extrapolation from the measured 1,157.5 s over the 4 segments of cp-empty-1:
        per_segment = round(min(r["wall_s"] for r in self.laptop if r["name"] == "cp-empty-1")
                            / self.l4["cp-empty-1"]["prove"]["segments"])
        idle, busy = self.l4["cp-empty-360"]["prove"]["segments"], self.l4["cp-pp50-360"]["prove"]["segments"]
        self.assertEqual(per_segment, 289)
        self.assertEqual(round(idle * per_segment / 60), 34)  # "≈ 34 min per idle hour"
        self.assertEqual(round(busy * per_segment / 3600, 1), 4.8)  # "≈ 4.8 h per busy hour"
        self.assertEqual(idle * per_segment, 2023)  # gpu-prover's "≈ 2,023 s"
        self.perf.has("9.85 GB peak")
        self.perf.has("1,157.5–1,758.9 s fixed part; ≈ 34 min per idle hour and ≈ 4.8 h per busy hour (extrapolated)")
        self.gpu.has("1,157.5 s quiet start / 1,758.9 s loaded").has("against 9.85 GB for the CPU proof")
        self.ck.has("1,157.5 s", 3).has("1,758.9 s", 2).has("9.85 GB (host + r0vm)", 3)

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
        """195.1 s and 215.2 s on the laptop, the prover container's peak 7.3 GiB; one figure is not recomputable."""
        walls = sorted(round(r["wall_s"], 1) for r in self.wraps)
        self.assertEqual(walls, [195.1, 215.2])
        for r in self.wraps:  # what the committed record of the wrap holds:
            self.assertEqual((r["name"], r["seal_bytes"], r["selector"]), ("cp-empty-1", 260, "0x73c457ba"))
        logs = {n: rd(f"checkpoint/results/logs/{n}") for n in
                ("20261110T030244Z-groth16-cp-empty-1.log", "20261110T061223Z-groth16-cp-empty-1.log")}
        # The wrap's memory, recomputed from the committed wrap logs: the prover container's largest process
        # peak (VmHWM), 7.30 and 7.31 GiB, and the host process's GNU-time peak, 1.48 GB (1,444,084 and
        # 1,443,392 KiB — "1.44 GB" before this branch corrected the same /1e6 misreading as the 9.62 GB).
        hwm = sorted(float(re.search(r"largest process peak RSS \(VmHWM\) ([\d.]+) GiB", t).group(1)) for t in logs.values())
        self.assertEqual(hwm, [7.30, 7.31])
        self.assertEqual({round(h, 1) for h in hwm}, {7.3})  # the documents' "7.3 GiB"
        cgroup = sorted(float(re.search(r"cgroup memory\.peak \(max over polls\) ([\d.]+) GiB", t).group(1)) for t in logs.values())
        self.assertEqual(cgroup, [7.36, 8.48])  # checkpoint/RESULTS.md's two wrap rows
        for t in logs.values():
            self.assertEqual(round(gnu_time(t)["rss_kb"] * 1024 / 1e9, 2), 1.48)  # "host + r0vm 1.48 GB"
        self.perf.has("195.1–215.2 s (laptop)").has("| x86 | 7.3 GiB |")
        self.ck.has("Groth16 wrap 215.2 s / 195.1 s under load").has("Groth16 prover 7.3 GiB")
        self.ck.has("host + r0vm 1.48 GB; container seen 48.2 s, cgroup peak 8.48 GiB (incl. page cache), "
                    "`prover` VmHWM 7.30 GiB")
        self.ck.has("host + r0vm 1.48 GB; container seen 53.8 s, cgroup peak 7.36 GiB, `prover` VmHWM 7.31 GiB")
        # NOT recomputable from committed data, asserted nowhere: the same row's "recursion 18.7 s on 16
        # cores + 14 s Groth16" of an end-to-end run, cited to docs/design/bridge.md §18.7, whose raw
        # per-step timings are not committed.

    def test_the_release_proofs_peak_memory(self):
        """RFP-022 P.4's release proofs' peak, corrected with the same KiB misreading as the 9.62 GB ("1.46 GB" was
        KiB / 1e6). v1's three succinct proofs (bridge/RESULTS.md: 43.2, 109.4 and 95.8 s; 558-622 CPU-s) peak at
        1,458,420 KiB = 1.49 GB; v2's nine (the 80,501-cycle guest of bridge.md section 18.2, performance.md's P.4
        row, the user guide and bridge/RESULTS.md's "9 runs logged": 519-651 CPU-s, 37.1-48.6 s with most of the
        threads) peak at 1,461,332 KiB = 1.50 GB; both 1.39 GiB."""
        v1 = [gnu_time(rd(f"bridge/artifacts/v1/logs/{n}")) for n in
              ("20261109T213444Z-prove-eth.log", "20261109T213648Z-prove-erc20.log", "20261109T220505Z-prove-eth.log")]
        v2 = [gnu_time(rd(f"bridge/artifacts/logs/20261110T{n}.log")) for n in
              ("021234Z-prove-eth", "021505Z-prove-erc20", "021705Z-prove-ethpull", "030949Z-prove-eth",
               "031026Z-prove-erc20", "031106Z-prove-ethpull", "062310Z-prove-eth", "062804Z-prove-erc20",
               "063113Z-prove-ethpull")]
        p1, p2 = max(t["rss_kb"] for t in v1), max(t["rss_kb"] for t in v2)
        self.assertEqual((p1, p2), (1458420, 1461332))
        self.assertEqual((round(p1 * 1024 / 1e9, 2), round(p2 * 1024 / 1e9, 2)), (1.49, 1.50))
        self.assertEqual((round(p1 / 1024 / 1024, 2), round(p2 / 1024 / 1024, 2)), (1.39, 1.39))
        self.assertEqual((round(min(t["user"] for t in v1)), round(max(t["user"] for t in v1))), (558, 622))
        self.assertEqual((round(min(t["user"] for t in v2)), round(max(t["user"] for t in v2))), (519, 651))
        br = Stated("bridge/RESULTS.md")
        br.has("peak RSS 1.49 GB (host + r0vm)").has("1.49 GB peak RSS (r0vm 3.0.5")  # v1
        br.has("1.50 GB peak RSS (one 2^17 segment")  # v2, "over the 9 runs logged"
        self.perf.has("peak 1.50 GB, receipt 223,930 B")  # P.4's table, "release (succinct), earlier image"
        Stated("docs/design/bridge.md").has("peak RSS 1.50 GB; receipt 223,930 bytes")  # section 18.2's row
        Stated("docs/guides/user-guide.md").has("peak 1.50 GB, receipt 223,930 bytes")

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


class TestGeneratedRecords(unittest.TestCase):
    """The three generated records, byte for byte: each generator, run on the committed records, gives the
    document. This recomputes every computed figure of `lez/results/perf-latency-bridge.md` (P.5's stamps,
    slots, tables), of `lez/results/perf-daemons.md` (the daemons' every cell) and of
    `lez/results/perf-gate-sequencer.md` (whence performance.md §P.6's updater row takes its figures), at the
    generator's own rounding; the generators' own hard-coded prose is checked as text only."""

    @classmethod
    def setUpClass(cls):
        cls.perf = Stated("docs/performance.md")

    def gen(self, script, *args):
        return subprocess.run([sys.executable, "-I", script, *args], cwd=ROOT, capture_output=True,
                              encoding="utf-8", check=True).stdout

    def test_perf_latency_bridge_is_its_generator_output(self):
        self.assertEqual(self.gen("bench/perf/latency_bridge.py", "e2e/runs/crown-live-3"),
                         rd("lez/results/perf-latency-bridge.md"))

    def test_perf_daemons_body_is_its_generator_output(self):
        out = self.gen("bench/perf/summarize_daemons.py", "bench/perf/results/daemons-anvil")
        doc = rd("lez/results/perf-daemons.md")
        self.assertTrue(out.startswith("## Run `daemons-anvil`"))
        self.assertTrue(doc.endswith(out))  # what precedes is the record's hand-written header

    def test_perf_gate_sequencer_body_is_its_generator_output(self):
        """The gate record is entirely generator output (no hand-written header): equality, not a suffix."""
        out = self.gen("bench/perf/gate_report.py", *(f"bench/perf/results/gate-g{i}" for i in (2, 3, 4, 5)))
        self.assertEqual(out, rd("lez/results/perf-gate-sequencer.md"))

    def test_the_updater_row_of_the_p6_table(self):
        """0.043–0.092 of a core and 57.3–57.8 MiB peak (the honest daemons of the gate runs), 10.3–10.4 MB in
        31 min (gate G2's honest daemons' beacon bytes over their 1,849 s wall)."""
        doc = rd("lez/results/perf-gate-sequencer.md")

        def daemon_rows(section):
            """The daemon table of a run section: the rows after the header that names 'CPU share of one core'."""
            out, in_table = [], False
            for line in section.splitlines():
                if line.startswith("| daemon |") and "CPU share of one core" in line:
                    in_table = True
                    continue
                if in_table:
                    if not line.startswith("|"):
                        break
                    if line.startswith("| honest-"):
                        out.append(line.split("|"))
            return out

        honest = [r for run in doc.split("## Run `gate-")[1:] for r in daemon_rows(run)]
        shares = sorted(float(f[6]) for f in honest)     # "CPU share of one core"
        peaks = sorted(float(f[7]) for f in honest)      # "peak RSS MiB"
        self.assertEqual((round(shares[0], 3), round(shares[-1], 3)), (0.043, 0.092))
        self.assertEqual((round(peaks[0], 1), round(peaks[-1], 1)), (57.3, 57.8))
        g2 = daemon_rows(doc.split("## Run `gate-g2`", 1)[1])
        for f, mb in zip(g2, (10.3, 10.4)):  # "beacon API: requests / bytes served", over the "wall s" column
            self.assertEqual(round(int(f[9].split("/")[1].strip().replace(",", "")) / 1e6, 1), mb)
            self.assertEqual(round(int(f[4].strip().replace(",", "")) / 60), 31)  # 1,849 s ≈ 31 min
        # The replay-rate gloss the row states: the recorded updates' median spacing over the reveal step.
        g2_text = doc.split("## Run `gate-g2`", 1)[1].split("## Run `gate-", 1)[0]
        m = re.search(r"so the replay runs about (\d+) times faster than the lab recorded them and (\d+) times", g2_text)
        self.assertEqual((m.group(1), m.group(2)), ("122", "60"))
        # The whole row, as one string: it must appear exactly once, so the same figures copied into another
        # table (RFP-021 §P.6) cannot mask a drift of this one.
        self.perf.has("| the updater, whose records the root imports need | 0.043–0.092 of one core | 57.3–57.8 MiB peak | "
                      "10.3–10.4 MB from the beacon API in 31 min (gate G2, replayed about 122 times faster than the lab "
                      "recorded them, 60 times the design's hourly cadence)")


class TestGate100(unittest.TestCase):
    """`lez/results/perf-gate-sequencer-100.md` (RFP-021 R.9 and R.10 at A3's size: runs G6 and G7, 106 recorded
    updates, windows 8 and 30): a hand-written header, then `bench/perf/summarize_gate.py`'s output on the two runs,
    byte for byte; the runs are the ones the header describes (each `machine.txt`, `done.txt`, `reveals.tsv`, two
    daemons); A3's two figures, the steps applied twice and the claim that every refused step raced an applied step of
    its own block are recomputed from the journals themselves; the header's other figures are the generator's (its
    `summarize`). Each figure is asserted as the header's own string (whitespace folded), with its count."""

    RUNS = (("gate-g6", 8), ("gate-g7", 30))
    DOC = "lez/results/perf-gate-sequencer-100.md"

    @classmethod
    def setUpClass(cls):
        dirs = [f"bench/perf/results/{name}" for name, _ in cls.RUNS]
        cls.out = subprocess.run([sys.executable, "-I", "bench/perf/summarize_gate.py", *dirs], cwd=ROOT,
                                 capture_output=True, encoding="utf-8", check=True).stdout
        cls.doc = rd(cls.DOC)
        head = cls.doc[: len(cls.doc) - len(cls.out)] if cls.doc.endswith(cls.out) else ""
        cls.header = " ".join(head.split())
        sys.path.insert(0, str(ROOT / "bench" / "perf"))
        import contextlib
        import io
        import summarize_gate
        with contextlib.redirect_stdout(io.StringIO()):
            cls.keys = {name: summarize_gate.summarize(ROOT / d) for (name, _), d in zip(cls.RUNS, dirs)}
        cls.journals = {}
        for name, _ in cls.RUNS:
            rows = []
            for d in ("honest-1", "honest-2"):
                for line in rd(f"bench/perf/results/{name}/state-{d}/journal.tsv").splitlines():
                    f = line.split("\t")
                    if len(f) >= 12:  # round, step, job, slot, status, exit, gas exec, gas storage, fee, block, t, label
                        rows.append(dict(daemon=d, step=f[1], job=f[2], status=f[4], exit=f[5], gas=int(f[6] or 0),
                                         fee=int(f[8] or 0), block=int(f[9] or -1), label=f[11]))
            cls.journals[name] = rows

    def has(self, needle, count=1):
        n = self.header.count(needle)
        self.assertEqual(n, count, f"{self.DOC}'s header states '{needle}' {n} time(s), not {count}")

    def test_the_record_is_its_header_then_the_generator_output(self):
        self.assertTrue(self.out.startswith("## Run `gate-g6`"))
        self.assertTrue(self.doc.endswith(self.out))
        self.assertTrue(self.header.startswith("# RFP-021 R.9 and R.10 at a hundred updates"))
        self.assertNotIn("## Run", self.header)

    def test_the_runs_are_the_ones_described(self):
        for name, window in self.RUNS:
            run = f"bench/perf/results/{name}"
            machine = rd(f"{run}/machine.txt")
            self.assertIn(f"\nGATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW={window} GATE_POLL=4 GATE_ADVERSARY=0\n", machine)
            self.assertIn("\nFAKETIME=-5d ", machine)
            self.assertIn("FAKETIME_DONT_FAKE_MONOTONIC=1)", machine)
            self.assertEqual(rd(f"{run}/done.txt"), "done=1\n")
            kinds = [line.split("\t")[2] for line in rd(f"{run}/reveals.tsv").splitlines()[1:]]
            self.assertEqual((len(kinds), kinds.count("finality"), kinds.count("period")), (106, 96, 10))
            self.assertEqual(sorted(p.name for p in (ROOT / run).glob("state-*")), ["state-honest-1", "state-honest-2"])
        self.has("each run reveals 106 of the lab's recorded mainnet updates (96 finality updates and 10 sync-committee "
                 "period updates; `GATE_ENTRIES=106`; A3's gate asks 100), one every 60 s (`GATE_STEP=60`)")
        self.has("no adversary (`GATE_ADVERSARY=0`); `gate-g6` at the default deferral window of 8 rounds, `gate-g7` at 30")
        self.has("(`FAKETIME=-5d`, monotonic clocks real)")

    def test_the_reveals_are_the_recorded_updates(self):
        """Each run's reveals.tsv is the lab's recorded mainnet updates in the replay's order (sorted names), entry by
        entry, with the slots bench/perf/beacon_replay.py reads from each SSZ file; each UTC stamp is its own Unix time's
        (truncated to the second, the Unix time rounded to the millisecond), the times rising; and the header names each
        run's updater build as its machine.txt and hook.log do."""
        import beacon_replay
        expect = []
        for i, p in enumerate(sorted((ROOT / "lez/lab/results/record/net/mainnet/updates").glob("*.ssz"))[:106], 1):
            kind = p.name.split(".")[1]
            expect.append([str(i), p.name, kind, *map(str, beacon_replay.slots(kind, p.read_bytes()))])
        self.assertEqual(len(expect), 106)
        stamp = lambda x: datetime.fromtimestamp(int(x), timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        shas = []
        for name, _ in self.RUNS:
            rows = [line.split("\t") for line in rd(f"bench/perf/results/{name}/reveals.tsv").splitlines()[1:]]
            self.assertEqual([r[:6] for r in rows], expect)
            last = 0.0
            for r in rows:
                t = float(r[7])
                self.assertIn(r[6], (stamp(t), stamp(t - 0.0005)), r)
                self.assertGreater(t, last, r)
                last = t
            sha = re.search(r"ferrata-updater sha256 ([0-9a-f]{64})", rd(f"bench/perf/results/{name}/machine.txt")).group(1)
            self.assertIn(f"updater sha256 {sha};", rd(f"bench/perf/results/{name}/hook.log"))
            shas.append(sha)
        self.has(f"G6's updater sha256 `{shas[0][:8]}…`, G7's `{shas[1][:8]}…`")

    def test_a3s_figures_from_the_journals(self):
        """The honest daemons' execution gas on refused steps over applied steps, honest-1's share of the fees (applied
        and refused), and no (job, step, label) applied twice: summed from the journals, not read from the generator."""
        stated = {}
        for (name, window) in self.RUNS:
            rows = self.journals[name]
            applied = sum(r["gas"] for r in rows if r["status"] == "applied")
            refused = sum(r["gas"] for r in rows if r["status"] == "refused")
            fees = {d: sum(r["fee"] for r in rows if r["daemon"] == d and r["status"] in ("applied", "refused"))
                    for d in ("honest-1", "honest-2")}
            ratio, share = refused / applied, fees["honest-1"] / sum(fees.values())
            self.assertLessEqual(ratio, 0.10)
            self.assertTrue(0.35 <= share <= 0.65)
            self.assertEqual((f"{ratio:.4f}", f"{share:.3f}"),
                             (f"{self.keys[name]['r9']:.4f}", f"{self.keys[name]['share']:.3f}"))
            done = [(r["job"], r["step"], r["label"]) for r in rows if r["status"] == "applied"]
            self.assertEqual(len(done), len(set(done)))
            stated[window] = (ratio, share)
        r8, s8 = stated[8]
        r30, s30 = stated[30]
        self.has(f"`gate-g6` (window 8) **{r8:.4f}** and honest-1's share **{s8:.3f}**; "
                 f"`gate-g7` (window 30) **{r30:.4f}** and **{s30:.3f}**. Both runs meet both limits, "
                 f"window 8 by {0.10 - r8:.4f}.")
        self.has("In both runs no step was applied twice")

    def test_every_refused_step_raced_an_applied_step_of_its_block(self):
        """Each refused step shares its block with the same step of the same job applied, or (exit 82, StoreChanged) is
        a `Finish` whose job another job's applied `Finish` overtook in that block; the header names those blocks."""
        store_changed = {}
        for name, _ in self.RUNS:
            rows = self.journals[name]
            applied = {(r["job"], r["step"], r["block"]) for r in rows if r["status"] == "applied"}
            finishes = {(r["job"], r["block"]) for r in rows if r["status"] == "applied" and r["step"] == "finish"}
            for r in (r for r in rows if r["status"] == "refused"):
                if (r["job"], r["step"], r["block"]) in applied:
                    continue
                self.assertEqual((r["step"], r["exit"]), ("finish", "82"), r)
                self.assertTrue(any(b == r["block"] and j != r["job"] for j, b in finishes), r)
                store_changed.setdefault(name, []).append(r["block"])
        self.assertEqual({k: sorted(v) for k, v in store_changed.items()}, {"gate-g6": [243, 267, 289]})
        self.has("`gate-g6`'s three refused `Finish` steps with exit code 82 (`StoreChanged`) show as \"0 of 3\"")
        self.has("another job (blocks 243, 267 and 289)")
        self.assertIn("| honest daemons | finish | 82 | 3 | 11,055,840 | 0 of 3 |", self.out)

    def test_the_headers_other_figures_are_the_generators(self):
        k6, k7 = self.keys["gate-g6"], self.keys["gate-g7"]
        for k in (k6, k7):
            self.assertEqual((k["lat_n"], k["fin_revealed"], k["twice"]), (96, 96, 0))
        self.has(f"all {k6['lat_n']} revealed finality updates were covered: {k6['own']} of them by their own job at "
                 f"window 8 (latency median {k6['lat_med']:.0f} s, p90 {k6['lat_p90']:.0f} s), {k7['own']} at window 30 "
                 f"(median {k7['lat_med']:.0f} s, p90 {k7['lat_p90']:.0f} s)")
        self.has(f"{g(k6['gas_aband'])} execution gas at window 8 and {g(k7['gas_aband'])} at window 30, so that all the "
                 f"daemons' gas is **{k6['mult']:.3f}** and **{k7['mult']:.3f}** times the completed jobs'")

    def test_the_operator_guides_rows_and_figures(self):
        """docs/guides/operator-guide.md §3.4 (R.10's documented effect of the window on latency): its G6 and G7 rows
        and the figures of the sentence comparing them, each from the generator's figures for the run."""
        guide = rd("docs/guides/operator-guide.md")
        folded = " ".join(guide.split())
        cells = {}
        for name, window in self.RUNS:
            k = self.keys[name]
            lo, hi = (f"{x:.1f}" for x in k["round_s"])
            cells[name] = dict(gas=f"{k['total_gas'] / k['revealed'] / 1e6:.1f}M", aband=f"{k['gas_aband'] / 1e6:.1f}M",
                               ref=f"{k['gas_ref'] / 1e6:.1f}M")
            row = (f"| {name[5:].upper()}: {k['revealed']} updates | {window} ({lo if lo == hi else f'{lo}–{hi}'} s) | "
                   f"{k['reveal_step']:.0f} s | {k['lat_med']:.0f} / {k['lat_p90']:.0f} s | {k['own']} / {k['later']} | "
                   f"{cells[name]['gas']} | {k['mult']:.3f} | {k['waste']:.3f} | {k['r9']:.4f} | {k['share']:.3f} |")
            self.assertEqual(guide.count(row), 1, row)
        k6, k7, c6, c7 = self.keys["gate-g6"], self.keys["gate-g7"], cells["gate-g6"], cells["gate-g7"]
        for needle in (f"the median latency grows from {k6['lat_med']:.0f} to {k7['lat_med']:.0f} s, the execution gas "
                       f"per revealed update falls from {c6['gas']} to {c7['gas']}",
                       f"(G6: {c6['aband']} abandoned against {c6['ref']} refused; G7: {c7['aband']} against {c7['ref']})",
                       "meet A3's two limits without its adversarial daemon"):
            self.assertEqual(folded.count(needle), 1, needle)


class TestGateJoin(unittest.TestCase):
    """`lez/results/perf-gate-sequencer-join.md` (RFP-021 R.9 and R.10 with join or wait, `policy.join_wait_rounds`):
    a hand-written header, then `bench/perf/summarize_gate.py`'s output on the four runs with join or wait, then
    `bench/perf/gate_abandoned.py`'s on G6, G7 (before it) and them, byte for byte. The runs are the ones the header
    describes (each `machine.txt`, `done.txt`, `reveals.tsv`, two daemons; one updater build for both runs of a pair,
    the two pairs' builds different, each named by the header with its commit); the reveals are the recorded updates;
    A3's two figures and limits, the steps applied twice and "every refused step in the block of the same step of the
    same job, applied" are recomputed from the journals; abandoned jobs, and for each whether the later `Open` was sent
    before any job open on its store was published, are recomputed here from the journals, the daemons' logs and the
    sequencer's log (none in the final build's runs); the policy's counters from each daemon's last `/metrics`; the
    operator guide's rows for the four runs."""

    RUNS = (("gate-g8", 8), ("gate-g9", 30), ("gate-g10", 8), ("gate-g11", 30))
    FINAL = ("gate-g10", "gate-g11")
    BEFORE = ("gate-g6", "gate-g7")
    DOC = "lez/results/perf-gate-sequencer-join.md"
    BUILDS = {"gate-g8": "b9377f5f", "gate-g10": "39a87ce4"}

    @classmethod
    def setUpClass(cls):
        dirs = [f"bench/perf/results/{name}" for name, _ in cls.RUNS]
        run = lambda script, ds: subprocess.run([sys.executable, "-I", script, *ds], cwd=ROOT, capture_output=True,
                                                encoding="utf-8", check=True).stdout
        cls.out = run("bench/perf/summarize_gate.py", dirs)
        cls.abandoned_out = run("bench/perf/gate_abandoned.py", [f"bench/perf/results/{n}" for n in cls.BEFORE] + dirs)
        cls.doc = rd(cls.DOC)
        tail = cls.out + cls.abandoned_out
        head = cls.doc[: len(cls.doc) - len(tail)] if cls.doc.endswith(tail) else ""
        cls.header = " ".join(head.split())
        sys.path.insert(0, str(ROOT / "bench" / "perf"))
        import contextlib
        import io
        import summarize_gate
        with contextlib.redirect_stdout(io.StringIO()):
            cls.keys = {name: summarize_gate.summarize(ROOT / d) for (name, _), d in zip(cls.RUNS, dirs)}
        cls.journals = {name: cls.journal(name) for name in cls.BEFORE + tuple(n for n, _ in cls.RUNS)}

    def has(self, needle, count=1):
        n = self.header.count(needle)
        self.assertEqual(n, count, f"{self.DOC}'s header states '{needle}' {n} time(s), not {count}")

    @staticmethod
    def journal(name):
        rows = []
        for d in ("honest-1", "honest-2"):
            for line in rd(f"bench/perf/results/{name}/state-{d}/journal.tsv").splitlines():
                f = line.split("\t")
                if len(f) >= 13:
                    rows.append(dict(daemon=d, step=f[1], job=f[2], slot=int(f[3]), status=f[4], exit=f[5],
                                     gas=int(f[6] or 0), fee=int(f[8] or 0), block=int(f[9]), t=unix(f[10]),
                                     label=f[11], hash=f[12].split(" ")[0]))
        return rows

    def sha(self, name):
        return re.search(r"ferrata-updater sha256 ([0-9a-f]{64})", rd(f"bench/perf/results/{name}/machine.txt")).group(1)

    def test_the_record_is_its_header_then_the_generators_output(self):
        self.assertTrue(self.out.startswith("## Run `gate-g8`"))
        self.assertTrue(self.abandoned_out.startswith("### Abandoned jobs of `gate-g6`"))
        self.assertTrue(self.doc.endswith(self.out + self.abandoned_out))
        self.assertTrue(self.header.startswith("# RFP-021 R.9 with join or wait"))
        self.assertNotIn("## Run", self.header)

    def test_the_runs_are_the_ones_described(self):
        for name, window in self.RUNS:
            run = f"bench/perf/results/{name}"
            machine = rd(f"{run}/machine.txt")
            self.assertIn(f"\nGATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW={window} GATE_POLL=4 GATE_ADVERSARY=0\n", machine)
            self.assertIn("\nFAKETIME=-5d ", machine)
            self.assertIn("FAKETIME_DONT_FAKE_MONOTONIC=1)", machine)
            self.assertEqual(rd(f"{run}/done.txt"), "done=1\n")
            kinds = [line.split("\t")[2] for line in rd(f"{run}/reveals.tsv").splitlines()[1:]]
            self.assertEqual((len(kinds), kinds.count("finality"), kinds.count("period")), (106, 96, 10))
            self.assertEqual(sorted(p.name for p in (ROOT / run).glob("state-*")), ["state-honest-1", "state-honest-2"])
            self.assertIn(f"updater sha256 {self.sha(name)};", rd(f"{run}/hook.log"))
            for d in ("honest-1", "honest-2"):
                self.assertIn("ferrata_updater_waits_for_other_jobs_total{", rd(f"{run}/metrics-{d}.txt"))
        self.assertEqual(self.sha("gate-g8"), self.sha("gate-g9"), "one build for the first pair")
        self.assertEqual(self.sha("gate-g10"), self.sha("gate-g11"), "one build for the second pair")
        self.assertNotEqual(self.sha("gate-g8"), self.sha("gate-g10"), "two builds")
        for first, commit in self.BUILDS.items():
            self.has(f"(commit `{commit}`, updater sha256 `{self.sha(first)[:8]}…`)")

    def test_the_reveals_are_the_recorded_updates(self):
        import beacon_replay
        expect = []
        for i, p in enumerate(sorted((ROOT / "lez/lab/results/record/net/mainnet/updates").glob("*.ssz"))[:106], 1):
            kind = p.name.split(".")[1]
            expect.append([str(i), p.name, kind, *map(str, beacon_replay.slots(kind, p.read_bytes()))])
        for name, _ in self.RUNS:
            rows = [line.split("\t") for line in rd(f"bench/perf/results/{name}/reveals.tsv").splitlines()[1:]]
            self.assertEqual([r[:6] for r in rows], expect, name)

    def test_a3s_figures_limits_and_steps_from_the_journals(self):
        for name, window in self.RUNS:
            rows = self.journals[name]
            applied = sum(r["gas"] for r in rows if r["status"] == "applied")
            refused = sum(r["gas"] for r in rows if r["status"] == "refused")
            fees = {d: sum(r["fee"] for r in rows if r["daemon"] == d and r["status"] in ("applied", "refused"))
                    for d in ("honest-1", "honest-2")}
            ratio, share = refused / applied, fees["honest-1"] / sum(fees.values())
            self.assertLessEqual(ratio, 0.10, name)
            self.assertTrue(0.35 <= share <= 0.65, f"{name}: {share:.3f}")
            self.assertEqual((f"{ratio:.4f}", f"{share:.3f}"),
                             (f"{self.keys[name]['r9']:.4f}", f"{self.keys[name]['share']:.3f}"))
            self.has(f"`{name}` (window {window}) **{ratio:.4f}** and honest-1's share **{share:.3f}**")
            done = [(r["job"], r["step"], r["label"]) for r in rows if r["status"] == "applied"]
            self.assertEqual(len(done), len(set(done)), f"{name}: a step applied twice")
            twins = {(r["job"], r["step"], r["block"]) for r in rows if r["status"] == "applied"}
            finishes = {(r["job"], r["block"]) for r in rows if r["status"] == "applied" and r["step"] == "finish"}
            for r in (r for r in rows if r["status"] == "refused"):
                if (r["job"], r["step"], r["block"]) in twins:
                    continue
                # A Finish refused because another job's Finish in its block moved the store first (exit 82).
                self.assertEqual((r["step"], r["exit"]), ("finish", "82"), f"{name}: a refused step without its twin: {r}")
                self.assertTrue(any(b == r["block"] and j != r["job"] for j, b in finishes), r)
        self.has("every refused step shared its block with the same step of the same job, applied")

    def test_abandoned_jobs_and_when_their_opens_were_sent(self):
        """Recomputed here: a job with an applied step that never completed, overtaken by the first job to complete at or
        after its first applied step. For each, the later `Open` of the pair, sent (its settlement less the delay its
        daemon logged) at least a second before any job open on its store was published, or in one block with it, is
        contention no read could avoid; any other is not. G6 and G7: 23 and 15 abandoned, none so. The runs with join or
        wait: one abandoned (G10), opened in the block of the job that overtook it."""
        import gzip
        avoidable, abandoned = {}, {}
        for name in self.BEFORE + tuple(n for n, _ in self.RUNS):
            run = ROOT / "bench/perf/results" / name
            rows = self.journals[name]
            delay = {}
            for d in ("honest-1", "honest-2"):
                for line in (run / f"daemon-{d}.log").read_text().splitlines():
                    m = re.match(r"\[updater\] .* \+\s*([\d.]+)s ([0-9a-f]{64})$", line)
                    if m:
                        delay[m.group(2)] = float(m.group(1))
            published = {}
            for line in gzip.decompress((run / "sequencer.log.gz").read_bytes()).decode("utf-8", "replace").splitlines():
                m = re.match(r"\[(\S+Z) INFO\s+sequencer_core\] Publishing block (\d+) ", line)
                if m:
                    published.setdefault(int(m.group(2)), unix(m.group(1)))
            last = lambda r: r["step"] == "finish" or r["label"].endswith("init adopt 2/2")
            applied = [r for r in rows if r["status"] == "applied"]
            done = {r["job"]: r["block"] for r in applied if last(r)}
            first = {}
            for r in applied:
                first[r["job"]] = min(first.get(r["job"], r["block"]), r["block"])
            opens = {r["job"]: r for r in applied if r["step"] == "open"}
            n = bad = 0
            for j, b in first.items():
                over = min(((ob, k) for k, ob in done.items() if ob >= b), default=None)
                if j in done or over is None:
                    continue
                n += 1
                pair = [opens.get(j), opens.get(over[1])]
                self.assertTrue(all(pair), f"{name}: an abandoned job's pair without an applied Open")
                late = max(pair, key=lambda r: (r["block"], r["t"]))
                sent = late["t"] - delay[late["hash"]]
                before = max((x for x in done.values() if x < late["block"]), default=-1)
                pubs = [published[r["block"]] for r in opens.values()
                        if before < r["block"] <= late["block"] and r is not late and r["block"] != late["block"]]
                if pubs and sent > min(pubs) - 1:
                    bad += 1
            abandoned[name], avoidable[name] = n, bad
        self.assertEqual({n: (abandoned[n], avoidable[n]) for n in self.BEFORE}, {"gate-g6": (23, 23), "gate-g7": (15, 15)})
        for name, _ in self.RUNS:
            self.assertEqual(abandoned[name], self.keys[name]["abandoned"], name)
            self.assertIn(f"### Abandoned jobs of `{name}`", self.abandoned_out)
        for name in self.FINAL:
            self.assertEqual(avoidable[name], 0, f"{name}: an Open sent once a job open on its store was published")
        self.assertEqual({n: abandoned[n] for n, _ in self.RUNS}, {"gate-g8": 0, "gate-g9": 0, "gate-g10": 1, "gate-g11": 0})
        self.has("In the four runs one job was abandoned, in `gate-g10`, opened in the block of the job that overtook it")

    def test_the_policys_counters(self):
        for name in self.FINAL:
            waits = capped = 0
            for d in ("honest-1", "honest-2"):
                m = rd(f"bench/perf/results/{name}/metrics-{d}.txt")
                waits += int(re.search(r"^ferrata_updater_waits_for_other_jobs_total\{[^}]*\} (\d+)$", m, re.M).group(1))
                capped += int(re.search(r"^ferrata_updater_join_waits_capped_total\{[^}]*\} (\d+)$", m, re.M).group(1))
            self.assertGreater(waits, 0, name)
            self.assertEqual(capped, 0, name)

    def test_the_operator_guides_rows(self):
        guide = rd("docs/guides/operator-guide.md")
        for name, window in self.RUNS:
            k = self.keys[name]
            lo, hi = (f"{x:.1f}" for x in k["round_s"])
            gas = f"{k['total_gas'] / k['revealed'] / 1e6:.1f}M"
            row = (f"| {name[5:].upper()}: {k['revealed']} updates, join or wait | {window} "
                   f"({lo if lo == hi else f'{lo}–{hi}'} s) | {k['reveal_step']:.0f} s | {k['lat_med']:.0f} / "
                   f"{k['lat_p90']:.0f} s | {k['own']} / {k['later']} | {gas} | {k['mult']:.3f} | {k['waste']:.3f} | "
                   f"{k['r9']:.4f} | {k['share']:.3f} |")
            self.assertEqual(guide.count(row), 1, row)


class TestGateR910(unittest.TestCase):
    """`lez/results/perf-gate-sequencer-r910.md` (RFP-021 R.9 and R.10 with the turns on the chain's height, ADR 0048):
    a hand-written header, then `bench/perf/summarize_gate.py`'s output on the five runs, then
    `bench/perf/gate_abandoned.py`'s, byte for byte. The runs are the ones the header describes, all on one build
    (each `machine.txt`, `done.txt`, `reveals.tsv`, the daemons' configurations): `gate-g22` honest daemons polling
    every 2 s and 6 s, `gate-g23` honest-2 served copies of every update (its signature slot moved by one), `gate-g24`
    A3's adversary beside the two honest daemons, `gate-g25` and `gate-g26` the default. A3's two figures and limits,
    the steps applied twice, every honest refused step's twin, the abandoned jobs, the turn clock (each daemon's last
    `/metrics`) and the updates learned from the chain are recomputed here from the raw files."""

    RUNS = ("gate-g22", "gate-g23", "gate-g24", "gate-g25", "gate-g26")
    DOC = "lez/results/perf-gate-sequencer-r910.md"
    BUILD = "860be342"
    SETTINGS = {
        "gate-g22": "GATE_ADVERSARY=0 GATE_POLLS=2,6 GATE_COPIES=0 GATE_ADV_POLL=2",
        "gate-g23": "GATE_ADVERSARY=0 GATE_POLLS=4,4 GATE_COPIES=1 GATE_ADV_POLL=2",
        "gate-g24": "GATE_ADVERSARY=1 GATE_POLLS=4,4 GATE_COPIES=0 GATE_ADV_POLL=2",
        "gate-g25": "GATE_ADVERSARY=0 GATE_POLLS=4,4 GATE_COPIES=0 GATE_ADV_POLL=2",
        "gate-g26": "GATE_ADVERSARY=0 GATE_POLLS=4,4 GATE_COPIES=0 GATE_ADV_POLL=2",
    }

    @classmethod
    def setUpClass(cls):
        dirs = [f"bench/perf/results/{name}" for name in cls.RUNS]
        run = lambda script, ds: subprocess.run([sys.executable, "-I", script, *ds], cwd=ROOT, capture_output=True,
                                                encoding="utf-8", check=True).stdout
        cls.out = run("bench/perf/summarize_gate.py", dirs)
        cls.abandoned_out = run("bench/perf/gate_abandoned.py", dirs)
        cls.doc = rd(cls.DOC)
        tail = cls.out + cls.abandoned_out
        head = cls.doc[: len(cls.doc) - len(tail)] if cls.doc.endswith(tail) else ""
        cls.header = " ".join(head.split())
        sys.path.insert(0, str(ROOT / "bench" / "perf"))
        import contextlib
        import io
        import summarize_gate
        with contextlib.redirect_stdout(io.StringIO()):
            cls.keys = {name: summarize_gate.summarize(ROOT / d) for name, d in zip(cls.RUNS, dirs)}
        cls.journals = {name: cls.journal(name) for name in cls.RUNS}

    def has(self, needle, count=1):
        n = self.header.count(needle)
        self.assertEqual(n, count, f"{self.DOC}'s header states '{needle}' {n} time(s), not {count}")

    @staticmethod
    def daemons(name):
        return sorted(p.name[len("state-"):] for p in (ROOT / "bench/perf/results" / name).glob("state-*"))

    @classmethod
    def journal(cls, name):
        rows = []
        for d in cls.daemons(name):
            for line in rd(f"bench/perf/results/{name}/state-{d}/journal.tsv").splitlines():
                f = line.split("\t")
                if len(f) >= 13:
                    rows.append(dict(daemon=d, step=f[1], job=f[2], slot=int(f[3]), status=f[4], exit=f[5],
                                     gas=int(f[6] or 0), fee=int(f[8] or 0), block=int(f[9]), t=unix(f[10]),
                                     label=f[11], hash=f[12].split(" ")[0]))
        return rows

    def sha(self, name):
        return re.search(r"ferrata-updater sha256 ([0-9a-f]{64})", rd(f"bench/perf/results/{name}/machine.txt")).group(1)

    def metric(self, name, daemon, key):
        m = rd(f"bench/perf/results/{name}/metrics-{daemon}.txt")
        return int(re.search(rf"^ferrata_updater_{key}\{{[^}}]*\}} (\d+)$", m, re.M).group(1))

    def test_the_record_is_its_header_then_the_generators_output(self):
        self.assertTrue(self.out.startswith("## Run `gate-g22`"))
        self.assertTrue(self.abandoned_out.startswith("### Abandoned jobs of `gate-g22`"))
        self.assertTrue(self.doc.endswith(self.out + self.abandoned_out))
        self.assertTrue(self.header.startswith("# RFP-021 R.9 and R.10 with the turns on the chain's height"))
        self.assertNotIn("## Run", self.header)

    def test_the_runs_are_the_ones_described(self):
        for name in self.RUNS:
            run = f"bench/perf/results/{name}"
            machine = rd(f"{run}/machine.txt")
            self.assertIn(f"\nGATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 {self.SETTINGS[name]}\n", machine)
            self.assertIn("\nFAKETIME=-5d ", machine)
            self.assertIn("FAKETIME_DONT_FAKE_MONOTONIC=1)", machine)
            self.assertEqual(rd(f"{run}/done.txt"), "done=1\n")
            kinds = [line.split("\t")[2] for line in rd(f"{run}/reveals.tsv").splitlines()[1:]]
            self.assertEqual((len(kinds), kinds.count("finality"), kinds.count("period")), (106, 96, 10))
            want = ["honest-1", "honest-2"] + (["adversary"] if name == "gate-g24" else [])
            self.assertEqual(self.daemons(name), sorted(want))
            self.assertIn(f"updater sha256 {self.sha(name)};", rd(f"{run}/hook.log"))
        shas = {self.sha(n) for n in self.RUNS}
        self.assertEqual(len(shas), 1, "one build for the five runs")
        self.has(f"(commit `{self.BUILD}`, updater sha256 `{shas.pop()[:8]}…`)")
        polls = {d: re.search(r"poll_secs = (\d+)", rd(f"bench/perf/results/gate-g22/updater-{d}.toml")).group(1)
                 for d in ("honest-1", "honest-2")}
        self.assertEqual(polls, {"honest-1": "2", "honest-2": "6"})
        adv = rd("bench/perf/results/gate-g24/updater-adversary.toml")
        for line in ("defer_window = 1", "join_wait_rounds = 0", "poll_secs = 2"):
            self.assertIn(f"\n{line}\n", adv)
        for d in ("honest-1", "honest-2"):
            self.assertNotIn("join_wait_rounds", rd(f"bench/perf/results/gate-g24/updater-{d}.toml"))

    def test_the_reveals_are_the_recorded_updates_and_the_copies(self):
        import beacon_replay
        expect, copies = [], []
        for i, p in enumerate(sorted((ROOT / "lez/lab/results/record/net/mainnet/updates").glob("*.ssz"))[:106], 1):
            kind = p.name.split(".")[1]
            expect.append([str(i), p.name, kind, *map(str, beacon_replay.slots(kind, p.read_bytes()))])
            copies.append(str(beacon_replay.copy_update(kind, p.read_bytes())[1]))
        for name in self.RUNS:
            rows = [line.split("\t") for line in rd(f"bench/perf/results/{name}/reveals.tsv").splitlines()[1:]]
            self.assertEqual([r[:6] for r in rows], expect, name)
            self.assertEqual([r[8] for r in rows], copies, name)
            self.assertTrue(all(r[8] != r[5] for r in rows), "every update has a copy")

    def test_a3s_figures_limits_and_steps_from_the_journals(self):
        for name in self.RUNS:
            rows = self.journals[name]
            honest = [r for r in rows if r["daemon"] != "adversary"]
            applied = sum(r["gas"] for r in honest if r["status"] == "applied")
            refused = sum(r["gas"] for r in honest if r["status"] == "refused")
            fees = {d: sum(r["fee"] for r in honest if r["daemon"] == d and r["status"] in ("applied", "refused"))
                    for d in ("honest-1", "honest-2")}
            ratio, share = refused / applied, fees["honest-1"] / sum(fees.values())
            self.assertEqual((f"{ratio:.4f}", f"{share:.3f}"),
                             (f"{self.keys[name]['r9']:.4f}", f"{self.keys[name]['share']:.3f}"), name)
            done = [(r["job"], r["step"], r["label"]) for r in rows if r["status"] == "applied"]
            self.assertEqual(len(done), len(set(done)), f"{name}: a step applied twice")
            twins = {(r["job"], r["step"], r["block"]) for r in rows if r["status"] == "applied"}
            finishes = {(r["job"], r["block"]) for r in rows if r["status"] == "applied" and r["step"] == "finish"}
            for r in (r for r in honest if r["status"] == "refused"):
                if (r["job"], r["step"], r["block"]) in twins:
                    continue
                self.assertEqual((r["step"], r["exit"]), ("finish", "82"), f"{name}: a refused step without its twin: {r}")
                self.assertTrue(any(b == r["block"] and j != r["job"] for j, b in finishes), r)

    def test_the_turns_counted_the_chains_height(self):
        for name in self.RUNS:
            for d in self.daemons(name):
                self.assertEqual(self.metric(name, d, "turn_clock_is_height"), 1, f"{name} {d}")
                self.assertGreater(self.metric(name, d, "turn_clock"), 100, f"{name} {d}")

    def test_the_copies_were_learned_from_the_chain(self):
        learned = {d: self.metric("gate-g23", d, "updates_from_chain_total") for d in ("honest-1", "honest-2")}
        self.assertGreater(learned["honest-1"], 0)
        self.assertGreater(learned["honest-2"], 0)
        for name in ("gate-g22", "gate-g24", "gate-g25", "gate-g26"):
            for d in ("honest-1", "honest-2"):
                self.assertEqual(self.metric(name, d, "updates_from_chain_total"), 0, f"{name} {d}: one replay, one copy")

    def test_the_headers_figures(self):
        """Each figure the header states, rebuilt here from the records (summarize_gate's keys, the journals, the last
        `/metrics`), with A3's two limits asserted on the journals' own figures."""
        what = {"gate-g22": "honest-1 polling every 2 s, honest-2 every 6 s", "gate-g23": "honest-2 served copies",
                "gate-g24": "with A3's adversary", "gate-g25": "the default", "gate-g26": "the default"}
        for name in self.RUNS:
            k = self.keys[name]
            if name == "gate-g24":
                # A3's adversary on a sequencer: the honest daemons' ratio is outside A3's limit, as the header states.
                self.assertGreater(k["r9"], 0.10, name)
            else:
                self.assertLessEqual(k["r9"], 0.10, name)
            self.assertTrue(0.35 <= k["share"] <= 0.65, f"{name}: {k['share']:.3f}")
            self.has(f"`{name}` ({what[name]}) **{k['r9']:.4f}** and honest-1's share **{k['share']:.3f}**")
            self.has(f"`{name}` {k['lat_med']:.0f} / {k['lat_p90']:.0f} s ({k['own'] + k['later']} of 96 covered)")
            self.assertEqual(k["own"] + k["later"], 96, name)
            self.has(f"`{name}` {k['mult']:.3f} ({k['abandoned']} job(s) abandoned)")
            opens = [sum(1 for r in self.journals[name] if r["daemon"] == d and r["step"] == "open" and r["status"] == "applied")
                     for d in ("honest-1", "honest-2")]
            self.has(f"`{name}` {opens[0]} / {opens[1]}")
        self.has(f"in `gate-g23` honest-1 learned {self.metric('gate-g23', 'honest-1', 'updates_from_chain_total')} updates "
                 f"from honest-2's `Open`s and honest-2 {self.metric('gate-g23', 'honest-2', 'updates_from_chain_total')} "
                 f"from honest-1's")

    def test_the_operator_guides_rows(self):
        guide = rd("docs/guides/operator-guide.md")
        what = {"gate-g22": "honest-1 every 2 s, honest-2 every 6 s", "gate-g23": "honest-2 served copies",
                "gate-g24": "with A3's adversary", "gate-g25": "the default", "gate-g26": "the default"}
        for name in self.RUNS:
            k = self.keys[name]
            gas = f"{k['total_gas'] / k['revealed'] / 1e6:.1f}M"
            row = (f"| {name[5:].upper()}: {what[name]} | {k['reveal_step']:.0f} s | {k['lat_med']:.0f} / "
                   f"{k['lat_p90']:.0f} s | {k['own']} / {k['later']} | {gas} | {k['mult']:.3f} | {k['waste']:.3f} | "
                   f"{k['r9']:.4f} | {k['share']:.3f} |")
            self.assertEqual(guide.count(row), 1, row)


def g(n):
    """A gas figure as the generator prints it (bench/perf/summarize_gate.py's `g`)."""
    return f"{int(round(n)):,}"


if __name__ == "__main__":
    unittest.main()
