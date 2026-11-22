#!/usr/bin/env bash
# Fetch the pinned test vectors into ./vectors/ and verify them on every run.
#   consensus-specs v1.7.0-beta.4: light-client `sync` and `update_ranking` (minimal), and the
#     `general` BLS vectors;
#   ethereum/bls12-381-tests v0.1.2: `fast_aggregate_verify` and `verify`;
#   ethereum/tests at a pinned commit: the Merkle-Patricia `TrieTests` (file by file).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SPEC="v1.7.0-beta.4"
SPEC_DIR="${ROOT}/vectors/${SPEC}"
BLS="v0.1.2"
BLS_DIR="${ROOT}/vectors/bls12-381-tests-${BLS}"
# ethereum/tests `develop` on 9 Nov 2026; TrieTests last changed in 6581e1f (2 Oct 2017).
ETH_TESTS="c67e485ff8b5be9abc8ad15345ec21aa22e290d9"
TRIE_DIR="${ROOT}/vectors/ethereum-tests-${ETH_TESTS:0:7}/TrieTests"
TRIE_FILES=(
  "trietest.json 0ce5e1151210958edf47911b332fe188696d741f9f44b1a471ee62bc666c1f0f"
  "trieanyorder.json 92404d5c2076524e62f02e9657a684aa0561067d49f3b489b78b5033c6fc3e2d"
  "trietest_secureTrie.json 98b76fd92fed69cb449d7a555cdb3eb397e7179614857de1289de2f63ac8e77a"
  "trieanyorder_secureTrie.json acba24dcef034b8ddd78d4ba8e716468854bac1a5ac886cc806c93f1c93f1ed4"
  "hex_encoded_securetrie_test.json 487f9e1e404e46dc0a54d526b14927d3a5ba90f7f52625e7d49cd170974ce9ff"
  "trietestnextprev.json ac9d8f62664d6b47ab25050e16617d617ef3363b4905590829267f9a9d33c6f0"
)

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
  # Only the minimal preset's light-client `sync` and `update_ranking` tests: the names of their files
  # are listed first and extracted by name, which GNU tar and BSD tar (macOS's) both do the same way (a
  # pattern would need GNU tar's --wildcards, an option BSD tar refuses). Files only: GNU tar takes a
  # listed directory for all it holds and then reports its files, listed again, as not found. The tree
  # hash below checks the result.
  tar -tzf "${SPEC_DIR}/minimal.tar.gz" \
    | grep -E '^tests/minimal/[^/]+/light_client/(sync|update_ranking)/.*[^/]$' > "${SPEC_DIR}/minimal.list"
  tar -xzf "${SPEC_DIR}/minimal.tar.gz" -C "${SPEC_DIR}" -T "${SPEC_DIR}/minimal.list"
  rm -f "${SPEC_DIR}/minimal.list"
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

mkdir -p "${TRIE_DIR}"
for entry in "${TRIE_FILES[@]}"; do
  read -r name sha <<<"${entry}"
  if [ ! -f "${TRIE_DIR}/${name}" ]; then
    fetch "https://raw.githubusercontent.com/ethereum/tests/${ETH_TESTS}/TrieTests/${name}" "${sha}" "${TRIE_DIR}/${name}"
  fi
  echo "${sha}  ${TRIE_DIR}/${name}" | "${SHA[@]}" -c --quiet - || { echo "${name} does not match its pinned hash" >&2; exit 1; }
done

got=$(cd "${SPEC_DIR}" && tree_hash . tests/minimal/*/light_client/sync tests/minimal/*/light_client/update_ranking tests/general)
[ "$got" = "${SPEC_TREE}" ] || { echo "consensus-spec vectors do not match the pinned tree hash: $got" >&2; exit 1; }
got=$(tree_hash "${BLS_DIR}" fast_aggregate_verify verify)
[ "$got" = "${BLS_TREE}" ] || { echo "BLS vectors do not match the pinned tree hash: $got" >&2; exit 1; }
echo "vectors verified: ${SPEC_DIR}, ${BLS_DIR}, ${TRIE_DIR}"
