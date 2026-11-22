#!/usr/bin/env python3
"""cargo-deny over every Rust workspace of the repository, with the policy in deny.toml (review-code-ops M4).

    python3 -I scripts/deny_all.py [--root DIR] [--config deny.toml] [--out DIR] [--advisories {report,baseline,off}]
                                   [--baseline FILE] [--write-baseline FILE] [--require-all] [WORKSPACE ...]

Every directory with a Cargo.lock is a workspace (git's list of tracked lockfiles; a walk where there is no .git, as
on a build box; target/, vault/lib/ and fuzz corpora are not searched). WORKSPACE arguments (directories relative to
the root) limit the run to those. The policy is the deny.toml nearest to the workspace, from its directory up to the
root (gpu-prover/deny.toml for gpu-prover/prover, the root's for the others); --config uses one file for all.

For each workspace, one cargo-deny run (`--all-features --locked`) checks bans, licenses, sources and advisories:
- bans, licenses, sources: any error fails the run.
- the dependency graph must resolve with --locked; if it does not, the run FAILS (a git or registry source that
  cannot be fetched, a lockfile that does not match, a missing toolchain: all failures), with two exceptions, which
  are SKIPPED with what is missing printed (--require-all turns a skip into a failure):
  - cargo could not read a path dependency's Cargo.toml that does not exist and lies beside the repository, outside
    it and outside cargo's home (a source tree the workspace builds against, such as ../upstream/lez-v0.3.1), and
    no git or registry source failed to load;
  - a Logos Core module's library (modules/*/rust-lib) whose staged copies are absent: the missing Cargo.toml lies
    under the module's logos-rust-sdk-src/, rust-lib/staged/ or rust-lib/upstream/ (git-ignored; scripts/stage.py).
- advisories (RustSec vulnerabilities, unmaintained and unsound crates, and yanked versions): `report` (the default)
  lists them and does not fail; `baseline` fails on any one that FILE (default scripts/deny-advisories-baseline.txt)
  does not accept for that workspace (each line: an advisory id, `yanked:CRATE@VERSION` or `CODE:CRATE@VERSION`,
  then the workspaces it is accepted in), so a new advisory, a newly yanked crate, or a known advisory reaching
  another workspace turns the run red; `off` skips the check.

--out DIR: one JSON log per workspace (cargo-deny's own lines, machine paths replaced by . and ~) and summary.tsv.
--write-baseline FILE: write every advisory and yanked crate found, with the workspaces it is in.
Exit status 0 when every workspace passed or was skipped as above, 1 otherwise. Needs cargo-deny 0.20 on PATH.
Standard library only.
"""
import argparse
import datetime
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHECK_BITS = {"advisories": 1, "bans": 2, "licenses": 4, "sources": 8}
ADVISORY_CODES = {"vulnerability", "unmaintained", "unsound", "notice", "yanked"}
PRUNE = {"target", ".git", "node_modules"}
# The Logos Core modules' Rust libraries build against copies a staging step makes (git-ignored: rust-lib/staged/,
# rust-lib/upstream/, logos-rust-sdk-src); unstaged, their graph cannot resolve, and that is a skip, not a failure.
STAGED = re.compile(r"^modules/[^/]+/rust-lib$")


def workspaces(root):
    """The directories (relative to root) holding a Cargo.lock."""
    try:
        r = subprocess.run(["git", "-C", root, "ls-files", "--", "*Cargo.lock"], capture_output=True, text=True)
        locks = r.stdout.split() if r.returncode == 0 else []
    except OSError:
        locks = []
    if not locks:
        for dp, dns, fns in os.walk(root):
            rel = os.path.relpath(dp, root)
            dns[:] = sorted(d for d in dns if d not in PRUNE and os.path.join(rel, d) not in ("vault/lib", "./vault/lib")
                            and not (rel == "fuzz" and d.startswith("corpus")))
            if "Cargo.lock" in fns:
                locks.append(os.path.join(rel, "Cargo.lock"))
    return sorted({os.path.normpath(os.path.dirname(l)) for l in locks})


def scrub(text, root):
    return text.replace(root + "/", "./").replace(root, ".").replace(os.path.expanduser("~"), "~")


def under(path, top):
    """Whether path is top or inside it, as given or with links resolved."""
    for a, b in ((path, top), (os.path.realpath(path), os.path.realpath(top))):
        if a == b or a.startswith(b.rstrip(os.sep) + os.sep):
            return True
    return False


def cargo_home():
    return os.environ.get("CARGO_HOME") or os.path.join(os.path.expanduser("~"), ".cargo")


def skip_reason(stderr, root, ws):
    """Why a workspace whose graph does not resolve may be skipped, or None (then it fails). Only a missing path
    dependency beside the repository, or a module's absent staged copies, is a skip; anything to do with fetching a
    git or registry source, or with the lockfile, is not (review-fix-supply M-3)."""
    # Any URL in cargo's error chain means a git or registry source was involved; a lockfile cargo would have to
    # change, or offline mode, is never a missing tree.
    if re.search(r"(?:https?|ssh|git|sparse\+https?)://|--locked was passed|cannot update the lock file|"
                 r"needs to be updated|offline", stderr, re.I):
        return None
    parent = os.path.dirname(root.rstrip(os.sep))
    for p in re.findall(r"failed to read `(/[^`]+/Cargo\.toml)`", stderr):
        p = os.path.normpath(p)
        if os.path.exists(p):
            continue
        if STAGED.match(ws):
            mod = os.path.join(root, os.path.dirname(ws))
            if any(under(p, os.path.join(mod, d)) for d in ("logos-rust-sdk-src", "rust-lib/staged", "rust-lib/upstream")):
                return "needs its staged sources (a Logos Core module: modules/README.md, scripts/stage.py, " \
                    "logos-rust-sdk-src)"
        if not under(p, root) and under(p, parent) and not under(p, cargo_home()):
            return f"needs {scrub(p, root)} (beside the repository, absent)"
    return None


def config_for(root, ws, explicit):
    """The policy file: --config if given, else the deny.toml nearest to the workspace (its directory up to the root)."""
    if explicit:
        return explicit
    d = os.path.normpath(os.path.join(root, ws))
    while True:
        if os.path.isfile(os.path.join(d, "deny.toml")):
            return os.path.join(d, "deny.toml")
        if os.path.normpath(d) == os.path.normpath(root) or os.path.dirname(d) == d:
            return os.path.join(root, "deny.toml")
        d = os.path.dirname(d)


def krate(fields):
    for g in fields.get("graphs") or []:
        k = g.get("Krate") or {}
        if k.get("name"):
            return f"{k['name']}@{k.get('version', '?')}"
    return "?"


def advisory_key(fields):
    """The baseline key of a diagnostic: the advisory id, else yanked:CRATE@VERSION or CODE:CRATE@VERSION."""
    if fields.get("code") == "yanked":
        return f"yanked:{krate(fields)}"
    adv = fields.get("advisory") or {}
    return adv.get("id") or f"{fields.get('code')}:{krate(fields)}"


def run_one(root, ws, cfg, checks):
    """(status, details, advisories {key: crate}, raw log, whether cargo-deny ran to its summary)."""
    man = os.path.join(root, ws, "Cargo.toml")
    meta = subprocess.run(["cargo", "metadata", "--locked", "--all-features", "--format-version", "1",
                           "--manifest-path", man], capture_output=True, text=True, cwd=os.path.join(root, ws))
    if meta.returncode != 0:
        why = skip_reason(meta.stderr, root, ws)
        if why:
            return "SKIPPED", why, {}, meta.stderr, False
        last = [l for l in meta.stderr.strip().splitlines() if l.strip()][-3:]
        return "FAIL", "dependency graph does not resolve with --locked: " + scrub(" | ".join(last), root)[:400], {}, \
            meta.stderr, False
    cmd = ["cargo", "deny", "--manifest-path", man, "--all-features", "--locked", "--format", "json", "--color", "never",
           "--config", cfg, "check"] + checks
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=os.path.join(root, ws))
    errors, advisories, summary = {}, {}, None
    for line in r.stderr.splitlines():
        try:
            j = json.loads(line)
        except ValueError:
            continue
        f = j.get("fields") or {}
        if j.get("type") == "summary":
            summary = f
        elif j.get("type") == "diagnostic" and f.get("severity") == "error":
            if f.get("code") in ADVISORY_CODES:
                advisories[advisory_key(f)] = krate(f)
            else:
                errors.setdefault(f.get("code"), set()).add(krate(f))
    if summary is None:
        tail = [l for l in r.stderr.strip().splitlines() if l.strip()][-3:]
        return "FAIL", f"cargo-deny did not finish (exit {r.returncode}): " + scrub(" | ".join(tail), root)[:400], {}, \
            r.stderr, False
    failed = [c for c in ("bans", "licenses", "sources") if c in checks and r.returncode & CHECK_BITS[c]]
    if failed:
        what = "; ".join(f"{code}: {', '.join(sorted(ks))}" for code, ks in sorted(errors.items(), key=lambda x: str(x[0])))
        return "FAIL", f"{'+'.join(failed)} failed ({what})", advisories, r.stderr, True
    return "PASS", "", advisories, r.stderr, True


def read_baseline(path):
    """{key: workspaces it is accepted in} from the baseline file (a line: KEY WORKSPACE... [# note])."""
    keys = {}
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()
    for n, line in enumerate(lines, 1):
        words = line.split("#", 1)[0].split()
        if not words:
            continue
        if len(words) < 2:
            sys.exit(f"deny_all: {path}:{n}: {words[0]} names no workspace (each entry is accepted only in the "
                     "workspaces it lists; rewrite the file with --write-baseline)")
        keys.setdefault(words[0], set()).update(os.path.normpath(w) for w in words[1:])
    return keys


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--root", default=ROOT)
    ap.add_argument("--config", default="", help="the policy (default: deny.toml at the root)")
    ap.add_argument("--out", default="")
    ap.add_argument("--advisories", choices=("report", "baseline", "off"), default="report")
    ap.add_argument("--baseline", default=os.path.join(ROOT, "scripts", "deny-advisories-baseline.txt"))
    ap.add_argument("--write-baseline", default="")
    ap.add_argument("--require-all", action="store_true")
    ap.add_argument("workspace", nargs="*")
    a = ap.parse_args()
    root = os.path.abspath(a.root)
    explicit = os.path.abspath(a.config) if a.config else ""
    found = workspaces(root)
    todo = [os.path.normpath(w) for w in a.workspace] or found
    unknown = [w for w in todo if w not in found]
    if unknown:
        sys.exit(f"deny_all: not a workspace with a Cargo.lock: {', '.join(unknown)}")
    checks = ["bans", "licenses", "sources"] + ([] if a.advisories == "off" else ["advisories"])
    baseline = read_baseline(a.baseline) if a.advisories == "baseline" else {}
    if a.out:
        os.makedirs(a.out, exist_ok=True)
    rows, seen, bad = [], {}, 0
    for ws in todo:
        cfg = config_for(root, ws, explicit)
        status, detail, advs, log, _ = run_one(root, ws, cfg, checks)
        new = sorted(k for k in advs if ws not in baseline.get(k, ())) if a.advisories == "baseline" else []
        if status == "PASS" and new:
            status, detail = "FAIL", "advisories the baseline does not accept here: " + ", ".join(new)
        if cfg != os.path.join(root, "deny.toml"):
            detail = (detail + " " if detail else "") + f"(policy {os.path.relpath(cfg, root)})"
        if status == "SKIPPED" and a.require_all:
            status = "FAIL"
        bad += status == "FAIL"
        for k in advs:
            seen.setdefault(k, set()).add(ws)
        rows.append((ws, status, len(advs), detail))
        print(f"{status:<8}{ws:<52}{len(advs):>3} advisories  {detail}", flush=True)
        if a.out:
            with open(os.path.join(a.out, (ws.replace("/", "_") if ws != "." else "root") + ".json"), "w") as f:
                f.write(scrub(log, root))
    if a.advisories != "off":
        print(f"advisories and yanked crates found: {len(seen)}")
        for k in sorted(seen):
            extra = sorted(seen[k] - baseline.get(k, set())) if a.advisories == "baseline" else []
            print(f"  {k:<44} in {len(seen[k])} workspace(s)" + (f"; NOT accepted in: {', '.join(extra)}" if extra else ""))
        if a.advisories == "baseline":
            stale = sorted(f"{k} {w}" for k, ws_ in baseline.items() for w in ws_
                           if w in todo and w not in seen.get(k, set()))
            if stale:
                print(f"baseline entries no longer reported (remove them): {'; '.join(stale)}")
    n = {s: sum(r[1] == s for r in rows) for s in ("PASS", "FAIL", "SKIPPED")}
    print(f"deny_all: {len(rows)} workspaces: {n['PASS']} passed, {n['FAIL']} failed, {n['SKIPPED']} skipped "
          f"(checks: {', '.join(checks)}; advisories: {a.advisories})")
    if a.out:
        with open(os.path.join(a.out, "summary.tsv"), "w") as f:
            f.write("workspace\tstatus\tadvisories\tdetail\n" + "".join("\t".join(map(str, r)) + "\n" for r in rows))
    if a.write_baseline:
        with open(a.write_baseline, "w") as f:
            f.write("# Advisories and yanked crates that scripts/deny_all.py --advisories baseline accepts, each only in the\n"
                    "# workspaces its line names; any other one, or one of these in another workspace, fails the run.\n"
                    "# Written by `scripts/deny_all.py --write-baseline` on "
                    f"{datetime.date.today().isoformat()}.\n"
                    "# A listing here is not a triage: see docs/reviews/security/ for which ones are reachable.\n")
            for k in sorted(seen):
                f.write(f"{k} {' '.join(sorted(seen[k]))}\n")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
