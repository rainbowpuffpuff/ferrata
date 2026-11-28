// The bridge's claim on LEZ from the page (RFP-022): a deposit to this wallet's PUBLIC account is claimed with the
// bridge program's plaintext `Claim`, a public transaction that carries the deposit's opening (s, kind, destination)
// and its Merkle path under an imported root. No proof is made anywhere: the program checks the path, the commitment
// and the nullifier itself. What it makes public is what a public claim is: the deposit (leaf, depositor, amount), its
// destination account and s (which opens this deposit only, not the wallet's keys). A deposit to a PRIVATE account is
// claimed privately with the `ferrata` tool (it needs a proof of LEZ's privacy circuit with the account's keys).
// Layouts: bridge/crates/ferrata-bridge-core (encoding.rs, tree.rs, inbound.rs), lez/bridge-wire/src/lib.rs, the IDL.
import { sha256, hmac } from './vendor/noble.js';
import { concat, utf8, hex, unhex, Writer, Reader } from './lez.js';
import { forPublicPda } from './attest.js';

const u64be = (v) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), false); return b; };
const u64le = (v) => new Writer().u64(v).done();
const u256 = (v) => { const b = new Uint8Array(32); const h = BigInt(v).toString(16).padStart(64, '0'); b.set(unhex(h)); return b; };

// ------------------------------------------------------------------ the vault's deposit tree (SPEC-v2)
export const LEAF_TAG = unhex('17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b'); // sha256("ferrata/vault/leaf/v2")
export const TREE_DEPTH = 32;
/** deposit leaf = SHA-256(0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖ depositor:20 ‖ token:20 ‖ amount:u256 ‖ C). */
export const depositLeaf = (index, depositor, token, amount32, C) => sha256(concat(Uint8Array.of(0), LEAF_TAG, u64be(index), depositor, token, amount32, C));
export const node = (l, r) => sha256(concat(Uint8Array.of(1), l, r));
let ZEROS = null;
export function zeros() {
  if (ZEROS) return ZEROS;
  ZEROS = [new Uint8Array(32)];
  for (let h = 0; h < TREE_DEPTH; h++) ZEROS.push(node(ZEROS[h], ZEROS[h]));
  return ZEROS;
}
/** The root of the first `leaves.length` leaves of the depth-32 tree, and the 32 siblings of leaf `i` (if given). */
export function treeOf(leaves, i = null) {
  const z = zeros();
  let level = leaves.slice(), idx = i;
  const path = [];
  for (let h = 0; h < TREE_DEPTH; h++) {
    if (idx !== null) { const sib = idx ^ 1; path.push(sib < level.length ? level[sib] : z[h]); idx >>= 1; }
    const next = [];
    for (let k = 0; k < level.length; k += 2) next.push(node(level[k], k + 1 < level.length ? level[k + 1] : z[h]));
    level = next.length ? next : [];
  }
  return { root: level.length ? level[0] : z[TREE_DEPTH], path };
}
/** fid = SHA-256("ferrata/fact/acc-leaf/v1" ‖ evmChainId:u256 ‖ vault:20 ‖ slot:u256 (0) ‖ leafIndex:u64 ‖ leaf); N = SHA-256("ferrata/nf/v1" ‖ s ‖ fid). */
export const fidOf = (chainId, vault, leafIndex, leaf) => sha256(concat(utf8('ferrata/fact/acc-leaf/v1'), u256(chainId), vault, u256(0), u64be(leafIndex), leaf));
export const nullifierOf = (s, fid) => sha256(concat(utf8('ferrata/nf/v1'), s, fid));

// ------------------------------------------------------------------ the bridge program's actors (lez/bridge-wire)
const bridgeHash = (tag, ...parts) => sha256(concat(utf8('ferrata-lez/v1/bridge/'), utf8(tag), ...parts));
export const seeds = {
  config: (cid) => bridgeHash('config', cid),
  token: (cid, token) => bridgeHash('token', cid, token),
  root: (cid, n) => bridgeHash('root', cid, u64le(n)),
  nullifier: (cid, chainId, vault, N) => bridgeHash('nullifier', cid, u256(chainId), vault, N),
  holder: (cid, account) => bridgeHash('holder', cid, account),
  burn: (cid, r) => bridgeHash('burn', cid, r),
};
export const pdaActor = (program, seed) => ({ account: forPublicPda(program, seed), program, seed });

// ------------------------------------------------------------------ the program's states (borsh BridgeState)
const u128s = (r) => r.vec((x) => x.u128());
function readCap(r) { return { capacity: r.u128(), refill: r.u128() }; }
/** BridgeState: Config 0, Root 1, Nullifier 2, Holder 3, Burn 4, Token 5 (only what a claim reads is decoded). */
export function decodeBridgeState(bytes) {
  if (!bytes || !bytes.length) return null;
  const r = new Reader(bytes);
  const tag = r.u8();
  if (tag === 1) {
    return { kind: 'root', configId: r.take(32), evmChainId: Number(r.u64()), vault: r.take(20), leafCount: Number(r.u64()),
      root: r.take(32), recordSlot: Number(r.u64()), blockNumber: Number(r.u64()) };
  }
  if (tag === 2) return { kind: 'nullifier' };
  if (tag === 3) {
    const holdings = r.vec((x) => ({ configId: x.take(32), token: x.take(20), amount: x.u128() }));
    return { kind: 'holder', holdings };
  }
  if (tag === 5) {
    const configId = r.take(32), evmChainId = Number(r.u64()), vault = r.take(20), attestation = r.take(32), instanceId = r.take(32);
    const entry = { token: r.take(20), lezDefinition: r.take(32), decimals: r.u8(), lezUnit: r.u128(), unitWeight: r.u128(),
      denominations: u128s(r), relayFees: u128s(r) };
    const schedules = r.vec((x) => ({ index: x.u64(), fromLeaf: x.u64(), announcedAt: x.u64(), effectiveAt: x.u64(),
      mintFees: u128s(x), burnFees: u128s(x), checkpointFees: u128s(x) }));
    const knownUntil = r.u64();
    const controls = { admin: r.take(32), freezeAuthority: r.take(32), feeRecipient: r.take(32), floor: readCap(r), tokenCap: readCap(r), globalCap: readCap(r) };
    return { kind: 'token', configId, evmChainId, vault, attestation, instanceId, entry, schedules, knownUntil, controls };
  }
  return { kind: ['config', 'root', 'nullifier', 'holder', 'burn', 'token'][tag] ?? 'unknown' };
}
/** The schedule range a wallet names for a claim of `leaf` (cli/wallet/src/claim.rs range_for). */
export function rangeFor(token, leaf) {
  const L = BigInt(leaf), S = token.schedules;
  if (!S.length) return { index: 0n, fromLeaf: 0n, untilLeaf: (1n << 64n) - 1n };
  let j = 0;
  for (let k = 0; k < S.length; k++) if (S[k].fromLeaf <= L) j = k;
  const until = j + 1 < S.length ? S[j + 1].fromLeaf : token.knownUntil;
  return { index: BigInt(j), fromLeaf: S[j].fromLeaf, untilLeaf: until };
}

// ------------------------------------------------------------------ the message (BridgeMessage::Claim, variant 5)
/** w: { depositor: 20, token: 20, amount: 32, leafIndex, path: [32 x 32], s: 32, kind, dest: 32 }. */
export function encodeClaim({ configId, leafCount, witness: w, range, relayer = null }) {
  const x = new Writer().u8(5).fixed(configId, 32).u64(leafCount)
    .fixed(w.depositor, 20).fixed(w.token, 20).fixed(w.amount, 32).u64(w.leafIndex).u32(w.path.length);
  for (const p of w.path) x.fixed(p, 32);
  x.fixed(w.s, 32).u8(w.kind).fixed(w.dest, 32).u64(range.index).u64(range.fromLeaf).u64(range.untilLeaf);
  if (relayer) x.u8(1).fixed(relayer, 32); else x.u8(0);
  return x.done();
}
export const CLAIM_GAS = 1000000n; // measured ~240k for a plaintext claim and its mints (lez/BRIDGE-RESULTS.md)
export { hex };

// ------------------------------------------------------------------ the imports a claim may need first (permissionless)
// ImportRoot (variant 1) writes root(cid, n) from the vault's slots 0 (root) and 1 (leaf count) proven under an
// attestation record; ImportFees (variant 12) extends token(cid, token)'s fee knowledge (`known_until`) by attesting
// the vault's fee schedule slot `index` (here: its absence) and the leaf count. Both are sent at config(cid) and read
// header(instance, slot) of the attestation program (cli/wallet/src/import.rs; lez/crown/driver/src/crown.rs).
export const SLOT_ROOT = new Uint8Array(32);
export const SLOT_COUNT = (() => { const w = new Uint8Array(32); w[31] = 1; return w; })();
/** sha256("ferrata/vault/fees/v1" ‖ token ‖ index:u64 BE): the vault's slot of token's fee schedule `index`. */
export const feeSlot = (token, index) => sha256(concat(utf8('ferrata/vault/fees/v1'), token, u64be(index)));
const HEADER_MARKER = utf8('ferrata-lez/v1/bridge/rlp-header');
/** An account proof with the block's RLP header in front (a Gloas record binds the block hash only). */
export const withHeader = (header, nodes) => (header ? [HEADER_MARKER, header, ...nodes] : nodes);
const vecvec = (w, list) => { w.u32(list.length); for (const n of list) w.bytes(typeof n === 'string' ? unhex(n) : n); };
export function encodeImportRoot({ configId, recordSlot, root, leafCount, account, rootSlot, countSlot }) {
  const w = new Writer().u8(1).fixed(configId, 32).u64(recordSlot).fixed(root, 32).u64(leafCount);
  vecvec(w, account); vecvec(w, rootSlot); vecvec(w, countSlot);
  return w.done();
}
/** ImportFees with `schedule: None` (the vault has no schedule `index` at the attested leaf count). */
export function encodeImportFeesAbsent({ configId, token, index, recordSlot, account, slots }) {
  const w = new Writer().u8(12).fixed(configId, 32).fixed(token, 20).u64(index).u8(0).u64(recordSlot);
  vecvec(w, account);
  w.u32(slots.length); for (const s of slots) vecvec(w, s);
  return w.done();
}
export const IMPORT_GAS = 3000000n; // the live Init (one attested vault slot under a Gloas record) used 367,512 execution gas; ample margin

// ------------------------------------------------------------------ the driver: what the Bridge tab's "Claim" runs
import { keccak, headerRlp, word } from './eth.js';
import { headerSeed, decodeInstance, decodeRecord, instanceSeed } from './attest.js';
import { idFromString, idToString, actorStateOf, feeFor, equalBytes,
  buildCall as rc3BuildCall, decodeBlock as rc3DecodeBlock, outcomeIn as rc3OutcomeIn } from './lez.js';
const netOf = (c) => c.net ?? {
  buildCall: c.lez?.buildCall ?? rc3BuildCall,
  outcome: async (hash, raw) => {
    const decode = c.lez?.decodeBlock ?? rc3DecodeBlock, outcomeIn = c.lez?.outcomeIn ?? rc3OutcomeIn;
    return outcomeIn(decode(raw), hash);
  },
};

const DEPOSIT_TOPIC = '0x' + hex(keccak(utf8('Deposit(address,uint64,uint64,address,uint256,bytes32)')));
const bytesOf = (h) => unhex(String(h).replace(/^0x/, '').padStart(64, '0'));
/** BridgeState::Config(Ready { config: BridgeConfig { .., attestation, instance_id, evm_chain_id, vault, .. } }). */
export function decodeConfigState(bytes) {
  const r = new Reader(bytes);
  if (r.u8() !== 0) throw new Error('not a configuration state');
  const ready = r.u8() === 1;
  const c = { bedrockGenesisId: r.take(32), channelId: r.take(32), bridgeImageId: r.take(32), attestation: r.take(32),
    instanceId: r.take(32), evmChainId: Number(r.u64()), vault: r.take(20) };
  return { ready, ...c };
}
/** The vault's Deposit events, in leaf order: { leafIndex, depositor, token, amount32, C, index, block, tx }. */
export async function vaultDeposits(eth, vault, fromBlock) {
  // In windows of at most 5,000 blocks, halved when the endpoint refuses (some cap a query's block range at 2,000).
  const head = Number(BigInt(await eth.request({ method: 'eth_blockNumber', params: [] })));
  const q = (n) => '0x' + n.toString(16);
  const logs = [];
  for (let from = Number(fromBlock), step = 5000; from <= head;) {
    const to = Math.min(from + step - 1, head);
    let part;
    try { part = await eth.request({ method: 'eth_getLogs', params: [{ address: vault, fromBlock: q(from), toBlock: q(to), topics: [DEPOSIT_TOPIC] }] }); }
    catch (e) { if (step > 250) { step = Math.ceil(step / 2); continue; } throw e; }
    logs.push(...part);
    from = to + 1;
  }
  const out = logs.map((l) => {
    const d = unhex(l.data.slice(2));
    return { depositor: unhex(l.topics[1].slice(-40)), token: unhex(l.topics[2].slice(-40)), index: BigInt('0x' + hex(d.subarray(0, 32))),
      leafIndex: Number(BigInt('0x' + hex(d.subarray(32, 64)))), amount32: d.slice(64, 96), C: d.slice(96, 128),
      block: Number(BigInt(l.blockNumber)), tx: l.transactionHash };
  }).sort((a, b) => a.leafIndex - b.leafIndex);
  out.forEach((x, i) => { if (x.leafIndex !== i) throw new Error(`the vault's deposit list has a gap at leaf ${i}: the Ethereum endpoint returned partial logs`); });
  return out;
}

/**
 * Claim one public-kind deposit. c: the page's context; dep: { leafIndex, depositor (bytes 20), token (bytes 20), amount32,
 * C, block, j, s, dest (bytes 32) }; eth: an EIP-1193 provider; say(text): progress. Returns { block, hash, holding }.
 */
export async function claimPublic(c, dep, eth, say) {
  const prep = await prepareClaim(c, dep, eth, say);
  return claimPrepared(c, dep, prep, say);
}

/** Signs with the selected account, sends, and waits for the block's outcome (reverted: an error). */
function sender(c, say) {
  const a = c.acct(), net = netOf(c);
  return async (label, to, actors, evidence, data, gasLimit, reviewedMaxFee = null) => {
    const [info, fs, held] = await Promise.all([c.seq.nonces([a.node.accountId]), c.seq.feeState(), c.seq.balance(a.node.accountId)]);
    const nonce = BigInt(info[0]), bal = BigInt(held);
    // The network's version is read right before signing: nothing is awaited between that read and the signature.
    if (c.versionConfirmed && !await c.versionConfirmed()) throw new Error(net.error);
    const args = { payer: a.node, to, actors, data, nonce, gasLimit, evidence };
    const maxFee = reviewedMaxFee ?? feeFor({ gasLimit, size: net.buildCall({ ...args, maxFee: 0n }).size, feeState: fs }).maxFee;
    if (bal < maxFee) throw new Error(`${label} needs up to ${c.fmt(maxFee)} in LEZ fees; this account holds ${c.fmt(bal)}: use the faucet first.`);
    const tx = net.buildCall({ ...args, maxFee });
    say(`${label}: signing and sending…`);
    const hash = await c.seq.send(tx.wire);
    for (let i = 0; i < 90; i++) {
      await new Promise((r) => setTimeout(r, 2000));
      const t = await c.seq.transaction(hash).catch(() => null);
      if (!t) continue;
      const bid = Number(Array.isArray(t) ? t[1] : t.block_id);
      const o = await net.outcome(hash, net.version === 'v0.4.0' ? undefined : await c.seq.block(bid));
      if (o === 'reverted') throw new Error(`${label} was refused by the bridge program in LEZ block ${bid} (its fee was charged). Transaction ${hash}.`);
      if (o === 'missing') throw new Error(`${label}: the RPC places transaction ${hash} in LEZ block ${bid}, but that block does not list it: outcome unknown. Check before trying again.`);
      say(`${label}: done in LEZ block ${bid}.`);
      return { block: bid, hash };
    }
    throw new Error(`${label}: not in a LEZ block after three minutes (${hash}).`);
  };
}

/**
 * Steps 1 and 2 of a claim, which anyone may run for any deposit (they reveal nothing about it): the fee range import
 * and the root import when the live bridge lacks them. dep needs { leafIndex, token (bytes 20), block }.
 */
export async function prepareClaim(c, dep, eth, say) {
  const B = c.cfg.bridge, program = idFromString(B.program), cid = unhex(B.configId), vault = unhex(B.vault);
  const state = async (seed) => decodeBridgeState(actorStateOf(await c.seq.account(forPublicPda(program, seed)), program));
  const sendCall = sender(c, say);
  const actor = (seed) => ({ account: forPublicPda(program, seed), program });
  const pda = (seed) => ({ pda: { program, seed } });

  // The configuration and, only when an import is needed, its attestation instance's newest record (every import is
  // proven under it, with eth_getProof at its block: public endpoints keep proofs for recent blocks only).
  const cfgState = await c.seq.account(forPublicPda(program, seeds.config(cid)));
  const conf = decodeConfigState(actorStateOf(cfgState, program));
  if (!conf.ready) throw new Error('The bridge is still pairing with its vault: claims open once it is ready.');
  let rec = null;
  const record = async () => {
    if (rec) return rec;
    const attProgram = conf.attestation, instance = conf.instanceId;
    const inst = decodeInstance(actorStateOf(await c.seq.account(forPublicPda(attProgram, instanceSeed(instance))), attProgram));
    const recSlot = inst.finalizedSlot;
    const ex = decodeRecord(actorStateOf(await c.seq.account(forPublicPda(attProgram, headerSeed(instance, recSlot))), attProgram)).execution;
    const recBlock = ex.kind === 'payload' ? await eth.request({ method: 'eth_getBlockByNumber', params: ['0x' + ex.blockNumber.toString(16), false] })
      : await eth.request({ method: 'eth_getBlockByHash', params: ['0x' + hex(ex.blockHash), false] });
    if (!recBlock) throw new Error('The Ethereum endpoint does not have the attestation record\'s block.');
    const recNumber = Number(BigInt(recBlock.number));
    if (recNumber < dep.block) {
      throw new Error(`Not yet: LEZ's newest Ethereum record is block ${recNumber}, your deposit is in block ${dep.block}. It must be final on Ethereum and attested on LEZ first (about 15 to 25 minutes after the deposit); try again then.`);
    }
    rec = { recSlot, header: ex.kind === 'blockHash' ? headerRlp(recBlock) : null,
      headerActor: { account: forPublicPda(attProgram, headerSeed(instance, recSlot)), program: attProgram },
      proofAt: (slots) => eth.request({ method: 'eth_getProof', params: ['0x' + hex(vault), slots.map((x) => '0x' + hex(x)), '0x' + recNumber.toString(16)] }) };
    return rec;
  };

  // 1. The token's fee knowledge must cover the leaf (ImportFees: "no further schedule" at the record's leaf count).
  let tok = await state(seeds.token(cid, dep.token));
  if (!tok || tok.kind !== 'token') throw new Error('This token is not registered on the LEZ side of the bridge yet.');
  if (tok.knownUntil <= BigInt(dep.leafIndex)) {
    const { recSlot, header, headerActor, proofAt } = await record();
    const idx = BigInt(tok.schedules.length);
    const p = await proofAt([feeSlot(dep.token, idx), SLOT_COUNT]);
    if (BigInt(p.storageProof[0].value) !== 0n) throw new Error(`The vault has a new fee schedule (${idx}) for this token: the operator must import it before claims.`);
    if (BigInt(p.storageProof[1].value) <= BigInt(dep.leafIndex)) throw new Error('The record\'s vault state does not include the deposit yet.');
    const data = encodeImportFeesAbsent({ configId: cid, token: dep.token, index: idx, recordSlot: BigInt(recSlot),
      account: withHeader(header, p.accountProof.map(unhex)), slots: [p.storageProof[0].proof.map(unhex), p.storageProof[1].proof.map(unhex)] });
    await sendCall('Fee range import', actor(seeds.config(cid)), [actor(seeds.config(cid)), headerActor, actor(seeds.token(cid, dep.token))],
      [pda(seeds.config(cid)), pda(seeds.token(cid, dep.token))], data, IMPORT_GAS);
    tok = await state(seeds.token(cid, dep.token));
  }

  // 2. A root covering the leaf: the newest imported one among the last 64 counts, else ImportRoot of the vault's state
  // at the record.
  const deps = await vaultDeposits(eth, B.vault, B.fromBlock ?? 0);
  if (deps.length <= dep.leafIndex) throw new Error('The Ethereum endpoint does not show this deposit in the vault\'s events yet.');
  const leaves = deps.map((d) => depositLeaf(d.leafIndex, d.depositor, d.token, d.amount32, d.C));
  let n = 0, rootState = null;
  for (let k = deps.length; k > dep.leafIndex && k > deps.length - 64 && !rootState; k--) {
    const s = await state(seeds.root(cid, k));
    if (s && s.kind === 'root') { rootState = s; n = k; }
  }
  if (!rootState) {
    const { recSlot, header, headerActor, proofAt } = await record();
    const p = await proofAt([SLOT_ROOT, SLOT_COUNT]);
    n = Number(BigInt(p.storageProof[1].value));
    if (n <= dep.leafIndex) throw new Error('The record\'s vault state does not include the deposit yet.');
    const root = bytesOf(p.storageProof[0].value);
    if (!equalBytes(treeOf(leaves.slice(0, n)).root, root)) throw new Error('The vault\'s Deposit events do not give the vault\'s root: the Ethereum endpoint serves partial logs.');
    const data = encodeImportRoot({ configId: cid, recordSlot: BigInt(recSlot), root, leafCount: BigInt(n),
      account: withHeader(header, p.accountProof.map(unhex)), rootSlot: p.storageProof[0].proof.map(unhex), countSlot: p.storageProof[1].proof.map(unhex) });
    await sendCall(`Root import (${n} deposits)`, actor(seeds.config(cid)), [actor(seeds.config(cid)), headerActor, actor(seeds.root(cid, n))],
      [pda(seeds.config(cid)), pda(seeds.root(cid, n))], data, IMPORT_GAS);
    rootState = await state(seeds.root(cid, n));
  }
  if (!rootState || rootState.kind !== 'root') throw new Error('No imported root covers the deposit.');
  return { conf, tok, n, rootState, leaves, sendCall, state, actor, pda, program, cid, vault };
}

async function claimPrepared(c, dep, prep, say) {
  const { conf, tok, n, rootState, leaves, sendCall, state, actor, pda, cid, vault } = prep;
  // 3. The claim itself.
  const { root, path } = treeOf(leaves.slice(0, n), dep.leafIndex);
  if (!equalBytes(root, rootState.root)) throw new Error('The deposit list does not give the imported root.');
  const leaf = leaves[dep.leafIndex];
  if (!equalBytes(leaf, depositLeaf(dep.leafIndex, dep.depositor, dep.token, dep.amount32, dep.C))) throw new Error('This deposit is not the vault\'s leaf at that index.');
  const N = nullifierOf(dep.s, fidOf(BigInt(conf.evmChainId), vault, dep.leafIndex, leaf));
  const nSeed = seeds.nullifier(cid, conf.evmChainId, vault, N);
  if (await state(nSeed)) throw new Error('This deposit has been claimed already.');
  const range = rangeFor(tok, dep.leafIndex);
  const data = encodeClaim({ configId: cid, leafCount: BigInt(n), range, witness: { depositor: dep.depositor, token: dep.token,
    amount: dep.amount32, leafIndex: BigInt(dep.leafIndex), path, s: dep.s, kind: 2, dest: dep.dest } });
  const holderSeeds = [seeds.holder(cid, dep.dest)];
  if (!equalBytes(tok.controls.feeRecipient, dep.dest)) holderSeeds.push(seeds.holder(cid, tok.controls.feeRecipient));
  const pdaSeeds = [seeds.root(cid, n), seeds.token(cid, dep.token), seeds.config(cid), nSeed, ...holderSeeds];
  const out = await sendCall('Claim', actor(seeds.root(cid, n)), pdaSeeds.map(actor), pdaSeeds.map(pda), data, CLAIM_GAS);
  const holder = await state(seeds.holder(cid, dep.dest));
  const held = holder?.holdings?.find((h) => equalBytes(h.token, dep.token))?.amount ?? 0n;
  return { ...out, holding: held, nullifier: hex(N), leafCount: n };
}
/** The holding of `account` at holder(cid, account) for `token` (0 if none). */
export async function holdingOf(c, account, token) {
  const B = c.cfg.bridge, program = idFromString(B.program), cid = unhex(B.configId);
  const s = decodeBridgeState(actorStateOf(await c.seq.account(forPublicPda(program, seeds.holder(cid, account))), program));
  return s?.holdings?.find((h) => equalBytes(h.token, token))?.amount ?? 0n;
}
export { idToString, word };

// ------------------------------------------------------------------ outbound: a public burn (SPEC-v2 §5.1)
// The note is built exactly as the `ferrata` tool's BurnNote::create (bridge/crates/ferrata-bridge-core/src/outbound.rs),
// so the tool releases it from the same phrase. A burn from a public holding is a plain public call (no proof): it makes
// public the account, the token, the denomination, the time and the note (r, memo, mac, COut); the Ethereum recipient
// stays encrypted in the memo (only kView opens it). The release on Ethereum needs a proof whose private input is the
// burn's index, its path, sOut and r, i.e. exactly which burn is being released: whoever makes that proof learns the
// link between this burn and the Ethereum recipient, so the page leaves it to the `ferrata` tool on the user's machine.
const T_BURN_TAG = utf8('ferrata/burn-tag/v1'), T_S_OUT = utf8('ferrata/s-out/v1'), T_MEMO = utf8('ferrata/memo/v1');
const T_BURN_MAC = utf8('ferrata/burn-mac/v2'), T_C_OUT = utf8('ferrata/Cout/v1'), T_NF_OUT = utf8('ferrata/nf-out/v2');
const prf = (key, msg) => hmac(sha256, key, msg);
/** r_i = HMAC(kView, "ferrata/burn-tag/v1" ‖ configId ‖ i:u64 BE). */
export const burnTag = (kView, configId, i) => prf(kView, concat(T_BURN_TAG, configId, u64be(i)));
/** The burn note of counter i: { r, sOut, memo, mac, cOut, nOut } for (chainId, vault, recipient), token and amount (32 bytes). */
export function burnNote({ kAuth, kView, configId, i, chainId, vault, recipient, token, amount32 }) {
  const r = burnTag(kView, configId, i);
  const sOut = prf(kAuth, concat(T_S_OUT, configId, r));
  const pad = prf(kView, concat(T_MEMO, r)).slice(0, 20);
  const memo = recipient.map((b, k) => b ^ pad[k]);
  const cOut = sha256(concat(T_C_OUT, sOut, u256(chainId), vault, recipient));
  const mac = prf(kView, concat(T_BURN_MAC, r, memo, token, amount32, cOut)).slice(0, 16);
  return { r, sOut, memo, mac, cOut, nOut: sha256(concat(T_NF_OUT, sOut)) };
}
/** BridgeMessage::Burn (variant 0): configId ‖ r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut. */
export const encodeBurn = ({ configId, note, token, amount32 }) =>
  new Writer().u8(0).fixed(configId, 32).fixed(note.r, 32).fixed(note.memo, 20).fixed(note.mac, 16).fixed(token, 20).fixed(amount32, 32).fixed(note.cOut, 32).done();
export const BURN_GAS = 1000000n; // measured 158,905 execution gas for a public burn of ETH (lez/BRIDGE-RESULTS.md)
export const u256word = u256;

/**
 * Plans a burn of one denomination `amount` (bigint, the token's base units) of `token` (bytes 20) from the selected
 * public account's holding, to be released to `recipient` (bytes 20) on the configuration's vault; signs nothing.
 * kAuth, kView: this phrase's bridge keys. The counter is the first i whose burn(cid, r_i) does not exist (a refused
 * burn leaves none). Returns what the review shows and the exact call sendBurn signs.
 */
export async function planBurn(c, { kAuth, kView, token, amount, recipient }) {
  const B = c.cfg.bridge, program = idFromString(B.program), cid = unhex(B.configId), vault = unhex(B.vault);
  const a = c.acct(), net = netOf(c);
  const state = async (seed) => decodeBridgeState(actorStateOf(await c.seq.account(forPublicPda(program, seed)), program));
  const tok = await state(seeds.token(cid, token));
  if (!tok || tok.kind !== 'token') throw new Error('This token is not registered on the LEZ side of the bridge.');
  if (!tok.entry.denominations.includes(amount)) throw new Error('The amount is not one of the token\'s denominations.');
  const held = await holdingOf(c, a.node.accountId, token);
  if (held < amount) throw new Error(`This account holds ${held} base units of the token on the bridge, less than ${amount}.`);
  let i = 0;
  for (; i < 1000; i++) if (!(await state(seeds.burn(cid, burnTag(kView, cid, i))))) break;
  const note = burnNote({ kAuth, kView, configId: cid, i, chainId: BigInt(B.evmChainId), vault, recipient, token, amount32: u256(amount) });
  const own = { account: a.node.accountId, program };
  const pdaSeeds = [seeds.holder(cid, a.node.accountId), seeds.token(cid, token), seeds.config(cid), seeds.burn(cid, note.r)];
  const actors = [own, ...pdaSeeds.map((sd) => ({ account: forPublicPda(program, sd), program }))];
  const evidence = pdaSeeds.map((seed) => ({ pda: { program, seed } }));
  const data = encodeBurn({ configId: cid, note, token, amount32: u256(amount) });
  const [nonces, fs] = await Promise.all([c.seq.nonces([a.node.accountId]), c.seq.feeState()]);
  // The size comes from a signed (unsent) build: the network's version is read right before it, as before a send.
  if (c.versionConfirmed && !await c.versionConfirmed()) throw new Error(net.error);
  const size = net.buildCall({ payer: a.node, to: own, actors, data, nonce: BigInt(nonces[0]), gasLimit: BURN_GAS, evidence, maxFee: 0n }).size;
  const { maxFee } = feeFor({ gasLimit: BURN_GAS, size, feeState: fs });
  return { account: a, i, note, token, amount, recipient, held, to: own, actors, evidence, data, maxFee };
}

/** Signs and sends a planned burn, after checking that nothing it was reviewed against has changed. */
export async function sendBurn(c, plan, say) {
  const B = c.cfg.bridge, program = idFromString(B.program), cid = unhex(B.configId);
  if (c.acct() !== plan.account) throw new Error('The selected account changed since the review; nothing was sent. Review again.');
  const used = decodeBridgeState(actorStateOf(await c.seq.account(forPublicPda(program, seeds.burn(cid, plan.note.r))), program));
  if (used) throw new Error(`Burn counter ${plan.i} was used since the review (another tab?); nothing was sent. Review again.`);
  if (await holdingOf(c, plan.account.node.accountId, plan.token) < plan.amount) throw new Error('The holding changed since the review; nothing was sent. Review again.');
  const out = await sender(c, say)(`Burn (counter ${plan.i})`, plan.to, plan.actors, plan.evidence, plan.data, BURN_GAS, plan.maxFee);
  return { ...out, i: plan.i, r: hex(plan.note.r), nOut: hex(plan.note.nOut), held: await holdingOf(c, plan.account.node.accountId, plan.token) };
}

/** planBurn then sendBurn, without a review in between (scripts and tests). */
export async function burnPublic(c, args, say) { return sendBurn(c, await planBurn(c, args), say); }
/** The bridge's token(cid, token) state (null if the token is not registered). */
export async function tokenOf(c, token) {
  const B = c.cfg.bridge, program = idFromString(B.program), cid = unhex(B.configId);
  const s = decodeBridgeState(actorStateOf(await c.seq.account(forPublicPda(program, seeds.token(cid, token))), program));
  return s?.kind === 'token' ? s : null;
}
