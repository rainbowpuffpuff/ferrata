#!/usr/bin/env python3
"""Compare the lez/ guests of two builds made against two LEZ tags, and say what differs.

    python3 -I lez/scripts/compare-guest-builds.py --a ROOT_A --b ROOT_B --tag-a v0.4.0-rc1 --tag-b v0.4.0-rc3 \
        --table docs/image-ids.md --scratch DIR > image-ids.md

ROOT_A and ROOT_B are directories whose `lez/target` is a cargo target directory in which `cargo build -p methods`
(lez/) ran against the tag named, through scripts/guest-rustc (so the only build path left in a guest is the LEZ
source tree's, remapped to `/upstream/lez-<tag>/`). For every guest of both builds: the image id of each (risc0-build's,
from methods.rs), the id recorded in docs/image-ids.md (the final programs), the user ELFs' sizes and the bytes that
differ, how many embedded LEZ source paths each holds, and the test: B with every `/upstream/lez-<tag-b>/` written as
`/upstream/lez-<tag-a>/` (same length), compared byte for byte with A's user ELF, and the image id r0vm gives the
R0BF binary of B so rewritten. If that id is A's, the two builds differ only in the tag inside the LEZ source paths
the guest embeds (panic locations): the code and data are the same. Rewritten binaries go to DIR. Standard library
and r0vm only.
"""
import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path


def guests(root: Path) -> dict:
    out, clash = {}, []
    for m in sorted((root / "lez" / "target").glob("**/build/methods*-*/out/methods.rs")):
        src = m.read_text()
        for name, path in re.findall(r'pub const ([A-Z0-9_]+)_PATH: &str = "([^"]+)";', src):
            mid = re.search(rf"pub const {name}_ID: \[u32; 8\] = \[([^\]]+)\];", src)
            f = Path(path)
            if not mid or not f.exists():
                continue
            words = [int(w.strip()) for w in mid.group(1).split(",") if w.strip()]
            gid = "0x" + "".join(w.to_bytes(4, "little").hex() for w in words)
            key = name.lower()
            if key in out and out[key][0] != gid:
                clash.append(key)
            out.setdefault(key, (gid, f))
    if clash:
        sys.exit(f"{root}: two different builds of {', '.join(sorted(set(clash)))} in one target directory")
    return out


def recorded(table: Path) -> dict:
    rows = {}
    pat = re.compile(r"`([a-z0-9_]+)` \| `(0x[0-9a-f]{64})` \| `([0-9a-f]{64})` \| ([0-9,]+) \| `([0-9a-f]{64})` \|")
    for line in table.read_text().splitlines():
        m = pat.search(line)
        if m:
            rows[m.group(1)] = {"id": m.group(2), "bin_sha": m.group(3), "elf_sha": m.group(5)}
    return rows


def r0vm_id(path: Path) -> str:
    r = subprocess.run(["r0vm", "--elf", str(path), "--id"], capture_output=True, text=True)
    lines = r.stdout.strip().splitlines()
    return "0x" + lines[-1] if r.returncode == 0 and lines else f"r0vm failed: {r.stderr.strip()[:80]}"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    for k in ("--a", "--b", "--tag-a", "--tag-b", "--table", "--scratch"):
        ap.add_argument(k, required=True)
    ap.add_argument("--label-b", help="what to call build B (default: the tag-b build)")
    a = ap.parse_args()
    lb = a.label_b or f"{a.tag_b} build"
    ga, gb, rec = guests(Path(a.a)), guests(Path(a.b)), recorded(Path(a.table))
    pa, pb = f"/upstream/lez-{a.tag_a}/".encode(), f"/upstream/lez-{a.tag_b}/".encode()
    if len(pa) != len(pb):
        sys.exit("the two tags differ in length: a byte-for-byte rewrite is not possible")
    scratch = Path(a.scratch)
    scratch.mkdir(parents=True, exist_ok=True)
    print(f"# The lez/ guests built against LEZ {a.tag_a} and {a.tag_b}\n")
    print(f"Both builds on the same box, from the same tree, through `scripts/guest-rustc` (paths remapped: the LEZ "
          f"sources appear in a guest as `/upstream/lez-<tag>/...`). \"final\": the id recorded in `docs/image-ids.md`. "
          f"\"rewritten\": the {a.tag_b} build with every `/upstream/lez-{a.tag_b}/` replaced by "
          f"`/upstream/lez-{a.tag_a}/` (the same length), then compared with the {a.tag_a} build; its image id by "
          f"`r0vm --elf <R0BF> --id`. Made by `lez/scripts/compare-guest-builds.py`.\n")
    print(f"| guest | final id | {a.tag_a} build | {lb} | user ELF bytes ({a.tag_a} / {lb}) "
          f"| bytes that differ | LEZ paths embedded ({a.tag_a} / {lb}) | rewritten user ELF = {a.tag_a}'s "
          f"| rewritten image id |")
    print("|---|---|---|---|---|---:|---|---|---|")
    verdicts = []
    for name in sorted(set(ga) | set(gb)):
        if name not in ga or name not in gb:
            print(f"| `{name}` | | {'yes' if name in ga else 'missing'} | {'yes' if name in gb else 'missing'} "
                  f"| | | | | |")
            verdicts.append(False)
            continue
        (ida, bina), (idb, binb) = ga[name], gb[name]
        elfa, elfb = bina.with_suffix(""), binb.with_suffix("")
        ea, eb = elfa.read_bytes(), elfb.read_bytes()
        diff = sum(x != y for x, y in zip(ea, eb)) + abs(len(ea) - len(eb))
        rew = eb.replace(pb, pa)
        same_elf = rew == ea
        rbin = binb.read_bytes().replace(pb, pa)
        out = scratch / f"{name}.rewritten.bin"
        out.write_bytes(rbin)
        rid = r0vm_id(out)
        final = rec.get(name, {}).get("id", "not recorded")

        def short(x: str) -> str:
            return f"`{x[:10]}…`" if x.startswith("0x") else x

        print(f"| `{name}` | {short(final)} | {short(ida)}{' = final' if ida == final else ''} | {short(idb)}"
              f"{' = final' if idb == final else ''} | {len(ea):,} / {len(eb):,} | {diff:,} | {ea.count(pa)} / "
              f"{eb.count(pb)} | {'yes' if same_elf else 'NO'} | {short(rid)}{' = ' + a.tag_a if rid == ida else ''} |")
        verdicts.append(ida == final and same_elf and rid == ida)
        print(f"{name}\t{ida}\t{idb}\t{rid}\t{sha(ea)}\t{sha(eb)}\t{sha(rew)}", file=sys.stderr)
    n = len(verdicts)
    print(f"\n{sum(verdicts)} of {n} guests: the {a.tag_a} build has the final id, and the {lb} differs "
          f"from it only in the tag inside the embedded LEZ source paths (rewritten, it is byte for byte the "
          f"{a.tag_a} build and has its image id).")
    return 0 if n and all(verdicts) else 1


if __name__ == "__main__":
    sys.exit(main())
