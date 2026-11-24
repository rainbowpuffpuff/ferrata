#!/usr/bin/env bash
# Full differential oracle run comparing helios-consensus-core 0.12.0, source unmodified
# (a16z/helios rev 2af9b41887e069144ee56cd710f3ddca195473f8, pure-Rust bls12_381 0.8.0,
# built with this repo's lockfile) against Ferrata's program step path
# (ferrata_core::steps + witness, blst 0.3.17) and native stores.
#
# Runs:
#   1. Pin and no-[patch] verification (Cargo.lock + source tree SHA-256).
#   2. All 7 default tests in crates/ferrata-spectests/tests/helios_step_oracle.rs
#      (pin check, all 321 recorded real updates across all 332 repo .ssz files,
#      all 33 Helios-decodable consensus-specs sync vectors [96 two-way + 15
#      post-force_update one-way], 467 field/edge/SSZ-framing update mutations,
#      17 bootstrap/store differential mutations, 18 step-path-only wire/witness/job
#      refusals, 8 adoption key-root binding refusals, 45 re-signed semantic edge
#      cases, and the 400-mutation CI subset).
#   3. The full ignored seeded random mutation sweep on the program's step path
#      (default: 2,500 mutations x 4 classes = 10,000 mutations across Electra
#      and Gloas period and finality updates, fixed and dynamic clocks; override
#      per-class count via FERRATA_ORACLE_MUTATIONS_PER_CLASS).
#   4. The existing Helios suites (helios_lab_record and helios_oracle, including
#      the 2,000 mainnet random mutations and upstream #871/#872 pin tests).
#
# Usage:
#   scripts/helios-oracle-full.sh [--out FILE]
#
# Environment knobs:
#   CARGO_BUILD_JOBS                    build parallelism (default: 4)
#   FERRATA_ORACLE_TEST_THREADS         cargo test threads (default: 4)
#   FERRATA_ORACLE_MUTATIONS_PER_CLASS  random mutations per class (default: 2500 -> 10,000 total)
#   FERRATA_ORACLE_FAILURES_DIR         if set, writes any failing case report + .ssz bytes there

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
cd "$repo"

out_file=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --out)
      shift
      if [ "$#" -eq 0 ]; then
        echo "usage: scripts/helios-oracle-full.sh [--out FILE]" >&2
        exit 2
      fi
      out_file="$1"
      shift
      ;;
    -h|--help)
      sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      if [ -z "$out_file" ]; then
        out_file="$1"
        shift
      else
        echo "unexpected argument: $1" >&2
        exit 2
      fi
      ;;
  esac
done

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
test_threads="${FERRATA_ORACLE_TEST_THREADS:-4}"
muts_per_class="${FERRATA_ORACLE_MUTATIONS_PER_CLASS:-2500}"
export FERRATA_ORACLE_MUTATIONS_PER_CLASS="$muts_per_class"

if [ ! -d vectors/v1.7.0-beta.4 ]; then
  echo "vectors/v1.7.0-beta.4 missing; running scripts/fetch-vectors.sh..." >&2
  bash scripts/fetch-vectors.sh
fi

run_suite() {
  start_s=$(date +%s)
  rev=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
  branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
  utc_now=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
  rustc_v=$(rustc --version)
  cargo_v=$(cargo --version)
  uname_s=$(uname -srm)

  cat <<EOF
# Ferrata vs unmodified helios-consensus-core 0.12.0 — full step-path oracle run
date_utc:      $utc_now
git_rev:       $rev ($branch)
rustc:         $rustc_v
cargo:         $cargo_v
host:          $uname_s
build_jobs:    $CARGO_BUILD_JOBS
test_threads:  $test_threads
muts_per_class:$muts_per_class (total random step-path mutations: $((muts_per_class * 4)))
helios_pin:    helios-consensus-core 0.12.0 rev 2af9b41887e069144ee56cd710f3ddca195473f8
helios_src_sha:17be8945b6af08d5b07da5f84d734c09c9375d86f982713beaf1a9cdfa410a20

EOF

  echo "=== [1/3] helios_step_oracle (default CI subset: pin, all 321 real updates, 33 spec vectors [96+15], 467 update + 17 boot/store + 18 step-only + 8 adopt-root, 45 re-signed, 400 random) ==="
  cargo test --locked -p ferrata-spectests --test helios_step_oracle -- --nocapture --test-threads="$test_threads" || return $?
  echo

  echo "=== [2/3] helios_step_oracle (full --ignored sweep: $((muts_per_class * 4)) seeded random mutations across all 4 layouts/kinds on program step path) ==="
  cargo test --locked -p ferrata-spectests --test helios_step_oracle \
    full_random_mutations_across_all_layouts_agree_with_helios_on_step_path \
    -- --ignored --nocapture --test-threads="$test_threads" || return $?
  echo

  echo "=== [3/3] helios_lab_record + helios_oracle (lab record replay, spec vectors, 2,000 mainnet random mutations, #871/#872 pins) ==="
  cargo test --locked -p ferrata-spectests --test helios_lab_record --test helios_oracle \
    -- --nocapture --test-threads="$test_threads" || return $?
  echo

  end_s=$(date +%s)
  echo "=== DONE: all Helios differential suites passed with 0 unexpected disagreements in $((end_s - start_s)) s ==="
}

if [ -n "$out_file" ]; then
  mkdir -p "$(dirname "$out_file")"
  tmp_out="${out_file}.tmp.$$"
  # Each cargo call returns on failure: `set -e` does not apply inside a function called as an `if` condition.
  if run_suite >"$tmp_out" 2>&1; then
    # Keep machine paths out of the record (cargo prints the workspace path when it compiles).
    sed -e "s#${repo}/##g" "$tmp_out" >"$out_file" && rm -f "$tmp_out"
    cat "$out_file"
  else
    status=$?
    cat "$tmp_out" >&2
    rm -f "$tmp_out"
    exit "$status"
  fi
else
  run_suite
fi
