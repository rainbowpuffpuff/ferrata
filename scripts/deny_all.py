#!/usr/bin/env python3
"""cargo-deny over every Rust workspace of the repository, with the policy in deny.toml (review-code-ops M4).

    python3 -I scripts/deny_all.py [--root DIR] [--config deny.toml] [--out DIR] [--advisories {report,baseline,off}]
                                   [--baseline FILE] [--write-baseline FILE] [--require-all] [WORKSPACE ...]

Every directory with a Cargo.lock is a workspace (git's list of tracked lockfiles; a walk where there is no .git, as
on a build box; target/, vault/lib/ and fuzz corpora are not searched). WORKSPACE arguments (directories relative to
the root) limit the run to those.

For each workspace, one cargo-deny run (`--all-features --locked`, the root's deny.toml) checks bans, licenses,
sources and advisories:
- bans, licenses, sources: any error fails the run.
- the dependency graph must resolve with --locked; if it does not, the run fails, unless what is missing is a path
  outside the repository (a source tree beside it that the workspace builds against, such as ../upstream/lez-v0.3.1),
  or the staged sources of a Logos Core module (modules/*/rust-lib): such a workspace is SKIPPED, with what is
  missing printed, and --require-all turns a skip into a failure.
- advisories (RustSec vulnerabilities, unmaintained and unsound crates, and yanked versions): `report` (the default)
  lists them and does not fail; `baseline` fails on any one not in FILE (default scripts/deny-advisories-baseline.txt),
  so a new advisory or a newly yanked crate turns the run red while the listed ones wait for their fix; `off` skips
  the check.

--out DIR: one JSON log per workspace (cargo-deny's own lines, machine paths replaced by . and ~) and summary.tsv.
--write-baseline FILE: write every advisory and yanked crate found, with the workspaces it is in.
Exit status 0 when every workspace passed or was skipped for a missing outside path, 1 otherwise. Needs cargo-deny
0.20 on PATH. Standard library only.
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


def missing_outside(stderr, root):
    """The first path cargo could not read that lies outside the repository, if that is why metadata failed."""
    for p in re.findall(r"failed to (?:read|load source for dependency|get `[^`]*` as a dependency)[^`]*`([^`]+)`", stderr) + \
            re.findall(r"(/[^\s`'\"]+/Cargo\.toml)", stderr):
        p = os.path.normpath(p)
        if not os.path.exists(p) and not (p + os.sep).startswith(os.path.realpath(root) + os.sep) \
                and not (p + os.sep).startswith(root + os.sep):
            return p
    return None


def krate(fields):
    for g in fields.get("graphs") or []:
        k = g.get("Krate") or {}
        if k.get("name"):
            return f"{k['name']}@{k.get('version', '?')}"
    return "?"


def advisory_key(fields):
    """The baseline key of an advisories diagnostic: the advisory id, or 'yanked CRATE@VERSION'."""
    if fields.get("code") == "yanked":
        return f"yanked {krate(fields)}"
    adv = fields.get("advisory") or {}
    return adv.get("id") or f"{fields.get('code')} {krate(fields)}"


def run_one(root, ws, cfg, checks):
    """(status, details, advisories {key: crate}, raw log, whether cargo-deny ran to its summary)."""
    man = os.path.join(root, ws, "Cargo.toml")
    meta = subprocess.run(["cargo", "metadata", "--locked", "--all-features", "--format-version", "1",
                           "--manifest-path", man], capture_output=True, text=True, cwd=os.path.join(root, ws))
    if meta.returncode != 0:
        miss = missing_outside(meta.stderr, root)
        if miss:
            return "SKIPPED", f"needs {scrub(miss, root)} (outside the repository)", {}, meta.stderr, False
        if STAGED.match(ws) and re.search(r"failed to read `[^`]*Cargo\.toml`", meta.stderr):
            return "SKIPPED", "needs its staged sources (a Logos Core module: modules/README.md, scripts/stage.py, " \
                "logos-rust-sdk-src)", {}, meta.stderr, False
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
    keys = set()
    for line in open(path, encoding="utf-8"):
        line = line.split("#", 1)[0].strip()
        if line:
            keys.add(line)
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
    cfg = os.path.abspath(a.config) if a.config else os.path.join(root, "deny.toml")
    found = workspaces(root)
    todo = [os.path.normpath(w) for w in a.workspace] or found
    unknown = [w for w in todo if w not in found]
    if unknown:
        sys.exit(f"deny_all: not a workspace with a Cargo.lock: {', '.join(unknown)}")
    checks = ["bans", "licenses", "sources"] + ([] if a.advisories == "off" else ["advisories"])
    baseline = read_baseline(a.baseline) if a.advisories == "baseline" else set()
    if a.out:
        os.makedirs(a.out, exist_ok=True)
    rows, seen, bad = [], {}, 0
    for ws in todo:
        status, detail, advs, log, _ = run_one(root, ws, cfg, checks)
        new = sorted(k for k in advs if k not in baseline) if a.advisories == "baseline" else []
        if status == "PASS" and new:
            status, detail = "FAIL", "advisories not in the baseline: " + ", ".join(new)
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
            print(f"  {k:<40} {'' if k in baseline or a.advisories != 'baseline' else 'NEW '}"
                  f"in {len(seen[k])} workspace(s)")
        if a.advisories == "baseline":
            stale = sorted(baseline - set(seen))
            if stale:
                print(f"baseline entries no longer reported (remove them): {', '.join(stale)}")
    n = {s: sum(r[1] == s for r in rows) for s in ("PASS", "FAIL", "SKIPPED")}
    print(f"deny_all: {len(rows)} workspaces: {n['PASS']} passed, {n['FAIL']} failed, {n['SKIPPED']} skipped "
          f"(checks: {', '.join(checks)}; advisories: {a.advisories})")
    if a.out:
        with open(os.path.join(a.out, "summary.tsv"), "w") as f:
            f.write("workspace\tstatus\tadvisories\tdetail\n" + "".join("\t".join(map(str, r)) + "\n" for r in rows))
    if a.write_baseline:
        with open(a.write_baseline, "w") as f:
            f.write("# Advisories and yanked crates that scripts/deny_all.py --advisories baseline accepts; any other one\n"
                    "# fails the run. Written by `scripts/deny_all.py --write-baseline` on "
                    f"{datetime.date.today().isoformat()}.\n"
                    "# A listing here is not a triage: see docs/reviews/security/ for which ones are reachable.\n")
            for k in sorted(seen):
                f.write(f"{k:<40} # {len(seen[k])} workspace(s): {', '.join(sorted(seen[k]))[:300]}\n")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
