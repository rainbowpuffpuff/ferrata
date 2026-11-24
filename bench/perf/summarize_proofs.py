#!/usr/bin/env python3
"""Tables of RFP-021 P.2's proofs from prove-actions.sh or prove_modal.py output directories (index.tsv,
machine.txt, prove-NNN.log, private-*.log).

    python3 -I bench/perf/summarize_proofs.py OUT_DIR [MORE_DIR ...] > lez/results/perf-proofs.md

The first directory's index.tsv lists the transitions. A later directory may hold proofs of some of them from an
earlier dump: its index rows for those transitions must equal the first's (the same handler, image id, session
limit, cycles, input and journal sizes and transaction gas), or the summary stops. Each transition's proof is taken
from the first directory, in the order given, whose log holds a RESULT line, and must be that transition's: its
RESULT line's transition number and user cycles, and the handler its `image <id> (<handler>)` line names, equal
index.tsv's. Every proof is checked against its image id: the `image` line's id must equal index.tsv's and be the row
of the first table of docs/image-ids.md (this tree's guests) of the guest the handler runs in, `ferrata_consumer`
for the reference consumer's handlers (`consumer …`) and `ferrata_lez` for every other and for each private run's
program; where a directory holds the stage's SHA256SUMS, it must list the guest, with the sha256 of that row. The
check is printed as the file's last section. A record that fails any of this stops the summary: nothing is printed
and the exit status is 1 (`bench/perf/test_summarize_proofs.py` tests the refusals)."""

import contextlib
import io
import json
import re
import sys
from collections import OrderedDict
from pathlib import Path


def g(n):
    return f"{int(n):,}"


def gnu_time(text):
    rss = re.search(r"Maximum resident set size \(kbytes\): (\d+)", text)
    wall = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([\d:.]+)", text)
    cpu = re.search(r"Percent of CPU this job got: (\d+)%", text)
    secs = None
    if wall:
        parts = [float(p) for p in wall.group(1).split(":")]
        secs = sum(p * 60 ** i for i, p in enumerate(reversed(parts)))
    return (int(rss.group(1)) if rss else None, secs, int(cpu.group(1)) if cpu else None)


CPU_NAMES = {("AuthenticAMD", "25", "1"): "AMD Zen 3", ("AuthenticAMD", "25", "17"): "AMD Zen 4", ("AuthenticAMD", "23", "49"): "AMD Zen 2",
             ("GenuineIntel", "6", "85"): "Intel Xeon (Skylake/Cascade Lake)", ("GenuineIntel", "6", "106"): "Intel Xeon (Ice Lake)",
             ("GenuineIntel", "6", "143"): "Intel Xeon (Sapphire Rapids)"}


ROOT = Path(__file__).resolve().parents[2]
SAME = ("tx_label", "position", "handler", "image_id", "session_limit", "exec_cycles", "input_bytes", "journal_bytes",
        "tx_gas_exec", "tx_gas_stor")


def tree_ids():
    """docs/image-ids.md's first table, this tree's guests: image id (hex, no 0x) -> (name, sha256 of the binary)."""
    out, seen = {}, False
    for line in (ROOT / "docs/image-ids.md").read_text().splitlines():
        if line.startswith("| Program | Image id |"):
            seen = True
            continue
        if seen and not line.startswith("|"):
            break
        if seen and line.startswith("| ") and "`0x" in line:
            cells = [c.strip() for c in line.strip("|").split("|")]
            name = re.findall(r"`([^`]+)`", cells[0])[-1]
            out[cells[1].strip("`").removeprefix("0x")] = (name, cells[2].strip("`"))
    return out


def stage_sums(d):
    """The stage's SHA256SUMS in a results directory: dump/elf-<id>.bin -> sha256."""
    p = d / "SHA256SUMS"
    if not p.exists():
        return {}
    pairs = (l.split() for l in p.read_text().splitlines() if l.strip())
    return {m.group(1): h for h, f in pairs if (m := re.fullmatch(r"dump/elf-([0-9a-f]{64})\.bin", f))}


def machine_of(out, n):
    """The container a prove_modal.py proof ran in (machine-NNN.json), or '' for prove-actions.sh's one machine."""
    p = out / f"machine-{n:03d}.json"
    if not p.exists():
        return ""
    m = json.loads(p.read_text())
    cpu = CPU_NAMES.get((m.get("vendor"), m.get("family"), m.get("model")),
                        f"{m.get('vendor')} family {m.get('family')} model {m.get('model')}")
    return f"{cpu}, {m.get('nproc')} CPUs"


def spans(ns):
    """[1, 2, 3, 5] -> [(1, 3), (5, 5)]."""
    out = []
    for n in ns:
        if out and n == out[-1][1] + 1:
            out[-1] = (out[-1][0], n)
        else:
            out.append((n, n))
    return out


def index_of(d):
    rows = [l.split("\t") for l in (d / "index.tsv").read_text().splitlines()]
    return {h: i for i, h in enumerate(rows[0])}, rows[1:]


def main():
    dirs = [Path(a) for a in sys.argv[1:]]
    if not dirs:
        sys.exit("usage: summarize_proofs.py OUT_DIR [MORE_DIR ...]")
    out = dirs[0]
    col, rows = index_of(out)
    by_n = {int(r[col["n"]]): r for r in rows}
    for d in dirs[1:]:
        c, rs = index_of(d)
        for r in rs:
            mine = by_n.get(int(r[c["n"]]))
            if mine is None or any(mine[col[k]] != r[c[k]] for k in SAME):
                sys.exit(f"{d}/index.tsv row {r[c['n']]} is not {out}/index.tsv's: the dumps differ")
    ids = tree_ids()
    results, source, checked = {}, {}, {}
    for r in rows:
        n = int(r[col["n"]])
        for d in dirs:
            log = d / f"prove-{n:03d}.log"
            if not log.exists():
                continue
            text = log.read_text(errors="replace")
            m = re.search(r"^RESULT\t(.*)$", text, re.M)
            if not m:
                results.setdefault(n, None)
                continue
            image = re.search(r"^image ([0-9a-f]{64}) \(", text, re.M)
            if not image or image.group(1) != r[col["image_id"]]:
                sys.exit(f"{log}: proven against {image.group(1) if image else 'no image line'}, index.tsv says {r[col['image_id']]}")
            checked.setdefault(image.group(1), []).append((n, d))
            f = m.group(1).split("\t")
            # The record is this transition's: its number, its cycles and its handler, as index.tsv gives them.
            handler = re.search(r"^image [0-9a-f]{64} \((.*)\)$", text, re.M)
            if int(f[0]) != n or f[1] != r[col["exec_cycles"]] or not handler or handler.group(1) != r[col["handler"]]:
                sys.exit(f"{log}: RESULT of transition {f[0]}, {f[1]} user cycles, handler "
                         f"{handler.group(1) if handler else '?'}; index.tsv row {n}: {r[col['exec_cycles']]}, {r[col['handler']]}")
            # And of the right guest: the consumer's handlers on the reference consumer, every other on the program.
            want = "ferrata_consumer" if r[col["handler"]].startswith("consumer") else "ferrata_lez"
            if ids.get(image.group(1), ("not a row of docs/image-ids.md",))[0] != want:
                sys.exit(f"{log}: proven on {ids.get(image.group(1), ('not a row of docs/image-ids.md',))[0]}, not {want}")
            rss, wall, cpu = gnu_time(text)
            results[n] = dict(user=int(f[1]), total=int(f[2]), segs=int(f[3]), comp_s=float(f[4]), comp_b=int(f[5]),
                              succ_s=float(f[6]), succ_inner_b=int(f[7]), succ_b=int(f[8]), verify_ms=float(f[9]),
                              rss_kb=rss, wall=wall, cpu=cpu, machine=machine_of(d, n))
            source[n] = d
            break

    print("# RFP-021 P.2: proof time and proof size of every program action (real proofs, CPU)\n")
    print(f"Generated by `python3 -I bench/perf/summarize_proofs.py {' '.join(str(d) for d in dirs)}` from the logs of `perf_prove --prove`")
    print("(`lez/harness/src/bin/perf_prove.rs`), run by `bench/perf/prove_modal.py` (one container per transition) or")
    print("`bench/perf/prove-actions.sh` (one machine). **Measured.** Each row is one guest execution of the attestation")
    print("program (or the reference consumer) with the exact input LEZ v0.4.0-rc1 gives it in the transaction named,")
    print("proven for real by `r0vm` (`RISC0_DEV_MODE=0`): a composite receipt (one per segment), verified against")
    print("the image id with its journal compared to the executor's, then compressed to one succinct receipt, verified")
    print("again. LEZ v0.4 does not prove public transactions (every node re-executes them); these are the proofs a")
    print("prover of the same execution would make, RFP-020's method (\"RISC0 proof time, RISC0 proof size\"). Peak")
    print("memory is the whole process's (`/usr/bin/time -v`, maximum resident set of `r0vm`, which proves).\n")
    print("Machines, per results directory (the transitions each one proved; each proof's own host is in the last table):\n")
    for d in dirs:
        ns = sorted(n for n, x in source.items() if x == d)
        span = ", ".join(f"{a}–{b}" if a != b else f"{a}" for a, b in spans(ns)) or "none"
        privs = sorted(p.stem for p in d.glob("private-*.log"))
        machine = (d / "machine.txt").read_text().strip() if (d / "machine.txt").exists() else "(not recorded)"
        print(f"- `{d.name}`: transitions {span}" + (f"; {', '.join(privs)}" if privs else "") + f"\n\n  ```\n  {machine}\n  ```\n")

    actions = OrderedDict()
    for r in rows:
        actions.setdefault(r[col["action"]], []).append(r)
    print("## Per action (one transaction)\n")
    print("| action | transitions | exec gas | stor gas | user cycles proven | segments | prove (composite) s | compress to succinct s | composite B | succinct B (one per transition) | peak RSS GiB |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for action, rs in actions.items():
        res = [results.get(int(r[col["n"]])) for r in rs]
        if any(x is None for x in res):
            missing = sum(1 for x in res if x is None)
            print(f"| {action} | {len(rs)} | {g(rs[0][col['tx_gas_exec']])} | {g(rs[0][col['tx_gas_stor']])} | not proven ({missing} of {len(rs)} missing) | | | | | | |")
            continue
        rss = max(x["rss_kb"] or 0 for x in res) / 1024 / 1024
        print(
            f"| {action} | {len(rs)} | {g(rs[0][col['tx_gas_exec']])} | {g(rs[0][col['tx_gas_stor']])} | "
            f"{g(sum(x['user'] for x in res))} | {sum(x['segs'] for x in res)} | {sum(x['comp_s'] for x in res):.1f} | "
            f"{sum(x['succ_s'] for x in res):.1f} | {g(sum(x['comp_b'] for x in res))} | "
            f"{' + '.join(g(x['succ_inner_b']) for x in res)} | {rss:.2f} |"
        )
    # The proven path's private transactions, one section per kind (prove_private's "private transition (VerifyAll,
    # <kind>)" line): a finality update (p2-a) or a committee handoff (p2-b, `--handoff`).
    about = {
        "finality update": ("a whole finality update in one private transaction",
                            "`prove_private` (`lez/harness/src/bin/prove_private.rs`, `lez/RESULTS.md` §3): the program's `VerifyAll`\n"
                            "transition (step 1, Miller loops, final exponentiation, `ApplyVerified`) at a private account's actor, the fee\n"
                            "transfer, and LEZ's privacy-preserving circuit over both, proven for real (`execute_and_prove`), then settled\n"
                            "through LEZ's state machine with the receipt verified; the update is mainnet's (period 1899 fixture).\n"),
        "committee handoff, 512 keys adopted": ("a committee handoff in one private transaction",
                            "`prove_private --handoff` (`lez/harness/src/bin/prove_private.rs`): the program's `VerifyAll` transition over a\n"
                            "committee handoff (mainnet's update_1 as served, its 512 keys adopted, no new header) at a private account's\n"
                            "actor, the fee transfer, and LEZ's privacy-preserving circuit over both, proven for real (`execute_and_prove`),\n"
                            "then settled through LEZ's state machine with the receipt verified.\n"),
    }
    by_kind = OrderedDict()
    for log in sorted(p for d in dirs for p in d.glob("private-*.log")):
        text = log.read_text(errors="replace")
        image = re.search(r"^program image ([0-9a-f]{64}) ", text, re.M)
        if not image:
            sys.exit(f"{log}: no program image line")
        if ids.get(image.group(1), (None,))[0] != "ferrata_lez":
            sys.exit(f"{log}: program image {image.group(1)} is not ferrata_lez's")
        checked.setdefault(image.group(1), []).append((log.stem, log.parent))
        k = re.search(r"private transition \(VerifyAll, ([^)]*)\):", text)
        by_kind.setdefault(k.group(1) if k else "?", []).append(log)
    for kind, logs in by_kind.items():
        title, text_about = about.get(kind, (f"one private transaction ({kind})", ""))
        print(f"\n## The proven update path: {title}\n" if kind == "finality update" else f"\n## The proven path: {title}\n")
        if text_about:
            print(text_about)
        print("| run | private transition: user cycles (segments) | `execute_and_prove` s | peak RSS GiB | receipt B | transaction B | settled: execution gas / storage gas | settle exit | machine |")
        print("|---|---:|---:|---:|---:|---:|---|---:|---|")
        for log in logs:
            text = log.read_text(errors="replace")
            i = log.stem.removeprefix("private-")
            cyc = re.search(r"private transition \(VerifyAll, [^)]*\): ([\d,]+) user cycles, (\d+) segments", text)
            proof = re.search(r"REAL PROOF: execute_and_prove ([\d.]+) s.*?receipt \(.*?\) (\d+) bytes; transaction (\d+) bytes", text)
            settled = re.findall(r"settled: execution gas ([\d,]*\d) .*?storage gas ([\d,]*\d)", text)
            sx = re.search(r"settle exit (\d+)", text)
            rss, _, _ = gnu_time(text)
            mp = log.parent / f"machine-{log.stem}.json"
            mach = ""
            if mp.exists():
                m = json.loads(mp.read_text())
                mach = CPU_NAMES.get((m.get("vendor"), m.get("family"), m.get("model")), f"{m.get('vendor')} {m.get('family')}/{m.get('model')}") + f", {m.get('nproc')} CPUs"
            print(f"| {i} | {cyc.group(1) if cyc else '?'} ({cyc.group(2) if cyc else '?'}) | {proof.group(1) if proof else 'failed'} | "
                  f"{(rss or 0) / 1024 / 1024:.2f} | {int(proof.group(2)):,} | {int(proof.group(3)):,} | "
                  f"{' then '.join(f'{a} / {b}' for a, b in settled) or '?'} | {sx.group(1) if sx else '?'} | {mach} |"
                  if proof else f"| {i} | | failed | | | | | | {mach} |")

    print("\n## Per transition\n")
    print("| n | action | handler | exec cycles | user cycles | total cycles (padded) | segments | composite s | composite B | succinct s | succinct InnerReceipt B | succinct Receipt B | verify ms | peak RSS GiB | CPU % | machine |")
    print("|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|")
    for r in rows:
        n = int(r[col["n"]])
        x = results.get(n)
        if not x:
            print(f"| {n} | {r[col['action']]} | {r[col['handler']]} | {g(r[col['exec_cycles']])} | {'failed' if n in results else 'not run'} | | | | | | | | | | | |")
            continue
        print(
            f"| {n} | {r[col['action']]} | {r[col['handler']]} | {g(r[col['exec_cycles']])} | {g(x['user'])} | {g(x['total'])} | "
            f"{x['segs']} | {x['comp_s']:.1f} | {g(x['comp_b'])} | {x['succ_s']:.1f} | {g(x['succ_inner_b'])} | {g(x['succ_b'])} | "
            f"{x['verify_ms']:.1f} | {(x['rss_kb'] or 0) / 1024 / 1024:.2f} | {x['cpu'] or ''} | {x['machine']} |"
        )


    print("\n## Every record checked against its image id\n")
    print("Each proof's `image` line (perf_prove computes the id from the guest binary it proves, verifies both receipts")
    print("against it and checks the journal) equals index.tsv's; each private run's `program image` line is its")
    print("program's. Every id is a row of the first table of [`docs/image-ids.md`](../../docs/image-ids.md), this tree's")
    print("guests; where the stage's SHA256SUMS is kept, the guest binary's sha256 equals that row's.\n")
    print("| image id | docs/image-ids.md | binary sha256 (stage) | proofs (results directory: transitions) |")
    print("|---|---|---|---|")
    bad = []
    for image, where in checked.items():
        name, want = ids.get(image, (None, None))
        sums = {stage_sums(d).get(image) for _, d in where} - {None}
        sha = "not kept" if not sums else ("equal" if sums == {want} else f"DIFFERS: {', '.join(sorted(sums))}")
        if any((d / "SHA256SUMS").exists() and image not in stage_sums(d) for _, d in where):
            sha = "DIFFERS: a stage's SHA256SUMS lacks this guest"
        if name is None or sha.startswith("DIFFERS"):
            bad.append(image)
        per = OrderedDict()
        for n, d in where:
            per.setdefault(d.name, []).append(n)
        text = "; ".join(
            f"`{k}`: " + ", ".join([f"{a}–{b}" if a != b else f"{a}" for a, b in spans(sorted(x for x in v if isinstance(x, int)))]
                                   + sorted(x for x in v if not isinstance(x, int)))
            for k, v in per.items())
        print(f"| `0x{image[:8]}…{image[-4:]}` | {('`' + name + '`') if name else 'NOT A ROW: not this tree'} | {sha} | {text} |")
    if bad:
        sys.exit(f"records not of this tree's images: {', '.join(bad)}")


if __name__ == "__main__":
    # Nothing is printed unless every record passes: a refused record leaves no summary behind it.
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        main()
    sys.stdout.write(out.getvalue())
