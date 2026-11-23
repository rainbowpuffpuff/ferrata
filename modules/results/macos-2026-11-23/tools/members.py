#!/usr/bin/env python3
"""Map risc0-zkp's object files (one per codegen unit) for aarch64-apple-darwin: which of them need Metal or the
Objective-C runtime at link time (directly, or through another member of the same archive they reference), and what
those members define. A Rust staticlib carries these objects as archive members; ld64 loads a member whole when it
resolves an undefined symbol, and then needs every symbol that member references.

    members.py RLIB LLVM_NM LLVM_AR OUTDIR
"""
import collections
import json
import os
import re
import subprocess
import sys

rlib, nm, ar, out = sys.argv[1:5]
os.makedirs(out, exist_ok=True)
work = os.path.join(out, "members")
os.makedirs(work, exist_ok=True)
names = [n for n in subprocess.run([ar, "t", rlib], capture_output=True, text=True, check=True).stdout.split()
         if n.endswith(".o")]
subprocess.run([ar, "x", rlib, *names], cwd=work, check=True)


def demangle_legacy(sym):
    """The item path of a Rust symbol, legacy (_ZN...E) or v0 (_R...) mangling; other symbols unchanged. For v0 the
    path is read off its length-prefixed identifiers, which is enough to tell modules apart."""
    s = sym.lstrip("_")
    if s.startswith("R"):
        parts = []
        for m in re.finditer(r"(\d+)_?", s):
            n, st = int(m.group(1)), m.end()
            ident = s[st:st + n]
            if n and len(ident) == n and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", ident):
                parts.append(ident)
        if "risc0_zkp" in parts:
            parts = parts[parts.index("risc0_zkp"):]
        return "::".join(parts) or sym
    if not s.startswith("ZN"):
        return sym
    i, parts = 2, []
    while i < len(s) and s[i].isdigit():
        j = i
        while s[j].isdigit():
            j += 1
        n = int(s[i:j])
        parts.append(s[j:j + n])
        i = j + n
    if parts and re.fullmatch(r"h[0-9a-f]{16}", parts[-1]):
        parts = parts[:-1]
    return "::".join(parts)


def syms(path, flag):
    r = subprocess.run([nm, flag, "--format=just-symbols", path], capture_output=True, text=True, check=True)
    return [l.strip() for l in r.stdout.splitlines() if l.strip()]


defined, undefined = {}, {}
for n in names:
    p = os.path.join(work, n)
    defined[n] = set(syms(p, "--defined-only"))
    undefined[n] = set(syms(p, "--undefined-only"))
owner = {}
for n, ds in defined.items():
    for d in ds:
        owner.setdefault(d, n)

FOREIGN = re.compile(r"(^|::)(metal|objc|block|foreign_types|core_graphics_types|malloc_buf)::")
RAW = re.compile(r"^_+(objc_|sel_|class_|object_|MTL|OBJC_|NSConcrete|Block_)")


def foreign(sym):
    return bool(RAW.match(sym)) or bool(FOREIGN.match(demangle_legacy(sym)))


direct = {n: sorted(u for u in undefined[n] if foreign(u)) for n in names}
edges = {n: {owner[u] for u in undefined[n] if u in owner and owner[u] != n} for n in names}
tainted = {n for n in names if direct[n]}
changed = True
while changed:
    changed = False
    for n in names:
        if n not in tainted and edges[n] & tainted:
            tainted.add(n)
            changed = True


def module(sym, depth=3):
    d = demangle_legacy(sym)
    d = re.sub(r"<[^<>]*>", "", d)
    return "::".join(d.split("::")[:depth]) if "::" in d else d


report = {"rlib": os.path.basename(rlib), "members": len(names), "tainted": [], "untainted": []}
for n in sorted(names):
    mods = collections.Counter(module(d) for d in defined[n])
    entry = {"member": n, "defines": dict(mods.most_common()), "direct_foreign": direct[n][:12],
             "n_direct_foreign": len(direct[n]), "refs_members": sorted(edges[n])}
    (report["tainted"] if n in tainted else report["untainted"]).append(entry)
with open(os.path.join(out, "members.json"), "w") as f:
    json.dump(report, f, indent=1, sort_keys=True)

# What the relayer reaches without the prover: risc0-groth16's to_json (core::digest, core::hash::poseidon_254)
# and the verifier side of risc0-zkvm and the circuits (core, verify, adapter, taps, layout, merkle).
REACHED = re.compile(r"^risc0_zkp::(core|verify|adapter|taps|layout|merkle)\b")
print(f"members: {len(names)}; needing Metal/objc (directly or through a member they reference): {len(tainted)}")
for e in report["tainted"]:
    reached = sorted(m for m in e["defines"] if REACHED.match(m))
    print(f"  {e['member']}: {e['n_direct_foreign']} foreign refs; defines {sum(e['defines'].values())} symbols in "
          f"{sorted(e['defines'])[:8]}{' ...' if len(e['defines']) > 8 else ''}; non-prover modules: {reached or 'none'}")
for e in report["untainted"]:
    print(f"  clean {e['member']}: {sorted(e['defines'])[:6]}{' ...' if len(e['defines']) > 6 else ''}")
hits = [e["member"] for e in report["tainted"] if any(REACHED.match(m) for m in e["defines"])]
print("VERDICT:", "a member the relayer's non-prover code reaches also needs Metal/objc: " + ", ".join(hits)
      if hits else "no member defining non-prover code (core, verify, adapter, taps, layout, merkle) needs Metal/objc")
