#!/usr/bin/env bash
# Stage ferrata_updater_module's sources into its crate directory (rust-lib/vendor/, ignored by git).
#
# The updater links LEZ v0.4.0-rc1's state machine, and for its in-process chain (feature `harness`,
# the chain its tests run on) the lez/ harness, which embeds the guests risc0-build makes. The module
# builder copies only rust-lib/ and runs no RISC Zero toolchain, so:
#
#   vendor/ferrata/                the repository's crates the updater needs, in the repository's own
#                                  layout, with the workspace manifests they inherit from (the root's,
#                                  bridge/'s, lez/'s), so every path dependency resolves unchanged:
#                                  lez/{updater,updater/sequencer,advance,wire,bridge-wire,holdings-wire,harness}
#                                  (advance: the update's job planning the updater shares with
#                                  ferrata_attest_module; updater/sequencer: the backend for a running LEZ
#                                  sequencer, a library the module links, with the live driver's modules it
#                                  compiles in by path, lez/live/driver/src/),
#                                  crates/{ferrata-core,ferrata-spectests}, bridge/crates/ferrata-bridge-core,
#                                  checkpoint/crates/{checkpoint-core,attestor-core} (a dev-dependency's path),
#                                  and the files the harness includes at compile time (SHA256SUMS of the
#                                  fixtures, bridge/vectors/inbound-v2.json)
#   vendor/ferrata/lez/methods/    a stand-in for lez/methods with no build script: the same constants
#                                  (`*_ELF`, `*_ID`), the ELF bytes copied from a risc0-build output of
#                                  lez/methods (the guests are not changed; their ids are recorded in
#                                  vendor/STAGED.json and checked against docs/image-ids.md by check-daemons.sh)
#   vendor/upstream/lez-v0.4.0-rc1/  the LEZ tree beside the repository (without target/ and .git),
#                                  whose artifacts/ LEZ's own build scripts embed
#   vendor/lbc/                    the prebuilt artifact of logos-blockchain-circuits that LEZ's node-side crates
#                                  (the sequencer backend's) link and embed, the release their version names
#                                  (rust-lib/.cargo/config.toml points LBC_ROOT_DIR at it; otherwise their build
#                                  scripts download it into the user's cache: no network inside Nix's sandbox).
#                                  Taken from that cache when it is there, else from the circuits' GitHub release
#   vendor/rapidsnark/             rapidsnark's static libraries (RAPIDSNARK_LIB_DIR), the release rust-rapidsnark's
#                                  build script would download (logos-blockchain-rust-rapidsnark, rapidsnark-pic-v0.0.8)
#
# Needs the guests built (`cargo build --locked -p methods` in lez/, RISC0_HOME as lez/RESULTS.md says).
# METHODS_RS=path picks the risc0-build output (methods.rs); default: the newest under lez/target.
#
#   modules/scripts/stage-updater.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
spike=$(cd "$repo/.." && pwd)
lez_up="$spike/upstream/lez-v0.4.0-rc1"
commit=${FERRATA_COMMIT:-$(git -C "$repo" rev-parse --verify -q HEAD 2>/dev/null || echo unknown)}
# Portable to macOS's userland (no find -printf; sha256sum where it exists, else shasum; bash 3.2). What it stages
# is not: the circuits' and rapidsnark's prebuilt artifacts below are their linux-x86_64 releases, so the module
# it stages builds for x86-64 Linux only (.github/workflows/macos.yml leaves it out).
if command -v sha256sum > /dev/null; then SHA256=(sha256sum); else SHA256=(shasum -a 256); fi
r="$repo/modules/ferrata_updater_module/rust-lib"
v="$r/vendor"

methods_rs=${METHODS_RS:-$(python3 -I -c 'import glob, os, sys
found = glob.glob(os.path.join(sys.argv[1], "**", "build", "methods-*", "out", "methods.rs"), recursive=True)
print(max(found, key=os.path.getmtime) if found else "")' "$repo/lez/target")}
if [ -z "$methods_rs" ] || [ ! -f "$methods_rs" ]; then
  echo "stage-updater.sh: no risc0-build output of lez/methods (build it: cd lez && cargo build --locked -p methods)" >&2
  exit 1
fi
[ -d "$lez_up/lee" ] || { echo "stage-updater.sh: $lez_up missing" >&2; exit 1; }

rm -rf "$v"
mkdir -p "$v/ferrata" "$v/upstream"

copy() { # SOURCE_DIR (relative to the repository): its Cargo.toml, README.md, build.rs and src/
  local src="$repo/$1" dst="$v/ferrata/$1"
  mkdir -p "$dst"
  cp -r "$src/Cargo.toml" "$src/src" "$dst/"
  for f in README.md build.rs; do if [ -f "$src/$f" ]; then cp "$src/$f" "$dst/"; fi; done
}
crates="lez/updater lez/updater/sequencer lez/advance lez/wire lez/bridge-wire lez/holdings-wire lez/harness crates/ferrata-core crates/ferrata-spectests
        bridge/crates/ferrata-bridge-core checkpoint/crates/checkpoint-core checkpoint/crates/attestor-core"
for c in $crates; do copy "$c"; done
# The live driver's modules the sequencer backend compiles in by path (lez/updater/sequencer/src/lib.rs).
mkdir -p "$v/ferrata/lez/live/driver/src"
for f in blocklog follower live manifest plan programs; do cp "$repo/lez/live/driver/src/$f.rs" "$v/ferrata/lez/live/driver/src/"; done
# The workspace manifests the copies inherit from.
cp "$repo/Cargo.toml" "$v/ferrata/Cargo.toml"
cp "$repo/lez/Cargo.toml" "$v/ferrata/lez/Cargo.toml"
cp "$repo/bridge/Cargo.toml" "$v/ferrata/bridge/Cargo.toml"
cp "$repo/checkpoint/Cargo.toml" "$v/ferrata/checkpoint/Cargo.toml"
# Files the harness includes when it compiles.
for f in $(cd "$repo" && ls crates/ferrata-spectests/fixtures/*/SHA256SUMS) bridge/vectors/inbound-v2.json; do
  mkdir -p "$(dirname "$v/ferrata/$f")"
  cp "$repo/$f" "$v/ferrata/$f"
done

# The circuits' prebuilt artifact (rust-lib/.cargo/config.toml: LBC_ROOT_DIR), at the version of the circuits crates
# the sequencer backend links (its lock), kept in the cache their build scripts use.
lbc_version=$(awk '/^name = "logos-blockchain-circuits-common"$/ {getline; gsub(/version = |"/, ""); print; exit}' "$repo/lez/updater/sequencer/Cargo.lock")
lbc="logos-blockchain-circuits-v$lbc_version-linux-x86_64"
grep -q "\"vendor/lbc/$lbc\"" "$r/.cargo/config.toml" || { echo "stage-updater.sh: rust-lib/.cargo/config.toml does not name $lbc" >&2; exit 1; }
lbc_cache="${XDG_CACHE_HOME:-$HOME/.cache}/logos/blockchain"
if [ ! -d "$lbc_cache/$lbc" ]; then
  mkdir -p "$lbc_cache"
  curl -fsSL -A ferrata-build -o "$lbc_cache/$lbc.tar.gz.part" \
    "https://github.com/logos-blockchain/logos-blockchain-circuits/releases/download/v$lbc_version/$lbc.tar.gz"
  tar -xzf "$lbc_cache/$lbc.tar.gz.part" -C "$lbc_cache"
  mv "$lbc_cache/$lbc.tar.gz.part" "$lbc_cache/$lbc.tar.gz"
fi
mkdir -p "$v/lbc"
cp -r "$lbc_cache/$lbc" "$v/lbc/"
lbc_sum=$( (cd "$v/lbc" && find "$lbc" -type f | LC_ALL=C sort | xargs "${SHA256[@]}") | "${SHA256[@]}" | cut -d' ' -f1)
# rapidsnark's static libraries, which logos-blockchain-circuits-prover links (static-rapidsnark): rust-rapidsnark's
# build script reads ../RAPIDSNARK_VERSION from its git checkout and downloads them, unless RAPIDSNARK_LIB_DIR names
# them (rust-lib/.cargo/config.toml); Nix vendors each crate alone, without that file. The version is the one of the
# pinned rust-rapidsnark rev (e91187f: 0.0.8), the release its script downloads for x86_64 Linux.
rs_version=0.0.8
rs="rapidsnark-linux-x86_64-pic-v$rs_version"
if [ ! -d "$lbc_cache/$rs" ]; then
  curl -fsSL -A ferrata-build -o "$lbc_cache/$rs.zip.part" \
    "https://github.com/logos-blockchain/logos-blockchain-rust-rapidsnark/releases/download/rapidsnark-pic-v$rs_version/$rs.zip"
  (cd "$lbc_cache" && unzip -q -o "$rs.zip.part" && mv "$rs.zip.part" "$rs.zip")
fi
mkdir -p "$v/rapidsnark"
cp "$lbc_cache/$rs/lib/"*.a "$v/rapidsnark/"
rs_sum=$( (cd "$v/rapidsnark" && ls | LC_ALL=C sort | xargs "${SHA256[@]}") | "${SHA256[@]}" | cut -d' ' -f1)

# The LEZ tree.
tar -C "$spike/upstream" --exclude='lez-v0.4.0-rc1/target' --exclude='lez-v0.4.0-rc1/.git' -cf - lez-v0.4.0-rc1 | tar -C "$v/upstream" -xf -

# lez/methods, without its build script: the guests risc0-build made, as constants of the same names.
m="$v/ferrata/lez/methods"
mkdir -p "$m/src" "$m/guests"
cat > "$m/Cargo.toml" <<'EOF'
# A stand-in for lez/methods in ferrata_updater_module (modules/scripts/stage-updater.sh): the guests
# risc0-build made for lez/methods, embedded under the same names, without the build script (the
# module builder runs no RISC Zero toolchain).
[package]
name = "methods"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
publish = false
EOF
python3 -I - "$methods_rs" "$m" "$v/ids.json" <<'PY'
import json, os, re, shutil, struct, sys
src, out, ids_out = sys.argv[1:4]
text = open(src).read()
lines, ids = ["//! The guests of lez/methods as risc0-build embedded them (modules/scripts/stage-updater.sh).", ""], {}
for m in re.finditer(r'pub const ([A-Z0-9_]+)_ELF: &\[u8\] = include_bytes!\("([^"]+)"\);', text):
    name, path = m.group(1), m.group(2)
    fname = name.lower() + ".bin"
    shutil.copyfile(path, os.path.join(out, "guests", fname))
    idm = re.search(r'pub const ' + name + r'_ID: \[u32; 8\] = \[([^\]]+)\];', text)
    words = [int(w.strip()) for w in idm.group(1).split(",") if w.strip()]
    ids[name.lower()] = "0x" + struct.pack("<8I", *words).hex()
    lines.append(f'pub const {name}_ELF: &[u8] = include_bytes!("../guests/{fname}");')
    lines.append(f'pub const {name}_ID: [u32; 8] = {words};')
if not ids:
    sys.exit(f"no guests found in {src}")
open(os.path.join(out, "src", "lib.rs"), "w").write("\n".join(lines) + "\n")
json.dump(ids, open(ids_out, "w"), indent=1, sort_keys=True)
print("stage-updater.sh: guests " + ", ".join(f"{k} {v[:12]}…" for k, v in sorted(ids.items())))
PY

# What was staged, for version().
sum=$( (cd "$repo" && { for c in $crates; do find "$c/Cargo.toml" "$c/src" -type f; done
                         for f in blocklog follower live manifest plan programs; do echo "lez/live/driver/src/$f.rs"; done; } \
        | LC_ALL=C sort | xargs "${SHA256[@]}") | "${SHA256[@]}" | cut -d' ' -f1)
lez_rev=$(git -C "$lez_up" rev-parse --short HEAD 2>/dev/null || echo "1806e87 (lez/RESULTS.md)")
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/lez/updater/Cargo.toml" | head -n 1)
python3 -I - "$v/STAGED.json" "$version" "$commit" "$sum" "$lez_rev" "$v/ids.json" "$lbc" "$lbc_sum" "$rs" "$rs_sum" <<'PY'
import json, sys
out, version, commit, s, lez, ids, lbc, lbc_sum, rs, rs_sum = sys.argv[1:11]
json.dump({"version": version, "commit": commit, "source_sha256": s, "lez": "v0.4.0-rc1 " + lez,
           "guests": json.load(open(ids)), "circuits": {"artifact": lbc, "files_sha256": lbc_sum},
           "rapidsnark": {"artifact": rs, "files_sha256": rs_sum}},
          open(out, "w"), sort_keys=True)
PY
echo "stage-updater.sh: ferrata-updater $version into ferrata_updater_module, commit $commit, sources sha256 $sum, LEZ $lez_rev ($(du -sh "$v" | cut -f1))"
