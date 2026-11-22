"""Run some of the updater's tests with one of its fixes undone, to see them fail without it (red), then restore.

    python3 -I lez/updater/dev/mutants.py NAME [CARGO TEST ARGUMENTS...]
    python3 -I lez/updater/dev/mutants.py --check     # every mutant still applies to the sources (nothing written)

from the repository's root, with the environment the suite needs (`cd lez`-relative paths are handled here;
RISC0_DEV_MODE=1 and RISC0_HOME as for `cargo test -p ferrata-updater --features harness`). NAME is one of the keys
below; without arguments the tests that fix's commit added are run. Each mutant undoes one fix in the sources (the API
kept, so the tests compile), the tests run, and the sources are put back byte for byte, also when the run fails or is
interrupted. Exit status: cargo's (non-zero expected: the tests fail without the fix).

The fixes (22 November 2026): m1 and m1near, review-integ9d M1 / review-integ9c M4 (an update signed in the future is
refused, of one finalized header the earliest-signed is taken); u2, L-U2 (a round that cannot fetch the committees
counts towards `down`); u3, L-U3 (a refusal whose re-reads fail is judged once the chain reads again); u5, L-U5 (holds
for good outlive HOLD_ROUNDS and restarts); u4f, L-U4f (an IPv6 client counts by its /64); u6, L-U6 (the bound tests
fail for a reader that takes an answer whole and then refuses it).
"""
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
U = "lez/updater/src/updater.rs"
S = "lez/updater/src/serve.rs"
Q = "lez/updater/src/source.rs"
T = "lez/updater/tests/refusals.rs"
FUTURE = ("let Some(now) = now_ms else {", "let Some(now) = now_ms.filter(|_| false) else {")
KEY = ("                std::cmp::Reverse(p.update.signature_slot),\n"
       "                p.update.sync_aggregate.bits.iter().filter(|b| **b).count(),\n",
       "                p.update.sync_aggregate.bits.iter().filter(|b| **b).count(),\n"
       "                std::cmp::Reverse(p.update.signature_slot),\n")
MUTANTS = {
    # No refusal of candidates signed in the future, and the key of 0ddb5168 (the most signers before the earliest
    # signature slot).
    "m1": ([(U, *FUTURE), (U, *KEY)], ["--test", "refusals", "a_re_dated"]),
    # The same, with only the more-signers cases re-dated within FUTURE_MARGIN_MS (two and five slots past the clock).
    "m1near": ([(U, *FUTURE), (U, *KEY),
                (T, "    for (case, to, future, payer) in cases {",
                 "    for (case, to, future, payer) in cases.into_iter().filter(|c| !c.2) {")],
               ["--test", "refusals", "a_re_dated_update_with_more_signers"]),
    "u2": ([(U, "self.ensure_committees(&store)?;",
             "if let Err(e) = self.ensure_committees(&store) {\n            return Ok(Plan::Stuck(format!(\"{e:#}\")));\n        }")],
           ["--test", "health", "committees_no_source_serves"]),
    "u3": ([(U, "fn judge_unread(&mut self, ledger: &dyn Ledger) {",
             "fn judge_unread(&mut self, ledger: &dyn Ledger) {\n        if ledger.now_ms() != Some(u64::MAX) {\n            return;\n        }")],
           ["--test", "refusals", "a_refusal_with_the_chain_unreadable"]),
    "u5": ([(U, "self.held.retain(|_, h| h.retry_at.is_none() || h.round + HOLD_ROUNDS > round);",
             "self.held.retain(|_, h| h.round + HOLD_ROUNDS > round);"),
            (U, 'fs::read_to_string(dir.join("held.tsv"))', 'fs::read_to_string(dir.join("held.tsv.none"))')],
           ["--test", "refusals", "a_hold_for_good"]),
    "u4f": ([(S, ".filter(|ip| !ip.is_loopback()).map(host);", ".filter(|ip| !ip.is_loopback());")],
            ["--lib", "ipv6_clients"]),
    # A reader that takes the answer whole and then refuses it: the reader's bound and curl's both removed.
    "u6": ([(Q, ".take(max as u64 + 1).read_to_end(&mut body)", ".read_to_end(&mut body)"),
            (Q, '.args(["--max-time", &timeout_secs.max(1).to_string(), "--max-filesize", &max.to_string()])',
             '.args(["--max-time", &timeout_secs.max(1).to_string()])')],
           ["--test", "bounds", "huge_answers"]),
}


def check() -> int:
    bad = 0
    for name, (edits, _) in MUTANTS.items():
        texts = {}
        for path, old, new in edits:
            p = os.path.join(ROOT, path)
            t = texts.setdefault(p, open(p, encoding="utf-8").read())
            if t.count(old) != 1:
                bad += 1
                print(f"{name}: {old[:60]!r} found {t.count(old)} times in {path}")
            texts[p] = t.replace(old, new, 1)
    print(f"{len(MUTANTS) - bad if not bad else 'not all'} of {len(MUTANTS)} mutants apply")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--check"]:
        return check()
    if len(sys.argv) < 2 or sys.argv[1] not in MUTANTS:
        print(__doc__ + "\nNAME: " + ", ".join(MUTANTS), file=sys.stderr)
        return 2
    edits, default = MUTANTS[sys.argv[1]]
    args = sys.argv[2:] or default
    originals = {}
    try:
        for path, old, new in edits:
            p = os.path.join(ROOT, path)
            originals.setdefault(p, open(p, encoding="utf-8").read())
            current = open(p, encoding="utf-8").read()
            if current.count(old) != 1:
                print(f"{sys.argv[1]}: {old[:60]!r} found {current.count(old)} times in {path}", file=sys.stderr)
                return 2
            with open(p, "w", encoding="utf-8") as f:
                f.write(current.replace(old, new))
        cmd = ["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked", *args, "--",
               "--nocapture", "--test-threads", "4"]
        print(f"mutant {sys.argv[1]}: {' '.join(cmd)}", flush=True)
        return subprocess.run(cmd, cwd=os.path.join(ROOT, "lez")).returncode
    finally:
        for p, text in originals.items():
            with open(p, "w", encoding="utf-8") as f:
                f.write(text)


if __name__ == "__main__":
    sys.exit(main())
