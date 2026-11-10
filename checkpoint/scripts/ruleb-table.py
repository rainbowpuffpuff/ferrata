#!/usr/bin/env python3
"""Rule (B) against rule (A): tables from results/ruleb-bench.jsonl (written by `host bench`).

Measured: every user-cycle and segment count is a row of the JSON lines. Derived: the marginal
cost per attestor (rule (B)) and per supporter (rule (A)) is the least-squares slope of the total
over t on one source's blocks, and the intercept is everything else in that proof.

    python3 -I scripts/ruleb-table.py [results/ruleb-bench.jsonl]
"""

import json
import sys
from pathlib import Path

path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "results/ruleb-bench.jsonl"
rows = {}
for line in path.read_text().splitlines():
    r = json.loads(line)
    rows[r["name"]] = r  # the last run of a name wins

COMMITTEES = [3, 4, 7, 10, 15]
SOURCES = [("empty-1", "T + 3 empty blocks"), ("empty-360", "idle hour"), ("pp5-360", "hour, 5 PP txs"),
           ("pp50-360", "hour, 50 PP txs")]


def cyc(name):
    r = rows.get(name)
    return None if r is None else r["user_cycles"]


def fmt(x):
    return "–" if x is None else f"{x:,}"


def fit(points):
    """Least-squares line through (t, cycles)."""
    n = len(points)
    mx = sum(p[0] for p in points) / n
    my = sum(p[1] for p in points) / n
    sxx = sum((p[0] - mx) ** 2 for p in points)
    slope = sum((p[0] - mx) * (p[1] - my) for p in points) / sxx
    worst = max(abs(p[1] - (my + slope * (p[0] - mx))) for p in points)
    return slope, my - slope * mx, worst


def t_of(n):
    return max(1, min(-(-2 * n // 3), n - 1))


print("Original rule (A) fixtures, n = 4, re-run on this build (RESULTS §2 values for comparison):")
for name in ("cp-empty-1", "cp-empty-360", "cp-pp5-360", "cp-pp50-360"):
    r = rows.get(name)
    if r:
        print(f"  {name:<14} {r['user_cycles']:>12,} user cycles, {r['segments']} segments")
print()

print("| n | t | source | rule (A) user cycles (segments) | rule (B) user cycles (segments) | (B) − (A) |")
print("|---|---|---|---:|---:|---:|")
for source, label in SOURCES:
    for n in COMMITTEES:
        a, b = rows.get(f"ra-{source}-n{n}"), rows.get(f"rb-{source}-n{n}")
        da = "–" if a is None else f"{a['user_cycles']:,} ({a['segments']})"
        db = "–" if b is None else f"{b['user_cycles']:,} ({b['segments']})"
        diff = "–" if a is None or b is None else f"{b['user_cycles'] - a['user_cycles']:+,}"
        print(f"| {n} | {t_of(n)} | {label} | {da} | {db} | {diff} |")
print()

print("Rule (B) over T alone (one empty block, every attestation on T):")
print("| n | t | user cycles | segments |")
print("|---|---|---:|---:|")
for n in COMMITTEES:
    r = rows.get(f"rb-tonly-n{n}")
    if r:
        print(f"| {n} | {t_of(n)} | {r['user_cycles']:,} | {r['segments']} |")
print()

print("Derived: slope of the total over t (least squares over n = 3, 4, 7, 10, 15), and the intercept:")
for prefix, what in (("rb-tonly", "rule (B), T alone"), ("rb-empty-1", "rule (B), T + 3"),
                     ("rb-empty-360", "rule (B), idle hour"), ("rb-pp5-360", "rule (B), 5 PP"),
                     ("rb-pp50-360", "rule (B), 50 PP"), ("ra-empty-1", "rule (A), T + 3"),
                     ("ra-empty-360", "rule (A), idle hour"), ("ra-pp5-360", "rule (A), 5 PP"),
                     ("ra-pp50-360", "rule (A), 50 PP")):
    pts = [(t_of(n), cyc(f"{prefix}-n{n}")) for n in COMMITTEES if cyc(f"{prefix}-n{n}") is not None]
    if len(pts) >= 2:
        slope, icept, worst = fit(pts)
        print(f"  {what:<22} {slope:>11,.0f} cycles per t, intercept {icept:>12,.0f}, worst residual {worst:>9,.0f}")
