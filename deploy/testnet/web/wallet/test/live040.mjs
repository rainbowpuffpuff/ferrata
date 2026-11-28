// The web wallet's LEZ v0.4.0 codec (../lez040.js) against a running LEZ v0.4.0 sequencer: the version probe, two
// public native transfers built and signed here (the second at nonce 1: the account's key rotated by the first, LEZ
// #1135), each read back by its receipt (getTransactionReceipt, LEZ #1107) and in its block, decoded by this codec and
// by the explorer's. For a local sequencer only (cli/v040/live.sh starts LEZ's standalone one); it refuses any other.
//
//   node deploy/testnet/web/wallet/test/live040.mjs http://127.0.0.1:PORT OUT.json
//
// The payer is LEZ v0.4.0's published local test account (its Justfile's `account import public --secret`, funded by
// LEZ's local genesis); the recipient is account /0 of a fixed test seed (BIP-39's all-`abandon` phrase, a published
// test value). Exit 0 when every check holds.
import { writeFileSync } from 'node:fs';
import { webcrypto } from 'node:crypto';
// Node 18 has no global Web Crypto (signing draws fresh BIP-340 randomness from it, as a browser does).
if (!globalThis.crypto) globalThis.crypto = webcrypto;
import * as lez from '../lez040.js';
import { seedOf } from '../lez.js'; // BIP-39 seed: version-independent, kept in rc3's module
import * as explorer from '../../explorer/assets/lez040.js';
import { connect } from '../net.js'; // the page's network layer (app.js builds, signs and reads through it)
import { b64decode } from '../../explorer/assets/util.js';

const [url, out] = process.argv.slice(2);
if (!url || !out) { console.error('usage: live040.mjs http://127.0.0.1:PORT OUT.json'); process.exit(2); }
const u = new URL(url);
if (u.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(u.hostname)) {
  console.error(`live040: ${url} is not a local sequencer; this script sends only to loopback`); process.exit(2);
}
const PUBLISHED_SECRET = '10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d';
const TEST_PHRASE = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
const facts = { url, checks: [] };
const check = (ok, what) => { facts.checks.push({ ok: !!ok, what }); console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) throw new Error(what); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const seq = new lez.Sequencer(url);
async function receiptOf(hash) {
  for (let i = 0; i < 90; i++) { const r = await seq.receipt(hash); if (r) return r; await sleep(1000); }
  throw new Error(`no receipt for ${hash} in 90 s`);
}
async function blockOf(hash) {
  const t = await seq.transaction(hash);
  if (!t) throw new Error(`getTransaction ${hash}: not found`);
  const n = Number(t[1]);
  return { n, b64: await seq.block(n) };
}
const big = (v) => BigInt(v);

try {
  const version = await lez.detectLezVersion(seq);
  facts.version = version;
  check(version === 'v0.4.0', `getSequencerStatus: the network runs LEZ ${version}`);
  facts.status = await seq.status();

  const secret = lez.unhex(PUBLISHED_SECRET);
  const from = { secret, accountId: lez.publicAccountId(secret) };
  const recipient = lez.deriveNode(seedOf(TEST_PHRASE), [0]);
  facts.payer = lez.idToString(from.accountId);
  facts.recipient = lez.idToString(recipient.accountId);
  const fee = await seq.feeState();
  facts.fee_state = Object.fromEntries(Object.entries(fee).map(([k, v]) => [k, String(v)]));
  const balance0 = big(await seq.balance(recipient.accountId) ?? 0);

  const sends = [];
  for (const [i, amount] of [[0, 1000n], [1, 2345n]]) {
    const nonces = await seq.nonces([from.accountId]);
    const nonce = big(nonces[0]);
    // A native transfer costs no execution gas (LEZ's own wallet measures it so: gas limit 0); the storage it takes
    // is priced at the head with headroom (lez.js's feeFor), well under the payer's genesis balance.
    const { maxFee } = feeFor(fee, 900n);
    const tx = lez.buildTransfer({ from, to: recipient.accountId, toKey: i === 0 ? lez.initialKey(recipient.secret) : null,
      amount, nonce, gasLimit: 0n, tip: 0n, maxFee });
    const sent = await seq.send(tx.wire);
    check(String(sent).replace(/^0x/, '').toLowerCase() === tx.hash, `transfer ${i + 1}: sendTransaction answers its hash (${tx.hash.slice(0, 16)}…), nonce ${nonce}`);
    const r = await receiptOf(tx.hash);
    check(r.success === true, `transfer ${i + 1}: receipt success (fee base ${r.fee.fee_base}, storage gas ${r.fee.gas_stor})`);
    const { n, b64 } = await blockOf(tx.hash);
    const w = lez.decodeBlock(b64);
    const e = explorer.decodeBlock(b64decode(b64));
    check(w.hashOk && e.hash_ok, `transfer ${i + 1}: block ${n}'s hash recomputed by the wallet's and the explorer's decoders`);
    check(lez.outcomeIn(w, tx.hash) === 'applied', `transfer ${i + 1}: the block's receipt for it says applied`);
    const pos = w.transactions.findIndex((d) => d.hash === tx.hash);
    const entry = w.transactions[pos].tx.entries[0];
    check(entry.auth_data.length === (nonce === 0n ? 129 : 96), `transfer ${i + 1}: its witness at nonce ${nonce} is ${entry.auth_data.length} bytes (initial key only at nonce 0)`);
    check(lez.equalBytes(entry.next, lez.keyState(secret, nonce + 1n)), `transfer ${i + 1}: it rotates the payer to its nonce-${nonce + 1n} key`);
    sends.push({ hash: tx.hash, nonce: String(nonce), amount: String(amount), block: n, receipt: JSON.parse(JSON.stringify(r, (_, v) => typeof v === 'bigint' ? v.toString() : v)), block_b64: b64 });
  }
  // 3. The page's own path: net.js's connect(), the payer imported as the page imports a raw key (on v0.4.0, the
  // account secret), a fresh recipient named by its receive code's admission key (parseAdmissionKey), the transfer
  // built by net.buildTransfer and its outcome read by net.outcome (the receipt RPC).
  const net = await connect(url);
  check(net.version === 'v0.4.0' && !net.readOnly, `the page's connect(): ${net.version}, signing allowed`);
  const payer = net.importAccount(PUBLISHED_SECRET);
  check(lez.equalBytes(payer.accountId, from.accountId), 'the page imports the secret as the same account');
  const fresh = net.deriveAccount(seedOf(TEST_PHRASE), '/1');
  const code = net.parseAdmissionKey(lez.hex(net.admissionKeyOf(fresh)));
  check(lez.equalBytes(code.id, fresh.accountId), 'the receive code\'s admission key names the fresh account');
  const nonce3 = big((await net.seq.nonces([payer.accountId]))[0]);
  const tx3 = net.buildTransfer({ from: payer, to: code.id, toKey: code.key, amount: 777n, nonce: nonce3, gasLimit: 0n,
    tip: 0n, maxFee: feeFor(fee, 900n).maxFee });
  await net.seq.send(tx3.wire);
  let o3 = 'missing';
  for (let i = 0; i < 90 && o3 === 'missing'; i++) { o3 = await net.outcome(tx3.hash); if (o3 === 'missing') await sleep(1000); }
  check(o3 === 'applied', `transfer 3 (the page's path, nonce ${nonce3}): net.outcome reads its receipt: ${o3}`);
  const { n: n3, b64: b3 } = await blockOf(tx3.hash);
  sends.push({ hash: tx3.hash, nonce: String(nonce3), amount: '777', block: n3, path: 'net.js',
    receipt: JSON.parse(JSON.stringify(await seq.receipt(tx3.hash), (_, v) => typeof v === 'bigint' ? v.toString() : v)), block_b64: b3 });
  facts.sends = sends;
  const balance1 = big(await seq.balance(recipient.accountId));
  check(balance1 - balance0 === 1000n + 2345n, `the recipient's balance rose by 3345 (${balance0} -> ${balance1})`);
  check(big(await seq.balance(fresh.accountId)) === 777n, 'the fresh account holds 777');
  facts.ok = true;
} catch (e) {
  facts.ok = false; facts.error = String(e && e.message || e);
  console.error(`live040: ${facts.error}`);
}
writeFileSync(out, JSON.stringify(facts, null, 1));
process.exit(facts.ok ? 0 : 1);

/** max_fee for a transfer of at most `size` bytes and no execution gas: the storage price at the head x8 (at least 64),
 *  the rule of lez.js's feeFor. */
function feeFor(feeState, size) {
  const bs = big(feeState.base_fee_stor);
  return { maxFee: size * (bs * 8n > 64n ? bs * 8n : 64n) };
}
