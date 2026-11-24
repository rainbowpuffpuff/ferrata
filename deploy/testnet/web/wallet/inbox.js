// The encrypted inbox (the messages program, ferrata_msg): publish this account's messaging key, send end-to-end encrypted
// messages (hybrid X25519 + ML-KEM-768 envelopes, padded to fixed sizes), read and decrypt the inbox in the page, block
// senders, keep contacts, accept contacts only. Keys derive from the account's own signing secret (the messages
// library's deriveKeys): nothing new to store or back up. Every rule of the program is checked before paying (preflightSend).
import * as M from './msg/ferrata-msg.js';
import * as C from './msg/client.js';

const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(typeof k === 'string' ? document.createTextNode(k) : k);
  return e;
};
const WHY = {
  NO_KEY: 'the recipient has not published a messaging key yet', BLOCKED: 'the recipient has blocked this account',
  NOT_A_CONTACT: 'the recipient accepts messages from contacts only', INBOX_FULL: 'the recipient\'s inbox is full',
  STRANGER_LIMIT: 'the recipient has unread messages from you already', STRANGERS_FULL: 'the recipient takes no more new senders until they read',
  RATE_LIMITED: 'too many messages this hour', STALE_KEY: 'the recipient changed keys; reload and try again',
  SELF_SEND: 'that is this account', BAD_SIZE: 'the message is too long', CLOCK_BACKWARDS: 'this device\'s clock is behind',
};

export function mountInbox(root, c) {
  const P = c.cfg.msg?.program;
  root.textContent = '';
  if (!P) {
    root.append(el('p', { class: 'msg warn', 'data-testid': 'inbox-unconfigured', text: 'No messages program is configured on this network (config.json: msg.program).' }));
    return { render() {} };
  }
  const program = c.lez.idFromString(P);
  const status = el('p', { class: 'fine', 'data-testid': 'inbox-status', text: '…' });
  const publish = el('button', { 'data-testid': 'inbox-publish', text: 'Publish my messaging key' });
  const policy = el('select', { 'data-testid': 'inbox-policy' }, el('option', { value: 'open', text: 'anyone can write to me' }), el('option', { value: 'contactsOnly', text: 'contacts only' }));
  const to = el('input', { placeholder: 'to: account id, receive code or name', autocomplete: 'off', spellcheck: 'false', 'data-testid': 'inbox-to' });
  // spellcheck off: a browser's enhanced (cloud) spell check would send the plaintext to its provider.
  const text = el('textarea', { rows: '3', placeholder: 'message (end-to-end encrypted)', spellcheck: 'false', autocomplete: 'off', autocorrect: 'off', autocapitalize: 'off', 'data-testid': 'inbox-text' });
  const count = el('span', { class: 'fine', 'data-testid': 'inbox-count' });
  const send = el('button', { class: 'primary', 'data-testid': 'inbox-send', text: 'Encrypt and send' });
  const out = el('p', { class: 'msg', role: 'status', 'data-testid': 'inbox-out' });
  const list = el('ul', { class: 'list', 'data-testid': 'inbox-list' });
  root.append(status, el('div', { class: 'row' }, publish, el('label', { class: 'inline' }, 'Who can write: ', policy)),
    el('h3', { text: 'Write' }), to, text, el('div', { class: 'row' }, send, count), out,
    el('h3', { text: 'Inbox' }), list,
    el('div', { class: 'privacy' }, el('b', { text: 'Public on LEZ: ' }), 'that this account sent a message to that account, when, and its padded size (512 to 4096 bytes); your public messaging key; your contacts, blocked list and policy. ',
      el('b', { text: 'Private: ' }), 'the text, readable only with the recipient\'s keys (X25519 and ML-KEM-768, derived from their account key). Reading happens in this page; nothing is decrypted elsewhere.'));

  let mine = null;
  const me = () => c.acct();
  const keysOf = (a, epoch = 0) => M.deriveKeys(a.node.ssk, epoch);
  const cap = () => M.capacity(M.V_HYBRID, M.BUCKETS[M.BUCKETS.length - 1]);
  text.oninput = () => { const n = new TextEncoder().encode(text.value).length; count.textContent = `${n} / ${cap()} bytes`; };
  text.oninput();

  async function outcome(hash) {
    for (let i = 0; i < 90; i++) {
      await new Promise((r) => setTimeout(r, 2000));
      const t = await c.seq.transaction(hash).catch(() => null);
      if (!t) continue;
      const bid = Number(Array.isArray(t) ? t[1] : t.block_id);
      const blk = c.lez.decodeBlock(await c.seq.block(bid));
      const o = c.lez.outcomeIn(blk, hash);
      return { block: bid, reverted: o === 'reverted', missing: o === 'missing' };
    }
    return null;
  }
  async function act(label, data, gasLimit, others = []) {
    const a = me();
    out.className = 'msg'; out.textContent = `${label}: signing and sending…`;
    const r = await C.call(c.seq, { node: a.node, program, data, others, gasLimit });
    out.textContent = `${label}: sent ${r.hash}; waiting for a block…`;
    const o = await outcome(r.hash);
    if (!o) throw new Error(`${label}: not in a block after three minutes (${r.hash})`);
    if (o.reverted) throw new Error(`${label}: refused by the messages program in block ${o.block} (the fee was charged). ${r.hash}`);
    if (o.missing) throw new Error(`${label}: the RPC places ${r.hash} in block ${o.block}, but that block does not list it: outcome unknown.`);
    await render(); // the mailbox as the block left it, before saying done
    c.refresh();
    out.textContent = `${label}: done in block ${o.block}. ${r.hash}`; out.className = 'msg ok';
    return o;
  }
  async function recipientId(s) {
    const t = s.trim();
    const code = t.match(/^lez:([1-9A-HJ-NP-Za-km-z]{32,44})\?pk=/);
    if (code) return c.lez.idFromString(code[1]);
    if (c.names && /^[a-zA-Z0-9-]{3,32}(\.lez)?$/.test(t) && !/^[1-9A-HJ-NP-Za-km-z]{43,44}$/.test(t)) {
      const r = await c.names.resolve(t);
      if (!r) throw new Error(`No live name "${t}".`);
      return c.lez.idFromString(r.target);
    }
    return c.lez.idFromString(t);
  }

  publish.onclick = async () => {
    try { await act('Publish key', M.encodePublish(M.publicRecord(keysOf(me()))), C.GAS.publish); } catch (e) { out.textContent = e.message; out.className = 'msg bad'; }
  };
  policy.onchange = async () => {
    try { await act('Policy', M.encodeSetPolicy(M.POLICY[policy.value]), C.GAS.edit); } catch (e) { out.textContent = e.message; out.className = 'msg bad'; }
  };
  send.onclick = async () => {
    out.className = 'msg'; out.textContent = '';
    try {
      const a = me(), toId = await recipientId(to.value);
      const [theirs, own] = await Promise.all([C.mailbox(c.seq, toId, program), C.mailbox(c.seq, a.node.accountId, program)]);
      if (!theirs?.key) throw new Error(`Not sent: ${WHY.NO_KEY}.`);
      const nowMs = Date.now();
      const body = M.seal({ sender: a.node.accountId, recipient: toId, key: theirs.key, text: text.value, sentMs: nowMs });
      const why = M.preflightSend({ senderId: a.node.accountId, recipientId: toId, sender: own, recipient: theirs, body, nowMs });
      if (why) throw new Error(`Not sent (${why}): ${WHY[why] ?? 'the program would refuse it'}. No fee was paid.`);
      await act('Send', M.encodeSend({ to: toId, nowMs, body }), C.GAS.send, [toId]);
      text.value = ''; text.oninput();
    } catch (e) { out.textContent = e.message; out.className = 'msg bad'; }
  };

  async function render() {
    const a = me();
    if (!a) return;
    try {
      mine = await C.mailbox(c.seq, a.node.accountId, program);
      const fp = mine?.key ? M.fingerprint(mine.key) : null;
      status.textContent = fp ? `Your messaging key is published (fingerprint ${fp}); ${mine.inbox.length} message(s); ${mine.contacts.length} contact(s), ${mine.blocked.length} blocked.`
        : 'Your messaging key is not published yet: publish it so others can write to you.';
      publish.hidden = !!fp;
      policy.value = mine?.policy ?? 'open';
      list.textContent = '';
      const items = M.readInbox(mine, a.node.accountId, a.node.ssk).reverse();
      if (!items.length) list.append(el('li', { text: 'No messages.' }));
      for (const m of items) {
        const who = c.names ? await c.names.reverse(m.from).catch(() => null) : null;
        const li = el('li', { 'data-testid': 'inbox-item' },
          el('span', { text: m.error ? `(cannot decrypt: ${m.error})` : m.text }),
          el('span', { class: 'amt', text: new Date(Number(m.atMs)).toLocaleString() }),
          el('span', { class: 'sub', text: `from ${who ? who + '.lez · ' : ''}${m.from} · #${m.seq}` }));
        const isContact = mine.contacts.some((x) => c.lez.equalBytes(x, m.fromId));
        const row = el('span', { class: 'sub' });
        const b1 = el('button', { class: 'ghost small', 'data-testid': 'inbox-contact', text: isContact ? 'Contact ✓' : 'Add contact' });
        const b2 = el('button', { class: 'ghost small', 'data-testid': 'inbox-block', text: 'Block' });
        const b3 = el('button', { class: 'ghost small', 'data-testid': 'inbox-delete', text: 'Delete' });
        b1.disabled = isContact;
        b1.onclick = () => act('Add contact', M.encodeContacts([m.fromId], []), C.GAS.edit).catch((e) => { out.textContent = e.message; out.className = 'msg bad'; });
        b2.onclick = () => act('Block', M.encodeBlock([m.fromId], []), C.GAS.edit).catch((e) => { out.textContent = e.message; out.className = 'msg bad'; });
        b3.onclick = () => act('Delete', M.encodeDelete([m.seq]), C.GAS.edit).catch((e) => { out.textContent = e.message; out.className = 'msg bad'; });
        row.append(b1, ' ', b2, ' ', b3);
        li.append(row);
        list.append(li);
      }
    } catch (e) { status.textContent = `Could not read the mailbox: ${e.message}`; }
  }
  return { render };
}
