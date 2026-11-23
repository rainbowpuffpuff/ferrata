#!/usr/bin/env bash
# Rebuild vendor/noble.js from the exact package versions of vendor/package-lock.json (npm's integrity hashes), with the
# esbuild version pinned there. Scratch goes to SCRATCH (default: target/wallet-vendor-build in the repository,
# git-ignored).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRATCH="${SCRATCH:-$(cd "$HERE/../../../../.." && pwd)/target/wallet-vendor-build}"
mkdir -p "$SCRATCH"
cp "$HERE/package.json" "$HERE/package-lock.json" "$HERE/entry.js" "$SCRATCH/"
cd "$SCRATCH"
npm ci --ignore-scripts --no-audit --no-fund > npm-ci.log 2>&1
node_modules/.bin/esbuild entry.js --bundle --format=esm --minify --legal-comments=inline --target=es2020 \
  --outfile="$HERE/noble.js"
sha256sum "$HERE/noble.js"
