#!/usr/bin/env python3
"""Write a v0.4.0 image-id record's build.json (scripts/verify-lez040-ids.sh --record).

    python3 -I scripts/reproduce/lez040_build_json.py OUT COMMIT LEZ_COMMIT STARTED CARGO GUEST_TOOLCHAINS R0VM CRATE...
"""
import json
import sys


def main() -> int:
    out, commit, lez_commit, started, cargo, guest, r0vm, *crates = sys.argv[1:]
    rec = {"ferrata_commit": commit, "lez_tag": "v0.4.0", "lez_commit": lez_commit, "crates": crates,
           "date": started,
           "where": "a clean git worktree of the commit beside upstream/lez-v0.4.0, a fresh target directory",
           "command": "scripts/verify-lez040-ids.sh --record NAME " + " ".join(crates),
           "toolchains": {"cargo": cargo, "guest toolchains in RISC0_HOME": guest.strip(),
                          "r0vm (the cross-check column)": r0vm}}
    with open(out, "w") as f:
        json.dump(rec, f, indent=2)
        f.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
