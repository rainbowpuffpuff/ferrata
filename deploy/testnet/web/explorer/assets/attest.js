// The live state of Ferrata's light-client instances, read from LEZ and checked against Ethereum's public APIs from
// the browser: the instance's finalized header (its store), that header's record, the lag behind Ethereum's own
// finality, the committee period, the weak-subjectivity deadline, and the alarms that follow from them.

import { hex, b58, fromB58, fromHex } from "./util.js";
import { attestationPda, beaconClock, beaconHeaderRoot, decodeAttestationState, CHAIN_NAMES } from "./ferrata.js";
import { rootOf } from "./lez.js";

const DEFAULT_BEACON = {
  1: ["https://ethereum-beacon-api.publicnode.com", "https://lodestar-mainnet.chainsafe.io"],
  11155111: ["https://ethereum-sepolia-beacon-api.publicnode.com", "https://lodestar-sepolia.chainsafe.io"],
};
const DEFAULT_EXEC = {
  1: ["https://ethereum-rpc.publicnode.com"],
  11155111: ["https://ethereum-sepolia-rpc.publicnode.com"],
};

export const beaconApis = (config, chainId) => (config.beacon_apis && config.beacon_apis[chainId]) || DEFAULT_BEACON[chainId] || [];
export const executionRpcs = (config, chainId) => (config.execution_rpcs && config.execution_rpcs[chainId]) || DEFAULT_EXEC[chainId] || [];
export const hostOf = (u) => {
  try {
    return new URL(u, location.href).host;
  } catch {
    return u;
  }
};

async function getJson(url, timeoutMs = 10000) {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const r = await fetch(url, { signal: ctrl.signal, headers: { accept: "application/json" } });
    if (r.status === 404) return null;
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    return await r.json();
  } finally {
    clearTimeout(t);
  }
}

async function ethCall(url, method, params, timeoutMs = 10000) {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const r = await fetch(url, { method: "POST", signal: ctrl.signal, headers: { "content-type": "application/json" }, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }) });
    const j = await r.json();
    if (j.error) throw new Error(j.error.message);
    return j.result;
  } finally {
    clearTimeout(t);
  }
}

/** The beacon API's header at `slot` ("finalized" for its finalized one): {slot, root} or null. */
export async function beaconHeader(api, slot) {
  const j = await getJson(`${api.replace(/\/$/, "")}/eth/v1/beacon/headers/${slot}`);
  if (!j || !j.data) return null;
  return { slot: BigInt(j.data.header.message.slot), root: j.data.root.replace(/^0x/, "").toLowerCase() };
}

export async function executionBlock(rpc, number) {
  const b = await ethCall(rpc, "eth_getBlockByNumber", [`0x${BigInt(number).toString(16)}`, false]);
  return b ? { number: BigInt(b.number), hash: b.hash.replace(/^0x/, "").toLowerCase(), state_root: b.stateRoot.replace(/^0x/, "").toLowerCase(), timestamp: BigInt(b.timestamp) } : null;
}

/** Read an instance and its newest record from LEZ. */
export async function readInstance(rpc, programs, inst) {
  const program = fromB58(inst.program);
  const info = await programs.info(program);
  if (!info.idl) throw new Error(`no IDL for program ${inst.program}`);
  const account = inst.account ? fromB58(inst.account) : attestationPda(program, "instance", fromHex(inst.instance_id));
  const view = await rpc.accountView(account, program);
  const st = view && view.states.find((s) => hex(s.program) === hex(program));
  if (!st || !st.data.length) return { inst, program, account, missing: true };
  const d = decodeAttestationState(info.idl, st.data);
  if (!d || d.type !== "InstanceState") return { inst, program, account, error: "the account's state is not an instance" };
  const cfg = d.plain.config;
  const clock = beaconClock(cfg);
  const out = { inst, program, account, state: d, config: cfg, clock, chainId: Number(cfg.chain_id) };
  if (d.store) {
    out.slot = d.store.finalized.slot;
    out.root = hex(beaconHeaderRoot(d.store.finalized));
    out.recordAccount = attestationPda(program, "header", fromHex(d.plain.id), out.slot);
    const rv = await rpc.accountView(out.recordAccount, program).catch(() => null);
    const rs = rv && rv.states.find((s) => hex(s.program) === hex(program));
    if (rs && rs.data.length) {
      const r = decodeAttestationState(info.idl, rs.data);
      if (r && r.type === "RecordState") out.record = r;
    }
  }
  return out;
}

/** Checks against the public APIs: each provider's root at our slot, its finalized slot, the execution block. */
export async function checkInstance(config, s) {
  const out = { beacon: [], execution: [], networkFinalized: null };
  if (s.slot === undefined) return out;
  await Promise.all(
    beaconApis(config, s.chainId).map(async (api) => {
      const row = { api, host: hostOf(api) };
      try {
        const [at, fin] = await Promise.all([beaconHeader(api, s.slot.toString()), beaconHeader(api, "finalized")]);
        row.root = at ? at.root : null;
        row.equal = at ? at.root === s.root : null;
        row.missing = !at;
        row.finalized = fin ? fin.slot : null;
        if (fin && (out.networkFinalized === null || fin.slot > out.networkFinalized)) out.networkFinalized = fin.slot;
      } catch (e) {
        row.error = e.message || String(e);
      }
      out.beacon.push(row);
    }),
  );
  const ex = s.record && s.record.record && s.record.record.execution;
  if (ex && ex.block_number !== undefined) {
    await Promise.all(
      executionRpcs(config, s.chainId).map(async (rpc) => {
        const row = { rpc, host: hostOf(rpc) };
        try {
          const b = await executionBlock(rpc, ex.block_number);
          row.hash = b ? b.hash : null;
          row.equal = b ? b.hash === hex(ex.block_hash) && (!ex.state_root || b.state_root === hex(ex.state_root)) : null;
        } catch (e) {
          row.error = e.message || String(e);
        }
        out.execution.push(row);
      }),
    );
  }
  return out;
}

/**
 * Alarms: [{level: ok|warn|bad, text}]. Thresholds from config.alarms (epochs): lag_warn (default 4), lag_bad (64).
 */
export function alarms(config, s, checks, now = Date.now()) {
  const a = [];
  const th = Object.assign({ lag_warn_epochs: 4, lag_bad_epochs: 64, period_warn_hours: 12, ws_warn_hours: 24 }, config.alarms || {});
  if (s.missing) return [{ level: "bad", text: "The instance account has no state on this network (not created, or the network was reset)." }];
  if (s.error) return [{ level: "bad", text: s.error }];
  if (!s.state.plain.active) a.push({ level: "warn", text: "Not yet active: the initial committee's adoption has not finished." });
  if (!s.state.store) return [...a, { level: "bad", text: `The instance's store does not decode: ${s.state.inner_error}` }];
  const spe = BigInt(s.config.slots_per_epoch);
  // lag behind Ethereum's finality, by the providers; by the wall clock if none answered
  const wallSlot = s.clock.slotAtMs(now);
  const ref = checks && checks.networkFinalized !== null && checks.networkFinalized !== undefined ? checks.networkFinalized : null;
  const lagSlots = ref !== null ? ref - s.slot : wallSlot - s.slot - 2n * spe;
  const lagEpochs = Number(lagSlots / spe);
  const by = ref !== null ? "Ethereum's finalized checkpoint (public beacon API)" : "the wall clock (no beacon API answered)";
  if (ref !== null) {
    const providersBehind = Number((wallSlot - ref) / spe);
    if (providersBehind >= th.lag_bad_epochs) a.push({ level: "warn", text: `Ethereum's finalized checkpoint, as the public beacon APIs serve it, is ${providersBehind} epochs behind the wall clock: Ethereum's finality is stalled, or the providers are behind.` });
  }
  if (lagEpochs >= th.lag_bad_epochs) a.push({ level: "bad", text: `Finalized header ${lagEpochs} epochs behind ${by}: the updater is not keeping up or has stopped.` });
  else if (lagEpochs >= th.lag_warn_epochs) a.push({ level: "warn", text: `Finalized header ${lagEpochs} epochs behind ${by}.` });
  // committee period
  const periodEndMs = s.clock.timeOfSlotMs(s.clock.periodEndSlot(s.slot));
  if (!s.state.store.next) {
    const hours = (periodEndMs - now) / 3600000;
    if (hours < 0) a.push({ level: "bad", text: "The next sync committee is unknown and the finalized header's period has ended: the instance cannot follow without a committee update." });
    else if (hours < th.period_warn_hours) a.push({ level: "warn", text: `The next sync committee is unknown and the period ends in ${hours.toFixed(1)} h.` });
  }
  // weak subjectivity
  const wsEndMs = s.clock.timeOfSlotMs(s.slot + BigInt(s.config.ws_period_epochs) * spe);
  const wsHours = (wsEndMs - now) / 3600000;
  if (wsHours < 0) a.push({ level: "bad", text: "The finalized header is older than the weak-subjectivity period: the program refuses further updates; a successor instance is needed." });
  else if (wsHours < th.ws_warn_hours) a.push({ level: "warn", text: `The weak-subjectivity period of the finalized header ends in ${wsHours.toFixed(1)} h.` });
  // checks
  if (checks) {
    for (const b of checks.beacon) {
      if (b.equal === false) a.push({ level: "bad", text: `${b.host} serves another block root for slot ${s.slot}: the attested header disagrees with that provider.` });
    }
    for (const e of checks.execution) {
      if (e.equal === false) a.push({ level: "bad", text: `${e.host} serves another execution block for the record's block number.` });
    }
    if (checks.beacon.length && checks.beacon.every((b) => b.error)) a.push({ level: "warn", text: "No public beacon API answered from this browser; the lag is estimated from the wall clock." });
  }
  if (!s.record) a.push({ level: "warn", text: "No record found for the finalized header (header(id, slot) has no state)." });
  if (!a.length) a.push({ level: "ok", text: "Following Ethereum's finality; header checked against public APIs." });
  return a.map((x) => ({ ...x, lagEpochs, lagSlots, wsEndMs, periodEndMs }));
}

/** Recent calls to attestation programs: [{tx, block, index, ins, args, program}] newest first. */
export async function recentAttestationCalls(rpc, programs, fromBlock, toBlock) {
  const list = await rpc.blockRange(fromBlock, toBlock);
  const out = [];
  for (const b of list) {
    for (let i = 0; i < b.transactions.length; i++) {
      const tx = b.transactions[i];
      const root = rootOf(tx);
      if (!root) continue;
      const info = await programs.info(root.to.program_account_id);
      if (info.key !== "attestation" || !info.idl) continue;
      let ins = null, args = null, error = null;
      try {
        ({ ins, args } = info.idl.decodeInstruction(root.message));
      } catch (e) {
        error = e.message;
      }
      out.push({ tx, block: b, index: i, root, program: root.to.program_account_id, ins, args, error, reverted: b.reverted.includes(i) });
    }
  }
  return out.reverse();
}

/** Instances found from `init` calls (the receiver of an init is the instance account). */
export function instancesFromCalls(calls) {
  const seen = new Map();
  for (const c of calls) {
    if (c.ins && c.ins.name === "init" && !c.reverted) {
      const k = b58(c.root.to.account_id);
      if (!seen.has(k)) seen.set(k, { label: "instance", program: b58(c.program), account: k, discovered: true });
    }
  }
  return [...seen.values()];
}

export { CHAIN_NAMES };
