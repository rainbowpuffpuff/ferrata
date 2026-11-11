#!/usr/bin/env bash
# Release engineering (A4, B4): one versioned, checksummed bundle of everything a testnet deployment needs.
#
#   scripts/release.sh build   OUT            build the programs, the vault and the CLI into OUT/build (build box only:
#                                              the guests are built at the crown build path, scripts/reproduce-image-ids.sh)
#   scripts/release.sh package OUT VERSION    assemble OUT/ferrata-VERSION/ (programs, vault, cli, MANIFEST.json,
#                                              SHA256SUMS) and OUT/ferrata-VERSION-x86_64-linux.tar.gz (+ .sha256)
#   scripts/release.sh verify  DIR            check a release directory: every checksum, the manifest's sizes and
#                                              hashes, and (where r0vm 3.0.5 is installed) every LEZ program's image id
#   scripts/release.sh record  DIR            copy MANIFEST.json and SHA256SUMS into releases/VERSION/ of this tree
#
# A release holds:
#   programs/   the six RISC Zero guests (ferrata_lez.bin and ferrata_bridge.bin: LEZ program ELFs; checkpoint,
#               checkpoint_b, segment, release: R0BF binaries as ferrata-e2e program-binary writes them), image ids
#   vault/      FerrataVault and its linked library FerrataRegistry: ABI, creation and runtime bytecode (the vault's with
#               solc's link placeholder), compiler settings; the deploy scripts and their parameters
#   cli/        the `ferrata` binary (x86_64-unknown-linux-gnu)
# Image ids of guests built without Docker depend on the build path (scripts/reproduce-image-ids.sh explains); the
# build therefore runs at the crown build path, so the ids are comparable with e2e/crown/README.md's table.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mode=${1:-}; shift || true

sha() { sha256sum "$1" | cut -c1-64; }

build_() {
  local out=$1
  mkdir -p "$out/build/logs"; out=$(cd "$out" && pwd)
  local log=$out/build/logs/steps.txt
  echo "$(date -u +%FT%TZ) release build of $(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)" > "$log"
  # 1. The guests, at the crown build path (writes out/build/guests/{programs,r0bf,ids.txt}).
  ( "$ROOT/scripts/reproduce-image-ids.sh" build --out "$out/build/guests" > "$out/build/logs/guests.log" 2>&1 ) & local g=$!
  # 2. The vault: forge build (solc 0.8.26, offline, vault/foundry.toml).
  ( cd "$ROOT/vault" && forge build --sizes > "$out/build/logs/vault.log" 2>&1 ) & local v=$!
  # 3. The CLI: its own workspace, Rust 1.98.1 (cli/rust-toolchain.toml), LEZ beside the repository.
  ( cd "$ROOT/cli" && CARGO_BUILD_JOBS=8 cargo build --release --locked > "$out/build/logs/cli.log" 2>&1 ) & local c=$!
  local fail=0
  if wait $g; then echo "$(date -u +%T) guests ok" >> "$log"; else echo "$(date -u +%T) guests FAILED" >> "$log"; fail=1; fi
  if wait $v; then echo "$(date -u +%T) vault ok" >> "$log"; else echo "$(date -u +%T) vault FAILED" >> "$log"; fail=1; fi
  if wait $c; then echo "$(date -u +%T) cli ok" >> "$log"; else echo "$(date -u +%T) cli FAILED" >> "$log"; fail=1; fi
  mkdir -p "$out/build/vault" "$out/build/cli"
  for a in FerrataVault FerrataRegistry; do cp "$ROOT/vault/out/$a.sol/$a.json" "$out/build/vault/" 2>/dev/null || fail=1; done
  cp "$ROOT/cli/target/release/ferrata" "$out/build/cli/ferrata" 2>/dev/null || fail=1
  {
    echo "rustc_host $(rustc --version)"
    echo "rustc_cli $(cd "$ROOT/cli" && rustc --version)"
    echo "r0vm $(r0vm --version 2>&1 | head -1)"
    echo "forge $(forge --version 2>&1 | head -1)"
    echo "solc 0.8.26 (vault/foundry.toml)"
    echo "lez $(git -C "$ROOT/../upstream/lez-v0.4.0-rc1" rev-parse HEAD 2>/dev/null || echo 1806e87)"
    echo "host $(uname -m) $(nproc) threads $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)"
  } > "$out/build/toolchains.txt"
  cat "$log"
  return $fail
}

package_() {
  local out=$1 ver=$2
  out=$(cd "$out" && pwd)
  local b=$out/build d=$out/ferrata-$ver
  [ -d "$b/guests/programs" ] || { echo "package: no build in $b (run build first)" >&2; return 1; }
  [ ! -e "$d" ] || { echo "package: $d exists; move it away first (a release is never overwritten)" >&2; return 1; }
  mkdir -p "$d/programs" "$d/vault" "$d/cli"
  cp "$b/guests/programs/ferrata_lez.bin" "$b/guests/programs/ferrata_bridge.bin" "$d/programs/"
  local f
  for f in checkpoint checkpoint_b segment release; do cp "$b/guests/r0bf/$f.bin" "$d/programs/$f.bin"; done
  cp "$b/guests/ids.txt" "$b/guests/r0bf/ids.jsonl" "$d/programs/"
  cp "$b/vault/FerrataVault.json" "$b/vault/FerrataRegistry.json" "$d/vault/"
  mkdir -p "$d/vault/script/params"
  cp "$ROOT/vault/script/DeployVault.s.sol" "$ROOT/vault/script/deploy.sh" "$d/vault/script/"
  cp "$ROOT/vault/script/params/"*.json "$d/vault/script/params/"
  cp "$b/cli/ferrata" "$d/cli/ferrata"
  cp "$ROOT/scripts/deploy-testnet.sh" "$d/" 2>/dev/null || true
  cp "$b/toolchains.txt" "$d/"
  python3 -I "$ROOT/scripts/release_manifest.py" make "$d" "$ver" "$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)" \
    || return 1
  ( cd "$d" && find . -type f ! -name SHA256SUMS | LC_ALL=C sort | sed 's|^\./||' | xargs sha256sum > SHA256SUMS )
  tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner --mtime='2026-11-17 00:00Z' \
    -czf "$out/ferrata-$ver-x86_64-linux.tar.gz" "ferrata-$ver"
  ( cd "$out" && sha256sum "ferrata-$ver-x86_64-linux.tar.gz" > "ferrata-$ver-x86_64-linux.tar.gz.sha256" )
  echo "release $d"; cat "$out/ferrata-$ver-x86_64-linux.tar.gz.sha256"
}

verify_() {
  local d=$1
  ( cd "$d" && sha256sum --quiet -c SHA256SUMS ) || { echo "verify: a checksum does not match" >&2; return 1; }
  python3 -I "$ROOT/scripts/release_manifest.py" check "$d" || return 1
  if command -v r0vm >/dev/null; then
    local f id want
    for f in ferrata_lez.bin ferrata_bridge.bin; do
      id=$(r0vm --elf "$d/programs/$f" --id 2>&1 | tail -n 1)
      want=$(python3 -I "$ROOT/scripts/release_manifest.py" id "$d" "$f")
      [ "${id#0x}" = "${want#0x}" ] || { echo "verify: $f image id $id, manifest $want" >&2; return 1; }
    done
    echo "verify: LEZ program image ids recomputed with r0vm: match"
  fi
  echo "verify: $d OK"
}

record_() {
  local d=$1 ver
  ver=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$d/MANIFEST.json")
  mkdir -p "$ROOT/releases/$ver"
  cp "$d/MANIFEST.json" "$d/SHA256SUMS" "$ROOT/releases/$ver/"
  echo "recorded releases/$ver/"
}

case "$mode" in
  build) build_ "${1:?OUT}" ;;
  package) package_ "${1:?OUT}" "${2:?VERSION}" ;;
  verify) verify_ "${1:?DIR}" ;;
  record) record_ "${1:?DIR}" ;;
  *) sed -n '2,12p' "$0"; exit 2 ;;
esac
