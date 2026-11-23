// The public state of an account's messages on ferrata_msg_v2, as JSON: its profile, its inbox, and for each entry
// (and each `--envelope SENDER:SEQ`) whether the envelope account still holds a ciphertext, and the raw states' bytes in
// hex (for the e2e's search for plaintext).
//   node test/probe.mjs RPC PROGRAM ACCOUNT [--envelope SENDER:SEQ]...
const [rpc, programS, accountS, ...rest] = process.argv.slice(2);
const lez = await import('../../wallet/lez.js');
const M = await import('../ferrata-msg.js');
const C = await import('../client.js');
const seq = new lez.Sequencer(rpc);
const program = lez.idFromString(programS), account = lez.idFromString(accountS);
const raw = async (a) => M.hex(await C.actorState(seq, a, program));
const profileHex = await raw(account);
const inboxHex = await raw(M.inboxAccount(program, account));
const inbox = M.decodeInbox(lez.unhex(inboxHex));
const extra = [];
for (let i = 0; i < rest.length; i++) if (rest[i] === '--envelope') { const [s, q] = rest[++i].split(':'); extra.push({ from: lez.idFromString(s), seq: BigInt(q) }); }
const envelopes = [];
for (const e of [...(inbox?.entries ?? []), ...extra]) {
  const h = await raw(M.envelopeAccount(program, account, e.from, e.seq));
  envelopes.push({ n: e.n != null ? String(e.n) : null, from: lez.idToString(e.from), seq: String(e.seq), held: h.length > 0, hex: h });
}
const p = M.decodeProfile(lez.unhex(profileHex));
console.log(JSON.stringify({
  key: p.key ? { epoch: p.key.epoch, hybrid: !!p.key.mlkem768, fingerprint: M.fingerprint(p.key) } : null,
  policy: p.policy, contacts: p.contacts.length, blocked: p.blocked.length, sentTotal: String(p.sentTotal),
  inbox: inbox ? { nextN: String(inbox.nextN), entries: inbox.entries.map((e) => ({ n: String(e.n), from: lez.idToString(e.from), seq: String(e.seq), len: e.len, contact: e.contact })) } : null,
  envelopes, profileHex, inboxHex,
}));
