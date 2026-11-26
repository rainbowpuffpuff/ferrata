"""The checks of scripts/verify.py: what each re-checks, against what, for which requirement rows and EVIDENCE rows,
and how they run. Python standard library only.

A check is either a Python function `fn(ctx)` that returns its result lines and raises `CheckFailed` (or `Skipped`),
or a command run from the repository root that passes when it exits 0. Every check reads the repository through
`ctx.root`, so the tamper tests (scripts/test_verify.py) can run it on a copy with one piece of evidence altered.
"""
from __future__ import annotations

import dataclasses
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[2]

FAMILIES = {
    "record": "numbers re-derived from the committed records of a run",
    "hash": "hashes and image ids re-computed and compared",
    "seal": "Groth16 seals verified locally against their image ids and journals",
    "signature": "signatures verified locally against the published key",
    "docs": "the documents' own consistency (links, decisions, the row-to-test map)",
    "unit": "a test script of the repository's own tools (Python or Node, no toolchain)",
    "test": "a named test of the requirement map, run with its toolchain (cargo, forge, RISC Zero, LEZ)",
    "run-record": "the light client's unattended run on the public test network, from chain data",
    "sepolia": "Sepolia, read-only: transactions, logs, contract code and storage",
    "lez-network": "our public LEZ v0.4.0-rc3 test network, read-only: blocks, transactions, accounts",
    "site": "the live site's manifest and files against the repository",
    "beacon": "public beacon APIs, read-only",
}
TIERS = ("quick", "tests")
TOOLS = {"python3": "python3", "node": "node", "cargo": "cargo", "forge": "forge", "git": "git", "bash": "bash",
         "r0vm": "r0vm", "docker": "docker", "cast": "cast", "anvil": "anvil"}


class CheckFailed(Exception):
    """The evidence does not hold: the message says what differs."""


class Skipped(Exception):
    """The check cannot run here (a tool or the network missing); the message says what it needs."""


@dataclasses.dataclass
class Check:
    id: str
    title: str
    family: str
    mode: str  # "offline" | "online"
    rows: tuple = ()  # "RFP-021 F.4"
    evidence: tuple = ()  # EVIDENCE.md row numbers
    what: str = ""  # what it re-checks
    against: str = ""  # against what
    needs: tuple = ("python3",)
    tier: str = "quick"  # "quick": seconds to a few minutes, no toolchain; "tests": a toolchain, minutes to hours
    fn: Callable | None = None
    argv: tuple | None = None  # a command (argv), run in root/cwd
    shell: str | None = None  # or a shell command line (bash -c), run in root/cwd
    cwd: str = "."
    env: dict | None = None
    timeout: int = 1800
    inputs: tuple = ()  # paths (relative to the root) it reads: documentation and the tamper tests
    source: str = ""  # where the check comes from (the map entry for generated ones)

    def __post_init__(self):
        if self.family not in FAMILIES:
            raise ValueError(f"{self.id}: family {self.family!r}")
        if self.mode not in ("offline", "online"):
            raise ValueError(f"{self.id}: mode {self.mode!r}")
        if self.tier not in TIERS:
            raise ValueError(f"{self.id}: tier {self.tier!r}")
        if sum(x is not None for x in (self.fn, self.argv, self.shell)) != 1:
            raise ValueError(f"{self.id}: exactly one of fn, argv, shell")
        if self.mode == "online" and "network" not in self.needs:
            self.needs = tuple(self.needs) + ("network",)


REGISTRY: dict[str, Check] = {}


def register(c: Check) -> Check:
    if c.id in REGISTRY:
        raise ValueError(f"check {c.id} registered twice")
    REGISTRY[c.id] = c
    return c


def check(**kw):
    """Decorator: a Python check. `fn(ctx)` returns a string or a list of strings (its result lines)."""
    def deco(fn):
        register(Check(fn=fn, **kw))
        return fn
    return deco


def command(**kw) -> Check:
    return register(Check(**kw))


@dataclasses.dataclass
class Ctx:
    root: Path = ROOT
    sepolia_rpc: list = dataclasses.field(default_factory=list)
    mainnet_rpc: list = dataclasses.field(default_factory=list)
    lez_url: str = ""  # the test network's site (its /rpc, /programs.json, the manifest)
    beacon: list = dataclasses.field(default_factory=list)
    verbose: bool = False
    cache: Path | None = None
    git_dir: Path | None = None  # where git objects are read (the tamper tests' copies have none of their own)
    out: Callable = print

    def path(self, rel: str) -> Path:
        return self.root / rel


@dataclasses.dataclass
class Result:
    check: Check
    status: str  # PASS | FAIL | SKIP
    lines: list
    seconds: float


def missing_tools(c: Check) -> list:
    out = []
    for t in c.needs:
        if t == "network":
            continue
        exe = TOOLS.get(t)
        if exe and shutil.which(exe) is None:
            out.append(t)
    return out


def _tail(text: str, n: int) -> list:
    lines = [ln for ln in text.splitlines() if ln.strip()]
    return lines[-n:]


def run_check(c: Check, ctx: Ctx) -> Result:
    t0 = time.monotonic()
    miss = missing_tools(c)
    if miss:
        cmd = c.shell or (" ".join(c.argv) if c.argv else "")
        how = f"; run it yourself: (cd {c.cwd} && {cmd})" if cmd else ""
        return Result(c, "SKIP", [f"needs {', '.join(miss)} (not on PATH){how}"], 0.0)
    try:
        if c.fn is not None:
            got = c.fn(ctx)
            lines = [got] if isinstance(got, str) else list(got or [])
            return Result(c, "PASS", lines, time.monotonic() - t0)
        env = dict(os.environ)
        env.update(c.env or {})
        env["FERRATA_VERIFY"] = "1"
        argv = list(c.argv) if c.argv else ["bash", "-c", c.shell]
        if argv[0] == "python3":
            argv[0] = sys.executable
        proc = subprocess.run(argv, cwd=ctx.root / c.cwd, env=env, capture_output=True, text=True,
                              timeout=c.timeout)
        out = (proc.stdout or "") + ("\n" + proc.stderr if proc.stderr else "")
        if proc.returncode == 0:
            return Result(c, "PASS", _tail(proc.stdout or proc.stderr or "", 40 if ctx.verbose else 3),
                          time.monotonic() - t0)
        return Result(c, "FAIL", [f"exit status {proc.returncode}"] + _tail(out, 40 if ctx.verbose else 15),
                      time.monotonic() - t0)
    except Skipped as e:
        return Result(c, "SKIP", [str(e)], time.monotonic() - t0)
    except CheckFailed as e:
        return Result(c, "FAIL", str(e).splitlines() or ["failed"], time.monotonic() - t0)
    except subprocess.TimeoutExpired:
        return Result(c, "FAIL", [f"no result within {c.timeout} s"], time.monotonic() - t0)
    except Exception as e:  # noqa: BLE001 - a crash of a check is a failure of it, reported with its cause
        return Result(c, "FAIL", [f"{type(e).__name__}: {e}"], time.monotonic() - t0)


def run_script(ctx: "Ctx", argv: list, timeout: int = 1800, env: dict | None = None):
    """(exit status, non-empty output lines) of a repository script run from ctx.root (python3 -> this Python)."""
    e = dict(os.environ)
    e.update(env or {})
    argv = [sys.executable if a == "python3" else a for a in argv]
    p = subprocess.run(argv, cwd=ctx.root, capture_output=True, text=True, timeout=timeout, env=e)
    return p.returncode, [ln for ln in (p.stdout + "\n" + p.stderr).splitlines() if ln.strip()]


def fail_if(problems: list, what: str) -> None:
    """Raise CheckFailed listing `problems` (at most 20 shown) if there are any."""
    if problems:
        shown = problems[:20]
        more = [f"... and {len(problems) - 20} more"] if len(problems) > 20 else []
        raise CheckFailed("\n".join([f"{what}: {len(problems)} problem(s)"] + shown + more))
