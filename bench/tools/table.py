#!/usr/bin/env python3
"""Turn the host's result files into markdown tables.

    python3 -I tools/table.py DIR              # every section, one column per config found in DIR
    python3 -I tools/table.py DIR --summary    # the cross-config summary (earlier all-config run)
    python3 -I tools/table.py DIR --headline   # the headline table (blst-accel, real core)

DIR holds `<config>.<modes>.txt` files written by `host` (one per config; `all` = every mode).
Every number is copied from the host output; nothing is computed except unit scaling.
"""
import re
import sys
from pathlib import Path

CONFIGS = ["blst-accel", "blst-accel-v0314", "zkcrypto-accel", "zkcrypto-plain", "blst-plain", "nopatch"]
SECTIONS = [
    "ONCHAIN", "SINGLE-ONCHAIN handoff", "SINGLE-ONCHAIN finality", "CORESPLIT 1", "CORESPLIT 2",
    "STEP 1", "STEP 2", "STEP 3", "PHASES", "E2E", "SINGLE handoff", "SINGLE finality", "SPLIT 1", "SPLIT 2",
]


def section_key(title):
    if title.startswith("SINGLE-ONCHAIN"):
        return "SINGLE-ONCHAIN handoff" if "handoff" in title else "SINGLE-ONCHAIN finality"
    if title.startswith("SINGLE"):
        return "SINGLE handoff" if "handoff" in title else "SINGLE finality"
    for prefix, key in [("CORESPLIT execution 1", "CORESPLIT 1"), ("CORESPLIT execution 2", "CORESPLIT 2"),
                        ("SPLIT execution 1", "SPLIT 1"), ("SPLIT execution 2", "SPLIT 2"),
                        ("STEP 1", "STEP 1"), ("STEP 2", "STEP 2"), ("STEP 3", "STEP 3")]:
        if title.startswith(prefix):
            return key
    return title.split(":")[0]


def parse(path):
    """{section_key: {code: (label, value)}} and the section titles."""
    out, titles, cur = {}, {}, None
    for line in path.read_text().splitlines():
        if line.startswith("### "):
            title = line[4:].strip()
            cur = section_key(title)
            titles[cur] = title
            out[cur] = {}
            continue
        m = re.match(r"^\s*([0-9.]+)\s{2}(.*)$", line)
        if cur is None or not m:
            continue
        value, label = m.group(1), m.group(2)
        if label.startswith("EXECUTION user"):
            code = "XU"
        elif label.startswith("EXECUTION total"):
            code = "XT"
            seg = re.search(r"over (\d+) segments", label)
            out[cur]["XS"] = ("segments", seg.group(1) if seg else "?")
        elif label.startswith("executor wall-clock"):
            code = "XW"
        elif label.startswith("info:"):
            code = label
        else:
            code = label.split()[0]
        out[cur][code] = (label, value)
    return out, titles


def cycles(v):
    if "." in v:
        return f"{float(v):.1f} s"
    return f"{int(v):,}"


def mega(v):
    n = int(v)
    return f"{n / 1e6:,.3f} M" if n < 1_000_000 else f"{n / 1e6:,.2f} M"


SPECIAL = {
    "XU": "**execution total, user cycles** (SessionInfo)",
    "XT": "**execution total incl. paging + padding** (Σ 2^po2)",
    "XS": "segments (2^20 cycles each)",
    "XW": "executor wall-clock",
}


def table(data, configs, section):
    rows, labels = [], {}
    for c in configs:
        prev = None
        for code, (label, _) in data[c].get(section, {}).items():
            if code not in labels:
                labels[code] = label
                # Keep each config's own order: insert after the code that precedes it there.
                rows.insert(rows.index(prev) + 1 if prev in rows else len(rows), code)
            prev = code
    print("| step | " + " | ".join(configs) + " |")
    print("|---|" + "---:|" * len(configs))
    for code in rows:
        cells = []
        for c in configs:
            entry = data[c].get(section, {}).get(code)
            cells.append(cycles(entry[1]) if entry and code != "XS" else (entry[1] if entry else "n/a"))
        print(f"| {SPECIAL.get(code, labels[code])} | " + " | ".join(cells) + " |")


SUMMARY = [
    ("PHASES", "1", "SSZ decode of the update (1)"),
    ("PHASES", "P2", "`process_update`, finality update, no committee adoption (P2)"),
    ("PHASES", "B1", "  of which `verify_checked`, subtractive (B1)"),
    ("PHASES", "P1", "`process_update` with committee handoff + adoption (P1)"),
    ("PHASES", "10a'", "  of which adoption, `check_committee`: 512 × KeyValidate (10a′)"),
    ("PHASES", "10g", "  of which the 512 G1 subgroup checks (10g)"),
    ("PHASES", "10a0", "adoption without per-key subgroup check, compressed keys (10a0)"),
    ("PHASES", "10b", "adoption from uncompressed keys, with subgroup checks (10b)"),
    ("PHASES", "10b0", "adoption from uncompressed keys, no subgroup checks (10b0)"),
    ("SINGLE finality", "XU", "one execution, finality update: total user cycles"),
    ("SINGLE finality", "R2", "  of which `from_core` (2 × `check_committee`)"),
    ("SINGLE handoff", "XU", "one execution, handoff update: total user cycles"),
    ("SINGLE handoff", "XT", "one execution, handoff update: Σ 2^po2 (incl. paging, padding)"),
    ("SPLIT 1", "XU", "split execution 1 (… Miller loops), user cycles"),
    ("SPLIT 1", "XS", "  segments"),
    ("SPLIT 2", "XU", "split execution 2 (final exp + check), user cycles"),
    ("SPLIT 2", "XS", "  segments"),
    ("E2E", "XU", "E2E execution (init + update_0 + update_1), user cycles"),
]

HEADLINE = [
    ("ONCHAIN", "A2", "`process_update`, finality update (no adoption)"),
    ("ONCHAIN", "A1", "`process_update`, handoff update, `Adoption::OnChain`"),
    ("ONCHAIN", "B1", "  of which `check_committee_onchain` (512 keys on-curve + not identity, sum)"),
    ("ONCHAIN", "B3", "  of which committee `hash_tree_root` + next-committee branch"),
    ("SINGLE-ONCHAIN finality", "XU", "one whole execution, finality update: decode saved state + update, policy, new root"),
    ("SINGLE-ONCHAIN finality", "XT", "  the same incl. paging + padding (Σ 2^po2)"),
    ("SINGLE-ONCHAIN handoff", "XU", "one whole execution, handoff update (OnChain adoption)"),
    ("SINGLE-ONCHAIN handoff", "XT", "  the same incl. paging + padding (Σ 2^po2)"),
    ("CORESPLIT 1", "XU", "split via the core, execution 1: finality checks + `Blst::miller_half`"),
    ("CORESPLIT 1", "C1e", "  of which `Blst::miller_half` itself"),
    ("CORESPLIT 2", "XU", "split via the core, execution 2: `Blst::final_half`"),
    ("CORESPLIT 2", "C2", "  of which `Blst::final_half` itself"),
    ("STEP 1", "XU", "3-step job, step 1: everything before the Miller loops"),
    ("STEP 2", "XU", "3-step job, step 2: Miller loops"),
    ("STEP 2", "T2b", "  of which the two Miller loops + Fp12 mul"),
    ("STEP 3", "XU", "3-step job, step 3: final exponentiation + check (`final_half`)"),
    ("ONCHAIN", "O2", "`initialize_with(.., Adoption::OnChain)` from the bootstrap"),
    ("ONCHAIN", "B2", "`check_committee` (Full: per-key subgroup check), for comparison"),
    ("PHASES", "P1", "`process_update`, handoff update, `Adoption::Full` (for comparison)"),
    ("ONCHAIN", "R2", "`FinalizedStore::from_core` (restore; forces the Full check on both committees)"),
]


def rows_table(data, configs, spec, header):
    print(f"| {header} | " + " | ".join(configs) + " |")
    print("|---|" + "---:|" * len(configs))
    for section, code, label in spec:
        cells = []
        for c in configs:
            entry = data[c].get(section, {}).get(code)
            if entry is None:
                cells.append("n/a")
            elif code == "XS":
                cells.append(entry[1])
            else:
                segs = data[c].get(section, {}).get("XS")
                extra = f" ({segs[1]} seg.)" if code == "XU" and segs else ""
                cells.append(mega(entry[1]) + extra)
        print(f"| {label} | " + " | ".join(cells) + " |")


def main():
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "results")
    flag = sys.argv[2] if len(sys.argv) > 2 else ""
    data, titles = {}, {}
    for c in CONFIGS:
        found = sorted(root.glob(f"{c}.*.txt"))
        if found:
            data[c], t = parse(found[0])
            titles.update(t)
    configs = [c for c in CONFIGS if c in data]
    if flag == "--summary":
        rows_table(data, configs, SUMMARY, "")
        return
    if flag == "--headline":
        rows_table(data, configs, HEADLINE, "update_1 (period 1900, 510/512 signers), user cycles")
        return
    print("Exact user cycles (env::cycle_count deltas; execution rows from SessionInfo). n/a: not applicable to that library.")
    for section in SECTIONS:
        present = [c for c in configs if section in data[c]]
        if present:
            print(f"\n#### {titles[section]}\n")
            table(data, present, section)


if __name__ == "__main__":
    main()
