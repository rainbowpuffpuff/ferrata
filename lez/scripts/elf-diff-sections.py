#!/usr/bin/env python3
"""Where two RISC Zero guest ELFs (32-bit, little-endian) differ: the differing bytes per section, and the printable
strings that occur in one ELF's read-only data and not the other's.

    python3 -I lez/scripts/elf-diff-sections.py A.elf B.elf

Standard library only. Used for lez/results/v0.4.0-rc3/image-ids-rc3-as-rc1.md.
"""
import re
import struct
import sys
from collections import Counter


def sections(data: bytes) -> list:
    shoff, = struct.unpack_from("<I", data, 32)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", data, 46)
    hdrs = [struct.unpack_from("<10I", data, shoff + i * shentsize) for i in range(shnum)]
    strtab = hdrs[shstrndx]
    names = data[strtab[4]:strtab[4] + strtab[5]]
    out = []
    for h in hdrs:
        name = names[h[0]:names.index(b"\0", h[0])].decode()
        if h[1] != 8:  # not NOBITS
            out.append((name, h[4], h[5]))  # name, file offset, size
    return out


def strings(blob: bytes) -> Counter:
    return Counter(m.group().decode() for m in re.finditer(rb"[\x20-\x7e]{6,}", blob))


def main() -> int:
    a, b = open(sys.argv[1], "rb").read(), open(sys.argv[2], "rb").read()
    print(f"sizes {len(a):,} / {len(b):,}")
    secs = sections(a)
    for name, off, size in secs:
        if off + size > len(b):
            continue
        n = sum(x != y for x, y in zip(a[off:off + size], b[off:off + size]))
        if n:
            print(f"section {name}: {n:,} of {size:,} bytes differ")
    ro_a = b"".join(a[o:o + s] for n, o, s in secs if n.startswith(".rodata"))
    ro_b = b"".join(b[o:o + s] for n, o, s in secs if n.startswith(".rodata"))
    sa, sb = strings(ro_a), strings(ro_b)
    for s in sorted(set(sa) - set(sb))[:12]:
        print(f"only in A: {s[:160]!r}")
    for s in sorted(set(sb) - set(sa))[:12]:
        print(f"only in B: {s[:160]!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
