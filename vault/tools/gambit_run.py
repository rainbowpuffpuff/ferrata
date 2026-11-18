#!/usr/bin/env python3
"""Run the vault's whole Foundry suite against every mutant Certora's Gambit wrote, in parallel.

Gambit (tag v1.0.7) writes the mutants of src/FerrataVault.sol and src/FerrataRegistry.sol with
`gambit mutate --json gambit.conf.json` (into notes/gambit_out/vault/ and notes/gambit_out/registry/, each with
gambit_results.json and mutants.log; a mutant is named <dir>-<Gambit's id>, e.g. vault-12). This
script gives each worker its own scratch copy of the project (notes/gambit-ws/w<k>: foundry.toml, src/, test/,
script/, lib/ symlinked) and, for each mutant:

1. writes the mutated file into the copy (the other source file stays original) and builds with the project's own
   settings (optimizer on, 200 runs, cancun), except that no CBOR metadata is appended (cbor_metadata = false,
   bytecode_hash = "none"), so the bytecode does not carry a hash of the source text;
2. "equivalent (TCE)": the creation and runtime bytecode of FerrataVault and FerrataRegistry are byte for byte those
   of the original (trivial compiler equivalence: the compiled vault is the same program, so no test can tell);
3. otherwise runs every suite except the invariant campaign with --fail-fast: a failure is "killed";
4. then the invariant campaign (test/invariant/): a failure is "killed" (by the invariants), a pass "survived".

A stage that runs past its limit is "timeout" (a mutant that makes a loop unbounded, for example); a mutant the
build rejects is "invalid" (gambit.conf.json sets skip_validate: Gambit 1.0.7's own validation kept none of the
vault's 1934 mutants, though they compile, so the build is the validation here).
The real src/ is never modified. Each worker first checks that the unmutated suite passes in its copy.

Usage (from vault/, with forge and Gambit's output present):
  python3 -I tools/gambit_run.py run GAMBIT_DIRS results.jsonl [--workers N] [--ids-file FILE]
  python3 -I tools/gambit_run.py summary GAMBIT_DIRS results.jsonl [more.jsonl ...] [--survivors FILE]
GAMBIT_DIRS is a comma-separated list of Gambit output directories (notes/gambit_out/vault,notes/gambit_out/registry).
`run` appends one JSON line per mutant and skips ids already in the file (so it resumes). `summary` reads Gambit's
gambit_results.json and the result files (a later file overrides an earlier one for the same id), prints the counts,
and writes the survivors with their diffs to --survivors.
Environment: GAMBIT_RUN_THREADS (forge --threads per process, default 2), GAMBIT_RUN_TIMEOUT_A (build and tests,
default 300 s), GAMBIT_RUN_TIMEOUT_B (invariant campaign, default 600 s).
"""

import collections
import json
import os
import queue
import re
import shutil
import signal
import subprocess
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCES = ("src/FerrataVault.sol", "src/FerrataRegistry.sol")
ARTIFACTS = ("out/FerrataVault.sol/FerrataVault.json", "out/FerrataRegistry.sol/FerrataRegistry.json")
THREADS = os.environ.get("GAMBIT_RUN_THREADS", "2")
TIMEOUT_A = int(os.environ.get("GAMBIT_RUN_TIMEOUT_A", "300"))
TIMEOUT_B = int(os.environ.get("GAMBIT_RUN_TIMEOUT_B", "600"))
ENV = dict(os.environ, FOUNDRY_DISABLE_NIGHTLY_WARNING="1")


def sh(cmd: list, timeout: int) -> tuple:
    """(exit code or None on timeout, output). Kills the whole process group (forge and its solc) on timeout."""
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=ENV,
                         start_new_session=True)
    try:
        out, _ = p.communicate(timeout=timeout)
        return p.returncode, out
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        out, _ = p.communicate()
        return None, out


def prepare(work: str) -> None:
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    with open(os.path.join(ROOT, "foundry.toml"), encoding="utf-8") as f:
        config = f.read()
    head = "[profile.default]\n"
    assert config.count(head) == 1 and "cbor_metadata" not in config and "bytecode_hash" not in config
    with open(os.path.join(work, "foundry.toml"), "w", encoding="utf-8") as f:
        f.write(config.replace(head, head + 'cbor_metadata = false\nbytecode_hash = "none"\n'))
    for d in ("src", "test", "script"):  # test/real imports script/TestToken.sol
        shutil.copytree(os.path.join(ROOT, d), os.path.join(work, d))
    os.symlink(os.path.join(ROOT, "lib"), os.path.join(work, "lib"))
    # test/PrivacySuite.t.sol reads ../target/privacy (relative to the copy) and skips without a scenario there.
    os.makedirs(os.path.join(os.path.dirname(work), "target", "privacy"), exist_ok=True)


def bytecodes(work: str) -> tuple:
    out = []
    for a in ARTIFACTS:
        with open(os.path.join(work, a), encoding="utf-8") as f:
            j = json.load(f)
        out += [j["bytecode"]["object"], j["deployedBytecode"]["object"]]
    return tuple(out)


def first_fail(output: str) -> str:
    fails = [line.strip() for line in output.splitlines() if line.startswith("[FAIL")]
    m = re.findall(r"^(?:Ran \d+ tests? for|Failing tests in) (\S+)", output, re.M)
    suite = m[-1] if m else ""
    return (suite + " " + fails[0] if fails else suite + " (no [FAIL] line)").strip()[:300]


def build(work: str) -> tuple:
    return sh(["forge", "build", "--root", work, "--threads", THREADS], TIMEOUT_A)


def suite_a(work: str) -> tuple:
    return sh(["forge", "test", "--root", work, "--threads", THREADS, "--fail-fast",
               "--no-match-path", "test/invariant/*"], TIMEOUT_A)


def suite_b(work: str) -> tuple:
    return sh(["forge", "test", "--root", work, "--threads", THREADS, "--match-path", "test/invariant/*"], TIMEOUT_B)


def target_of(m: dict) -> str:
    # name is "mutants/<id>/src/<File>.sol"
    parts = m["name"].split("/")
    return "/".join(parts[2:])


def load_mutants(gdirs: str) -> dict:
    """key -> Gambit's record, with the record's directory under "dir" and its key under "key"."""
    out = {}
    for d in gdirs.split(","):
        with open(os.path.join(d, "gambit_results.json"), encoding="utf-8") as f:
            for m in json.load(f):
                m["dir"], m["key"] = d, f"{os.path.basename(os.path.normpath(d))}-{m['id']}"
                out[m["key"]] = m
    return out


def run_one(work: str, m: dict, originals: dict, ref: tuple) -> dict:
    t0 = time.time()
    target = target_of(m)
    with open(os.path.join(m["dir"], m["name"]), encoding="utf-8") as f:
        mutated = f.read()
    for path, text in originals.items():
        with open(os.path.join(work, path), "w", encoding="utf-8") as f:
            f.write(mutated if path == target else text)
    r = {"id": m["key"], "file": target, "op": m.get("description", "")}
    code, out = build(work)
    if code is None:
        r.update(status="timeout", stage="build")
    elif code != 0:
        errs = [line for line in out.splitlines() if "Error" in line]
        r.update(status="invalid", stage="build", detail=(" | ".join(errs[:2]) or out[-300:])[:300])
    elif bytecodes(work) == ref:
        r.update(status="equivalent", stage="tce")
    else:
        code, out = suite_a(work)
        if code is None:
            r.update(status="timeout", stage="suite")
        elif code != 0:
            r.update(status="killed", stage="suite", detail=first_fail(out))
        else:
            code, out = suite_b(work)
            if code is None:
                r.update(status="timeout", stage="invariant")
            elif code != 0:
                r.update(status="killed", stage="invariant", detail=first_fail(out))
            else:
                r.update(status="survived", stage="invariant")
    r["seconds"] = round(time.time() - t0, 1)
    return r


def load_results(paths: list) -> dict:
    res = {}
    for p in paths:
        if os.path.exists(p):
            with open(p, encoding="utf-8") as f:
                for line in f:
                    if line.strip():
                        r = json.loads(line)
                        res[r["id"]] = r
    return res


def cmd_run(gdirs: str, results: str, workers: int, ids_file: str) -> int:
    mutants = list(load_mutants(gdirs).values())
    if ids_file:
        with open(ids_file, encoding="utf-8") as f:
            wanted = set(f.read().split())
        mutants = [m for m in mutants if m["key"] in wanted]
    done = load_results([results])
    todo = [m for m in mutants if m["key"] not in done]
    print(f"{len(mutants)} mutants selected, {len(todo)} to run, {workers} workers", flush=True)
    originals = {}
    for p in SOURCES:
        with open(os.path.join(ROOT, p), encoding="utf-8") as f:
            originals[p] = f.read()
    q = queue.Queue()
    for m in todo:
        q.put(m)
    lock = threading.Lock()
    failed = []

    def worker(k: int) -> None:
        work = os.path.join(ROOT, "notes", "gambit-ws", f"w{k:02d}")
        prepare(work)
        code, out = build(work)
        ok = code == 0
        if ok:
            ref = bytecodes(work)
            code_a, out_a = suite_a(work)
            code_b, out_b = suite_b(work) if code_a == 0 else (1, "")
            ok = code_a == 0 and code_b == 0
        if not ok:
            with lock:
                failed.append(k)
                print(f"worker {k}: the unmutated suite does not pass in {work}; worker stops", flush=True)
                print((out if code != 0 else out_a if code_a != 0 else out_b)[-2000:], flush=True)
            return
        while True:
            try:
                m = q.get_nowait()
            except queue.Empty:
                return
            r = run_one(work, m, originals, ref)
            with lock:
                with open(results, "a", encoding="utf-8") as f:
                    f.write(json.dumps(r) + "\n")
                print(f"{r['id']:13s} {r['status']:10s} {r['seconds']:7.1f}s {r['file']} {r['op']} "
                      f"{r.get('detail', '')[:120]}", flush=True)

    threads = [threading.Thread(target=worker, args=(k,)) for k in range(workers)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    if failed:
        print(f"{len(failed)} workers did not start: {failed}", flush=True)
        return 2
    return 0


def changed_line(diff: str) -> str:
    m = re.search(r"^@@ -(\d+)", diff, re.M)
    return m.group(1) if m else "?"


def sort_key(key: str) -> tuple:
    name, _, n = key.rpartition("-")
    return (name, int(n))


def cmd_summary(gdirs: str, result_files: list, survivors: str) -> int:
    mutants = load_mutants(gdirs)
    res = load_results(result_files)
    missing = sorted(set(mutants) - set(res))
    total = collections.Counter(r["status"] for r in res.values())
    print(f"mutants: {len(mutants)}; with a result: {len(res)}; without: {len(missing)}")
    print("overall: " + ", ".join(f"{k} {v}" for k, v in sorted(total.items())))
    by_file = collections.defaultdict(collections.Counter)
    by_op = collections.defaultdict(collections.Counter)
    for r in res.values():
        by_file[r["file"]][r["status"]] += 1
        by_op[r["op"]][r["status"]] += 1
    for name, table in (("file", by_file), ("operator", by_op)):
        print(f"by {name}:")
        for k in sorted(table):
            print(f"  {k}: " + ", ".join(f"{s} {n}" for s, n in sorted(table[k].items())))
    stages = collections.Counter((r["status"], r.get("stage")) for r in res.values())
    print("by stage: " + ", ".join(f"{s}/{st} {n}" for (s, st), n in sorted(stages.items(), key=str)))
    if survivors:
        with open(survivors, "w", encoding="utf-8") as f:
            for i in sorted(res, key=sort_key):
                r = res[i]
                if r["status"] in ("survived", "timeout", "invalid"):
                    m = mutants[i]
                    f.write(f"## {i} {r['status']} {r['file']}:{changed_line(m['diff'])} {r['op']}\n\n```diff\n")
                    f.write("\n".join(line for line in m["diff"].splitlines() if line[:1] in "+-@") + "\n```\n\n")
    return 0


def main() -> int:
    a = sys.argv[1:]
    if len(a) < 3 or a[0] not in ("run", "summary"):
        print(__doc__, file=sys.stderr)
        return 2

    def opt(name: str, default=None):
        if name in a:
            i = a.index(name)
            v = a[i + 1]
            del a[i:i + 2]
            return v
        return default

    if a[0] == "run":
        workers = int(opt("--workers", "24"))
        ids_file = opt("--ids-file")
        return cmd_run(a[1], a[2], workers, ids_file)
    survivors = opt("--survivors")
    return cmd_summary(a[1], a[2:], survivors)


if __name__ == "__main__":
    sys.exit(main())
