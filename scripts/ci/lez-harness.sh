#!/usr/bin/env bash
# CI's LEZ suites (job `lez-harness` of .github/workflows/ci.yml; docs/guides/ci.md): the attestation and bridge
# programs driven through LEZ v0.4.0-rc1's state machine and fee settlement in the harness, every test file of
# lez/harness/tests/ that builds without an optional feature (the red-team suites and the privacy suite's LEZ half
# included), the update-submitting daemon on its recorded data (lez/updater/tests/), and the privacy suite end to end
# (scripts/privacy/run.sh: the LEZ half, the vault's PrivacySuite, the adversary) with its gate
# (scripts/privacy/check-results.py: the differential gate, chance-level pairing, nothing hidden found, recovery from
# the seed, the published tables reproduced). The guests run in RISC Zero dev mode: executed in the zkVM, no proof.
# Nothing is sent to any network but the dependency fetches of cargo and forge.
#
# Needs: Rust 1.98.1 (lez/rust-toolchain.toml), RISC Zero's rzup toolchains with guest Rust 1.91.1 in
# $RISC0_HOME_LEZ or ../risc0-home-lez or ~/risc0-home-lez (scripts/ci/install-risc0.sh), the upstream sources beside
# the repository (scripts/fetch-upstream.sh); for the privacy suite, Foundry with vault/lib installed.
#
#   scripts/ci/lez-harness.sh          build what is missing, then run the suites
#   scripts/ci/lez-harness.sh build    build only
#
# FERRATA_TEST_THREADS (default 4, the cores of GitHub's hosted runner) sets the test threads.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.foundry/bin:$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
lez_home="${RISC0_HOME_LEZ:-}"
if [ -z "$lez_home" ]; then
  for d in "$repo/../risc0-home-lez" "$HOME/risc0-home-lez"; do [ -d "$d" ] && { lez_home="$(cd "$d" && pwd)"; break; }; done
fi
[ -n "$lez_home" ] || { echo "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ)" >&2; exit 1; }
# The guests are built with guest Rust 1.91.1, the version of LEZ's own guest builds (lez/RESULTS.md section 6).
export RISC0_HOME="$lez_home"
# No debug info and no incremental artefacts, as e2e/ci/run.sh: smaller target directories on the runner's disk.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}" CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
threads="${FERRATA_TEST_THREADS:-4}"
t0=$(date +%s)
stamp() { echo "[$(( $(date +%s) - t0 )) s] $*"; }

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
privacy_out="${FERRATA_PRIVACY_OUT:-${RUNNER_TEMP:-$repo/../tmp}/ci-privacy}"
stamp "the privacy suite end to end, then its gate, into $privacy_out"
bash "$repo/scripts/privacy/run.sh" "$privacy_out"
python3 -I "$repo/scripts/privacy/check-results.py" "$privacy_out"
stamp "passed"
