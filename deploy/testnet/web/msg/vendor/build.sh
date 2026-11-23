#!/usr/bin/env bash
# Rebuild vendor/crypto.js from the exact package versions of vendor/package-lock.json (npm's integrity hashes), with
# the esbuild version pinned there, and vendor/NOTICE: the licence of every package the bundle contains (the packages
# entry.js imports and their dependencies), copied from the installed packages. Scratch goes to SCRATCH (default:
# target/msg-vendor-build in the repository, git-ignored).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../../.." && pwd)"
SCRATCH="${SCRATCH:-$REPO/target/msg-vendor-build}"
mkdir -p "$SCRATCH"
cp "$HERE/package.json" "$HERE/package-lock.json" "$HERE/entry.js" "$SCRATCH/"
cd "$SCRATCH"
npm ci --ignore-scripts --no-audit --no-fund > npm-ci.log 2>&1
node_modules/.bin/esbuild entry.js --bundle --format=esm --minify --legal-comments=inline --target=es2020 \
  --outfile="$HERE/crypto.js"
node --input-type=module - "$HERE/NOTICE" <<'JS'
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
const out = process.argv[2];
const pkg = (n) => JSON.parse(readFileSync(`node_modules/${n}/package.json`, 'utf8'));
const roots = [...readFileSync('entry.js', 'utf8').matchAll(/from '(@[^/']+\/[^/']+|[^@./'][^/']*)/g)].map((m) => m[1]);
const seen = new Set();
const visit = (n) => { if (seen.has(n)) return; seen.add(n); Object.keys(pkg(n).dependencies ?? {}).forEach(visit); };
roots.forEach(visit);
let text = 'vendor/crypto.js bundles the following packages (vendor/build.sh, from vendor/package-lock.json); their licences follow.\n';
for (const n of [...seen].sort()) {
  const p = pkg(n);
  const lic = readdirSync(`node_modules/${n}`).find((f) => /^licen[cs]e/i.test(f));
  text += `\n==== ${n} ${p.version} (${p.license})\n\n${readFileSync(`node_modules/${n}/${lic}`, 'utf8').trim()}\n`;
}
writeFileSync(out, text);
console.log(`NOTICE: ${[...seen].sort().join(', ')}`);
JS
sha256sum "$HERE/crypto.js"
