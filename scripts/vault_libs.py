#!/usr/bin/env python3
"""The vault's Solidity libraries pinned by content, not only by tag (review-code-ops L6).

    python3 -I scripts/vault_libs.py install [--lib DIR]   forge install --no-git the pinned tags (if absent), then check
    python3 -I scripts/vault_libs.py check   [--lib DIR]   every library's tree hash equals vault/lib.lock's
    python3 -I scripts/vault_libs.py hash    [--lib DIR]   print the tree hashes (to write a new vault/lib.lock)

`forge install --no-git` takes a tag and keeps no commit, and the release's vault bytecode, the Groth16 verifier and
the emergency stop that scripts/deploy-testnet.sh deploys are compiled from these trees: a moved tag would change them
silently. A library's tree hash is the sha256 of its files' sorted `sha256  path` lines (links as `link path -> target`;
.git entries left out). DIR defaults to vault/lib. Standard library only (install needs forge).
"""
import argparse
import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOCK = os.path.join(ROOT, "vault", "lib.lock")


def read_lock(path=LOCK):
    """[(directory, forge install spec, tree hash)] from vault/lib.lock."""
    out = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if line:
                d, spec, h = line.split()
                out.append((d, spec, h))
    return out


def tree_hash(top):
    lines = []
    for dp, dns, fns in os.walk(top):
        dns[:] = sorted(d for d in dns if d != ".git")
        for n in sorted(fns):
            if n == ".git":
                continue
            p = os.path.join(dp, n)
            rel = os.path.relpath(p, top).replace(os.sep, "/")
            if os.path.islink(p):
                lines.append(f"link {rel} -> {os.readlink(p)}")
            else:
                h = hashlib.sha256()
                with open(p, "rb") as f:
                    for chunk in iter(lambda: f.read(1 << 20), b""):
                        h.update(chunk)
                lines.append(f"{h.hexdigest()}  {rel}")
    lines.sort()
    return hashlib.sha256(("\n".join(lines) + "\n").encode()).hexdigest(), len(lines)


def check(lib, lock):
    bad = 0
    for d, spec, want in lock:
        p = os.path.join(lib, d)
        if not os.path.isdir(p):
            print(f"vault_libs: {d} missing ({spec}): python3 -I scripts/vault_libs.py install")
            bad += 1
            continue
        got, n = tree_hash(p)
        if got != want:
            print(f"vault_libs: {d} ({spec}): tree hash {got} over {n} files, vault/lib.lock says {want}")
            bad += 1
        else:
            print(f"vault_libs: {d} ({spec}): {n} files, tree hash {got}: as pinned")
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("cmd", choices=("install", "check", "hash"))
    ap.add_argument("--lib", default=os.path.join(ROOT, "vault", "lib"))
    ap.add_argument("--lock", default=LOCK)
    a = ap.parse_args()
    lock = read_lock(a.lock)
    if a.cmd == "hash":
        for d, spec, _ in lock:
            h, n = tree_hash(os.path.join(a.lib, d))
            print(f"{d:<24} {spec:<44} {h}  # {n} files")
        return 0
    if a.cmd == "install":
        missing = [spec for d, spec, _ in lock if not os.path.isdir(os.path.join(a.lib, d))]
        if missing:
            r = subprocess.run(["forge", "install", "--no-git", *missing], cwd=os.path.dirname(os.path.abspath(a.lib)))
            if r.returncode != 0:
                return r.returncode
    return 1 if check(a.lib, lock) else 0


if __name__ == "__main__":
    sys.exit(main())
