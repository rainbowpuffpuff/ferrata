"""Run some of the updater's tests with one of its fixes undone, to see them fail without it (red), then restore.

    python3 -I lez/updater/dev/mutants.py NAME [CARGO TEST ARGUMENTS...]
    python3 -I lez/updater/dev/mutants.py --check     # every mutant still applies to the sources (nothing written)

from the repository's root, with the environment the suite needs (`cd lez`-relative paths are handled here;
RISC0_DEV_MODE=1 and RISC0_HOME as for `cargo test -p ferrata-updater --features harness`). NAME is one of the keys
below; without arguments the tests that fix's commit added are run. Each mutant undoes one fix in the sources (the API
kept, so the tests compile), the tests run, and the sources are put back byte for byte, also when the run fails or is
stopped by Ctrl-C, SIGTERM or SIGHUP (cargo and the test processes it started are killed first: cargo runs in a session
of its own; SIGKILL cannot be caught). Exit status: cargo's (non-zero
expected: the tests fail without the fix); 128 + the signal's number when stopped. `dev/test_mutants.py` checks the
restoring.

The fixes (22 November 2026): m1 and m1near (an update signed in the future is
refused, of one finalized header the earliest-signed is taken; m1near also undoes m1roll's fix, which alone would pass
its tests; m1 too, so that it restores the old behaviour whole); m1roll (an update that can be
sent now ranks first, so one re-dated within the margin every round holds back only itself);
m4 (an `Open` waits its turn counted by the instance's state, not by its job, so a source serving a new copy each round
cannot postpone it); m6 (so does a step of a job this updater does not carry, so a source
opening each copy itself cannot either); m4reset and m7, their companions (after a step is sent on a state, the next of
its kind there waits a whole turn: m4reset, no new turn; m7, a fresh one); u2, L-U2 (a round
that cannot fetch the committees counts towards `down`); u3, L-U3 (a refusal whose re-reads fail is judged once the
chain reads again); u5, L-U5 (holds for good outlive HOLD_ROUNDS and restarts); u4f, L-U4f (an IPv6 client counts by
its /64); u6, L-U6 (the bound tests fail for a reader that takes an answer whole and then refuses it).
"""
import os
import signal
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
# The order's first element, an update that can be sent now, made the same for every candidate.
# After a step sent on a state: a whole turn for the next of its kind there.
SENT = ".insert((state_digest(&action.store_seen), action.step), (self.round + 1, Some(whole)));"
OPEN = ("                now.is_none_or(|t| config.slot_start_ms(p.update.signature_slot) <= t),\n",
        "                now.map(|_| true).unwrap_or(true),\n")
MUTANTS = {
    # No refusal of candidates signed in the future, and the key of 0ddb5168 (the most signers before the earliest
    # signature slot).
    "m1": ([(U, *FUTURE), (U, *KEY), (U, *OPEN)], ["--test", "refusals", "a_re_dated"]),
    # The same, with only the more-signers cases re-dated within FUTURE_MARGIN_MS (two and five slots past the clock).
    "m1near": ([(U, *FUTURE), (U, *KEY), (U, *OPEN),
                (T, "    for (case, to, future, payer) in cases {",
                 "    for (case, to, future, payer) in cases.into_iter().filter(|c| !c.2) {")],
               ["--test", "refusals", "a_re_dated_update_with_more_signers"]),
    # The order of 81bbea61: an update whose window has not opened can rank first (an internal review's M1).
    "m1roll": ([(U, *OPEN)], ["--test", "refusals", "within_the_margin_every_round"]),
    # An `Open`'s turn counted by its job, as before an internal review's M1: a new copy each round is a new job.
    "m4": ([(U, "            Step::Open => state_turn(state),\n", "            Step::Open => chosen.jid,\n")],
           ["--test", "redating", "started_slots_an_adopting"]),
    # A step of a job this updater does not carry counted by the job, as before a later review's M1: a new copy the
    # source opened itself each round restarts the count.
    "m6": ([(U, "            _ => state_turn(state),\n", "            _ => chosen.jid,\n")],
           ["--test", "redating", "opener_"]),
    # No new turn after a step sent on a state: once the first turn has passed, an `Open` each round.
    "m4reset": ([(U, SENT, ".retain(|_, _| true);\n"
                           "                    let _ = (whole, state_digest(&action.store_seen));")],
                ["--test", "redating", "finish_own_jobs_off"]),
    # A fresh turn after a step sent on a state, its wait drawn from each round's job, as before a later review's L2.
    "m7": ([(U, SENT, ".remove(&(state_digest(&action.store_seen), action.step));\n"
                      "                    let _ = whole;")],
           ["--test", "redating", "newest_only"]),
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


def cargo_test(args: list) -> list:
    """The command a mutant runs (from `lez/`)."""
    return ["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked", *args, "--", "--nocapture",
            "--test-threads", "4"]


def stopped(signum, _frame):
    """SIGTERM and SIGHUP end the run as Ctrl-C does: by an exception, so that `finally` puts the sources back (their
    default action ends the process at once, the sources left mutated, as an internal review found). Further signals are
    ignored from here, so that none stops the restoring half way."""
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, signal.SIG_IGN)
    raise SystemExit(128 + signum)


def main() -> int:
    if sys.argv[1:] == ["--check"]:
        return check()
    if len(sys.argv) < 2 or sys.argv[1] not in MUTANTS:
        print(__doc__ + "\nNAME: " + ", ".join(MUTANTS), file=sys.stderr)
        return 2
    edits, default = MUTANTS[sys.argv[1]]
    args = sys.argv[2:] or default
    originals = {}
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, stopped)
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
        cmd = cargo_test(args)
        print(f"mutant {sys.argv[1]}: {' '.join(cmd)}", flush=True)
        # cargo in a session (and process group) of its own: on an exception (Ctrl-C, or a signal above) the group is
        # killed, the test processes cargo started with it, before the sources are put back (killing cargo alone left
        # a test binary running on with the mutated build, as an internal review found).
        proc = subprocess.Popen(cmd, cwd=os.path.join(ROOT, "lez"), start_new_session=True)
        try:
            return proc.wait()
        except BaseException:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait()
            raise
    except KeyboardInterrupt:
        return 128 + signal.SIGINT
    finally:
        # A second signal must not stop the restoring half way.
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, signal.SIG_IGN)
        for p, text in originals.items():
            with open(p, "w", encoding="utf-8") as f:
                f.write(text)


if __name__ == "__main__":
    sys.exit(main())
