// LEZ v0.4.0 explorer decoding and RPC version selection, entirely against LEZ's own generated vectors and a stub fetch.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import * as lez from "../assets/lez040.js";
import * as rc3 from "../assets/lez.js";
import { Rpc, RpcError } from "../assets/rpc.js";
import { features } from "../assets/features.js";
import { b64decode, b64encode, concat, hex } from "../assets/util.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "../../../../..");
const fixture = JSON.parse(readFileSync(join(REPO, "deploy/testnet/web/wallet/test/fixtures/lez-v040-vectors.json"), "utf8"));
const H = (s) => Uint8Array.from(s.match(/../g).map((x) => parseInt(x, 16)));
const id = (a) => ({ account_id: H(a.account), program_account_id: H(a.program) });
const u32 = (n) => Uint8Array.of(n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255);

function expectedParts(v) {
  const p = v.parts;
  return {
    actors: p.actors.map((a) => ({ ...id(a.actor), authority: a.authority })),
    authorized: p.authorized_accounts.map(H),
    promotions: p.promotions.map(BigInt),
    privateSigners: p.private_signers.map(id),
    root: { to: id(p.to), message: H(p.data) },
    nonces: p.nonces.map(([account, nonce]) => ({ account: H(account), nonce: BigInt(nonce) })),
    fee: p.fee && { payer: H(p.fee.payer), gas_limit: BigInt(p.fee.gas_limit), tip: BigInt(p.fee.tip), max_fee: BigInt(p.fee.max_fee) },
    evidence: p.evidence.map((e) => e.key ? { kind: "key", public_key: H(e.key) } : {
      kind: "pda", program: H(e.pda.program), seed: H(e.pda.seed),
    }),
    witnesses: v.witness_entries.map((e) => ({ auth_data: H(e.auth_data), next: H(e.next) })),
  };
}

function checkPublic(v, tx) {
  assert.equal(tx.kind, "Public", v.name);
  assert.equal(hex(tx.hash), v.tx_hash, `${v.name} hash`);
  assert.equal(hex(tx.raw), v.lee_transaction, `${v.name} wrapper`);
  assert.equal(tx.size, H(v.tx).length, `${v.name} inner size`);
  assert.deepEqual(tx.raw, b64decode(v.wire), `${v.name} wire`);
  const p = expectedParts(v), c = tx.public_context;
  assert.deepEqual(c.actors, p.actors, `${v.name} actors and authority`);
  assert.deepEqual(c.authorized_accounts, p.authorized, `${v.name} authorized accounts`);
  assert.deepEqual(c.promotions, p.promotions, `${v.name} promotions`);
  assert.deepEqual(c.private_signers, p.privateSigners, `${v.name} private signers`);
  assert.deepEqual(tx.execution.root, p.root, `${v.name} root`);
  assert.deepEqual(tx.nonces, p.nonces, `${v.name} nonces`);
  assert.deepEqual(tx.execution.fee, p.fee, `${v.name} fee declaration`);
  assert.deepEqual(tx.admission_evidence, p.evidence, `${v.name} evidence`);
  assert.deepEqual(tx.witnesses, p.witnesses, `${v.name} witness entries`);
  assert.deepEqual(lez.rootOf(tx), p.root, `${v.name} rootOf`);
  assert.deepEqual(lez.signerAccounts(tx).map(hex), v.signer_ids, `${v.name} signers`);
}

function expectedReceipt(r) {
  return {
    success: r.success,
    fee: {
      gas_exec: BigInt(r.fee.gas_exec), gas_stor: BigInt(r.fee.gas_stor),
      fee_base: BigInt(r.fee.fee_base), tip: BigInt(r.fee.tip), refund: BigInt(r.fee.refund),
    },
  };
}

test("every public vector: complete public parts, key-state witnesses, hashes and sizes", () => {
  assert.equal(fixture.lez.tag, "v0.4.0");
  for (const v of fixture.public) checkPublic(v, lez.decodeTransaction(b64decode(v.wire)));
});

test("every private vector: filled execution, publications, fallback, fee tip, proof and full consumption", () => {
  for (const v of fixture.private) {
    const tx = lez.decodeTransaction(b64decode(v.wire));
    assert.equal(tx.kind, "PrivacyPreserving");
    assert.equal(hex(tx.hash), v.tx_hash);
    assert.equal(hex(tx.raw), v.lee_transaction);
    assert.equal(tx.size, H(v.tx).length);
    assert.equal(tx.proof.length, 40);
    assert.equal(tx.execution.private_actions.length, 1);
    assert.equal(tx.execution.async_messages.length, 2);
    assert.equal(tx.execution.async_messages[0].kind, "Clear");
    assert.equal(tx.execution.async_messages[1].kind, "Sealed");
    assert.equal(tx.execution.fee.tip, 11n);
    assert.equal(tx.execution.fee.fallback.ciphertext.length, 2);
    assert.equal(tx.execution.fee.fallback.boundary[0].kind, "EndPublicSubtree");
    assert.equal(tx.fee, null);
    assert.deepEqual(tx.witnesses.map((w) => hex(w.next)), v.next_keys);
    assert.deepEqual(lez.txSummaryCounts(tx), { nullifiers: 1, commitments: 3, boundary: 4, proof_bytes: 40 });
  }
});

test("every block vector: hash, transaction hashes, reverted indices and exact bigint receipts", () => {
  for (const v of fixture.blocks) {
    const block = lez.decodeBlock(b64decode(v.json));
    assert.equal(hex(block.raw || b64decode(v.json)), v.borsh);
    assert.equal(hex(block.header.hash), v.hash);
    assert.equal(hex(block.computed_hash), v.computed_hash ?? v.hash);
    assert.ok(block.hash_ok);
    assert.equal(block.bedrock_status, "Pending");
    assert.equal(block.size, b64decode(v.json).length);
    assert.deepEqual(block.transactions.map((tx) => hex(tx.hash)), v.tx_hashes ?? []);
    assert.deepEqual(block.reverted, v.reverted ?? []);
    assert.ok(block.reverted.every((n) => Number.isInteger(n)));
    block.receipts.forEach((r, i) => assert.deepEqual(r, expectedReceipt(JSON.parse(fixture.receipts[i].json))));
    for (const r of block.receipts) for (const n of Object.values(r.fee)) assert.equal(typeof n, "bigint");
    block.transactions.forEach((tx, i) => {
      if (tx.kind === "Public") checkPublic(fixture.public.find((v) => v.tx_hash === hex(tx.hash)), tx);
    });
  }
});

test("block lists consume every byte and retain each block's exact raw encoding", () => {
  const raw = fixture.blocks.map((b) => H(b.borsh));
  const decoded = lez.decodeBlockList(concat(u32(raw.length), ...raw));
  assert.equal(decoded.length, raw.length);
  decoded.forEach((b, i) => assert.deepEqual(b.raw, raw[i]));
  assert.throws(() => lez.decodeBlockList(concat(u32(raw.length), ...raw, Uint8Array.of(0))), /byte\(s\) left/);
});

test("transactions and blocks refuse trailing bytes", () => {
  for (const v of [...fixture.public, ...fixture.private]) {
    assert.throws(() => lez.decodeTransaction(concat(b64decode(v.wire), Uint8Array.of(0))), /byte\(s\) left after LeeTransaction/);
  }
  for (const v of fixture.blocks) {
    assert.throws(() => lez.decodeBlock(concat(b64decode(v.json), Uint8Array.of(0))), /byte\(s\) left after Block/);
  }
});

const response = (body) => ({ status: 200, headers: { get: () => null }, text: async () => typeof body === "string" ? body : JSON.stringify(body) });
const errorBody = (code, message) => ({ jsonrpc: "2.0", id: 1, error: { code, message } });
const resultBody = (result) => ({ jsonrpc: "2.0", id: 1, result });

function rc3Block(finalized = false) {
  const path = join(REPO, "lez/live/results/v0.4.0-rc3/blocks.borsh");
  const raw = rc3.decodeBlockList(new Uint8Array(readFileSync(path)))[0].raw.slice();
  if (finalized) raw[raw.length - 1] = 2; // Bedrock status is outside the block hash.
  return b64encode(raw);
}

function v040Block(finalized = false) {
  const raw = b64decode(fixture.blocks[0].json).slice();
  if (finalized) raw[raw.length - 1] = 2;
  return b64encode(raw);
}

test("RPC: status answer selects v0.4.0 decoding and the decision is cached", async (t) => {
  const methods = [];
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body); methods.push(q.method);
    return response(q.method === "getSequencerStatus" ? resultBody({ state: "online", height: 7, isOurTurn: true }) : resultBody(fixture.blocks[0].json));
  });
  const rpc = new Rpc("http://stub");
  const block = await rpc.block(7);
  assert.equal(await rpc.lezVersion(), "v0.4.0");
  assert.ok(Array.isArray(block.receipts));
  assert.deepEqual(block.reverted, [1]);
  assert.deepEqual(methods, ["getSequencerStatus", "getBlock"]);
});

test("RPC: method-not-found selects rc3 decoding and does not probe again", async (t) => {
  const methods = [];
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body); methods.push(q.method);
    return response(q.method === "getSequencerStatus" ? errorBody(-32601, "Method not found") : resultBody(rc3Block()));
  });
  const rpc = new Rpc("http://stub");
  const block = await rpc.block(1);
  assert.equal(await rpc.lezVersion(), "v0.4.0-rc3");
  assert.equal(block.receipts, undefined);
  assert.ok(Array.isArray(block.reverted));
  assert.deepEqual(methods, ["getSequencerStatus", "getBlock"]);
});

test("RPC: a probe network error uses rc3 for that call, retries, and never crosses the finalized cache", async (t) => {
  let probes = 0, blockCalls = 0;
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body);
    if (q.method === "getSequencerStatus") {
      probes++;
      if (probes === 1) throw new Error("network down");
      return response(resultBody({ state: "online", height: 7, isOurTurn: true }));
    }
    assert.equal(q.method, "getBlock");
    blockCalls++;
    return response(resultBody(probes === 1 ? rc3Block(true) : v040Block(true)));
  });
  const rpc = new Rpc("http://stub");
  const first = await rpc.block(7);
  assert.equal(first.receipts, undefined, "the failed-probe call falls back to rc3");
  const second = await rpc.block(7); // its probe is the retry and discovers v0.4.0
  assert.equal(await rpc.lezVersion(), "v0.4.0");
  assert.ok(Array.isArray(second.receipts));
  assert.equal(blockCalls, 2);
  const cached = await rpc.block(7);
  assert.strictEqual(cached, second);
  assert.equal(blockCalls, 2, "the v0.4.0 cache is hit without another getBlock");
  assert.equal(rpc.blocks.size, 2, "the finalized rc3 and v0.4.0 objects have separate entries");
  assert.equal(probes, 2);
});

test("RPC: receipt preserves u64/u128 JSON integers exactly and rc3 refuses the method locally", async (t) => {
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body);
    if (q.method === "getSequencerStatus") return response(resultBody({ state: "online", height: 7, isOurTurn: true }));
    assert.equal(q.method, "getTransactionReceipt");
    assert.equal(q.params[0], "abcdef");
    return response('{"jsonrpc":"2.0","id":1,"result":{"success":true,"fee":{"gas_exec":9007199254740993,'
      + '"gas_stor":5,"fee_base":340282366920938463463374607431768211455,"tip":18446744073709551616,"refund":0}}}');
  });
  const rpc = new Rpc("http://stub"), r = await rpc.receipt("0xABCDEF");
  assert.equal(r.fee.gas_exec, 9007199254740993n);
  assert.equal(r.fee.gas_stor, 5n);
  assert.equal(r.fee.fee_base, (1n << 128n) - 1n);
  assert.equal(r.fee.tip, 1n << 64n);
  assert.equal(r.fee.refund, 0n);
  const old = new Rpc("http://unused");
  old.versionKnown = true;
  await assert.rejects(() => old.receipt("abcdef"), (e) => e instanceof RpcError && e.code === -32601);
});

test("rc3 transactions keep rc3's signer rule; derived v0.4.0 programs are listed and no cross-zone PDA was added", () => {
  const path = join(REPO, "lez/live/results/v0.4.0-rc3/blocks.borsh");
  let checked = 0;
  for (const b of rc3.decodeBlockList(new Uint8Array(readFileSync(path)))) {
    for (const tx of b.transactions) {
      if (!tx.witnesses.length) continue;
      assert.deepEqual(lez.signerAccounts(tx).map(hex), rc3.signerAccounts(tx).map(hex));
      checked++;
    }
  }
  assert.ok(checked > 0, "an rc3 transaction with witnesses was checked");
  const names = new Set([...lez.BUILTINS.values()].filter((p) => hex(p.id) === hex(lez.systemProgramId(p.name.replaceAll(" ", "_")))).map((p) => p.name));
  for (const n of ["clock", "fee", "bridge", "sequencer stake", "signature"]) assert.ok(names.has(n), n);
  assert.ok(!names.has("cross zone inbox"));
});

test("blocks a real LEZ v0.4.0 sequencer served: hashes, the transfer and its receipt", () => {
  const real = JSON.parse(readFileSync(join(REPO, "deploy/testnet/web/wallet/test/fixtures/lez-v040-standalone-blocks.json"), "utf8"));
  const b2 = lez.decodeBlock(b64decode(real.blocks["2"]));
  assert.ok(b2.hash_ok);
  assert.deepEqual(b2.receipts, []);
  const b3 = lez.decodeBlock(b64decode(real.blocks["3"]));
  assert.ok(b3.hash_ok);
  assert.equal(hex(b3.transactions[0].hash), real.facts.transfer_tx);
  assert.deepEqual(b3.reverted, []);
  assert.equal(b3.receipts[0].success, true);
  assert.equal(b3.receipts[0].fee.fee_base, BigInt(real.facts.transfer_receipt.fee.fee_base));
  assert.equal(b3.receipts[0].fee.gas_stor, BigInt(real.facts.transfer_receipt.fee.gas_stor));
  const tx = b3.transactions[0];
  assert.deepEqual(tx.public_context.actors.map((a) => a.authority).sort(), ["none", "signer"]);
  assert.equal(lez.signerAccounts(tx).length, 1);
});

test("v0.4.0's system programs as the explorer derives them are the ones a real v0.4.0 block names", () => {
  const real = JSON.parse(readFileSync(join(REPO, "deploy/testnet/web/wallet/test/fixtures/lez-v040-standalone-blocks.json"), "utf8"));
  const b2 = lez.decodeBlock(b64decode(real.blocks["2"]));
  assert.deepEqual(b2.transactions.map((t) => hex(t.execution.root.to.program_account_id)),
    [hex(lez.systemProgramId("fee")), hex(lez.systemProgramId("clock"))]);
  assert.deepEqual(b2.transactions.map((t) => lez.BUILTINS.get(hex(t.execution.root.to.program_account_id)).name), ["fee", "clock"]);
});

test("RPC: with the version unknown (a probe that named none), v0.4.0 bytes are read as v0.4.0, rc3 bytes as rc3", async (t) => {
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body);
    if (q.method === "getSequencerStatus") return response(resultBody(null));
    return response(resultBody(q.params[0] === 7 ? v040Block() : rc3Block()));
  });
  const rpc = new Rpc("http://stub");
  const v = await rpc.block(7);
  assert.ok(Array.isArray(v.receipts), "v0.4.0 bytes read by the v0.4.0 decoder");
  const r = await rpc.block(8);
  assert.equal(r.receipts, undefined, "rc3 bytes read by rc3's decoder");
  assert.equal(await rpc.lezVersion(), "v0.4.0-rc3");
});

test("v0.4.0's system upgrader is listed at its own fixed 0xfd account", () => {
  const upgrader = lez.BUILTINS.get("fd".repeat(32));
  assert.ok(upgrader, "system_upgrader is a v0.4.0 built-in");
  assert.deepEqual(upgrader.id, new Uint8Array(32).fill(0xfd));
  assert.equal(upgrader.name, "system upgrader");
  assert.equal(upgrader.idl, null);
  assert.strictEqual(lez.systemProgramId("system_upgrader"), upgrader.id);
});

test("RPC: a probed rc3 version throws on v0.4.0-only bytes instead of falling back", async (t) => {
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body);
    return response(q.method === "getSequencerStatus" ? errorBody(-32601, "Method not found") : resultBody(v040Block()));
  });
  const rpc = new Rpc("http://stub");
  await assert.rejects(() => rpc.block(7));
  assert.equal(await rpc.lezVersion(), "v0.4.0-rc3");
  assert.equal(rpc.versionKnown, true);
});

test("RPC: v0.4.0's -31902 not-online answer selects v0.4.0 decoding", async (t) => {
  const methods = [];
  t.mock.method(globalThis, "fetch", async (_url, init) => {
    const q = JSON.parse(init.body); methods.push(q.method);
    return response(q.method === "getSequencerStatus" ? errorBody(-31902, "Sequencer is not online") : resultBody(v040Block()));
  });
  const rpc = new Rpc("http://stub");
  const block = await rpc.block(7);
  assert.equal(await rpc.lezVersion(), "v0.4.0");
  assert.equal(rpc.versionKnown, true);
  assert.ok(Array.isArray(block.receipts));
  assert.deepEqual(methods, ["getSequencerStatus", "getBlock"]);
});

test("features: Transaction outcome restores rc3's exact line-66 source link and leaves v0.4.0 unchanged", (t) => {
  const oldNode = globalThis.Node, oldDocument = globalThis.document;
  class FakeNode {
    constructor(tag, text) { this.tag = tag; this.text = text; this.attributes = {}; this.children = []; }
    setAttribute(name, value) { this.attributes[name] = value; }
    addEventListener() {}
    append(...children) { this.children.push(...children); }
  }
  globalThis.Node = FakeNode;
  globalThis.document = {
    createElement: (tag) => new FakeNode(tag),
    createTextNode: (text) => new FakeNode(null, text),
  };
  t.after(() => {
    if (oldNode === undefined) delete globalThis.Node; else globalThis.Node = oldNode;
    if (oldDocument === undefined) delete globalThis.document; else globalThis.document = oldDocument;
  });
  const sourceFor = (receipts) => {
    const block = { header: { block_id: 7n }, transactions: [], reverted: [], receipts, bedrock_status: "Pending" };
    const list = features({ view: [block], head: 7, fee: null, ids: null, config: { programs: [] } });
    return list.find((f) => f.name === "Transaction outcome").sources[0];
  };
  const old = sourceFor(undefined);
  assert.equal(old.attributes.href,
    "https://github.com/logos-blockchain/logos-execution-zone/blob/v0.4.0-rc3/lez/common/src/block.rs#L66");
  assert.equal(old.children[0].text, "BlockBody.reverted");
  const current = sourceFor([]);
  assert.equal(current.attributes.href,
    "https://github.com/logos-blockchain/logos-execution-zone/blob/v0.4.0/lez/common/src/block.rs");
  assert.equal(current.children[0].text, "BlockBody.receipts");
});
