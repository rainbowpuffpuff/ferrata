#!/usr/bin/env python3
"""check_network.evaluate: what makes a deployed program "ok" (no network, no build).

    python3 -I scripts/reproduce/test_check_network.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_network import evaluate  # noqa: E402

ID, SHA = "0x" + "ab" * 32, "cd" * 32
P = {"name": "prog", "image_id": ID, "binary_sha256": SHA}
ROW = {"image_id": ID, "sha256_bin": SHA, "r0vm_id": "= id"}
LIVE = {"image_id": ID, "binary_sha256": SHA, "program_account": "acct"}


def hdr(img=ID, imm=True):
    return lambda _acct: (img, imm)


def boom(_acct):
    raise RuntimeError("no loader state")


CASES = [  # (name, compare, row, network, live, header, expected ok)
    ("all equal, immutable", True, ROW, True, LIVE, hdr(), True),
    ("mutable header", True, ROW, True, LIVE, hdr(imm=False), False),
    ("header names another image", True, ROW, True, LIVE, hdr(img="0x" + "00" * 32), False),
    ("r0vm disagrees", True, {**ROW, "r0vm_id": "0x" + "11" * 32}, True, LIVE, hdr(), False),
    ("r0vm not run", True, {**ROW, "r0vm_id": "?"}, True, LIVE, hdr(), False),
    ("rebuilt binary differs", True, {**ROW, "sha256_bin": "00" * 32}, True, LIVE, hdr(), False),
    ("not rebuilt", True, None, True, LIVE, hdr(), False),
    ("absent from /programs.json", True, ROW, True, None, hdr(), False),
    ("published entry differs", True, ROW, True, {**LIVE, "binary_sha256": "00" * 32}, hdr(), False),
    ("header unreadable", True, ROW, True, LIVE, boom, False),
    ("chain only, immutable", False, None, True, LIVE, hdr(), True),
    ("chain only, mutable", False, None, True, LIVE, hdr(imm=False), False),
    ("rebuild only", True, ROW, False, None, None, True),
]
fails = 0
for name, compare, row, net, live, reader, want in CASES:
    _, cells, ok = evaluate(P, compare, row, net, live, reader)
    if ok != want:
        fails += 1
        print(f"FAIL {name}: ok={ok}, want {want} ({cells})")
print(f"{len(CASES) - fails} of {len(CASES)} cases as expected")
sys.exit(1 if fails else 0)
