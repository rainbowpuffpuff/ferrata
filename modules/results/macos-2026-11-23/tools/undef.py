#!/usr/bin/env python3
"""External C symbols (not Rust, not defined by any rlib in DEPS) that the darwin rlibs in DEPS reference, per crate.

    undef.py DEPS LLVM_NM LLVM_AR OUT
"""
import collections, glob, os, re, subprocess, sys
deps, nm, ar, out = sys.argv[1:5]
os.makedirs(out, exist_ok=True)
defined, undef = set(), collections.defaultdict(set)
for rlib in sorted(glob.glob(os.path.join(deps, "*.rlib"))):
    crate = re.sub(r"^lib(.*)-[0-9a-f]+\.rlib$", r"\1", os.path.basename(rlib))
    w = os.path.join(out, crate); os.makedirs(w, exist_ok=True)
    names = [n for n in subprocess.run([ar, "t", rlib], capture_output=True, text=True).stdout.split() if n.endswith(".o")]
    if not names: continue
    subprocess.run([ar, "x", rlib, *names], cwd=w)
    for n in names:
        p = os.path.join(w, n)
        defined.update(subprocess.run([nm, "--defined-only", "--extern-only", "--format=just-symbols", p], capture_output=True, text=True).stdout.split())
        for s in subprocess.run([nm, "--undefined-only", "--format=just-symbols", p], capture_output=True, text=True).stdout.split():
            undef[crate].add(s)
for crate in sorted(undef):
    c = sorted(s for s in undef[crate] - defined if not re.match(r"^_+R", s) and not s.startswith("__ZN"))
    if c: print(f"{crate}: {' '.join(c)}")
