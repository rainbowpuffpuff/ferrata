#!/usr/bin/env python3
"""rustc wrapper for RISC Zero guest builds: one commit, one image id, wherever it is built.

Called by `scripts/guest-rustc` (cargo's `build.rustc-wrapper`, set in `.cargo/config.toml` at the root of the
repository) for the units compiled for the zkVM (`--target riscv32im-risc0-zkvm-elf`); every other rustc call
(host crates, build scripts, proc macros) is passed through unchanged by that script.

Why: a guest program's image id is a hash of its ELF, and a guest built outside RISC Zero's Docker image embeds where
it was built, in two ways.
  1. Source paths: panic locations and `file!()` hold the absolute paths of the crates outside the guest's own
     workspace: the tree's shared crates (`lez/wire`, `crates/ferrata-core`, ...), the upstream LEZ sources beside the
     tree, and every crate of the cargo registry under `$HOME/.cargo`.
  2. Symbol names: cargo hashes the absolute path of a path dependency outside the workspace into that crate's
     `-C metadata` (`SourceId::stable_hash` strips only the workspace root), and so into every crate that links it;
     the hash names every symbol, and rustc orders functions by symbol name, so the code itself moves.
So this wrapper
  1. remaps the tree, the upstream sources beside it, the cargo home, the RISC Zero home, the guest toolchain and
     the guest target directory to fixed names (`--remap-path-prefix`), and
  2. replaces cargo's `-C metadata` by a hash of what identifies the unit without its location: crate name, crate
     types, edition, target, the remapped source path, the cfgs, and the replacement hashes of the crates it links
     (each recorded beside its outputs as `.ferrata-meta-<cargo's hash>` when it was compiled).
risc0-build 3.0.5 (`cargo_command_internal`) builds a guest with `CARGO_ENCODED_RUSTFLAGS` (its fixed flags plus
`[package.metadata.risc0] rustc-flags`, static strings), which overrides `RUSTFLAGS` and every config's rustflags,
and removes `CARGO_*` from the guest cargo's environment; `RUSTC_WRAPPER` (and the config file, found from the
methods crate's directory) still reach it. So the wrapper is the way in for flags that name the builder's paths.
"""
import hashlib
import os
import sys

TRIPLE = "riscv32im-risc0-zkvm-elf"
# rustc options that take their value as the next argument
VALUED = {"--crate-name", "--crate-type", "--edition", "--emit", "-C", "--codegen", "--cfg", "--check-cfg", "-L",
          "--extern", "--out-dir", "--target", "--error-format", "--json", "-o", "-A", "-W", "-D", "-F",
          "--cap-lints", "-Z", "--remap-path-prefix", "--print", "--sysroot", "-l", "--crate-attr", "--explain",
          "--force-warn", "--diagnostic-width", "--env-set"}


def die(msg: str) -> None:
    print(f"guest-rustc: {msg}", file=sys.stderr)
    sys.exit(101)


def norm(p: str) -> list:
    """The forms of a directory a build may name it by: as given (normalised) and with symlinks resolved."""
    a = os.path.normpath(os.path.abspath(p))
    return sorted({a, os.path.realpath(a)})


def main() -> None:
    argv = sys.argv[1:]
    tool = [argv[0]]
    args = argv[1:]
    # RUSTC_WORKSPACE_WRAPPER (clippy-driver) comes first and is given rustc's path
    if args and not args[0].startswith("-") and os.path.basename(args[0]).startswith("rustc") \
            and not args[0].endswith(".rs"):
        tool.append(args[0])
        args = args[1:]

    flags, positional = [], []  # (option, value, index of the value) and indices of positional arguments
    i = 0
    while i < len(args):
        a = args[i]
        if a in VALUED and i + 1 < len(args):
            flags.append((a, args[i + 1], i + 1))
            i += 2
            continue
        if a.startswith("--") and "=" in a:
            k, v = a.split("=", 1)
            flags.append((k, v, i))
        elif len(a) > 2 and a[:2] in ("-C", "-L", "-Z", "-l", "-o", "-A", "-W", "-D", "-F"):
            flags.append((a[:2], a[2:], i))
        elif a == "-" or not a.startswith("-"):
            positional.append(i)
        i += 1

    def values(opt: str) -> list:
        return [v for k, v, _ in flags if k == opt]

    if TRIPLE not in values("--target"):
        os.execvp(tool[0], tool + args)

    # 1. the remapping, general before specific: rustc applies the last mapping that matches
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    home = os.environ.get("HOME", "")
    pairs = []

    def add(frm: str, to: str) -> None:
        if frm:
            for f in norm(frm):
                if f != "/":
                    pairs.append((f, to))

    add(os.path.join(repo, "..", "upstream"), "/upstream")
    add(repo, "/ferrata")
    for h in (os.path.join(home, ".cargo") if home else "", os.environ.get("CARGO_HOME", "")):
        add(h, "/cargo")
    for h in (os.path.join(home, ".risc0") if home else "", os.environ.get("RISC0_HOME", "")):
        add(h, "/risc0")
    rustc = tool[-1]
    if os.path.isabs(rustc):
        sysroot = os.path.dirname(os.path.dirname(os.path.realpath(rustc)))
        if os.path.isdir(os.path.join(sysroot, "lib", "rustlib")):
            add(sysroot, "/rust")
    out_dir = (values("--out-dir") or [""])[-1]
    if f"/{TRIPLE}/" in out_dir:
        add(out_dir.split(f"/{TRIPLE}/")[0], "/target")

    def remap(p: str) -> str:
        if not os.path.isabs(p):
            return p  # cargo names a workspace member's source relative to the workspace root
        p = os.path.normpath(p)
        best = p
        for frm, to in pairs:
            if p == frm or p.startswith(frm + "/"):
                best = to + p[len(frm):]
        return best

    # 2. -C metadata without the builder's paths
    meta_at = [(j, v) for k, v, j in flags if k in ("-C", "--codegen") and v.startswith("metadata=")]
    extra = [v.split("=", 1)[1].lstrip("-") for k, v, _ in flags
             if k in ("-C", "--codegen") and v.startswith("extra-filename=")]
    if meta_at:
        srcs = [args[j] for j in positional]
        if len(srcs) != 1:
            die(f"expected one source file, got {srcs}")
        deps = []
        for e in values("--extern"):
            name, _, path = e.partition("=")
            ident = ""
            if path:
                stem = os.path.basename(path).rsplit(".", 1)[0]
                h = stem.rsplit("-", 1)[1] if "-" in stem else ""
                side = os.path.join(os.path.dirname(path), f".ferrata-meta-{h}")
                ident = open(side).read().strip() if h and os.path.isfile(side) else "file:" + os.path.basename(path)
            deps.append(f"{name}={ident}")
        key = "\0".join(["ferrata-guest-meta/1", (values("--crate-name") or [""])[0],
                         ",".join(sorted(values("--crate-type"))), ",".join(values("--edition")), TRIPLE,
                         remap(srcs[0])] + sorted(values("--cfg")) + sorted(deps))
        new = hashlib.sha256(key.encode()).hexdigest()[:16]
        for j, v in meta_at:  # the value ends the argument in every form (-C metadata=, -Cmetadata=, --codegen=...)
            args[j] = args[j][:len(args[j]) - len(v)] + "metadata=" + new
        if extra and out_dir:
            side = os.path.join(out_dir, f".ferrata-meta-{extra[-1]}")
            tmp = f"{side}.{os.getpid()}"
            with open(tmp, "w") as f:
                f.write(new + "\n")
            os.replace(tmp, side)

    remaps = []
    for frm, to in pairs:
        remaps += ["--remap-path-prefix", f"{frm}={to}"]
    os.execvp(tool[0], tool + args + remaps)


if __name__ == "__main__":
    main()
