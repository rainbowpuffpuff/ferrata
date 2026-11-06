#!/usr/bin/env bash
# Download the pinned consensus-spec-tests release and keep only the light-client vectors.
set -euo pipefail

VERSION="v1.7.0-beta.4"
SHA256="26d09aea70c62ed235a120a95082d544ab1c2e6fe36bd35ea1bd5da62f3b7b68"
URL="https://github.com/ethereum/consensus-specs/releases/download/${VERSION}/minimal.tar.gz"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${ROOT}/vectors/${VERSION}"

if [ -d "${DEST}/tests/minimal" ]; then
  echo "vectors already present: ${DEST}/tests/minimal"
  exit 0
fi

mkdir -p "${DEST}"
TARBALL="${DEST}/minimal.tar.gz"
curl -fL --retry 3 -o "${TARBALL}" "${URL}"
if command -v sha256sum >/dev/null; then
  echo "${SHA256}  ${TARBALL}" | sha256sum -c -
else
  echo "${SHA256}  ${TARBALL}" | shasum -a 256 -c -
fi
tar -xzf "${TARBALL}" -C "${DEST}" --wildcards 'tests/minimal/*/light_client/*'
rm -f "${TARBALL}"
echo "light-client vectors in ${DEST}/tests/minimal"
