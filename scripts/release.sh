#!/usr/bin/env bash
# Release engineering (A4, B4): one versioned, checksummed bundle of everything a testnet deployment needs.
#
#   scripts/release.sh build   OUT            build the programs, the vault and the CLI into OUT/build (build box only:
#                                              the guests are built in place, path-independent ids, scripts/reproduce-image-ids.sh);
#                                              OUT/build/BUILD-OK is written only when every step passed
#   scripts/release.sh package OUT VERSION    from a build with BUILD-OK only: assemble OUT/ferrata-VERSION/ (programs, vault,
#                                              cli, licences and notices, MANIFEST.json, SHA256SUMS),
#                                              OUT/ferrata-VERSION-x86_64-linux.tar.gz and OUT/ferrata-VERSION-third-party-sources.tar
#                                              (each with its .sha256)
#   scripts/release.sh verify  DIR [--strict] check a release directory: every checksum, the manifest's sizes and hashes,
#                                              that its CLI is the binary check-build passed, the third-party notices
#                                              (native code and crates) against the release's lockfile and dependency
#                                              set, the crates in the binary's symbols (GNU nm), the sources asset
#                                              beside it, and (where r0vm 3.0.5 is installed) the six programs' image
#                                              ids; --strict fails when r0vm, nm or the sources asset is not there
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
#               script linked anything else; check-build.json (its record: the binary's sha256, the archives),
#               Cargo.lock, crates.tsv (the crates linked into the binary)
#   LICENSE-MIT, LICENSE-APACHE, THIRD-PARTY-NOTICES.md, THIRD-PARTY-CRATES.md, licenses/   Ferrata's licence; the
#               notices, licence texts and source offer for the native code in the binary (releases/third-party/);
#               the notices of every Rust crate in it and of the standard library (scripts/crate_notices.py, per build)
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
  local c st; c=$(git -C "$ROOT" rev-parse --verify -q HEAD 2>/dev/null) || { echo unknown; return; }
  st=$(git -C "$ROOT" status --porcelain --untracked-files=normal 2>/dev/null) || { echo "$c+dirty"; return; }
  if [ -z "$st" ]; then echo "$c"; else echo "$c+dirty"; fi
}

build_() {
  local out=$1
  mkdir -p "$out/build/logs"; out=$(cd "$out" && pwd)
  local b=$out/build log=$out/build/logs/steps.txt commit
  # A build directory is never half reused: an earlier build's BUILD-OK, CLI and notices go before anything starts.
  rm -rf "$b/BUILD-OK" "$b/cli" "$b/cli.partial"
  commit=$(commit_)
  echo "$(date -u +%FT%TZ) release build of $commit" > "$log"
  # 0. Before anything is built: the native archives the CLI links, fetched and checked against scripts/native-deps.json
  #    (a corrupted or replaced archive stops the build here), and the LEZ tree at the commit the release names.
  local native_env lez lez_commit
  native_env=$(python3 -I "$ROOT/scripts/fetch_native.py" env --dir "$NATIVE_DIR" 2> "$b/logs/native.log") \
    || { cat "$b/logs/native.log" >&2; echo "$(date -u +%T) native archives FAILED" >> "$log"; return 1; }
  echo "$(date -u +%T) native archives checked ($NATIVE_DIR)" >> "$log"
  lez=$ROOT/../upstream/lez-v0.4.0-rc1
  lez_commit=$(git -C "$lez" rev-parse --verify -q HEAD 2>/dev/null) \
    || { echo "build: cannot read the commit of $lez (scripts/fetch-upstream.sh)" | tee -a "$log" >&2; return 1; }
  [ "$lez_commit" = "$LEZ_COMMIT" ] \
    || { echo "build: $lez is at $lez_commit, not $LEZ_COMMIT" | tee -a "$log" >&2; return 1; }
  # The vault's Solidity libraries must be the pinned trees (vault/lib.lock): the release's bytecode is built from them.
  python3 -I "$ROOT/scripts/vault_libs.py" check > "$b/logs/vault-libs.log" 2>&1 \
    || { cat "$b/logs/vault-libs.log" >&2; echo "$(date -u +%T) vault libraries FAILED" >> "$log"; return 1; }
  # An inherited remap would change the guests' paths and so their image ids (scripts/guest-rustc.py).
  unset FERRATA_GUEST_REMAP_EXTRA
  # 1. The guests (writes b/guests/{programs,r0bf,ids.txt}).
  ( "$ROOT/scripts/reproduce-image-ids.sh" build --out "$b/guests" > "$b/logs/guests.log" 2>&1 ) & local g=$!
  # 2. The vault: forge build (solc 0.8.26, offline, vault/foundry.toml) into an output directory of the release's own
  #    (the sources' contracts only: test and script contracts are not part of a release; EIP-170 is checked on the
  #    release's two contracts by release_manifest.py).
  ( cd "$ROOT/vault" && forge build --skip test --skip script --out "$b/vault-out" \
      --cache-path "$b/vault-cache" > "$b/logs/vault.log" 2>&1 ) & local v=$!
  # 3. The CLI: its own workspace, Rust 1.98.1 (cli/rust-toolchain.toml), LEZ beside the repository, as cli/README.md
  #    builds it, --locked (cli/Cargo.lock is what cargo builds, or the build stops), in cli/target (an inherited
  #    CARGO_TARGET_DIR is dropped: check-build and the copy read cli/target). RAPIDSNARK_LIB_DIR and LBC_ROOT_DIR point
  #    the upstream build scripts at the checked archives; check-build then reads cargo's messages and fails unless
  #    cargo reported success and no build script linked native code from anywhere else; only then is the binary it
  #    checked copied (with its record, check-build.json) and the crate notices generated (scripts/crate_notices.py:
  #    crates.tsv, THIRD-PARTY-CRATES.md, the standard library's notices). b/cli appears only when all of that passed.
  ( unset CARGO_TARGET_DIR
    w=$b/cli.partial
    mkdir -p "$w" && cd "$ROOT/cli" && eval "$native_env" \
      && CARGO_BUILD_JOBS=8 cargo build --release --locked --message-format=json-render-diagnostics \
           > "$b/logs/cli-messages.jsonl" 2> "$b/logs/cli.log" \
      && python3 -I "$ROOT/scripts/fetch_native.py" check-build "$b/logs/cli-messages.jsonl" --dir "$NATIVE_DIR" \
           --target-dir "$ROOT/cli/target" --record cli --binary ferrata --copy-binary "$w/ferrata" \
           --out-json "$w/check-build.json" >> "$b/logs/cli.log" 2>&1 \
      && cp "$ROOT/cli/Cargo.lock" "$w/Cargo.lock" \
      && python3 -I "$ROOT/scripts/crate_notices.py" generate --cli "$ROOT/cli" --messages "$b/logs/cli-messages.jsonl" \
           --out "$w" >> "$b/logs/cli.log" 2>&1 \
      && mv "$w" "$b/cli" ) & local c=$!
  local fail=0 cli_ok=0
  if wait $g; then echo "$(date -u +%T) guests ok" >> "$log"; else echo "$(date -u +%T) guests FAILED" >> "$log"; fail=1; fi
  if wait $v; then echo "$(date -u +%T) vault ok" >> "$log"; else echo "$(date -u +%T) vault FAILED" >> "$log"; fail=1; fi
  if wait $c && [ -f "$b/cli/ferrata" ]; then
    echo "$(date -u +%T) cli ok" >> "$log"; cli_ok=1
  else
    echo "$(date -u +%T) cli FAILED (build, check-build or crate notices: $b/logs/cli.log)" >> "$log"; fail=1
    rm -rf "$b/cli" "$b/cli.partial"
  fi
  mkdir -p "$b/vault"
  for a in FerrataVault FerrataRegistry; do cp "$b/vault-out/$a.sol/$a.json" "$b/vault/" 2>/dev/null || fail=1; done
  {
    echo "rustc_host $(rustc --version)"
    echo "rustc_cli $(cd "$ROOT/cli" && rustc --version)"
    echo "r0vm $(r0vm --version 2>&1 | head -1)"
    echo "forge $(forge --version 2>&1 | head -1)"
    echo "solc 0.8.26 (vault/foundry.toml)"
    echo "lez $lez_commit"
    # The native archives as check-build verified them after the build (none when the CLI step failed).
    [ "$cli_ok" = 1 ] && python3 -I "$ROOT/scripts/fetch_native.py" record --check-build "$b/cli/check-build.json"
    echo "host $(uname -m) $(nproc) threads $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)"
  } > "$b/toolchains.txt"
  if [ "$fail" = 0 ]; then
    { echo "commit $commit"
      echo "finished_utc $(date -u +%FT%TZ)"
      echo "steps guests vault cli"
      echo "cli_sha256 $(sha "$b/cli/ferrata")"
      echo "check_build_sha256 $(sha "$b/cli/check-build.json")"
      echo "crate_notices_sha256 $(sha "$b/cli/THIRD-PARTY-CRATES.md")"
    } > "$b/BUILD-OK"
    echo "$(date -u +%T) BUILD-OK" >> "$log"
  else
    echo "$(date -u +%T) build FAILED: no BUILD-OK, and scripts/release.sh package refuses this build" >> "$log"
  fi
  cat "$log"
  return $fail
}

package_() {
  local out=$1 ver=$2
  out=$(cd "$out" && pwd)
  local b=$out/build d=$out/ferrata-$ver
  # Only a build whose every step passed is released, and only with the CLI binary that check-build passed.
  [ -f "$b/BUILD-OK" ] || { echo "package: $b has no BUILD-OK: its build failed or did not finish ($b/logs/steps.txt);" \
    "a release is made only from a build whose every step passed" >&2; return 1; }
  local f want got
  for f in cli/ferrata:cli_sha256 cli/check-build.json:check_build_sha256 cli/THIRD-PARTY-CRATES.md:crate_notices_sha256; do
    want=$(sed -n "s/^${f#*:} //p" "$b/BUILD-OK")
    got=$(sha "$b/${f%%:*}" 2>/dev/null) || got=missing
    [ -n "$want" ] && [ "$want" = "$got" ] || { echo "package: $b/${f%%:*} (sha256 $got) is not the one the build" \
      "checked (BUILD-OK: ${want:-none})" >&2; return 1; }
  done
  [ -d "$b/guests/programs" ] || { echo "package: no guests in $b" >&2; return 1; }
  [ ! -e "$d" ] || { echo "package: $d exists; move it away first (a release is never overwritten)" >&2; return 1; }
  mkdir -p "$d/programs" "$d/vault" "$d/cli"
  cp "$b/guests/programs/ferrata_lez.bin" "$b/guests/programs/ferrata_bridge.bin" "$d/programs/"
  for f in checkpoint checkpoint_b segment release; do cp "$b/guests/r0bf/$f.bin" "$d/programs/$f.bin"; done
  cp "$b/guests/ids.txt" "$b/guests/r0bf/ids.jsonl" "$d/programs/"
  cp "$b/vault/FerrataVault.json" "$b/vault/FerrataRegistry.json" "$d/vault/"
  mkdir -p "$d/vault/script/params"
  cp "$ROOT/vault/script/DeployVault.s.sol" "$ROOT/vault/script/deploy.sh" "$d/vault/script/"
  cp "$ROOT/vault/script/params/"*.json "$d/vault/script/params/"
  cp "$b/cli/ferrata" "$b/cli/Cargo.lock" "$b/cli/check-build.json" "$b/cli/crates.tsv" "$d/cli/"
  cp "$ROOT/scripts/deploy-testnet.sh" "$d/" 2>/dev/null || true
  cp "$b/toolchains.txt" "$d/"
  # Licences: Ferrata's own; the notices, texts and source offer for the native code in the CLI binary; the notices of
  # the crates in it and of the standard library (generated by the build).
  cp "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/releases/third-party/THIRD-PARTY-NOTICES.md" \
     "$b/cli/THIRD-PARTY-CRATES.md" "$d/"
  mkdir -p "$d/licenses" && cp "$ROOT/releases/third-party/licenses/"*.txt "$b/cli/rust-std-COPYRIGHT-library.html" "$d/licenses/"
  # The GMP sources the offer names, checked against scripts/native-deps.json, as an asset beside the release.
  local src=ferrata-$ver-third-party-sources
  python3 -I "$ROOT/scripts/fetch_native.py" sources --out "$out/$src" || return 1
  tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner --mode=u+rwX,go+rX,go-w --mtime='2026-01-01 00:00Z' \
    -cf "$out/$src.tar" "$src" || return 1
  python3 -I "$ROOT/scripts/release_manifest.py" make "$d" "$ver" "$(sed -n 's/^commit //p' "$b/BUILD-OK")" \
    "$out/$src.tar" "$b/BUILD-OK" || return 1
  ( cd "$d" && find . -type f ! -name SHA256SUMS | LC_ALL=C sort | sed 's|^\./||' | xargs sha256sum > SHA256SUMS )
  tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner --mode=u+rwX,go+rX,go-w --mtime='2026-01-01 00:00Z' \
    -czf "$out/ferrata-$ver-x86_64-linux.tar.gz" "ferrata-$ver"
  ( cd "$out" && sha256sum "ferrata-$ver-x86_64-linux.tar.gz" > "ferrata-$ver-x86_64-linux.tar.gz.sha256" \
      && sha256sum "$src.tar" > "$src.tar.sha256" )
  echo "release $d"; cat "$out/ferrata-$ver-x86_64-linux.tar.gz.sha256" "$out/$src.tar.sha256"
}

verify_() {
  local d=$1 strict=${2:-} s=()
  [ "$strict" = --strict ] && s=(--strict)
  ( cd "$d" && sha256sum --quiet -c SHA256SUMS ) || { echo "verify: a checksum does not match" >&2; return 1; }
  python3 -I "$ROOT/scripts/release_manifest.py" check "$d" "${s[@]}" || return 1
  python3 -I "$ROOT/scripts/crate_notices.py" binary-check "$d" "${s[@]}" || return 1
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
  *) sed -n '2,19p' "$0"; exit 2 ;;
esac
