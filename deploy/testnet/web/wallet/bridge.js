// The bridge (RFP-022) from the web wallet. The deposit is signed by the user's own browser wallet (EIP-1193, e.g.
// MetaMask) and sent to FerrataVault on Sepolia; its commitment C is derived here from this wallet's recovery phrase
// exactly as the `ferrata` tool derives it (cli/src/seed.rs: bridgeSeed = HMAC-SHA256(seed64, "ferrata/bridge-seed/v1");
// SPEC-v2 via bridge/crates/ferrata-bridge-core/src/encoding.rs: kAuth, s, C), so a deposit made here can be claimed
// with the `ferrata` tool from the same phrase, and the page never sends the phrase or any LEZ key anywhere.
import { sha256, hmac } from './vendor/noble.js';
import { keccak, word, address as addrBytes, toChecksumAddress, hasChecksum } from './eth.js';
import { concat, utf8, hex, unhex } from './lez.js';
import { claimPublic, vaultDeposits, holdingOf, planBurn, sendBurn, tokenOf } from './claim.js';

// ------------------------------------------------------------------ SPEC-v2 derivations
const hs = (key, msg) => hmac(sha256, key, msg);
const u64be = (v) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), false); return b; };
export const bridgeSeedOf = (seed64) => hs(seed64, utf8('ferrata/bridge-seed/v1'));
export const kAuthOf = (bridgeSeed) => hs(bridgeSeed, utf8('ferrata/auth/v1'));
export const kViewOf = (bridgeSeed) => hs(bridgeSeed, utf8('ferrata/view/v1'));
/** s = HMAC(kAuth, "ferrata/s/v1" ‖ evmChainId:u256 ‖ vault:20 ‖ depositor:20 ‖ j:u64). */
export const depositSecret = (kAuth, chainId, vault, depositor, j) =>
  hs(kAuth, concat(utf8('ferrata/s/v1'), word(BigInt(chainId)), vault, depositor, u64be(j)));
export const KIND = { private: 1, public: 2 };
/** C = SHA-256("ferrata/C/v1" ‖ s ‖ kind ‖ dest). */
export const commitment = (s, kind, dest) => sha256(concat(utf8('ferrata/C/v1'), s, Uint8Array.of(kind), dest));

// ------------------------------------------------------------------ the vault's ABI (the calls this page makes)
const sel = (sig) => keccak(utf8(sig)).subarray(0, 4);
const callData = (sig, words) => '0x' + hex(concat(sel(sig), ...words));
const addrWord = (a) => word(addrBytes(a));
const words = (ret) => { const b = unhex(ret); const out = []; for (let i = 0; i + 32 <= b.length; i += 32) out.push(BigInt('0x' + hex(b.subarray(i, i + 32)))); return out; };
export const ETH = '0x0000000000000000000000000000000000000000'; // FerrataVault.ETH
const DEPOSIT_TOPIC = '0x' + hex(keccak(utf8('Deposit(address,uint64,uint64,address,uint256,bytes32)')));
export const depositEthData = (denomIndex, C, j) => callData('depositETH(uint8,bytes32,uint64)', [word(BigInt(denomIndex)), C, word(BigInt(j))]);
/** FerrataVault.deposit(token, denomIndex, C, j): an ERC-20 deposit; the vault pulls denomination + fees (approve first). */
export const depositTokenData = (token, denomIndex, C, j) =>
  callData('deposit(address,uint8,bytes32,uint64)', [addrWord(token), word(BigInt(denomIndex)), C, word(BigInt(j))]);
export const approveData = (spender, amount) => callData('approve(address,uint256)', [addrWord(spender), word(BigInt(amount))]);
/** The test token's open mint (vault/script/TestToken.sol); offered only when config.json says bridge.token.testMint. */
export const mintData = (to, amount) => callData('mint(address,uint256)', [addrWord(to), word(BigInt(amount))]);

async function call(p, to, sig, args) { return p.request({ method: 'eth_call', params: [{ to, data: callData(sig, args) }, 'latest'] }); }
export async function readVault(p, vault, depositor, token = ETH) {
  const cfgW = words(await call(p, vault, 'tokenConfig(address)', [addrWord(token)]));
  // (bool registered, bool enabled, uint256, uint256, uint256[] denominations): word 4 is the array's offset.
  const off = Number(cfgW[4]) / 32, n = Number(cfgW[off] ?? 0n);
  const denominations = cfgW.slice(off + 1, off + 1 + n);
  const [current, pendingAt] = words(await call(p, vault, 'feeState(address)', [addrWord(token)]));
  const blk = await p.request({ method: 'eth_getBlockByNumber', params: ['latest', false] });
  const now = BigInt(blk.timestamp);
  const schedule = pendingAt !== 0n && now >= pendingAt ? current + 1n : current; // cli/src/eth.rs deposit_price
  const fees = [];
  for (let i = 0; i < denominations.length; i++) {
    const f = words(await call(p, vault, 'fees(address,uint256,uint8)', [addrWord(token), word(schedule), word(BigInt(i))]));
    fees.push({ mint: f[0], relay: f[1] });
  }
  const j = depositor ? words(await call(p, vault, 'depositCount(address)', [addrWord(depositor)]))[0] : null;
  let balance = null, allowance = null;
  if (token.toLowerCase() !== ETH && depositor) {
    balance = words(await call(p, token, 'balanceOf(address)', [addrWord(depositor)]))[0];
    allowance = words(await call(p, token, 'allowance(address,address)', [addrWord(depositor), addrWord(vault)]))[0];
  }
  return { registered: cfgW[0] === 1n, enabled: cfgW[1] === 1n, denominations, fees, schedule, j, balance, allowance };
}
export function depositLogOf(receipt, vault) {
  for (const l of receipt.logs ?? []) {
    if (l.address.toLowerCase() !== vault.toLowerCase() || (l.topics?.[0] ?? '').toLowerCase() !== DEPOSIT_TOPIC) continue;
    const w = unhex(l.data);
    const at = (i) => w.subarray(32 * i, 32 * i + 32);
    return { depositor: '0x' + l.topics[1].slice(-40), token: '0x' + l.topics[2].slice(-40), index: BigInt('0x' + hex(at(0))),
      leafIndex: BigInt('0x' + hex(at(1))), amount: BigInt('0x' + hex(at(2))), commitment: '0x' + hex(at(3)) };
  }
  return null;
}

// ------------------------------------------------------------------ the tab
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(k);
  return e;
};
export const fmtUnits = (v, decimals = 18, symbol = 'ETH') => {
  const w = BigInt(v), u = 10n ** BigInt(decimals);
  const s = decimals ? (w % u).toString().padStart(decimals, '0').replace(/0+$/, '') : '';
  return `${w / u}${s ? '.' + s : ''} ${symbol}`;
};

// ------------------------------------------------------------------ refusals and costs in plain words
const selOf = (sig) => '0x' + hex(keccak(utf8(sig)).subarray(0, 4));
/** What each refusal a deposit can meet means to the depositor: the vault's own errors on the deposit path
 *  (vault/src/FerrataVault.sol, `_checkDeposit`, `deposit`, `depositETH` and the daily caps) and the two of
 *  OpenZeppelin's ERC-20 that a token deposit can raise. test/ux.test.mjs holds each to its declaration. */
export const DEPOSIT_REFUSALS = {
  'VaultRetired()': 'the vault is retired and takes no more deposits',
  'DepositsAreFrozen()': 'deposits are frozen for now',
  'TokenDepositsAreFrozen()': 'deposits of this asset are frozen for now',
  'TokenNotRegistered()': 'the vault does not know this asset',
  'DepositsDisabled()': 'deposits of this asset are switched off',
  'InvalidDenominationIndex()': 'that amount is not one of the vault\'s denominations',
  'ZeroCommitment()': 'the deposit\'s commitment C is zero',
  'WrongDepositIndex()': 'your deposit index j has moved (another deposit from this address?): review again',
  'WrongEthAmount()': 'the ETH sent is not the amount plus its fees exactly (the fees may have changed): review again',
  'TransferAmountMismatch()': 'the token moved a different amount than the vault asked for',
  'TreeFull()': 'the vault\'s deposit tree is full',
  'TokenDepositCapExceeded()': 'this asset\'s daily deposit cap is reached: try again later, or a smaller amount',
  'GlobalDepositCapExceeded()': 'the vault\'s daily deposit cap is reached: try again later',
  'ERC20InsufficientBalance(address,uint256,uint256)': 'your token balance is less than the amount plus its fees',
  'ERC20InsufficientAllowance(address,uint256,uint256)': 'the vault may not take that much of your token yet: it needs the approval first',
};
const REFUSAL_BY_SELECTOR = Object.fromEntries(Object.entries(DEPOSIT_REFUSALS)
  .map(([sig, why]) => [selOf(sig), { name: sig.slice(0, sig.indexOf('(')), why }]));

/** The revert data an EIP-1193 error carries wherever the wallet put it (MetaMask nests it under data.originalError),
 *  or the custom error's selector its message names ("execution reverted: custom error 0x…"); null if none. */
export function revertDataOf(e, depth = 0) {
  if (!e || typeof e !== 'object' || depth > 4) return null;
  if (typeof e.data === 'string' && /^0x[0-9a-fA-F]{8}/.test(e.data)) return e.data.toLowerCase();
  for (const k of ['data', 'error', 'originalError', 'cause']) {
    const r = revertDataOf(e[k], depth + 1);
    if (r) return r;
  }
  const m = /revert.*?\b(0x[0-9a-fA-F]{8})\b/i.exec(String(e.message ?? ''));
  return m ? m[1].toLowerCase() : null;
}

/** A browser wallet's (EIP-1193) error, or a contract's refusal, in plain words: what happened and what to do. The
 *  refusing contract is the vault unless the error names another (`e.subject`: the token for an approval, the test
 *  token for its mint). Anything it does not recognise keeps its own message. For the browser wallet's requests only:
 *  LEZ's errors are shown as they are. */
export function walletError(e) {
  const who = (e && typeof e === 'object' && e.subject) || 'The vault';
  const msg = String(e?.message ?? e ?? 'unknown error');
  const code = e?.code;
  if (code === 4001 || /user (rejected|denied)|rejected the request/i.test(msg)) return 'You declined it in your browser wallet, so it was not sent.';
  if (code === -32002) return 'Your browser wallet already has a request waiting: open the wallet, finish or reject that request, then try again.';
  if (code === 4100) return 'Your browser wallet has not allowed this page to use the account: connect again and allow it.';
  if (code === 4900 || code === 4901) return 'Your browser wallet is not connected to the network: check it, then try again.';
  if (/insufficient funds/i.test(msg)) {
    return 'Your browser wallet\'s Sepolia ETH does not cover the amount, its fees and the gas, so it was not sent: get some Sepolia ETH first (the testers\' page, Limits, names public faucets).';
  }
  const data = revertDataOf(e);
  const r = data && REFUSAL_BY_SELECTOR[data.slice(0, 10)];
  if (r) return `${who} refuses it (${r.name}): ${r.why}. It was not sent.`;
  if (/revert/i.test(msg)) return `${who} refuses it (${msg}). It was not sent.`;
  return msg;
}

/** The review's gas line, priced now from the wallet's endpoint (never a remembered price), rounded up to the gwei;
 *  and whether `balance` (wei) covers `value` plus the gas. */
export function costOf({ gas, gasPrice, balance, value = 0n }) {
  const g = BigInt(gas), p = BigInt(gasPrice), giga = 10n ** 9n;
  const fee = ((g * p + giga - 1n) / giga) * giga;
  const gwei = Number(p) / 1e9;
  const at = gwei >= 0.01 ? gwei.toFixed(2) : gwei.toPrecision(2);
  return { fee, need: BigInt(value) + fee, enough: BigInt(balance) >= BigInt(value) + fee,
    text: `about ${fmtUnits(fee)} (${Number(g).toLocaleString('en-US')} gas at ${at} gwei, read now; your wallet shows the final figure)` };
}

/** An error tagged with the contract that refused (`walletError` reads `subject`): the error itself when it can take
 *  the tag, otherwise a new Error carrying its message, code and data (a frozen or primitive rejection). */
export function refusedBy(err, subject) {
  if (err && typeof err === 'object' && Object.isExtensible(err)) return Object.assign(err, { subject });
  const e = new Error(String(err?.message ?? err ?? 'unknown error'));
  return Object.assign(e, { code: err?.code, data: err?.data, subject, cause: err });
}

/** One operation at a time over a group of controls (the browser wallet's requests and the deposit's plan are shared
 *  by connect, mint, review and the deposit): while one runs, every control of the group is disabled and a second
 *  start returns at once; after it, each control's own state is put back, whatever the operation did elsewhere. */
export function lockGroup(controls) {
  let held = false;
  return (fn) => async (...args) => {
    if (held) return undefined;
    held = true;
    const before = controls.map((x) => x.disabled);
    for (const x of controls) x.disabled = true;
    try { return await fn(...args); } finally {
      controls.forEach((x, i) => { x.disabled = before[i]; });
      held = false;
    }
  };
}

/**
 * The burn's Ethereum recipient and the words the review uses for where it came from: an address typed in the field
 * (mixed case must pass EIP-55; all one case has no checksum to check) or, when the field is empty, the browser
 * wallet's connected address. The zero address is refused: nobody could spend what is released to it. A name (ENS or
 * any other) is refused with the reason: the page resolves no names, and the recipient is final once burned.
 */
export function burnRecipient(typed, connected) {
  const t = String(typed ?? '').trim(), to = t || String(connected ?? '').trim();
  if (!to) throw new Error('Type the Ethereum recipient, or connect your browser wallet to use its address.');
  if (!/^0x/i.test(to) && /^[^\s/]+\.[a-z]{2,}$/i.test(to)) {
    throw new Error(`${to} looks like a name: this page resolves no names (ENS or other). Paste the recipient's 0x address; it is final once burned.`);
  }
  const bytes = addrBytes(to); // refuses a malformed address and a mixed-case one whose EIP-55 checksum fails
  if (bytes.every((b) => b === 0)) throw new Error(`${ETH} cannot be the recipient: nobody can spend what is released to it.`);
  const how = !t ? 'from your browser wallet: its connected address, the field was empty'
    : hasChecksum(t) ? 'typed; EIP-55 checksum checked' : 'typed without a checksum: compare every character';
  return { to, bytes, how };
}


// ------------------------------------------------------------------ the fee payer: the network's gas-drop relayer
/** Where the network publishes its gas-drop relayer: the origin of the page's RPC, `/relayer/gas-drop` (the `ferrata`
 *  tool reads the same URL, from its sequencer's origin, and uses that relayer by default: RFP-022 U.5, Priv.7). */
export const gasDropUrl = (rpc, base) => new URL('/relayer/gas-drop', new URL(rpc, base)).href;
/** The page's fee-payer line for an answer of that URL (`status`, its JSON `terms`, or `null` when unreachable), as the
 *  `ferrata` tool reads the same answer: only 404 and 410 mean "no relayer published"; anything else it cannot use
 *  (429, other errors, a malformed document, a paused or unreadable relayer) stops a private claim there. */
export function gasDropLine(status, terms, url) {
  if (status === 200 && terms && /^0x[0-9a-f]{64}$/.test(terms.relayer ?? '') && /^[0-9]+$/.test(String(terms.amount ?? ''))) {
    const covers = Array.isArray(terms.covers) && terms.covers.length ? terms.covers.join(' and ') : 'claim';
    return { state: 'on', text: `Fee payer (private claim${covers.includes('burn') ? ' and burn' : ''}, by default): this network's gas-drop relayer ${terms.relayer}. `
      + `With no setting, the ferrata tool asks it for one credit of ${terms.amount} native LEZ per deposit, sealed to a fresh one-time private account, `
      + `which pays the LEZ fee of your private ${covers}; the relayer learns the deposit's nullifier, that account, a time and your address, never which deposit. `
      + `Relay fee: the claim names this relayer, so the deposit's registered relay fee (the same amount whoever is named) is paid to it rather than to the configuration's fee recipient. `
      + `Published at ${url}. Opt out: --no-gas-drop (the fee then comes from your own private account, and where its native LEZ came from can link the claim to you).` };
  }
  if (status === 404 || status === 410) {
    return { state: 'none', text: 'Fee payer: this network publishes no gas-drop relayer, so a private claim\'s LEZ fee is paid from your own private account; where its native LEZ came from (a faucet, a purchase) can link the claim to you.' };
  }
  if (status === 503 && /paused/.test(terms?.error ?? '')) {
    return { state: 'paused', text: `Fee payer: this network's gas-drop relayer is paused (${terms.error}): the ferrata tool stops a private claim until it is back, or pays from your own private account with --no-gas-drop.` };
  }
  if (status === 429) {
    return { state: 'limited', text: `Fee payer: this network's gas-drop relayer answered "too many requests" (${url}): the ferrata tool stops a private claim until it can read it, or pays from your own private account with --no-gas-drop.` };
  }
  return { state: 'unreachable', text: `Fee payer: this network's gas-drop relayer could not be read (${url}${status ? `: ${status}` : ''}): the ferrata tool stops a private claim until it can, or pays from your own private account with --no-gas-drop.` };
}

/** Fills `node` with the fee-payer line, read from the network (its `data-state`: on, paused, unreachable, none). */
export async function showGasDrop(node, rpc, base, fetchFn = fetch) {
  const url = gasDropUrl(rpc, base);
  let status = null, terms = null;
  try {
    const r = await fetchFn(url, { cache: 'no-store' });
    status = r.status;
    try { terms = await r.json(); } catch { terms = null; }
  } catch { status = null; }
  const l = gasDropLine(status, terms, url);
  node.textContent = l.text;
  node.dataset.state = l.state;
  return l;
}

export function mountBridge(root, c) {
  const B = c.cfg.bridge ?? {};
  root.textContent = '';
  const steps = el('ol', { class: 'steps', 'data-testid': 'bridge-steps' },
    el('li', { text: 'Deposit ETH (or the network\'s test token) on Sepolia from your browser wallet. The vault records a commitment C to your LEZ destination; only you can open it.' }),
    el('li', { text: 'Claim on LEZ once the zone has imported the vault\'s deposits: privately (a privacy-preserving proof: no link from the deposit to your LEZ account) or to a public account.' }),
    el('li', { text: 'Burn on LEZ to an Ethereum address of your choice: from a public account\'s holding in this page, or privately from a private account with the ferrata tool.' }),
    el('li', { text: 'Release on Sepolia: a relayer (or you) presents the burn, proven, to the vault after its delay.' }));
  root.append(steps);
  if (!B.vault) {
    root.append(el('p', { class: 'msg warn', 'data-testid': 'bridge-unconfigured', text: 'No bridge vault is configured on this network yet (config.json: bridge.vault). When the operator sets it, deposits work from this tab.' }));
    return;
  }
  // The assets this page deposits: ETH, and the ERC-20 config.json names (bridge.token: address, symbol, decimals).
  const assets = [{ address: ETH, symbol: 'ETH', decimals: 18 }];
  if (B.token?.address) {
    assets.push({ address: String(B.token.address).toLowerCase(), symbol: String(B.token.symbol ?? 'token'),
      decimals: Number(B.token.decimals ?? 18), testMint: B.token.testMint === true });
  }
  const status = el('p', { class: 'fine', 'data-testid': 'bridge-status', text: `Vault ${B.vault} on chain ${B.evmChainId}.` });
  const connect = el('button', { 'data-testid': 'bridge-connect', text: 'Connect your browser wallet' });
  const assetSel = el('select', { 'data-testid': 'bridge-asset' });
  assets.forEach((a, i) => assetSel.append(el('option', { value: i, text: a.address === ETH ? 'ETH' : `${a.symbol} (ERC-20 ${a.address})` })));
  const denom = el('select', { 'data-testid': 'bridge-denom' });
  const kind = el('select', { 'data-testid': 'bridge-kind' },
    el('option', { value: 'public', text: 'this wallet\'s selected public account (claim is public)' }),
    el('option', { value: 'private', text: 'this wallet\'s private account /0 (claim is private, proven by the ferrata tool)' }));
  // A private destination is claimed from a terminal, not in this page: say what that needs before anything is signed.
  const kindNote = el('p', { class: 'fine', 'data-testid': 'bridge-kind-note', hidden: '' },
    'A deposit to your private account is claimed with the ferrata tool, from this phrase, on an x86-64 Linux computer '
    + '(16 threads and 16 GB for the proof); the claim waits a delay of 1 to 12 hours for privacy. ',
    el('a', { href: '/testers/#step-private' }, 'The steps'), '.');
  const mint = el('button', { 'data-testid': 'bridge-mint', text: 'Get test tokens (mint to my Ethereum address)' });
  const review = el('button', { class: 'primary', 'data-testid': 'bridge-review', text: 'Review the deposit' });
  const go = el('button', { 'data-testid': 'bridge-go', text: 'Deposit with my browser wallet' }); go.disabled = true;
  const out = el('div', { class: 'msg', role: 'status', 'data-testid': 'bridge-out' });
  const sum = el('dl', { 'data-testid': 'bridge-summary' });
  const list = el('ul', { class: 'list', 'data-testid': 'bridge-deposits' });
  const findBtn = el('button', { 'data-testid': 'bridge-find', text: 'Find my deposits on the vault' });
  const claimOut = el('div', { class: 'msg', role: 'status', 'data-testid': 'bridge-claim-out' });
  const holdBtn = el('button', { 'data-testid': 'bridge-holdings', text: 'Show this account\'s bridge holdings' });
  const holdOut = el('div', { class: 'msg', role: 'status', 'data-testid': 'bridge-holdings-out' });
  const burnAsset = el('select', { 'data-testid': 'bridge-burn-asset' });
  const burnDenom = el('select', { 'data-testid': 'bridge-burn-denom' });
  const burnTo = el('input', { 'data-testid': 'bridge-burn-to', placeholder: '0x… (default: your connected browser wallet\'s address)', autocomplete: 'off' });
  const burnBtn = el('button', { class: 'primary', 'data-testid': 'bridge-burn', text: 'Review the burn' });
  const burnGo = el('button', { 'data-testid': 'bridge-burn-go', text: 'Sign and burn' }); burnGo.disabled = true;
  const burnSum = el('dl', { 'data-testid': 'bridge-burn-summary' });
  const burnOut = el('div', { class: 'msg', role: 'status', 'data-testid': 'bridge-burn-out' });
  const gasDrop = el('p', { class: 'privacy', 'data-testid': 'bridge-gas-drop', text: 'Fee payer: reading this network\'s gas-drop relayer…' });
  showGasDrop(gasDrop, c.cfg.rpc, location.href);
  root.append(status, el('div', { class: 'row' }, connect), el('label', {}, 'Asset', assetSel), el('div', { class: 'row' }, mint),
    el('label', {}, 'Amount', denom), el('label', {}, 'Destination on LEZ', kind), kindNote,
    el('div', { class: 'row' }, review, go), out, sum,
    el('div', { class: 'privacy' }, el('b', { text: 'Public on Ethereum: ' }), 'your Ethereum address, the asset, the amount, the commitment C and the time. ',
      el('b', { text: 'Private: ' }), 'your LEZ destination (inside C, which only this wallet\'s phrase opens). A private claim on LEZ shows no link to this deposit; a public claim shows the destination account.'),
    el('h3', { text: 'Your deposits' }), el('div', { class: 'row' }, findBtn), claimOut, list,
    el('h3', { text: 'Burn to Ethereum' }), el('div', { class: 'row' }, holdBtn), holdOut,
    el('label', {}, 'Asset', burnAsset), el('label', {}, 'Amount (one denomination)', burnDenom), el('label', {}, 'Ethereum recipient', burnTo),
    el('div', { class: 'row' }, burnBtn, burnGo), burnSum, burnOut,
    el('div', { class: 'privacy' }, el('b', { text: 'Public on LEZ: ' }), 'the selected account, the asset, the amount, the time and the burn note (its tag r, the commitment COut). ',
      el('b', { text: 'Private: ' }), 'the Ethereum recipient, encrypted in the note\'s memo (only this phrase opens it). The release on Sepolia shows the recipient and the amount but, proven, not which burn it is.'),
    el('p', { class: 'fine', text: 'The public claim and the public burn run in this page (no proof). A private claim, a private burn and the release need a proof: the `ferrata` tool proves them on your machine from the same phrase (ferrata status, ferrata claim, ferrata burn, ferrata release); see the Private tab.' }),
    gasDrop);
  if (B.lezReady === false) {
    root.insertBefore(el('p', { class: 'msg warn', 'data-testid': 'bridge-not-ready', text: 'The LEZ side of this bridge is still being set up (its configuration and token registration). Deposits open here when it is done; until then nothing can be claimed on LEZ.' }), status);
  }
  const asset = () => assets[Number(assetSel.value || 0)];
  const isEth = () => asset().address === ETH;
  const fmt = (v) => fmtUnits(v, asset().decimals, asset().symbol);
  const syncMint = () => { mint.parentElement.hidden = !asset().testMint; };
  syncMint();

  let P = null, depositor = null, vault = null, plan = null;
  // Connect, mint, review, the deposit, "Find my deposits" and the burn's review share the browser wallet (and the
  // deposit its plan): one of them at a time, the asset, amount and destination fixed meanwhile (a review started
  // during a deposit would replace the plan it sends; two wallet requests at once are refused by the wallet).
  const exclusive = lockGroup([connect, mint, review, assetSel, denom, kind, findBtn, burnBtn, burnAsset, burnDenom, burnTo]);
  // The other buttons: one request at a time each, held from the click until the answer, released in finally.
  const busy = (btn, fn) => async (...args) => {
    if (btn.disabled) return;
    btn.disabled = true;
    try { return await fn(...args); } finally { btn.disabled = false; }
  };
  const provider = () => (typeof window !== 'undefined' ? window.ethereum : null);
  const totalOf = (i) => vault.denominations[i] + vault.fees[i].mint + vault.fees[i].relay;
  async function connectWallet() {
    P = provider();
    if (!P) throw new Error('No browser wallet (EIP-1193) in this page: install MetaMask or another, then reload.');
    const accts = await P.request({ method: 'eth_requestAccounts' });
    const before = depositor;
    depositor = String(accts[0]).toLowerCase();
    if (before && before !== depositor && plan) clearDeposit();   // a review made for another account is void
    let chain = Number(BigInt(await P.request({ method: 'eth_chainId' })));
    if (chain !== Number(B.evmChainId)) {
      // Ask the wallet to switch (EIP-3326), adding the network first if it does not know it (EIP-3085).
      const want = '0x' + Number(B.evmChainId).toString(16);
      try {
        await P.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: want }] });
      } catch (e) {
        if (e?.code === 4902 && B.evmRpc) {
          await P.request({ method: 'wallet_addEthereumChain', params: [{ chainId: want, chainName: B.evmChainName ?? `chain ${B.evmChainId}`,
            rpcUrls: [B.evmRpc], nativeCurrency: { name: 'Ether', symbol: 'ETH', decimals: 18 } }] }).catch(() => {});
          await P.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: want }] }).catch(() => {});
        }
      }
      chain = Number(BigInt(await P.request({ method: 'eth_chainId' })));
    }
    if (chain !== Number(B.evmChainId)) throw new Error(`Your wallet is on chain ${chain}; the vault is on chain ${B.evmChainId}. Switch networks in your wallet.`);
    const code = await P.request({ method: 'eth_getCode', params: [B.vault, 'latest'] });
    if (!code || code === '0x') throw new Error(`No contract at the vault address on chain ${chain}.`);
    if (B.configId) {
      // The bridge configuration the LEZ side was set up with (bridge.configId) must be the one this vault holds.
      const id = await P.request({ method: 'eth_call', params: [{ to: B.vault, data: callData('lezConfigId()', []) }, 'latest'] });
      if (String(id).toLowerCase() !== String(B.configId).toLowerCase()) throw new Error(`The vault's lezConfigId ${id} is not this network's bridge configuration ${B.configId}; nothing will be sent.`);
    }
    const keep = denom.value;
    vault = await readVault(P, B.vault, depositor, asset().address);
    if (!vault.registered || !vault.enabled) throw new Error(`The vault does not take ${asset().symbol} deposits now.`);
    denom.textContent = '';
    vault.denominations.forEach((d, i) => denom.append(el('option', { value: i, text: `${fmt(d)} (+ fees ${fmt(vault.fees[i].mint + vault.fees[i].relay)})` })));
    if (keep && Number(keep) < vault.denominations.length) denom.value = keep;
    status.textContent = `Connected ${toChecksumAddress(depositor)} on chain ${chain}; your next deposit index j = ${vault.j}.`
      + (isEth() ? '' : ` Your ${asset().symbol} balance: ${fmt(vault.balance)}.`);
  }
  const fail = (e) => { out.textContent = walletError(e); out.className = 'msg bad'; };
  const clearDeposit = () => { plan = null; go.disabled = true; sum.textContent = ''; };
  denom.onchange = clearDeposit;
  kind.onchange = () => { clearDeposit(); kindNote.hidden = kind.value !== 'private'; };
  connect.onclick = exclusive(async () => { out.textContent = ''; try { await connectWallet(); } catch (e) { fail(e); } });
  assetSel.onchange = exclusive(async () => {
    syncMint(); sum.textContent = ''; go.disabled = true; plan = null; out.textContent = ''; denom.textContent = '';
    if (P) { try { await connectWallet(); } catch (e) { fail(e); } }
  });

  async function sendAndWait(tx, what) {
    const hash = await P.request({ method: 'eth_sendTransaction', params: [tx] });
    out.textContent = `${what}: sent ${hash}; waiting for it to be mined…`; out.className = 'msg';
    let receipt = null;
    for (let i = 0; i < 120 && !receipt; i++) { await new Promise((r) => setTimeout(r, 1500)); receipt = await P.request({ method: 'eth_getTransactionReceipt', params: [hash] }); }
    return { hash, receipt };
  }

  mint.onclick = exclusive(async () => {
    out.textContent = ''; out.className = 'msg';
    try {
      await connectWallet();
      const amount = totalOf(Number(denom.value || 0));
      out.textContent = `Waiting for your wallet's signature: mint ${fmt(amount)} to ${depositor}…`;
      const { hash, receipt } = await sendAndWait({ from: depositor, to: asset().address, value: '0x0', data: mintData(depositor, amount),
        chainId: '0x' + Number(B.evmChainId).toString(16) }, 'Mint').catch((err) => { throw refusedBy(err, 'The test token'); });
      if (!receipt || receipt.status !== '0x1') throw new Error(`The mint ${hash} ${receipt ? 'failed' : 'is not mined yet'}.`);
      await connectWallet();
      out.textContent = `Minted ${fmt(amount)} (${hash}). Now review the deposit.`; out.className = 'msg ok';
    } catch (e) { fail(e); }
  });

  review.onclick = exclusive(async () => {
    out.textContent = ''; out.className = 'msg'; sum.textContent = ''; go.disabled = true; plan = null;
    try {
      if (B.lezReady === false) throw new Error('Deposits are not open yet: the LEZ side of this bridge is still being set up.');
      if (!c.S.seed) throw new Error('Bridging needs a wallet made from a recovery phrase (the bridge keys derive from it); this one holds only an imported key.');
      await connectWallet(); // the chain, the account and the vault read again at every review
      const i = Number(denom.value || 0);
      const A = asset();
      const a = c.acct();
      const k = KIND[kind.value];
      // Only this phrase's own accounts: a deposit to someone else's account could never be claimed (its owner lacks s).
      const d = kind.value === 'public' ? a.node.accountId : c.lez.derivePrivate(c.S.seed, [0]).accountId;
      const bs = bridgeSeedOf(c.S.seed);
      const s = depositSecret(kAuthOf(bs), B.evmChainId, addrBytes(B.vault), addrBytes(depositor), vault.j);
      const C = commitment(s, k, d);
      const total = totalOf(i);
      const chainId = '0x' + Number(B.evmChainId).toString(16);
      if (!isEth() && vault.balance < total) {
        throw new Error(`Your ${A.symbol} balance ${fmt(vault.balance)} is less than ${fmt(total)}.` + (A.testMint ? ' Get test tokens first.' : ''));
      }
      const data = isEth() ? depositEthData(i, C, vault.j) : depositTokenData(A.address, i, C, vault.j);
      plan = { tx: { from: depositor, to: B.vault, value: isEth() ? '0x' + total.toString(16) : '0x0', data, chainId },
        approve: !isEth() && vault.allowance < total ? { from: depositor, to: A.address, value: '0x0', data: approveData(B.vault, total), chainId } : null,
        record: { chainId: Number(B.evmChainId), vault: B.vault.toLowerCase(), token: A.address, symbol: A.symbol, decimals: A.decimals,
          depositor, j: vault.j.toString(), denomIndex: i, kind: kind.value,
          dest: c.lez.idToString(d), C: '0x' + hex(C), amount: vault.denominations[i].toString(), total: total.toString(), schedule: vault.schedule.toString() } };
      // The cost, priced now, before anything is signed: the first transaction's gas and whether this address's
      // Sepolia ETH covers it. For ETH, or a token already approved, that is the deposit itself, so an estimate that
      // fails is the vault's refusal, said here and not after a signature. A token that needs an approval first: the
      // approval is priced here (a refusal is the token's); the deposit, and the vault's checks, come after it.
      const first = plan.approve ?? plan.tx;
      const value = isEth() ? total : 0n;
      const short = (need, what) => new Error(`Your Sepolia ETH, ${fmtUnits(balance)}, does not cover ${fmtUnits(need)} (${what}): `
        + 'get some Sepolia ETH first (the testers\' page, Limits, names public faucets). Nothing was signed.');
      // The balance first, so that one too small for the deposit itself is said the same way whatever the estimate does.
      const balance = BigInt(await P.request({ method: 'eth_getBalance', params: [depositor, 'latest'] }));
      if (balance < value) throw short(value, 'the deposit and its fees, before the gas');
      const [gas, gasPrice] = await Promise.all([
        P.request({ method: 'eth_estimateGas', params: [{ from: first.from, to: first.to, value: first.value, data: first.data }] }),
        P.request({ method: 'eth_gasPrice' })]).catch((err) => { throw plan.approve ? refusedBy(err, 'The token') : err; });
      const cost = costOf({ gas, gasPrice, balance, value });
      if (!cost.enough) throw short(cost.need, isEth() ? 'the deposit, its fees and the gas' : 'the gas');
      const add = (x, y) => sum.append(el('dt', { text: x }), el('dd', { text: y }));
      add('From', toChecksumAddress(depositor)); add('Vault', toChecksumAddress(B.vault)); add('Asset', isEth() ? 'ETH' : `${A.symbol} ${toChecksumAddress(A.address)}`); add('Amount', fmt(vault.denominations[i]));
      add('Fees', `mint ${fmt(vault.fees[i].mint)}, relay ${fmt(vault.fees[i].relay)} (schedule ${vault.schedule})`); add('Total', fmt(total));
      if (!isEth()) {
        add('Approval', plan.approve ? `first a separate transaction approving the vault for exactly ${fmt(total)}, then the deposit`
          : `none needed (the vault may already take ${fmt(vault.allowance)})`);
      }
      add(plan.approve ? 'Gas, the approval' : 'Gas', cost.text);
      if (plan.approve) add('Gas, the deposit', 'priced by your wallet after the approval; the vault checks the deposit then');
      add('Your Sepolia ETH', fmtUnits(balance));
      add('Index j', vault.j.toString()); add('Commitment C', '0x' + hex(C));
      add('Destination', `${kind.value} LEZ account ${c.lez.idToString(d)} (inside C, not public)`);
      add('Calldata', data);
      out.textContent = 'Check C and the vault address again in your wallet\'s confirmation screen before you sign.'; out.className = 'msg';
      go.disabled = false;
    } catch (e) { plan = null; go.disabled = true; fail(e); }
  });

  go.onclick = exclusive(async () => {
    const p = plan;   // the reviewed plan, whatever happens to `plan` meanwhile
    if (!p) return;
    go.disabled = true;
    try {
      const chain = Number(BigInt(await P.request({ method: 'eth_chainId' })));
      const acct = String((await P.request({ method: 'eth_accounts' }))[0] ?? '').toLowerCase();
      if (chain !== p.record.chainId || acct !== p.record.depositor) throw new Error('Your wallet changed network or account since the review; nothing was sent. Review again.');
      if (p.record.kind === 'public' && p.record.dest !== c.acct().id) { clearDeposit(); throw new Error('The selected LEZ account changed since the review; nothing was sent. Review again.'); }
      const token = p.record.token ?? ETH;
      let fresh = await readVault(P, B.vault, p.record.depositor, token);
      if (fresh.j.toString() !== p.record.j) throw new Error('Your deposit index changed since the review (another deposit?); nothing was sent. Review again.');
      if (p.approve && fresh.allowance < BigInt(p.record.total)) {
        out.textContent = `Waiting for your wallet's signature: approve the vault for ${fmtUnits(p.record.total, p.record.decimals, p.record.symbol)} (1 of 2)…`; out.className = 'msg';
        const ap = await sendAndWait(p.approve, 'Approval (1 of 2)').catch((err) => { throw refusedBy(err, 'The token'); });
        if (!ap.receipt || ap.receipt.status !== '0x1') throw new Error(`The approval ${ap.hash} ${ap.receipt ? 'failed' : 'is not mined yet'}; nothing was deposited.`);
        fresh = await readVault(P, B.vault, p.record.depositor, token);
        if (fresh.allowance < BigInt(p.record.total)) throw new Error('The approval is mined but the vault\'s allowance is still short; nothing was deposited.');
        if (fresh.j.toString() !== p.record.j) throw new Error('Your deposit index changed during the approval; nothing was deposited. Review again.');
      }
      out.textContent = `Waiting for your wallet's signature${p.approve ? ' (2 of 2)' : ''}…`; out.className = 'msg';
      const hash = await P.request({ method: 'eth_sendTransaction', params: [p.tx] });
      const rec = { ...p.record, ethTx: hash, sent: Date.now() };
      (c.S.secret.deposits ??= []).push(rec); await c.persist();
      out.textContent = `Sent ${hash}; waiting for it to be mined…`;
      let receipt = null;
      for (let i = 0; i < 120 && !receipt; i++) { await new Promise((r) => setTimeout(r, 1500)); receipt = await P.request({ method: 'eth_getTransactionReceipt', params: [hash] }); }
      if (!receipt) { out.textContent = `Sent ${hash}; not mined yet. Your deposits list keeps it.`; out.className = 'msg warn'; return; }
      if (receipt.status !== '0x1') { rec.status = 'failed'; await c.persist(); throw new Error(`The deposit transaction ${hash} failed on Ethereum (status ${receipt.status}).`); }
      const log = depositLogOf(receipt, B.vault);
      if (!log || log.commitment !== rec.C || log.depositor !== p.record.depositor || log.index.toString() !== rec.j || log.token !== token) {
        throw new Error('Mined, but the vault\'s Deposit event does not carry this deposit\'s C, depositor, index and asset.');
      }
      rec.leafIndex = log.leafIndex.toString(); rec.block = Number(BigInt(receipt.blockNumber)); rec.status = 'deposited';
      await c.persist();
      out.textContent = `Deposited: leaf ${rec.leafIndex} in block ${rec.block} (${hash}). Next: the claim on LEZ, once the zone imports the vault's deposits.`; out.className = 'msg ok';
      plan = null; renderList();
    } catch (e) { fail(e); go.disabled = plan !== p; }
  });

  // The deposits this phrase opens, read from the vault's Deposit events: for each, s from (depositor, j) and C tried
  // against this wallet's public accounts (and its private account /0), so deposits made anywhere with this phrase show.
  async function findMine() {
    if (!c.S.seed) throw new Error('Finding deposits needs a wallet made from a recovery phrase.');
    if (!P) await connectWallet();
    const logs = await vaultDeposits(P, B.vault, B.fromBlock ?? 0);
    const kAuth = kAuthOf(bridgeSeedOf(c.S.seed));
    const dests = c.S.accounts.map((a) => ({ kind: 'public', id: a.node.accountId, b58: a.id }));
    const priv = c.lez.derivePrivate(c.S.seed, [0]);
    dests.push({ kind: 'private', id: priv.accountId, b58: c.lez.idToString(priv.accountId) });
    const kept = (c.S.secret.deposits ??= []);
    let found = 0;
    for (const l of logs) {
      const s = depositSecret(kAuth, B.evmChainId, addrBytes(B.vault), l.depositor, l.index);
      for (const d of dests) {
        if (hex(commitment(s, KIND[d.kind], d.id)) !== hex(l.C)) continue;
        found++;
        const C = '0x' + hex(l.C);
        let rec = kept.find((x) => x.C === C && x.vault === String(B.vault).toLowerCase());
        if (!rec) {
          const tokenHex = '0x' + hex(l.token);
          const A = assets.find((x) => x.address === tokenHex)
            ?? (tokenHex === ETH ? { symbol: 'ETH', decimals: 18 } : { symbol: `base units of token ${tokenHex}`, decimals: 0 });
          rec = { chainId: Number(B.evmChainId), vault: String(B.vault).toLowerCase(), token: tokenHex, symbol: A.symbol, decimals: A.decimals,
            depositor: '0x' + hex(l.depositor), j: l.index.toString(), kind: d.kind, dest: d.b58, C, amount: BigInt('0x' + hex(l.amount32)).toString(), ethTx: l.tx };
          kept.push(rec);
        }
        Object.assign(rec, { leafIndex: String(l.leafIndex), block: l.block, status: rec.status === 'claimed' ? 'claimed' : 'deposited' });
      }
    }
    await c.persist();
    return { found, total: logs.length };
  }
  findBtn.onclick = exclusive(async () => {
    claimOut.className = 'msg'; claimOut.textContent = 'Reading the vault\'s deposits…';
    try { const r = await findMine(); claimOut.textContent = `${r.found} of the vault's ${r.total} deposits are this wallet's.`; claimOut.className = 'msg ok'; renderList(); }
    catch (e) { claimOut.textContent = walletError(e); claimOut.className = 'msg bad'; }
  });
  async function claimOne(d, btn) {
    btn.disabled = true; claimOut.className = 'msg'; claimOut.textContent = 'Preparing the claim…';
    try {
      if (!P) await connectWallet();
      const logs = await vaultDeposits(P, B.vault, B.fromBlock ?? 0);
      const l = logs.find((x) => '0x' + hex(x.C) === d.C);
      if (!l) throw new Error('The vault has no deposit with this commitment (yet): is the deposit mined?');
      const s = depositSecret(kAuthOf(bridgeSeedOf(c.S.seed)), B.evmChainId, addrBytes(B.vault), l.depositor, l.index);
      const destId = c.lez.idFromString(d.dest);
      if (hex(commitment(s, KIND.public, destId)) !== hex(l.C)) throw new Error('This phrase does not open this deposit as a public one.');
      const a = c.S.accounts.find((x) => x.id === d.dest);
      if (!a) throw new Error(`The deposit pays account ${d.dest}, which is not in this wallet.`);
      if (c.acct() !== a) throw new Error(`Select the account the deposit pays (${a.label}) first: it signs and pays the claim's LEZ fees.`);
      const r = await claimPublic(c, { leafIndex: l.leafIndex, depositor: l.depositor, token: l.token, amount32: l.amount32, C: l.C, block: l.block, s, dest: destId }, P,
        (t) => { claimOut.textContent = t; });
      d.status = 'claimed'; d.claimTx = r.hash; await c.persist();
      claimOut.textContent = `Claimed in LEZ block ${r.block}: ${fmtUnits(r.holding, d.decimals ?? 18, d.symbol ?? 'ETH')} now held by your account on LEZ (the bridge's holder account for it). Transaction ${r.hash}.`;
      claimOut.className = 'msg ok';
      renderList();
    } catch (e) { claimOut.textContent = e?.message ?? String(e); claimOut.className = 'msg bad'; btn.disabled = false; }
  }

  // The public burn: one denomination from the selected public account's holding at holder(cid, account).
  assets.forEach((a, i) => burnAsset.append(el('option', { value: i, text: a.symbol })));
  const burnAssetOf = () => assets[Number(burnAsset.value || 0)];
  async function showHoldings() {
    if (!B.program || !B.configId) throw new Error('The LEZ side of the bridge is not configured on this network.');
    const a = c.acct(), A = burnAssetOf(), token = addrBytes(A.address);
    const [held, tok] = await Promise.all([holdingOf(c, a.node.accountId, token), tokenOf(c, token)]);
    if (burnAssetOf() !== A) return null;   // the asset changed meanwhile: a later read fills the list
    burnDenom.textContent = '';
    for (const d of tok?.entry?.denominations ?? []) {
      burnDenom.append(el('option', { value: d.toString(), text: `${fmtUnits(d, A.decimals, A.symbol)}${d > held ? ' (more than you hold)' : ''}` }));
    }
    const firstOk = (tok?.entry?.denominations ?? []).find((d) => d <= held);
    if (firstOk !== undefined) burnDenom.value = firstOk.toString();
    return { held, A, label: a.label };
  }
  const refreshHoldings = async () => {
    const asset = burnAsset.value;
    holdOut.className = 'msg'; holdOut.textContent = 'Reading the bridge program\'s holder account…';
    try {
      const r = await showHoldings();
      if (r) { holdOut.textContent = `${r.label} holds ${fmtUnits(r.held, r.A.decimals, r.A.symbol)} on the bridge.`; holdOut.className = 'msg ok'; }
    } catch (e) { if (burnAsset.value === asset) { holdOut.textContent = e?.message ?? String(e); holdOut.className = 'msg bad'; } }
  };
  holdBtn.onclick = busy(holdBtn, refreshHoldings);
  // The burn: "Review the burn" plans it (nothing signed) and shows everything it binds; "Sign and burn" sends exactly
  // that plan. The recipient is final once burned (it is inside the note), so a mixed-case address must pass EIP-55.
  let burnPlan = null;
  const clearBurn = () => { burnPlan = null; burnGo.disabled = true; burnSum.textContent = ''; };
  burnAsset.onchange = () => { clearBurn(); refreshHoldings(); };   // every change reads; a stale answer is dropped
  burnDenom.onchange = clearBurn;
  burnTo.addEventListener('input', clearBurn);
  burnBtn.onclick = exclusive(async () => {
    clearBurn(); burnBtn.disabled = true; burnOut.className = 'msg'; burnOut.textContent = 'Preparing the review…';
    try {
      if (!c.S.seed) throw new Error('Burning needs a wallet made from a recovery phrase (the note is derived from it).');
      if (!burnDenom.value && !(await showHoldings())) throw new Error('The asset changed while its holdings were read: review again.');
      if (!burnTo.value.trim()) await connectWallet();
      const { to, bytes: recipient, how } = burnRecipient(burnTo.value, depositor);
      const A = burnAssetOf(), amount = BigInt(burnDenom.value), bs = bridgeSeedOf(c.S.seed);
      const plan = await planBurn(c, { kAuth: kAuthOf(bs), kView: kViewOf(bs), token: addrBytes(A.address), amount, recipient });
      const add = (x, y) => burnSum.append(el('dt', { text: x }), el('dd', { text: y }));
      add('From', `${plan.account.label} (${plan.account.id}), its bridge holding ${fmtUnits(plan.held, A.decimals, A.symbol)}`);
      add('Burn', `${fmtUnits(amount, A.decimals, A.symbol)} (one denomination)`);
      add('Ethereum recipient', `${toChecksumAddress(to)} (${how})`);
      add('Release', `on chain ${B.evmChainId}, vault ${B.vault}, by the ferrata tool (\`ferrata release --burn ${plan.i}\`) once a checkpoint holds the burn`);
      add('Burn counter', String(plan.i));
      add('LEZ fee', `at most ${c.fmt(plan.maxFee)}`);
      add('Final', 'the recipient is bound into the note: a wrong address cannot be corrected after the burn');
      burnPlan = { plan, A, to, amount };
      burnOut.textContent = 'Check the recipient character by character, then sign.'; burnOut.className = 'msg';
      burnGo.disabled = false;
    } catch (e) { burnOut.textContent = e.message; burnOut.className = 'msg bad'; } finally { burnBtn.disabled = false; }
  });
  burnGo.onclick = async () => {
    if (!burnPlan) return;
    const { plan, A, to, amount } = burnPlan; clearBurn();
    burnOut.className = 'msg'; burnOut.textContent = 'Signing and sending…';
    try {
      const r = await sendBurn(c, plan, (t) => { burnOut.textContent = t; });
      (c.S.secret.burns ??= []).push({ configId: String(B.configId).toLowerCase(), vault: String(B.vault).toLowerCase(), i: r.i, r: '0x' + r.r,
        token: A.address, symbol: A.symbol, amount: amount.toString(), recipient: to.toLowerCase(), account: plan.account.id, block: r.block, hash: r.hash });
      await c.persist();
      burnOut.textContent = `Burned ${fmtUnits(amount, A.decimals, A.symbol)} in LEZ block ${r.block} (burn counter ${r.i}; transaction ${r.hash}); this account now holds ${fmtUnits(r.held, A.decimals, A.symbol)} on the bridge. `
        + `Release on Sepolia once a checkpoint holding this burn is accepted by the vault and its delay has passed: \`ferrata release --burn ${r.i}\` with this phrase proves on your machine that one of the checkpointed burns is yours without saying which (a prover service given that input would learn which burn it is, and so the link to ${toChecksumAddress(to)}).`;
      burnOut.className = 'msg ok';
    } catch (e) { burnOut.textContent = e.message; burnOut.className = 'msg bad'; }
  };

  function renderList() {
    list.textContent = '';
    const deps = (c.S.secret?.deposits ?? []).filter((d) => d.vault === String(B.vault).toLowerCase());
    if (!deps.length) { list.append(el('li', { text: 'None yet.' })); return; }
    for (const d of deps.slice().reverse()) {
      const li = el('li', { 'data-testid': 'bridge-deposit' });
      let btn = null;
      if (d.kind === 'public' && d.status !== 'claimed' && B.program && B.configId) {
        btn = el('button', { class: 'small primary', 'data-testid': 'bridge-claim', text: 'Claim on LEZ' });
        btn.onclick = () => claimOne(d, btn);
      }
      li.append(el('span', { text: `${fmtUnits(d.amount, d.decimals ?? 18, d.symbol ?? 'ETH')} → ${d.kind} LEZ account ${d.dest.slice(0, 8)}… · ${d.status ?? 'sent'}${d.leafIndex !== undefined ? ' · leaf ' + d.leafIndex : ''}` }),
        el('span', { class: 'amt', text: `j = ${d.j}` }),
        el('span', { class: 'sub', text: `C ${d.C} · ${d.ethTx ?? ''} · ${d.kind === 'private' ? 'claim privately with the ferrata tool (same phrase): a private claim needs a proof made with your keys' : d.status === 'claimed' ? 'claimed on LEZ' + (d.claimTx ? ' (' + d.claimTx + ')' : '') : 'claim here (public: shows the deposit and its LEZ account), or with the ferrata tool'}` }));
      if (d.kind === 'private' && d.status !== 'claimed') li.append(el('span', { class: 'sub' }, el('a', { href: '/testers/#step-private' }, 'The steps of a private claim')));
      if (btn) li.append(el('span', { class: 'sub' }, btn));
      list.append(li);
    }
  }
  root.addEventListener('focusin', () => { if (c.S.secret) renderList(); }, { once: true });
  if (c.S.secret) renderList();
  if (onHash) window.removeEventListener('hashchange', onHash);
  onHash = () => { if (location.hash === '#bridge' && c.S.secret) renderList(); };
  window.addEventListener('hashchange', onHash);
}
let onHash = null; // the current tab's listener (the tab is rebuilt on lock and when config.json changes)
