#!/usr/bin/env python3
"""The native archives the release CLI links, fetched once and checked against scripts/native-deps.json (review-code-ops
M1): rapidsnark (with GMP and ffiasm) and the Logos Blockchain circuits, which the upstream build scripts would
otherwise download with no checksum.

    python3 -I scripts/fetch_native.py fetch   [--dir DIR] [--refetch]   download (if absent), check, unpack (if absent)
    python3 -I scripts/fetch_native.py env     [--dir DIR] [--refetch]   the same, then `export VAR=...` lines for eval:
                                                                          RAPIDSNARK_LIB_DIR and LBC_ROOT_DIR
    python3 -I scripts/fetch_native.py record  [--dir DIR]               lines for a release's toolchains.txt
    python3 -I scripts/fetch_native.py check-build MESSAGES.jsonl [--dir DIR] [--target-dir T] [--record cli]
                                                                          after `cargo build --message-format=json`: fail
                                                                          if a build script linked native code from
                                                                          anywhere but the checked archives or the build,
                                                                          or (--record) a package not recorded compiles
                                                                          and links native code of its own
    python3 -I scripts/fetch_native.py sources --out DIR                 the pinned source tarballs (GMP) for the
                                                                          release's source offer, checked

Every use checks the cached archive's size and sha256 again before anything is taken from it, and checks the unpacked
files against their pinned sha256s; a cached archive that differs is an error (it is never used, nor silently
replaced: --refetch deletes it and downloads again). Unpacking refuses absolute paths, `..` and links. DIR defaults to
$FERRATA_NATIVE_DIR, else ~/.cache/ferrata/native. --pins FILE (default scripts/native-deps.json) is for tests.
Standard library only.
"""
import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import sys
import tarfile
import tempfile
import urllib.request
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
PINS = os.path.join(HERE, "native-deps.json")
SYSTEM_LIB_DIRS = ("/usr/lib", "/usr/lib64", "/lib", "/lib64", "/usr/local/lib")


class Bad(Exception):
    pass


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def default_dir():
    return os.environ.get("FERRATA_NATIVE_DIR") or os.path.join(
        os.environ.get("XDG_CACHE_HOME") or os.path.join(os.path.expanduser("~"), ".cache"), "ferrata", "native")


def load_pins(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def archive_path(d, a):
    ext = ".zip" if a["format"] == "zip" else ".tar.gz"
    return os.path.join(d, "archives", f"{a['name']}-{a['version']}-{a['sha256'][:16]}{ext}")


def unpacked_path(d, a):
    return os.path.join(d, f"{a['name']}-{a['version']}-{a['sha256'][:16]}")


def check_file(path, want_sha, want_bytes, what):
    size = os.path.getsize(path)
    if size != want_bytes:
        raise Bad(f"{what}: {size} bytes, pinned {want_bytes}")
    got = sha256_file(path)
    if got != want_sha:
        raise Bad(f"{what}: sha256 {got}, pinned {want_sha}")


def download(url, dest, want_sha, want_bytes, what):
    """Download to a temporary name beside dest; check size and sha256; only then rename it into place."""
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=".partial-", dir=os.path.dirname(dest))
    try:
        h, n = hashlib.sha256(), 0
        req = urllib.request.Request(url, headers={"User-Agent": "ferrata-fetch-native"})
        with os.fdopen(fd, "wb") as out, urllib.request.urlopen(req, timeout=60) as r:
            while True:
                chunk = r.read(1 << 20)
                if not chunk:
                    break
                n += len(chunk)
                if n > want_bytes:
                    raise Bad(f"{what}: more than the pinned {want_bytes} bytes from {url}")
                h.update(chunk)
                out.write(chunk)
        if n != want_bytes:
            raise Bad(f"{what}: {n} bytes from {url}, pinned {want_bytes}")
        if h.hexdigest() != want_sha:
            raise Bad(f"{what}: sha256 {h.hexdigest()} from {url}, pinned {want_sha}")
        os.replace(tmp, dest)
    except OSError as e:
        raise Bad(f"{what}: download from {url} failed: {e}") from None
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def safe_name(name):
    parts = name.replace("\\", "/").split("/")
    return not (name.startswith("/") or ".." in parts or re.match(r"^[A-Za-z]:", name))


def unpack(archive, fmt, dest):
    """Unpack into dest (which must not exist): regular files and directories only, no path leaving dest."""
    os.makedirs(dest)
    if fmt == "zip":
        with zipfile.ZipFile(archive) as z:
            for i in z.infolist():
                mode = (i.external_attr >> 16) & 0o170000
                if not safe_name(i.filename) or mode not in (0, 0o100000, 0o040000):
                    raise Bad(f"{os.path.basename(archive)}: refused member {i.filename!r}")
            z.extractall(dest)
    else:
        with tarfile.open(archive, "r:gz") as t:
            members = t.getmembers()
            for m in members:
                if not safe_name(m.name) or not (m.isfile() or m.isdir()):
                    raise Bad(f"{os.path.basename(archive)}: refused member {m.name!r}")
            if hasattr(tarfile, "data_filter"):
                t.extractall(dest, members=members, filter="data")
            else:
                t.extractall(dest, members=members)


def check_unpacked(root, a):
    for rel, want in a["files"].items():
        p = os.path.join(root, rel)
        if not os.path.isfile(p):
            raise Bad(f"{a['name']}: {rel} missing from the unpacked archive")
        got = sha256_file(p)
        if got != want:
            raise Bad(f"{a['name']}: {rel} has sha256 {got}, pinned {want}")


def fetch_one(d, a, refetch=False, log=print):
    arc, out = archive_path(d, a), unpacked_path(d, a)
    what = f"{a['name']} {a['version']} ({os.path.basename(a['url'])})"
    if os.path.exists(arc):
        try:
            check_file(arc, a["sha256"], a["bytes"], f"cached {what} at {arc}")
        except Bad as e:
            if not refetch:
                raise Bad(f"{e}: the cached archive is corrupted or was replaced; it was not used. Delete it, "
                          "or run again with --refetch") from None
            log(f"{e}: deleting it and downloading again (--refetch)")
            os.unlink(arc)
            shutil.rmtree(out, ignore_errors=True)
    if not os.path.exists(arc):
        log(f"downloading {what}")
        download(a["url"], arc, a["sha256"], a["bytes"], what)
    if os.path.isdir(out):
        try:
            check_unpacked(out, a)
            return out
        except Bad as e:
            log(f"{e}: unpacking again")
            shutil.rmtree(out)
    tmp = tempfile.mkdtemp(prefix=".unpack-", dir=d)
    try:
        unpack(arc, a["format"], os.path.join(tmp, "x"))
        check_unpacked(os.path.join(tmp, "x"), a)
        try:
            os.replace(os.path.join(tmp, "x"), out)
        except OSError:
            if not os.path.isdir(out):     # another fetch put it there first: use it if it checks out
                raise
            check_unpacked(out, a)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return out


def fetch_all(d, pins, refetch=False, log=print):
    roots = {}
    for a in pins["archives"]:
        roots[a["name"]] = fetch_one(d, a, refetch, log)
        log(f"ok {a['name']} {a['version']}: sha256 {a['sha256']}, {len(a['files'])} files checked, {roots[a['name']]}")
    return roots


def env_lines(d, pins, roots):
    out = []
    for a in pins["archives"]:
        for var, sub in a["env"].items():
            p = os.path.join(roots[a["name"]], sub)
            if not os.path.isdir(p):
                raise Bad(f"{a['name']}: {sub} is not a directory of the archive")
            out.append(f"export {var}={shlex.quote(os.path.realpath(p))}")
    return out


# ---------------------------------------------------------------------------------------------------------------
# check-build: what the build scripts linked
# ---------------------------------------------------------------------------------------------------------------
def package_name(pkgid):
    """The package name of a cargo package id: `name version (source)` or a package id spec `url#name@version`."""
    if " " in pkgid:
        return pkgid.split(" ", 1)[0]
    url, _, frag = pkgid.partition("#")
    if "@" in frag:
        return frag.split("@", 1)[0]
    return url.rstrip("/").rsplit("/", 1)[-1].split("?", 1)[0]


def lib_name(spec):
    """`static=foo`, `static:+whole-archive=foo`, `dylib=foo`, `foo` -> (kind, name)."""
    kind, _, name = spec.rpartition("=")
    return (kind.split(":", 1)[0] or "default"), name.split(":", 1)[0]


def within(path, roots):
    p = os.path.realpath(path)
    return any(p == r or p.startswith(r + os.sep) for r in roots)


def check_build(messages, d, pins, target_dir, record=None):
    """Problems found (empty: the build linked native code only from the checked archives and from its own output).
    record: a key of the pins' compiled_from_crate_sources (the packages whose build scripts compile and link static
    libraries from their own sources in that build): any other package doing so, or a recorded one no longer doing
    so, is a problem."""
    scripts = {}
    with open(messages, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                j = json.loads(line)
            except ValueError:
                continue
            if j.get("reason") == "build-script-executed":
                scripts.setdefault(package_name(j.get("package_id", "")), []).append(j)
    problems = []
    if not scripts:
        return ["no build-script-executed messages: build with --message-format=json and pass that output"]
    native_roots = {}
    for a in pins["archives"]:
        root = os.path.realpath(unpacked_path(d, a))
        native_roots[a["name"]] = root
        for pkg in a["build_packages"]:
            if pkg not in scripts:
                problems.append(f"{pkg}: its build script is not in this build's messages (expected to link {a['name']})")
                continue
            for j in scripts[pkg]:
                paths = [p.split("=", 1)[-1] for p in j.get("linked_paths", [])]
                if not paths:
                    problems.append(f"{pkg}: links no native path (expected {a['name']}'s)")
                for p in paths:
                    if not within(p, [root]):
                        problems.append(f"{pkg}: links from {p}, not from the checked {a['name']} archive at {root} "
                                        "(its build script fetched its own copy: is "
                                        f"{'/'.join(a['env'])} set?)")
                for spec in j.get("linked_libs", []):
                    kind, name = lib_name(spec)
                    if kind == "static" and name not in a["static_libs"]:
                        problems.append(f"{pkg}: links static {name}, not one of {a['name']}'s {a['static_libs']}")
    owned = {pkg for a in pins["archives"] for pkg in a["build_packages"]}
    target = os.path.realpath(target_dir) if target_dir else None
    if record is not None and record not in pins.get("compiled_from_crate_sources", {}):
        return [f"no record {record!r} in compiled_from_crate_sources"]
    recorded = pins["compiled_from_crate_sources"][record] if record is not None else None
    compiling = set()
    for pkg, msgs in sorted(scripts.items()):
        if pkg in owned:
            continue
        for j in msgs:
            out_dir = os.path.realpath(j.get("out_dir") or "/nonexistent")
            for p in (x.split("=", 1)[-1] for x in j.get("linked_paths", [])):
                if within(p, [out_dir] + ([target] if target else [])):
                    continue
                if within(p, list(native_roots.values())):
                    problems.append(f"{pkg}: links from a native archive meant for {sorted(owned)}: {p}")
                elif not within(p, [os.path.realpath(x) for x in SYSTEM_LIB_DIRS]):
                    problems.append(f"{pkg}: links from {p}, outside the build, the checked archives and the system "
                                    "library directories")
            statics = sorted(lib_name(s)[1] for s in j.get("linked_libs", []) if lib_name(s)[0] == "static")
            if statics:
                compiling.add(pkg)
                if recorded is not None and pkg not in recorded:
                    problems.append(f"{pkg}: links static {statics}, and compiled_from_crate_sources[{record!r}] does "
                                    "not list it (new native code in the build: check where it comes from and its "
                                    "licence, then record it)")
    if recorded is not None:
        for pkg in sorted(set(recorded) - compiling):
            problems.append(f"{pkg}: listed in compiled_from_crate_sources[{record!r}] but links no static library in "
                            "this build (update the record)")
    return problems


def record_lines(pins):
    return [f"native_{a['name']} {a['version']} sha256 {a['sha256']} {a['url']}" for a in pins["archives"]]


def fetch_sources(out, pins, log=print):
    os.makedirs(out, exist_ok=True)
    for s in pins["sources"]:
        name = os.path.basename(s["url"])
        dest = os.path.join(out, name)
        if os.path.exists(dest):
            check_file(dest, s["sha256"], s["bytes"], f"{name} in {out}")
        else:
            log(f"downloading {name}")
            download(s["url"], dest, s["sha256"], s["bytes"], name)
        log(f"ok {name}: sha256 {s['sha256']}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("cmd", choices=("fetch", "env", "record", "check-build", "sources"))
    ap.add_argument("messages", nargs="?")
    ap.add_argument("--dir", default=default_dir())
    ap.add_argument("--pins", default=PINS)
    ap.add_argument("--refetch", action="store_true")
    ap.add_argument("--target-dir", default="")
    ap.add_argument("--record", default=None, help="check-build: the compiled_from_crate_sources entry of this build")
    ap.add_argument("--out", default="")
    a = ap.parse_args()
    pins = load_pins(a.pins)
    d = os.path.abspath(a.dir)
    err = lambda m: print(m, file=sys.stderr)
    try:
        if a.cmd == "fetch":
            fetch_all(d, pins, a.refetch)
        elif a.cmd == "env":
            roots = fetch_all(d, pins, a.refetch, log=err)
            print("\n".join(env_lines(d, pins, roots)))
        elif a.cmd == "record":
            print("\n".join(record_lines(pins)))
        elif a.cmd == "check-build":
            if not a.messages:
                raise Bad("check-build: MESSAGES.jsonl (cargo build --message-format=json output) is required")
            problems = check_build(a.messages, d, pins, a.target_dir, a.record)
            for p in problems:
                err(f"check-build: {p}")
            if problems:
                return 1
            print(f"check-build: every build script linked native code only from its own build output, the system "
                  f"library directories, or the checked archives ({', '.join(x['name'] for x in pins['archives'])})")
        elif a.cmd == "sources":
            if not a.out:
                raise Bad("sources: --out DIR is required")
            fetch_sources(a.out, pins)
    except Bad as e:
        err(f"fetch_native: {e}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
