#!/usr/bin/env python3
"""Notices for the Rust crates in the release CLI (review-fix-supply M-1): every crate linked into `cli/ferrata`, its
licence and the licence and notice files of its package, generated from the build itself.

    python3 -I scripts/crate_notices.py generate --cli DIR --messages FILE --out DIR
                                        [--cargo-tree FILE] [--metadata FILE] [--std-copyright FILE] [--rustc-version S]
        after `cargo build --release --locked --message-format=json` in DIR (scripts/release.sh build does both):
        writes OUT/crates.tsv (the dependency set), OUT/THIRD-PARTY-CRATES.md (the notices) and
        OUT/rust-std-COPYRIGHT-library.html (the standard library's notices, as the toolchain ships them)
    python3 -I scripts/crate_notices.py check RELEASE_DIR
        the release's notices match its dependency set and lockfile (scripts/release_manifest.py check runs this)
    python3 -I scripts/crate_notices.py binary-check RELEASE_DIR [--strict]
        every crate whose code is in cli/ferrata (its Rust symbols, read with GNU nm) is in the notices; --strict fails
        when nm is missing or cannot demangle Rust symbols (scripts/release.sh verify runs this)

The dependency set is `cargo tree -e normal,no-proc-macro --target x86_64-unknown-linux-gnu -p ferrata-cli`: the
packages linked into the binary, with the build's own feature resolution (build scripts, proc-macros and their
dependencies, and dev-dependencies are not in it). `generate` fails unless every one of them was compiled by the build
whose messages it is given. For each third-party package (the repository's own crates are MIT OR Apache-2.0, the
release's LICENSE-MIT and LICENSE-APACHE): the licence it is used under (Apache-2.0 where its licence offers it, else
MIT, ...: one alternative of an `OR`), and its licence, copying, notice and copyright files: those at the package's top
level and in LICENSES/, else (git and path packages) those of the nearest directory above it up to the checkout's
root. Packages that compile native code from their own sources (a `links` key, or listed in scripts/native-deps.json
`compiled_from_crate_sources.cli`: AWS-LC, ring, RocksDB, NVTX) give every such file anywhere in the package, and the
SPDX identifiers and copyright lines of their C, C++ and assembly sources. A package with no such file is used under
the licence its Cargo.toml declares: Apache-2.0 (the release's LICENSE-APACHE), MIT (the MIT text with the authors the
package names), or a licence that asks for no notice in a binary (CC0-1.0, Unlicense, 0BSD, MIT-0, BSL-1.0); any other
licence without a file stops `generate`. Each distinct text (whitespace aside) is given once. `--cargo-tree`,
`--metadata`, `--std-copyright` and `--rustc-version` replace the commands (tests). Standard library only.
"""
import argparse
import collections
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
TARGET = "x86_64-unknown-linux-gnu"
PACKAGE = "ferrata-cli"
NOTICES = "THIRD-PARTY-CRATES.md"
CRATES = "crates.tsv"                                    # cli/crates.tsv in a release
STD = "rust-std-COPYRIGHT-library.html"                  # licenses/rust-std-COPYRIGHT-library.html in a release
TSV_COLUMNS = ["name", "version", "source", "lib", "licence", "used_under", "origin", "texts"]
LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense|authors|patents|third[-_]?party|legal)", re.I)
CODE_FILE = re.compile(r"\.(c|h|cc|cpp|cxx|hpp|hh|rs|py|inc|cmake|sh|pl|go|java|js|ts|json|toml|ya?ml|in|bzl|bazel|mk|"
                       r"s|asm|cu|cuh|patch|diff)$", re.I)
SOURCE_FILE = re.compile(r"\.(c|h|cc|cpp|cxx|hpp|hh|S|s|asm|inc|cu|cuh)$")
NO_NOTICE = {"CC0-1.0", "Unlicense", "0BSD", "MIT-0", "BSL-1.0"}
PREFER = ["Apache-2.0", "MIT", "Apache-2.0 WITH LLVM-exception", "BSD-3-Clause", "BSD-2-Clause", "ISC", "Zlib", "0BSD",
          "Unlicense", "CC0-1.0", "MIT-0", "BSL-1.0", "Unicode-3.0", "Unicode-DFS-2016", "CDLA-Permissive-2.0", "MPL-2.0"]
# The standard library's crates that live in rust-lang/rust itself (its out-of-tree dependencies are read from the
# toolchain's COPYRIGHT-library.html).
STD_IN_TREE = {"core", "alloc", "std", "proc_macro", "test", "panic_unwind", "panic_abort", "unwind", "std_detect",
               "compiler_builtins", "profiler_builtins", "rustc_std_workspace_core", "rustc_std_workspace_alloc",
               "rustc_std_workspace_std", "sysroot"}


class Bad(Exception):
    pass


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


# ---------------------------------------------------------------------------------------------------------------
# licence expressions
# ---------------------------------------------------------------------------------------------------------------
def alternatives(expr):
    """An SPDX expression (or the old `A/B` form) as alternatives: [frozenset of terms], a term being `ID` or
    `ID WITH EXCEPTION`; None when it does not parse."""
    if not expr or not expr.strip():
        return None
    toks = re.findall(r"\(|\)|[A-Za-z0-9.+:-]+", re.sub(r"\s*/\s*", " OR ", expr.strip()))
    toks = [t.upper() if t.upper() in ("AND", "OR", "WITH") else t for t in toks]
    pos = [0]

    def peek():
        return toks[pos[0]] if pos[0] < len(toks) else None

    def take():
        pos[0] += 1
        return toks[pos[0] - 1]

    def primary():
        t = peek()
        if t == "(":
            take()
            r = disj()
            if take() != ")":
                raise ValueError
            return r
        if t is None or t in ("AND", "OR", "WITH", ")"):
            raise ValueError
        term = take()
        if peek() == "WITH":
            take()
            term = f"{term} WITH {take()}"
        return [frozenset([term])]

    def conj():
        r = primary()
        while peek() == "AND":
            take()
            rhs = primary()
            r = [a | b for a in r for b in rhs]
        return r

    def disj():
        r = conj()
        while peek() == "OR":
            take()
            r = r + conj()
        return r

    try:
        r = disj()
    except (ValueError, IndexError):
        return None
    if pos[0] != len(toks):
        return None
    out = []
    for a in r:
        if a not in out:
            out.append(a)
    return out


def used_under(expr):
    """The alternative this release takes (Apache-2.0 where offered, else MIT, ...), as text; None if unparseable."""
    alts = alternatives(expr)
    if not alts:
        return None
    singles = [next(iter(a)) for a in alts if len(a) == 1]
    for p in PREFER:
        if p in singles:
            return p
    return " AND ".join(sorted(min(alts, key=lambda a: (len(a), sorted(a)))))


# ---------------------------------------------------------------------------------------------------------------
# the dependency set
# ---------------------------------------------------------------------------------------------------------------
def run(cmd, cwd):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        raise Bad(f"{' '.join(cmd[:3])} failed in {cwd}: " + " | ".join(r.stderr.strip().splitlines()[-3:]))
    return r.stdout


def tree_lines(cli):
    return run(["cargo", "tree", "--locked", "--offline", "-p", PACKAGE, "-e", "normal,no-proc-macro", "--target", TARGET,
                "--prefix", "none", "-f", "{p}"], cli)


def metadata_json(cli):
    return run(["cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", TARGET], cli)


def tree_matches(line, pkg):
    """Whether a `cargo tree -f {p}` line (`name vX.Y.Z [(source)]`) is this metadata package."""
    m = re.match(r"^(\S+) v(\S+)(?: \((.*)\))?$", line)
    if not m or m.group(1) != pkg["name"] or m.group(2) != pkg["version"]:
        return False
    src, shown = pkg.get("source"), m.group(3)
    if src is None:
        return shown is not None and os.path.normpath(shown) == os.path.dirname(pkg["manifest_path"])
    if src.startswith("git+"):
        if not shown:
            return False
        url, _, full = src[4:].partition("#")
        surl, _, short = shown.partition("#")
        return surl == url and full.startswith(short)
    return shown is None or shown == "*"


def dependency_set(tree_text, meta):
    """The metadata packages of the tree's lines (each line names exactly one)."""
    by_name = collections.defaultdict(list)
    for p in meta["packages"]:
        by_name[p["name"]].append(p)
    out = {}
    for line in sorted({re.sub(r" \(\*\)$", "", l.strip()) for l in tree_text.splitlines() if l.strip()}):
        name = line.split(" ", 1)[0]
        hits = [p for p in by_name.get(name, []) if tree_matches(line, p)]
        if len(hits) != 1:
            raise Bad(f"cargo tree line {line!r} matches {len(hits)} packages of cargo metadata")
        out[hits[0]["id"]] = hits[0]
    return out


def compiled_ids(messages):
    """Package ids the build compiled a library (or the binary) of, from cargo's JSON messages."""
    ids = set()
    with open(messages, encoding="utf-8", errors="replace") as f:
        for line in f:
            if '"compiler-artifact"' not in line:
                continue
            try:
                j = json.loads(line)
            except ValueError:
                continue
            kinds = set((j.get("target") or {}).get("kind") or [])
            if j.get("reason") == "compiler-artifact" and kinds & {"lib", "rlib", "dylib", "staticlib", "cdylib", "bin"}:
                ids.add(j.get("package_id"))
    return ids


def lib_name(pkg):
    for t in pkg["targets"]:
        if set(t["kind"]) & {"lib", "rlib", "dylib", "staticlib", "cdylib"}:
            return t["name"].replace("-", "_")
    return "-"


def is_own(pkg):
    if pkg.get("source") is not None:
        return False
    d = os.path.realpath(os.path.dirname(pkg["manifest_path"]))
    return d == os.path.realpath(ROOT) or d.startswith(os.path.realpath(ROOT) + os.sep)


def source_of(pkg):
    """crates.tsv's source: the lockfile's `source` for registry and git packages, `path:DIR` (relative to the
    directory holding the repository) for path packages."""
    if pkg.get("source"):
        return pkg["source"]
    d = os.path.dirname(pkg["manifest_path"])
    rel = os.path.relpath(d, os.path.dirname(ROOT))
    if rel.startswith(".."):
        rel = "/".join(d.split(os.sep)[-4:])
    return "path:" + rel.replace(os.sep, "/")


def shown_source(src):
    if src.startswith("registry+https://github.com/rust-lang/crates.io-index"):
        return "crates.io"
    if src.startswith("git+"):
        url, _, commit = src[4:].partition("#")
        return f"git {url.split('?', 1)[0]} at {commit[:12]}"
    if src.startswith("path:"):
        return f"path {src[5:]}"
    return src


# ---------------------------------------------------------------------------------------------------------------
# licence and notice files
# ---------------------------------------------------------------------------------------------------------------
def licence_like(name):
    return bool(LICENCE_FILE.match(name)) and not CODE_FILE.search(name)


def files_in(d):
    return [os.path.join(d, n) for n in sorted(os.listdir(d)) if licence_like(n) and os.path.isfile(os.path.join(d, n))]


def package_files(pkg, native):
    """(licence and notice files, where they were found: 'package', 'package (whole tree)' or 'ANCESTOR')."""
    d = os.path.dirname(pkg["manifest_path"])
    files = []
    if native:
        for dp, dns, fns in os.walk(d):
            dns[:] = sorted(x for x in dns if x not in ("target", ".git"))
            files += [os.path.join(dp, n) for n in sorted(fns) if licence_like(n)]
    else:
        files = files_in(d)
        for sub in ("LICENSES", "licenses"):
            if os.path.isdir(os.path.join(d, sub)):
                files += [os.path.join(d, sub, n) for n in sorted(os.listdir(os.path.join(d, sub)))
                          if os.path.isfile(os.path.join(d, sub, n))]
    if pkg.get("license_file"):
        p = os.path.normpath(os.path.join(d, pkg["license_file"]))
        if os.path.isfile(p) and p not in files:
            files.append(p)
    where = "package (whole tree)" if native else "package"
    src = pkg.get("source") or ""
    if not files and (not src or src.startswith("git+")):
        a = d
        while not files:
            if os.path.exists(os.path.join(a, ".cargo-ok")) or os.path.exists(os.path.join(a, ".git")):
                break                                    # the checkout's or repository's root: searched already
            parent = os.path.dirname(a)
            if parent == a:
                break
            a = parent
            files = files_in(a)
            where = os.path.relpath(a, d).replace(os.sep, "/") + "/"
    return files, where


def source_notices(d):
    """SPDX identifiers (with counts) and distinct copyright lines in the first 4 KiB of a package's C, C++ and
    assembly sources."""
    spdx, lines = collections.Counter(), set()
    for dp, dns, fns in os.walk(d):
        dns[:] = sorted(x for x in dns if x not in ("target", ".git"))
        for n in sorted(fns):
            if not SOURCE_FILE.search(n):
                continue
            with open(os.path.join(dp, n), "rb") as f:
                head = f.read(4096).decode("utf-8", "replace")
            for m in re.finditer(r"SPDX-License-Identifier:\s*(.+?)\s*(?:\*/)?\s*$", head, re.M):
                spdx[m.group(1).strip()] += 1
            for m in re.finditer(r"^[\s/*#;!-]*(Copyright\b.*?)\s*(?:\*/)?\s*$", head, re.M | re.I):
                line = " ".join(m.group(1).split())
                if re.search(r"\d|\(c\)|©", line, re.I):
                    lines.add(line)
    return spdx, sorted(lines)


def normalised(text):
    return " ".join(text.split())


def mit_text(authors, name):
    """The MIT licence with the authors a package names, for a package that ships no licence file."""
    with open(os.path.join(ROOT, "LICENSE-MIT"), encoding="utf-8") as f:
        body = f.read()
    body = body[body.index("Permission is hereby granted"):]
    who = ", ".join(re.sub(r"\s*<[^>]*>", "", a).strip() for a in authors) or f"the authors of {name}"
    return f"MIT License\n\nCopyright (c) {who}\n\n{body}"


def fence(text):
    run_len = max((len(m) for m in re.findall(r"`+", text)), default=0)
    return "`" * max(3, run_len + 1)


# ---------------------------------------------------------------------------------------------------------------
# generate
# ---------------------------------------------------------------------------------------------------------------
def generate(cli, messages, out, tree_text=None, meta=None, std_copyright=None, rustc_version=None):
    cli = os.path.abspath(cli)
    tree_text = tree_text if tree_text is not None else tree_lines(cli)
    meta = meta if meta is not None else json.loads(metadata_json(cli))
    deps = dependency_set(tree_text, meta)
    built = compiled_ids(messages)
    missing = sorted(f"{p['name']} {p['version']}" for i, p in deps.items() if i not in built)
    if missing:
        raise Bad(f"{len(missing)} packages of the dependency set were not compiled by this build "
                  f"({', '.join(missing[:8])}{' ...' if len(missing) > 8 else ''}): wrong messages file?")
    if not std_copyright:
        sysroot = run(["rustc", "--print", "sysroot"], cli).strip()
        std_copyright = os.path.join(sysroot, "share", "doc", "rust", "COPYRIGHT-library.html")
    if not os.path.isfile(std_copyright):
        raise Bad(f"{std_copyright} missing: the standard library's notices (part of the toolchain's rust-std)")
    rustc_version = rustc_version or run(["rustc", "--version"], cli).strip()
    with open(os.path.join(HERE, "native-deps.json"), encoding="utf-8") as f:
        compiled_native = set(json.load(f).get("compiled_from_crate_sources", {}).get("cli", {}))
    lock = os.path.join(cli, "Cargo.lock")

    rows, texts, order, native_notes, problems = [], {}, [], [], []
    for pkg in sorted(deps.values(), key=lambda p: (p["name"], p["version"], p["id"])):
        row = {"name": pkg["name"], "version": pkg["version"], "source": source_of(pkg), "lib": lib_name(pkg),
               "licence": pkg.get("license") or (f"see {pkg['license_file']}" if pkg.get("license_file") else "")}
        if is_own(pkg):
            row.update(used_under="MIT OR Apache-2.0", origin="ferrata", texts="LICENSE-MIT, LICENSE-APACHE")
            rows.append(row)
            continue
        native = bool(pkg.get("links")) or pkg["name"] in compiled_native
        if not os.path.isdir(os.path.dirname(pkg["manifest_path"])):
            problems.append(f"{pkg['name']} {pkg['version']}: its sources are not on this machine")
            continue
        files, where = package_files(pkg, native)
        use = used_under(pkg.get("license") or "")
        if use is None and not pkg.get("license_file"):
            problems.append(f"{pkg['name']} {pkg['version']}: licence {row['licence']!r} does not parse")
        row["used_under"] = use or row["licence"]
        ids = []
        d = os.path.dirname(pkg["manifest_path"])
        for p in files:
            text = read(p)
            key = normalised(text)
            if key not in texts:
                texts[key] = {"id": f"T{len(order) + 1}", "text": text, "from": []}
                order.append(key)
            texts[key]["from"].append(f"{pkg['name']} {pkg['version']} (`{os.path.relpath(p, d).replace(os.sep, '/')}`)")
            if texts[key]["id"] not in ids:
                ids.append(texts[key]["id"])
        if ids:
            cell = ", ".join(ids) + ("" if where.startswith("package") else f" (from `{where}`)")
        else:
            terms = set(re.split(r" AND ", row["used_under"])) if use else set()
            if terms == {"Apache-2.0"} or terms == {"Apache-2.0 WITH LLVM-exception"}:
                cell = "LICENSE-APACHE (no licence file in the package)"
            elif terms == {"MIT"}:
                text = mit_text(pkg.get("authors") or [], pkg["name"])
                key = normalised(text)
                if key not in texts:
                    texts[key] = {"id": f"T{len(order) + 1}", "text": text, "from": []}
                    order.append(key)
                texts[key]["from"].append(f"{pkg['name']} {pkg['version']} (no licence file: the MIT text with the "
                                          "authors its Cargo.toml names)")
                cell = f"{texts[key]['id']} (no licence file in the package: MIT with its authors)"
            elif terms and terms <= NO_NOTICE:
                cell = f"none required ({row['used_under']}; no licence file in the package)"
            else:
                problems.append(f"{pkg['name']} {pkg['version']}: used under {row['used_under']!r} and its package has "
                                "no licence file: add the text by hand (scripts/crate_notices.py)")
                cell = "MISSING"
        row.update(origin="third-party", texts=cell)
        rows.append(row)
        if native:
            spdx, lines = source_notices(d)
            native_notes.append((pkg, spdx, lines))
    if problems:
        raise Bad("crate notices incomplete:\n  " + "\n  ".join(problems))

    os.makedirs(out, exist_ok=True)
    tsv = os.path.join(out, CRATES)
    with open(tsv, "w", encoding="utf-8") as f:
        f.write("\t".join(TSV_COLUMNS) + "\n")
        for r in rows:
            f.write("\t".join(r[c] for c in TSV_COLUMNS) + "\n")
    shutil.copyfile(std_copyright, os.path.join(out, STD))
    own = [r for r in rows if r["origin"] == "ferrata"]
    third = [r for r in rows if r["origin"] == "third-party"]
    kinds = collections.Counter(shown_source(r["source"]).split(" ", 1)[0] for r in third)
    census = collections.Counter(r["used_under"] for r in third)
    lines = [
        "# Third-party notices: the Rust crates in `cli/ferrata`",
        "",
        "Generated by `scripts/crate_notices.py generate` during `scripts/release.sh build`, from the build's own",
        "dependency graph, and part of every Ferrata release beside `THIRD-PARTY-NOTICES.md` (the native libraries of",
        "the two prebuilt archives) and `licenses/`. Not to be edited: `scripts/release.sh verify` fails a release whose",
        "notices differ from its lockfile, its dependency set or its binary.",
        "",
        f"- Lockfile: `cli/Cargo.lock`, sha256 `{sha256(lock)}`.",
        f"- Dependency set: `cli/crates.tsv`, sha256 `{sha256(tsv)}`: `cargo tree -e normal,no-proc-macro --target "
        f"{TARGET} -p {PACKAGE}`, {len(rows)} packages, every one compiled by this build (build scripts, procedural "
        "macros and dev-dependencies are not linked into the binary and are not listed).",
        f"- Rust: {rustc_version}. The standard library's notices, as the toolchain ships them: `licenses/{STD}`.",
        f"- Ferrata's own crates ({len(own)}), MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`): "
        + ", ".join(f"{r['name']} {r['version']}" for r in own) + ".",
        f"- Third-party crates: {len(third)} ("
        + ", ".join(f"{n} from {k}" for k, n in sorted(kinds.items(), key=lambda x: -x[1])) + "). Used under: "
        + "; ".join(f"{k} {n}" for k, n in sorted(census.items(), key=lambda x: (-x[1], x[0]))) + ".",
        "",
        "Each crate's row gives the licence it is used under here (where its licence offers a choice, one alternative:",
        "Apache-2.0 where offered, else MIT, ...) and the licence, copying and notice files of its package, each text",
        "given once below. The Apache License 2.0 is `LICENSE-APACHE`. A package that ships no licence file is used under",
        "the licence its Cargo.toml declares: for MIT, its text is given with the authors the package names. Packages",
        "that compile native code from their own sources (AWS-LC in aws-lc-sys, ring, RocksDB in librocksdb-sys, NVTX in",
        "nvtx) give every licence file anywhere in the package, and the SPDX identifiers and copyright lines of their C,",
        "C++ and assembly sources (at the end).",
        "",
        "## Crates",
        "",
        "| Crate | Version | Source | Declared licence | Used under | Licence and notice files |",
        "|---|---|---|---|---|---|",
    ]
    for r in third:
        lines.append(f"| {r['name']} | {r['version']} | {shown_source(r['source'])} | {r['licence'] or '-'} | "
                     f"{r['used_under']} | {r['texts']} |")
    lines += ["", "## Licence and notice texts", ""]
    for key in order:
        t = texts[key]
        fe = fence(t["text"])
        lines += [f"### {t['id']}", "", "From: " + "; ".join(t["from"]) + ".", "", fe + "text", t["text"].rstrip("\n"),
                  fe, ""]
    lines += ["## Native code compiled from crate sources", ""]
    for pkg, spdx, cr in native_notes:
        lines += [f"### {pkg['name']} {pkg['version']}", "",
                  "SPDX identifiers in its C, C++ and assembly sources (files): "
                  + ("; ".join(f"`{k}` ({n})" for k, n in sorted(spdx.items(), key=lambda x: (-x[1], x[0]))) or "none")
                  + ".", ""]
        if cr:
            fe = fence("\n".join(cr))
            lines += ["Copyright lines in those sources:", "", fe + "text"] + cr + [fe, ""]
        else:
            lines += ["No copyright lines in those sources.", ""]
    with open(os.path.join(out, NOTICES), "w", encoding="utf-8") as f:
        f.write("\n".join(lines).rstrip("\n") + "\n")
    print(f"crate notices: {len(rows)} packages ({len(own)} Ferrata's, {len(third)} third-party), {len(order)} distinct "
          f"texts, {len(native_notes)} packages compiling native code; {NOTICES}, {CRATES}, {STD}")
    return rows


# ---------------------------------------------------------------------------------------------------------------
# check (a release directory)
# ---------------------------------------------------------------------------------------------------------------
def read_tsv(path):
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()
    if not lines or lines[0].split("\t") != TSV_COLUMNS:
        raise Bad(f"{path}: not a crates.tsv (columns {TSV_COLUMNS})")
    return [dict(zip(TSV_COLUMNS, l.split("\t"))) for l in lines[1:] if l.strip()]


def lock_packages(path):
    """{(name, version, source or '')} of a Cargo.lock."""
    out = set()
    for block in read(path).split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "([^"]+)"', block, re.M)
        if name and version:
            out.add((name.group(1), version.group(1), source.group(1) if source else ""))
    return out


def check(d):
    """Why the release's crate notices do not match its dependency set and lockfile (empty: they match)."""
    paths = {k: os.path.join(d, v) for k, v in
             (("notices", NOTICES), ("tsv", "cli/" + CRATES), ("lock", "cli/Cargo.lock"), ("std", "licenses/" + STD))}
    probs = [f"{os.path.relpath(p, d)} missing" for p in paths.values() if not os.path.isfile(p)]
    if probs:
        return probs
    text = read(paths["notices"])
    m = re.search(r"`cli/Cargo\.lock`, sha256 `([0-9a-f]{64})`", text)
    if not m or m.group(1) != sha256(paths["lock"]):
        probs.append(f"{NOTICES} was made for another cli/Cargo.lock (sha256 {m.group(1) if m else 'not given'}, the "
                     f"release's {sha256(paths['lock'])})")
    m = re.search(r"`cli/crates\.tsv`, sha256 `([0-9a-f]{64})`", text)
    if not m or m.group(1) != sha256(paths["tsv"]):
        probs.append(f"{NOTICES} was made from another dependency set than cli/{CRATES}")
    try:
        rows = read_tsv(paths["tsv"])
    except Bad as e:
        return probs + [str(e)]
    lock = lock_packages(paths["lock"])
    for r in rows:
        src = "" if r["source"].startswith("path:") else r["source"]
        if (r["name"], r["version"], src) not in lock:
            probs.append(f"cli/{CRATES}: {r['name']} {r['version']} ({shown_source(r['source'])}) is not in cli/Cargo.lock")
    table = text.split("\n## Crates\n", 1)[-1].split("\n## ", 1)[0]
    listed = set()
    for line in table.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) == 6 and cells[0] not in ("Crate", "---"):
            listed.add(tuple(cells[:3]) + (cells[4], cells[5]))
    want = {(r["name"], r["version"], shown_source(r["source"]), r["used_under"], r["texts"])
            for r in rows if r["origin"] == "third-party"}
    for x in sorted(want - listed):
        probs.append(f"{NOTICES} does not list {x[0]} {x[1]} ({x[2]}) as cli/{CRATES} does")
    for x in sorted(listed - want):
        probs.append(f"{NOTICES} lists {x[0]} {x[1]} ({x[2]}), which cli/{CRATES} does not")
    sections = {}
    for m in re.finditer(r"^### (T\d+)\n\nFrom: (.*)$", text, re.M):
        sections[m.group(1)] = m.group(2)
    for r in rows:
        if r["origin"] != "third-party":
            continue
        tids = re.findall(r"\bT\d+\b", r["texts"])
        for t in tids:
            if t not in sections:
                probs.append(f"{NOTICES}: {r['name']} {r['version']} points to {t}, which it does not give")
            elif f"{r['name']} {r['version']} (" not in sections[t]:
                probs.append(f"{NOTICES}: {t} does not name {r['name']} {r['version']}")
        if not tids:
            terms = set(r["used_under"].split(" AND "))
            ok = (r["texts"].startswith("LICENSE-APACHE") and terms <= {"Apache-2.0", "Apache-2.0 WITH LLVM-exception"}) \
                or (r["texts"].startswith("none required") and terms <= NO_NOTICE)
            if not ok:
                probs.append(f"{NOTICES}: {r['name']} {r['version']} ({r['used_under']}) has no licence text")
    return probs


# ---------------------------------------------------------------------------------------------------------------
# binary-check: the crates whose code is in cli/ferrata
# ---------------------------------------------------------------------------------------------------------------
def std_crates(html_path):
    names = set(STD_IN_TREE)
    for m in re.finditer(r"📦\s*([A-Za-z0-9_.-]+?)-(\d+\.\d+\.\d+[A-Za-z0-9.+-]*)", read(html_path)):
        names.add(m.group(1).replace("-", "_"))
    return names


def symbol_crates(binary):
    """{crate name: symbols} of the crate roots in the binary's demangled Rust symbols (GNU nm -C); None if nm is
    missing."""
    if not shutil.which("nm"):
        return None
    p = subprocess.Popen(["nm", "--defined-only", "-C", binary], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                         text=True, errors="replace")
    heads = collections.Counter()
    head = re.compile(r"(?:^|[<(,\s&*\[{;])([A-Za-z_][A-Za-z0-9_]*)::")
    for line in p.stdout:
        parts = line.rstrip("\n").split(" ", 2)
        if len(parts) == 3:
            for m in head.finditer(parts[2]):
                heads[m.group(1)] += 1
    p.wait()
    return heads


def binary_check(d, strict=False):
    """(problems, notes) of the crates seen in cli/ferrata's symbols against cli/crates.tsv."""
    tsv, lock, binary, html = (os.path.join(d, x) for x in ("cli/" + CRATES, "cli/Cargo.lock", "cli/ferrata",
                                                           "licenses/" + STD))
    for p in (tsv, lock, binary, html):
        if not os.path.isfile(p):
            return [f"{os.path.relpath(p, d)} missing"], []
    heads = symbol_crates(binary)
    if heads is None:
        msg = "nm (GNU binutils) not found: the crates in cli/ferrata's symbols were not compared with the notices"
        return ([msg], []) if strict else ([], [msg])
    if not {"core", "std", "alloc"} & set(heads):
        msg = "nm did not demangle cli/ferrata's Rust symbols (binutils 2.36 or later reads v0 mangling)"
        return ([msg], []) if strict else ([], [msg])
    rows = read_tsv(tsv)
    listed = {r["lib"] for r in rows} | {r["name"].replace("-", "_") for r in rows}
    std = std_crates(html)
    locked = {n.replace("-", "_") for n, _, _ in lock_packages(lock)}
    unlisted = sorted(h for h in heads if h in locked and h not in listed and h not in std)
    seen = sorted(h for h in heads if h in listed)
    probs = [f"cli/ferrata has symbols of crate {h} ({heads[h]}), which cli/{CRATES} and {NOTICES} do not list"
             for h in unlisted]
    notes = [f"{len(seen)} crates of cli/{CRATES} and {len(set(heads) & std)} of the standard library have symbols in "
             f"cli/ferrata; {len(unlisted)} crates of cli/Cargo.lock with symbols there are not listed"]
    return probs, notes


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("cmd", choices=("generate", "check", "binary-check"))
    ap.add_argument("release", nargs="?")
    ap.add_argument("--cli", default=os.path.join(ROOT, "cli"))
    ap.add_argument("--messages", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--cargo-tree", default="")
    ap.add_argument("--metadata", default="")
    ap.add_argument("--std-copyright", default="")
    ap.add_argument("--rustc-version", default="")
    ap.add_argument("--strict", action="store_true")
    a = ap.parse_args()
    try:
        if a.cmd == "generate":
            if not (a.messages and a.out):
                raise Bad("generate: --messages (cargo's JSON messages of the build) and --out are required")
            generate(a.cli, a.messages, a.out, tree_text=read(a.cargo_tree) if a.cargo_tree else None,
                     meta=json.loads(read(a.metadata)) if a.metadata else None,
                     std_copyright=a.std_copyright or None, rustc_version=a.rustc_version or None)
        elif a.cmd == "check":
            probs = check(a.release)
            for p in probs:
                print(f"crate notices: {p}")
            if probs:
                return 1
            print(f"crate notices: {NOTICES} matches cli/{CRATES} and cli/Cargo.lock")
        else:
            probs, notes = binary_check(a.release, a.strict)
            for n in notes:
                print(f"binary-check: {n}")
            for p in probs:
                print(f"binary-check: {p}", file=sys.stderr)
            return 1 if probs else 0
    except Bad as e:
        print(f"crate_notices: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
