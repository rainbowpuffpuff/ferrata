// Program registry and app directory: Ferrata's programs with their image ids, binaries, sources and IDLs, each
// checked live against the loader header on this network; LEZ's built-in programs; the apps and tools; how to deploy.

import { shell, el, kv, table, panel, badge, accountLink, setTitle, href, loadIdl, copyButton } from "./ui.js";
import { hex, b58, fmtInt, shortHex, fromB58 } from "./util.js";
import { LOADER_IDL, PROGRAM_LOADER_ID, builtinProgramId, NATIVE_TOKEN_PROGRAM_ID } from "./lez.js";

const { main, config, rpc } = await shell("registry");
setTitle("Programs and apps");

const BUILTIN_TEXT = {
  "native token": [NATIVE_TOKEN_PROGRAM_ID, "balances and transfers of the zone's native token"],
  "program loader": [new Uint8Array(32).fill(0xfe), "deploys programs: segments of RISC Zero bytecode and a header naming the image"],
  clock: [builtinProgramId("clock"), "the block id and time, ticked by the sequencer as each block's last transaction"],
  fee: [builtinProgramId("fee"), "the fee market: base fees, the block's distribution to the producer"],
  bridge: [builtinProgramId("bridge"), "LEZ's native bridge from Logos Blockchain deposits (withdrawals disabled in this version)"],
  "bridge lock": [builtinProgramId("bridge_lock"), "locks for LEZ's native bridge"],
  "sequencer stake": [builtinProgramId("sequencer_stake"), "the sequencers' stake"],
  "cross-zone inbox": [builtinProgramId("cross_zone_inbox"), "messages arriving from other zones"],
  "cross-zone outbox": [builtinProgramId("cross_zone_outbox"), "messages leaving for other zones"],
  "wrapped token": [builtinProgramId("wrapped_token"), "LEZ's wrapped-token program"],
};

const progBox = el("div");
main.replaceChildren(
  el("h1", {}, "Programs and apps"),
  el("p", { class: "muted" }, "Every program on LEZ is a RISC Zero guest, named on chain by its image id: the hash of its exact binary. Ferrata's binaries build reproducibly (the same commit gives the same ids on any machine), so you can rebuild one and compare its id with the header on this network."),
  el("h2", {}, "Ferrata's programs"),
  progBox,
  builtinPanel(),
  appsPanel(),
  deployPanel(),
);

renderPrograms();

async function renderPrograms() {
  // every card at once with its static facts; each fills in its live check as its read returns
  const list = config.programs || [];
  const slots = list.map((p) => el("div", {}, panel(el("h3", {}, p.name), el("p", { class: "mono small" }, `0x${p.image_id}`), el("p", { class: "muted small" }, "checking the loader header…"))));
  progBox.replaceChildren(...slots);
  await Promise.all(list.map(async (p, i) => slots[i].replaceChildren(await programCard(p))));
}

async function programCard(p) {
  let live;
  if (!p.account) live = badge("not deployed on this network yet", "warn");
  else {
    try {
      const acct = fromB58(p.account);
      const view = await rpc.accountView(acct, PROGRAM_LOADER_ID);
      const st = view && view.states.find((s) => hex(s.program) === hex(PROGRAM_LOADER_ID));
      if (!st || !st.data.length) live = badge("no loader header at that account", "bad");
      else {
        const v = LOADER_IDL.decodeType(st.data, "LoaderEntry");
        const img = v.variant === "Header" ? hex(v.fields[0][1].v) : null;
        const imm = v.variant === "Header" ? v.fields[2][1].v : null;
        live = img === p.image_id ? el("span", {}, badge("deployed: header's image id equals the build", "ok"), imm ? badge("immutable", "info") : badge("mutable", "warn")) : badge(`header names another image (0x${shortHex(img || "?", 6)})`, "bad");
      }
    } catch (e) {
      live = badge(`could not read: ${e.message}`, "warn");
    }
  }
  let idlInfo = null;
  if (p.idl) {
    try {
      const idl = await loadIdl(p.idl);
      idlInfo = el("details", {}, el("summary", {}, `IDL: ${idl.json.instructions.length} instructions, ${idl.accountTypes.length} account type(s), ${idl.errors.size} error codes`),
        table(["#", "Instruction", "What it does"], idl.json.instructions.map((i) => [String(i.discriminator[0]), el("span", { class: "callname" }, i.name), el("span", { class: "wrap small" }, (i.docs || [""])[0])])));
    } catch {}
  }
  return panel(
    el("h3", {}, p.name, " ", p.rfp ? badge(p.rfp, "info") : null, " ", live),
    p.what ? el("p", {}, p.what) : null,
    kv([
      ["Account", p.account ? accountLink(fromB58(p.account), { short: false }) : el("span", { class: "muted" }, "—")],
      ["Image id", el("span", {}, el("span", { class: "mono small" }, `0x${p.image_id}`), copyButton(p.image_id))],
      p.binary_sha256 ? ["Binary", el("span", {}, el("span", { class: "mono small" }, `sha256 ${p.binary_sha256}`), p.binary_bytes ? ` · ${fmtInt(p.binary_bytes)} bytes` : "", p.crate ? [" · ", el("code", {}, p.crate)] : null)] : null,
      p.source ? ["Source", el("a", { href: p.source }, p.source.replace(/^https:\/\/github.com\//, ""))] : null,
      p.guide ? ["Guide", el("a", { href: p.guide }, p.guide.replace(/^.*\//, ""))] : null,
      p.idl ? ["IDL (SPEL format)", el("a", { href: href(p.idl) }, p.idl)] : null,
      ["Rebuild and compare", el("code", { class: "small" }, "scripts/reproduce-image-ids.sh build && scripts/reproduce-image-ids.sh compare")],
    ]),
    idlInfo,
  );
}

function builtinPanel() {
  const box = panel(el("h2", {}, "Built into LEZ"), el("p", { class: "muted small" }, "Programs every LEZ v0.4 zone carries; their account ids are fixed by name (SHA-256 of a prefix and the name)."));
  const rows = Object.entries(BUILTIN_TEXT).map(([name, [id, what]]) => [el("strong", {}, name), accountLink(id), el("span", { class: "wrap small" }, what)]);
  box.append(table(["Program", "Account", "What it does"], rows));
  const circuit = el("p", { class: "small muted" }, "privacy-preserving circuit: …");
  box.append(circuit);
  rpc.programIds().then((ids) => {
    const c = ids.privacy_preserving_circuit;
    circuit.replaceChildren("Privacy-preserving circuit image id (getProgramIds): ", el("span", { class: "mono" }, c ? `0x${c}` : "not reported"));
  }).catch(() => circuit.replaceChildren("getProgramIds did not answer"));
  return box;
}

function appsPanel() {
  const apps = [
    ["Explorer", href("explorer/"), "Blocks, transactions (private ones: their public parts), accounts and programs, decoded in your browser; every block hash recomputed."],
    ["Attestation status", href("status/"), "The light clients' head, lag, committee period and alarms, checked against public beacon APIs; attested facts re-verified in your browser."],
    config.wallet ? ["Web wallet", config.wallet, "A browser wallet for this test network: accounts, test tokens, transfers."] : null,
    config.faucet ? ["Faucet", config.faucet, "Test tokens for a new account."] : null,
    ["For AI agents", href("agents/"), "AGENTS.md: every RPC method with a request and a response, program ids, scripts in Python and JavaScript, the rules."],
    config.lez_explorer ? ["LEZ's own explorer", config.lez_explorer, "LEZ's Leptos explorer over its indexer (shows final blocks only)."] : null,
  ].filter(Boolean);
  const tools = [
    ["ferrata (CLI)", "https://github.com/rainbowpuffpuff/ferrata/tree/main/cli", "The bridge user's tool: keys, deposit, claim, burn, release, recover, status; attest against a running sequencer."],
    ["ferrata-updater", "https://github.com/rainbowpuffpuff/ferrata/tree/main/lez/updater", "The daemon that keeps an instance following Ethereum (any party may run it; it only carries data the program checks)."],
    ["Logos Core modules", "https://github.com/rainbowpuffpuff/ferrata/tree/main/modules", "Attestor, monitor, updater, relayer and the bridge mini-app for Logos Basecamp."],
    ["Web depositor", "https://github.com/rainbowpuffpuff/ferrata/tree/main/web/depositor", "A one-file page to make a bridge deposit from MetaMask or any EIP-1193 wallet, served from your own machine."],
    ["IDLs", href("idl/ferrata_attestation.idl.json"), "Both programs' interfaces in SPEL's IDL format; decode any message or account with them."],
  ];
  const card = ([name, url, text]) => el("a", { class: "card", href: url }, el("h3", {}, name), el("p", {}, text));
  return el("div", {}, el("h2", {}, "Apps on this network"), el("div", { class: "cards" }, apps.map(card)), el("h2", {}, "Tools"), el("div", { class: "cards" }, tools.map(card)));
}

function deployPanel() {
  const rpcUrl = config.public_rpc || "https://<this site>/rpc";
  return panel(
    el("h2", {}, "Deploy a program here"),
    el("p", {}, "LEZ's own wallet (", el("code", {}, "wallet"), ", built from the LEZ tag this network runs, ", el("code", {}, config.lez || "v0.4.0-rc3"), ") deploys any RISC Zero guest through the program loader: it splits the binary into segments of at most 96 KiB, writes one segment per account, then creates the header that names the image."),
    el("pre", { class: "code" }, [
      `# a wallet home for this network; set "sequencer": {"sequencer_addr": "${rpcUrl}"} in its wallet_config.json`,
      "export LEE_WALLET_HOME_DIR=$HOME/.lez-ferrata-testnet",
      "wallet account new public --label deployer          # fund it from the faucet",
      "wallet account new public --label my-program        # the header: the program's account id",
      "wallet account new public --label seg-0             # one account per 96 KiB segment of the binary",
      "wallet account new public --label seg-1",
      "wallet program-loader deploy --elf my_program.bin --header my-program --segments seg-0 seg-1 --immutable --payer deployer",
    ].join("\n")),
    el("p", { class: "small" }, "Then: your program's account id is the header's; its image id is in the header (open the account in the explorer). Publish an IDL so others can decode your messages, and build reproducibly so anyone can compare the image id. Ferrata's own deployment is scripted end to end (signed once, resumable, verified): ", el("a", { href: "https://github.com/rainbowpuffpuff/ferrata/blob/main/lez/testnet04/README.md" }, "lez/testnet04/README.md"), "."),
  );
}
