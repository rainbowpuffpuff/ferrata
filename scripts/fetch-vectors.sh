#!/usr/bin/env bash
# Fetch the pinned test vectors into ./vectors/ and verify them on every run.
#   consensus-specs v1.7.0-beta.4: light-client `sync` and `update_ranking` (minimal), and the
#     `general` BLS vectors;
#   ethereum/bls12-381-tests v0.1.2: `fast_aggregate_verify` and `verify`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SPEC="v1.7.0-beta.4"
SPEC_DIR="${ROOT}/vectors/${SPEC}"
BLS="v0.1.2"
BLS_DIR="${ROOT}/vectors/bls12-381-tests-${BLS}"

# Hashes of the release assets, and of the extracted trees (checked even on a cache hit).
MINIMAL_SHA="26d09aea70c62ed235a120a95082d544ab1c2e6fe36bd35ea1bd5da62f3b7b68"
GENERAL_SHA="c4f395f71f606c28093500e32067990e430cc73a2277be6f3bf5090adcce3f67"
BLS_SHA="a4ef909b6248395094949a3bcdeb4e1b5b999733d4436f2c5006edba99d29279"
SPEC_TREE="34347c379bbc5f202cb5b505f2d27887716cec003509e71a36287af3dd029376"
BLS_TREE="ff4c9e64d9d111ae6f8eb6fbeed47a944b5bdabf85d419d253cf7a9933dbe7d9"

if command -v sha256sum >/dev/null; then SHA=(sha256sum); else SHA=(shasum -a 256); fi

fetch() { # url sha256 dest
  curl -fL --retry 3 -o "$3" "$1"
  echo "$2  $3" | "${SHA[@]}" -c -
}

tree_hash() { # dir, then paths relative to it
  local dir="$1"; shift
  (cd "$dir" && find "$@" -type f -print0 | LC_ALL=C sort -z | xargs -0 "${SHA[@]}" | "${SHA[@]}" | cut -d' ' -f1)
}

if [ ! -d "${SPEC_DIR}/tests/minimal" ] || [ ! -d "${SPEC_DIR}/tests/general" ]; then
  mkdir -p "${SPEC_DIR}"
  base="https://github.com/ethereum/consensus-specs/releases/download/${SPEC}"
  fetch "${base}/minimal.tar.gz" "${MINIMAL_SHA}" "${SPEC_DIR}/minimal.tar.gz"
  fetch "${base}/general.tar.gz" "${GENERAL_SHA}" "${SPEC_DIR}/general.tar.gz"
  tar -xzf "${SPEC_DIR}/minimal.tar.gz" -C "${SPEC_DIR}" --wildcards \
    'tests/minimal/*/light_client/sync/*' 'tests/minimal/*/light_client/update_ranking/*'
  tar -xzf "${SPEC_DIR}/general.tar.gz" -C "${SPEC_DIR}"
  rm -f "${SPEC_DIR}/minimal.tar.gz" "${SPEC_DIR}/general.tar.gz"
fi

if [ ! -d "${BLS_DIR}/fast_aggregate_verify" ]; then
  mkdir -p "${BLS_DIR}"
  fetch "https://github.com/ethereum/bls12-381-tests/releases/download/${BLS}/bls_tests_yaml.tar.gz" \
    "${BLS_SHA}" "${BLS_DIR}/bls_tests_yaml.tar.gz"
  tar -xzf "${BLS_DIR}/bls_tests_yaml.tar.gz" -C "${BLS_DIR}"
  rm -f "${BLS_DIR}/bls_tests_yaml.tar.gz"
fi

got=$(cd "${SPEC_DIR}" && tree_hash . tests/minimal/*/light_client/sync tests/minimal/*/light_client/update_ranking tests/general)
[ "$got" = "${SPEC_TREE}" ] || { echo "consensus-spec vectors do not match the pinned tree hash: $got" >&2; exit 1; }
got=$(tree_hash "${BLS_DIR}" fast_aggregate_verify verify)
[ "$got" = "${BLS_TREE}" ] || { echo "BLS vectors do not match the pinned tree hash: $got" >&2; exit 1; }
echo "vectors verified: ${SPEC_DIR}, ${BLS_DIR}"
