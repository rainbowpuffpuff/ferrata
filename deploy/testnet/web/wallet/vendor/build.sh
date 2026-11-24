#!/usr/bin/env bash
# Rebuild vendor/noble.js and vendor/qr.js from the exact package versions of vendor/package-lock.json (npm's integrity
# hashes), with the esbuild version pinned there. Scratch goes to SCRATCH (default: ferrata-scratch/wallet/vendor-build
# beside the repository). `build.sh lock` refreshes package-lock.json after a change to package.json (locked packages
# keep their versions).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRATCH="${SCRATCH:-$(cd "$HERE/../../../../../.." && pwd)/ferrata-scratch/wallet/vendor-build}"
mkdir -p "$SCRATCH"
cp "$HERE/package.json" "$HERE/package-lock.json" "$HERE/entry.js" "$SCRATCH/"
cd "$SCRATCH"
if [ "${1:-}" = lock ]; then
  npm install --package-lock-only --ignore-scripts --no-audit --no-fund > npm-lock.log 2>&1
  cp package-lock.json "$HERE/package-lock.json"
fi
npm ci --ignore-scripts --no-audit --no-fund > npm-ci.log 2>&1
node_modules/.bin/esbuild entry.js --bundle --format=esm --minify --legal-comments=inline --target=es2020 \
  --outfile="$HERE/noble.js"
# QR codes (paulmillr's qr: encoding for receive codes, decoding for the camera and pictures). The entry stays here,
# out of the served folder.
cat > entry-qr.js <<'EOF'
export { default as encodeQR } from 'qr';
export { default as decodeQR } from 'qr/decode.js';
EOF
node_modules/.bin/esbuild entry-qr.js --bundle --format=esm --minify --legal-comments=inline --target=es2020 \
  --outfile="$HERE/qr.js"
sha256sum "$HERE/noble.js" "$HERE/qr.js"
