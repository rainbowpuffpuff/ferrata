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
// The claim and the burn are jobs of ferrata_bridge_module (calls/src/jobs.rs): the view's call starts the job and is
// answered at once, the view polls it; while a job proves, its picture is taken, the view's button is checked disabled, a
// second start of the same step is checked to answer the running job, and a call that would wait for the proof is checked
// refused at once. At the end every call of the view is checked answered within Basecamp's 20 s, none cut.
// BASECAMP_REOPEN=1: while the burn's job proves, the view is unloaded and loaded again through Basecamp's Apps
// Inspector (a fresh instance of the view), which must find the running job and complete the step.
//
// Environment: SEQNET_OPS (ops.sh), SEQNET_CHAIN_URL (seqnet-chain), BASECAMP_LEZ (lez_core | plain),
// BASECAMP_LEZ_PARAMS (the `lez` call's parameters: the user's LEZ wallet for lez_core), BASECAMP_WAIT_MS (a private
// transaction's longest wait), BASECAMP_REOPEN.
import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { fw, OUT, log, idOf, idsOf, evalIn, clickName, shot, waitFor, sidebarSection, writeJson, sleep } from "./lib.mjs";
const { test, run } = fw;

const V = "ferrata.root";                       // the view's root: expressions are evaluated in the view's context
const LEZ = process.env.BASECAMP_LEZ || "lez_core";
const CORE = LEZ === "lez_core" ? ["ferrata_bridge_module", "lez_core"] : ["ferrata_bridge_module"];
const WAIT = parseInt(process.env.BASECAMP_WAIT_MS || "180000", 10);
const REOPEN = process.env.BASECAMP_REOPEN === "1";
const HOST_CAP_MS = 20000;                      // Basecamp's cut of a view's call to a core module (results README, L4)
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
function chain(label, record = true) {
  const out = execFileSync("bash", ["-c", `${process.env.SEQNET_OPS} chain ${st.fresh}`], { encoding: "utf8", maxBuffer: 64 << 20 });
  const j = JSON.parse(out.trim().split("\n").pop());
  if (record) appendFileSync(`${OUT}/chain-checks.jsonl`, JSON.stringify({ t: new Date().toISOString(), label, ...j }) + "\n");
  st.chain[label] = j;
  return j;
}
// The step's job, once the view's call has started it: the module's record as the view keeps it (claimJob, burnJob).
async function jobOf(app, kind) {
  await waitFor(app, async () => { if ((await get(app, `${kind}Job !== null`)) !== true) throw new Error(`no ${kind} job: ${await get(app, "lastError")}`); }, 30000, `the ${kind}'s job`);
  return getJson(app, `${kind}Job`);
}
// While the step's job proves: the view's button is disabled, a second start of the same step (as a view that lost the
// reply would send it) answers the running job, a call that would wait for the proof is refused at once; the picture.
// In dev mode the proof takes about a second: the job may be over before it is looked at (logged).
async function jobInProgress(app, kind, again, shotName) {
  await waitFor(app, async () => { const s = await get(app, `${kind}Job ? ${kind}Job.state : ""`); if (s === "" || s === "queued") throw new Error(s); }, 120000, `the ${kind}'s job past queued`);
  const j = await getJson(app, `${kind}Job`);
  if (j.state !== "proving") { log({ job_in_progress: kind, id: j.id, state: j.state, note: "over or sent before it was looked at (dev mode proves in about a second)" }); return j; }
  check((await evalIn(app, `${kind}.submit`, "enabled")) === false && has(await evalIn(app, `${kind}.submit`, "text"), `Proof being made (job ${j.id})`), `while job ${j.id} proves, the view's ${kind} button is disabled and says so`);
  const t0 = Date.now();
  const second = await wallet(app, kind, again);
  check(second.ok === true && second.result.started === false && second.result.job.id === j.id, `a second ${kind} while job ${j.id} proves answers that job (started: false, ${Date.now() - t0} ms): no second transaction`);
  const t1 = Date.now();
  const scan = await wallet(app, "scan", {});
  const ms = Date.now() - t1;
  check(scan.ok === false && has(scan.error, `busy: job ${j.id}`) && ms < 5000, `a scan while job ${j.id} proves is refused at once (${ms} ms): it would wait for lez_core`);
  await sleep(4000);                            // a poll or two: the elapsed time moves on in the picture
  const k = await getJson(app, `${kind}Job`);
  log({ job_in_progress: kind, id: k.id, state: k.state, proving_ms: k.proving_ms, elapsed_ms: k.elapsed_ms, text: await evalIn(app, `${kind}.job`, "text") });
  await shot(app, shotName);
  return k;
}
async function jobDone(app, kind) {
  await waitFor(app, async () => { if ((await get(app, `jobOver(${kind}Job)`)) !== true) throw new Error(await get(app, `${kind}Job.state`)); }, WAIT, `the ${kind}'s job done`);
  const j = await getJson(app, `${kind}Job`);
  log({ job_done: kind, id: j.id, state: j.state, elapsed_ms: j.elapsed_ms, proving_ms: j.proving_ms, tx_hash: j.tx_hash, phases: j.phases.map((p) => p.state) });
  check(j.state === "settled", `the ${kind}'s job ${j.id} settled (${j.state}${j.error ? ": " + j.error : ""}) after ${j.elapsed_ms} ms, its proof ${j.proving_ms} ms`);
  return j;
}
// R2: the view unloaded and loaded again through Basecamp's Apps Inspector while the step's job proves; the fresh view
// must find the job (it lists the module's jobs when it opens) and complete the step from it.
async function reopenView(app, kind) {
  const before = await getJson(app, `${kind}Job`);
  writeJson("view-calls-before-reopen.json", await getJson(app, "callLog"));
  const toggle = "moduleRow.loadToggle.ferrata_bridge_ui";
  // The toggle shown: Basecamp lists the ui modules (apps) in its Apps Inspector and the core modules in its Module
  // Inspector, and an inspector page not shown may still hold rows (a click there lands on what is shown instead).
  const shown = async () => {
    for (const id of await idsOf(app, "objectName", toggle)) {
      const r = await app.inspector.send("evaluate", { objectId: id, expression: "visible ? text : ''" });
      if (!r.error && r.result) return { id, text: r.result };
    }
    return null;
  };
  const press = async (t) => {
    const r = await app.inspector.send("click", { objectId: t.id });
    if (r.error) throw new Error(`click ${toggle}: ${r.error}`);
    log({ click: toggle, was: t.text });
  };
  await clickName(app, await sidebarSection(app, "Settings"));
  let inspector = "", t = null;
  for (const section of ["Apps Inspector", "Module Inspector"]) {
    await app.click(section);
    await sleep(1000);
    t = await shown();
    if (t) { inspector = section; break; }
  }
  check(t !== null, `the view's row shown in Basecamp's ${inspector || "Apps or Module Inspector"}`);
  check(t.text === "Unload", `the view is loaded (${t.text})`);
  await press(t);
  await waitFor(app, async () => { const u = await shown(); if (!u || u.text !== "Load") throw new Error(u ? u.text : "no row"); }, 60000, "the view unloaded");
  await waitFor(app, async () => { const n = (await idsOf(app, "objectName", V)).length; if (n !== 0) throw new Error(`${n} instances`); }, 15000, "the view's instance destroyed");
  log({ view_unloaded: true, inspector, views_left: 0 });
  check(true, "the view is gone (no instance of it left in Basecamp's window)");
  await shot(app, `${kind === "burn" ? "17c" : "14c"}-view-unloaded-while-the-${kind}-is-proven`);
  await press(await shown());
  // Loaded again, Basecamp opens the app, and the inspector's row goes off screen: wait for the view's new instance (or
  // for the row to say Unload, then open the app from the sidebar).
  await waitFor(app, async () => {
    if ((await idsOf(app, "objectName", V)).length > 0) return;
    const u = await shown();
    if (u && u.text === "Unload") return;
    throw new Error(`no view yet (row: ${u ? u.text : "not shown"})`);
  }, 60000, "the view loaded again");
  log({ view_loaded_again: true, instances: (await idsOf(app, "objectName", V)).length });
  await clickName(app, "sidebar.app.ferrata_bridge_ui");
  await sleep(1000);
  await waitFor(app, async () => { const id = await get(app, `${kind}Job ? ${kind}Job.id : -1`); if (id !== before.id) throw new Error(`job ${id}`); }, 60000, "the reopened view found the job");
  const j = await getJson(app, `${kind}Job`);
  check((await get(app, "connectedAccount")) === "" && (await get(app, "callLog.length")) < 10, "a fresh instance of the view (not connected, its call log new)");
  check(j.id === before.id, `the reopened view found job ${j.id} (${j.state}, ${j.elapsed_ms} ms) from the module's listing`);
  log({ reopened: kind, job: j.id, state: j.state, elapsed_ms: j.elapsed_ms });
  const refused = (await getJson(app, "callLog")).filter((c) => !c.ok);
  check(refused.length === 0 && (await get(app, "lastError")) === "", `the reopened view's calls all answered, none refused (${refused.map((c) => c.method + ": " + c.error).join("; ").slice(0, 200)})`);
  // The user connects the reopened view again.
  await click(app, "wallet.connect");
  await waitFor(app, async () => { if ((await get(app, "selecting")) !== true) throw new Error("not selecting"); }, 10000, "account selection");
  await idle(app);
  await click(app, "wallet.account.0");
  await tab(app, kind === "burn" ? "redeem" : "claim");
  await sleep(4000);
  await shot(app, `${kind === "burn" ? "17d" : "14d"}-reopened-view-found-the-${kind}-job`);
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
  // The vault keeps the denomination and the deposit's fees; the user paid both and the gas.
  const inVault = BigInt(j.eth.vault_balance_wei) - BigInt(before.eth.vault_balance_wei), paid = BigInt(before.eth.user_balance_wei) - BigInt(j.eth.user_balance_wei);
  check(inVault >= word(d.data, 2) && paid > inVault, `the vault holds ${inVault} wei more (the Deposit log's ${word(d.data, 2)} and its fees); the user paid ${paid} wei with gas`);
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
  const started = await jobOf(app, "claim");
  log({ job_started: "claim", id: started.id, state: started.state });
  await jobInProgress(app, "claim", { leaf_index: st.leaf, private: true }, "14b-claim-job-proving");
  st.claimJob = await jobDone(app, "claim");
  await waitFor(app, async () => { if (!has(await get(app, "claimText"), "Claimed privately")) throw new Error(await get(app, "claimText + ' ' + lastError")); }, 30000, "the claim's reply applied");
  st.claim = await getJson(app, "claimReply");
  check(st.claim.settled && st.claim.private, "settled as a private transaction on the sequencer");
  check(hex(st.claim.tx_hash) === hex(st.claimJob.tx_hash), "the view's reply is the job's (its transaction)");
  await click(app, "claim.refresh");
  await idle(app);
  await waitFor(app, async () => { const s = await get(app, "positions.deposits[0].state"); if (s !== "claimed") throw new Error(s); }, 60000, "the deposit shows as claimed");
  const j = chain("after claim");
  const t = lezTx(j, st.claim.tx_hash);
  check(t && t.kind === "private" && !t.reverted, `the claim's transaction ${hex(t && t.tx).slice(0, 12)}… is in LEZ block ${t && t.block}, private, not reverted`);
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
  const before = chain("before burn");
  const delayMs = (await get(app, "releaseDelayHours")) * 3600 * 1000;
  await click(app, "burn.submit");
  await approve(app, "Burn", "17-burn-approval");
  const started = await jobOf(app, "burn");
  log({ job_started: "burn", id: started.id, state: started.state, release_delay_ms: started.params.release_delay_ms });
  check(started.params.release_delay_ms === delayMs, `the burn's job carries the view's random release delay (${delayMs} ms)`);
  await jobInProgress(app, "burn", { account: 0, amount: started.params.amount, recipient: st.fresh, private: true, release_delay_ms: delayMs }, "17b-burn-job-proving");
  if (REOPEN) {
    const j = await getJson(app, "burnJob");
    log({ reopen: j.state === "proving" ? "while the burn's job proves" : `after the burn's job was ${j.state} (dev mode proves in about a second)` });
    await reopenView(app, "burn");
  }
  st.burnJob = await jobDone(app, "burn");
  await waitFor(app, async () => { if (!has(await get(app, "burnText"), "Burned privately")) throw new Error(await get(app, "burnText + ' ' + lastError")); }, 30000, "the burn's reply applied");
  st.burn = await getJson(app, "burnReply");
  check(st.burn.settled && st.burn.private && st.burn.balance === "0", "settled privately; the private holding is spent");
  check(hex(st.burn.recipient) === hex(st.fresh), "the note is for the fresh recipient");
  check(st.burn.release_delay_ms === delayMs, `the burn's reply (the job's result) carries its release delay (${st.burn.release_delay_ms} ms)`);
  check((await get(app, "releaseHeldUntil[0]")) === st.burnJob.finished_ms + (await get(app, "clockOffset")) + delayMs, "the release is held from the burn's settlement for that delay");
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
  const calls = await getJson(app, "callLog");
  writeJson("view-calls.json", calls);
  let all = calls;
  if (REOPEN) { try { all = JSON.parse(readFileSync(`${OUT}/view-calls-before-reopen.json`, "utf8")).concat(calls); } catch (e) { log({ note: `no record of the first view's calls: ${e.message}` }); } }
  const longest = all.reduce((m, c) => Math.max(m, c.ms), 0);
  const cutCalls = all.filter((c) => /Logos host|timed out/i.test(c.error));
  const polls = all.filter((c) => c.method === "job").length;
  log({ view_calls: all.length, polls, longest_ms: longest, cut: cutCalls.length });
  check(cutCalls.length === 0, `no call of the view cut by the host (${all.length} calls, ${polls} of them job polls)`);
  check(longest < HOST_CAP_MS, `every call of the view answered within Basecamp's ${HOST_CAP_MS} ms: the longest ${longest} ms`);
  const jobs = await wallet(app, "jobs", {});
  writeJson("jobs.json", jobs.result ?? jobs);
  if (LEZ === "lez_core") {
    const r = await wallet(app, "lez_report", {});
    writeJson("lez-report.json", r.result ?? r);
    check(r.ok === true, "lez_report");
  }
  writeJson("summary.json", { lez: LEZ, deposit: st.deposit, leaf: st.leaf, claim: st.claim, burn: st.burn, fresh: st.fresh,
    claim_job: st.claimJob, burn_job: st.burnJob, reopen: REOPEN });
});

run();
