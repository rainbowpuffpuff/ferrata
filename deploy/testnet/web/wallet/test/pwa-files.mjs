// The files the wallet serves, with their SHA-256: everything in the bundle except the tests, the README, the vendor
// build's inputs, config.json (the host's own) and sw.js itself. The list goes into sw.js (between its FILES markers),
// so the service worker keeps exactly this build, and is what a deployment installs.
//
//   node test/pwa-files.mjs          rewrite sw.js's list
//   node test/pwa-files.mjs --check  exit 1 if sw.js's list differs from the files on disk
//   node test/pwa-files.mjs --list   print every file a deployment installs (sw.js and config.json included)
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
// package.json: Node's module type for the tests and the CLIs that import lez.js (Node 20 before 20.19), not a page file.
const SKIP = new Set(['README.md', 'config.json', 'sw.js', 'package.json', 'vendor/build.sh', 'vendor/package.json',
  'vendor/package-lock.json', 'vendor/entry.js']);

export function servedFiles(root = ROOT) {
  const out = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir).sort()) {
      if (name.startsWith('.') || name === 'node_modules') continue;
      const p = join(dir, name), rel = relative(root, p).split('\\').join('/');
      if (rel === 'test') continue;
      if (statSync(p).isDirectory()) walk(p);
      else if (!SKIP.has(rel)) out.push(rel);
    }
  };
  walk(root);
  return out.sort();
}

export function listing(root = ROOT) {
  const files = Object.fromEntries(servedFiles(root).map((f) => [f, createHash('sha256').update(readFileSync(join(root, f))).digest('hex')]));
  const build = createHash('sha256').update(Object.entries(files).map(([f, h]) => `${h}  ${f}\n`).join('')).digest('hex').slice(0, 16);
  return { build, files };
}

export function block({ build, files }) {
  const lines = Object.entries(files).map(([f, h]) => `  '${f}': '${h}',`);
  return `const BUILD = '${build}';\nconst FILES = {\n${lines.join('\n')}\n};\n`;
}

const MARK = /(\/\/ BEGIN FILES[^\n]*\n)([\s\S]*?)(\/\/ END FILES)/;
export function swWith(sw, l) {
  if (!MARK.test(sw)) throw new Error('sw.js has no FILES markers');
  return sw.replace(MARK, (_, a, _b, c) => a + block(l) + c);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const swPath = join(ROOT, 'sw.js'), sw = readFileSync(swPath, 'utf8'), l = listing();
  const arg = process.argv[2];
  if (arg === '--list') {
    for (const f of [...Object.keys(l.files), 'config.json', 'sw.js'].sort()) console.log(f);
  } else if (arg === '--check') {
    if (swWith(sw, l) !== sw) { console.error('sw.js lists another build than the files on disk: run node test/pwa-files.mjs'); process.exit(1); }
    console.log(`sw.js lists build ${l.build}: ${Object.keys(l.files).length} files`);
  } else {
    writeFileSync(swPath, swWith(sw, l));
    console.log(`sw.js: build ${l.build}, ${Object.keys(l.files).length} files`);
  }
}
