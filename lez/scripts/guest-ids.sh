#!/usr/bin/env bash
# Build every guest of lez/ against one LEZ tag and write their image ids.
#
#   lez/scripts/guest-ids.sh --tag TAG --out DIR [--dry-run]
#
# Selects TAG with lez-select.sh (and selects the tag it found again at the end, also on failure), builds every
# methods crate of lez/ (`cargo build --locked`, through scripts/guest-rustc as .cargo/config.toml sets it, so the
# ids do not depend on where the tree, $HOME or the toolchains are) into DIR/target, and writes DIR/ids.tsv: guest,
# image id (risc0-build's, from methods.rs), sha256 and bytes of the R0BF binary, sha256 of the user ELF; the
# binaries go to DIR/bin/. --dry-run prints the cargo command and the list of methods crates, and builds nothing.
#
# Needs the LEZ tree of TAG beside the repository (scripts/fetch-upstream.sh) and a RISC Zero home with guest Rust
# 1.91.1 (the version of LEZ's own guest builds, its Justfile's r0.1.91.1, on every tag up to v0.4.0) in
# RISC0_HOME_LEZ, ../risc0-home-lez or ~/risc0-home-lez (scripts/ci/install-risc0.sh). The guests' RISC Zero crates
# are the tag's (risc0-zkvm 3.0.5 up to v0.4.0-rc3, 3.1.0 on v0.4.0, with risc0-build 3.0.5 or 3.0.7).
set -euo pipefail
lez="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="$(cd "$lez/.." && pwd)"
tag="" out="" dry=0
while [ $# -gt 0 ]; do
  case "$1" in
    --tag) tag="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --dry-run) dry=1; shift ;;
    *) echo "guest-ids.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$tag" ] && [ -n "$out" ] || { echo "usage: guest-ids.sh --tag TAG --out DIR [--dry-run]" >&2; exit 2; }
methods=()
for d in "$lez"/methods "$lez"/methods-*; do
  [ -f "$d/Cargo.toml" ] && grep -q '^\[package.metadata.risc0\]' "$d/Cargo.toml" \
    && methods+=("$(sed -n 's/^name = "\(.*\)"$/\1/p' "$d/Cargo.toml" | head -1)")
done
[ "${#methods[@]}" -gt 0 ] || { echo "guest-ids.sh: no methods crates under $lez" >&2; exit 1; }
cmd=(cargo build --locked)
for m in "${methods[@]}"; do cmd+=(-p "$m"); done
if [ "$dry" = 1 ]; then
  echo "tag: $tag"
  echo "methods: ${methods[*]}"
  echo "command: (cd lez && CARGO_TARGET_DIR=$out/target RISC0_HOME=<guest Rust 1.91.1> ${cmd[*]})"
  exit 0
fi

home="${RISC0_HOME_LEZ:-}"
if [ -z "$home" ]; then
  for d in "$repo/../risc0-home-lez" "$HOME/risc0-home-lez"; do [ -d "$d" ] && { home="$(cd "$d" && pwd)"; break; }; done
fi
[ -n "$home" ] || { echo "guest-ids.sh: no RISC Zero home with guest Rust 1.91.1 (set RISC0_HOME_LEZ)" >&2; exit 1; }
mkdir -p "$out/bin"; out="$(cd "$out" && pwd)"
from="$("$lez/scripts/lez-select.sh" --show)"
restore() { "$lez/scripts/lez-select.sh" "$from" > /dev/null; }
trap restore EXIT
"$lez/scripts/lez-select.sh" "$tag"
# The wrapper makes the ids independent of the build paths: never let an inherited one, or inherited flags, replace it.
unset RUSTC_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS FERRATA_GUEST_REMAP_EXTRA
export RISC0_HOME="$home" RISC0_BUILD_LOCKED=1 CARGO_TERM_COLOR=never CARGO_TARGET_DIR="$out/target"
echo "guest-ids: LEZ $tag, $(cargo --version), RISC0_HOME $home ($(ls "$home/toolchains" | tr '\n' ' '))"
(cd "$lez" && "${cmd[@]}")
python3 -I - "$out" <<'EOF'
import hashlib, re, shutil, sys
from pathlib import Path
out = Path(sys.argv[1])
rows = {}
for m in sorted((out / "target").glob("*/build/methods*-*/out/methods.rs"), key=lambda p: p.stat().st_mtime):
    src = m.read_text()
    for name, path in re.findall(r'pub const ([A-Z0-9_]+)_PATH: &str = "([^"]+)";', src):
        words = re.search(rf"pub const {name}_ID: \[u32; 8\] = \[([^\]]+)\];", src)
        binary = Path(path)
        if not words or not binary.exists():
            continue
        gid = "0x" + "".join(int(w).to_bytes(4, "little").hex() for w in words.group(1).split(","))
        data = binary.read_bytes()
        elf = binary.with_suffix("")
        elf_sha = hashlib.sha256(elf.read_bytes()).hexdigest() if elf.exists() else "-"
        rows[name.lower()] = (gid, hashlib.sha256(data).hexdigest(), len(data), elf_sha)
        shutil.copy2(binary, out / "bin" / binary.name)
with open(out / "ids.tsv", "w") as f:
    f.write("guest\timage_id\tbinary_sha256\tbinary_bytes\telf_sha256\n")
    for name, (gid, sha, size, elf_sha) in sorted(rows.items()):
        f.write(f"{name}\t{gid}\t{sha}\t{size}\t{elf_sha}\n")
print(f"guest-ids: {len(rows)} guests -> {out / 'ids.tsv'}")
EOF
