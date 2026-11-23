"""Tests of dev/mutants.py: `python3 -I lez/updater/dev/test_mutants.py`.

A mutant's run stopped by SIGTERM, SIGHUP or SIGINT (Ctrl-C) puts the sources back byte for byte and kills the command
it ran and the process that command started, as cargo starts a test binary (internal reviews: SIGTERM used to end
the process before the `finally` that restores them, the sources left mutated; then the test binary
cargo started ran on). No cargo: the run's command is replaced by a sleeping Python process that starts another, and
the mutant edits a copy of the one source it touches, in a temporary directory (TMPDIR). Standard library only.
"""

import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = "lez/updater/src/serve.rs"  # the one file mutant u4f edits

# Runs mutants.main() on the tree argv[2], its command a sleeper that starts another sleeper (as cargo starts a test
# binary) and writes both pids to argv[4] first.
CHILD = r"""
import sys
sys.path.insert(0, sys.argv[1])
import mutants
mutants.ROOT = sys.argv[2]
pidfile = sys.argv[4]
mutants.cargo_test = lambda args: [sys.executable, "-I", "-c",
    "import os, subprocess, sys, time; "
    "g = subprocess.Popen([sys.executable, '-I', '-c', 'import time; time.sleep(60)']); "
    "open(sys.argv[1], 'w').write(f'{os.getpid()} {g.pid}'); time.sleep(60)", pidfile]
sys.argv = ["mutants.py", sys.argv[3]]
sys.exit(mutants.main())
"""


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    # A zombie (exited, not reaped) counts as gone.
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8") as f:
            return f.read().rsplit(")", 1)[1].split()[0] != "Z"
    except OSError:
        return False


def wait_for(cond, seconds: float) -> bool:
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if cond():
            return True
        time.sleep(0.05)
    return cond()


class StoppedRun(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        src = os.path.join(HERE, "..", "..", "..", SOURCE)
        self.path = os.path.join(self.root, SOURCE)
        os.makedirs(os.path.dirname(self.path))
        shutil.copyfile(src, self.path)
        with open(self.path, "rb") as f:
            self.original = f.read()
        self.pidfile = os.path.join(self.root, "sleeper.pid")

    def tearDown(self):
        self.tmp.cleanup()

    def stop_with(self, sig: int) -> None:
        # Its output to a file, not a pipe: a command left running would hold a pipe open.
        log = os.path.join(self.root, "run.log")
        with open(log, "wb") as out:
            run = subprocess.Popen([sys.executable, "-I", "-c", CHILD, HERE, self.root, "u4f", self.pidfile],
                                   stdout=out, stderr=subprocess.STDOUT)
        try:
            self.assertTrue(wait_for(lambda: os.path.exists(self.pidfile) and os.path.getsize(self.pidfile) > 0, 20),
                            "the command never started")
            with open(self.path, "rb") as f:
                self.assertNotEqual(f.read(), self.original, "the mutant is applied while its command runs")
            with open(self.pidfile, encoding="utf-8") as f:
                sleeper, started = (int(x) for x in f.read().split())
            run.send_signal(sig)
            code = run.wait(timeout=20)
        finally:
            if run.poll() is None:
                run.kill()
                run.wait()
        with open(log, encoding="utf-8", errors="replace") as f:
            err = f.read()
        with open(self.path, "rb") as f:
            self.assertEqual(f.read(), self.original, f"{signal.Signals(sig).name}: the source not put back; {err}")
        self.assertEqual(code, 128 + sig, err)
        self.assertTrue(wait_for(lambda: not alive(sleeper), 5), "the command it ran is still running")
        self.assertTrue(wait_for(lambda: not alive(started), 5), "the process the command started is still running")

    def test_sigterm_restores_the_sources(self):
        self.stop_with(signal.SIGTERM)

    def test_sighup_restores_the_sources(self):
        self.stop_with(signal.SIGHUP)

    def test_sigint_restores_the_sources(self):
        self.stop_with(signal.SIGINT)


if __name__ == "__main__":
    unittest.main()
