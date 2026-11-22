#!/usr/bin/env python3
"""Every guest risc0-build built in a tree: workspace, name, image id (risc0-build's own, from the methods crate's
methods.rs), sha256 and size of the binary it embeds (R0BF), sha256 of the user ELF beside it, and r0vm's id of the
binary as a cross-check. Used by scripts/reproduce-image-ids.sh build.

    python3 -I scripts/reproduce/guest_ids.py TREE OUT.tsv

A target directory can hold more than one methods.rs for a guest (a toolchain or lockfile change leaves the old
`methods-<hash>` directory behind). All of them are read: when they give the same id the newest is used; when they
disagree the guest is reported as a conflict, left out of OUT.tsv, and the exit status is 1 (rebuild in a fresh
target directory). The r0vm column is "= id" when r0vm computes risc0-build's id from the binary, r0vm's own id when
it differs, and "?" when r0vm is not installed or fails; scripts/reproduce/compare_ids.py counts anything but "= id"
as a difference.
"""
import hashlib
import re
import subprocess
import sys
from pathlib import Path

WORKSPACES = ("lez", "checkpoint", "bridge")


def candidates(root: Path) -> dict:
    """(workspace, NAME) -> [(methods.rs, id hex, binary path)], every methods.rs under the workspaces' target dirs."""
    out = {}
    for ws in WORKSPACES:
        for m in sorted((root / ws / "target").glob("**/build/methods*-*/out/methods.rs")):
            src = m.read_text()
            for name, path in re.findall(r'pub const ([A-Z0-9_]+)_PATH: &str = "([^"]+)";', src):
                mid = re.search(rf"pub const {name}_ID: \[u32; 8\] = \[([^\]]+)\];", src)
                if not mid:
                    continue
                words = [int(w.strip()) for w in mid.group(1).split(",") if w.strip()]
                idhex = "".join(w.to_bytes(4, "little").hex() for w in words)
                out.setdefault((ws, name), []).append((m, idhex, Path(path)))
    return out


def r0vm_id(binary: Path) -> str:
    try:
        r = subprocess.run(["r0vm", "--elf", str(binary), "--id"], capture_output=True, text=True)
    except OSError:
        return "?"
    lines = r.stdout.strip().splitlines()
    return lines[-1].strip() if r.returncode == 0 and lines else "?"


def main() -> int:
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    rows, conflicts = [], []
    for (ws, name), cands in sorted(candidates(root).items()):
        present = [c for c in cands if c[2].exists()]
        if not present:
            continue
        ids = sorted({c[1] for c in present})
        if len(ids) > 1:
            conflicts.append((ws, name, present))
            continue
        m, idhex, f = max(present, key=lambda c: c[0].stat().st_mtime)
        data = f.read_bytes()
        elf = f.with_suffix("") if f.suffix == ".bin" else f
        elf_sha = hashlib.sha256(elf.read_bytes()).hexdigest() if elf.exists() and elf != f else "-"
        r0 = r0vm_id(f)
        rows.append((ws, name.lower(), "0x" + idhex, hashlib.sha256(data).hexdigest(), str(len(data)), elf_sha,
                     "= id" if r0.removeprefix("0x") == idhex else r0))
    rows.sort()
    out.write_text("workspace\tname\timage_id\tsha256_bin\tbytes_bin\tsha256_user_elf\tr0vm_id\n"
                   + "".join("\t".join(r) + "\n" for r in rows))
    print(out.read_text(), end="")
    for ws, name, present in conflicts:
        print(f"CONFLICT {ws} {name.lower()}: {len(present)} methods.rs give different image ids (stale build output; "
              "rebuild in a fresh target directory):", file=sys.stderr)
        for m, idhex, _ in sorted(present, key=lambda c: c[0].stat().st_mtime):
            print(f"  0x{idhex}  {m.relative_to(root)}", file=sys.stderr)
    return 0 if rows and not conflicts else 1


if __name__ == "__main__":
    sys.exit(main())
