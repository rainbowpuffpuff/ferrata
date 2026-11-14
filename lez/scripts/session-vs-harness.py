#!/usr/bin/env python3
"""Each transaction of a live session's driver log (lez/live/driver) against the harness row of the same label.

    python3 -I lez/scripts/session-vs-harness.py DRIVER_LOG HARNESS_RESULTS_DIR

For a session that has no export (stopped before it, as the rc3 session of lez/results/v0.4.0-rc3/README.md):
every "[phase] <label> <outcome> gas <N> block <B> + <T>s" line is paired with the row whose transaction column is
<label> in HARNESS_RESULTS_DIR/stage1-*.md, and the outcome and execution gas are compared (the harness's "result"
column against the driver's outcome word). Prints one line per pair, then the counts. Standard library only.
"""
import glob
import os
import re
import sys


def harness_rows(d: str) -> dict:
    rows = {}
    for f in sorted(glob.glob(os.path.join(d, "stage1-*.md"))):
        for line in open(f):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) >= 8 and cells[0].isdigit():
                gas = cells[3].replace(",", "")
                if gas.isdigit():
                    rows.setdefault(cells[1], (int(gas), cells[-1].split(":")[0].split(" ")[0], os.path.basename(f)))
    return rows


def main() -> int:
    log, d = sys.argv[1], sys.argv[2]
    rows = harness_rows(d)
    pat = re.compile(r"^\[(\w[\w-]*)\] (.+?)\s{2,}(applied|refused|reverted|dropped)\b.*?\bgas\s+(\d+)\s+block\s+(\d+)\s+\+\s*([\d.]+)s")
    n = same_outcome = same_gas = unmatched = 0
    settle = []
    for line in open(log, errors="replace"):
        m = pat.match(line)
        if not m:
            continue
        n += 1
        phase, label, outcome, gas, block, secs = m.groups()
        settle.append(float(secs))
        h = rows.get(label.strip())
        if h is None:
            unmatched += 1
            print(f"{phase:9} {label.strip():52} {outcome:8} gas {gas:>9}  (no harness row)")
            continue
        o_ok = h[1] == outcome
        g_ok = h[0] == int(gas)
        same_outcome += o_ok
        same_gas += g_ok
        print(f"{phase:9} {label.strip():52} {outcome:8} gas {gas:>9}  harness {h[0]:>9} {h[1]:8} "
              f"{'=' if o_ok and g_ok else 'DIFFERS'} ({h[2]})")
    matched = n - unmatched
    print(f"\n{n} transactions in the log; {matched} with a harness row: the same outcome in {same_outcome}, the same "
          f"execution gas in {same_gas}; {unmatched} without a row (deployment and others)")
    if settle:
        print(f"sent to settled: {min(settle):.1f}-{max(settle):.1f} s, mean {sum(settle) / len(settle):.1f} s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
