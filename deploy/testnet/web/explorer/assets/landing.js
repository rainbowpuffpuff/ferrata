// The landing page: what this network is, its live state in one row, and where to go next.

import { shell, el, stat, setTitle, href, badge, copyButton } from "./ui.js";
import { fmtInt, fmtAge, shortHex } from "./util.js";
import { Programs } from "./programs.js";
import { readInstance, checkInstance, alarms, CHAIN_NAMES } from "./attest.js";

const { main, config, rpc } = await shell("home");
setTitle("");
const programs = new Programs(rpc, config);
const rpcHref = config.public_rpc || new URL(config.rpc || "/rpc", location.href).href;

const live = el("div", { class: "stats" }, stat("Head block", "…"), stat("Block interval", "…"), stat("Bedrock", "…"), stat("Ethereum light client", "…"));
const card = (title, url, text, extra) => el("a", { class: "card", href: url }, el("h3", {}, title, extra ? [" ", extra] : null), el("p", {}, text));
const deployed = (config.programs || []).filter((p) => p.account).length;

main.replaceChildren(
  el("section", { class: "hero" },
    el("h1", {}, "Ferrata's LEZ test network"),
    el("p", {}, `A public Logos Execution Zone (LEZ ${config.lez || "v0.4.0-rc3"}) network anchored to its own Logos Blockchain node, running Ferrata's programs: Ethereum light clients that attest Ethereum state on LEZ (RFP-021), and the private bridge for ETH and ERC-20s between Sepolia and LEZ (RFP-022).`),
    el("p", { class: "small" }, "JSON-RPC: ", el("code", {}, rpcHref), copyButton(rpcHref), " · test network only, tokens have no value")),
  live,
  el("div", { class: "cards" },
    card("Explorer", href("explorer/"), "Blocks, transactions and accounts, decoded in your browser; Ferrata's accounts read through their IDLs; every block hash recomputed."),
    card("Attestation status", href("status/"), "The light clients' head, finality lag, committee period and alarms, checked against Ethereum's public beacon APIs from your browser; attested facts re-verified."),
    card("Programs & apps", href("registry/"), `Ferrata's programs with their image ids, sources and IDLs${deployed ? `, ${deployed} deployed here` : ""}; LEZ's built-ins; how to deploy your own.`),
    config.wallet ? card("Wallet", config.wallet, "A web wallet for this network: make an account, get test tokens, send them.") : null,
    config.faucet ? card("Faucet", config.faucet, "Test tokens for a new account.") : null,
    card("For AI agents", href("agents/"), "AGENTS.md: the endpoint, every RPC method with a request and a response, program ids, how to attest a fact and use the bridge, scripts in Python and JavaScript."),
  ),
  el("h2", {}, "What this network shows"),
  el("div", { class: "grid two" },
    el("div", { class: "panel" }, el("h3", {}, "Anchored, and you can see it"), el("p", {}, "Every LEZ block is inscribed into this network's own Logos Blockchain node; each block's Bedrock status (pending, finalized) is shown beside it, and every block hash is recomputed in your browser from the block's bytes.")),
    el("div", { class: "panel" }, el("h3", {}, "Ethereum, verified on LEZ"), el("p", {}, "Light-client instances follow Ethereum's finality from sync-committee signatures verified by a LEZ program. Each finalized header becomes a record that anyone can compare with a public beacon API; the status page does it for you and raises alarms when an instance falls behind.")),
    el("div", { class: "panel" }, el("h3", {}, "Facts you can re-check"), el("p", {}, "An attestation proves storage slots, accounts and logs of a finalized Ethereum block against a record. The explorer verifies the same Merkle proofs again in your browser, with independent code, and shows whether its verdict agrees with the chain's.")),
    el("div", { class: "panel" }, el("h3", {}, "Reproducible and open"), el("p", {}, "Each program is named by its image id, the hash of a binary that builds the same everywhere; the IDLs, the sources and this site's code are public. Nothing here needs a key to read.")),
  ),
);

(async () => {
  try {
    const head = await rpc.lastBlockId();
    const blocks = await rpc.blockRange(Math.max(1, head - 20), head);
    const last = blocks[blocks.length - 1];
    const gaps = [];
    for (let i = 1; i < blocks.length; i++) if (blocks[i - 1].header.timestamp > 0n) gaps.push(Number(blocks[i].header.timestamp - blocks[i - 1].header.timestamp));
    const avg = gaps.length ? gaps.reduce((a, b) => a + b, 0) / gaps.length / 1000 : null;
    const fin = [...blocks].reverse().find((b) => b.bedrock_status === "Finalized");
    const cells = [
      stat("Head block", `#${fmtInt(head)}`, fmtAge(Number(last.header.timestamp))),
      stat("Block interval", avg ? `${avg.toFixed(1)} s` : "—", `last ${gaps.length} blocks`),
      stat("Bedrock", fin ? `#${fmtInt(fin.header.block_id)} finalized` : "pending", fin ? `${head - Number(fin.header.block_id)} block(s) behind the head` : "no finalized block in the last 20"),
    ];
    const inst = (config.instances || [])[0];
    if (inst) {
      const s = await readInstance(rpc, programs, inst);
      const c = s.slot !== undefined ? await checkInstance(config, s).catch(() => null) : null;
      const a = alarms(config, s, c);
      const level = a.some((x) => x.level === "bad") ? "bad" : a.some((x) => x.level === "warn") ? "warn" : "ok";
      const chain = CHAIN_NAMES[s.chainId] || `chain ${s.chainId}`;
      cells.push(stat("Ethereum light client", s.slot !== undefined ? `slot ${fmtInt(s.slot)}` : "—", el("span", {}, `${chain} · `, badge(level === "ok" ? "healthy" : level === "warn" ? "warning" : "alarm", level))));
    } else cells.push(stat("Ethereum light client", "—", "instances listed once created"));
    live.replaceChildren(...cells);
  } catch (e) {
    live.replaceChildren(stat("Network", "unreachable", e.message));
  }
})();
