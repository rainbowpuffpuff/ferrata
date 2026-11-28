// client.js: ferrata_msg_v2 over LEZ's JSON-RPC: read an account's profile, inbox and envelopes (the actor states are
// the same on both LEZ versions); sign and send as public transactions through the optional net's selected codec; and
// read the outcome from rc3's block or v0.4.0's receipt. Existing callers default to rc3's ../wallet/lez.js.
// Used by the CLI; the wallet imports it. ferrata_msg (v1) is read only: legacyMailbox.
import * as lez from '../wallet/lez.js';
import * as M from './ferrata-msg.js';

// Gas limits per action. lez/harness/tests/messages_v2.rs and messages_findings.rs read these two lines and check every
// applied transaction of their scenarios against them, the worst cases included (lez/results/messages-v2.md, its table
// of the largest transaction of each kind). An edit's limit grows with the ids it names: review finding N2 (a bulk edit,
// 64 ids removed and 64 added, measured 273,830 against the flat 250,000 of round 2). Each id is compared with up to 64
// list entries, byte by byte until they differ: ids sharing 31 bytes (an owner's own choice; account ids are hashes)
// cost about 243 cycles a comparison, so 18,000 an id covers them. Review finding M2: v1's send limit
// was 1,500,000, below a v1 send between two near-full inboxes (1,550,595 measured). A send outside its validity window
// is charged its whole limit, so the send limit stays near use. v1 is read only here; GAS_V1 is for other clients.
export const GAS_V1 = { publish: 1_200_000n, send: 2_000_000n, edit: 1_200_000n };
export const GAS_V2 = { publish: 250_000n, edit: 200_000n, editPer: 18_000n, send: 600_000n, removeBase: 150_000n, removePer: 45_000n };

/**
 * The gas limit of a v2 action: 'publish', 'send', 'edit' (policy, contacts, block) naming `k` ids, or 'remove' of `k`
 * envelopes.
 */
export function gasFor(kind, k = 0) {
  if (kind === 'remove') return GAS_V2.removeBase + GAS_V2.removePer * BigInt(k);
  if (kind === 'edit') return GAS_V2.edit + GAS_V2.editPer * BigInt(k);
  const g = GAS_V2[kind];
  if (g === undefined) throw new Error(`unknown action ${kind}`);
  return g;
}

export const actorOf = (account, program) => ({ account, program });

/** The raw state of the actor (account, program), or an empty array. */
export async function actorState(seq, account, program) {
  const acc = await seq.account(account);
  const states = acc?.data?.actor_states ?? acc?.data?.actorStates ?? {};
  const v = states[lez.idToString(program)];
  if (v == null) return new Uint8Array(0);
  if (Array.isArray(v)) return Uint8Array.from(v);
  if (typeof v === 'string') return /^[0-9a-f]*$/i.test(v) && v.length % 2 === 0 ? lez.unhex(v) : lez.base64.decode(v);
  throw new Error('unexpected actor state encoding');
}
export async function profile(seq, program, account) { return M.decodeProfile(await actorState(seq, account, program)); }
export async function inbox(seq, program, owner) { return M.decodeInbox(await actorState(seq, M.inboxAccount(program, owner), program)); }
export async function envelope(seq, program, recipient, sender, n) {
  return M.decodeEnvelope(await actorState(seq, M.envelopeAccount(program, recipient, sender, n), program));
}
/** ferrata_msg (v1)'s mailbox of `account` (read only). */
export async function legacyMailbox(seq, program, account) { return M.decodeLegacyMailbox(await actorState(seq, account, program)); }

/** Every message of `node`'s inbox, decrypted locally (M.openInbox), and the inbox itself. */
export async function messages(seq, { program, node, net = null }) {
  const ix = await inbox(seq, program, node.accountId);
  const envelopes = new Map();
  for (const e of ix?.entries ?? []) {
    const env = await envelope(seq, program, node.accountId, e.from, e.seq);
    if (env) envelopes.set(String(e.n), env.body);
  }
  const accountSecret = net ? net.messagingSecretOf(node) : node.ssk;
  return { inbox: ix, list: M.openInbox({ inbox: ix, envelopes, recipientId: node.accountId, accountSecret }) };
}

/** Sign `data` to the signer's own profile, declaring the actors of `accounts` too, pay with the signer, send it. */
export async function call(seq, { node, program, data, accounts = [], gasLimit, nonce = null, net = null, gate = null }) {
  const feeState = await seq.feeState();
  if (nonce == null) nonce = lez.big((await seq.nonces([node.accountId]))[0]);
  // `gate` (a page's version re-read) runs after the reads, right before signing: nothing is awaited in between.
  if (gate && !(await gate())) throw new Error(net?.error ?? 'the network\'s LEZ version could not be read, so nothing is signed');
  const actors = [actorOf(node.accountId, program), ...accounts.map((a) => actorOf(a, program))];
  const args = { payer: node, to: actorOf(node.accountId, program), actors, data, nonce, gasLimit, evidence: [] };
  // Omitting net is deliberately the old rc3 construction, including its size calculation and witness layout.
  const build = net ? (a) => net.buildCall(a) : lez.buildCall;
  const probe = build({ ...args, maxFee: 0n });
  const { maxFee, reserve } = lez.feeFor({ gasLimit, size: probe.size, feeState });
  const tx = build({ ...args, maxFee });
  const hash = await seq.send(tx.wire);
  return { hash: typeof hash === 'string' ? hash : tx.hash, size: tx.size, reserve, maxFee, gasLimit };
}

/**
 * Wait until the transaction is in a block, then read its outcome from that block on rc3 or its receipt on v0.4.0
 * (a refusal by the program or by LEZ: the fee is charged and nothing changes). Returns { block, reverted }, or null
 * after `timeoutMs`. Reading the actor states above never needs the selected codec.
 */
export async function outcome(seq, hash, { timeoutMs = 180_000, everyMs = 1500, net = null } = {}) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const t = await seq.transaction(hash).catch(() => null);
    if (t) {
      const block = Number(Array.isArray(t) ? t[1] : t.block_id ?? t);
      if (net?.version === 'v0.4.0') {
        const found = await net.outcome(hash);
        if (found !== 'missing') return { block, reverted: found === 'reverted' };
      } else if (net) {
        const found = await net.outcome(hash, await seq.block(block));
        if (found === 'missing') throw new Error(`transaction ${hash} is not in block ${block}`);
        return { block, reverted: found === 'reverted' };
      } else {
        const blk = lez.decodeBlock(await seq.block(block));
        const pos = blk.transactions.findIndex((d) => d.hash === hash);
        if (pos < 0) throw new Error(`transaction ${hash} is not in block ${block}`);
        return { block, reverted: blk.reverted.includes(pos) };
      }
    }
    await new Promise((ok) => setTimeout(ok, everyMs));
  }
  return null;
}

async function done(seq, sent, wait, net = null) {
  if (!wait) return sent;
  const o = await outcome(seq, sent.hash, { net });
  if (!o) throw new Error(`transaction ${sent.hash} not in a block after three minutes`);
  return { ...sent, ...o };
}

// ---------------------------------------------------------------- the owner's actions
export async function publish(seq, { node, program, epoch = null, hybrid = true, wait = true, net = null, gate = null }) {
  const mine = await profile(seq, program, node.accountId);
  const e = epoch ?? (mine.key ? mine.key.epoch + 1 : 0);
  const record = M.publicRecord(M.deriveKeys(net ? net.messagingSecretOf(node) : node.ssk, e), { hybrid });
  const sent = await call(seq, { node, program, data: M.encodePublish(record), accounts: [M.inboxAccount(program, node.accountId)], gasLimit: gasFor('publish'), net, gate });
  return { ...(await done(seq, sent, wait, net)), record, fingerprint: M.fingerprint(record) };
}
const edit = (data, ids) => async (seq, { node, program, wait = true, net = null, gate = null }) => done(seq, await call(seq, { node, program, data, gasLimit: gasFor('edit', ids), net, gate }), wait, net);
export const setPolicy = (seq, o) => edit(M.encodeSetPolicy(M.POLICY[o.policy]), 0)(seq, o);
const ids = (o) => (o.add ?? []).length + (o.remove ?? []).length;
const listed = (field, encode) => async (seq, o) => {
  const p = await profile(seq, o.program, o.node.accountId);
  const why = M.preflightEdit(p[field], o.add ?? [], o.remove ?? []);
  if (why) return { refused: why, why: `the ${field} list would hold more than ${M.MAX_LIST} ids` };
  return edit(encode(o.add ?? [], o.remove ?? []), ids(o))(seq, o);
};
export const contacts = listed('contacts', M.encodeContacts);
export const block = listed('blocked', M.encodeBlock);

/** Delete messages `ns` (numbers) from `node`'s inbox: their envelopes are emptied too. */
export async function remove(seq, { node, program, ns, upto = null, wait = true, net = null, gate = null }) {
  const ix = await inbox(seq, program, node.accountId);
  if (!ix) throw new Error('no inbox yet (publish a key first): nothing to delete');
  const gone = ix.entries.filter((e) => (upto != null ? e.n <= BigInt(upto) : ns.map(BigInt).includes(e.n)));
  if (upto == null) {
    const missing = ns.map(BigInt).filter((n) => !ix.entries.some((e) => e.n === n));
    if (missing.length) throw new Error(`the inbox holds no message ${missing.join(', ')} (NOT_FOUND): nothing sent`);
  }
  const accounts = [M.inboxAccount(program, node.accountId), ...gone.map((e) => M.envelopeAccount(program, node.accountId, e.from, e.seq))];
  const data = upto != null ? M.encodeClear(BigInt(upto)) : M.encodeDelete(ns);
  return { ...(await done(seq, await call(seq, { node, program, data, accounts, gasLimit: gasFor('remove', gone.length), net, gate }), wait, net)), removed: gone.length };
}

// ---------------------------------------------------------------- sending
/**
 * Encrypt `text` to `to`'s published key and deliver it. Checks first, paying nothing when a check fails: the key
 * exists; its fingerprint is `expectKey`, if given (the RPC could otherwise substitute a key); every rule of the
 * program (preflightSend). `nowMs` defaults to a few seconds behind this clock and never before the last send.
 */
export async function send(seq, { node, program, to, text, expectKey = null, nowMs = null, force = false, wait = true, sentMs = Date.now(), net = null, gate = null }) {
  const [mine, theirRaw, ix] = await Promise.all([profile(seq, program, node.accountId), actorState(seq, to, program), inbox(seq, program, to)]);
  let theirs;
  try {
    theirs = M.decodeProfile(theirRaw);
  } catch {
    // Not a profile: one of the program's own accounts (an inbox or an envelope); the program answers BAD_STATE (review N3).
    return { refused: 'BAD_STATE', why: 'the address is one of the messages program\'s own accounts (an inbox or an envelope), not an account' };
  }
  if (!theirs.key) return { refused: 'NO_KEY', why: 'the recipient has published no messaging key' };
  const fingerprint = M.fingerprint(theirs.key);
  if (expectKey != null && !M.hasFingerprint(theirs.key, expectKey)) {
    return { refused: 'KEY_MISMATCH', why: `the recipient's key has fingerprint ${fingerprint}, not ${expectKey}`, fingerprint };
  }
  const now = nowMs != null ? BigInt(nowMs) : [BigInt(Date.now() - 5_000), mine.lastSendMs].reduce((a, b) => (a > b ? a : b));
  const body = M.seal({ sender: node.accountId, recipient: to, key: theirs.key, text, sentMs });
  const seqNo = mine.sentTotal;
  const pre = M.preflightSend({ senderId: node.accountId, recipientId: to, senderProfile: mine, recipientProfile: theirs, recipientInbox: ix, body, nowMs: now, seq: seqNo });
  if (pre && !force) return { refused: pre, fingerprint, why: 'the program would refuse it' };
  const accounts = [to, M.inboxAccount(program, to), M.envelopeAccount(program, to, node.accountId, seqNo)];
  const sent = await call(seq, { node, program, data: M.encodeSend({ to, nowMs: now, seq: seqNo, body }), accounts, gasLimit: gasFor('send'), net, gate });
  return { ...(await done(seq, sent, wait, net)), fingerprint, preflight: pre, bucket: body.length, version: body[0], seq: seqNo };
}
