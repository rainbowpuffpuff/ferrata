"""`just m0`: every check of milestone M0 (the proposal's "M0: Open checks") with its measured value and GO / NO-GO.

    python3 -I scripts/m0.py [--online] [--json FILE] [--strict] [--p4 DIR]

Each value is read from a result file of this repository (named beside it), never typed in here; the few facts that
are not measurements (an upstream release, Logos's answers) are listed in FACTS with their source and date, and
`--online` re-reads the upstream ones from GitHub (read-only `git ls-remote`). `--p4 DIR` reads the user-proving
summaries from DIR instead of lez/results/p4-minimum (a fresh `just bench prove` writes them there).
Verdicts: GO, NO-GO (measured and failed, or not built: the proposal's "Otherwise" applies), OPEN (not measured yet,
or waiting on someone else). Exit status 0; with --strict, 1 unless every check is GO.
Standard library only.
"""

import argparse
import glob
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIMIT_S = 20 * 60  # GO: <= 20 min each
LIMIT_GIB = 16.0  # GO: <= 16 GB each (the container's hard limit is 16 GiB; GB vs GiB: the peak is far below both)
STOP_GIB = 32.0  # above 32 GB: STOP for that requirement
LEZ_REPO = "https://github.com/logos-blockchain/logos-execution-zone.git"
TOKEN_REPO = "https://github.com/logos-blockchain/lez-programs.git"

# Facts that are not measurements. Each has its source; --online re-checks the upstream ones.
FACTS = {
    "lez_newest_tag": ("v0.4.0-rc3", "lez/results/v0.4.0-rc3/README.md (tagged 11 Nov 2026, commit bafd07b)"),
    "token_program_v04": (
        "released for v0.2.4; the v0.4 port is an unmerged branch",
        "proposal §B9 and README.md (state of 11 Nov 2026, spike/upstream/lez-programs at 6674d78)",
    ),
    "ferrata_attestor_answer": ("pending: asked in the proposal's review, due by day 5 of M0", "proposal §A5 rule (B)"),
}


def rel(p):
    return os.path.relpath(p, ROOT)


def load(path):
    with open(path) as f:
        return json.load(f)


def fmt_s(s):
    return f"{s:,.1f} s ({s / 60:.1f} min)"


def check_user_proving(p4):
    """Check 1: claim and attestation padded as LEZ's wallet pads, and a release receipt, at the declared minimum."""
    pad_f, scale_f = os.path.join(p4, "summary-pad.json"), os.path.join(p4, "summary-scale.json")
    rows, verdicts = [], []
    if not os.path.exists(pad_f):
        return dict(verdict="OPEN", value=f"no {rel(pad_f)}: run `just bench prove`", rows=[], sources=[rel(pad_f)])
    pad_all = load(pad_f)
    pad = pad_all["proofs"]
    cpus_seen = set()
    for r in pad_all.get("runs", []):
        m = r.get("machine") or {}
        cpus_seen.add(f"{m.get('vendor', '?')} family {m.get('family', '?')} model {m.get('model', '?')}"
                      f" ({m.get('cloud', '?')} {m.get('region', '?')})")
    for label in ("private attestation", "private claim", "private burn"):
        p = pad.get(label)
        if not p:
            rows.append((label, "not measured", "OPEN")); verdicts.append("OPEN"); continue
        for cpus in ("16", "8"):
            med = (p.get("median_s") or {}).get(cpus)
            if med is None:
                continue
            gib = max(p.get("peak_rss_gib", 0), p.get("peak_container_gib", 0))
            ok = med <= LIMIT_S and gib <= LIMIT_GIB
            settled = f"{p.get('settled_with_receipt_verified')}/{p.get('of')} settled with the receipt verified"
            v = "GO" if ok else ("STOP" if gib > STOP_GIB else "NO-GO")
            minimum = cpus == "16"
            rows.append((f"{label}, padded, {cpus} threads{' (the declared minimum)' if minimum else ' (below it)'}",
                         f"median {fmt_s(med)}, peak {gib:.2f} GiB; {settled}", v))
            if minimum and label != "private burn":
                verdicts.append(v)
    if os.path.exists(scale_f):
        rel_runs = load(scale_f).get("release", {})
        for cpus in ("16", "8"):
            runs = rel_runs.get(cpus) or []
            ts = [r["prove"]["s"] for r in runs if r.get("prove", {}).get("s") is not None]
            if not ts:
                continue
            peak = max(max(r["prove"].get("sampled_max", {}).get("cgroup_current_bytes", 0) for r in runs) / 2**30,
                       max(int(r["prove"].get("gnu_time", {}).get("max_rss_kib", 0)) for r in runs) / 2**20)
            ok = max(ts) <= LIMIT_S and peak <= LIMIT_GIB
            v = "GO" if ok else "NO-GO"
            rows.append((f"release receipt (succinct), {cpus} threads{' (the declared minimum)' if cpus == '16' else ' (below it)'}",
                         f"{', '.join(f'{t:.1f}' for t in ts)} s, peak {peak:.2f} GiB", v))
            if cpus == "16":
                verdicts.append(v)
    else:
        rows.append(("release receipt", "not measured", "OPEN")); verdicts.append("OPEN")
    mac_f = os.path.join(ROOT, "lez/results/p4-macos/summary.json")
    mac_verdicts = []
    if os.path.exists(mac_f):
        mac = load(mac_f)
        names = {"m1": "Apple M1 Mac mini, 8 cores, 16 GB (the declared minimum on macOS)",
                 "m4": "Apple M4 Mac mini, 10 cores, 24 GB"}
        for key in ("m1", "m4"):
            by = {}
            for r in (mac.get(key) or {}).get("proofs") or []:
                if r.get("prove_exit") == 0 and r.get("settle_exit") == 0 and r.get("execute_and_prove_s") is not None:
                    by.setdefault(r.get("label") or r.get("job"), []).append(r)
            rel_ok = [r for r in (mac.get(key) or {}).get("release") or [] if r.get("prove_exit") == 0 and r.get("wall_s")]
            if rel_ok:
                by["release receipt (succinct)"] = [dict(r, execute_and_prove_s=r["wall_s"]) for r in rel_ok]
            for label, rs in sorted(by.items()):
                ts = sorted(x["execute_and_prove_s"] for x in rs)
                med = ts[len(ts) // 2] if len(ts) % 2 else (ts[len(ts) // 2 - 1] + ts[len(ts) // 2]) / 2
                peak = max((x.get("time_l") or {}).get("max_rss_bytes", 0) for x in rs) / 2**30
                v = "GO" if max(ts) <= LIMIT_S and peak <= LIMIT_GIB else "NO-GO"
                rows.append((f"{label}, {names[key]}", f"median {fmt_s(med)}, peak {peak:.2f} GiB; {len(rs)} runs, settled", v))
                if key == "m1" and label != "private burn":
                    mac_verdicts.append(v)
    else:
        rows.append(("macOS (Apple Silicon)", "not measured: no macOS machine in the measurements", "OPEN"))
    rows.append(("a laptop (not a cloud container or a Mac mini)", "not measured at the minimum", "OPEN"))
    linux = "GO" if verdicts and all(v == "GO" for v in verdicts) else ("NO-GO" if "NO-GO" in verdicts or "STOP" in verdicts else "OPEN")
    macos = "GO" if mac_verdicts and all(v == "GO" for v in mac_verdicts) else ("NO-GO" if "NO-GO" in mac_verdicts else "OPEN")
    return dict(verdict=f"{linux} (Linux); {macos} (macOS, M1 16 GB)",
                value="Linux x86-64 Modal containers whose CPU and memory request equals their limit (16 GiB); CPUs: "
                      + ("; ".join(sorted(cpus_seen)) or "not recorded"),
                rows=rows, sources=[rel(pad_f), rel(scale_f), "lez/results/p4-minimum/README.md"])


GAS_DROP_CHECKS = ("session", "zero_before", "negative", "receipt", "relayer", "sealed", "claim", "accounting")
GAS_DROP_OTHERWISE = ("otherwise", "first claims are faucet-funded, the correlation disclosed, Logos decides", "")


def gas_drop_verdict(summary, newest):
    """Check 2 from a gas-drop summary (lez/scripts/gas-drop-summary.py --write): GO only when it is of the newest tag,
    its sequencer ran without RISC0_DEV_MODE, and every check of the summary passed: the claimant's private and public
    native balances zero before, the negative control (the same claim without the credit) refused, the receipt real,
    the relayer's drop sent once, the credit sealed, and the claim that sweeps it applied with a real proof in a block
    final on Bedrock, its fee paid from the credit. Returns (verdict, value, rows)."""
    checks = summary.get("checks", {})
    rows = [(k, (checks.get(k) or {}).get("read", "not in the summary"),
             "pass" if (checks.get(k) or {}).get("passed") else "not passed") for k in GAS_DROP_CHECKS]
    claim, acc = summary.get("claim", {}), summary.get("accounting", {})
    tag, dev = summary.get("tag"), summary.get("session", {}).get("sequencer_dev_mode")
    failed = [k for k in GAS_DROP_CHECKS if not (checks.get(k) or {}).get("passed")]
    real = bool(claim.get("real_proof") and claim.get("status") == "applied" and claim.get("bedrock_status") == "Finalized")
    if tag == newest and dev is False and not failed and summary.get("passed") and real:
        return ("GO", f"newest tag {newest}: a private claim (ClaimPrivately) by a claimant with no native LEZ swept the "
                f"relayer's sealed credit of {acc.get('credit')} into its one-time account and paid its fee "
                f"({acc.get('feeCharged')}) from it in the same transaction; proven for real in "
                f"{fmt_s(claim['prove_and_submit_s']) if claim.get('prove_and_submit_s') is not None else '?'}, applied in "
                f"block {claim.get('block')} (final on Bedrock); the same claim without the credit refused by the wallet",
                rows)
    why = (f"the record is of {tag}, not {newest}" if tag != newest else
           "its sequencer is not shown without RISC0_DEV_MODE" if dev is not False else
           "not passed: " + ", ".join(failed) if failed else "the claim is not shown applied with a real proof in a final block")
    return ("NO-GO (measured: the sweep did not pass)", f"newest tag {newest}: {why}", rows + [GAS_DROP_OTHERWISE])


def check_gas_drop():
    """Check 2: lez/results/<newest tag>/gas-drop/summary.json, written from that tag's session records; without one,
    what is built of the gas drop (its receipt and the relayer's side)."""
    newest, source = FACTS["lez_newest_tag"]
    path = os.path.join(ROOT, "lez", "results", newest, "gas-drop", "summary.json")
    if os.path.exists(path):
        verdict, value, rows = gas_drop_verdict(load(path), newest)
        return dict(verdict=verdict, value=value, rows=rows,
                    sources=[source, rel(path), rel(os.path.join(os.path.dirname(path), "README.md"))])
    hits = []
    for pat in ("lez/**/*.rs", "bridge/**/*.rs", "cli/**/*.rs", "e2e/**/*.py"):
        for f in glob.glob(os.path.join(ROOT, pat), recursive=True):
            if "/target/" in f:
                continue
            try:
                text = open(f, errors="replace").read()
            except OSError:
                continue
            for m in re.finditer(r"fn\s+(\w*gas_drop\w*)\s*\(", text):
                hits.append(f"{rel(f)}::{m.group(1)}")
    if not hits:
        return dict(verdict="NO-GO (not built)",
                    value="no test of the gas drop in the repository (specified in the proposal §B2, not built)",
                    rows=[GAS_DROP_OTHERWISE], sources=["grep: fn *gas_drop* in lez/, bridge/, cli/, e2e/"])
    return dict(verdict=f"OPEN (not measured on {newest})",
                value=f"the gas drop's receipt binding (N, oneTimeAddress, relayer) and the relayer's side are built and tested "
                      f"({len(hits)} named function(s): " + ", ".join(hits[:6]) + f"); no gas-drop record for {newest} "
                      f"({rel(path)})",
                rows=[GAS_DROP_OTHERWISE], sources=hits[:6] + [rel(path)])


def ls_remote(url, pattern):
    try:
        out = subprocess.run(["git", "ls-remote", "--tags", "--refs", url], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, str(e)
    if out.returncode:
        return None, out.stderr.strip()[:200]
    tags = [l.split("refs/tags/")[1] for l in out.stdout.splitlines() if "refs/tags/" in l]
    tags = [t for t in tags if re.match(pattern, t)]

    def key(t):
        nums = [int(x) for x in re.findall(r"\d+", t)]
        return nums + [0] * (6 - len(nums)) + [0 if "rc" in t else 1]

    return sorted(tags, key=key), None


def check_token_program(online):
    value, source = FACTS["token_program_v04"]
    rows = []
    if online:
        tags, err = ls_remote(TOKEN_REPO, r".*")
        rows.append(("lez-programs tags (online)", err or (", ".join(tags[-5:]) if tags else "no tags"), ""))
    return dict(verdict="OPEN (not pinned upstream; if still open at M0, Logos decides: proposal §B9)", value=value,
                rows=rows + [("otherwise", "balances stay in the bridge's own accounts, moved only by the bridge (its Transfer included); Logos chooses", "")],
                sources=[source])


REAL_PROOF_ITEMS = ("update", "attestation", "claim", "burn")


def real_proofs_verdict(summary, newest):
    """Check 4 from a real-proofs summary (lez/scripts/real-proofs-summary.py --write): GO only when it is of the newest
    tag, its sessions' sequencers ran without RISC0_DEV_MODE, and each of the private update, attestation, claim and
    burn was applied with a real proof in a block final on Bedrock. Returns (verdict, value, rows)."""
    rows = []
    items = {i["key"]: i for i in summary.get("items", [])}
    for key in REAL_PROOF_ITEMS:
        i = items.get(key)
        if not i or not i.get("found"):
            rows.append((key, f"no record of it in {i['manifest'] if i else 'the summary'}", "not passed"))
            continue
        final = i.get("final_after_s")
        rows.append((i["what"], f"{i['status']} in block {i['block']} ({i.get('bedrock_status')}"
                     + (f", final {final:,.0f} s after it was published" if final is not None else "") + "); proven and sent in "
                     + (fmt_s(i["prove_and_submit_s"]) if i.get("prove_and_submit_s") is not None else "?")
                     + f"; transaction {i.get('tx_bytes') or 0:,} B, receipt {i.get('receipt_bytes') or 0:,} B",
                     "pass" if i.get("passed") else "not passed"))
    sessions = summary.get("sessions", {})
    verifying = bool(sessions) and all(v.get("sequencer_dev_mode") is False for v in sessions.values())
    rows.append(("the sequencer", "RISC0_DEV_MODE absent from its environment in every session: it verifies the receipts"
                 if verifying else "RISC0_DEV_MODE not shown absent in every session", "" if verifying else "not passed"))
    fake = summary.get("fake_receipt")
    if fake:
        rows.append(("a fake receipt of the private update", f"{fake['status']} by the sequencer", ""))
    passed = [k for k in REAL_PROOF_ITEMS if items.get(k, {}).get("passed")]
    tag = summary.get("tag")
    if tag == newest and verifying and len(passed) == len(REAL_PROOF_ITEMS):
        return ("GO", f"newest tag {newest}: the private update, attestation, claim and burn proven for real and applied by "
                f"its sequencer in final blocks ({len(passed)} of {len(REAL_PROOF_ITEMS)})", rows)
    missing = [k for k in REAL_PROOF_ITEMS if k not in passed]
    why = f"the record is of {tag}" if tag != newest else ("not yet: " + ", ".join(missing) if missing else "the sequencer")
    return ("NO-GO (open on the newest tag)",
            f"newest tag {newest}: {len(passed)} of {len(REAL_PROOF_ITEMS)} real proofs pass; {why}", rows)


def check_real_proofs(online):
    """Check 4: lez/results/<newest tag>/real-proofs/summary.json, written from that tag's session records."""
    newest, source = FACTS["lez_newest_tag"]
    rows = []
    if online:
        tags, err = ls_remote(LEZ_REPO, r"^v\d")
        if tags:
            newest = tags[-1]
            rows.append(("newest LEZ tag (online)", newest, "" if newest == FACTS["lez_newest_tag"][0] else "NEW: re-run"))
        else:
            rows.append(("newest LEZ tag (online)", f"not read: {err}", ""))
    path = os.path.join(ROOT, "lez", "results", newest, "real-proofs", "summary.json")
    otherwise = ("otherwise", "the tag's change is traced and fixed in A1", "")
    if not os.path.exists(path):
        return dict(verdict="NO-GO (open on the newest tag)",
                    value=f"newest tag {newest}: no real-proof record for it ({rel(path)})",
                    rows=rows + [otherwise], sources=[source, rel(path)])
    verdict, value, measured = real_proofs_verdict(load(path), newest)
    return dict(verdict=verdict, value=value, rows=rows + measured + ([] if verdict == "GO" else [otherwise]),
                sources=[source, rel(path), rel(os.path.join(os.path.dirname(path), "README.md"))])


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--online", action="store_true", help="re-read upstream tags from GitHub (read-only)")
    ap.add_argument("--json", help="also write the checks as JSON to this file")
    ap.add_argument("--strict", action="store_true", help="exit 1 unless every check is GO")
    ap.add_argument("--p4", default=os.path.join(ROOT, "lez/results/p4-minimum"), help="user-proving summaries")
    a = ap.parse_args()
    checks = [
        ("1", "A full LEZ private claim and a private attestation, padded as LEZ's wallet pads, and a release receipt "
              "on the declared minimum (8 cores, 16 threads, 16 GB), Linux and macOS",
         "<= 20 min and <= 16 GB each", check_user_proving(a.p4)),
        ("2", "The gas drop: a private claim that sweeps a sealed native credit and pays its fee from it", "passes",
         check_gas_drop()),
        ("3", "Token program on v0.4: immutable; the bridge's mint authority; burns only through synchronous messages; "
              "ATA-compatible accounts", "pinned", check_token_program(a.online)),
        ("4", "Real proofs on the newest LEZ tag of the private update, attestation, claim and burn, settled by its "
              "sequencer", "pass", check_real_proofs(a.online)),
    ]
    print("M0: open checks (proposal, milestone M0). Values from the files named; GO / NO-GO / OPEN.\n")
    for n, what, go, c in checks:
        print(f"[{c['verdict']}] M0.{n} {what}")
        print(f"    GO when: {go}")
        print(f"    measured: {c['value']}")
        for label, val, v in c["rows"]:
            print(f"      - {label}: {val}" + (f"  [{v}]" if v else ""))
        print(f"    sources: {'; '.join(c['sources'])}\n")
    print("Also in M0 (answers and parameters from Logos, not measurable here): OPEN")
    print(f"  - Logos's answer on ferrata_attestor by day 5: {FACTS['ferrata_attestor_answer'][0]}")
    print("  - testnet 0.4's k, slot time, turn length, committee size, threshold, minimum stake, posting timeframe")
    print("  - a notice period and burn freeze for breaking LEZ changes; a Logos LEZ engineer's check of the finality "
          "conditions; audit quotes requested")
    go = sum(1 for *_, c in checks if c["verdict"].startswith("GO"))
    print(f"\nSummary: {go} of {len(checks)} checks GO.")
    if a.json:
        json.dump([dict(check=n, what=w, go_when=g, **c) for n, w, g, c in checks], open(a.json, "w"), indent=1)
    if a.strict and go != len(checks):
        sys.exit(1)


if __name__ == "__main__":
    main()
