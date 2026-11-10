#!/usr/bin/env python3
"""Derived figures from the measured runs (results/bench.jsonl, results/micro-summary.jsonl,
results/prove.jsonl). Every input number is a measured one; every output shows its formula.

usage: python3 -I scripts/derive.py [results-dir]
"""
import json
import pathlib
import sys

res = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).parent.parent / "results")


def lines(name):
    p = res / name
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []


bench = {r["name"]: r for r in lines("bench.jsonl")}
micro = lines("micro-summary.jsonl")
micro = micro[-1] if micro else {}
prove = lines("prove.jsonl")


def c(name):
    return bench[name]["user_cycles"]


def b(name, key="block_bytes"):
    return bench[name][key]


out = []
p = out.append
p("## Derived (scripts/derive.py)")
p("")
e1, e60, e360 = c("cp-empty-1"), c("cp-empty-60"), c("cp-empty-360")
per_empty = (e360 - e60) / 300
p(f"- marginal cost per empty block: (cp-empty-360 - cp-empty-60) / 300 = ({e360:,} - {e60:,}) / 300 = **{per_empty:,.0f} cycles**")
per_empty_b = (e360 - e1) / 359
p(f"  (same from cp-empty-1: ({e360:,} - {e1:,}) / 359 = {per_empty_b:,.0f})")
fixed = e1 - per_empty
p(f"- fixed cost of a checkpoint over empty blocks (T's inscription, 3 support inscriptions, state, journal): cp-empty-1 - one block = {fixed:,.0f} cycles")
idle_hour = 360 * per_empty
p(f"- one idle hour (360 empty blocks): 360 x {per_empty:,.0f} = **{idle_hour:,.0f} cycles**")
if "cp-pp50-360" in bench:
    pp50, pp5 = c("cp-pp50-360"), c("cp-pp5-360")
    db50 = b("cp-pp50-360") - b("cp-empty-360")
    per_kb = (pp50 - e360) / (db50 / 1000)
    p(f"- marginal cost per KB of block data: (cp-pp50-360 - cp-empty-360) / extra KB = ({pp50:,} - {e360:,}) / {db50/1000:,.1f} KB = **{per_kb:,.0f} cycles/KB**")
    per_pp = (pp50 - e360) / 50
    p(f"- per privacy-preserving transaction (in a block that is not T or support): ({pp50:,} - {e360:,}) / 50 = {per_pp:,.0f} cycles")
    busy_hour = pp50 - fixed
    p(f"- one busy hour (50 PP txs) beyond the fixed part: {pp50:,} - {fixed:,.0f} = **{busy_hour:,.0f} cycles**")
    for budget in (50e6, 200e6):
        p(f"- hours that fit in {budget/1e6:.0f}M cycles with empty T and support blocks: idle ({budget/1e6:.0f}M - {fixed/1e6:.2f}M) / {idle_hour/1e6:.3f}M = **{(budget - fixed) / idle_hour:,.1f} h**; busy ({budget/1e6:.0f}M - {fixed/1e6:.2f}M) / {busy_hour/1e6:.2f}M = **{(budget - fixed) / busy_hour:,.1f} h**")
if "cp-busytip-t3" in bench:
    bt = c("cp-busytip-t3")
    p(f"- T and the 3 support blocks each carrying 4 PP txs (cp-busytip-t3): **{bt:,} cycles**, i.e. {bt/1e6:.0f}M before a single block of history")
if "seg-empty-180" in bench:
    s = c("seg-empty-180")
    p(f"- backlog: a segment over 180 empty blocks costs {s:,} cycles = {2*s:,} per idle hour as segments (vs {idle_hour:,.0f} inside a checkpoint)")
for t in (2, 3, 5):
    n = f"cp-support-t{t}"
    if n in bench:
        p(f"- support check, t = {t}: {n} = {c(n):,} cycles ({t} support + T's inscription, 1+{t} empty blocks)")
if micro:
    p(f"- components (micro): SHA-256 {micro['sha256_per_byte']:.2f} cycles/B, BLAKE2b {micro['blake2b_per_byte']:.1f} cycles/B, Ed25519 {micro['ed25519_each']:,.0f} each, burn {micro['burn_append_each']:,.0f} each, input read {micro['read_per_byte']:.3f} cycles/B, empty block parse {micro['empty_block_parse']:,.0f} + hash {micro['empty_block_hash']:,.0f}")
if prove:
    pr = prove[-1]
    per_seg = pr["wall_s"] / pr["segments"]
    p(f"- proving on this machine (load {pr['loadavg'].split()[0]} at the end): {pr['segments']} segments in {pr['wall_s']:.1f} s = {per_seg:.1f} s per segment (succinct, incl. recursion)")
    if "cp-empty-360" in bench:
        for name in ("cp-empty-360", "cp-pp50-360", "cp-busytip-t3"):
            if name in bench:
                segs = bench[name]["segments"]
                p(f"  - extrapolated: {name} has {segs} segments -> about {segs * per_seg / 60:,.1f} min succinct at that rate (EXTRAPOLATION)")
print("\n".join(out))
