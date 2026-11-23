#!/usr/bin/env bash
# CI's names checks (job `names` of .github/workflows/ci.yml; names/README.md): the names program's wire crate tests,
# its harness suites (lez/harness/tests/names.rs and names_review.rs, feature `names`; LEZ v0.4.0-rc1's state machine
# and fee settlement, RISC Zero dev mode), the IDL check against the program's own encodings, and the JS resolver's
# and the CLI's tests. Nothing is sent to any network but the dependency fetches of cargo and, when the runner's Node
# is older than 22.7 / 20.19, Node 22.12.0 from nodejs.org (checked against its SHASUMS256).
#
# Needs: Rust 1.98.1 (lez/rust-toolchain.toml), RISC Zero's rzup toolchains with guest Rust 1.91.1 in
# $RISC0_HOME_LEZ or ../risc0-home-lez or ~/risc0-home-lez (scripts/ci/install-risc0.sh), the upstream sources beside
# the repository (scripts/fetch-upstream.sh).
#
#   scripts/ci/names.sh          build what is missing, then run every check
#   scripts/ci/names.sh build    build only
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
lez_home="${RISC0_HOME_LEZ:-}"
if [ -z "$lez_home" ]; then
  for d in "$repo/../risc0-home-lez" "$HOME/risc0-home-lez"; do [ -d "$d" ] && { lez_home="$(cd "$d" && pwd)"; break; }; done
fi
[ -n "$lez_home" ] || { echo "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ)" >&2; exit 1; }
export RISC0_HOME="$lez_home" CARGO_TERM_COLOR=never
stamp() { echo "== $(date -u +%H:%M:%S) $*"; }

cd "$repo/lez"
stamp "build: the names wire crate and the harness's names suites (guests with guest Rust 1.91.1 from $RISC0_HOME)"
cargo test -p ferrata-lez-names-wire --locked --no-run
cargo test -p harness --features names --locked --test names --test names_review --no-run
[ "${1:-}" = build ] && exit 0

stamp "the wire crate's tests (the rules, the skeleton, the exit codes)"
cargo test -p ferrata-lez-names-wire --locked
stamp "the harness: every action and refusal, and the review's cases, dev mode"
RISC0_DEV_MODE=1 cargo test -p harness --features names --locked --test names --test names_review -- --test-threads 1
cd "$repo"
stamp "the IDL against the program's encodings (lez/results/names-vectors.json, rewritten by the run above)"
git diff --exit-code -- lez/results/names-vectors.json || { echo "the program's encodings changed: commit lez/results/names-vectors.json" >&2; exit 1; }
python3 -I idl/tools/check_names_idl.py
node=node
if ! node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22||(a===22&&b>=7)||(a===20&&b>=19)?0:1)' 2>/dev/null; then
  N="$repo/target/node22"; V=v22.12.0; T=node-v22.12.0-linux-x64.tar.xz
  if [ ! -x "$N/bin/node" ]; then
    mkdir -p "$N.dl" "$N"
    curl -sSfL -o "$N.dl/$T" "https://nodejs.org/dist/$V/$T"
    curl -sSfL -o "$N.dl/SHASUMS256.txt" "https://nodejs.org/dist/$V/SHASUMS256.txt"
    (cd "$N.dl" && grep " $T\$" SHASUMS256.txt | sha256sum -c -)
    tar -xJf "$N.dl/$T" -C "$N" --strip-components=1
  fi
  node="$N/bin/node"
fi
stamp "the JS resolver against the program's vectors, and the CLI ($("$node" --version))"
"$node" --test names/js/test/names.test.mjs names/js/test/review.test.mjs
stamp "passed"
