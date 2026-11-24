// Rendering of an attest request re-checked in the browser (assets/eth.js), shared by the explorer and the status page.

import { el, badge, kv, accountLink } from "./ui.js";
import { hex, fmtInt } from "./util.js";
import { checkRequest, requestFromIdl } from "./eth.js";
import { decodeAttestationState } from "./ferrata.js";

const short = (b) => `0x${hex(b).replace(/^0+(?=.)/, "")}`;
const addr = (b) => `0x${hex(b)}`;

/** {req, record, result} for an attest call: the record is read from LEZ (the call's receiver). */
export async function checkAttest(rpc, info, root, args, cache = new Map()) {
  const req = requestFromIdl(args[0][1]);
  const key = hex(root.to.account_id);
  if (!cache.has(key)) {
    cache.set(key, rpc.accountView(root.to.account_id, root.to.program_account_id).then((v) => {
      const st = v && v.states.find((s) => hex(s.program) === hex(root.to.program_account_id));
      const d = st && st.data.length ? decodeAttestationState(info.idl, st.data) : null;
      return d && d.type === "RecordState" ? d : null;
    }).catch(() => null));
  }
  const record = await cache.get(key);
  const result = checkRequest(req, record ? record.record : null);
  return { req, record, result };
}

/** One line per fact, values included: for tables. */
export function factLines(result) {
  const out = [];
  for (const f of result.facts) {
    if (f.kind === "account") {
      const head = f.absent ? "absent" : `nonce ${fmtInt(f.nonce)}, balance ${fmtInt(f.balance)} wei`;
      out.push(el("div", {}, el("span", { class: "mono small" }, addr(f.address)), " ", f.proof_ok ? el("span", { class: "muted small" }, head) : badge("bad proof", "bad")));
      for (const s of f.slots) out.push(el("div", { class: "small" }, "  slot ", el("span", { class: "mono" }, short(s.slot)), " = ", s.proof_ok ? el("span", { class: "mono" }, short(s.value)) : badge("bad proof", "bad"), condText(s.conditions)));
      if (f.conditions.length) out.push(el("div", { class: "small" }, condText(f.conditions)));
    } else {
      out.push(el("div", {}, `log ${f.log_index} of tx ${f.tx_index}`, f.proof_ok ? el("span", { class: "muted small" }, ` from `, el("span", { class: "mono" }, addr(f.address)), `, ${f.topics.length} topic(s)`) : badge("bad proof", "bad"), condText(f.conditions)));
    }
  }
  return out;
}

function condText(conds) {
  if (!conds.length) return null;
  return el("span", {}, conds.map((c) => el("span", { class: "small" }, " · ", c.text, " ", c.holds ? badge("holds", "ok") : badge("fails", "bad"))));
}

/** The browser's verdict next to the chain's outcome. */
export function verdictBadge(result, reverted) {
  const agrees = result.ok === !reverted;
  return el("span", {}, result.ok ? badge("holds", "ok") : badge("fails", "warn"), " ", agrees ? badge("agrees with the chain", "info") : badge("DISAGREES", "bad"));
}

/** The full panel for a transaction page. */
export function factsPanel({ req, record, result }, reverted) {
  const rows = [];
  rows.push(["Record", record ? el("span", {}, `slot ${fmtInt(record.record.beacon.slot)}`, record.record.execution.block_number !== undefined ? ` · execution block ${fmtInt(record.record.execution.block_number)}` : "") : el("span", { class: "muted" }, "not readable (the record account has no state)")]);
  if (result.header) rows.push(["Request's header", el("span", {}, `block ${fmtInt(result.header.number)} · keccak `, el("span", { class: "mono small" }, short(result.header.hash)), " ", result.header.binds === undefined ? badge("no record to bind", "warn") : result.header.binds ? badge("equals the record's block hash", "ok") : badge("differs from the record", "bad"))]);
  rows.push(["Facts", el("div", {}, factLines(result))]);
  for (const n of result.notes) rows.push(["Note", n]);
  rows.push(["Verdict in this browser", verdictBadge(result, reverted)]);
  return el("div", { class: "panel" },
    el("h2", {}, "Facts, re-checked in this browser"),
    el("p", { class: "muted small" }, "The request's Merkle-Patricia proofs were verified here with an independent implementation (Keccak-256, RLP, the state, storage and receipt tries) against the roots of the record the call names, and its conditions evaluated. The program's own verdict is the block's: applied, or in the reverted list."),
    kv(rows));
}
