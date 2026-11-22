#!/usr/bin/env bash
# Release engineering (A4, B4): one versioned, checksummed bundle of everything a testnet deployment needs.
#
#   scripts/release.sh build   OUT            build the programs, the vault and the CLI into OUT/build (build box only:
#                                              the guests are built in place, path-independent ids, scripts/reproduce-image-ids.sh)
#   scripts/release.sh package OUT VERSION    assemble OUT/ferrata-VERSION/ (programs, vault, cli, licences and notices,
#                                              MANIFEST.json, SHA256SUMS), OUT/ferrata-VERSION-x86_64-linux.tar.gz and
#                                              OUT/ferrata-VERSION-third-party-sources.tar (each with its .sha256)
#   scripts/release.sh verify  DIR [--strict] check a release directory: every checksum, the manifest's sizes and
#                                              hashes, the third-party notices, and (where r0vm 3.0.5 is installed) the
#                                              six programs' image ids; --strict fails when r0vm is not there
#   scripts/release.sh record  DIR            copy MANIFEST.json and SHA256SUMS into releases/VERSION/ of this tree
#   scripts/release.sh commit                 the commit a build records (FERRATA_COMMIT, else HEAD, "+dirty" when
#                                              a tracked file differs or an untracked one exists)
#
# A release holds:
#   programs/   the six RISC Zero guests (ferrata_lez.bin and ferrata_bridge.bin: LEZ program ELFs; checkpoint,
#               checkpoint_b, segment, release: R0BF binaries as ferrata-e2e program-binary writes them), image ids
#   vault/      FerrataVault and its linked library FerrataRegistry: ABI, creation and runtime bytecode (the vault's with
#               solc's link placeholder), compiler settings; the deploy scripts and their parameters
#   cli/        the `ferrata` binary (x86_64-unknown-linux-gnu), built --locked; the native code it links statically
#               (rapidsnark, GMP, the Logos circuits) comes from the archives scripts/native-deps.json pins, fetched
#               and checked by scripts/fetch_native.py, and `fetch_native.py check-build` fails the build if a build
#               script linked anything else
#   LICENSE-MIT, LICENSE-APACHE, THIRD-PARTY-NOTICES.md, licenses/   Ferrata's licence, and the notices, licence texts
#               and source offer for the third-party code in the binary (releases/third-party/)
#   beside the directory: ferrata-VERSION-third-party-sources.tar, the GMP sources of that offer (checked)
# Guest builds go through scripts/guest-rustc (.cargo/config.toml), so their image ids do not depend on where the tree
# is (scripts/reproduce-image-ids.sh explains) and are comparable with e2e/crown/README.md's table and docs/image-ids.md.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mode=${1:-}; shift || true
# The LEZ release the programs and the CLI are built against (../upstream/lez-v0.4.0-rc1, scripts/fetch-upstream.sh).
LEZ_COMMIT=1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21
NATIVE_DIR=${FERRATA_NATIVE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/ferrata/native}

sha() { sha256sum "$1" | cut -c1-64; }
# The commit built: FERRATA_COMMIT where the tree is a copy without .git (the build box), else git's HEAD, with
# "+dirty" when a tracked file differs from it or an untracked file (not ignored) exists: cargo and the box's sync use
# untracked files too (a build.rs, a src/bin/*.rs, a rust-toolchain.toml).
commit_() {
  if [ -n "${FERRATA_COMMIT:-}" ]; then echo "$FERRATA_COMMIT"; return; fi
  local c st; c=$(git -C "$ROOT" rev-parse --verify -q HEAD) || { echo unknown; return; }
  st=$(git -C "$ROOT" status --porcelain --untracked-files=normal 2>/dev/null) || { echo "$c+dirty"; return; }
  if [ -z "$st" ]; then echo "$c"; else echo "$c+dirty"; fi
}

build_() {
  local out=$1
  mkdir -p "$out/build/logs"; out=$(cd "$out" && pwd)
  local log=$out/build/logs/steps.txt
  echo "$(date -u +%FT%TZ) release build of $(commit_)" > "$log"
  # 0. Before anything is built: the native archives the CLI links, fetched and checked against scripts/native-deps.json
  #    (a corrupted or replaced archive stops the build here), and the LEZ tree at the commit the release names.
  local native_env lez lez_commit
  native_env=$(python3 -I "$ROOT/scripts/fetch_native.py" env --dir "$NATIVE_DIR" 2> "$out/build/logs/native.log") \
    || { cat "$out/build/logs/native.log" >&2; echo "$(date -u +%T) native archives FAILED" >> "$log"; return 1; }
  echo "$(date -u +%T) native archives checked ($NATIVE_DIR)" >> "$log"
  lez=$ROOT/../upstream/lez-v0.4.0-rc1
  lez_commit=$(git -C "$lez" rev-parse --verify -q HEAD 2>/dev/null) \
    || { echo "build: cannot read the commit of $lez (scripts/fetch-upstream.sh)" | tee -a "$log" >&2; return 1; }
  [ "$lez_commit" = "$LEZ_COMMIT" ] \
    || { echo "build: $lez is at $lez_commit, not $LEZ_COMMIT" | tee -a "$log" >&2; return 1; }
  # An inherited remap would change the guests' paths and so their image ids (scripts/guest-rustc.py).
  unset FERRATA_GUEST_REMAP_EXTRA
  # 1. The guests (writes out/build/guests/{programs,r0bf,ids.txt}).
  ( "$ROOT/scripts/reproduce-image-ids.sh" build --out "$out/build/guests" > "$out/build/logs/guests.log" 2>&1 ) & local g=$!
  # 2. The vault: forge build (solc 0.8.26, offline, vault/foundry.toml) into an output directory of the release's own
  #    (the sources' contracts only: test and script contracts are not part of a release; EIP-170 is checked on the
  #    release's two contracts by release_manifest.py).
  ( cd "$ROOT/vault" && forge build --skip test --skip script --out "$out/build/vault-out" \
      --cache-path "$out/build/vault-cache" > "$out/build/logs/vault.log" 2>&1 ) & local v=$!
  # 3. The CLI: its own workspace, Rust 1.98.1 (cli/rust-toolchain.toml), LEZ beside the repository, as cli/README.md
  #    builds it, --locked (cli/Cargo.lock is what cargo builds, or the build stops). RAPIDSNARK_LIB_DIR and
  #    LBC_ROOT_DIR point the upstream build scripts at the checked archives; check-build then reads cargo's
  #    build-script messages and fails if any build script linked native code from anywhere else.
  ( cd "$ROOT/cli" && eval "$native_env" \
      && CARGO_BUILD_JOBS=8 cargo build --release --locked --message-format=json-render-diagnostics \
           > "$out/build/logs/cli-messages.jsonl" 2> "$out/build/logs/cli.log" \
      && python3 -I "$ROOT/scripts/fetch_native.py" check-build "$out/build/logs/cli-messages.jsonl" --dir "$NATIVE_DIR" \
           --target-dir "$ROOT/cli/target" >> "$out/build/logs/cli.log" 2>&1 ) & local c=$!
  local fail=0
  if wait $g; then echo "$(date -u +%T) guests ok" >> "$log"; else echo "$(date -u +%T) guests FAILED" >> "$log"; fail=1; fi
  if wait $v; then echo "$(date -u +%T) vault ok" >> "$log"; else echo "$(date -u +%T) vault FAILED" >> "$log"; fail=1; fi
  if wait $c; then echo "$(date -u +%T) cli ok" >> "$log"; else echo "$(date -u +%T) cli FAILED" >> "$log"; fail=1; fi
  mkdir -p "$out/build/vault" "$out/build/cli"
  for a in FerrataVault FerrataRegistry; do cp "$out/build/vault-out/$a.sol/$a.json" "$out/build/vault/" 2>/dev/null || fail=1; done
  cp "$ROOT/cli/target/release/ferrata" "$out/build/cli/ferrata" 2>/dev/null || fail=1
  cp "$ROOT/cli/Cargo.lock" "$out/build/cli/Cargo.lock" 2>/dev/null || fail=1
  {
    echo "rustc_host $(rustc --version)"
    echo "rustc_cli $(cd "$ROOT/cli" && rustc --version)"
    echo "r0vm $(r0vm --version 2>&1 | head -1)"
    echo "forge $(forge --version 2>&1 | head -1)"
    echo "solc 0.8.26 (vault/foundry.toml)"
    echo "lez $lez_commit"
    python3 -I "$ROOT/scripts/fetch_native.py" record
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
  cp "$b/cli/ferrata" "$b/cli/Cargo.lock" "$d/cli/"
  cp "$ROOT/scripts/deploy-testnet.sh" "$d/" 2>/dev/null || true
  cp "$b/toolchains.txt" "$d/"
  # Licences: Ferrata's own, and the notices, texts and source offer for the third-party code in the CLI binary.
  cp "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/releases/third-party/THIRD-PARTY-NOTICES.md" "$d/"
  mkdir -p "$d/licenses" && cp "$ROOT/releases/third-party/licenses/"*.txt "$d/licenses/"
  # The GMP sources the offer names, checked against scripts/native-deps.json, as an asset beside the release.
  local src=ferrata-$ver-third-party-sources
  python3 -I "$ROOT/scripts/fetch_native.py" sources --out "$out/$src" || return 1
  tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner --mtime='2026-01-01 00:00Z' -cf "$out/$src.tar" "$src" \
    || return 1
  python3 -I "$ROOT/scripts/release_manifest.py" make "$d" "$ver" "$(commit_)" "$out/$src.tar" \
    || return 1
  ( cd "$d" && find . -type f ! -name SHA256SUMS | LC_ALL=C sort | sed 's|^\./||' | xargs sha256sum > SHA256SUMS )
  tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner --mtime='2026-01-01 00:00Z' \
    -czf "$out/ferrata-$ver-x86_64-linux.tar.gz" "ferrata-$ver"
  ( cd "$out" && sha256sum "ferrata-$ver-x86_64-linux.tar.gz" > "ferrata-$ver-x86_64-linux.tar.gz.sha256" \
      && sha256sum "$src.tar" > "$src.tar.sha256" )
  echo "release $d"; cat "$out/ferrata-$ver-x86_64-linux.tar.gz.sha256" "$out/$src.tar.sha256"
}

verify_() {
  local d=$1 strict=${2:-}
  ( cd "$d" && sha256sum --quiet -c SHA256SUMS ) || { echo "verify: a checksum does not match" >&2; return 1; }
  python3 -I "$ROOT/scripts/release_manifest.py" check "$d" || return 1
  if command -v r0vm >/dev/null; then
    local f id want
    for f in ferrata_lez.bin ferrata_bridge.bin checkpoint.bin checkpoint_b.bin segment.bin release.bin; do
      id=$(r0vm --elf "$d/programs/$f" --id 2>&1 | tail -n 1)
      want=$(python3 -I "$ROOT/scripts/release_manifest.py" id "$d" "$f")
      [ "${id#0x}" = "${want#0x}" ] || { echo "verify: $f image id $id, manifest $want" >&2; return 1; }
    done
    echo "verify: the six programs' image ids recomputed with r0vm: match"
  elif [ "$strict" = --strict ]; then
    echo "verify: r0vm not found: the image ids were not recomputed (--strict)" >&2; return 1
  else
    echo "verify: image ids NOT recomputed (r0vm 3.0.5 not installed); checksums, manifest and notices only"
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
  verify) verify_ "${1:?DIR}" "${2:-}" ;;
  record) record_ "${1:?DIR}" ;;
  commit) commit_ ;;
  *) sed -n '2,14p' "$0"; exit 2 ;;
esac
