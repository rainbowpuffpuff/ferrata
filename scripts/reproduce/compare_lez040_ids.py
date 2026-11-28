#!/usr/bin/env python3
"""Compare a v0.4.0 image-id record's ids.tsv with a rebuild's (scripts/verify-lez040-ids.sh).

    python3 -I scripts/reproduce/compare_lez040_ids.py RECORDED.tsv REBUILT.tsv NAME

One line per recorded guest: the recorded and the rebuilt image id and whether the R0BF binaries' sha256 are equal.
Exit 1 if a recorded guest was not rebuilt or differs in its id or its binary.
"""
import sys


def rows(path: str) -> dict:
    try:
        lines = open(path).read().splitlines()[1:]
    except OSError:
        return {}
    return {r.split("\t")[1]: r.split("\t") for r in lines if r.strip()}


def main() -> int:
    rec, got, name = rows(sys.argv[1]), rows(sys.argv[2]), sys.argv[3]
    if not rec:
        print(f"{name}: the record lists no guest")
        return 1
    bad = False
    for n, r in sorted(rec.items()):
        g = got.get(n)
        if g is None:
            print(f"{name}: {n}: recorded {r[2]}, not rebuilt")
            bad = True
            continue
        same = g[2] == r[2] and g[3] == r[3]
        print(f"{name}: {n}: recorded {r[2]}, rebuilt {g[2]}, binary sha256 {'equal' if g[3] == r[3] else 'different'}: "
              + ("same" if same else "DIFFERENT"))
        bad |= not same
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
