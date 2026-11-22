#!/usr/bin/env python3
"""A run's records without the paths of the machine that made them, or refused.

  python3 -I scripts/scrub_records.py [--check-only] [--skip NAME]... DIR [--map FROM=TO | --scratch DIR=TO]...

Every text file under DIR (any name, any depth: logs/, net/, zone/ ...) has each rule applied in the order given:
  --map FROM=TO      the literal prefix FROM (an absolute path ending in /) becomes TO (TO may be empty: a path
                     relative to the repository);
  --scratch DIR=TO   DIR/<one directory>/ becomes TO (a scratch directory whose subdirectories are named after
                     whoever made them).
A file is rewritten only if a rule changed it. Then every file is checked, text or not (an archive, .gz, .zst or
.tar, by what it holds): none may contain a rule's FROM or DIR, a home directory (/home/<name>/, /Users/<name>/,
/root/), a scratch or worktree folder of this project's agents (spike/tmp/, ferrata-wt-, .claude/), or, in text, a
match of the regular expression in FERRATA_SCRUB_DENY if it is set (case-sensitive; names kept outside the
repository, such as cloud workspace names). Any of them: the file, the line and which pattern are listed (not the
text), exit 1.
--check-only: no file is rewritten. --skip NAME: DIR/NAME is neither rewritten nor checked (scratch that git ignores,
such as a run's work/).
"""
import argparse
import gzip
import io
import os
import re
import subprocess
import sys
import tarfile
import tempfile

GENERIC = [  # (what, pattern, text only)
    ("a home directory", re.compile(rb"/home/[^/\s\"'`]+/|/Users/[^/\s\"'`]+/|/root/"), False),
    ("an agent folder", re.compile(rb"spike/tmp/|ferrata-wt-|\.claude/"), False),
]


class Rule(argparse.Action):
    def __call__(self, parser, ns, value, option):
        src, sep, dst = value.rpartition("=")
        if not sep or not src.startswith("/"):
            parser.error(f"{option} {value!r}: want /absolute/path=replacement")
        ns.rules.append((option, src, dst))


def rules_to_regex(rules):
    out = []
    for option, src, dst in rules:
        if option == "--map":
            out.append((re.compile(re.escape(src.encode())), dst.encode()))
        else:
            base = src.rstrip("/")
            out.append((re.compile(re.escape(base.encode()) + rb"/[^/\s\"'`]+/"), dst.encode()))
    return out


def deny_patterns(rules):
    pats = [(f"a path the rules map ({option})", re.compile(re.escape(src.rstrip("/").encode() + b"/")), False)
            for option, src, _ in rules]
    pats += GENERIC
    extra = os.environ.get("FERRATA_SCRUB_DENY", "")
    if extra:  # short names: text only, case as given (random binary bytes would match them now and then)
        pats.append(("a name in FERRATA_SCRUB_DENY", re.compile(extra.encode()), True))
    return pats


def is_text(data):
    if b"\0" in data:
        return False
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return False
    return True


def write_same_mode(path, data):
    mode = os.stat(path).st_mode & 0o7777
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".scrub-")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        os.chmod(tmp, mode)
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise


def members(name, data):
    """(name, bytes) of a file, or, for an archive, of what it holds (recursively; not its compressed bytes)."""
    if name.endswith(".gz"):
        yield from members(name[:-3], gzip.decompress(data))
    elif name.endswith(".zst"):
        out = subprocess.run(["zstd", "-dc"], input=data, capture_output=True, check=True).stdout
        yield from members(name[:-4], out)
    elif name.endswith(".tar") or name.endswith(".tgz"):
        with tarfile.open(fileobj=io.BytesIO(data)) as t:
            for m in t.getmembers():
                if m.isfile():
                    yield from members(f"{name}!{m.name}", t.extractfile(m).read())
    else:
        yield name, data


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("dir")
    ap.add_argument("--check-only", action="store_true")
    ap.add_argument("--skip", action="append", default=[], metavar="NAME")
    ap.add_argument("--map", action=Rule, dest="rules", metavar="FROM=TO")
    ap.add_argument("--scratch", action=Rule, dest="rules", metavar="DIR=TO")
    ap.set_defaults(rules=[])
    a = ap.parse_args()
    root = os.path.abspath(a.dir)
    if not os.path.isdir(root):
        ap.error(f"{a.dir}: not a directory")
    subs, deny = rules_to_regex(a.rules), deny_patterns(a.rules)
    skip = {os.path.join(root, n.strip("/")) for n in a.skip}
    files = []
    for d, dirs, fs in os.walk(root):
        dirs[:] = sorted(x for x in dirs if os.path.join(d, x) not in skip)
        files += [os.path.join(d, f) for f in sorted(fs)
                  if os.path.join(d, f) not in skip and not os.path.islink(os.path.join(d, f))]
    rewritten, found = 0, []
    for path in files:
        with open(path, "rb") as f:
            data = f.read()
        if not a.check_only and is_text(data):
            new = data
            for rx, dst in subs:
                new = rx.sub(lambda _m, d=dst: d, new)
            if new != data:
                write_same_mode(path, new)
                rewritten += 1
                data = new
        rel = os.path.relpath(path, root)
        try:
            for name, body in members(rel, data):
                text = is_text(body)
                for label, rx, text_only in deny:
                    if text_only and not text:
                        continue
                    for m in rx.finditer(body):
                        line = body.count(b"\n", 0, m.start()) + 1
                        found.append(f"{name}:{line}: {label}")
                        break
        except (OSError, EOFError, tarfile.TarError, subprocess.CalledProcessError, gzip.BadGzipFile) as e:
            found.append(f"{rel}: cannot be opened to check ({type(e).__name__})")
    mode = "checked" if a.check_only else f"scrubbed ({rewritten} rewritten)"
    print(f"scrub_records: {len(files)} files under {os.path.basename(root)} {mode}; "
          f"{'none names this machine' if not found else str(len(found)) + ' finding(s)'}")
    for line in found:
        print(f"scrub_records: REFUSED {line}", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
