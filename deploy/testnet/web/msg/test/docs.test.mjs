// The messages docs say only what the repository shows (review findings M4, L9, L17-L20, L22): every path they cite
// exists; every number in the design and in EVIDENCE's messages row is the one its record prints; no agent scratch
// path, no
// machine path and no built-in RPC host in the committed files; vendor/NOTICE covers every bundled package; the wallet
// directory loads as ES modules on Node 20.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const MSG = join(HERE, '..');
const REPO = join(MSG, '../../../..');
// docs/EVIDENCE.md's messages row (the row this directory's evidence is folded into), read alone.
const ROW_DOC = 'docs/EVIDENCE.md#messages';
const read = (p) => p === ROW_DOC
  ? readFileSync(join(REPO, 'docs/EVIDENCE.md'), 'utf8').split('\n').filter((l) => l.includes('**Encrypted wallet-to-wallet messages on LEZ v0.4')).join('\n')
  : readFileSync(join(REPO, p), 'utf8');
const DOCS = ['docs/design/messages.md', ROW_DOC, 'deploy/testnet/web/msg/README.md', 'deploy/testnet/web/msg/AGENTS-section.md'];
const ROOTS = ['lez/', 'deploy/', 'idl/', 'docs/', 'scripts/'];

test('every repository path the docs cite exists', () => {
  const missing = [];
  for (const doc of DOCS) {
    const text = read(doc);
    const cited = new Set();
    for (const m of text.matchAll(/`([^`\s]+)`/g)) if (ROOTS.some((r) => m[1].startsWith(r))) cited.add(m[1]);
    for (const m of text.matchAll(/\]\(([^)\s#]+)(#[^)]*)?\)/g)) if (!/^https?:/.test(m[1])) cited.add(relative(REPO, join(REPO, dirname(doc), m[1])));
    for (let p of cited) {
      p = p.replace(/[.,;:]+$/, '');
      if (/[<>*…]/.test(p) || p.includes('...')) continue;
      if (!existsSync(join(REPO, p))) missing.push(`${doc}: ${p}`);
    }
  }
  assert.deepEqual(missing, []);
});

// The committed files under `dir` (git ls-files: a reviewer's untracked scratch file is not the repository's), or every
// file under it where git is not available.
function files(dir, out = []) {
  try {
    return execFileSync('git', ['ls-files', '-z', '--', dir], { cwd: REPO, encoding: 'utf8' }).split('\0').filter(Boolean);
  } catch { /* no git: walk the directory */ }
  for (const f of readdirSync(join(REPO, dir))) {
    const p = join(dir, f);
    if (['target', 'node_modules'].includes(f)) continue;
    if (statSync(join(REPO, p)).isDirectory()) files(p, out); else out.push(p);
  }
  return out;
}

test('no agent scratch path, machine path or built-in RPC host in the committed messages files', () => {
  const scope = [
    ...files('deploy/testnet/web/msg'), ...files('lez/msg-wire'), ...files('lez/msg-v2-wire'), ...files('lez/guests/msg/src'),
    ...files('lez/guests/msg-v2/src'), ...files('lez/harness/tests/msg_common'), 'lez/harness/tests/messages.rs',
    'lez/harness/tests/messages_v2.rs', 'lez/harness/tests/messages_findings.rs', 'idl/tools/msg_idl.py', ...DOCS,
  ];
  const bad = [];
  for (const p of scope) {
    if (p.endsWith('test/docs.test.mjs')) continue;
    const t = read(p);
    for (const pat of [/(^|[\s`'"(])(\.\.\/)*tmp\/[\w.-]+/m, /\/home\//, /\bspike\//, /\.workers\.dev/]) if (pat.test(t)) bad.push(`${p}: ${pat}`);
  }
  assert.deepEqual(bad, []);
  const cfg = JSON.parse(read('deploy/testnet/web/msg/config.json'));
  assert.equal(cfg.rpc, null, 'the host that serves /msg/ writes its RPC into config.json; the repository names none');
  // No host at all in the clients, their config or the messages docs: the network's address belongs to the host that
  // serves /msg/ (review L20: a default address had put a personal name into the repository).
  for (const p of ['deploy/testnet/web/msg/ferrata-msg.mjs', 'deploy/testnet/web/msg/client.js', 'deploy/testnet/web/msg/ferrata-msg.js',
    'deploy/testnet/web/msg/config.json', ...DOCS]) assert.ok(!/https?:\/\//.test(read(p)), `${p} names a URL`);
});

test('vendor/NOTICE names every package the bundle contains, at the locked version', () => {
  const lock = JSON.parse(readFileSync(join(MSG, 'vendor/package-lock.json'), 'utf8'));
  const notice = readFileSync(join(MSG, 'vendor/NOTICE'), 'utf8');
  const roots = [...readFileSync(join(MSG, 'vendor/entry.js'), 'utf8').matchAll(/from '(@[^/']+\/[^/']+)/g)].map((m) => m[1]);
  const seen = new Set();
  const visit = (n) => { if (seen.has(n)) return; seen.add(n); Object.keys(lock.packages[`node_modules/${n}`].dependencies ?? {}).forEach(visit); };
  roots.forEach(visit);
  for (const n of seen) assert.ok(notice.includes(`==== ${n} ${lock.packages[`node_modules/${n}`].version} (MIT)`), n);
});

test('the wallet directory and its vendored bundle are ES modules (Node 20 before 20.19 needs it)', () => {
  assert.equal(JSON.parse(read('deploy/testnet/web/wallet/package.json')).type, 'module');
  assert.equal(JSON.parse(read('deploy/testnet/web/wallet/vendor/package.json')).type, 'module');
  assert.equal(JSON.parse(readFileSync(join(MSG, 'vendor/package.json'), 'utf8')).type, 'module');
});

// [doc, what the doc says, record, what the record prints]
const CLAIMS = [
  ['docs/design/messages.md', '284 transactions, 248 applied, 34 refused by the program with a code, 2 reverted by LEZ', 'lez/results/messages-v2.md', '284 transactions: 248 applied, 34 refused by the program with a code, 2 reverted by LEZ'],
  [ROW_DOC, '284 signed transactions', 'lez/results/messages-v2.md', '284 transactions:'],
  [ROW_DOC, '248 applied, 34 refused by the program with a code, 2 reverted', 'lez/results/messages-v2.md', '248 applied, 34 refused by the program with a code, 2 reverted'],
  ['docs/design/messages.md', '21 of 21 exit codes', 'lez/results/messages-v2.md', 'Codes reached: 21 of 21'],
  [ROW_DOC, '21 of the program\'s 21 exit codes', 'lez/results/messages-v2.md', 'Codes reached: 21 of 21'],
  [ROW_DOC, '19 of its 20 exit codes', 'lez/results/messages.md', 'Codes reached: 19 of 20'],
  ['docs/design/messages.md', '19 of its 20 exit codes', 'lez/results/messages.md', 'Codes reached: 19 of 20'],
  ['docs/design/messages.md', 'worst case 407,277 cycles', 'lez/results/messages-v2.md', '| send | 407277 |'],
  ['docs/design/messages.md', 'removing 64 and adding 64 cost up to 307,004 cycles', 'lez/results/messages-v2.md', 'heavy removes 64 absent ids and re-adds its 64 blocked | applied | 307004 |'],
  ['docs/design/messages.md', 'cost up to 1,633,147 for the same edit', 'lez/results/messages-v2.md', '| edit 128 | 1633147 |'],
  ['docs/design/messages.md', 'an edit of 128 ids 307,004 with account ids and\n  1,633,147 with ids sharing 31 bytes', 'lez/results/messages-v2.md', '| edit 128 | 1633147 |'],
  ['docs/design/messages.md', 'publish 135,667 (a first key on full lists', 'lez/results/messages-v2.md', '| publish | 135667 |'],
  ['docs/design/messages.md', 'clear of 32\n  envelopes 1,148,014 (by an owner with full lists)', 'lez/results/messages-v2.md', '| remove 32 | 1148014 |'],
  ['docs/design/messages.md', '4,096 B 718,983', 'lez/results/messages-v2.md', '| remove 16 | 718983 |'],
  ['docs/design/messages.md', 'an edit 200,000 plus 18,000 per id', 'deploy/testnet/web/msg/client.js', 'edit: 200_000n, editPer: 18_000n'],
  ['docs/design/messages.md', 'a send\n  600,000', 'deploy/testnet/web/msg/client.js', 'send: 600_000n'],
  ['docs/design/messages.md', 'SetPolicy 9,092 cycles with or without', 'lez/results/messages-findings.md', '| 9092 -> 9092 (1.00x) |'],
  ['docs/design/messages.md', 'v1: 9,292 → 338,022, 36.4x', 'lez/results/messages-findings.md', '| 9292 -> 338022 (36.4x) |'],
  ['docs/design/messages.md', 'the owner\'s own send 74,798 either way', 'lez/results/messages-findings.md', '| 74798 -> 74798 (1.00x) |'],
  ['docs/design/messages.md', 'v1: 34,758 → 388,388, 11.2x', 'lez/results/messages-findings.md', '| 34758 -> 388388 (11.2x) |'],
  ['docs/design/messages.md', 'v1: 1,550,595 for a send between two', 'lez/results/messages-findings.md', '| 1550595 cycles > 1500000'],
  ['docs/design/messages.md', '59 sends landed within 90', 'lez/results/messages-findings.md', '| 59 landed in 90 s |'],
  ['docs/design/messages.md', 'Eight funded accounts can occupy every stranger slot** (v1: four)', 'lez/results/messages-findings.md', '| L8 accounts to occupy every stranger slot | 4 | 8 |'],
  ['docs/design/messages.md', '27 of 27 checks', 'deploy/testnet/web/msg/test/results/local-2/record.txt', '== 27 of 27 checks passed'],
  [ROW_DOC, '27 of 27', 'deploy/testnet/web/msg/test/results/local-2/record.txt', '== 27 of 27 checks passed'],
  ['docs/design/messages.md', 'deployed by LEZ\'s CLI in 123 s (3 segments and a header)', 'deploy/testnet/web/msg/test/results/local-2/record.txt', 'deployed in 123 s by LEZ\'s CLI (immutable): 3 segments + header'],
  ['docs/design/messages.md', 'fee 1,022,264\n  units', 'deploy/testnet/web/msg/test/results/local-2/record.txt', 'fee 1022264 units'],
  ['docs/design/messages.md', 'fee 724,328 units', 'deploy/testnet/web/msg/test/results/local-2/record.txt', 'reverted transaction 724328 units'],
  ['docs/design/messages.md', 'in its block 3.6 s after', 'deploy/testnet/web/msg/test/results/local-2/record.txt', 'send to its block 3611 ms'],
  ['docs/design/messages.md', 'a send to an inbox\n  account refused before sending', 'deploy/testnet/web/msg/test/results/local-2/record.txt', 'PASS send to bob\'s inbox account (one of the program\'s own accounts): refused BAD_STATE before sending'],
  [ROW_DOC, '(63 ids, then 128) applied within its limits', 'deploy/testnet/web/msg/test/results/local-2/record.txt', 'PASS bulk edits applied within the library\'s limits (63 ids, then 128)'],
  [ROW_DOC, '`0x480f5c4e…4f59`, binary sha256 `ebcc609a…1e95`, 300,620 bytes', 'docs/image-ids.md', '`0x480f5c4eedb3bde85c26e3e408c962d91cdea4941391b3e5ac63aefdb2eb4f59` | `ebcc609a1bd79cb924813f8ce856b6ae6b0465a80d5e3311693ac8e922091e95` | 300,620'],
  [ROW_DOC, '`0x04ae1988…5277`', 'docs/image-ids.md', '`0x04ae1988fe84eef6da472152a75b17e6c7453e6e4775264f12af1a750b945277` | `5b7af994f1169f2b6a905ccfda478f098f5bdef9e09cce0edae07fa8fe733134`'],
  ['docs/design/messages.md', '`0x480f5c4e…4f59`', 'lez/results/messages-v2.md', 'image id `480f5c4eedb3bde85c26e3e408c962d91cdea4941391b3e5ac63aefdb2eb4f59`'],
  ['docs/design/messages.md', '`0x04ae1988…5277`', 'lez/results/messages.md', 'image id `04ae1988fe84eef6da472152a75b17e6c7453e6e4775264f12af1a750b945277`'],
  ['docs/design/messages.md', 'state grows by one empty account per message ever delivered', 'lez/harness/tests/messages_v2.rs', 'its account stays in the state for good (review N4)'],
];

test('every number in the design and in EVIDENCE\'s messages row is the one its record prints', () => {
  const wrong = [];
  for (const [doc, says, rec, prints] of CLAIMS) {
    if (!read(doc).includes(says)) wrong.push(`${doc} does not say: ${says}`);
    if (!read(rec).includes(prints)) wrong.push(`${rec} does not print: ${prints}`);
  }
  assert.deepEqual(wrong, []);
});

test('the row\'s count of JS tests is the count in the test files', () => {
  const n = readdirSync(HERE).filter((f) => f.endsWith('.test.mjs')).reduce((k, f) => k + (readFileSync(join(HERE, f), 'utf8').match(/^test\(/gm) ?? []).length, 0);
  assert.ok(read(ROW_DOC).includes(`\`node --test deploy/testnet/web/msg/test/*.test.mjs\`: ${n} tests`), `${n} tests`);
});

test('the gas limits the clients use are the ones the harness checks, and the v1 send limit covers the measured send', () => {
  const client = readFileSync(join(MSG, 'client.js'), 'utf8');
  const v2 = Object.fromEntries([...client.match(/export const GAS_V2 = \{([^}]*)\}/)[1].matchAll(/(\w+): ([\d_]+)n/g)].map((m) => [m[1], Number(m[2].replaceAll('_', ''))]));
  const v1 = Object.fromEntries([...client.match(/export const GAS_V1 = \{([^}]*)\}/)[1].matchAll(/(\w+): ([\d_]+)n/g)].map((m) => [m[1], Number(m[2].replaceAll('_', ''))]));
  const rec = read('lez/results/messages-v2.md');
  const largest = (k) => Number(rec.match(new RegExp(`\\| ${k} \\| (\\d+) \\|`))[1]);
  assert.ok(largest('send') <= v2.send && largest('publish') <= v2.publish);
  const edits = [...rec.matchAll(/\| edit (\d+) \| (\d+) \|/g)].map((m) => [Number(m[1]), Number(m[2])]);
  assert.ok(edits.some(([n]) => n === 128), 'the record has the 128-id edit (64 removed, 64 added)');
  for (const [n, cycles] of edits) assert.ok(cycles <= v2.edit + n * v2.editPer, `edit of ${n} ids: ${cycles} > ${v2.edit + n * v2.editPer}`);
  assert.ok(largest('remove 32') <= v2.removeBase + 32 * v2.removePer && largest('remove 16') <= v2.removeBase + 16 * v2.removePer);
  const f = read('lez/results/messages-findings.md');
  assert.ok(Number(f.match(/\| (\d+) cycles > 1500000/)[1]) <= v1.send);
});
