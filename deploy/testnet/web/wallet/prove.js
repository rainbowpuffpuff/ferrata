// Prove an Ethereum fact on LEZ (RFP-021): the user picks a Sepolia contract and storage slot (or an account); the page
// reads the attestation instance's newest record from LEZ, fetches the proof from Ethereum (through the user's browser
// wallet or the network's endpoint), checks it itself against the record's block, then sends the `Attest` transaction
// to the attestation program and reads the outcome from LEZ's block. Nothing here needs an Ethereum key.
import * as at from './attest.js';
import * as eth from './eth.js';

const h = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(k);
  return e;
};

export function mountProve(el, c) {
  const A = c.cfg.attestation ?? {};
  el.textContent = '';
  el.append(h('p', { class: 'lead', text: 'Attest a value of Ethereum\'s state on LEZ: the attestation program checks the Merkle proof against a Sepolia block whose header it verified from Ethereum\'s sync committee, and the transaction is the proof that LEZ accepted it.' }));
  if (!A.program || !(A.instances ?? []).length) {
    el.append(h('p', { class: 'msg warn', 'data-testid': 'prove-unconfigured', text: 'No attestation program is configured on this network yet. When the operator deploys it, this tab attests storage slots and accounts of Sepolia.' }));
    return;
  }
  const program = c.lez.idFromString(A.program);
  const sel = h('select', { id: 'prove-instance', 'data-testid': 'prove-instance' });
  A.instances.forEach((ins, i) => sel.append(h('option', { value: i, text: `${ins.name ?? 'instance'} · chain ${ins.chainId} · ${ins.id.slice(0, 10)}…` })));
  const status = h('p', { class: 'fine', 'data-testid': 'prove-record', text: 'Reading the instance…' });
  const src = h('select', { id: 'prove-source', 'data-testid': 'prove-source' },
    h('option', { value: 'wallet', text: 'my browser wallet (MetaMask or another)' }),
    ...(A.ethRpc ? [h('option', { value: 'rpc', text: 'this network\'s Ethereum endpoint' })] : []));
  if (A.ethRpc) src.value = 'rpc';
  const addr = h('input', { id: 'prove-address', placeholder: '0x… contract or account', autocomplete: 'off', spellcheck: 'false', 'data-testid': 'prove-address' });
  const kind = h('select', { id: 'prove-kind', 'data-testid': 'prove-kind' },
    h('option', { value: 'slot', text: 'a storage slot' }), h('option', { value: 'mapping', text: 'a mapping entry (e.g. an ERC-20 balance)' }),
    h('option', { value: 'account', text: 'the account itself (nonce, balance, code)' }));
  const slot = h('input', { id: 'prove-slot', placeholder: 'slot: 0x… or a number', autocomplete: 'off', 'data-testid': 'prove-slot' });
  const key = h('input', { id: 'prove-key', placeholder: 'mapping key: 0x… address', autocomplete: 'off', 'data-testid': 'prove-key' });
  const cmp = h('select', { id: 'prove-cmp', 'data-testid': 'prove-cmp' }, h('option', { value: '', text: 'no condition: state the value' }),
    ...[['Eq', '='], ['Ne', '≠'], ['Ge', '≥'], ['Gt', '>'], ['Le', '≤'], ['Lt', '<']].map(([v, t]) => h('option', { value: v, text: `value ${t}` })));
  const rhs = h('input', { id: 'prove-rhs', placeholder: 'condition value: 0x… or a number', autocomplete: 'off', 'data-testid': 'prove-rhs' });
  const check = h('button', { class: 'primary', 'data-testid': 'prove-check', text: 'Fetch the proof and check it' });
  const send = h('button', { 'data-testid': 'prove-send', text: 'Attest on LEZ' }); send.disabled = true;
  const out = h('div', { class: 'msg', role: 'status', 'data-testid': 'prove-out' });
  const facts = h('dl', { 'data-testid': 'prove-facts' });
  el.append(h('label', {}, 'Instance (the chain it follows)', sel), status,
    h('label', {}, 'Read Ethereum through', src),
    h('label', {}, 'What to prove', kind), h('label', {}, 'Address', addr), h('label', {}, 'Slot', slot), h('label', {}, 'Key', key),
    h('label', {}, 'Condition (optional: the program refuses unless it holds)', cmp), h('label', {}, 'Condition value', rhs),
    h('div', { class: 'row' }, check, send), out, facts,
    h('div', { class: 'privacy' }, h('b', { text: 'Public: ' }), 'the fact (address, slot, value or condition), the Sepolia block, the instance, your LEZ account as the fee payer, and when. ',
      h('b', { text: 'Private: ' }), 'nothing: an attestation is a public statement on LEZ. A private consumer can later use it without saying which (RFP-021 private consumption).'));
  const sync = () => { const k = kind.value; slot.parentElement.hidden = k === 'account'; key.parentElement.hidden = k !== 'mapping'; rhs.parentElement.hidden = !cmp.value; };
  kind.onchange = cmp.onchange = sync; sync();

  let ctx = null;   // the instance and record read from LEZ
  let ready = null; // the checked request
  const word = (s) => { const t = s.trim(); if (!t) throw new Error('enter a value'); return eth.word(t.startsWith('0x') ? BigInt(t) : BigInt(t)); };

  async function loadRecord() {
    ctx = null; ready = null; send.disabled = true;
    const ins = A.instances[Number(sel.value)];
    const id = c.lez.unhex(ins.id);
    try {
      const ia = await c.seq.account(at.instanceAccount(program, id));
      const st = c.lez.actorStateOf(ia, program);
      if (!st) throw new Error('the instance is not on this network');
      const inst = at.decodeInstance(st);
      const ra = await c.seq.account(at.headerAccount(program, id, inst.finalizedSlot));
      const rs = c.lez.actorStateOf(ra, program);
      if (!rs) throw new Error(`no record at the instance's finalized slot ${inst.finalizedSlot}`);
      const rec = at.decodeRecord(rs);
      ctx = { ins, id, inst, rec };
      const ex = rec.execution;
      status.textContent = `Newest record: beacon slot ${rec.slot}, ${ex.kind === 'payload' ? `block ${ex.blockNumber}` : 'block'} ${ex.blockHash ? '0x' + c.lez.hex(ex.blockHash).slice(0, 16) + '…' : ''} (${inst.active ? 'active' : 'not active'}, ${inst.applied} updates applied).`;
    } catch (e) { status.textContent = `Could not read the instance: ${e.message}`; }
  }
  sel.onchange = loadRecord;

  check.onclick = async () => {
    out.textContent = ''; out.className = 'msg'; facts.textContent = ''; ready = null; send.disabled = true;
    try {
      if (!ctx) await loadRecord();
      if (!ctx) throw new Error('no record to prove against');
      const source = src.value === 'rpc' ? A.ethRpc : eth.injected();
      if (!source) throw new Error('No browser wallet found: install one (e.g. MetaMask) or use the network\'s endpoint.');
      if (typeof source !== 'string') await source.request({ method: 'eth_requestAccounts' }).catch(() => {});
      const E = new eth.Eth(source);
      const chain = await E.chainId();
      if (chain !== ctx.inst.chainId) throw new Error(`The Ethereum source is on chain ${chain}; this instance follows chain ${ctx.inst.chainId}. Switch your wallet's network.`);
      const ex = ctx.rec.execution;
      const block = ex.kind === 'payload' ? await E.blockByNumber(ex.blockNumber) : await E.blockByHash('0x' + c.lez.hex(ex.blockHash));
      if (!block || block.hash.toLowerCase() !== '0x' + c.lez.hex(ex.blockHash)) throw new Error('the endpoint does not serve the record\'s block');
      const header = ex.kind === 'blockHash' ? eth.headerRlp(block) : null; // a Gloas record binds the hash only
      const stateRoot = c.lez.unhex(block.stateRoot);
      const a = eth.address(addr.value);
      const k = kind.value;
      const s = k === 'slot' ? word(slot.value) : k === 'mapping' ? eth.mappingSlot(eth.address(key.value), BigInt(slot.value.trim() || '0')) : null;
      const p = await E.proof(addr.value.trim(), s ? ['0x' + c.lez.hex(s)] : [], BigInt(block.number));
      const acct = eth.provenAccount(stateRoot, a, p.accountProof);
      if (!acct) throw new Error('Ethereum has no such account at this block');
      const value = s ? eth.provenSlot(acct.storageRoot, s, p.storageProof[0].proof) : null;
      const conditions = cmp.value && s ? [{ field: 'Value', cmp: cmp.value, rhs: word(rhs.value) }] : [];
      if (conditions.length) {
        const [x, y] = [BigInt('0x' + c.lez.hex(value)), BigInt('0x' + c.lez.hex(conditions[0].rhs))];
        const ok = { Eq: x === y, Ne: x !== y, Lt: x < y, Le: x <= y, Gt: x > y, Ge: x >= y }[cmp.value];
        if (!ok) throw new Error(`The condition does not hold (the value is ${x}); the program would refuse it, so nothing is sent.`);
      }
      const req = { instanceId: ctx.id, chainId: ctx.inst.chainId, header,
        accounts: [{ address: a, proof: p.accountProof, conditions: [], slots: s ? [{ slot: s, proof: p.storageProof[0].proof, conditions }] : [] }] };
      const data = at.encodeAttest(req);
      const blockNumber = Number(BigInt(block.number));
      const statements = [{ kind: 'account', id: at.accountStatementId(ctx.inst.chainId, blockNumber, a, { nonce: acct.nonce, balance: eth.word(acct.balance), storageRoot: acct.storageRoot, codeHash: acct.codeHash }) }];
      if (s) statements.push({ kind: 'storage', id: at.storageStatementId(ctx.inst.chainId, blockNumber, a, s, value) });
      ready = { data, statements, blockNumber, value, slot: s, address: a };
      const add = (k2, v) => facts.append(h('dt', { text: k2 }), h('dd', { text: v }));
      add('Block', `${blockNumber} (0x${c.lez.hex(c.lez.unhex(block.hash)).slice(0, 16)}…)`);
      add('Account', `nonce ${acct.nonce}, balance ${acct.balance} wei`);
      if (s) add('Slot', '0x' + c.lez.hex(s));
      if (value) add('Value', '0x' + c.lez.hex(value) + ` (${BigInt('0x' + c.lez.hex(value))})`);
      if (conditions.length) add('Condition', `value ${cmp.selectedOptions[0].textContent.replace('value ', '')} ${BigInt('0x' + c.lez.hex(conditions[0].rhs))}: holds`);
      for (const st of statements) add(`Statement (${st.kind})`, '0x' + c.lez.hex(st.id));
      add('Request', `${data.length} bytes to the record header(${ctx.ins.id.slice(0, 10)}…, ${ctx.rec.slot})`);
      out.textContent = 'The proof holds against the record\'s block (checked in this page). Attesting costs a LEZ fee.'; out.className = 'msg ok';
      send.disabled = false;
    } catch (e) { out.textContent = e.message; out.className = 'msg bad'; }
  };

  send.onclick = async () => {
    if (!ready || !ctx) return;
    send.disabled = true;
    out.className = 'msg'; out.textContent = 'Signing and sending…';
    try {
      const a = c.acct();
      await c.refresh();
      const fs = c.feeState();
      const seed = at.headerSeed(ctx.id, ctx.rec.slot);
      const to = { account: at.forPublicPda(program, seed), program };
      const probe = c.lez.buildCall({ payer: a.node, to, data: ready.data, nonce: a.chain.nonce, gasLimit: at.ATTEST_GAS, maxFee: 0n, evidence: [{ pda: { program, seed } }] });
      const { maxFee } = c.lez.feeFor({ gasLimit: at.ATTEST_GAS, size: probe.size, feeState: fs });
      if (a.chain.balance < maxFee) throw new Error(`Attesting needs up to ${c.fmt(maxFee)} in fees; this account holds ${c.fmt(a.chain.balance)}. Use the faucet first.`);
      const tx = c.lez.buildCall({ payer: a.node, to, data: ready.data, nonce: a.chain.nonce, gasLimit: at.ATTEST_GAS, maxFee, evidence: [{ pda: { program, seed } }] });
      const hash = await c.seq.send(tx.wire);
      out.textContent = `Sent ${hash}. Waiting for a LEZ block…`;
      for (let i = 0; i < 90; i++) {
        await new Promise((r) => setTimeout(r, 2000));
        const t = await c.seq.transaction(hash).catch(() => null);
        if (!t) continue;
        const bid = Number(Array.isArray(t) ? t[1] : t.block_id);
        const blk = c.lez.decodeBlock(await c.seq.block(bid));
        const o = c.lez.outcomeIn(blk, hash);
        if (o === 'reverted') { out.textContent = `Refused by the attestation program in LEZ block ${bid} (the transaction reverted; its fee was charged). Transaction ${hash}.`; out.className = 'msg bad'; return; }
        if (o === 'missing') { out.textContent = `The RPC places transaction ${hash} in LEZ block ${bid}, but that block does not list it: outcome unknown.`; out.className = 'msg warn'; return; }
        out.textContent = `Attested: the attestation program accepted the proof in LEZ block ${bid}. Transaction ${hash}. The statements above are now LEZ's, under the record header(${ctx.ins.id.slice(0, 10)}…, ${ctx.rec.slot}) (account ${c.lez.idToString(to.account)}).`;
        out.className = 'msg ok';
        c.refresh();
        return;
      }
      out.textContent = `Not in a block after three minutes (transaction ${hash}).`; out.className = 'msg warn';
    } catch (e) { out.textContent = e.message; out.className = 'msg bad'; } finally { send.disabled = false; }
  };
  loadRecord();
}
