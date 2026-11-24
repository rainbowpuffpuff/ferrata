#!/usr/bin/env python3
"""Notices for the Basecamp release's Logos packages: the Rust crates linked into each module's plugin, with their
licences and the licence and notice files of their packages.

    python3 -I modules/scripts/module_notices.py collect EXPORTS OUT
        EXPORTS: basecamp-release.sh build's OUT/src (one exported repository per module, its rust-lib/ with the
        crates staged and Cargo.lock). For each module and each target it is released for (x86_64-unknown-linux-gnu;
        aarch64-apple-darwin but for ferrata_updater_module), the crates of `cargo tree -e normal,no-proc-macro
        --target T --locked` (the crates linked into the plugin: build scripts, proc-macros and dev-dependencies are
        not), each package's licence, source and authors (`cargo metadata --locked`), and the files at the top of each
        third-party package whose names start with LICENSE, LICENCE, COPYING, NOTICE, COPYRIGHT or UNLICENSE (also its
        `license-file`; for a package that compiles native code from its own sources, a `links` key, every such file
        anywhere in it). An export without `logos-rust-sdk-src` (the builder adds it at build time) gets a link to the
        flake's pinned logos-rust-sdk in the Nix store (needs nix). Writes OUT/<module>.json and OUT/files/<package>-<version>/. Needs cargo and the network (or a
        filled cargo registry).
    python3 -I modules/scripts/module_notices.py render OUT DIR
        DIR/crates.md: per module, every crate with its version, licence and source; then each distinct licence or
        notice text once, with the crates whose package carries it. Crates whose package has no such file are listed
        with the licence their Cargo.toml declares, under the standard text of that licence in DIR/licenses/.

Path dependencies are Ferrata's own (MIT OR Apache-2.0, the repository's LICENSE-MIT and LICENSE-APACHE) when they lie in
the export outside any `upstream/` directory; the others (LEZ's crates staged from upstream/, the Logos Rust SDK the
module builder places beside rust-lib/) are third-party: their licence files are those of the nearest directory at or
above the package that has any. Standard library only.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

TARGETS = {"x86_64-unknown-linux-gnu": "linux-amd64", "aarch64-apple-darwin": "darwin-arm64"}
LINUX_ONLY = {"ferrata_updater_module"}
NAMES = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense)", re.I)


def run(cmd, cwd):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"module_notices.py: {' '.join(cmd)} in {cwd} failed:\n{r.stderr[-2000:]}")
    return r.stdout


def licence_files(d, stop):
    """The licence and notice files of directory d, else of the nearest directory above it, not above stop."""
    d, stop = os.path.realpath(d), os.path.realpath(stop)
    while True:
        names = sorted(f for f in os.listdir(d) if NAMES.match(f) and os.path.isfile(os.path.join(d, f)))
        if names or d == stop or len(d) <= len(stop) or d == os.path.dirname(d):
            return d, names
        d = os.path.dirname(d)


def sdk_source(export):
    """The Logos Rust SDK the module builder places beside rust-lib/ at build time: its pinned source in the Nix store,
    as `nix flake archive --json` (run in the exported module) names the input logos-rust-sdk."""
    tree = json.loads(run(["nix", "flake", "archive", "--json"], export))
    def find(node):
        for k, v in (node.get("inputs") or {}).items():
            if k == "logos-rust-sdk":
                return v["path"]
            r = find(v)
            if r:
                return r
    return find(tree)


def collect(exports, out):
    os.makedirs(os.path.join(out, "files"), exist_ok=True)
    for m in sorted(os.listdir(exports)):
        root = os.path.realpath(os.path.join(exports, m))
        lib = os.path.join(exports, m, "rust-lib")
        if not os.path.isfile(os.path.join(lib, "Cargo.toml")):
            continue
        if not os.path.exists(os.path.join(root, "logos-rust-sdk-src")):
            sdk = sdk_source(root)
            if not sdk:
                sys.exit(f"module_notices.py: {m}: no logos-rust-sdk input in its flake")
            os.symlink(sdk, os.path.join(root, "logos-rust-sdk-src"))
        meta = json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1"], lib))
        pkgs = {(p["name"], p["version"]): p for p in meta["packages"]}
        record = {"module": m, "targets": {}, "packages": {}}
        for triple, plat in TARGETS.items():
            if plat != "linux-amd64" and m in LINUX_ONLY:
                continue
            tree = run(["cargo", "tree", "--locked", "-e", "normal,no-proc-macro", "--target", triple, "--prefix", "none",
                        "--format", "{p}"], lib)
            seen = set()
            for line in tree.splitlines():
                g = re.match(r"^(\S+) v(\S+)", line.strip())
                if g:
                    seen.add((g.group(1), g.group(2)))
            record["targets"][plat] = sorted(f"{n} {v}" for n, v in seen)
            for n, v in seen:
                p = pkgs[(n, v)]
                pdir = os.path.realpath(os.path.dirname(p["manifest_path"]))
                rel = os.path.relpath(pdir, root) if pdir.startswith(root + os.sep) or pdir == root else None
                if p["source"] is None and rel is not None and not re.search(r"(^|/)upstream/", rel):
                    kind, where = "ferrata", "this repository"
                elif p["source"] is None:
                    tree = re.search(r"upstream/([^/]+)", rel or "")
                    kind = "path"
                    where = tree.group(1) if tree else ("logos-rust-sdk (the module builder's pin)" if "logos-rust-sdk" in n else "a path dependency")
                else:
                    kind, where = "registry", ("crates.io" if "crates.io" in p["source"] else p["source"].split("#")[0])
                local = kind == "ferrata"
                files = []
                if not local:
                    if kind == "path":
                        stop = os.path.join(root, "rust-lib") if rel is not None else re.sub(r"^(/nix/store/[^/]+).*", r"\1", pdir)
                        if rel is not None and "upstream/" in rel:
                            stop = os.path.join(root, rel[:rel.index("upstream/")] + "upstream", rel.split("upstream/")[1].split("/")[0])
                        d, names = licence_files(pdir, stop)
                    elif p.get("links"):
                        # It compiles native code from its own sources (RocksDB, ring, ...): every licence or notice
                        # file anywhere in the package, as the release CLI's notices do (scripts/crate_notices.py).
                        d = pdir
                        names = []
                        for r, dirs, fs in os.walk(d):
                            dirs[:] = [x for x in dirs if x not in ("target", ".git")]
                            names += [os.path.relpath(os.path.join(r, f), d) for f in fs if NAMES.match(f)]
                    else:
                        d = pdir
                        names = [f for f in os.listdir(d) if NAMES.match(f) and os.path.isfile(os.path.join(d, f))]
                    if p.get("license_file"):
                        names.append(os.path.relpath(os.path.join(pdir, p["license_file"]), d))
                    for f in sorted(set(names)):
                        src = os.path.normpath(os.path.join(d, f))
                        if not os.path.isfile(src):
                            continue
                        dst = os.path.join(out, "files", f"{n}-{v}", f.replace(os.sep, "__"))
                        os.makedirs(os.path.dirname(dst), exist_ok=True)
                        shutil.copyfile(src, dst)
                        files.append(f.replace(os.sep, "__"))
                record["packages"][f"{n} {v}"] = {"license": p.get("license"), "source": p["source"] or "path",
                                                  "authors": p.get("authors", []), "repository": p.get("repository"),
                                                  "files": files, "local": local, "kind": kind, "where": where}
        with open(os.path.join(out, f"{m}.json"), "w", encoding="utf-8") as f:
            json.dump(record, f, indent=1, sort_keys=True)
        print(f"{m}: " + ", ".join(f"{k} {len(v)} crates" for k, v in record["targets"].items()))


def render(out, dest):
    os.makedirs(os.path.join(dest, "licenses"), exist_ok=True)
    texts, by_text, lines, plain = {}, {}, [], {}
    lines.append("# Rust crates in the Basecamp release's plugins\n")
    lines.append("Generated by `modules/scripts/module_notices.py` from each exported module's `Cargo.lock` "
                 "(`cargo tree -e normal,no-proc-macro --target T --locked`: the crates linked into the plugin). "
                 "The repository's own crates (source `path`) are MIT OR Apache-2.0 (`licenses/LICENSE-MIT`, "
                 "`licenses/LICENSE-APACHE`). Each licence or notice file of a third-party package is given once, "
                 "below the lists, with the crates that carry it; a crate whose package has no such file is used under "
                 "the licence its `Cargo.toml` declares, whose standard text is in `licenses/`.\n")
    for name in sorted(os.listdir(out)):
        if not name.endswith(".json"):
            continue
        rec = json.load(open(os.path.join(out, name), encoding="utf-8"))
        lines.append(f"\n## {rec['module']}\n")
        all_pk = sorted(set(sum(rec["targets"].values(), [])), key=lambda s: (s.split()[0].lower(), s))
        lines.append("| Crate | Version | Licence | In | Source |\n|---|---|---|---|---|")
        for key in all_pk:
            n, v = key.split(" ", 1)
            p = rec["packages"][key]
            where = ", ".join(t for t, lst in sorted(rec["targets"].items()) if key in lst)
            src = p.get("where") or ("this repository" if p["local"] else p["source"])
            lines.append(f"| {n} | {v} | {p['license'] or '(see its files)'} | {where} | {src} |")
            if p["local"]:
                continue
            if not p["files"]:
                plain.setdefault(p["license"] or "?", {})[key] = ", ".join(p.get("authors") or []) or "its authors"
            for f in p["files"]:
                data = open(os.path.join(out, "files", f"{n}-{v}", f), "rb").read()
                norm = re.sub(rb"\s+", b" ", data).strip()
                h = hashlib.sha256(norm).hexdigest()[:16]
                texts.setdefault(h, data)
                by_text.setdefault(h, set()).add(key)
    lines.append("\n## Crates whose package carries no licence file\n")
    lines.append("Each is used under the licence its `Cargo.toml` declares, with the copyright of the authors it names "
                 "(below); the standard texts are in `licenses/` (`MIT.txt`, `Apache-2.0.txt`, `BSD-3-Clause.txt`; "
                 "CC0-1.0 asks for no notice). Where a choice is offered, it is used under the first.\n")
    for lic, keys in sorted(plain.items()):
        lines.append(f"\n### `{lic}`\n")
        for key, who in sorted(keys.items()):
            lines.append(f"- {key}: Copyright (c) {who}")
    lines.append("\n## Licence and notice texts of the packages\n")
    for h, data in sorted(texts.items(), key=lambda kv: sorted(by_text[kv[0]])[0]):
        lines.append(f"\n### {', '.join(sorted(by_text[h]))}\n")
        lines.append("```text\n" + data.decode("utf-8", "replace").rstrip() + "\n```")
    with open(os.path.join(dest, "crates.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"crates.md: {len(texts)} distinct texts; {sum(len(v) for v in plain.values())} crates without a licence file "
          f"under {len(plain)} declared licences: {sorted(plain)}")


if len(sys.argv) == 4 and sys.argv[1] == "collect":
    collect(sys.argv[2], sys.argv[3])
elif len(sys.argv) == 4 and sys.argv[1] == "render":
    render(sys.argv[2], sys.argv[3])
else:
    sys.exit("usage: module_notices.py collect EXPORTS OUT | render OUT DIR")
