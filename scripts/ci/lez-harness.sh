#!/usr/bin/env bash
# CI's LEZ suites (job `lez-harness` of .github/workflows/ci.yml and job `lez-harness-full` of
# .github/workflows/lez-full.yml; docs/guides/ci.md): the attestation and bridge
# programs driven through LEZ v0.4.0-rc1's state machine and fee settlement in the harness, every target of
# `lez/harness` and `lez/updater` that `cargo test -p` runs without an optional feature (lib unit tests, bin unit
# tests, integration test binaries including red-team suites, doc-tests), updater-gate's daemon process demo, and the
# privacy suite end to end
# (scripts/privacy/run.sh: the LEZ half, the vault's PrivacySuite, the adversary) with its gate
# (scripts/privacy/check-results.py: the differential gate, chance-level pairing, nothing hidden found, recovery from
# the seed, the published tables reproduced) and its negative controls (scripts/privacy/controls.py). The guests run in
# RISC Zero dev mode: executed in the zkVM, no proof.
# Nothing is sent to any network but the dependency fetches of cargo and forge.
#
# Needs: Rust 1.98.1 (lez/rust-toolchain.toml), RISC Zero's rzup toolchains with guest Rust 1.91.1 in
# $RISC0_HOME_LEZ or ../risc0-home-lez or ~/risc0-home-lez (scripts/ci/install-risc0.sh), the upstream sources beside
# the repository (scripts/fetch-upstream.sh); for the privacy suite, Foundry with vault/lib installed.
#
#   scripts/ci/lez-harness.sh                     build what is missing, then run all suites
#   scripts/ci/lez-harness.sh build               build all suites only
#   scripts/ci/lez-harness.sh --shard NAME        check shard map, then run shard NAME
#   scripts/ci/lez-harness.sh --shard NAME build  build shard NAME only
#   scripts/ci/lez-harness.sh --dry-run [--shard NAME] [build]
#                                                 print the exact commands for NAME (or every shard)
#
# FERRATA_TEST_THREADS (default 4, the cores of GitHub's hosted runner) sets the test threads. FERRATA_LEZ_TAG (job
# `lez-040`) selects another LEZ tag first (lez/scripts/lez-select.sh).
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.foundry/bin:$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
# No debug info and no incremental artefacts, as e2e/ci/run.sh: smaller target directories on the runner's disk.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}" CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
threads="${FERRATA_TEST_THREADS:-4}"
privacy_out="${FERRATA_PRIVACY_OUT:-${RUNNER_TEMP:-$repo/../tmp}/ci-privacy}"

shard=""
has_shard=0
dry_run=0
build_only=0
argv=("$@")
i=0
while [ "$i" -lt "${#argv[@]}" ]; do
  a="${argv[$i]}"
  case "$a" in
    --shard)
      [ "$has_shard" = 0 ] || { echo "lez-harness.sh: --shard given more than once" >&2; exit 2; }
      i=$((i + 1))
      [ "$i" -lt "${#argv[@]}" ] || { echo "lez-harness.sh: --shard needs a shard name" >&2; exit 2; }
      shard="${argv[$i]}"
      has_shard=1
      i=$((i + 1))
      ;;
    --shard=*)
      [ "$has_shard" = 0 ] || { echo "lez-harness.sh: --shard given more than once" >&2; exit 2; }
      shard="${a#--shard=}"
      has_shard=1
      i=$((i + 1))
      ;;
    --dry-run)
      dry_run=1
      i=$((i + 1))
      ;;
    build)
      build_only=1
      i=$((i + 1))
      ;;
    *)
      echo "lez-harness.sh: unknown argument: $a" >&2
      exit 2
      ;;
  esac
done

if [ "$has_shard" = 1 ] && [ -z "$shard" ]; then
  echo "lez-harness.sh: --shard requires a non-empty shard name" >&2
  exit 2
fi

if [ "$dry_run" = 1 ]; then
  args=(--dry-run)
  [ "$has_shard" = 1 ] && args+=("$shard")
  [ "$build_only" = 1 ] && args+=(--build-only)
  args+=(--threads "$threads" --privacy-out "$privacy_out")
  python3 -I "$repo/scripts/ci/check-lez-shards.py" "${args[@]}"
  exit 0
fi

lez_home="${RISC0_HOME_LEZ:-}"
if [ -z "$lez_home" ]; then
  for d in "$repo/../risc0-home-lez" "$HOME/risc0-home-lez"; do [ -d "$d" ] && { lez_home="$(cd "$d" && pwd)"; break; }; done
fi
[ -n "$lez_home" ] || { echo "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ)" >&2; exit 1; }
# The guests are built with guest Rust 1.91.1, the version of LEZ's own guest builds (lez/RESULTS.md section 6).
export RISC0_HOME="$lez_home"
t0=$(date +%s)
stamp() { echo "[$(( $(date +%s) - t0 )) s] $*"; }
# FERRATA_LEZ_TAG (job `lez-040`: v0.4.0): the LEZ tag the suites run against, selected first with
# lez/scripts/lez-select.sh; unset (the release candidates' jobs), the committed v0.4.0-rc1.
if [ -n "${FERRATA_LEZ_TAG:-}" ]; then
  stamp "LEZ $FERRATA_LEZ_TAG (lez/scripts/lez-select.sh)"
  "$repo/lez/scripts/lez-select.sh" "$FERRATA_LEZ_TAG"
  selected="$("$repo/lez/scripts/lez-select.sh" --show)"
  [ "$selected" = "$FERRATA_LEZ_TAG" ] || { echo "lez-harness.sh: the manifests are on $selected, not $FERRATA_LEZ_TAG" >&2; exit 1; }
fi

if [ "$has_shard" = 1 ]; then
  stamp "guard: check scripts/ci/lez-harness-shards.json against lez/harness and lez/updater"
  python3 -I "$repo/scripts/ci/check-lez-shards.py"
  plan_args=(--emit-tsv "$shard" --threads "$threads" --privacy-out "$privacy_out" --repo-prefix "$repo/")
  [ "$build_only" = 1 ] && plan_args+=(--build-only)
  plan="$(python3 -I "$repo/scripts/ci/check-lez-shards.py" "${plan_args[@]}")"
  expected=0
  executed=0
  while IFS= read -r line; do
    [ -n "$line" ] && expected=$((expected + 1))
  done <<< "$plan"
  if [ "$expected" -le 0 ]; then
    echo "lez-harness.sh: shard $shard produced no commands" >&2
    exit 1
  fi
  while IFS=$'\t' read -r -u 3 -a fields; do
    [ "${#fields[@]}" -ge 3 ] || continue
    step_cwd="${fields[0]}"
    step_dev="${fields[1]}"
    step_cmd=("${fields[@]:2}")
    stamp "shard $shard: ${step_cmd[*]}"
    if [ "$step_dev" = 1 ]; then
      (cd "$repo/$step_cwd" && RISC0_DEV_MODE=1 "${step_cmd[@]}") < /dev/null
    else
      (cd "$repo/$step_cwd" && "${step_cmd[@]}") < /dev/null
    fi
    executed=$((executed + 1))
  done 3<<< "$plan"
  if [ "$executed" -ne "$expected" ]; then
    echo "lez-harness.sh: shard $shard executed $executed of $expected planned step(s)" >&2
    exit 1
  fi
  if [ "$build_only" = 1 ]; then
    stamp "built shard $shard"
  else
    stamp "passed shard $shard"
  fi
  exit 0
fi

cd "$repo/lez"
stamp "build: the harness's tests and the updater's (guests with guest Rust 1.91.1 from $RISC0_HOME)"
cargo test -p harness --locked --no-run
cargo test -p ferrata-updater --features harness --locked --no-run
stamp "built"
[ "${1:-}" = build ] && exit 0

stamp "the harness: every test file of lez/harness/tests/, dev mode"
RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads "$threads"
stamp "the updater: lez/updater/tests/ on the recorded data, dev mode"
RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked -- --test-threads "$threads"
stamp "the privacy suite end to end, then its gate, into $privacy_out"
bash "$repo/scripts/privacy/run.sh" "$privacy_out"
python3 -I "$repo/scripts/privacy/check-results.py" "$privacy_out"
python3 -I "$repo/scripts/privacy/controls.py" "${FERRATA_PRIVACY_DIR:-$repo/target/privacy}" "$privacy_out-controls"
stamp "passed"
