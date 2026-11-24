// What a transaction did, as one sentence: transfers (the faucet's included), names, encrypted messages (who, to whom,
// how large; never the content), light-client updates and attestations, the bridge, program deployments, LEZ's own
// system steps. Built from the root call decoded with the program's IDL; the outcome from the block's reverted list.

import { el, accountLink, badge, labels } from "./ui.js";
import { hex, b58, fmtInt, fmtNative, fmtBytes } from "./util.js";
import { rootOf, signerAccounts } from "./lez.js";
import { field, plain } from "./idl.js";

const v = (args, name) => {
  const a = args.find(([n]) => n === name);
  return a ? a[1] : undefined;
};
const acct = (x) => (x && x.v instanceof Uint8Array ? accountLink(x.v) : x ? String(plain(x)) : "?");
const opt = (x) => (x && x.k === "opt" ? x.v : x);

/** {kind, icon, parts, reverted} for the activity feed; `parts` are nodes. Null for the sequencer's own clock/fee. */
export async function activityOf(tx, block, index, programs, config) {
  const root = rootOf(tx);
  const reverted = block.reverted.includes(index);
  const signer = signerAccounts(tx)[0] || null;
  const who = signer ? accountLink(signer) : el("span", { class: "muted" }, "someone");
  if (tx.kind === "PrivacyPreserving") {
    const n = tx.execution.private_actions.length;
    const call = root ? await programs.decodeMessage(root.to.program_account_id, root.message) : null;
    return { kind: "private", icon: "🔒", reverted, parts: ["a private transaction", ` (${n} private action${n === 1 ? "" : "s"}`, call && call.info && call.info.name ? `, public call to ${call.info.name}` : "", ")"] };
  }
  if (!root) return null;
  const { info, decoded } = await programs.decodeMessage(root.to.program_account_id, root.message);
  const key = info && (info.key || info.name);
  const name = decoded ? decoded.ins.name : null;
  const args = decoded ? decoded.args : [];
  if (!tx.witnesses.length && (key === "clock" || key === "fee" || info.name === "clock" || info.name === "fee")) return null;
  const prog = accountLink(root.to.program_account_id);
  const native = config.native;
  const out = (kind, icon, parts) => ({ kind, icon, reverted, parts });

  if (info.name === "native token" && name === "transfer") {
    const isFaucet = signer && labels.get(signer) && labels.get(signer).name === "faucet";
    return out(isFaucet ? "faucet" : "transfer", isFaucet ? "🚰" : "→", [isFaucet ? el("strong", {}, "Faucet") : who, " sent ", el("strong", {}, fmtNative(v(args, "amount").v, native)), " to ", acct(v(args, "to"))]);
  }
  if (key === "names" || key === "ferrata_names") {
    const nm = v(args, "name");
    const nameText = nm ? (nm.k === "opt" ? (nm.v ? nm.v.v : null) : nm.v) : null;
    const nameLink = nameText ? el("a", { href: `#/name/${nameText}`, class: "mono" }, `@${nameText}`) : null;
    const said = {
      register: [" registered ", nameLink],
      commit: [" committed to a new name (the name stays hidden until the reveal)"],
      set_primary: nameText ? [" set its primary name to ", nameLink] : [" cleared its primary name"],
      renew: [" renewed ", nameLink],
      transfer: [" offered ", nameLink, " to ", acct(v(args, "to"))],
      accept: [" accepted ", nameLink],
      release: [" released ", nameLink],
      set_target: [" pointed ", nameLink, " at a new target"],
    }[name] || [` called names: ${name || "?"}`];
    return out("name", "@", [who, ...said]);
  }
  if (key === "msg" || key === "ferrata_msg") {
    if (name === "send") {
      const body = v(args, "body").v;
      return out("message", "✉", [who, " sent an encrypted message to ", acct({ v: v(args, "to").v }), el("span", { class: "muted" }, ` · ${fmtBytes(body.length)} envelope, content not shown`)]);
    }
    const said = {
      publish: () => {
        const k = v(args, "key");
        return [" published its messaging key", el("span", { class: "muted" }, ` (epoch ${plain(field(k, "epoch"))}${field(k, "mlkem768").v ? ", X25519 + ML-KEM-768" : ", X25519"})`)];
      },
      set_policy: () => [" set its inbox to ", el("strong", {}, v(args, "policy").variant === "ContactsOnly" ? "contacts only" : "open")],
      contacts: () => [` updated its contacts (+${v(args, "add").items.length} −${v(args, "remove").items.length})`],
      block: () => [` updated its block list (+${v(args, "add").items.length} −${v(args, "remove").items.length})`],
      delete: () => [` deleted ${v(args, "seqs").items.length} message(s) from its inbox`],
      clear: () => [" cleared its inbox"],
    }[name];
    return out("message", "✉", [who, ...(said ? said() : [` called messages: ${name || "?"}`])]);
  }
  if (key === "attestation") {
    if (name === "attest") {
      const req = v(args, "request");
      const accts = field(req, "accounts").items.map((a) => `0x${hex(field(a, "address").v).slice(0, 8)}…`);
      const slots = field(req, "accounts").items.reduce((n, a) => n + field(a, "slots").items.length, 0);
      const logs = field(req, "logs").items.length;
      const facts = [accts.length ? `${accts.length} account(s) ${accts.join(", ")}` : null, slots ? `${slots} storage slot(s)` : null, logs ? `${logs} log(s)` : null].filter(Boolean).join(", ");
      return out("attest", "⛓", [who, " attested Ethereum facts: ", facts || "relations", " under the record ", accountLink(root.to.account_id)]);
    }
    const step = { init: "created a light-client instance", open: "opened a light-client update", miller: "checked an update's sync-committee signature", adopt: `adopted part ${plain(v(args, "part") || { k: "int", v: 0 })} of a new sync committee`, finish: "finished an update: a new finalized Ethereum header and its record", apply: "applied a proven update" }[name];
    return out("lightclient", "◷", ["Light client: ", who, " ", step || name || "called the attestation program"]);
  }
  if (key === "bridge" || key === "bridge-v0") return out("bridge", "⇄", [who, " called the Ferrata bridge: ", el("strong", { class: "mono" }, name || "?")]);
  if (info.name === "bridge" && name === "deposit") return out("bridge", "⇄", ["LEZ bridge credited ", el("strong", {}, fmtNative(v(args, "amount").v, native)), " to ", acct(v(args, "recipient_id")), el("span", { class: "muted" }, " (a Logos Blockchain deposit; genesis allocations come this way)")]);
  if (info.name === "program loader") {
    if (name === "write_segment") return out("deploy", "⬆", [who, ` uploaded a program segment (${fmtBytes(v(args, "bytecode").v.length)})`]);
    if (name === "create_header" || name === "update_header") {
      const p = await programs.info(root.to.account_id);
      return out("deploy", "⬆", [who, name === "create_header" ? " deployed a program at " : " updated the program at ", accountLink(root.to.account_id), p.name ? el("span", { class: "muted" }, ` (${p.name})`) : ""]);
    }
  }
  if (info.name === "sequencer stake") return out("stake", "◆", [who, ` sequencer stake: ${name || "?"}`]);
  return out("call", "·", [who, " called ", prog, name ? [" ", el("span", { class: "mono" }, name)] : el("span", { class: "muted" }, ` (${root.message.length} B)`)]);
}

/** The activity line as a list item. */
export function activityItem(a, tx, block, fresh = false) {
  const t = Number(block.header.timestamp);
  return el("li", { class: `act act-${a.kind}${fresh ? " fresh" : ""}` },
    el("span", { class: "act-icon", "aria-hidden": "true" }, a.icon),
    el("span", { class: "act-text" }, a.parts, a.reverted ? [" ", badge("refused", "bad")] : null),
    el("span", { class: "act-meta muted small" }, el("a", { href: `#/tx/${hex(tx.hash)}` }, "tx"), " · ", el("a", { href: `#/block/${block.header.block_id}` }, `#${fmtInt(block.header.block_id)}`), " · ", el("span", { "data-ts": String(t) }, "")));
}
