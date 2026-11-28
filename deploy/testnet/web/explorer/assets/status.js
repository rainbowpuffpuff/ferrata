// The live attestation dashboard: every light-client instance's head, finality lag, committee period, weak-subjectivity
// deadline and alarms, its newest record checked against public beacon APIs and an execution RPC from this browser,
// and the latest updates and attestations on LEZ.

import { shell, el, kv, table, panel, loading, errorBox, badge, stat, accountLink, blockLink, txLink, hexSpan, setTitle, href, renderValue } from "./ui.js";
import { hex, b58, fmtInt, fmtTime, fmtAge, shortHex, fromB58 } from "./util.js";
import { Programs } from "./programs.js";
import { readInstance, checkInstance, alarms, recentAttestationCalls, instancesFromCalls, beaconApis, executionRpcs, hostOf, CHAIN_NAMES } from "./attest.js";
import { field, plain } from "./idl.js";
import { attestationPda } from "./ferrata.js";
import { checkAttest, factLines, verdictBadge } from "./facts.js";
import { lineChart } from "./chart.js";
import { outcome } from "./outcome.js";

const { main, config, rpc } = await shell("status");
const programs = new Programs(rpc, config);
const SCAN = Number(config.status_scan_blocks || 600);
setTitle("Attestation status");

const top = el("div");
const instBox = el("div");
const activity = el("div");
const operator = el("div");
main.replaceChildren(
  el("h1", {}, "Live attestation status"),
  el("p", { class: "muted" }, "Ferrata's Ethereum light clients run as programs on this LEZ network. Each instance follows Ethereum's finality from sync-committee signatures verified on LEZ; every attested fact is bound to one of its records. This page reads them from LEZ and checks them against public Ethereum APIs, from your browser."),
  top, instBox, operator, activity,
);

let busy = false;
async function refresh() {
  if (busy) return;
  busy = true;
  try {
    await render();
  } catch (e) {
    top.replaceChildren(errorBox(e));
    console.error(e);
  } finally {
    busy = false;
  }
}
refresh();
setInterval(refresh, Number(config.status_refresh_s || 30) * 1000);

async function render() {
  if (!instBox.firstChild) instBox.replaceChildren(loading("reading the instances…"));
  const head = await rpc.lastBlockId();
  const calls = await recentAttestationCalls(rpc, programs, Math.max(1, head - SCAN + 1), head);
  let instances = (config.instances || []).filter((i) => i.program && (i.account || i.instance_id));
  if (!instances.length) instances = instancesFromCalls(calls);
  const states = await Promise.all(instances.map((i) => readInstance(rpc, programs, i).catch((e) => ({ inst: i, error: e.message }))));
  const checks = await Promise.all(states.map((s) => (s.slot !== undefined ? checkInstance(config, s).catch(() => null) : null)));
  const all = states.map((s, i) => ({ s, c: checks[i], a: s.error && !s.state ? [{ level: "bad", text: s.error }] : alarms(config, s, checks[i]) }));

  const bad = all.reduce((n, x) => n + x.a.filter((y) => y.level === "bad").length, 0);
  const warn = all.reduce((n, x) => n + x.a.filter((y) => y.level === "warn").length, 0);
  top.replaceChildren(
    !instances.length
      ? el("div", { class: "warnbox" }, `No light-client instance is configured, and no init call was seen in the last ${SCAN} blocks.`)
      : el("div", { class: bad ? "badbox" : warn ? "warnbox" : "note" }, bad ? `${bad} alarm(s) need attention` : warn ? `${warn} warning(s)` : `All ${instances.length} instance(s) healthy`, el("span", { class: "muted small" }, ` · LEZ block #${fmtInt(head)} · checked ${new Date().toISOString().slice(11, 19)} UTC`)),
  );
  instBox.replaceChildren(...all.map(({ s, c, a }) => instancePanel(s, c, a, calls)));
  operator.replaceChildren(await operatorPanel());
  activity.replaceChildren(await activityPanels(calls, head));
}

function instancePanel(s, c, a, calls) {
  const name = s.inst.label || "instance";
  if (!s.state) {
    return panel(el("h2", {}, name), a.map((x) => el("div", { class: "alarm" }, badge(x.level, x.level), el("span", {}, x.text))), s.account ? el("p", {}, "Account: ", accountLink(s.account, { short: false })) : null);
  }
  const p = s.state.plain;
  const st = s.state.store;
  const chain = CHAIN_NAMES[s.chainId] || `chain ${s.chainId}`;
  const level = a.some((x) => x.level === "bad") ? "bad" : a.some((x) => x.level === "warn") ? "warn" : "ok";
  const first = a[0] || {};
  const spe = BigInt(s.config.slots_per_epoch);
  const parts = [];
  parts.push(el("h2", {}, `${name} `, badge(chain, "info"), " ", p.active ? badge("active", "ok") : badge("not active", "warn"), " ", badge(level === "ok" ? "healthy" : level === "warn" ? "warning" : "alarm", level)));
  if (st) {
    const slotMs = s.clock.timeOfSlotMs(s.slot);
    parts.push(el("div", { class: "stats" },
      stat("Finalized slot", fmtInt(s.slot), `epoch ${fmtInt(s.clock.epochOf(s.slot))}`),
      stat("Header time", fmtAge(slotMs), fmtTime(slotMs)),
      stat("Behind Ethereum's finality", first.lagEpochs !== undefined ? `${fmtInt(first.lagSlots < 0n ? 0n : first.lagSlots)} slots` : "—", first.lagEpochs !== undefined ? `${Math.max(0, first.lagEpochs)} epoch(s)${c && c.networkFinalized !== null ? ` · Ethereum at ${fmtInt(c.networkFinalized)}` : " (by wall clock)"}` : ""),
      stat("Committee period", fmtInt(s.clock.periodOf(s.slot)), st.next ? "next committee known" : "next committee unknown"),
      stat("Period ends", fmtAge(first.periodEndMs), fmtTime(first.periodEndMs)),
      stat("Jobs applied", fmtInt(p.applied), st.fork + " layout"),
    ));
  }
  parts.push(el("div", {}, a.map((x) => el("div", { class: "alarm" }, badge(x.level === "ok" ? "ok" : x.level, x.level), el("span", {}, x.text)))));

  // the record and the checks
  const rec = s.record && s.record.record;
  const rows = [
    ["Instance account", accountLink(s.account, { short: false })],
    ["Instance id", el("span", { class: "mono small" }, p.id)],
    ["Program", accountLink(s.program, { short: false })],
    ["Finalized block root", s.root ? hexSpan(fromHexSafe(s.root)) : "—"],
    ["Its record", s.recordAccount ? accountLink(s.recordAccount, { short: false }) : "—"],
  ];
  if (rec) {
    const ex = rec.execution;
    if (ex.block_number !== undefined) rows.push(["Execution block", el("span", {}, fmtInt(ex.block_number), " · ", el("span", { class: "mono small" }, hex(ex.block_hash)))]);
    else if (ex.block_hash) rows.push(["Execution block hash", el("span", { class: "mono small" }, hex(ex.block_hash))]);
    if (ex.state_root) rows.push(["Execution state root", el("span", { class: "mono small" }, hex(ex.state_root))]);
  }
  rows.push(["Weak-subjectivity end", first.wsEndMs ? `${fmtTime(first.wsEndMs)} (${fmtAge(first.wsEndMs)})` : "—"]);
  parts.push(el("div", { class: "grid two" },
    el("div", {}, el("h3", {}, "On LEZ"), kv(rows)),
    el("div", {}, el("h3", {}, "Checked from this browser"), checksView(s, c))));
  return panel(...parts);
}

function fromHexSafe(h) {
  return Uint8Array.from(h.match(/../g).map((x) => parseInt(x, 16)));
}

function checksView(s, c) {
  if (!c) return el("p", { class: "muted" }, "no checks (no finalized header yet)");
  const rows = [];
  for (const b of c.beacon) {
    rows.push([
      el("span", {}, "beacon root @ slot ", fmtInt(s.slot)),
      el("a", { href: `${b.api.replace(/\/$/, "")}/eth/v1/beacon/headers/${s.slot}`, rel: "noopener" }, b.host),
      b.error ? badge("unreachable", "warn") : b.missing ? badge("slot not served", "warn") : b.equal ? badge("equal", "ok") : badge("DIFFERENT", "bad"),
    ]);
  }
  for (const e of c.execution) {
    rows.push([
      el("span", {}, "execution block hash"),
      el("span", {}, e.host),
      e.error ? badge("unreachable", "warn") : e.hash === null ? badge("not found", "warn") : e.equal ? badge("equal", "ok") : badge("DIFFERENT", "bad"),
    ]);
  }
  const how = el("details", {}, el("summary", { class: "small muted" }, "check it yourself"),
    el("pre", { class: "code" }, [
      ...beaconApis(config, s.chainId).slice(0, 1).map((api) => `curl -s ${api.replace(/\/$/, "")}/eth/v1/beacon/headers/${s.slot} | jq -r .data.root\n# expect 0x${s.root}`),
      ...(s.record && s.record.record.execution.block_number !== undefined
        ? executionRpcs(config, s.chainId).slice(0, 1).map((r) => `\ncurl -s ${r} -H 'content-type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["0x${s.record.record.execution.block_number.toString(16)}",false]}' | jq -r .result.hash\n# expect 0x${hex(s.record.record.execution.block_hash)}`)
        : []),
    ].join("")));
  return el("div", {}, table(["Check", "Provider", "Result"], rows, { empty: "no provider configured for this chain" }), how);
}

const UPDATE_NAMES = new Set(["init", "open", "miller", "adopt", "finish", "apply", "apply_verified", "job_open", "record", "succeed", "spawn"]);

async function activityPanels(calls, head) {
  const updates = calls.filter((c) => c.ins && UPDATE_NAMES.has(c.ins.name)).slice(0, 25);
  const attests = calls.filter((c) => c.ins && c.ins.name === "attest").slice(0, 25);
  const upRows = updates.map((c) => [
    el("span", { title: fmtTime(Number(c.block.header.timestamp)) }, fmtAge(Number(c.block.header.timestamp))),
    blockLink(Number(c.block.header.block_id)),
    txLink(c.tx.hash),
    el("span", { class: "callname" }, c.ins.name, c.ins.name === "adopt" ? ` part ${plain(c.args[0][1])}` : ""),
    accountLink(c.root.to.account_id),
    outcome(c.block, c.index, config.native),
  ]);
  const recordCache = new Map();
  const atRows = [];
  for (const c of attests) {
    let facts, verdict;
    try {
      const info = await programs.info(c.program);
      const chk = await checkAttest(rpc, info, c.root, c.args, recordCache);
      facts = factLines(chk.result);
      verdict = verdictBadge(chk.result, c.reverted);
    } catch (e) {
      facts = [el("span", { class: "muted" }, `not re-checked: ${e.message}`)];
      verdict = badge("—");
    }
    atRows.push([
      el("span", { title: fmtTime(Number(c.block.header.timestamp)) }, fmtAge(Number(c.block.header.timestamp))),
      blockLink(Number(c.block.header.block_id)),
      txLink(c.tx.hash),
      el("div", { class: "wrap" }, facts.length ? facts : el("span", { class: "muted" }, "relations only")),
      accountLink(c.root.to.account_id),
      outcome(c.block, c.index, config.native, { applied: "attested", reverted: "refused" }),
      verdict,
    ]);
  }
  return el("div", {},
    panel(el("h2", {}, "Latest light-client updates"), el("p", { class: "muted small" }, `Calls to the attestation program in the last ${SCAN} LEZ blocks (#${fmtInt(Math.max(1, head - SCAN + 1))}–#${fmtInt(head)}): a period or finality update is opened, its signature checked (miller), the committee adopted in parts and finished, writing the new header's record.`), table(["Age", "Block", "Transaction", "Step", "Account", "Outcome"], upRows, { empty: "no update in view" })),
    panel(el("h2", {}, "Latest attested Ethereum facts"), el("p", { class: "muted small" }, "Each attest call proves facts (accounts, storage slots, logs, accumulator leaves) against the record it names; an applied call means the program verified every proof against that record's roots. This page verifies the same proofs again in your browser, with its own Keccak, RLP and trie code, and shows whether its verdict agrees with the chain's. The record's header is checked against public beacon APIs above."), table(["Age", "Block", "Transaction", "Facts (values from the proofs)", "Record", "On LEZ", "Re-checked here"], atRows, { empty: "no attestation in view" })),
  );
}

// ---- the operator's view: the host's own status file (deploy/testnet/host/bin/ferrata-status, every minute) ----------

const ROOT_URL = new URL("../", import.meta.url);
async function getJsonMaybe(path) {
  try {
    const r = await fetch(new URL(path, ROOT_URL), { cache: "no-store" });
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  }
}

function scalars(o) {
  if (!o || typeof o !== "object") return [];
  return Object.entries(o).filter(([, v]) => v === null || ["string", "number", "boolean"].includes(typeof v)).slice(0, 12).map(([k, v]) => [k, el("span", { class: "mono small" }, String(v))]);
}

async function operatorPanel() {
  if (config.status_api === "") return el("div");
  const [st, hist] = await Promise.all([getJsonMaybe(config.status_api || "status/status.json"), getJsonMaybe(config.status_history || "status/history.json")]);
  if (!st) return el("div");
  const lez = st.lez || {};
  const bed = st.bedrock || {};
  const att = st.attestation || {};
  const mon = att.monitor && typeof att.monitor === "object" ? att.monitor : null;
  const upd = att.updater && typeof att.updater === "object" ? att.updater : null;
  const alarmsList = (st.alarms || []).map((a) => el("div", { class: "alarm" }, badge(a.level === "critical" ? "critical" : a.level, a.level === "critical" ? "bad" : a.level === "warn" ? "warn" : ""), el("strong", { class: "small" }, a.what || ""), el("span", { class: "small" }, typeof a.detail === "string" ? a.detail : JSON.stringify(a.detail))));
  const h = Array.isArray(hist) ? hist : [];
  const age = h.map((p) => ({ t: p.t, v: typeof p.final_age_s === "number" ? p.final_age_s : null }));
  const behind = h.map((p) => ({ t: p.t, v: typeof p.head === "number" && typeof p.final === "number" ? p.head - p.final : null }));
  return panel(
    el("h2", {}, "The operator's view ", el("span", { class: "muted small" }, `· the host's monitor, ${st.utc || ""}`)),
    el("p", { class: "muted small" }, "What the network's own host reports every minute: its Logos Blockchain node, how far LEZ's blocks are final on it, the updater daemon's health and attest-monitor's alerts. The checks above are made from your browser; these are the operator's, from the inside."),
    alarmsList.length ? el("div", {}, alarmsList) : el("div", { class: "alarm" }, badge("ok", "ok"), el("span", {}, "no host alarm")),
    el("div", { class: "stats" },
      stat("Logos Blockchain node", bed.height !== undefined ? `height ${fmtInt(bed.height)}` : "—", bed.lib !== undefined ? `LIB ${String(bed.lib).slice(0, 12)}…${bed.slot !== undefined ? ` · slot ${fmtInt(bed.slot)}` : ""}` : bed.error ? "unreachable" : ""),
      stat("LEZ final on Bedrock", lez.final !== undefined && lez.final !== null ? `#${fmtInt(lez.final)}` : "—", lez.final_age_s !== undefined && lez.final_age_s !== null ? `${lez.final_age_s} s old · ${lez.not_final} block(s) not final` : ""),
      stat("Updater", upd ? String(upd.status || upd.state || (upd.unavailable ? "unreachable" : "up")) : "—", upd && upd.unavailable ? "" : "the daemon's /health"),
      stat("attest-monitor", mon ? `${(mon.alerts || []).length} alert(s)` : "—", mon && typeof mon.lag_seconds === "number" ? `lag ${mon.lag_seconds} s` : ""),
    ),
    h.length ? el("div", { class: "grid two" },
      lineChart(age, { title: "Age of the newest LEZ block final on Bedrock (s), last 24 h", unit: "s", ref: [{ v: 300, label: "warn 300 s" }, { v: 1800, label: "critical 1,800 s" }] }),
      lineChart(behind, { title: "LEZ blocks not yet final on Bedrock, last 24 h", unit: "blocks" })) : null,
    upd ? el("details", {}, el("summary", { class: "small muted" }, "updater /health"), kv(scalars(upd))) : null,
    mon ? el("details", {}, el("summary", { class: "small muted" }, "attest-monitor status"), kv(scalars(mon)), (mon.alerts || []).length ? el("pre", { class: "code" }, JSON.stringify(mon.alerts, null, 1)) : null) : null,
  );
}
