#!/usr/bin/env python3
"""Pin every third-party GitHub Action a workflow uses to a full commit SHA: the owner's one step before publishing.

The lambda-prize release-integrity rules ask for Actions pinned by commit, so that a moved tag cannot change what a
release workflow runs. The SHAs cannot be checked offline, so this script resolves each `uses: owner/repo[/path]@ref`
that is not already a 40-character SHA with `git ls-remote https://github.com/owner/repo` (an annotated tag resolves to
the commit it points at), and rewrites the line as `uses: owner/repo[/path]@<sha> # <ref>`, dropping a
`# TODO: pin to a commit SHA` comment. Local actions (`./...`) and Docker actions (`docker://...`) are left alone.

    python3 scripts/ci/pin-actions.py                     # print what it would pin (reads the network)
    python3 scripts/ci/pin-actions.py --write             # rewrite the files
    python3 scripts/ci/pin-actions.py --write FILE...     # only these files

Default files: .github/workflows/*.yml and modules/catalog/.github/workflows/*.yml. Afterwards
`python3 -I scripts/ci/check-actions-pinned.py` passes; add it back to CI's `links` job then. Standard library and git
only. Exit 1 if a ref cannot be resolved (nothing is written then).
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
USES = re.compile(r"^(?P<lead>\s*(?:-\s+)?uses:\s*)(?P<action>[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:/[^@\s]+)?)@"
                  r"(?P<ref>[^\s#]+)(?P<rest>.*)$")
SHA = re.compile(r"^[0-9a-f]{40}$")


def resolve(repo: str, ref: str, cache: dict) -> str:
    key = (repo, ref)
    if key not in cache:
        out = subprocess.run(["git", "ls-remote", f"https://github.com/{repo}", ref, f"{ref}^{{}}",
                              f"refs/tags/{ref}", f"refs/heads/{ref}"],
                             capture_output=True, text=True, timeout=60).stdout
        refs = {}
        for line in out.splitlines():
            sha, name = line.split("\t", 1)
            refs[name] = sha
        sha = (refs.get(f"refs/tags/{ref}^{{}}") or refs.get(f"refs/tags/{ref}")
               or refs.get(f"refs/heads/{ref}"))
        if not sha:
            raise SystemExit(f"pin-actions: {repo}@{ref} does not resolve (git ls-remote found no tag or branch)")
        cache[key] = sha
    return cache[key]


def main(argv):
    write = "--write" in argv
    files = [pathlib.Path(a) for a in argv if not a.startswith("--")]
    if not files:
        files = sorted(ROOT.glob(".github/workflows/*.yml")) + sorted(ROOT.glob("modules/catalog/.github/workflows/*.yml"))
    cache, changes, new_text = {}, [], {}
    for f in files:
        lines = f.read_text().splitlines(keepends=True)
        for i, line in enumerate(lines):
            m = USES.match(line.rstrip("\n"))
            if not m or SHA.match(m.group("ref")) or m.group("action").startswith("."):
                continue
            action, ref = m.group("action"), m.group("ref")
            repo = "/".join(action.split("/")[:2])
            sha = resolve(repo, ref, cache)
            rest = re.sub(r"\s*#\s*TODO: pin to a commit SHA", "", m.group("rest")).strip()
            lines[i] = f"{m.group('lead')}{action}@{sha} # {ref}{' ' + rest if rest else ''}\n"
            changes.append(f"{f.relative_to(ROOT) if f.is_absolute() else f}:{i + 1}: {action}@{ref} -> {sha}")
        new_text[f] = "".join(lines)
    for c in changes:
        print(c)
    if write:
        for f, text in new_text.items():
            f.write_text(text)
    print(f"pin-actions: {len(changes)} uses {'pinned' if write else 'to pin (run with --write)'}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
