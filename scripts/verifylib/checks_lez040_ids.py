"""The image ids of Ferrata's LEZ guests built on LEZ v0.4.0, held to the source they were built from (offline).

A record under docs/port-v0.4.0/image-ids/<name>/ holds what one build wrote: `ids.tsv` (scripts/reproduce/
guest_ids.py's table: workspace, name, image id, sha256 and size of the R0BF binary, sha256 of the user ELF, r0vm's id)
and `build.json` (the Ferrata commit built, the LEZ tag and commit, the methods crates built, the toolchains, the
command, where and when). `lez040-image-ids` checks every record:

- its LEZ commit is the v0.4.0 tag's as docs/port-v0.4.0/lez-tags.json records it;
- every input of the build in this tree is byte for byte the file at the recorded commit (git's blob at that commit
  against the file here, both ways: a file changed, removed or added fails): the methods crates built, the guests they
  build, every crate of this repository those guests reach by path, lez/'s workspace manifest, lockfile and toolchain
  file, the selector and its v0.4.0 lockfile patch, and the build wrapper (.cargo/config.toml, scripts/guest-rustc*). So the recorded ids are this tree's: a change to a
  guest's source needs the build again before its id can be cited;
- the table is well formed: each guest once, a 32-byte id, r0vm's cross-check "= id" or "?" (r0vm not installed);
- the guests' ids that docs/image-ids.md's v0.4.0 section states, if it has one, are the record's;
- the core piece's own build of the guests on another machine (lez/results/v0.4.0/ids-v0.4.0.tsv), if present, has
  the same id, binary sha256 and size for every recorded guest.
"""
from __future__ import annotations

import hashlib
import json
import re
import subprocess
from pathlib import Path

from .core import CheckFailed, Skipped, check, command, fail_if

RECORDS = "docs/port-v0.4.0/image-ids"
TAGS = "docs/port-v0.4.0/lez-tags.json"
FIXED_INPUTS = ("lez/Cargo.toml", "lez/Cargo.lock", "lez/rust-toolchain.toml", "lez/scripts/lez-select.sh",
                "lez/lockfiles/v0.4.0.patch", ".cargo/config.toml", "scripts/guest-rustc", "scripts/guest-rustc.py")
DOC = "docs/image-ids.md"
OTHER_BUILD = "lez/results/v0.4.0/ids-v0.4.0.tsv"  # the core piece's build of every guest on the build box
DOC_SECTION = "## LEZ v0.4.0"


def _git(ctx):
    return getattr(ctx, "git_dir", None) or ctx.root


def _blob(data: bytes) -> str:
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def _crate_dir(root: Path, p: Path) -> Path | None:
    """The directory of the crate a path (a manifest's `path = ...`) points into: itself or the nearest parent with a
    Cargo.toml, within the repository."""
    p = p.resolve()
    for d in [p] + list(p.parents):
        if d == root or root not in d.parents:
            return None
        if d.is_dir() and (d / "Cargo.toml").is_file():
            return d
    return None


def _inputs(root: Path, crates: list) -> list:
    """Repository-relative directories and files the build of the methods crates `crates` reads from this tree."""
    root = root.resolve()
    todo, seen = [], set()
    for name in crates:
        d = root / "lez" / name
        if not (d / "Cargo.toml").is_file():
            raise CheckFailed(f"build.json names the methods crate {name}, which is not at lez/{name}/")
        todo.append(d)
    while todo:
        d = todo.pop()
        if d in seen:
            continue
        seen.add(d)
        text = (d / "Cargo.toml").read_text()
        refs = re.findall(r'path\s*=\s*"([^"]+)"', text)
        for block in re.findall(r"^methods\s*=\s*\[([^\]]*)\]", text, re.M | re.S):  # risc0-build's guest list
            refs += re.findall(r'"([^"]+)"', block)
        for ref in refs:
            c = _crate_dir(root, d / ref)
            if c is not None and c not in seen:
                todo.append(c)
    rels = sorted(str(d.relative_to(root)) for d in seen)
    return rels + [f for f in FIXED_INPUTS]


# What cargo does not compile when it builds a crate as a dependency (or a guest as a binary): its integration tests,
# benches and examples. A change there (e.g. a replay block in crates/ferrata-spectests/tests/) cannot change an id.
NOT_BUILT = ("tests", "benches", "examples")


def _built(root: Path, paths: list, f: str) -> bool:
    """Whether file `f` (repository-relative) under the inputs `paths` is part of a crate's build: not under the
    tests/, benches/ or examples/ directory of the innermost crate holding it."""
    crates = [p for p in paths if (root / p / "Cargo.toml").is_file()]
    holders = [c for c in crates if f.startswith(c.rstrip("/") + "/")]
    if not holders:
        return True
    rel = Path(f).relative_to(max(holders, key=len))
    return rel.parts[0] not in NOT_BUILT


def _tree_at(ctx, commit: str, paths: list) -> dict:
    """{path: blob} of the files under `paths` at `commit`, from the repository's git objects."""
    p = subprocess.run(["git", "cat-file", "-e", f"{commit}^{{commit}}"], cwd=_git(ctx), capture_output=True)
    if p.returncode != 0:
        raise Skipped(f"this clone lacks commit {commit[:12]} the record names (`git fetch` and run again)")
    p = subprocess.run(["git", "ls-tree", "-r", "-z", commit, "--"] + paths, cwd=_git(ctx), capture_output=True)
    if p.returncode != 0:
        raise CheckFailed(f"git ls-tree {commit[:12]} failed: {p.stderr.decode().strip()[-200:]}")
    out = {}
    for entry in p.stdout.split(b"\0"):
        if not entry:
            continue
        meta, path = entry.split(b"\t", 1)
        mode, kind, sha = meta.decode().split()
        if kind == "blob":
            out[path.decode()] = sha
    return out


def _tree_here(root: Path, paths: list) -> dict:
    """{path: blob} of the files under `paths` in this tree (build output directories left out)."""
    out = {}
    for rel in paths:
        p = root / rel
        files = [p] if p.is_file() else sorted(x for x in p.rglob("*") if x.is_file()) if p.is_dir() else []
        for f in files:
            parts = f.relative_to(root).parts
            if "target" in parts or ".scratch" in parts:
                continue
            out[str(Path(*parts))] = _blob(f.read_bytes())
    return out


TSV_HEADER = "workspace\tname\timage_id\tsha256_bin\tbytes_bin\tsha256_user_elf\tr0vm_id"


def _doc_rows(text: str, problems: list) -> dict:
    """name -> (image id, binary sha256, bytes, user ELF sha256) from the v0.4.0 section of docs/image-ids.md (absent:
    {}). The section names a guest with the suffix `_040` (`ferrata_rebate_040`): the document names each program once
    (scripts/test_image_ids.py), and the release candidates' rows keep the plain names."""
    if DOC_SECTION not in text:
        return {}
    sec = text.split(DOC_SECTION, 1)[1].split("\n## ", 1)[0]
    out = {}
    for ln in sec.splitlines():
        if not ln.startswith("|"):
            continue
        names = re.findall(r"`([a-z0-9_]+_040)`", ln)
        if not names:
            continue
        cells = [c.strip().strip("`") for c in ln.strip().strip("|").split("|")]
        name = names[0].removesuffix("_040")
        if len(cells) != 5 or not re.fullmatch(r"0x[0-9a-f]{64}", cells[1]) or not re.fullmatch(r"[0-9a-f]{64}", cells[2]) \
                or not re.fullmatch(r"[0-9]{1,3}(,[0-9]{3})*", cells[3]) or not re.fullmatch(r"[0-9a-f]{64}|-", cells[4]):
            problems.append(f"{DOC}: malformed v0.4.0 row for {name}: {ln[:120]}")
            continue
        if name in out:
            problems.append(f"{DOC}: {name} appears twice in the v0.4.0 section")
        out[name] = (cells[1], cells[2], int(cells[3].replace(",", "")), cells[4])
    return out


@check(id="lez040-image-ids", family="hash", mode="offline", needs=("python3", "git"), evidence=(302,),
       title="The LEZ guests' image ids built on LEZ v0.4.0, held to this tree's source",
       what="each record of docs/port-v0.4.0/image-ids/: its ids.tsv (a build's image ids, binaries' sha256, r0vm's "
            "cross-check) and build.json (the commit built, LEZ's commit, the toolchains)",
       against="git's blobs at the recorded commit against this tree's files, for every input of those guests' build "
               "(the guests, the crates they reach by path, the methods crates, the selector, the v0.4.0 lockfile "
               "patch, the build wrapper); LEZ's v0.4.0 commit in docs/port-v0.4.0/lez-tags.json; docs/image-ids.md",
       inputs=(RECORDS, TAGS, DOC, "lez", ".cargo/config.toml", "scripts/guest-rustc", "scripts/guest-rustc.py",
               "crates"))
def lez040_image_ids(ctx):
    base = ctx.root / RECORDS
    recs = sorted(base.glob("*/build.json")) if base.is_dir() else []
    if not recs:
        raise CheckFailed(f"no record under {RECORDS}/ (a build.json and an ids.tsv per build)")
    want_lez = json.loads((ctx.root / TAGS).read_text())["tags"]["v0.4.0"]["commit"]
    problems, lines, all_ids, full = [], [], {}, {}
    doc_rows = _doc_rows((ctx.root / DOC).read_text(), problems) if (ctx.root / DOC).is_file() else {}
    for bj in recs:
        name = bj.parent.name
        b = json.loads(bj.read_text())
        if b.get("lez_tag") != "v0.4.0" or b.get("lez_commit") != want_lez:
            problems.append(f"{name}: built on LEZ {b.get('lez_tag')} {str(b.get('lez_commit'))[:12]}, not v0.4.0 "
                            f"{want_lez[:12]}")
        commit = str(b.get("ferrata_commit", ""))
        if not re.fullmatch(r"[0-9a-f]{40}", commit):
            problems.append(f"{name}: build.json's ferrata_commit {commit!r} is not a 40-hex commit")
            continue
        paths = _inputs(ctx.root, list(b.get("crates", [])))
        then, now = _tree_at(ctx, commit, paths), _tree_here(ctx.root, paths)
        then = {f: b for f, b in then.items() if _built(ctx.root, paths, f)}
        now = {f: b for f, b in now.items() if _built(ctx.root, paths, f)}
        for f in sorted(set(then) | set(now)):
            if f not in now:
                problems.append(f"{name}: {f} is in the build's source ({commit[:12]}) and not in this tree")
            elif f not in then:
                problems.append(f"{name}: {f} is in this tree and was not in the build's source ({commit[:12]})")
            elif then[f] != now[f]:
                problems.append(f"{name}: {f} differs from the build's source ({commit[:12]}): rebuild before citing "
                                "the ids")
        tsv = (bj.parent / "ids.tsv").read_text().splitlines()
        if not tsv or tsv[0] != TSV_HEADER:
            problems.append(f"{name}: ids.tsv's header is not scripts/reproduce/guest_ids.py's")
        rows = [ln.split("\t") for ln in tsv[1:] if ln.strip()]
        if not rows:
            problems.append(f"{name}: ids.tsv lists no guest")
        for r in rows:
            if len(r) != 7 or not r[1] or not re.fullmatch(r"0x[0-9a-f]{64}", r[2]) \
                    or not re.fullmatch(r"[0-9a-f]{64}", r[3]) or not re.fullmatch(r"[1-9][0-9]*", r[4]) \
                    or not re.fullmatch(r"[0-9a-f]{64}|-", r[5]):
                problems.append(f"{name}: malformed ids.tsv row {r!r}")
                continue
            if r[6] not in ("= id", "?"):
                problems.append(f"{name}: {r[1]}: r0vm computes {r[6]} from the binary, not the recorded {r[2]}")
            key = r[1]
            if key in all_ids:
                problems.append(f"{name}: {r[1]} is recorded twice (each guest once in all the records)")
            all_ids[key] = r[2]
            full[key] = (r[2], r[3], int(r[4]), r[5])
        lines.append(f"{name}: {len(rows)} guest(s) built at {commit[:12]} on LEZ v0.4.0 {want_lez[:8]} "
                     f"({', '.join(r[1] + ' ' + r[2][:12] + '…' for r in rows if len(r) == 7)}); {len(now)} input "
                     f"files of the build equal to the commit's")
    other = ctx.root / OTHER_BUILD
    if other.is_file():
        theirs = {}
        for ln in other.read_text().splitlines()[1:]:
            c = ln.split("\t")
            if len(c) >= 4:
                theirs[c[0]] = (c[1], c[2], c[3])
        rows = {}
        for bj in recs:
            for ln in (bj.parent / "ids.tsv").read_text().splitlines()[1:]:
                c = ln.split("\t")
                if len(c) == 7:
                    rows[c[1]] = (c[2], c[3], c[4])
        for n, mine in sorted(rows.items()):
            if n not in theirs:
                problems.append(f"{OTHER_BUILD}: {n} is not in the other machine's build")
            elif theirs[n] != mine:
                problems.append(f"{OTHER_BUILD}: {n} differs from the build record: {theirs[n][0][:18]}… "
                                f"(binary {theirs[n][1][:12]}…, {theirs[n][2]} B) there, {mine[0][:18]}… "
                                f"(binary {mine[1][:12]}…, {mine[2]} B) here")
        if rows and not any(p.startswith(OTHER_BUILD) for p in problems):
            lines.append(f"{OTHER_BUILD} (another machine's build, the core piece's): {len(rows)} of {len(rows)} "
                         "guests with the same image id, binary sha256 and size")
    for n, cells in sorted(doc_rows.items()):
        if n not in full:
            problems.append(f"{DOC}: {n}'s v0.4.0 id {cells[0][:18]}… is in no build record")
            continue
        for what, there, here in zip(("image id", "binary sha256", "bytes", "user ELF sha256"), cells, full[n]):
            if there != here:
                problems.append(f"{DOC}: {n}: its {what} is {there} there, {here} in the build record")
    fail_if(problems, "the v0.4.0 guests' image ids")
    if doc_rows:
        lines.append(f"{DOC}'s v0.4.0 section: {len(doc_rows)} row(s), each one's id, binary sha256, size and user ELF "
                     "sha256 the record's")
    return lines


command(id="lez040-image-ids-rebuild", family="hash", mode="offline", tier="tests", timeout=3600, evidence=(302,),
        needs=("bash", "git", "cargo", "python3"), argv=("bash", "scripts/verify-lez040-ids.sh"),
        title="The v0.4.0 image-id records rebuilt from source: each recorded guest's id and binary equal",
        what="each record of docs/port-v0.4.0/image-ids/: the guests it names, rebuilt at its commit on LEZ v0.4.0 in "
             "a clean worktree beside the repository (scripts/verify-lez040-ids.sh)",
        against="the record's ids.tsv: image id and the R0BF binary's sha256 per guest",
        inputs=(RECORDS, "scripts/verify-lez040-ids.sh"))
