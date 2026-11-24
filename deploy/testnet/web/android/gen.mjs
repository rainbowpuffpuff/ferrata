// Generates the Android project of the wallet's Trusted Web Activity with Bubblewrap's own generator (@bubblewrap/core,
// pinned by package-lock.json), from twa.json with the host filled in. The host's name is not in the repository: it
// comes from HOST. ICONS: where the generator fetches the icons (build.sh serves this repository's own icons/ on
// 127.0.0.1, the same files /wallet/icons/ serves).
//
//   HOST=example.org ICONS=http://127.0.0.1:8765 node gen.mjs OUTDIR
import { readFileSync, writeFileSync } from 'node:fs';
import { ConsoleLog, TwaGenerator, TwaManifest } from '@bubblewrap/core';

const out = process.argv[2], { HOST, ICONS } = process.env;
if (!out || !HOST || !ICONS) throw new Error('usage: HOST=host ICONS=http://127.0.0.1:PORT node gen.mjs OUTDIR');
const text = readFileSync(new URL('./twa.json', import.meta.url), 'utf8').replaceAll('{{HOST}}', HOST).replaceAll('{{ICONS}}', ICONS);
const manifest = new TwaManifest(JSON.parse(text));
const problem = manifest.validate?.();
if (problem) throw new Error(`twa.json: ${problem}`);
await new TwaGenerator().createTwaProject(out, manifest, new ConsoleLog('generate'));
// The project's own record of its settings, with the icons named where the host serves them.
writeFileSync(`${out}/twa-manifest.json`, JSON.stringify(JSON.parse(text.replaceAll(ICONS, `https://${HOST}/wallet/icons`)), null, 2) + '\n');
