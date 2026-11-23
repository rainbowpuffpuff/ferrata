#!/usr/bin/env -S node --no-warnings
// ferrata-msg: encrypted wallet-to-wallet messages on the Ferrata LEZ v0.4 test network (prototype, unaudited).
//
//   ferrata-msg keygen   [--epoch N]                        your account, messaging keys and fingerprint (nothing sent)
//   ferrata-msg publish  [--epoch N] [--x25519-only]        publish (or rotate) your messaging key: one transaction
//   ferrata-msg send     ACCOUNT TEXT... [--expect-key FP]  encrypt to ACCOUNT's published key and deliver
//   ferrata-msg inbox    [--json] [--legacy]                read and decrypt your inbox (--legacy: the v1 program's)
//   ferrata-msg status   [ACCOUNT] [--legacy]               an account's published key, policy and inbox (public)
//   ferrata-msg delete   N...   |  clear UPTO                delete messages (their envelopes are emptied)
//   ferrata-msg contact  ACCOUNT  |  uncontact ACCOUNT  |  block ACCOUNT  |  unblock ACCOUNT
//   ferrata-msg policy   open|contacts
//
// Options (or environment, or config.json beside this file): --rpc URL (FERRATA_RPC), --program ID
// (FERRATA_MSG_PROGRAM: ferrata_msg_v2), --legacy-program ID (FERRATA_MSG_LEGACY_PROGRAM: ferrata_msg, v1, read only),
// --key FILE (FERRATA_KEY_FILE): a file holding a raw key (64 hex digits, as LEZ's wallet imports one) or a BIP-39
// mnemonic, with --path /0 (the first public account). The key file is read locally; only signed transactions and
// public reads go to the RPC. Exit status: 0 done, 1 refused (before sending, or reverted in its block), 2 usage, 3 error.
// Node 20 or later.
import { readFileSync } from 'node:fs';

const die = (msg, code = 2) => { console.error(`ferrata-msg: ${msg}`); process.exit(code); };
if (Number(process.versions.node.split('.')[0]) < 20) die(`Node 20 or later is needed (this is ${process.version})`, 3);
const lez = await import('../wallet/lez.js');
const M = await import('./ferrata-msg.js');
const C = await import('./client.js');

const argv = process.argv.slice(2);
const flags = {};
const pos = [];
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (a.startsWith('--')) {
    const k = a.slice(2);
    if (['json', 'x25519-only', 'no-wait', 'force', 'legacy'].includes(k)) flags[k] = true;
    else if (i + 1 < argv.length) flags[k] = argv[++i];
    else die(`--${k} needs a value`);
  } else pos.push(a);
}
const cmd = pos.shift();

function config() {
  try { return JSON.parse(readFileSync(new URL('./config.json', import.meta.url))); } catch { return {}; }
}
const cfg = config();
const rpcUrl = flags.rpc || process.env.FERRATA_RPC || cfg.rpc;
function programId(legacy = false) {
  const p = legacy
    ? flags['legacy-program'] || process.env.FERRATA_MSG_LEGACY_PROGRAM || cfg.legacy_program
    : flags.program || process.env.FERRATA_MSG_PROGRAM || cfg.program;
  if (!p) die(legacy ? 'no legacy (v1) program id: --legacy-program, FERRATA_MSG_LEGACY_PROGRAM or config.json' : 'no program id: --program, FERRATA_MSG_PROGRAM or config.json');
  return lez.idFromString(p);
}
function loadNode() {
  const file = flags.key || process.env.FERRATA_KEY_FILE;
  if (!file) die('no key: pass --key FILE or set FERRATA_KEY_FILE');
  const text = readFileSync(file, 'utf8').trim();
  if (/^(0x)?[0-9a-fA-F]{64}$/.test(text)) return lez.rawKeyNode(text.replace(/^0x/, ''));
  if (lez.isMnemonic(text)) return lez.deriveNode(lez.seedOf(text), lez.parsePath(flags.path || '/0'));
  die('the key file holds neither 64 hex digits nor a BIP-39 mnemonic');
}
const when = (ms) => new Date(Number(ms)).toISOString().replace('.000Z', 'Z');
const json = (x) => JSON.stringify(x, (k, v) => (typeof v === 'bigint' ? v.toString() : v instanceof Uint8Array ? lez.idToString(v) : v));
const keyText = (k) => `epoch ${k.epoch}, ${k.mlkem768 ? 'hybrid X25519 + ML-KEM-768' : 'X25519 only'}, fingerprint ${M.fingerprint(k)}`;

/** One signed action's result, as its block records it. */
function report(label, r) {
  if (r.refused) die(`${label}: not sent (${r.refused}): ${r.why}; no fee paid`, 1);
  if (r.reverted === undefined) { console.log(`${label}: sent, tx ${r.hash} (not waited for)`); return; }
  if (r.reverted) {
    const guess = r.preflight ? `; the client's check of the program's rules had said ${r.preflight}` : '';
    die(`${label}: reverted in block ${r.block}: refused (the fee was charged, nothing changed)${guess}; tx ${r.hash}`, 1);
  }
  console.log(`${label}: applied in block ${r.block}; tx ${r.hash}`);
}

async function main() {
  if (!cmd || cmd === 'help' || cmd === '--help') {
    console.log(readFileSync(new URL(import.meta.url), 'utf8').split('\n').slice(1, 20).map((l) => l.replace(/^\/\/ ?/, '')).join('\n'));
    return;
  }
  if (cmd === 'keygen') {
    const node = loadNode();
    const keys = M.deriveKeys(node.ssk, Number(flags.epoch ?? 0));
    const rec = M.publicRecord(keys);
    console.log(`account      ${lez.idToString(node.accountId)}`);
    console.log(`epoch        ${keys.epoch}`);
    console.log(`x25519       ${M.hex(rec.x25519)}`);
    console.log(`ml-kem-768   ${rec.mlkem768.length} bytes, sha256 ${M.hex(M.sha256(rec.mlkem768))}`);
    console.log(`fingerprint  ${M.fingerprint(rec)}   (give it to your contacts out of band: they send with --expect-key)`);
    console.log('derived from your account key; nothing new to back up; `ferrata-msg publish` puts it on chain');
    return;
  }
  if (!rpcUrl) die('no RPC: pass --rpc URL, set FERRATA_RPC, or fill config.json');
  const seq = new lez.Sequencer(rpcUrl);
  const wait = !flags['no-wait'];
  const node = ['status'].includes(cmd) && pos[0] ? null : loadNode();
  const program = () => programId(false);
  switch (cmd) {
    case 'publish': {
      const r = await C.publish(seq, { node, program: program(), epoch: flags.epoch != null ? Number(flags.epoch) : null, hybrid: !flags['x25519-only'], wait });
      return report(`publish key epoch ${r.record.epoch} (${r.record.mlkem768 ? 'hybrid' : 'X25519'}, fingerprint ${r.fingerprint})`, r);
    }
    case 'send': {
      const [to, ...words] = pos;
      if (!to || words.length === 0) die('usage: ferrata-msg send ACCOUNT TEXT... [--expect-key FINGERPRINT]');
      const toId = lez.idFromString(to);
      const theirs = await C.profile(seq, program(), toId).catch(() => null); // null: not a profile (C.send says why)
      if (theirs?.key) console.log(`to ${to}: key ${keyText(theirs.key)}${flags['expect-key'] ? '' : ' (not checked: pass --expect-key to refuse any other)'}`);
      const r = await C.send(seq, { node, program: program(), to: toId, text: words.join(' '), expectKey: flags['expect-key'] ?? null, force: !!flags.force, wait });
      return report(r.bucket ? `send (${r.bucket}-byte ${r.version === M.V_HYBRID ? 'hybrid' : 'X25519'} envelope, seq ${r.seq})` : 'send', r);
    }
    case 'inbox': {
      if (flags.legacy) {
        const mb = await C.legacyMailbox(seq, programId(true), node.accountId);
        const list = M.readLegacyInbox(mb, node.accountId, node.ssk);
        if (flags.json) { console.log(json(list.map(({ fromId, ...x }) => x))); return; }
        if (!list.length) { console.log('v1 inbox empty'); return; }
        for (const m of list) console.log(`v1 #${m.seq} ${when(m.atMs)} from ${m.from}${m.error ? `: cannot open: ${m.error}` : `\n    ${m.text}`}`);
        return;
      }
      const { inbox, list } = await C.messages(seq, { program: program(), node });
      if (flags.json) { console.log(json(list.map(({ fromId, ...x }) => x))); return; }
      if (!inbox) { console.log('no messaging key published yet: `ferrata-msg publish`'); return; }
      if (!list.length) { console.log('inbox empty'); return; }
      for (const m of list) {
        const head = `#${m.n} ${when(m.atMs)} from ${m.from}${m.contact ? ' (contact)' : ''} (${m.bucket} B, ${m.version === M.V_HYBRID ? 'hybrid' : 'X25519'})`;
        console.log(m.error ? `${head}: cannot open: ${m.error}` : `${head}\n    ${m.text}`);
      }
      const bytes = inbox.entries.reduce((n, e) => n + e.len, 0);
      console.log(`${list.length} of ${M.MAX_MESSAGES} messages, ${bytes} of ${M.MAX_INBOX_BYTES} bytes; free them with \`ferrata-msg clear ${list.at(-1).n}\``);
      return;
    }
    case 'status': {
      const id = pos[0] ? lez.idFromString(pos[0]) : node.accountId;
      if (flags.legacy) {
        const mb = await C.legacyMailbox(seq, programId(true), id);
        console.log(mb ? `v1: key ${mb.key ? keyText(mb.key) : 'none'}; ${mb.inbox.length} messages; ${mb.sentTotal} sent` : 'v1: nothing at this account');
        return;
      }
      const [p, ix] = await Promise.all([C.profile(seq, program(), id), C.inbox(seq, program(), id)]);
      console.log(`account   ${lez.idToString(id)}`);
      console.log(`key       ${p.key ? keyText(p.key) : 'none'}`);
      console.log(`policy    ${p.policy}; ${p.contacts.length} contacts, ${p.blocked.length} blocked`);
      console.log(`inbox     ${ix ? `${ix.entries.length} messages, ${ix.entries.reduce((n, e) => n + e.len, 0)} bytes; next number ${ix.nextN}` : 'none (no key published)'}`);
      console.log(`sent      ${p.sentTotal} in all, ${p.sentAt.length} declared within the last hour of its last send`);
      return;
    }
    case 'delete': {
      if (!pos.length) die('usage: ferrata-msg delete N...');
      const r = await C.remove(seq, { node, program: program(), ns: pos.map(BigInt), wait }).catch((e) => die(e.message, 1));
      return report(`delete ${pos.join(', ')} (${r.removed} envelopes emptied)`, r);
    }
    case 'clear': {
      if (pos[0] == null) die('usage: ferrata-msg clear UPTO');
      const r = await C.remove(seq, { node, program: program(), upto: BigInt(pos[0]), wait }).catch((e) => die(e.message, 1));
      return report(`clear up to ${pos[0]} (${r.removed} envelopes emptied)`, r);
    }
    case 'contact': case 'uncontact': case 'block': case 'unblock': {
      if (!pos[0]) die(`usage: ferrata-msg ${cmd} ACCOUNT`);
      const who = [lez.idFromString(pos[0])];
      const add = cmd === 'contact' || cmd === 'block' ? who : [];
      const rm = cmd === 'uncontact' || cmd === 'unblock' ? who : [];
      const f = cmd.endsWith('contact') ? C.contacts : C.block;
      return report(`${cmd} ${pos[0]}`, await f(seq, { node, program: program(), add, remove: rm, wait }));
    }
    case 'policy': {
      const policy = { open: 'open', contacts: 'contactsOnly' }[pos[0]];
      if (!policy) die('usage: ferrata-msg policy open|contacts');
      return report(`policy ${pos[0]}`, await C.setPolicy(seq, { node, program: program(), policy, wait }));
    }
    default:
      die(`unknown command ${cmd} (ferrata-msg help)`);
  }
}
main().catch((e) => die(e.message || String(e), 3));
