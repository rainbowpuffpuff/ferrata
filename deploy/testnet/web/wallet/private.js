// Private accounts and private transfers on LEZ v0.4, from this page: what runs here and what proving needs. Every
// number below is the repository's own measurement (repo lez/RESULTS.md §3, lez/results/v0.4.0-rc3/README.md).
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(typeof k === 'string' ? document.createTextNode(k) : k);
  return e;
};

export function mountPrivate(root, c) {
  root.textContent = '';
  const circuit = el('code', { 'data-testid': 'private-circuit', text: 'reading…' });
  root.append(
    el('p', { class: 'lead', text: 'A private LEZ account keeps its balance and its owner off the public state: the chain sees only a commitment, a nullifier when it is spent, and an encrypted note only your keys open.' }),
    el('h3', { text: 'What a private transfer is' }),
    el('p', {}, 'A privacy-preserving transaction: a RISC Zero proof that LEZ\'s privacy circuit (this network\'s image ', circuit,
      ') ran your transfer correctly, about 250 KB on the wire, costing a flat 409,764 execution gas plus 224,063 storage gas; at most four fit in a block.'),
    el('h3', { text: 'Why this page does not prove it (yet)' }),
    el('ul', {},
      el('li', { text: 'Proving takes a RISC Zero prover: measured 49.5 s on an L4 GPU, and 887 s for a private update on 32 CPU threads with LEZ\'s rc3 wallet (claim 264 s, burn 302 s). RISC Zero\'s prover has no browser (wasm32) build, and a phone or laptop tab would need hours and gigabytes of memory.' }),
      el('li', { text: 'The rest of a private transfer is light enough for a page: LEZ\'s private keys come from the same recovery phrase, and this page derives them (above: LEZ\'s private key tree and ML-KEM-768 viewing key, checked against LEZ\'s own vectors). Finding your notes is trial decryption of the notes in blocks (ML-KEM-768 + ChaCha20, both bundled here); that and building the circuit\'s input are not wired into this page yet.' })),
    el('h3', { text: 'What it takes, and the trade-off' }),
    el('ul', {},
      el('li', { text: 'On your own machine: the `ferrata` tool or LEZ\'s wallet, from this same recovery phrase, proves locally (GPU or CPU); keys and inputs never leave your machine. This is what the bridge\'s private claim and burn use today.' }),
      el('li', { text: 'A proving service the page calls: fast (a GPU proves in under a minute), but the service sees the transaction\'s private inputs (accounts, amounts, the notes): it learns what the chain does not. Only a prover you trust, or one in a confidential enclave, keeps the privacy.' }),
      el('li', { text: 'Delegating less: LEZ\'s design lets a proof be made by anyone holding the witness; splitting the witness so a service can prove without learning it is research, not something LEZ offers.' })),
    el('div', { class: 'privacy' }, el('b', { text: 'In this page today: ' }), 'public accounts only. Your public transfers, attestations and bridge deposits show your public account; for private value, use the `ferrata` tool with the same phrase.'));
  const keys = el('div', { 'data-testid': 'private-keys' });
  root.prepend(keys);
  const render = () => {
    keys.textContent = '';
    if (!c.S.seed) { if (c.S.key) keys.append(el('p', { class: 'fine', text: 'This wallet was imported from a key: it has no private accounts (they derive from a recovery phrase).' })); return; }
    const k = c.lez.derivePrivate(c.S.seed, [0]);
    const id = c.lez.idToString(k.accountId), npk = c.lez.hex(k.npk), vpk = c.lez.hex(k.vpk);
    keys.append(el('h3', { text: 'Your private account /0' }),
      el('p', { class: 'fine', text: 'Derived in this page from your phrase with LEZ\'s own private key tree (checked against LEZ\'s pinned vectors), the same account LEZ\'s wallet and the ferrata tool make first.' }),
      el('div', { class: 'kv' }, el('span', { text: 'Account' }), el('code', { class: 'copy wrap', 'data-testid': 'private-id', text: `Private/${id}` })),
      el('div', { class: 'kv' }, el('span', { text: 'Nullifier key' }), el('code', { class: 'copy wrap', 'data-testid': 'private-npk', text: npk })),
      el('div', { class: 'kv' }, el('span', { text: 'Viewing key' }), el('code', { class: 'copy wrap', 'data-testid': 'private-vpk', text: `${vpk.slice(0, 32)}… (ML-KEM-768, ${k.vpk.length} bytes; click to copy)` })),
      el('p', { class: 'fine', text: 'To receive privately, give a sender both keys; with LEZ\'s wallet they send with --to-npk and --to-vpk, and their wallet proves the transfer.' }));
    keys.querySelector('[data-testid=private-vpk]').onclick = () => navigator.clipboard?.writeText(vpk).catch(() => {});
  };
  window.addEventListener('hashchange', () => { if (location.hash === '#private') render(); });
  if (location.hash === '#private') render();
  c.seq.programIds().then((ids) => {
    const w = ids?.privacy_preserving_circuit;
    circuit.textContent = w ? c.lez.programIdHex(w).slice(0, 16) + '…' : 'unknown';
  }).catch(() => { circuit.textContent = 'unreadable'; });
}
