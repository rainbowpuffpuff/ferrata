// Builds the offline test world the replay RPC serves (test/out/world.json and test/out/config.json):
//  * the 182 real blocks of the recorded LEZ v0.4.0-rc3 run (lez/live/results/v0.4.0-rc3/blocks.borsh), served as
//    getBlock / getBlockRange / getTransaction serve them (base64 of borsh);
//  * account states the run did not record, built here in the programs' own encodings from real Sepolia data
//    (crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10: the beacon header of slot 11,549,664, finalized,
//    and its execution block 12,105,936): the attestation program's loader header (the run's image id), a Sepolia
//    light-client instance whose finalized header is that slot's, and that header's record; a funded user account;
//  * a mock beacon API and execution JSON-RPC serving that fixture's real responses, so the status page's checks run
//    offline.
//
//   node deploy/testnet/web/explorer/test/fixtures.mjs
//
// Nothing here is a claim about a network: it is test input for the pages.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { decodeBlockList, decodeBlock, PROGRAM_LOADER_ID, NATIVE_TOKEN_PROGRAM_ID, signerAccounts, accountFromPublicKey } from "../assets/lez.js";
import { attestationPda, beaconHeaderRoot } from "../assets/ferrata.js";
import { hex, b58, fromHex, fromB58, b64encode, b64decode as b64decodeNode, u64le, concat } from "../assets/util.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "../../../../..");
const RUN = join(REPO, "lez/live/results/v0.4.0-rc3");
const FIX = join(REPO, "crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10");
const OUT = join(HERE, "out");
mkdirSync(OUT, { recursive: true });

class W {
  constructor() {
    this.parts = [];
  }
  u8(v) { this.parts.push(Uint8Array.of(v)); return this; }
  u32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); this.parts.push(b); return this; }
  u64(v) { this.parts.push(u64le(v)); return this; }
  u128(v) { const x = BigInt(v); this.parts.push(concat(u64le(x & ((1n << 64n) - 1n)), u64le(x >> 64n))); return this; }
  bytes(b) { this.parts.push(b); return this; }
  vec(b) { return this.u32(b.length).bytes(b); }
  bool(v) { return this.u8(v ? 1 : 0); }
  out() { return concat(...this.parts); }
}

const tsv = (path) => {
  const [head, ...rows] = readFileSync(path, "utf8").trim().split("\n");
  const cols = head.split("\t");
  return rows.map((r) => Object.fromEntries(r.split("\t").map((v, i) => [cols[i], v])));
};

// ---- blocks --------------------------------------------------------------------------------------------------
const blocks = decodeBlockList(new Uint8Array(readFileSync(join(RUN, "blocks.borsh"))));
const world = { head: 0, blocks: {}, txs: {}, accounts: {}, program_ids: {}, channel_id: "", fee_state: null, beacon: {}, eth: {} };
for (const b of blocks) {
  const id = Number(b.header.block_id);
  world.blocks[id] = b64encode(b.raw);
  world.head = Math.max(world.head, id);
  for (const tx of b.transactions) world.txs[hex(tx.hash)] = [b64encode(tx.raw), id];
}
const ref = JSON.parse(readFileSync(join(REPO, "lez/testnet04/reference-rc3.json"), "utf8"));
world.program_ids = ref.program_ids;
world.channel_id = ref.channel_id_local_json;
world.fee_state = { ...ref.fee_state_at_height_30, height: world.head };

// ---- the attestation program: the run's `main` deployment --------------------------------------------------
const main = tsv(join(RUN, "programs.tsv")).find((p) => p.name === "main");
const program = fromB58(main.header_account);
const firstSegment = fromB58(tsv(join(RUN, "segments.tsv"))[0].segment_account);
const setState = (account, programId, data) => {
  const k = b58(account);
  world.accounts[k] = world.accounts[k] || { nonce: 0, data: { actor_states: {} } };
  world.accounts[k].data.actor_states[b58(programId)] = Array.from(data);
};
setState(program, PROGRAM_LOADER_ID, new W().u8(0).bytes(fromHex(main.image_id)).bytes(firstSegment).bool(true).out());
// the run's twin deployment of the same image: not named in the test config, so the pages must recognise it by the
// image id in its loader header (config.json's known_images)
const twin = tsv(join(RUN, "programs.tsv")).find((p) => p.name === "twin");
setState(fromB58(twin.header_account), PROGRAM_LOADER_ID, new W().u8(0).bytes(fromHex(twin.image_id)).bytes(firstSegment).bool(true).out());

// ---- a Sepolia instance and its record, from the real 10 Nov 2026 fixture --------------------------------
const fixture = JSON.parse(readFileSync(join(FIX, "fixture.json"), "utf8"));
const bh = JSON.parse(readFileSync(join(FIX, `beacon_header_${fixture.finalized_slot}_second.json`), "utf8")).data;
const block = JSON.parse(readFileSync(join(FIX, "block.json"), "utf8"));
const eb = block.result || block;
const m = bh.header.message;
const beaconSsz = new W().u64(BigInt(m.slot)).u64(BigInt(m.proposer_index)).bytes(fromHex(m.parent_root)).bytes(fromHex(m.state_root)).bytes(fromHex(m.body_root)).out();
const root = beaconHeaderRoot({ slot: BigInt(m.slot), proposer_index: BigInt(m.proposer_index), parent_root: fromHex(m.parent_root), state_root: fromHex(m.state_root), body_root: fromHex(m.body_root) });
if (hex(root) !== bh.root.slice(2)) throw new Error(`beacon root ${hex(root)} != ${bh.root}`);

const FAR = (1n << 64n) - 1n;
const v = (b) => Uint8Array.of(0x90, 0, 0, b);
const config = new W()
  .u64(11155111).u32(512).u64(32).u64(256).u64(1)
  .bytes(concat(v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), new Uint8Array(4)));
for (const e of [0n, 50n, 100n, 56832n, 132608n, 222464n, 272640n, 353024n, FAR]) config.u64(e);
config.bytes(fromHex("d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078")).u64(1655733600).u64(12).u8(4).u8(2).u64(3532);
const configBytes = config.out();
if (configBytes.length !== 202) throw new Error(`config ${configBytes.length} bytes`);

// the instance of the recorded attest messages (docs/guides/runs/2026-11-21-lez/attest-messages: LEZ's state machine
// in process, 21 Nov 2026, on the same Sepolia block 12,105,936), so their requests bind to this world's record
const ATTEST = join(REPO, "docs/guides/runs/2026-11-21-lez/attest-messages");
const attestMessage = (n) => fromHex(readFileSync(join(ATTEST, `message-${n}.hex`), "utf8").trim());
const instanceId = attestMessage(5).subarray(1, 33);
const filler = (tag, n) => Uint8Array.from({ length: n }, (_, i) => (tag * 31 + i * 7) & 0xff);
const store = new W().u8(4).bytes(beaconSsz).bytes(filler(1, 32)).bytes(filler(2, 48)).u8(1).bytes(filler(3, 32)).bytes(filler(4, 48)).out();
const instanceState = new W().bytes(instanceId).bytes(configBytes).bool(true).vec(store).u64(7).bytes(instanceId).bytes(new Uint8Array(32)).out();
const instanceAccount = attestationPda(program, "instance", instanceId);
setState(instanceAccount, program, instanceState);

const record = new W().bytes(beaconSsz).bytes(filler(5, 32)).u8(1)
  .u64(BigInt(eb.number)).bytes(fromHex(eb.hash)).bytes(fromHex(eb.parentHash)).bytes(fromHex(eb.stateRoot)).bytes(fromHex(eb.receiptsRoot)).u64(BigInt(eb.timestamp)).out();
const recordState = new W().bytes(instanceId).u64(11155111).vec(record).bytes(instanceId).out();
const recordAccount = attestationPda(program, "header", instanceId, BigInt(m.slot));
setState(recordAccount, program, recordState);

// ---- three blocks after the recorded ones, SYNTHETIC: each carries one public transaction sending a recorded attest
// message (slot 0xb of 0x1c7d…7238, the same with a failing `== 1` condition, refused, and a log) to the record ------
{
  const pk = filler(9, 32);
  const payer = accountFromPublicKey(pk);
  let prev = blocks[blocks.length - 1];
  const entries = [[5, false], [7, true], [9, false]];
  entries.forEach(([n, refused], k) => {
    const message = attestMessage(n);
    const actor = concat(recordAccount, program);
    const tx = new W().u8(0) // LeeTransaction::Public
      .u32(1).bytes(actor).u32(0).u32(0) // public context: the record actor, no authorized accounts, no promotions
      .bytes(actor).vec(message) // root call
      .u8(1).bytes(payer).u64(4000000).u64(0).u128(60000000) // fee declaration
      .u32(1).bytes(payer).u128(21 + k) // nonces
      .u32(0) // admission evidence
      .u32(1).bytes(filler(10 + k, 64)).bytes(pk) // witness (signature not checked by the pages)
      .out();
    const id = Number(prev.header.block_id) + 1;
    const ts = prev.header.timestamp + 10000n;
    const bodyBytes = refused ? new W().u32(1).bytes(tx).u32(1).u64(0).out() : new W().u32(1).bytes(tx).u32(0).out();
    const draft = new W().u64(id).bytes(prev.header.hash).bytes(new Uint8Array(32)).u64(ts).bytes(bodyBytes).u8(2).out();
    const computed = decodeBlock(draft).computed_hash;
    draft.set(computed, 40);
    const b = decodeBlock(draft);
    if (!b.hash_ok) throw new Error("synthetic block hash");
    world.blocks[id] = b64encode(draft);
    world.head = id;
    for (const t of b.transactions) world.txs[hex(t.hash)] = [b64encode(t.raw), id];
    prev = b;
  });
  world.synthetic_blocks = entries.map((_, k) => Number(blocks[blocks.length - 1].header.block_id) + 1 + k);
}
// ---- the live snapshot (test/live-snapshot.json, read-only from our test network): its names, messages and faucet
// transactions appended in blocks of their own (SYNTHETIC blocks around REAL transactions), and the accounts they touched
const snap = JSON.parse(readFileSync(join(HERE, "live-snapshot.json"), "utf8"));
{
  let prev = decodeBlock(b64decodeNode(world.blocks[world.head]));
  for (const t of snap.txs) {
    const txBytes = b64decodeNode(t.tx);
    const id = Number(prev.header.block_id) + 1;
    const ts = BigInt(t.timestamp) > prev.header.timestamp ? BigInt(t.timestamp) : prev.header.timestamp + 10000n;
    const bodyBytes = new W().u32(1).bytes(txBytes).u32(t.reverted ? 1 : 0).out();
    const rev = t.reverted ? new W().u64(0).out() : new Uint8Array(0);
    const draft = new W().u64(id).bytes(prev.header.hash).bytes(new Uint8Array(32)).u64(ts).bytes(bodyBytes).bytes(rev).u8(2).out();
    draft.set(decodeBlock(draft).computed_hash, 40);
    const b = decodeBlock(draft);
    if (!b.hash_ok || hex(b.transactions[0].hash) !== t.hash) throw new Error("snapshot block");
    world.blocks[id] = b64encode(draft);
    world.txs[t.hash] = [b64encode(b.transactions[0].raw), id];
    world.head = id;
    prev = b;
  }
  for (const [k, a] of Object.entries(snap.accounts)) if (a) world.accounts[k] = a;
  world.snapshot_blocks = [world.head - snap.txs.length + 1, world.head];
}
world.fee_state.height = world.head;

// a user: the first signer in the blocks, funded
const signer = blocks.flatMap((b) => b.transactions).map((t) => signerAccounts(t)[0]).find(Boolean);
setState(signer, NATIVE_TOKEN_PROGRAM_ID, new W().u128(123456789012345678901234567890n).out());
world.accounts[b58(signer)].nonce = 3;

// ---- the mock beacon API and execution RPC (the fixture's real answers) -------------------------------------
const finalizedNow = BigInt(m.slot) + 96n;
world.beacon["11155111"] = {
  headers: { [m.slot]: JSON.parse(readFileSync(join(FIX, `beacon_header_${fixture.finalized_slot}_second.json`), "utf8")) },
  finalized: { execution_optimistic: false, finalized: true, data: { root: `0x${"ab".repeat(32)}`, canonical: true, header: { message: { slot: String(finalizedNow), proposer_index: "1", parent_root: `0x${"00".repeat(32)}`, state_root: `0x${"00".repeat(32)}`, body_root: `0x${"00".repeat(32)}` } } } },
};
world.eth["11155111"] = { blocks: { [eb.number]: eb } };

writeFileSync(join(OUT, "world.json"), JSON.stringify(world));
// the shipped config.json, with this world's endpoints, its instance and the recorded run's deployment of the
// attestation program (an older image than the catalogue's, recognised through known_images)
const shipped = JSON.parse(readFileSync(join(HERE, "..", "config.json"), "utf8"));
const testConfig = {
  ...shipped,
  name: "Ferrata test network (offline test world)",
  rpc: "/rpc",
  wallet: "/wallet/",
  faucet: "/faucet/",
  programs: [
    { key: "attestation", name: "Ferrata attestation (recorded run)", rfp: "RFP-021", what: "The recorded LEZ v0.4.0-rc3 run's deployment.", account: b58(program), image_id: main.image_id, idl: "idl/ferrata_attestation.idl.json" },
    ...shipped.programs,
  ],
  known_images: { ...shipped.known_images, [main.image_id]: { key: "attestation", name: "Ferrata attestation (recorded run)", idl: "idl/ferrata_attestation.idl.json" } },
  instances: [],
  beacon_apis: { 11155111: ["/beacon-mock/11155111"] },
  execution_rpcs: { 11155111: ["/eth-mock/11155111"] },
};
writeFileSync(join(OUT, "config.json"), JSON.stringify(testConfig, null, 2));
// the deployment's programs.json, in the format deploy/testnet/deploy-programs.sh `publish` writes: the pages take the
// instance from here (the test config names none), its account derived from the program and the instance id
writeFileSync(join(OUT, "programs.json"), JSON.stringify({
  network: "ferrata-lez-testnet", lez: "v0.4.0-rc3",
  programs: {
    ferrata_attestation: { program_account: b58(program), image_id: `0x${main.image_id}`, immutable: true },
    ferrata_names: { program_account: snap.programs.ferrata_names, image_id: "0xee9578cab413451e1f0a0cf267a7874ca3231ed0a30907f5d793648ac4e6f790" },
    ferrata_msg: { program_account: snap.programs.ferrata_msg, image_id: "0x04ae1988fe84eef6da472152a75b17e6c7453e6e4775264f12af1a750b945277" },
  },
  instances: { sepolia: { instance_id: hex(instanceId), program: b58(program), preset: "sepolia", layout: "gloas", checkpoint_slot: 11549664 } },
}, null, 1));
// the host's status files in deploy/testnet/host/bin/ferrata-status's format (SYNTHETIC values, one point a minute
// for 24 h, a finality stall of 40 minutes in the middle), for the operator panel and its charts
{
  const now = Math.floor(Date.now() / 1000);
  const hist = [];
  for (let i = 1439; i >= 0; i--) {
    const t = now - i * 60;
    const stall = i > 700 && i < 740;
    const head = 10000 - Math.floor(i * 6);
    hist.push({ t, head, final: stall ? head - 6 * (740 - i) : head - 2, final_age_s: stall ? (740 - i) * 60 : 20 + (i % 3) * 5, alarms: stall ? 1 : 0 });
  }
  const last = hist[hist.length - 1];
  const status = {
    utc: new Date(now * 1000).toISOString().replace(/\.\d+Z$/, "Z"), network: "ferrata-lez-testnet",
    lez: { head: last.head, final: last.final, not_final: last.head - last.final, head_age_s: 4, final_age_s: last.final_age_s, head_status: "Pending" },
    bedrock: { slot: 123456, height: 30210, lib: "4f2a…", tip: "9c1b…", state: "Online", phase: "running" },
    attestation: { updater: { status: "ok", instance: "sepolia", finalized_slot: 11549664 }, monitor: { lag_seconds: 912, alerts: [] } },
    host: { data_used_pct: 12.5, load: 0.4 }, alarms: [],
  };
  mkdirSync(join(OUT, "status"), { recursive: true });
  writeFileSync(join(OUT, "status", "status.json"), JSON.stringify(status));
  writeFileSync(join(OUT, "status", "history.json"), JSON.stringify(hist));
}
writeFileSync(join(OUT, "summary.json"), JSON.stringify({ program: b58(program), instance: b58(instanceAccount), record: b58(recordAccount) }));
console.log(JSON.stringify({ twin: twin.header_account, head: world.head, blocks: Object.keys(world.blocks).length, txs: Object.keys(world.txs).length, accounts: Object.keys(world.accounts).length, program: b58(program), instance: b58(instanceAccount), record: b58(recordAccount), signer: b58(signer) }));
