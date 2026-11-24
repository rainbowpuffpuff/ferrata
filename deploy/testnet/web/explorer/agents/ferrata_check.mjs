#!/usr/bin/env node
// Read the Ferrata LEZ test network and check it yourself. Node 18+ (global fetch), no dependencies, read-only.
//
//   node ferrata_check.mjs https://<site>/        // the site serves /config.json and /rpc
//
// 1. The head block: getLastBlockId, getBlock; its hash recomputed from its bytes (SHA-256 over LEZ's encoding).
// 2. Each light-client instance in the site's config.json: its finalized Ethereum beacon header, read from the
//    instance's account (getAccountView) and hashed to its block root (SSZ hash_tree_root).
// 3. That root compared with a public beacon API's root for the same slot.
// Exit 0 if every check holds, 1 if one fails, 2 if something could not be read.

import { createHash } from "node:crypto";

const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const sha256 = (...parts) => createHash("sha256").update(Buffer.concat(parts.map((p) => Buffer.from(p)))).digest();
const pad32 = (s) => Buffer.concat([Buffer.from(s), Buffer.alloc(32 - s.length)]);
const b58encode = (b) => {
  let n = BigInt("0x" + (Buffer.from(b).toString("hex") || "0"));
  let s = "";
  while (n > 0n) {
    s = B58[Number(n % 58n)] + s;
    n /= 58n;
  }
  let z = 0;
  while (z < b.length && b[z] === 0) z++;
  return "1".repeat(z) + s;
};
const b58decode = (s) => {
  let n = 0n;
  for (const c of s) n = n * 58n + BigInt(B58.indexOf(c));
  let h = n.toString(16);
  if (h.length % 2) h = "0" + h;
  const body = n === 0n ? Buffer.alloc(0) : Buffer.from(h, "hex");
  let z = 0;
  while (z < s.length && s[z] === "1") z++;
  return Buffer.concat([Buffer.alloc(z), body]);
};

async function getJson(url, body) {
  const r = await fetch(url, body ? { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) } : {});
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.json();
}
async function rpc(url, method, params = []) {
  const r = await getJson(url, { jsonrpc: "2.0", id: 1, method, params });
  if (r.error) throw new Error(`${method}: ${JSON.stringify(r.error)}`);
  return r.result;
}

// LEZ v0.4: SHA-256("/LEE/v0.3/Message/Block/" padded to 32 ‖ id ‖ prev ‖ timestamp ‖ body); the last byte is the
// Bedrock status, outside the hash.
const blockHash = (raw) => sha256(pad32("/LEE/v0.3/Message/Block/"), raw.subarray(0, 40), raw.subarray(72, 80), raw.subarray(80, raw.length - 1));
const pda = (program, tag, ...parts) => sha256(pad32("/LEE/v0.2/AccountId/PDA/"), program, sha256(Buffer.from("ferrata-lez/v1/"), Buffer.from(tag), ...parts));
function beaconRoot(ssz) {
  const leaf = (b) => Buffer.concat([b, Buffer.alloc(32 - b.length)]);
  let l = [leaf(ssz.subarray(0, 8)), leaf(ssz.subarray(8, 16)), ssz.subarray(16, 48), ssz.subarray(48, 80), ssz.subarray(80, 112), Buffer.alloc(32), Buffer.alloc(32), Buffer.alloc(32)];
  while (l.length > 1) l = Array.from({ length: l.length / 2 }, (_, i) => sha256(l[2 * i], l[2 * i + 1]));
  return l[0];
}

const PRESET_CHAIN = { mainnet: 1, sepolia: 11155111, holesky: 17000, hoodi: 560048 };
const accountBytes = (s) => {
  const t = String(s).replace(/^Public\//, "");
  return /^(0x)?[0-9a-fA-F]{64}$/.test(t) ? Buffer.from(t.replace(/^0x/, ""), "hex") : b58decode(t);
};

// The site's config.json instances, else those of the deployment's programs.json.
async function instances(site, cfg) {
  if ((cfg.instances || []).length) return cfg.instances;
  let dep;
  try {
    dep = await getJson(new URL("programs.json", site));
  } catch {
    return [];
  }
  return Object.entries(dep.instances || {})
    .filter(([, i]) => i.program && i.instance_id)
    .map(([name, i]) => ({ label: name[0].toUpperCase() + name.slice(1), program: b58encode(accountBytes(i.program)), instance_id: i.instance_id.replace(/^0x/, ""), account: i.instance_account ? b58encode(accountBytes(i.instance_account)) : undefined }));
}

async function main() {
  const site = new URL((process.argv[2] || "http://127.0.0.1:18750/").replace(/\/?$/, "/"));
  const cfg = await getJson(new URL("config.json", site));
  const rpcUrl = new URL(cfg.rpc || "rpc", site).href;
  let ok = true;

  const head = await rpc(rpcUrl, "getLastBlockId");
  const raw = Buffer.from(await rpc(rpcUrl, "getBlock", [Number(head)]), "base64");
  const same = blockHash(raw).equals(raw.subarray(40, 72));
  ok &&= same;
  console.log(`head block #${raw.readBigUInt64LE(0)}: ${raw.readUInt32LE(80)} transaction(s), ${["Pending", "Safe", "Finalized"][raw[raw.length - 1]]} on Bedrock; hash ${raw.subarray(40, 72).toString("hex").slice(0, 16)}… recomputed: ${same ? "equal" : "DIFFERENT"}`);

  for (const inst of await instances(site, cfg)) {
    const program = b58decode(inst.program);
    const account = inst.account ? b58decode(inst.account) : pda(program, "instance", Buffer.from(inst.instance_id, "hex"));
    const view = await rpc(rpcUrl, "getAccountView", [{ account_id: b58encode(account), program_account_id: inst.program }]);
    const state = Buffer.from((view && view.data.actor_states[inst.program]) || []);
    if (!state.length) {
      console.log(`${inst.label}: no state at ${b58encode(account)}`);
      ok = false;
      continue;
    }
    // InstanceState (borsh): id[32] config[202] active u8 store Vec<u8> ...; CompactStore: fork u8, header[112]
    const chainId = state.readBigUInt64LE(32);
    const header = state.subarray(32 + 202 + 1 + 4 + 1, 32 + 202 + 1 + 4 + 1 + 112);
    const slot = header.readBigUInt64LE(0);
    const root = beaconRoot(header).toString("hex");
    console.log(`${inst.label}: chain ${chainId}, finalized slot ${slot}, block root 0x${root}`);
    for (const api of (cfg.beacon_apis || {})[String(chainId)] || []) {
      const url = new URL(`${api.replace(/\/$/, "")}/eth/v1/beacon/headers/${slot}`, site);
      try {
        const theirs = (await getJson(url)).data.root.slice(2);
        ok &&= theirs === root;
        console.log(`  ${url.host}: ${theirs === root ? "equal" : `DIFFERENT: 0x${theirs}`}`);
      } catch (e) {
        console.log(`  ${url.host}: could not read (${e.message})`);
      }
    }
  }
  process.exit(ok ? 0 : 1);
}

main().catch((e) => {
  console.error(`error: ${e.message}`);
  process.exit(2);
});
