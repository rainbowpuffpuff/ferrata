#!/usr/bin/env python3
"""Re-check Ferrata's evidence from a clone of the repository: every check, the requirement rows (RFP-021, RFP-022)
and the EVIDENCE rows it backs, what it re-checks and against what. Python 3.11+ standard library only.

    python3 -I scripts/verify.py --list                 every check: id, family, mode, rows, EVIDENCE rows
    python3 -I scripts/verify.py --coverage             every requirement row with its checks; the coverage numbers
    python3 -I scripts/verify.py --all                  every offline check of the quick tier (the default mode)
    python3 -I scripts/verify.py --all --online         every online check (read-only: Sepolia, our public LEZ
                                                        test network, the live site, public beacon APIs)
    python3 -I scripts/verify.py --all --both --tests   everything, the named tests with their toolchains included
    python3 -I scripts/verify.py RFP-021:F.4            the checks of one requirement row (also `021/F.4`, `F.4@021`)
    python3 -I scripts/verify.py evidence:151           the checks of one EVIDENCE row
    python3 -I scripts/verify.py family:seal            one family (--list shows them)
    python3 -I scripts/verify.py run-record --online    one check by its id

Modes: --offline (the default) reads only the repository (tests, vectors, image ids, recorded seals and receipts,
signatures, manifest hashes, the records' numbers); --online makes read-only requests to public endpoints and
never signs or sends anything; --both runs both. Tiers: the quick tier needs Python (and Node for some checks) and
runs in minutes; --tests adds the named tests of scripts/ci/row-tests.toml, which need their toolchains (cargo,
forge, RISC Zero, LEZ's sources) and can take hours: CI runs them in its jobs.

Each check prints PASS, FAIL or SKIP, what it re-checked and against what. Exit status: 1 if a check failed, 0
otherwise (a SKIP names what it needs; --strict makes it a failure), 2 on a usage error.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from verifylib import catalog  # noqa: E402
from verifylib.core import FAMILIES, REGISTRY, Ctx, run_check  # noqa: E402
from verifylib.rows import normalise_row  # noqa: E402


def _row_label(c) -> str:
    rows = ", ".join(c.rows) if c.rows else "-"
    ev = ", ".join(str(n) for n in c.evidence) if c.evidence else "-"
    return f"rows {rows} · EVIDENCE {ev}"


def select(args, rows) -> list:
    chosen = []
    for s in args.selectors:
        if s in REGISTRY:
            chosen.append(REGISTRY[s])
            continue
        if s.startswith("family:"):
            fam = s.split(":", 1)[1]
            if fam not in FAMILIES:
                raise SystemExit(f"verify.py: no family {fam!r} (families: {', '.join(FAMILIES)})")
            chosen.extend(c for c in REGISTRY.values() if c.family == fam)
            continue
        if s.lower().startswith(("evidence:", "e:")):
            n = int(s.split(":", 1)[1])
            chosen.extend(c for c in REGISTRY.values() if n in c.evidence)
            continue
        t = s
        if "@" in t:  # F.4@021
            a, b = t.split("@", 1)
            t = f"{b}:{a}"
        hits = normalise_row(t, rows)
        if len(hits) > 1:
            raise SystemExit(f"verify.py: {s} names {', '.join(h.key for h in hits)}: give the RFP (RFP-021:{s})")
        if hits:
            key = hits[0].key
            chosen.extend(c for c in REGISTRY.values() if key in c.rows)
            continue
        raise SystemExit(f"verify.py: {s!r} is not a check id, a requirement row, evidence:N or family:NAME "
                         "(--list shows the checks)")
    if args.all:
        chosen.extend(REGISTRY.values())
    out = []
    seen = set()
    for c in chosen:
        if c.id in seen:
            continue
        seen.add(c.id)
        out.append(c)
    return out


def mode_filter(args, checks, explicit_ids) -> tuple:
    """(to run, left out with the reason): the mode and tier flags apply to everything not named by its id."""
    modes = {"offline", "online"} if args.both else ({"online"} if args.online else {"offline"})
    run, left = [], []
    for c in checks:
        if c.id in explicit_ids:
            run.append(c)
        elif c.mode not in modes:
            left.append((c, f"{c.mode} (add --{c.mode if c.mode == 'online' else 'offline'} or --both)"))
        elif c.tier == "tests" and not args.tests:
            left.append((c, "a named test with its toolchain (add --tests)"))
        else:
            run.append(c)
    return run, left


def list_json(checks) -> list:
    return [{"id": c.id, "title": c.title, "family": c.family, "mode": c.mode, "tier": c.tier,
             "rows": list(c.rows), "evidence": list(c.evidence), "what": c.what, "against": c.against,
             "needs": list(c.needs), "command": c.shell or (" ".join(c.argv) if c.argv else None)} for c in checks]


def print_list(checks, as_json: bool) -> None:
    if as_json:
        print(json.dumps(list_json(checks), indent=1))
        return
    for c in checks:
        print(f"{c.id}  [{c.family}, {c.mode}, {c.tier}]  {c.title}")
        print(f"    {_row_label(c)}")
        if c.what:
            print(f"    re-checks: {c.what}")
        if c.against:
            print(f"    against:   {c.against}")
    print(f"\n{len(checks)} checks")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("selectors", nargs="*", help="check ids, requirement rows, evidence:N, family:NAME")
    ap.add_argument("--list", action="store_true", help="list the selected checks (all without a selector)")
    ap.add_argument("--coverage", action="store_true", help="every requirement row and its checks; the numbers")
    ap.add_argument("--all", action="store_true", help="every check of the selected mode and tier")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--offline", action="store_true", help="only checks that read the repository (the default)")
    g.add_argument("--online", action="store_true", help="only read-only checks against public endpoints")
    g.add_argument("--both", action="store_true", help="offline and online checks")
    ap.add_argument("--tests", action="store_true", help="include the named tests (toolchains; can take hours)")
    ap.add_argument("--strict", action="store_true", help="a SKIP is a failure")
    ap.add_argument("--json", metavar="FILE", help="write the results (or the list) as JSON")
    ap.add_argument("--sepolia-rpc", action="append", default=[], metavar="URL", help="a Sepolia JSON-RPC (first)")
    ap.add_argument("--mainnet-rpc", action="append", default=[], metavar="URL")
    ap.add_argument("--beacon", action="append", default=[], metavar="URL", help="a Sepolia beacon API (first)")
    ap.add_argument("--lez", metavar="URL", help="the test network's site (default: scripts/reproduce/"
                    "deployed-programs.json's network)")
    ap.add_argument("-v", "--verbose", action="store_true")
    args = ap.parse_args(argv)

    root = Path(__file__).resolve().parent.parent
    rows = catalog.load(root)
    if args.coverage:
        return catalog.print_coverage(root, rows, as_json=args.json)
    if args.list and not args.selectors and not args.all:
        args.all = True
    if not args.selectors and not args.all:
        ap.print_usage()
        print("give check ids, requirement rows, evidence:N or family:NAME, or --all; --list shows the checks")
        return 2
    try:
        checks = select(args, rows)
    except ValueError as e:
        print(f"verify.py: {e}", file=sys.stderr)
        return 2
    explicit = {s for s in args.selectors if s in REGISTRY}
    if args.list:
        run, _ = mode_filter(args, checks, explicit) if (args.online or args.both or args.tests) else (checks, [])
        print_list(run, as_json=False)
        if args.json:
            Path(args.json).write_text(json.dumps(list_json(run), indent=1) + "\n")
        return 0
    run, left = mode_filter(args, checks, explicit)
    ctx = catalog.context(root, args)
    results = []
    t0 = time.monotonic()
    for c in run:
        r = run_check(c, ctx)
        results.append(r)
        print(f"{r.status:4}  {c.id}  [{c.family}, {c.mode}]  {r.seconds:.1f} s")
        print(f"      {_row_label(c)}")
        print(f"      re-checked: {c.what}")
        print(f"      against:    {c.against}")
        for ln in r.lines:
            print(f"      {ln}")
        sys.stdout.flush()
    n = {s: sum(1 for r in results if r.status == s) for s in ("PASS", "FAIL", "SKIP")}
    print(f"\n{len(results)} checks in {time.monotonic() - t0:.0f} s: {n['PASS']} PASS, {n['FAIL']} FAIL, "
          f"{n['SKIP']} SKIP")
    if left:
        by = {}
        for c, why in left:
            by.setdefault(why, []).append(c.id)
        for why, ids in by.items():
            print(f"not run: {len(ids)} {why}")
    if n["FAIL"]:
        print("FAILED: " + ", ".join(r.check.id for r in results if r.status == "FAIL"))
    if args.json:
        Path(args.json).write_text(json.dumps([{"id": r.check.id, "status": r.status, "seconds": round(r.seconds, 2),
                                                "lines": r.lines} for r in results], indent=1))
    if n["FAIL"] or (args.strict and n["SKIP"]):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
