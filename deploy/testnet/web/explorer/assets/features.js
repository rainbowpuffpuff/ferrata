// Which of LEZ v0.4's features run on this network, each with evidence: a live link into this chain where one exists
// (computed from the blocks in view and the RPC), LEZ v0.4.0-rc3's source, and our own records.

import { el, badge, blockLink, txLink, accountLink, href } from "./ui.js";
import { hex, fmtInt, fmtAge, shortHex } from "./util.js";
import { rootOf, builtinProgramId, NATIVE_TOKEN_PROGRAM_ID, PROGRAM_LOADER_ID } from "./lez.js";

const LEZ = "https://github.com/logos-blockchain/logos-execution-zone/blob/v0.4.0-rc3/";
const src = (path, label) => el("a", { href: LEZ + path, rel: "noopener" }, label || path.replace(/^.*\//, "").replace(/#L\d+$/, ""));

/** The newest transaction in view whose root call goes to `program` (bytes) and satisfies `pred`. */
function newest(view, program, pred = () => true) {
  const p = hex(program);
  for (let i = view.length - 1; i >= 0; i--) {
    const b = view[i];
    for (let j = b.transactions.length - 1; j >= 0; j--) {
      const tx = b.transactions[j];
      const r = rootOf(tx);
      if (r && hex(r.to.program_account_id) === p && pred(tx, r)) return { tx, block: b, index: j };
    }
  }
  return null;
}

const evTx = (hit, text) => (hit ? el("span", {}, text, " ", txLink(hit.tx.hash), " in ", blockLink(Number(hit.block.header.block_id))) : el("span", { class: "muted" }, "none in the blocks in view"));

/**
 * ctx: {view (decoded blocks, oldest first), head, fee, ids (getProgramIds), deadLetters, status (host status.json),
 * config, programs (config.programs with accounts), keyOf (program bytes -> key)}.
 * Returns [{name, state: on|partial|off, what, live (node), sources: [nodes]}].
 */
export function features(ctx) {
  const { view, head, config } = ctx;
  const last = view[view.length - 1];
  const fin = [...view].reverse().find((b) => b.bedrock_status === "Finalized");
  const progByKey = (k) => (config.programs || []).find((p) => p.key === k && p.account);
  const repo = config.repo ? `${config.repo}/blob/main/` : null;
  const ours = (path, label) => (repo ? el("a", { href: repo + path, rel: "noopener" }, label) : null);
  const loader = newest(view, PROGRAM_LOADER_ID, (tx, r) => r.message[0] === 1);
  const msgProg = progByKey("msg");
  const namesProg = progByKey("names");
  const attProg = progByKey("attestation");
  const msgSend = msgProg ? newest(view, accountBytes(msgProg.account), (tx, r) => r.message[0] === 4) : null;
  const namesCall = namesProg ? newest(view, accountBytes(namesProg.account)) : null;
  const attest = attProg ? newest(view, accountBytes(attProg.account), (tx, r) => r.message[0] === 7) : null;
  const tick = newest(view, builtinProgramId("clock"));
  const feeTx = newest(view, builtinProgramId("fee"));
  const priv = view.flatMap((b) => b.transactions.map((tx) => ({ tx, block: b }))).filter((x) => x.tx.kind !== "Public");
  const deployed = (config.programs || []).filter((p) => p.account).length;
  const circuit = ctx.ids && ctx.ids.privacy_preserving_circuit;
  return [
    {
      name: "Blocks anchored on a Logos Blockchain node",
      state: "on",
      what: "Each LEZ block is inscribed into the zone's channel on this network's own Logos Blockchain node; a block's Bedrock status says whether its inscription is final there.",
      live: fin ? el("span", {}, "newest final block ", blockLink(Number(fin.header.block_id)), `, ${head - Number(fin.header.block_id)} block(s) behind the head`, ctx.status && ctx.status.bedrock && ctx.status.bedrock.height !== undefined ? `; node height ${fmtInt(ctx.status.bedrock.height)}` : "") : el("span", { class: "muted" }, "no finalized block in view"),
      sources: [src("lez/common/src/block.rs#L80", "BedrockStatus"), el("a", { href: href("status/") }, "operator view"), ours("docs/design/lez-v0.4-findings.md", "our findings §3")],
    },
    {
      name: "Clock",
      state: "on",
      what: "The sequencer ticks the clock as each block's last transaction; programs read the block id and time from it (names and messages bound their requests by it).",
      live: evTx(tick, "latest tick"),
      sources: [src("lez/programs/clock/core/src/lib.rs#L17", "clock")],
    },
    {
      name: "Fee market",
      state: "on",
      what: "Execution and storage base fees move with block fullness; every user transaction declares its gas limit and maximum fee; each block ends by distributing its fees.",
      live: el("span", {}, ctx.fee ? `base fees ${ctx.fee.base_fee_exec} / ${ctx.fee.base_fee_stor} at block #${fmtInt(ctx.fee.height)}; ` : "", evTx(feeTx, "latest distribution")),
      sources: [src("lez/programs/fee/core/src/lib.rs#L38", "fee program"), src("lee/state_machine/src/fees.rs", "fees")],
    },
    {
      name: "Program loader",
      state: "on",
      what: "RISC Zero programs deployed as segments and a header naming their image id; anyone may deploy.",
      live: el("span", {}, `${deployed} Ferrata program(s) deployed here (`, el("a", { href: href("registry/") }, "registry"), "); ", evTx(loader, "latest header")),
      sources: [src("lez/programs/program_loader/core/src/lib.rs#L28", "program loader")],
    },
    {
      name: "Message passing between programs",
      state: "on",
      what: "A call reaches other actors through messages: the names program forwards a signer's request to the name's account, the messages program delivers to the recipient's actor, the attestation program answers a consumer program.",
      live: msgSend || namesCall ? el("span", {}, msgSend ? evTx(msgSend, "a delivery (Send → the recipient's mailbox):") : evTx(namesCall, "a forwarded names call:")) : el("span", { class: "muted" }, "none in the blocks in view"),
      sources: [src("lee/state_machine/core/src/execution_state.rs#L123", "Delivery"), src("lee/state_machine/core/src/program/mod.rs#L280", "Publication")],
    },
    {
      name: "Program events",
      state: "partial",
      what: "Programs emit events (names: registered, renewed…; messages: delivered; the attestation program: each finalized header and attestation), but LEZ v0.4.0-rc3's sequencer RPC serves no events and no receipts: this explorer reads outcomes from the blocks and state instead.",
      live: el("span", { class: "muted" }, "emitted, not served by this version's RPC"),
      sources: [src("lee/state_machine/core/src/program/mod.rs#L730", "ProgramEvent"), src("lez/sequencer/service/rpc/src/lib.rs", "RPC methods")],
    },
    {
      name: "Transaction outcome",
      state: "partial",
      what: "No receipts: a block lists which of its charged transactions reverted. The explorer shows applied or refused from that list; exit codes are not on chain.",
      live: last ? el("span", {}, "reverted list of ", blockLink(Number(last.header.block_id)), `: ${last.reverted.length ? last.reverted.join(", ") : "empty"}`) : "—",
      sources: [src("lez/common/src/block.rs#L66", "BlockBody.reverted")],
    },
    {
      name: "Attested Ethereum state (Ferrata)",
      state: attProg ? "on" : "off",
      what: "Ethereum light clients verified by a LEZ program; facts about finalized Ethereum blocks attested against their records.",
      live: el("span", {}, attest ? evTx(attest, "latest attestation") : el("span", { class: "muted" }, "no attestation in view"), " · ", el("a", { href: href("status/") }, "status")),
      sources: [ours("lez/ATTEST-CLI.md", "attestation CLI"), ours("docs/guides/consumer-guide.md", "consumer guide")],
    },
    {
      name: "Privacy-preserving transactions",
      state: "on",
      what: "Private accounts and calls proven with the privacy circuit; the chain sees nullifiers, commitments and encrypted notes. Proving takes a GPU or minutes of CPU, so they are rare here.",
      live: el("span", {}, circuit ? `circuit 0x${shortHex(circuit, 6)} (getProgramIds); ` : "", priv.length ? evTx(priv[priv.length - 1], "latest private transaction") : el("span", { class: "muted" }, "none in the blocks in view")),
      sources: [src("lee/state_machine/src/privacy_preserving_transaction/transaction.rs", "privacy-preserving transaction")],
    },
    {
      name: "Validity windows",
      state: "on",
      what: "A transaction may bound the blocks or times it is valid in; names and messages requests carry their time and are refused outside a 10-minute window.",
      live: namesCall ? evTx(namesCall, "a time-bound names call") : el("span", { class: "muted" }, "none in view"),
      sources: [src("lee/state_machine/core/src/program/mod.rs#L591", "ValidityWindows")],
    },
    {
      name: "Deposits from the Logos Blockchain (LEZ bridge)",
      state: "partial",
      what: "Native tokens arrive through LEZ's bridge: at genesis on this network, or from a channel deposit on the Logos Blockchain; withdrawals are disabled in this LEZ version.",
      live: el("span", {}, "genesis allocations in ", blockLink(1)),
      sources: [src("lez/programs/bridge/core/src/lib.rs#L19", "bridge messages"), src("lez/programs/bridge/src/main.rs#L24", "withdrawals disabled")],
    },
    {
      name: "Cross-zone messages",
      state: "off",
      what: "Inbox and outbox programs for messages between zones are in the code; this network is one zone with no peer.",
      live: ctx.deadLetters ? el("span", {}, `dead letters: ${ctx.deadLetters.total_retired} retired, ${ctx.deadLetters.retained.length} retained`) : el("span", { class: "muted" }, "—"),
      sources: [src("lez/programs/cross_zone_inbox", "cross_zone_inbox"), src("lez/programs/cross_zone_outbox", "cross_zone_outbox")],
    },
    {
      name: "Sequencer staking and slashing",
      state: "partial",
      what: "The sequencer's stake is recorded at genesis; unstaking and slashing are in the code and not exercised here (a slash's reason is not checked in this version).",
      live: el("span", {}, "stake in ", blockLink(1)),
      sources: [src("lez/programs/sequencer_stake/core/src/lib.rs#L107", "Slash")],
    },
  ];
}

function accountBytes(s) {
  // base58 account id to bytes, via the util in a way that never throws on bad config
  try {
    return b58decodeSafe(s);
  } catch {
    return new Uint8Array(32);
  }
}
import { fromB58 as b58decodeSafe } from "./util.js";

export function featuresTable(list) {
  const state = { on: badge("on", "ok"), partial: badge("partly", "warn"), off: badge("in the code, not active", "") };
  return el("div", { class: "scroll" }, el("table", { class: "t features" },
    el("thead", {}, el("tr", {}, el("th", {}, "Feature"), el("th", {}, "Here"), el("th", {}, "Evidence on this chain"), el("th", {}, "Source"))),
    el("tbody", {}, list.map((f) => el("tr", {},
      el("td", { class: "wrap" }, el("strong", {}, f.name), el("div", { class: "muted small" }, f.what)),
      el("td", {}, state[f.state]),
      el("td", { class: "wrap small" }, f.live),
      el("td", { class: "wrap small" }, f.sources.filter(Boolean).flatMap((s, i) => (i ? [" · ", s] : [s]))))))));
}
