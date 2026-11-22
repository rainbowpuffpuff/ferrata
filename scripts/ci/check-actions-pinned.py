#!/usr/bin/env python3
"""Every third-party action a workflow uses is pinned to a full 40-character commit SHA.

Covers every workflow of this repository (.github/workflows/*.yml) and of the module catalog
(modules/catalog/.github/workflows/*.yml). A `uses:` line is:

  - pinned: `owner/repo@<40 hex>` (or a local path), which passes;
  - local: a path (anything with no `@`, our own workflows), reported separately, never a failure;
  - todo: a tag followed by the exact comment "# TODO: pin to a commit SHA" (the documented stopgap
    while no SHA is available in this tree), reported separately, never a failure; a TODO anywhere
    else on the line does not count;
  - unpinned: anything else - a tag, a branch, a short SHA - and a failure: a moving target in CI.

Prints one line per finding and the three lists; exits 1 if any unpinned use remains, 0 otherwise.

Standard library only. Usage: python3 -I scripts/ci/check-actions-pinned.py
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
SCOPES = [
    ROOT / ".github" / "workflows",
    ROOT / "modules" / "catalog" / ".github" / "workflows",
]
USES = re.compile(r"^\s*(?:-\s+)?uses:\s*(\S+)\s*(.*)$")
SHA = re.compile(r"^[0-9a-f]{40}$", re.IGNORECASE)
TODO = re.compile(r"#\s*TODO: pin to a commit SHA")


def main():
    pinned, local, todo, unpinned = [], [], [], []
    for scope in SCOPES:
        if not scope.is_dir():
            continue
        for path in sorted(scope.glob("*.yml")):
            for number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), start=1
            ):
                match = USES.match(line)
                if not match:
                    continue
                use, rest = match.groups()
                where = f"{path.relative_to(ROOT)}:{number}"
                ref = use.rsplit("@", 1)[1] if "@" in use else None
                if ref is not None and SHA.match(ref):
                    pinned.append(f"{where}: {use}")
                elif ref is None:
                    local.append(f"{where}: {use}")
                elif TODO.fullmatch(rest.strip()):
                    todo.append(f"{where}: {use}")
                else:
                    unpinned.append(f"{where}: {use}  (ref \"{ref}\" is not a 40-character SHA)")

    print("Actions used by the workflows, and how they are pinned")
    print(f"  pinned to a commit SHA: {len(pinned)}")
    print(f"  local workflow paths (no pin needed): {len(local)}")
    for line in local:
        print(f"    {line}")
    print(f"  TODO-marked (a tag today, to pin when a SHA is known): {len(todo)}")
    for line in todo:
        print(f"    {line}")
    print(f"  unpinned: {len(unpinned)}")
    for line in unpinned:
        print(f"    {line}")

    if unpinned:
        print(
            "check-actions-pinned.py: pin every action above to a full commit SHA "
            "(a moving tag in CI is a supply-chain hole)",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
