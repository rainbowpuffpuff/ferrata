#!/usr/bin/env python3
"""Every guest risc0-build built in a tree: workspace, name, image id (risc0-build's own, from the methods crate's
methods.rs), sha256 and size of the binary it embeds (R0BF), sha256 of the user ELF beside it, and r0vm's id of the
binary as a cross-check. Used by scripts/reproduce-image-ids.sh build.

    python3 -I scripts/reproduce/guest_ids.py TREE OUT.tsv
"""
import hashlib
import re
import subprocess
import sys
from pathlib import Path


def main() -> int:
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    rows, seen = [], set()
    for ws in ("lez", "checkpoint", "bridge"):
        for m in sorted((root / ws / "target").glob("**/build/methods*-*/out/methods.rs")):
            src = m.read_text()
            for name, path in re.findall(r'pub const ([A-Z0-9_]+)_PATH: &str = "([^"]+)";', src):
                mid = re.search(rf"pub const {name}_ID: \[u32; 8\] = \[([^\]]+)\];", src)
                f = Path(path)
                if not mid or not f.exists() or (ws, name) in seen:
                    continue
                seen.add((ws, name))
                words = [int(w.strip()) for w in mid.group(1).split(",") if w.strip()]
                idhex = "".join(w.to_bytes(4, "little").hex() for w in words)
                data = f.read_bytes()
                elf = f.with_suffix("") if f.suffix == ".bin" else f
                elf_sha = hashlib.sha256(elf.read_bytes()).hexdigest() if elf.exists() and elf != f else "-"
                r = subprocess.run(["r0vm", "--elf", str(f), "--id"], capture_output=True, text=True)
                r0 = (r.stdout.strip().splitlines() or ["?"])[-1]
                rows.append((ws, name.lower(), "0x" + idhex, hashlib.sha256(data).hexdigest(), str(len(data)), elf_sha,
                             "= id" if r0 == idhex else r0))
    rows.sort()
    out.write_text("workspace\tname\timage_id\tsha256_bin\tbytes_bin\tsha256_user_elf\tr0vm_id\n"
                   + "".join("\t".join(r) + "\n" for r in rows))
    print(out.read_text(), end="")
    return 0 if rows else 1


if __name__ == "__main__":
    sys.exit(main())
