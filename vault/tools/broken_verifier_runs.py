#!/usr/bin/env python3
"""Totals of the broken-verifier invariant campaign (test/invariant/BrokenVerifierInvariants.t.sol), from the line its
afterInvariant appends after every run to ../target/broken-verifier/runs-A.tsv and runs-B.tsv. Standard library only.

    rm -rf ../target/broken-verifier && mkdir -p ../target/broken-verifier      # from vault/
    FERRATA_BROKEN_VERIFIER_RUNS=1 forge test --match-path 'test/invariant/BrokenVerifier*'
    python3 -I tools/broken_verifier_runs.py ../target/broken-verifier
"""
import sys
from pathlib import Path

COLS = ["deposits", "forged candidates", "forged finalised", "forged sunsets", "cancellations", "revocations",
        "forged releases paid", "refunds paid", "stopped", "global fullest window bps"]
TOKENS = ["ETH", "USDC", "DAI", "USDT"]
PER_TOKEN = ["deposited", "released", "refunded", "fullest window bps"]


def main():
    for f in sorted(Path(sys.argv[1]).glob("runs-*.tsv")):
        rows = [list(map(int, line.split("\t"))) for line in f.read_text().splitlines() if line.strip()]
        n = len(rows)
        col = lambda name: [r[COLS.index(name)] for r in rows]
        tok = lambda t, name: [r[len(COLS) + 4 * TOKENS.index(t) + PER_TOKEN.index(name)] for r in rows]
        print(f"## rule {f.stem[-1]}: {n} runs")
        for name in COLS[:8]:
            v = col(name)
            print(f"- {name}: {sum(v)} in all; runs with at least one: {sum(1 for x in v if x)}; most in one run: {max(v)}")
        print(f"- runs in which the emergency stop was tripped: {sum(col('stopped'))}")
        print(f"- fullest window of the global bucket: {max(col('global fullest window bps')) / 100:.2f}% of its bound")
        for t in TOKENS:
            dep, rel, ref = sum(tok(t, "deposited")), sum(tok(t, "released")), sum(tok(t, "refunded"))
            share = max((r / d if d else 0) for r, d in zip(tok(t, "released"), tok(t, "deposited")))
            print(f"- {t}: forged releases took {rel} of {dep} deposited over all runs ({100 * rel / dep:.1f}%), refunds {ref};"
                  f" at most {100 * share:.1f}% of a run's deposits; fullest window {max(tok(t, 'fullest window bps')) / 100:.2f}%"
                  f" of the cap bound")


if __name__ == "__main__":
    main()
