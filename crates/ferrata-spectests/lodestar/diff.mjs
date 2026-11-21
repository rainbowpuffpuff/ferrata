// Lodestar's light client (@lodestar/light-client 1.43.0, TypeScript) as a second differential oracle
// for ferrata-core, beside Helios (../HELIOS-DIFFERENTIAL.md).
//
// Reads the corpus that `helios_oracle.rs`'s `export_random_mutations_for_a_second_client` writes
// (mainnet periods 1899-1903: the bootstrap, the five real updates as served and 2,000 seeded
// mutations of them, with ferrata's and Helios's verdicts and ferrata's heads) and runs every record
// through Lodestar's `LightclientSpec.onUpdate` against a copy of a store bootstrapped from the same bytes
// and advanced by the updates before it, at current slot = signature slot + 1 (as the Rust side does).
// Lodestar must refuse what ferrata's spec store refuses and accept what it accepts, with the same
// finalized and optimistic headers. Prints the tally by region and every disagreement; exits 1 on
// any disagreement.
//
//   node diff.mjs <corpus directory>
import fs from "node:fs";
import path from "node:path";
import {createBeaconConfig} from "@lodestar/config";
import {mainnetChainConfig} from "@lodestar/config/networks";
import * as lightClient from "@lodestar/light-client";
import {LightclientSpec} from "@lodestar/light-client/spec";
import {ssz} from "@lodestar/types";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node diff.mjs <corpus directory>");
  process.exit(2);
}
const read = (name) => fs.readFileSync(path.join(dir, name));
const meta = JSON.parse(read("meta.json").toString("utf8"));
const unhex = (s) => Uint8Array.from(Buffer.from(s.replace(/^0x/, ""), "hex"));
const hex = (b) => Buffer.from(b).toString("hex");

const config = createBeaconConfig(mainnetChainConfig, unhex(meta.genesis_validators_root));
// The updates' slots are in Fulu, whose light-client types are Electra's.
const types = ssz.fulu?.LightClientUpdate ? ssz.fulu : ssz.electra;
const bootstrap = types.LightClientBootstrap.deserialize(read("bootstrap.ssz"));
let bootstrapChecked = "not exported by this version";
if (typeof lightClient.validateLightClientBootstrap === "function") {
  lightClient.validateLightClientBootstrap(config, unhex(meta.trusted_root), bootstrap);
  bootstrapChecked = "validateLightClientBootstrap: valid under the trusted root";
} else if (hex(ssz.phase0.BeaconBlockHeader.hashTreeRoot(bootstrap.header.beacon)) === meta.trusted_root.replace(/^0x/, "")) {
  bootstrapChecked = "header root equal to the trusted root (validateLightClientBootstrap not exported)";
} else {
  throw Error("the bootstrap's header root is not the trusted root");
}

// The corpus: kind, update, mutation, length (u32 LE each), then the bytes.
const buf = read("corpus.bin");
const records = [];
for (let o = 0; o < buf.length; ) {
  const [kind, update, mutation, len] = [0, 4, 8, 12].map((k) => buf.readUInt32LE(o + k));
  records.push({kind, update, mutation, bytes: new Uint8Array(buf.subarray(o + 16, o + 16 + len))});
  o += 16 + len;
}
const originals = records.filter((r) => r.kind === 0).sort((a, b) => a.update - b.update).map((r) => r.bytes);

const lines = read("ferrata.tsv").toString("utf8").trim().split("\n");
const columns = lines[0].split("\t");
const ferrata = new Map();
for (const line of lines.slice(1)) {
  const f = Object.fromEntries(line.split("\t").map((v, i) => [columns[i], v]));
  ferrata.set(`${f.kind}/${f.update}/${f.mutation}`, f);
}
if (ferrata.size !== records.length || records.length !== meta.records) {
  throw Error(`${records.length} records, ${ferrata.size} rows of verdicts, meta says ${meta.records}`);
}

const decode = (bytes) => {
  try {
    return types.LightClientUpdate.deserialize(bytes);
  } catch {
    return null;
  }
};
const root = (header) => hex(ssz.phase0.BeaconBlockHeader.hashTreeRoot(header.beacon));
const opts = {allowForcedUpdates: false, updateHeadersOnForcedUpdate: false};

// A store as it stands just before update i: the bootstrap, then the updates before i as served.
function storeBefore(i) {
  const lc = new LightclientSpec(config, opts, bootstrap);
  for (let k = 0; k < i; k++) {
    const u = decode(originals[k]);
    lc.onUpdate(u.signatureSlot + 1, u);
  }
  return lc;
}

// A copy of a store that one more update cannot alter: `onUpdate` replaces the headers (never
// mutates them) and changes only the store's maps, which are copied; the committees' keys are
// shared, read only. Every 50th mutation is also run on a store rebuilt from the bootstrap
// (`storeBefore`), and the two runs must agree.
function copy(lc) {
  const store = Object.create(Object.getPrototypeOf(lc.store));
  for (const [k, v] of Object.entries(lc.store)) store[k] = v instanceof Map ? new Map(v) : v;
  return Object.assign(Object.create(Object.getPrototypeOf(lc)), lc, {store});
}

function judge(lc, bytes) {
  const u = decode(bytes);
  if (!u) return {ok: false, why: "does not decode"};
  try {
    lc.onUpdate(u.signatureSlot + 1, u);
  } catch (e) {
    return {ok: false, why: String(e?.message ?? e).slice(0, 160)};
  }
  const s = lc.store;
  return {
    ok: true,
    heads: [s.finalizedHeader.beacon.slot, root(s.finalizedHeader), s.optimisticHeader.beacon.slot, root(s.optimisticHeader)],
  };
}

const tally = new Map(); // region -> [records, refused by all four, accepted by ferrata's spec store and Lodestar]
const reasons = new Map(); // Lodestar's refusal message (digits elided) -> count
const disagreements = [];
const accepted = [];
const started = Date.now();
// The mutations of update i, then update i as served (to advance), as the Rust side ran them.
let chained = storeBefore(0);
let checked = 0;
for (let i = 0; i < originals.length; i++) {
  for (const r of records.filter((x) => x.kind === 1 && x.update === i)) {
    const f = ferrata.get(`1/${i}/${r.mutation}`);
    const v = judge(copy(chained), r.bytes);
    if (r.mutation % 50 === 0) {
      const w = judge(storeBefore(i), r.bytes);
      checked += 1;
      if (JSON.stringify(v) !== JSON.stringify(w)) {
        disagreements.push(`update ${i} mutation ${r.mutation}: a copied store and a rebuilt one disagree`);
      }
    }
    const t = tally.get(f.region) ?? [0, 0, 0];
    t[0] += 1;
    const spec = f.spec === "1";
    const label = `update ${i} mutation ${r.mutation} (${f.region})`;
    if (!v.ok) {
      const reason = v.why.replace(/\d+/g, "N");
      reasons.set(reason, (reasons.get(reason) ?? 0) + 1);
    }
    if (!v.ok && f.policy === "0" && !spec && f.helios === "0") t[1] += 1;
    if (v.ok && spec) {
      t[2] += 1;
      const want = [Number(f.finalized_slot), f.finalized_root, Number(f.optimistic_slot), f.optimistic_root];
      const same = want.every((x, k) => x === v.heads[k]);
      accepted.push(`${label}: heads ${same ? "the same" : "DIFFER"}`);
      if (!same) disagreements.push(`${label}: both accept, the heads differ: ferrata ${want.join(" ")}, Lodestar ${v.heads.join(" ")}`);
    }
    if (v.ok !== spec) {
      disagreements.push(`${label}: ferrata's spec store ${spec ? "accepts" : "refuses"}, Lodestar ${v.ok ? "accepts" : `refuses (${v.why})`}`);
    }
    tally.set(f.region, t);
  }
  // Update i as served: accepted by both, the same heads.
  const f = ferrata.get(`0/${i}/0`);
  const v = judge(chained, originals[i]);
  const want = [Number(f.finalized_slot), f.finalized_root, Number(f.optimistic_slot), f.optimistic_root];
  if (!v.ok || !want.every((x, k) => x === v.heads[k])) {
    disagreements.push(`update ${i} as served: ferrata ${want.join(" ")}, Lodestar ${v.ok ? v.heads.join(" ") : `refuses (${v.why})`}`);
  }
}

const total = [...tally.values()].reduce((a, t) => a.map((x, k) => x + t[k]), [0, 0, 0]);
let out = `Lodestar light client ${JSON.parse(fs.readFileSync(new URL("./node_modules/@lodestar/light-client/package.json", import.meta.url))).version}, node ${process.version}; bootstrap: ${bootstrapChecked}\n`;
out += `${records.length} records: ${originals.length} updates as served (each accepted by both, the same heads: ${disagreements.filter((d) => d.includes("as served")).length === 0 ? "yes" : "NO"}) and ${total[0]} mutations (seed ${meta.seed}, ${meta.per_update} per update), ${((Date.now() - started) / 1000).toFixed(1)} s\n`;
out += `${checked} mutations also run on a store rebuilt from the bootstrap: the same outcome as on the copied store (else listed below)\n`;
out += "region | mutations | refused by all four (ferrata's policy and spec stores, Helios, Lodestar) | accepted by ferrata's spec store and Lodestar (same heads)\n";
for (const [region, t] of [...tally.entries()].sort()) out += `${region} | ${t[0]} | ${t[1]} | ${t[2]}\n`;
out += `total | ${total[0]} | ${total[1]} | ${total[2]}\n`;
out += "Lodestar's refusals by message (numbers as N):\n";
for (const [reason, n] of [...reasons.entries()].sort((a, b) => b[1] - a[1])) out += `  ${n} ${reason}\n`;
out += "mutations accepted by both:\n";
for (const a of accepted) out += `  ${a}\n`;
out += `disagreements: ${disagreements.length}\n`;
for (const d of disagreements) out += `  ${d}\n`;
console.log(out);
process.exit(disagreements.length === 0 ? 0 : 1);
