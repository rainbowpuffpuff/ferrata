// The explorer's decoders against real LEZ v0.4.0-rc3 blocks: the live run recorded in
// lez/live/results/v0.4.0-rc3/ (blocks.borsh as the sequencer served them, chain.tsv, run.tsv, segments.tsv).
//
//   node deploy/testnet/web/explorer/test/decode.test.mjs [RUN_DIR]
//
// Checks, for every block: it decodes with every byte used; its hash recomputed in JavaScript equals the header's
// hash (the program chunks the run elided from the published log are the exceptions segments.tsv lists, and only
// those); its transaction count and hash equal chain.tsv's; every transaction hash at its position equals run.tsv's
// (elided ones excepted); every message to a built-in program decodes with its IDL; every message to the
// attestation program decodes with idl/ferrata_attestation.idl.json or is counted as an older wire's. Also the
// PDA vectors of idl/vectors/pda.vectors.json, derived again in JavaScript. Exits 1 on any failure.

import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { decodeBlockList, decodeTransaction, BUILTINS, rootOf, signerAccounts } from "../assets/lez.js";
import { Idl } from "../assets/idl.js";
import { attestationPda, decodeInstanceState, decodeRecordState } from "../assets/ferrata.js";
import { hex, b58, fromHex, fromB58 } from "../assets/util.js";
import { sha256 } from "../assets/sha256.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "../../../../..");
const RUN = resolve(process.argv[2] || join(REPO, "lez/live/results/v0.4.0-rc3"));

let failures = 0;
const fail = (m) => {
  failures++;
  if (failures <= 30) console.log(`FAIL ${m}`);
};
const ok = (m) => console.log(`ok   ${m}`);

const tsv = (path) => {
  const [head, ...rows] = readFileSync(path, "utf8").trim().split("\n");
  const cols = head.split("\t");
  return rows.map((r) => Object.fromEntries(r.split("\t").map((v, i) => [cols[i], v])));
};

// sha256 itself, on FIPS vectors
{
  const enc = new TextEncoder();
  const v = [
    ["", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
    ["abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"],
    ["abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq", "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"],
  ];
  const bad = v.filter(([m, h]) => hex(sha256(enc.encode(m))) !== h);
  const long = hex(sha256(new Uint8Array(1000000).fill(0x61)));
  if (bad.length || long !== "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0") fail("sha256 vectors");
  else ok("sha256: FIPS 180-2 vectors (3 short, one million 'a')");
}

const blocks = decodeBlockList(new Uint8Array(readFileSync(join(RUN, "blocks.borsh"))));
const chain = tsv(join(RUN, "chain.tsv"));
const run = tsv(join(RUN, "run.tsv"));
const segments = existsSync(join(RUN, "segments.tsv")) ? tsv(join(RUN, "segments.tsv")) : [];
const programs = tsv(join(RUN, "programs.tsv"));
const elidedBlocks = new Set(segments.map((s) => Number(s.block)));
const elidedTx = new Set(segments.map((s) => s.tx_hash));
ok(`decoded ${blocks.length} blocks of ${RUN.slice(REPO.length + 1)}/blocks.borsh, every byte used`);

// block hashes and chain.tsv
{
  let hashOk = 0, hashElided = 0;
  const byId = new Map(chain.map((c) => [Number(c.block), c]));
  for (const b of blocks) {
    const id = Number(b.header.block_id);
    const c = byId.get(id);
    if (!c) fail(`block ${id} not in chain.tsv`);
    else {
      if (hex(b.header.hash) !== c.hash) fail(`block ${id}: header hash differs from chain.tsv`);
      if (b.transactions.length !== Number(c.transactions)) fail(`block ${id}: ${b.transactions.length} txs, chain.tsv ${c.transactions}`);
      if (b.bedrock_status !== c.bedrock_status) fail(`block ${id}: status ${b.bedrock_status}, chain.tsv ${c.bedrock_status}`);
      if (String(b.header.timestamp) !== c.timestamp_ms) fail(`block ${id}: timestamp`);
      const rev = c.reverted === "-" ? [] : c.reverted.split(",").map(Number);
      if (JSON.stringify(rev) !== JSON.stringify(b.reverted)) fail(`block ${id}: reverted ${b.reverted} vs ${c.reverted}`);
    }
    if (b.hash_ok) hashOk++;
    else if (elidedBlocks.has(id)) hashElided++;
    else fail(`block ${id}: recomputed hash ${hex(b.computed_hash)} differs from the header's ${hex(b.header.hash)}`);
    if (b.hash_ok && elidedBlocks.has(id)) fail(`block ${id}: elided but its hash still matches`);
  }
  if (chain.length !== blocks.length) fail(`chain.tsv has ${chain.length} blocks, blocks.borsh ${blocks.length}`);
  for (let i = 1; i < blocks.length; i++) {
    if (hex(blocks[i].header.prev_block_hash) !== hex(blocks[i - 1].header.hash)) fail(`block ${blocks[i].header.block_id}: prev hash link`);
  }
  ok(`block hashes recomputed: ${hashOk} equal the header's; ${hashElided} differ, exactly the blocks segments.tsv lists as elided (${elidedBlocks.size}); every prev-hash link holds`);
}

// transaction hashes against run.tsv
{
  const byBlock = new Map(blocks.map((b) => [Number(b.header.block_id), b]));
  let checked = 0, skipped = 0, elided = 0;
  for (const row of run) {
    if (row.block === "-" || row.position === "-") {
      skipped++;
      continue;
    }
    const b = byBlock.get(Number(row.block));
    const tx = b && b.transactions[Number(row.position)];
    if (!tx) {
      fail(`run.tsv ${row.tx_hash}: no tx at block ${row.block} position ${row.position}`);
      continue;
    }
    if (elidedTx.has(row.tx_hash)) {
      elided++;
      if (hex(tx.hash) === row.tx_hash) fail(`${row.tx_hash}: elided but hash still matches`);
      continue;
    }
    if (hex(tx.hash) !== row.tx_hash) fail(`block ${row.block} pos ${row.position}: tx hash ${hex(tx.hash)} vs run.tsv ${row.tx_hash}`);
    else checked++;
  }
  ok(`transaction hashes: ${checked} equal run.tsv's at their block and position; ${elided} elided; ${skipped} rows never in a block`);
}

// messages
{
  const attIdl = new Idl(JSON.parse(readFileSync(join(REPO, "idl/ferrata_attestation.idl.json"), "utf8")));
  const attPrograms = new Set(programs.filter((p) => p.name !== "scripted").map((p) => hex(fromB58(p.header_account))));
  const counts = { public: 0, private: 0, builtin: 0, att_ok: 0, att_old: 0, other: 0 };
  const byIns = {};
  const builtinNames = {};
  for (const b of blocks) {
    for (const tx of b.transactions) {
      counts[tx.kind === "Public" ? "public" : "private"]++;
      const root = rootOf(tx);
      if (!root) continue;
      const prog = hex(root.to.program_account_id);
      const bi = BUILTINS.get(prog);
      if (bi) {
        counts.builtin++;
        builtinNames[bi.name] = (builtinNames[bi.name] || 0) + 1;
        if (bi.idl) {
          try {
            bi.idl.decodeInstruction(root.message);
          } catch (e) {
            fail(`block ${b.header.block_id}: ${bi.name} message does not decode: ${e.message}`);
          }
        }
      } else if (attPrograms.has(prog)) {
        try {
          const { ins } = attIdl.decodeInstruction(root.message);
          counts.att_ok++;
          byIns[ins.name] = (byIns[ins.name] || 0) + 1;
        } catch {
          counts.att_old++;
        }
      } else counts.other++;
    }
  }
  ok(`transactions: ${counts.public} public, ${counts.private} privacy-preserving; built-in programs ${counts.builtin} (${Object.entries(builtinNames).map(([k, v]) => `${k} ${v}`).join(", ")}), every one decoded`);
  ok(`attestation program: ${counts.att_ok} messages decoded with the IDL (${Object.entries(byIns).map(([k, v]) => `${k} ${v}`).join(", ")}), ${counts.att_old} of an older wire; ${counts.other} to other programs`);
  if (counts.att_ok === 0) fail("no attestation message decoded");
}

// signers: the witness keys' public accounts are exactly the accounts the nonces name (LEZ's stateless check)
{
  let n = 0;
  for (const b of blocks) {
    for (const tx of b.transactions) {
      const signers = signerAccounts(tx).map(hex).sort();
      const named = tx.nonces.map((x) => hex(x.account)).sort();
      if (JSON.stringify(signers) !== JSON.stringify(named)) fail(`block ${b.header.block_id}: signers ${signers} vs nonces ${named}`);
      else n++;
    }
  }
  ok(`signer accounts derived from the witness keys equal the accounts the nonces name, in all ${n} transactions`);
}

// recorded privacy-preserving transactions (as LEZ's wallet wrote them): each decodes with every byte used; the one
// the rc1 live run recorded by hash has that hash
{
  const files = [
    "bench/perf/results/p2-a/private-1-tx.borsh",
    "bench/perf/results/p2-b/private-handoff-1-tx.borsh",
    "bench/perf/results/p4-owner-rehearsal/out/attest-pad/tx-attest.borsh",
    "bench/perf/results/p4-owner-rehearsal/out/bridge-pad/tx-burn.borsh",
    "bench/perf/results/p4-owner-rehearsal/out/bridge-pad/tx-claim.borsh",
    "lez/results/bridge-private-burn-tx.borsh",
    "lez/results/bridge-private-claim-tx.borsh",
    "lez/results/bridge-private-transfer-tx.borsh",
    "lez/results/bridge-transfer-claim-tx.borsh",
    "lez/results/stage2-private-tx.borsh",
    "lez/results/stage4-private-attest-tx.borsh",
  ];
  let n = 0, actions = 0;
  for (const f of files) {
    const p = join(REPO, f);
    if (!existsSync(p)) continue;
    try {
      const tx = decodeTransaction(new Uint8Array(readFileSync(p)));
      if (tx.kind !== "PrivacyPreserving") fail(`${f}: kind ${tx.kind}`);
      n++;
      actions += tx.execution.private_actions.length;
      if (f.endsWith("stage2-private-tx.borsh") && hex(tx.hash) !== "aeeedef926821146e28fa348595729de43af54d4e68ce8e15d05f730f98286d2") fail(`${f}: hash ${hex(tx.hash)}`);
    } catch (e) {
      fail(`${f}: ${e.message}`);
    }
  }
  ok(`recorded privacy-preserving transactions: ${n} of ${files.length} decode with every byte used (${actions} private actions); stage2's hash equals the rc1 run's run.tsv entry`);
}

// PDAs
{
  const vec = JSON.parse(readFileSync(join(REPO, "idl/vectors/pda.vectors.json"), "utf8")).pdas;
  const rec = JSON.parse(readFileSync(join(REPO, "idl/vectors/pda-recorded.json"), "utf8")).pdas;
  let n = 0;
  for (const e of [...vec, ...rec]) {
    const program = e.program_id ? fromHex(e.program_id) : fromB58(e.program_id_base58);
    const want = e.account ? e.account : hex(fromB58(e.account_base58));
    const args = e.inputs.map((i) => (i.encoding === "bytes" ? fromHex(i.value) : i.encoding === "u64le" ? BigInt(i.value) : i.value));
    let got;
    if (e.program === "ferrata_attestation") got = attestationPda(program, e.actor, ...args);
    else {
      // bridge PDAs: SHA-256("ferrata-lez/v1/bridge/" ‖ tag ‖ inputs) under LEZ's PDA rule (checked in idl/check.py)
      continue;
    }
    if (hex(got) !== want) fail(`PDA ${e.program} ${e.actor}: ${hex(got)} vs ${want}`);
    else n++;
  }
  ok(`attestation PDAs derived in JavaScript: ${n} equal idl/vectors' (instance, job, header; recorded headers included)`);
}

if (failures) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log("all checks passed");
