#!/usr/bin/env python3
"""Check that every relative link in Markdown files resolves to a file or directory in the tree.

Usage: scripts/check-links.py [PATH ...]

With no arguments it checks README.md and every Markdown file under docs/. A PATH may be a file or
a directory (searched for *.md). Links with a scheme (https:, mailto:, ...) and in-page anchors
(#...) are skipped; for the rest the fragment and query are dropped and the path is resolved
against the linking file's directory. Inline links, images, reference definitions and HTML
href/src attributes are checked; code spans and fenced code blocks are not. Inside a git work tree
the target must be tracked by git (so a link to an ignored or untracked file fails), otherwise it
must exist on disk. Prints one line per broken link and exits 1 if there is any.

Standard library only.
"""

import pathlib
import re
import subprocess
import sys
from urllib.parse import unquote

ROOT = pathlib.Path(__file__).resolve().parent.parent

FENCE = re.compile(r"^\s{0,3}(```|~~~)")
CODE_SPAN = re.compile(r"(`+)(.+?)\1")
INLINE = re.compile(r"!?\[(?:[^\[\]]|\[[^\]]*\])*\]\(\s*(<[^>]*>|[^)\s]+)(?:\s+[\"'(][^)]*)?\)")
REFDEF = re.compile(r"^\s{0,3}\[[^\]]+\]:\s*(<[^>]*>|\S+)")
HTML = re.compile(r"""<(?:a|img|source)\b[^>]*?\b(?:href|src)\s*=\s*["']([^"']+)["']""", re.I)
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


def tracked_paths():
    """Tracked files and their parent directories, relative to ROOT, or None outside git."""
    try:
        out = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-z"], capture_output=True, check=True
        ).stdout.decode("utf-8", "surrogateescape")
    except (OSError, subprocess.CalledProcessError):
        return None
    paths = set()
    for name in filter(None, out.split("\0")):
        parts = pathlib.PurePosixPath(name).parts
        for i in range(1, len(parts) + 1):
            paths.add("/".join(parts[:i]))
    return paths


def links(text):
    """Yield (line number, target) for every link outside code."""
    in_fence = False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        m = REFDEF.match(line)
        if m:
            yield n, m.group(1)
            continue
        bare = CODE_SPAN.sub(lambda c: " " * len(c.group(0)), line)
        for m in INLINE.finditer(bare):
            yield n, m.group(1)
        for m in HTML.finditer(bare):
            yield n, m.group(1)


def broken(md, target, tracked):
    """Return a reason if the target does not resolve, else None."""
    target = target.strip("<>")
    if not target or target.startswith("#") or SCHEME.match(target) or target.startswith("//"):
        return None
    path = unquote(target.split("#", 1)[0].split("?", 1)[0])
    if not path:
        return None
    if path.startswith("/"):
        return "absolute path"
    resolved = (md.parent / path).resolve()
    try:
        rel = resolved.relative_to(ROOT).as_posix()
    except ValueError:
        return "outside the repository"
    if rel == ".":
        return None
    if tracked is not None:
        return None if rel in tracked else "not a tracked file or directory"
    return None if resolved.exists() else "missing"


def markdown_files(args):
    if not args:
        files = [ROOT / "README.md"] + sorted((ROOT / "docs").rglob("*.md"))
        return [f for f in files if f.is_file()]
    files = []
    for a in args:
        p = pathlib.Path(a).resolve()
        files.extend(sorted(p.rglob("*.md")) if p.is_dir() else [p])
    return files


def main(argv):
    tracked = tracked_paths()
    files = markdown_files(argv[1:])
    count = bad = 0
    for md in files:
        text = md.read_text(encoding="utf-8")
        for n, target in links(text):
            count += 1
            reason = broken(md, target, tracked)
            if reason:
                bad += 1
                print(f"{md.relative_to(ROOT).as_posix()}:{n}: {target} ({reason})")
    print(f"{len(files)} files, {count} links, {bad} broken", file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
