// The user's private round trip in the mini-app INSIDE Logos Basecamp (e2e/seqnet/hook-basecamp.sh; records in
// $BASECAMP_OUT). Basecamp (standalone mode) opens the view (Main.qml, unchanged in behaviour from the Qt Quick Test's
// run, e2e/seqnet's tst_miniapp_logos.qml) and loads the core modules with its Module Inspector's Load buttons; the
// view calls ferrata_bridge_module through the `logos` object Basecamp injects (its LogosQmlBridge). Every user step is
// a real click (the inspector posts mouse events to the view's QQuickWidget); the operator's and relayer's steps run
// between them (ops.sh, as the Qt Quick Test's seqnet.ops did); after each step the chains are read directly (ops.sh
// chain: cast on anvil, seqnet-txs on the sequencer) and checked against what the view shows; a picture of Basecamp's
// window is taken at each state. The view's random delays are waited out on the view's clock (the user waiting), the
// vault's on anvil's; none is overridden, no default is changed.
//
// Environment: SEQNET_OPS (ops.sh), SEQNET_CHAIN_URL (seqnet-chain), BASECAMP_LEZ (lez_core | plain),
// BASECAMP_LEZ_PARAMS (the `lez` call's parameters: the user's LEZ wallet for lez_core), BASECAMP_WAIT_MS (a private
// transaction's longest wait).
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { fw, OUT, log, idOf, evalIn, clickName, shot, waitFor, sidebarSection, writeJson, sleep } from "./lib.mjs";
const { test, run } = fw;

const V = "ferrata.root";                       // the view's root: expressions are evaluated in the view's context
const LEZ = process.env.BASECAMP_LEZ || "lez_core";
const CORE = LEZ === "lez_core" ? ["ferrata_bridge_module", "lez_core"] : ["ferrata_bridge_module"];
const WAIT = parseInt(process.env.BASECAMP_WAIT_MS || "180000", 10);
const st = { fresh: "", deposit: null, claim: null, burn: null, chain: {} };

// Fail fast: once a step fails the later ones are not run, so no later picture can show a stale state as a success.
let failed = "";
function step(name, fn) {
  test(name, async (app) => {
    if (failed) throw new Error(`not run: "${failed}" failed`);
    const t0 = Date.now();
    try { await fn(app); log({ step: name, ok: true, ms: Date.now() - t0 }); }
    catch (e) { failed = name; log({ step: name, ok: false, ms: Date.now() - t0, error: e.message }); throw e; }
  });
}
function check(cond, what) { if (!cond) throw new Error(`check failed: ${what}`); log({ check: what }); }
const has = (text, part) => String(text).indexOf(part) >= 0;
const hex = (h) => String(h || "").toLowerCase().replace(/^0x/, "");

function ops(args) {
  const t0 = Date.now();
  let out = "";
  try { out = execFileSync("bash", ["-c", `${process.env.SEQNET_OPS} ${args}`], { encoding: "utf8", maxBuffer: 64 << 20 }); }
  catch (e) { log({ ops: args, ms: Date.now() - t0, exit: e.status, out: String(e.stdout || "").slice(-2000) }); throw new Error(`ops ${args}: exit ${e.status}`); }
  log({ ops: args, ms: Date.now() - t0, exit: 0, out: out.slice(-1500) });
  return out;
}
// The chains' own state (not the view's, not the module's): ops.sh chain.
function chain(label) {
  const out = execFileSync("bash", ["-c", `${process.env.SEQNET_OPS} chain ${st.fresh}`], { encoding: "utf8", maxBuffer: 64 << 20 });
  const j = JSON.parse(out.trim().split("\n").pop());
  appendFileSync(`${OUT}/chain-checks.jsonl`, JSON.stringify({ t: new Date().toISOString(), label, ...j }) + "\n");
  st.chain[label] = j;
  return j;
}
const word = (data, i) => BigInt("0x" + hex(data).slice(64 * i, 64 * (i + 1)));
function lezTx(j, txHash) { return j.lez.txs.find((t) => hex(t.tx) === hex(txHash)); }

const get = (app, expr) => evalIn(app, V, expr);
const getJson = async (app, expr) => JSON.parse(await get(app, `JSON.stringify(${expr})`));
async function idle(app, ms = WAIT) {
  await waitFor(app, async () => { const b = await get(app, "busy"); if (b !== 0) throw new Error(`busy ${b}`); }, ms, "the view's calls to finish");
}
async function click(app, name) {
  await sleep(100);
  const vis = await evalIn(app, name, "visible && enabled");
  if (vis !== true) throw new Error(`${name} is not visible and enabled`);
  await clickName(app, name);
}
async function tab(app, name) { await click(app, `tab.${name}`); await sleep(250); }
async function approve(app, titlePart, shotName) {
  await waitFor(app, async () => { if ((await get(app, "approval !== null")) !== true) throw new Error("no prompt"); }, 30000, "the wallet's approval prompt");
  const title = await get(app, "approval.title");
  check(has(title, titlePart), `the approval prompt is for "${titlePart}" (${title})`);
  if (shotName) await shot(app, shotName);
  await click(app, "approval.approve");
  await idle(app);
}
async function picture(app, name) { await idle(app); await sleep(300); await shot(app, name); }
async function userWaits(app, untilExpr) { // the view's clock moves on past `until` (the user waits)
  const ms = await get(app, `(function () { var ms = Math.max(0, ${untilExpr} - now()) + 1000; clockOffset += ms; return ms; })()`);
  log({ waited_ms: ms, on: "the view's clock", until: untilExpr });
  return ms;
}
async function wallet(app, method, params) { // a call through the view's `logos` (Basecamp's bridge), synchronous
  const req = JSON.stringify({ method, params });
  const raw = await get(app, `logos.callModule("ferrata_bridge_module", "wallet", [${JSON.stringify(req)}])`);
  let r = raw; for (let i = 0; i < 2 && typeof r === "string"; i++) r = JSON.parse(r);
  log({ host_call: method, reply: JSON.stringify(r).slice(0, 2000) });
  return r;
}

step("Basecamp opens the mini-app", async (app) => {
  await waitFor(app, async () => { await app.expectTexts(["Ferrata Bridge"]); }, 60000, "the view");
  check((await get(app, "inLogos")) === true && (await get(app, "transport")) === "module", "inside Logos: the view's transport is the core module");
  check((await get(app, "typeof logos.callModuleAsync")) === "function", "Basecamp's bridge has callModuleAsync (the view calls asynchronously)");
  await shot(app, "01-basecamp-opens-the-mini-app");
});

step("Basecamp's Module Inspector loads the core modules", async (app) => {
  await clickName(app, await sidebarSection(app, "Settings"));
  await app.click("Module Inspector");
  await sleep(500);
  for (const m of CORE) {
    const toggle = `moduleRow.loadToggle.${m}`;
    await waitFor(app, async () => { await idOf(app, toggle); }, 20000, `${m} in the Module Inspector`);
    if ((await evalIn(app, toggle, "text")) === "Load") await clickName(app, toggle);
    await waitFor(app, async () => { const t = await evalIn(app, toggle, "text"); if (t !== "Unload") throw new Error(`${m}: ${t}`); }, 90000, `${m} loaded`);
    log({ loaded: m });
  }
  await sleep(1500);
  await shot(app, "02-module-inspector-core-modules-loaded");
  await clickName(app, "sidebar.app.ferrata_bridge_ui");
  await sleep(500);
});

step("the session: the module opened at the chains (the host's step)", async (app) => {
  // In Logos the wallet SDK and Logos Core's own modules would stand here; as in the Qt Quick Test's initTestCase the
  // test opens the module's wallet session, through the view's own `logos` object (Basecamp's bridge).
  const open = await wallet(app, "open", { chain: process.env.SEQNET_CHAIN_URL });
  check(open.ok === true, `wallet open: ${open.error ?? ""}`);
  let lez = null;
  if (LEZ === "lez_core") {
    lez = await wallet(app, "lez", JSON.parse(process.env.BASECAMP_LEZ_PARAMS));
    check(lez.ok === true, `wallet lez (the session's LEZ through lez_core): ${lez.error ?? ""}`);
  }
  writeJson("session.json", { open, lez });
});

step("connect: the account and both sides' balances", async (app) => {
  await shot(app, "03-start");
  check((await evalIn(app, "deposit.review", "enabled")) === false, "no deposit before the wallet is connected");
  await click(app, "wallet.connect");
  await waitFor(app, async () => { if ((await get(app, "selecting")) !== true) throw new Error("not selecting"); }, 10000, "account selection");
  await idle(app);
  const info = await getJson(app, "info");
  check(info && has(info.network, "LEZ"), `info from the module: ${String(info && info.network).slice(0, 120)}`);
  await picture(app, "04-wallet-account-selection");
  await click(app, "wallet.account.0");
  await waitFor(app, async () => { if (!(await get(app, "connectedAccount"))) throw new Error("not connected"); }, 10000, "connected");
  await click(app, "header.refresh");
  await idle(app);
  const j = chain("connected");
  const shown = await getJson(app, "info");
  check(hex(shown.user.depositor) === hex(j.eth.user), "the account the view shows is the chain's depositor");
  check(String(shown.user.eth_balance_wei) === String(j.eth.user_balance_wei), `the Ethereum balance the view shows equals anvil's (${j.eth.user_balance_wei} wei)`);
  log({ balances: await evalIn(app, "header.balances", "text") });
  await picture(app, "05-connected-account-and-balances");
});

step("registry before the operator's first import", async (app) => {
  await tab(app, "registry");
  await click(app, "registry.refresh");
  await idle(app);
  check(has(await evalIn(app, "registry.token.0", "text"), "ETH  registered"), "ETH registered on the vault");
  check((await evalIn(app, "registry.lez.0", "visible")) === false, "no LEZ side before the operator's first import");
  await picture(app, "06-registry-before-the-import");
});

step("deposit: private by default", async (app) => {
  await tab(app, "deposit");
  check((await get(app, "destKind")) === "private", "private destination pre-selected");
  check((await get(app, "delayOn && relayerOn")) === true, "the random delay and the relayer on by default");
  check(has(await evalIn(app, "deposit.leaks.summary", "text"), "Minimal"), "the leak indicator reads Minimal");
  const before = chain("before deposit");
  await picture(app, "07-deposit-defaults");
  await click(app, "deposit.review");
  await approve(app, "Deposit", "08-deposit-approval");
  await waitFor(app, async () => { if (!has(await get(app, "depositText"), "Deposited")) throw new Error(await get(app, "lastError")); }, 120000, "the deposit sent");
  st.deposit = await get(app, "depositText");
  check(has(st.deposit, "private account"), "deposited to a private account");
  await click(app, "header.refresh");
  await idle(app);
  const j = chain("after deposit");
  const deps = j.eth.logs.Deposit;
  check(deps.length === before.eth.logs.Deposit.length + 1, `one Deposit log more on the vault (${deps.length})`);
  const d = deps[deps.length - 1];
  check(hex(d.topics[0]).endsWith(hex(j.eth.user)), "the Deposit log names the user's account as depositor");
  check(BigInt(j.eth.vault_balance_wei) - BigInt(before.eth.vault_balance_wei) === word(d.data, 2), `the vault holds the deposited amount more (${word(d.data, 2)} wei)`);
  check(BigInt(j.eth.user_balance_wei) < BigInt(before.eth.user_balance_wei) - word(d.data, 2), "the user's balance fell by the amount and gas");
  st.leaf = Number(word(d.data, 1));
  await picture(app, "09-deposit-sent");
});

step("positions before the import (recovery from the seed)", async (app) => {
  await tab(app, "positions");
  await click(app, "positions.recover");
  await idle(app);
  const p = await getJson(app, "positions");
  check(p.deposits.length === 1 && p.deposits[0].state === "waiting for the root import", `the deposit found from the seed, waiting for the import (${p.deposits.length})`);
  check(p.deposits[0].leaf_index === st.leaf, `the recovered deposit is the vault's leaf ${st.leaf}`);
  await picture(app, "10-positions-before-the-import");
});

step("the operator's import and the gas drop (not the user's steps)", async (app) => {
  const before = chain("before import");
  ops("import");
  ops("fund");
  const j = chain("after import");
  const fresh = j.lez.txs.filter((t) => !before.lez.txs.some((b) => b.tx === t.tx));
  check(fresh.length > 0 && fresh.every((t) => !t.reverted), `the import's and the gas drop's LEZ transactions applied, none reverted (${fresh.length})`);
  await tab(app, "registry");
  await click(app, "registry.refresh");
  await idle(app);
  check((await evalIn(app, "registry.lez.0", "visible")) === true, "the LEZ side of the token is shown (token(cid, ETH), read by the module)");
  log({ registry_lez: await evalIn(app, "registry.lez.0", "text"), registry_eth: await evalIn(app, "registry.token.0", "text") });
  await picture(app, "11-registry-after-the-import-caps");
});

step("the private claim, once its random delay has passed", async (app) => {
  await tab(app, "positions");
  await click(app, "positions.recover");
  await idle(app);
  await waitFor(app, async () => { const s = await get(app, "positions.deposits[0].state"); if (s !== "claimable") throw new Error(s); }, 60000, "claimable");
  await tab(app, "claim");
  await click(app, "claim.pick.0");
  check((await get(app, "claimMode")) === "private", "the private claim pre-selected");
  check((await evalIn(app, "claim.submit", "enabled")) === false, "held by the deposit's random delay");
  check(has(await evalIn(app, "claim.leaks.summary", "text"), "Minimal"), "the leak indicator reads Minimal");
  await picture(app, "12-claim-held-by-its-delay");
  await userWaits(app, "heldUntil[0]");
  await waitFor(app, async () => { if ((await get(app, "held(0)")) !== false) throw new Error("held"); }, 10000, "the delay passed");
  check((await get(app, "skipDelay")) === false, "the delay was waited out, not overridden");
  await picture(app, "13-claim-delay-passed");
  const before = chain("before claim");
  await click(app, "claim.submit");
  await approve(app, "Private claim", "14-claim-approval");
  await waitFor(app, async () => { if (!has(await get(app, "claimText"), "Claimed privately")) throw new Error(await get(app, "claimText + ' ' + lastError")); }, WAIT, "the private claim");
  st.claim = await getJson(app, "claimReply");
  check(st.claim.settled && st.claim.private, "settled as a private transaction on the sequencer");
  await click(app, "claim.refresh");
  await idle(app);
  await waitFor(app, async () => { const s = await get(app, "positions.deposits[0].state"); if (s !== "claimed") throw new Error(s); }, 60000, "the deposit shows as claimed");
  const j = chain("after claim");
  const t = lezTx(j, st.claim.tx_hash);
  check(t && t.kind === "private" && !t.reverted, `the claim's transaction ${hex(st.claim.tx_hash).slice(0, 12)}… is in LEZ block ${t && t.block}, private, not reverted`);
  check(st.claim.block === undefined || Number(st.claim.block) === t.block, `the block the view names (${st.claim.block}) is the sequencer's (${t && t.block})`);
  check(j.lez.txs.length > before.lez.txs.length, "the zone has the claim's transaction more");
  await click(app, "header.refresh");
  await idle(app);
  await picture(app, "15-claim-settled-privately");
});

step("the private burn to a fresh address", async (app) => {
  await tab(app, "redeem");
  await click(app, "burn.kind.private");
  await click(app, "burn.denom.0");
  check((await get(app, "burnKind")) === "private", "the private holder (the default)");
  check((await get(app, "releaseDelayOn")) === true, "the release's random delay on by default");
  st.fresh = await get(app, "recipient");
  const dep = await get(app, "info.user.depositor");
  check(hex(st.fresh) !== hex(dep), `a fresh recipient generated (${st.fresh})`);
  check(has(await evalIn(app, "burn.leaks.summary", "text"), "Minimal"), "the leak indicator reads Minimal");
  await picture(app, "16-burn-private-defaults");
  await click(app, "burn.submit");
  await approve(app, "Burn", "17-burn-approval");
  await waitFor(app, async () => { if (!has(await get(app, "burnText"), "Burned privately")) throw new Error(await get(app, "burnText + ' ' + lastError")); }, WAIT, "the private burn");
  st.burn = await getJson(app, "burnReply");
  check(st.burn.settled && st.burn.private && st.burn.balance === "0", "settled privately; the private holding is spent");
  check(hex(st.burn.recipient) === hex(st.fresh), "the note is for the fresh recipient");
  const j = chain("after burn");
  const t = lezTx(j, st.burn.tx_hash);
  check(t && t.kind === "private" && !t.reverted && has(t.visible, "Burn"), `the burn's transaction is in LEZ block ${t && t.block}, private, not reverted, its Burn call visible to the checkpoint`);
  await picture(app, "18-burn-settled-privately");
});

step("the checkpoint (operator) and the release by a relayer", async (app) => {
  ops("checkpoint");
  const cp = chain("after checkpoint");
  check(cp.eth.logs.CheckpointFinalized.length >= 1, `a checkpoint finalized on the vault (${cp.eth.logs.CheckpointFinalized.length})`);
  await click(app, "redeem.refresh");
  await idle(app);
  check((await get(app, "positions.burns.length")) === 1, "the private burn found from the seed");
  await click(app, "release.pick.0");
  check((await evalIn(app, "release.submit", "enabled")) === false, "the release is held by its random delay");
  await picture(app, "19-release-held-by-its-delay");
  await userWaits(app, "releaseHeldUntil[0]");
  ops("pass-time 121 the vault release delay after finalizeAttested");
  await waitFor(app, async () => { if ((await get(app, "releaseHeld(0)")) !== false) throw new Error("held"); }, 10000, "the delay passed");
  check((await get(app, "releaseSkipDelay")) === false, "the delay was waited out, not overridden");
  await click(app, "release.submit");
  await approve(app, "Release", "20-release-approval");
  await waitFor(app, async () => { if (!has(await get(app, "releaseText"), "Released:")) throw new Error(await get(app, "releaseText + ' ' + lastError")); }, 600000, "the release");
  const text = await get(app, "releaseText");
  check(has(text.toLowerCase(), hex(st.fresh)), "paid to the burn's fresh recipient");
  const j = chain("after release");
  check(BigInt(j.eth.fresh.balance_wei) > 0n, `the fresh address holds ${j.eth.fresh.balance_wei} wei on anvil`);
  const rel = j.eth.logs.Release;
  check(rel.length === 1 && hex(rel[0].topics[2]).endsWith(hex(st.fresh)), "one Release log on the vault, to the fresh address");
  // The vault pays the recipient the amount less the relay fee and its own fees (FerrataVault.release).
  const paid = BigInt(j.eth.fresh.balance_wei), amount = word(rel[0].data, 0), relayFee = word(rel[0].data, 2);
  check(paid > 0n && paid <= amount - relayFee, `the fresh address got ${paid} of the Release log's ${amount} wei (relay fee ${relayFee})`);
  await picture(app, "21-released-to-the-fresh-address");
});

step("positions after the round trip", async (app) => {
  await tab(app, "positions");
  await click(app, "positions.recover");
  await idle(app);
  const s = await evalIn(app, "positions.summary", "text");
  check(has(s, "0 not released") && has(s, "0 not claimed"), `recovery from the seed: ${s}`);
  await click(app, "header.refresh");
  await idle(app);
  await picture(app, "22-positions-after-the-round-trip");
  await tab(app, "privacy");
  await picture(app, "23-privacy");
});

step("records: the view's calls, the module's LEZ path", async (app) => {
  writeJson("view-calls.json", await getJson(app, "callLog"));
  if (LEZ === "lez_core") {
    const r = await wallet(app, "lez_report", {});
    writeJson("lez-report.json", r.result ?? r);
    check(r.ok === true, "lez_report");
  }
  writeJson("summary.json", { lez: LEZ, deposit: st.deposit, leaf: st.leaf, claim: st.claim, burn: st.burn, fresh: st.fresh });
});

run();
