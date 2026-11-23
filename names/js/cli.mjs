#!/usr/bin/env node
// ferrata names: the LEZ v0.4 test network's names (program v2) from the command line (EXPERIMENTAL, test network only).
//
//   ferrata-names [--rpc URL] [--program ID] COMMAND ...
//
// Reading (no key):  check NAME | resolve NAME | whois NAME | reverse ACCOUNT | account ACCOUNT | rules
// Writing (signed):  register NAME [--target ID | --no-target] [--receiver-keys FILE]   (commit, wait ~4 min, reveal)
//                    commit NAME [same options] [--state FILE] | reveal NAME [--state FILE]
//                      (the two steps apart; the state file, default NAME.commit.json, 0600, holds the salt)
//                    renew NAME | transfer NAME TO (an offer; TO takes it with accept; TO = yourself withdraws it)
//                    accept NAME | release NAME | forget [NAME...] (names that left you; default: all of them)
//                    set-target NAME [--target ID | --no-target] [--receiver-keys FILE | --no-receiver]
//                    set-primary NAME | set-primary --clear
//                    keygen FILE (a new test key, written 0600; prints its account id and public key)
// Options: --key FILE (a 32-byte secret key in hex; the signer and fee payer), --gas N, --dry-run, --no-wait, --json.
// Environment: FERRATA_RPC, FERRATA_NAMES_PROGRAM, FERRATA_KEY_FILE, FERRATA_LEZ_JS (the web wallet's lez.js, for
// signing; default: deploy/testnet/web/wallet/lez.js of this repository). Node 22.7+ or 20.19+ (lez.js is an ES module
// without a package.json scope of its own: older Nodes cannot load it).
//
// A reveal that is refused or reverts publishes the name: `reveal` never sends one before its window, checks
// everything the program would refuse first (the commitment, the name, the list of names, the balance), forgets
// names that left the account when its list is full, and if the reveal still reverts inside its window, sends it
// again at once. Because the oldest commitment wins (program v2), a copier who read the name cannot keep it.
// --receiver-keys FILE: LEZ's wallet `--to-keys` format (the nullifier public key and the viewing public key in hex,
// one per line): what a sender needs to pay this name privately.
// Test-only (names/e2e/box.sh): FERRATA_NAMES_UNSAFE_REVEAL=1 sends a reveal without its checks;
// FERRATA_NAMES_TEST_COMMIT_OFFSET_MS moves a commit's now_ms (a copier backdating it).
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, resolve as resolvePath } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { randomBytes } from 'node:crypto';
import * as N from './names.js';

// Node 18 keeps WebCrypto under node:crypto; lez.js and names.js use the global.
if (!globalThis.crypto) globalThis.crypto = (await import('node:crypto')).webcrypto;

const HERE = dirname(fileURLToPath(import.meta.url));
// Each action is sent with names.js's gasFor (its worst case measured, with a margin; --gas overrides): a transaction
// outside its validity window, or out of gas, is charged its whole gas limit, so the limit is kept near what is needed.

/** lez.js needs ES module detection: Node 22.7+ (or 20.19+). */
function nodeSupported(v = process.versions.node) {
  const [major, minor] = v.split('.').map(Number);
  return major > 22 || (major === 22 && minor >= 7) || (major === 20 && minor >= 19);
}

function parse(argv) {
  const o = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) { o._.push(a); continue; }
    const k = a.slice(2);
    if (['no-target', 'no-receiver', 'force', 'dry-run', 'no-wait', 'json', 'clear', 'help'].includes(k)) o[k] = true;
    else if (i + 1 >= argv.length) die(`--${k} needs a value`);
    else o[k] = argv[++i];
  }
  return o;
}
const die = (m, code = 2) => { console.error(`ferrata-names: ${m}`); process.exit(code); };
const out = (o, json) => console.log(json ? JSON.stringify(o, (_, v) => (v instanceof Uint8Array ? N.hex(v) : v), 2) : o);

function receiverKeys(file) {
  const lines = readFileSync(file, 'utf8').split(/\s+/).filter(Boolean);
  if (lines.length !== 2) die(`${file}: two lines expected (npk hex, vpk hex)`);
  const npk = N.unhex(lines[0]); const vpk = N.unhex(lines[1]);
  if (npk.length !== N.NPK_LEN || vpk.length !== N.VPK_LEN) die(`${file}: npk must be ${N.NPK_LEN} bytes and vpk ${N.VPK_LEN}`);
  return { npk, vpk };
}
function target(o) { return o['no-target'] ? null : (o.target ?? undefined); }
function show(r, nowMs) {
  if (!r) return null;
  return { ...r, receiver: r.receiver ? { npk: N.hex(r.receiver.npk), vpk: N.hex(r.receiver.vpk) } : null,
    status: N.status(r, nowMs), expires: new Date(r.expiresMs).toISOString(),
    provisional: nowMs < N.finalAt(r), final: new Date(N.finalAt(r)).toISOString() };
}

async function lezjs() {
  if (!nodeSupported()) die(`Node ${process.versions.node} cannot load lez.js (an ES module without a package.json scope): use Node 22.7+ or 20.19+`);
  const p = process.env.FERRATA_LEZ_JS || resolvePath(HERE, '../../deploy/testnet/web/wallet/lez.js'); // empty = unset
  if (!existsSync(p)) die(`signing needs the web wallet's lez.js (FERRATA_LEZ_JS; not found at ${p})`);
  return import(pathToFileURL(p).href);
}

/**
 * A public transaction calling `call.to`, signed by `signer` (who also pays), from the web wallet's lez.js primitives
 * (encodeMessage, messageHash, sign, encodePublicTransaction, wrapPublic). The admission evidence is the message's
 * last field (`Vec<PublicAccountEvidence>`): the message is encoded without it and its empty vector replaced, so that
 * a PDA's evidence (`Pda { program, seed }`, variant 1) is written whatever evidence kinds lez.js knows.
 */
function signedCall(lez, { signer, call, nonce, gasLimit, maxFee }) {
  const base = lez.encodeMessage({ actors: call.actors, signers: [signer.accountId], to: call.to, data: call.data,
    fee: { payer: signer.accountId, gasLimit, tip: 0n, maxFee }, nonces: [[signer.accountId, nonce]], evidence: [] });
  if (base.length < 4 || base.slice(-4).some((b) => b !== 0)) throw new Error('lez.js message layout: no empty evidence vector at the end');
  const parts = [base.slice(0, -4), u32le(call.evidence.length)];
  for (const e of call.evidence) {
    if (e.pda) parts.push(Uint8Array.of(1), e.pda.program, e.pda.seed);
    else parts.push(Uint8Array.of(0), e.key);
  }
  const messageBytes = concatBytes(parts);
  const h = lez.messageHash(messageBytes);
  const tx = lez.encodePublicTransaction(messageBytes, [{ sig: lez.sign(signer.ssk, h), pk: signer.pk }]);
  return lez.wrapPublic(tx);
}
const u32le = (n) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; };
const concatBytes = (parts) => { const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0)); let o = 0; for (const p of parts) { out.set(p, o); o += p.length; } return out; };

async function send(names, o, msg) {
  const lez = await lezjs();
  const keyFile = o.key ?? process.env.FERRATA_KEY_FILE;
  if (!keyFile) die('a signed command needs --key FILE (or FERRATA_KEY_FILE)');
  const signer = lez.rawKeyNode(readFileSync(keyFile, 'utf8'));
  const me = lez.idToString(signer.accountId);
  const call = await names.call(me, msg);
  const seq = new lez.Sequencer(names.url);
  const [nonce] = await seq.nonces([signer.accountId]);
  const feeState = await seq.feeState();
  const gasLimit = Number(o.gas ?? N.gasFor(call.message));
  const build = (maxFee) => signedCall(lez, { signer, call, nonce: lez.big(nonce), gasLimit, maxFee });
  const { maxFee } = lez.feeFor({ gasLimit, size: build(0n).size, feeState });
  const tx = build(maxFee);
  const summary = { signer: me, action: msg.kind, name: call.message.name ?? null, nowMs: call.message.nowMs ?? null, hash: tx.hash, bytes: tx.size, gasLimit, maxFee: String(maxFee) };
  if (o['dry-run']) { out({ ...summary, dryRun: true, wire: tx.wire }, true); return { dryRun: true }; }
  // The fee reserve is taken before the program runs: a payer short of it is refused before anything is sent.
  const { reserve } = lez.feeFor({ gasLimit, size: tx.size, feeState });
  const balance = lez.big(await seq.balance(signer.accountId));
  if (balance < reserve) die(`balance ${balance} is below this transaction's fee reserve ${reserve}: nothing sent`, 1);
  await seq.send(tx.wire);
  if (o['no-wait']) { out({ ...summary, sent: true }, true); return { sent: true }; }
  for (let i = 0; i < 90; i++) {
    await new Promise((r) => setTimeout(r, 2000));
    const t = await seq.transaction(tx.hash).catch(() => null);
    if (!t) continue;
    const blockId = Number(Array.isArray(t) ? t[1] : t.block_id);
    const blk = lez.decodeBlock(await seq.block(blockId));
    const pos = blk.transactions.findIndex((d) => d.hash === tx.hash);
    const reverted = pos >= 0 && blk.reverted.includes(pos);
    out({ ...summary, block: blockId, result: reverted ? 'reverted (refused; the fee was charged)' : 'applied' }, true);
    return { reverted };
  }
  die('not in a block after three minutes', 3);
}

function signerId(lez, o) {
  const keyFile = o.key ?? process.env.FERRATA_KEY_FILE ?? die('a signed command needs --key FILE (or FERRATA_KEY_FILE)');
  return lez.idToString(lez.rawKeyNode(readFileSync(keyFile, 'utf8')).accountId);
}

const done = (r) => process.exit(r?.reverted ? 1 : 0);
const stateFile = (o, name) => o.state ?? `${N.normalize(name)}.commit.json`;

/** Step 1: commit (a hash of name, signer and a fresh salt); the salt and the reveal's window go to the state file. */
async function commit(names, o, name) {
  const lez = await lezjs();
  const me = signerId(lez, o);
  const offset = Number(process.env.FERRATA_NAMES_TEST_COMMIT_OFFSET_MS ?? 0); // test-only
  const c = await names.commitCall(me, name, { nowMs: Date.now() - 30_000 + offset });
  const file = stateFile(o, name);
  if (!o['dry-run']) {
    writeFileSync(file, JSON.stringify({ name: c.name, signer: me, salt: N.hex(c.salt), commitment: N.hex(c.commitment), nowMs: c.nowMs,
      revealFrom: c.revealFrom, revealUntil: c.revealUntil, target: o['no-target'] ? null : (o.target ?? me),
      receiverKeys: o['receiver-keys'] ?? null }, null, 2) + '\n', { mode: 0o600 });
    console.error(`commitment for ${c.name} kept in ${file}: reveal from ${new Date(c.revealFrom + 30_000).toISOString()} until ${new Date(c.revealUntil - N.COMMIT_SLACK_MS).toISOString()}`);
  }
  return send(names, o, { kind: 'Commit', commitment: c.commitment, nowMs: c.nowMs });
}

/**
 * Step 2: reveal (Register with the salt of the state file), never early, checked first, sent again at once if it
 * reverts inside its window.
 */
async function reveal(names, o, name) {
  const lez = await lezjs();
  const me = signerId(lez, o);
  const st = JSON.parse(readFileSync(stateFile(o, name), 'utf8'));
  const salt = N.unhex(st.salt);
  const message = () => ({ kind: 'Register', name: st.name, target: st.target, salt, nowMs: Date.now() - 30_000,
    receiver: st.receiverKeys ? receiverKeys(st.receiverKeys) : null });
  if (process.env.FERRATA_NAMES_UNSAFE_REVEAL === '1') { // test-only: the e2e's leaked reveal
    console.error('ferrata-names: FERRATA_NAMES_UNSAFE_REVEAL=1: sending without checks (test only)');
    return send(names, o, message());
  }
  if (o.force) console.error('ferrata-names: --force is ignored: a reveal that is refused or reverts publishes the name');
  const check = async () => {
    const nowMs = Date.now() - 30_000;
    if (nowMs < st.revealFrom) die(`too early: the reveal window opens in ${Math.ceil((st.revealFrom - nowMs) / 1000)} s; nothing sent`, 1);
    if (nowMs + N.COMMIT_SLACK_MS > st.revealUntil) die('the commitment has expired: commit again; nothing sent', 1);
    return names.preflightReveal(me, { name: st.name, salt, nowMs });
  };
  let p = await check();
  if (p.lost.length && !p.problems.some((x) => x.code === 251)) {
    const user = await names.user(me);
    if (user.names.length + 1 > N.MAX_NAMES) {
      console.error(`the list of names is full: forgetting ${p.lost.join(', ')} (they left this account) first`);
      const f = await send(names, o, { kind: 'Forget', names: p.lost });
      if (f?.reverted) die('forget reverted; nothing revealed', 1);
      p = await check();
    }
  }
  if (!p.ok) die(`not revealing (nothing sent): ${p.problems.map((x) => `${x.code} ${x.reason}`).join('; ')}`, 1);
  const first = await send(names, o, message());
  if (!first?.reverted) return first;
  if (Date.now() - 30_000 + N.COMMIT_SLACK_MS <= st.revealUntil) {
    console.error('the reveal reverted inside its window: the name is public now; sending it again at once');
    const again = await check();
    if (!again.ok) die(`not sending again: ${again.problems.map((x) => `${x.code} ${x.reason}`).join('; ')}`, 1);
    return send(names, o, message());
  }
  return first;
}

async function main() {
  const o = parse(process.argv.slice(2));
  const [cmd, a1, a2] = o._;
  if (!cmd || o.help) {
    console.log(readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n').slice(1, 27).map((l) => l.replace(/^\/\/ ?/, '')).join('\n'));
    return;
  }
  const nowMs = Date.now();
  if (cmd === 'rules') {
    return out({ minLen: N.MIN_LEN, maxLen: N.MAX_LEN, charset: 'a-z 0-9 -', noLeadingTrailingHyphen: true, noDoubleHyphen: true,
      reserved: N.RESERVED, reservedVariants: 'any -separated part with a reserved skeleton, as it is or without its edge digits',
      skeleton: '0→o, 1→l, then rn→m, then vv→w: one name per skeleton', maxNamesPerAccount: N.MAX_NAMES,
      termDays: N.TERM_MS / 86_400_000, graceDays: N.GRACE_MS / 86_400_000, fee: 'none on the test network',
      clockSlackMs: N.CLOCK_SLACK_MS, commitSlackMs: N.COMMIT_SLACK_MS, revealFromMs: N.COMMIT_SLACK_MS + N.MIN_REVEAL_DELAY_MS,
      revealWindowMs: N.MAX_REVEAL_WINDOW_MS, contestMs: N.CONTEST_MS, priority: 'the oldest commitment wins' }, true);
  }
  if (cmd === 'check') {
    const name = N.normalize(a1 ?? die('check NAME'));
    const bad = N.checkName(name);
    out({ name, ok: !bad, ...(bad ? { code: bad.code, reason: bad.reason } : {}) }, true);
    process.exit(bad ? 1 : 0);
  }
  if (cmd === 'keygen') {
    const lez = await lezjs();
    const file = a1 ?? die('keygen FILE');
    if (existsSync(file)) die(`${file} exists`);
    let sk;
    for (;;) { sk = randomBytes(32); try { lez.rawKeyNode(N.hex(sk)); break; } catch { /* out of range: draw again */ } }
    writeFileSync(file, N.hex(sk) + '\n', { mode: 0o600 });
    const node = lez.rawKeyNode(N.hex(sk));
    return out({ file, account: lez.idToString(node.accountId), publicKey: N.hex(node.pk) }, true);
  }
  const rpc = o.rpc ?? process.env.FERRATA_RPC ?? die('--rpc URL (or FERRATA_RPC)');
  const program = o.program ?? process.env.FERRATA_NAMES_PROGRAM ?? die('--program ID (or FERRATA_NAMES_PROGRAM)');
  const names = new N.Names({ rpc, program });
  names.url = rpc;
  switch (cmd) {
    case 'resolve': {
      const r = await names.resolve(a1 ?? die('resolve NAME'), nowMs);
      out(show(r, nowMs), true); process.exit(r ? 0 : 1);
    }
    // falls through (unreachable)
    case 'whois': {
      const name = N.normalize(a1 ?? die('whois NAME'));
      const r = await names.record(name);
      const lookAlike = r && r.name !== name ? r.name : null;
      return out({ name, skeleton: N.skeleton(name), account: N.idString(await N.nameAccountId(names.program, name)),
        record: show(r, nowMs), status: lookAlike ? 'look-alike registered' : N.status(r, nowMs), lookAlike }, true);
    }
    case 'reverse': {
      const n = await names.reverse(a1 ?? die('reverse ACCOUNT'), nowMs);
      out({ account: a1, name: n }, true); process.exit(n ? 0 : 1);
    }
    // falls through (unreachable)
    case 'account': return out({ account: a1, ...(await names.user(a1 ?? die('account ACCOUNT'))) }, true);
    case 'commit': return done(await commit(names, o, a1 ?? die('commit NAME')));
    case 'reveal': return done(await reveal(names, o, a1 ?? die('reveal NAME')));
    case 'register': { // commit, wait for the reveal window, reveal
      const c = await commit(names, o, a1 ?? die('register NAME'));
      if (c?.reverted) return done(c);
      const st = JSON.parse(readFileSync(stateFile(o, a1), 'utf8'));
      const wait = st.revealFrom + 30_000 - Date.now() + 1_000;
      if (wait > 0) { console.error(`waiting ${Math.ceil(wait / 1000)} s for the reveal window (front-running protection)`); await new Promise((r) => setTimeout(r, wait)); }
      return done(await reveal(names, o, a1));
    }
    case 'renew': return done(await send(names, o, { kind: 'Renew', name: a1 ?? die('renew NAME') }));
    case 'transfer': return done(await send(names, o, { kind: 'Transfer', name: a1 ?? die('transfer NAME TO'), to: a2 ?? die('transfer NAME TO') }));
    case 'accept': return done(await send(names, o, { kind: 'Accept', name: a1 ?? die('accept NAME') }));
    case 'release': return done(await send(names, o, { kind: 'Release', name: a1 ?? die('release NAME') }));
    case 'forget': {
      const lez = await lezjs();
      const me = signerId(lez, o);
      const list = o._.slice(1).length ? o._.slice(1) : await names.lostNames(me);
      if (!list.length) { out({ account: me, forget: [], note: 'nothing to forget' }, true); return; }
      return done(await send(names, o, { kind: 'Forget', names: list }));
    }
    case 'set-target': { // what is not given stays as it is
      const cur = await names.record(N.normalize(a1 ?? die('set-target NAME')));
      if (!cur) die(`${a1} is not registered`, 1);
      const t = o['no-target'] ? null : (o.target ?? cur.target);
      const receiver = o['no-receiver'] ? null : (o['receiver-keys'] ? receiverKeys(o['receiver-keys']) : cur.receiver);
      return done(await send(names, o, { kind: 'SetTarget', name: a1, target: t, receiver }));
    }
    case 'set-primary': return done(await send(names, o, { kind: 'SetPrimary', name: o.clear ? null : (a1 ?? die('set-primary NAME | --clear')) }));
    default: die(`unknown command ${cmd}`);
  }
}

main().catch((e) => die(e?.message ?? String(e), 1));
