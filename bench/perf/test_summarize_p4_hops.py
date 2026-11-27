#!/usr/bin/env python3
"""RFP-021 P.4, hops inside the user's proof: the committed records of the padded private claims through K hops
(bench/perf/results/p4-hops/) summarized again into lez/results/p4-hops/table.md, byte for byte; the parser's own
test; and a record with its settlement failed, its tampered transaction accepted or its proof line missing shown as
such, never as a settled proof. Standard library only.

    python3 -I bench/perf/test_summarize_p4_hops.py
"""

import copy
import gzip
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
RECORDS = REPO / "bench/perf/results/p4-hops"
TABLE = REPO / "lez/results/p4-hops/table.md"
TOOL = REPO / "bench/perf/summarize_p4_hops.py"


def summarize(root: pathlib.Path) -> str:
    return subprocess.run([sys.executable, "-I", str(TOOL), str(root)], check=True, capture_output=True, text=True).stdout


def main() -> None:
    subprocess.run([sys.executable, "-I", str(TOOL), "--test"], check=True)
    got = summarize(RECORDS)
    assert got == TABLE.read_text(), f"{TABLE} differs from the records' summary:\n{got}"
    rows = [line for line in got.splitlines() if line.startswith("| ") and not line.startswith("| hops")]
    assert rows and all("| yes |" in r for r in rows), "every committed run settled with its receipt verified"
    assert all(r.rstrip().endswith("| 101 |") for r in rows), "every tampered transaction refused (exit 101)"
    # A record whose settlement failed, whose tampered transaction settled, or that has no proof line.
    with tempfile.TemporaryDirectory(dir=REPO / "bench/perf/results") as tmp:
        work = pathlib.Path(tmp) / "copy"
        shutil.copytree(RECORDS, work)
        run = next(work.glob("*/claim-hops*-pad/run-1/run.json"))
        good = json.loads(run.read_text())
        for change, expect in (
            (lambda r: r.update(settle_exit=101), "| no (prove exit 0, settle exit 101) |"),
            (lambda r: r.update(exit_code=101), "| no (prove exit 101, settle exit 0) |"),
            (lambda r: r["tamper"][0].update(settle_exit=0), "| 0 |"),
        ):
            bad = copy.deepcopy(good)
            change(bad)
            run.write_text(json.dumps(bad))
            assert expect in summarize(work), f"a changed record must show {expect!r}"
        run.write_text(json.dumps(good))
        gz = run.parent / "prove.log.gz"
        text = gzip.decompress(gz.read_bytes()).decode(errors="replace")
        # A log whose binary was asked for other hops than its job's is shown as such.
        lines = text.splitlines()
        hops = int(run.parent.parent.name.split("hops")[1].split("-")[0])
        lines[0] = lines[0].replace(f"; hops {hops};", f"; hops {hops % 5 + 1};")
        gz.write_bytes(gzip.compress("\n".join(lines).encode()))
        assert "the log asked" in summarize(work), "a log asking for other hops must be flagged"
        gz.write_bytes(gzip.compress(text.encode()))
        gz.write_bytes(gzip.compress("\n".join(l for l in text.splitlines() if "REAL PROOF" not in l).encode()))
        assert "no proof" in summarize(work), "a run without its proof line must show no proof"
    print(f"test_summarize_p4_hops: {len(rows)} runs summarized as committed; parser and tamper cases ok")


if __name__ == "__main__":
    main()
