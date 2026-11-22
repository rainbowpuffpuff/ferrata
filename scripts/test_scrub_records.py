#!/usr/bin/env python3
"""scripts/scrub_records.py and the stage that uses it (checkpoint/attestor/zone-bridge.sh records), without a run.

  A  the helper: every text file at any depth (logs/, net/, zone/) is scrubbed by the rules in order; a binary file
     and the file modes are left alone; nothing that names a home directory or an agent's folder is left;
  B  a path no rule maps (another home directory) refuses (exit 1) and the refusal names the file and line, not the
     path; so does one inside a .gz archive or a binary file, which are checked but never rewritten;
  C  --check-only rewrites nothing and refuses a dirty tree; --skip leaves a directory alone (a run's ignored work/);
  D  FERRATA_SCRUB_DENY (names kept outside the repository) refuses a text file, or an archive's text member, that
     holds one (binary files are not searched for these short names);
  E  zone-bridge.sh records on a run directory holding this machine's paths where the second review found one
     (net/proxy.log: pubnet.py's traceback) and where forge and the box scripts write them (logs/, logs/deploy/,
     zone/): exit 0 and none left anywhere in the run, read by this test, not by the tool; a path the stage cannot
     map makes it exit 1.

Run from anywhere; scratch goes to $TMPDIR (set it to a directory of yours). E writes e2e/runs/scrub-records-test/
and removes it.
"""
import gzip
import os
import re
import shutil
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(REPO, "scripts", "scrub_records.py")
STAGE = os.path.join(REPO, "checkpoint", "attestor", "zone-bridge.sh")
FAILED = []


def check(name, cond, detail=""):
    print(("ok - " if cond else "FAILED - ") + name + ("" if cond else f": {detail}"))
    if not cond:
        FAILED.append(name)


def write(path, text, mode=0o644):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(text if isinstance(text, bytes) else text.encode())
    os.chmod(path, mode)


def read(path):
    with open(path, "rb") as f:
        return f.read()


def tool(*args, env=None):
    return subprocess.run([sys.executable, "-I", TOOL, *args], capture_output=True, text=True,
                          env={**os.environ, **(env or {})})


def leftovers(root, needles):
    """Files under root (archives opened) holding any of the byte strings."""
    out = []
    for d, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(d, f)
            data = read(p)
            if p.endswith(".gz"):
                data += gzip.decompress(data)
            if any(n in data for n in needles):
                out.append(os.path.relpath(p, root))
    return out


def case_helper(tmp):
    home = "/home/alice"; spike = f"{home}/work/spike"; repo = f"{spike}/ferrata-wt-demo"
    t = f"{spike}/tmp/zone-bridge"; w = f"{t}/work/run-a"
    rules = ["--map", f"{w}/=work/", "--map", f"{t}/=tmp/", "--map", f"{repo}/=", "--scratch", f"{spike}/tmp=tmp/",
             "--map", f"{spike}/=spike/", "--map", f"{home}/=~/"]
    run = os.path.join(tmp, "run-a")
    write(f"{run}/net/proxy.log", f'  File "{repo}/scripts/pubnet.py", line 1323, in do_POST\n')
    write(f"{run}/logs/deploy/deploy-1.log", f"Transactions saved to: {repo}/vault/broadcast/Deploy.s.sol/1/run-latest.json\n")
    write(f"{run}/zone/attestor-0/attest.log", f"signed {w}/att-0/rounds/96.json\n", mode=0o640)
    write(f"{run}/logs/box-session.log", f"harness tables: 55 rows from {spike}/tmp/agent-7/rc3/tree/lez/results\n")
    write(f"{run}/logs/forge.log", f"compiler {home}/.svm/0.8.26/solc and {spike}/upstream/lez-v0.4.0-rc1/x\n")
    binary = b"\x00\x01receipt" + b"\xff" * 16
    write(f"{run}/checkpoint/receipts/receipt.bin", binary)
    r = tool(run, *rules)
    check("A: exit 0 when every path is mapped", r.returncode == 0, r.stdout + r.stderr)
    check("A: net/proxy.log (the review's file) scrubbed",
          read(f"{run}/net/proxy.log") == b'  File "scripts/pubnet.py", line 1323, in do_POST\n', read(f"{run}/net/proxy.log"))
    check("A: forge's path relative to the repository",
          read(f"{run}/logs/deploy/deploy-1.log") == b"Transactions saved to: vault/broadcast/Deploy.s.sol/1/run-latest.json\n")
    check("A: the stage's work directory, nested", read(f"{run}/zone/attestor-0/attest.log") == b"signed work/att-0/rounds/96.json\n")
    check("A: an agent's scratch folder becomes tmp/",
          read(f"{run}/logs/box-session.log") == b"harness tables: 55 rows from tmp/rc3/tree/lez/results\n")
    check("A: spike/ and the home directory last",
          read(f"{run}/logs/forge.log") == b"compiler ~/.svm/0.8.26/solc and spike/upstream/lez-v0.4.0-rc1/x\n")
    check("A: a binary file is left alone", read(f"{run}/checkpoint/receipts/receipt.bin") == binary)
    check("A: a file's mode is kept", os.stat(f"{run}/zone/attestor-0/attest.log").st_mode & 0o777 == 0o640)
    check("A: nothing left", leftovers(run, [b"/home/", b"ferrata-wt-", b"spike/tmp/"]) == [])
    check("A: no temporary file left", not any(f.startswith(".scrub-") for _, _, fs in os.walk(run) for f in fs))

    write(f"{run}/net/proxy.log", "  File \"/home/bob/elsewhere/pubnet.py\", line 9\n")
    r = tool(run, *rules)
    check("B: a path no rule maps refuses", r.returncode == 1, r.stdout + r.stderr)
    check("B: the refusal names the file and line", "net/proxy.log:1: a home directory" in r.stderr, r.stderr)
    check("B: the refusal does not repeat the path", "/home/bob" not in r.stderr + r.stdout, r.stderr)
    write(f"{run}/net/proxy.log", "clean\n")
    write(f"{run}/zone/logs/seq-0.log.gz", gzip.compress(f"started from {repo}/lez\n".encode()))
    before = read(f"{run}/zone/logs/seq-0.log.gz")
    r = tool(run, *rules)
    check("B: a path inside a .gz refuses", r.returncode == 1 and "seq-0.log:1:" in r.stderr, r.stderr)
    check("B: an archive is never rewritten", read(f"{run}/zone/logs/seq-0.log.gz") == before)
    os.unlink(f"{run}/zone/logs/seq-0.log.gz")
    dirty = b"\x00ELF" + f"{repo}/guest/src/main.rs".encode() + b"\x00"
    write(f"{run}/checkpoint/guest.bin", dirty)
    r = tool(run, *rules)
    check("B: a path inside a binary file refuses, the file untouched",
          r.returncode == 1 and "checkpoint/guest.bin:1:" in r.stderr and read(f"{run}/checkpoint/guest.bin") == dirty, r.stderr)
    os.unlink(f"{run}/checkpoint/guest.bin")

    write(f"{run}/logs/late.log", f"{repo}/vault/out\n")
    r = tool("--check-only", run, *rules)
    check("C: --check-only refuses a dirty tree", r.returncode == 1, r.stdout + r.stderr)
    check("C: --check-only rewrites nothing", read(f"{run}/logs/late.log") == f"{repo}/vault/out\n".encode())
    write(f"{run}/work/checkpoint/input.json", f'{{"from": "{repo}/x"}}')
    r = tool("--skip", "work", run, *rules)
    check("C: --skip leaves work/ alone, the rest scrubbed", r.returncode == 0
          and read(f"{run}/work/checkpoint/input.json") == f'{{"from": "{repo}/x"}}'.encode()
          and read(f"{run}/logs/late.log") == b"vault/out\n", r.stdout + r.stderr)
    shutil.rmtree(f"{run}/work")

    write(f"{run}/logs/box.log", "profile examplews-1 used\n")
    r = tool(run, *rules, env={"FERRATA_SCRUB_DENY": r"\bexamplews-\d\b"})
    check("D: FERRATA_SCRUB_DENY refuses its names", r.returncode == 1 and "logs/box.log:1: a name in FERRATA_SCRUB_DENY" in r.stderr,
          r.stderr)
    r = tool(run, *rules, env={"FERRATA_SCRUB_DENY": ""})
    check("D: without it the same file passes", r.returncode == 0, r.stdout + r.stderr)
    write(f"{run}/zone/logs/box.log.gz", gzip.compress(b"profile examplews-2 used\n"))
    write(f"{run}/checkpoint/receipt-2.bin", b"\x00\x01examplews-3\x00")
    r = tool(run, *rules, env={"FERRATA_SCRUB_DENY": r"\bexamplews-\d\b"})
    check("D: a name inside an archive's text refuses; inside a binary file it does not (short names: text only)",
          r.returncode == 1 and "zone/logs/box.log:1: a name in FERRATA_SCRUB_DENY" in r.stderr
          and "receipt-2.bin" not in r.stderr, r.stderr)


def case_stage():
    name = "scrub-records-test"
    run = os.path.join(REPO, "e2e", "runs", name)
    spike = os.path.dirname(REPO); home = os.environ["HOME"]
    t = os.environ.get("ZB_TMP") or f"{spike}/tmp/zone-bridge"; w = f"{t}/work/{name}"
    shutil.rmtree(run, ignore_errors=True)
    try:
        write(f"{run}/net/proxy.log", f'  File "{REPO}/scripts/pubnet.py", line 1323, in do_POST\n')
        write(f"{run}/logs/deploy/deploy-11155111-B-1.log", f"Transactions saved to: {REPO}/vault/broadcast/Crown.s.sol/11155111/x.json\n")
        write(f"{run}/logs/box-verify.log", f'{{"program":"{spike}/tmp/some-agent/rc3/programs-rc3/checkpoint_b_0_3_5.bin"}}\n')
        write(f"{run}/zone/attestor-0/attest.log", f"round 3: {w}/att-0/rounds/96.json (cargo {home}/.cargo/bin)\n")
        write(f"{run}/work/checkpoint/input.json", f'{{"ignored scratch": "{w}"}}')
        env = {k: v for k, v in os.environ.items() if k != "FERRATA_SCRUB_DENY"}
        r = subprocess.run(["bash", STAGE, "records", run], capture_output=True, text=True, env=env, cwd=REPO)
        check("E: zone-bridge.sh records exits 0", r.returncode == 0, r.stdout + r.stderr)
        needles = [p.encode() for p in (REPO, spike, home)] + [b"ferrata-wt-", b"spike/tmp/", b"/home/"]
        left = [f for f in leftovers(run, needles) if not f.startswith("work/")]
        check("E: no path of this machine or agent's folder left in the run (net/ included)", left == [], left)
        check("E: net/proxy.log reads as the fixed tool writes it",
              read(f"{run}/net/proxy.log") == b'  File "scripts/pubnet.py", line 1323, in do_POST\n', read(f"{run}/net/proxy.log"))
        check("E: the ignored work/ is not touched", read(f"{run}/work/checkpoint/input.json") == f'{{"ignored scratch": "{w}"}}'.encode())
        write(f"{run}/net/proxy.log", '  File "/home/someone-else/x/pubnet.py", line 1\n')
        r = subprocess.run(["bash", STAGE, "records", run], capture_output=True, text=True, env=env, cwd=REPO)
        check("E: a path the stage cannot map: exit 1", r.returncode == 1 and "net/proxy.log:1: a home directory" in r.stderr,
              r.stdout + r.stderr)
        check("E: and the stage says so in its log", re.search(r"STOP: the records name this machine",
                                                                read(f"{run}/logs/zone-bridge.log").decode()) is not None)
    finally:
        shutil.rmtree(run, ignore_errors=True)


def main():
    with tempfile.TemporaryDirectory(prefix="scrub-records-") as tmp:
        case_helper(tmp)
    case_stage()
    print(f"{'FAILED' if FAILED else 'passed'}: {len(FAILED)} failed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
