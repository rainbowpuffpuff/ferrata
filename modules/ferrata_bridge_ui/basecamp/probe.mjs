// The mini-app and its core modules in one Logos Basecamp (L1 of results-basecamp-roundtrip): Basecamp opens the view
// (standalone mode, --module), lists the core modules of its --modules-dir in its Module Inspector, and loads them
// there with its own Load buttons; the view then calls ferrata_bridge_module through the `logos` object Basecamp
// injects. No chain is running in this probe: the module answers that no wallet session is open, which shows the
// path and the reply's shape. Run by scripts/basecamp-build.sh's caller through `logos-basecamp-dev --test`.
import { fw, log, idsOf, idOf, evalIn, clickName, shot, waitFor, sidebarSection, writeJson, sleep } from "./lib.mjs";
const { test, run } = fw;
const CORE = (process.env.BASECAMP_CORE_MODULES || "ferrata_bridge_module").split(",").filter(Boolean);

test("the view opens in Basecamp", async (app) => {
  await waitFor(app, async () => { await app.expectTexts(["Ferrata Bridge"]); }, 30000, "the mini-app's view");
  log({ transport: await evalIn(app, "tab.deposit", "transport"), inLogos: await evalIn(app, "tab.deposit", "inLogos") });
  await shot(app, "p01-view-opened");
  writeJson("interactive-start.json", await app.listInteractive());
});

test("Basecamp's Module Inspector lists and loads the core modules", async (app) => {
  await clickName(app, await sidebarSection(app, "Settings"));
  await app.click("Module Inspector");
  await sleep(500);
  await shot(app, "p02-module-inspector");
  for (const m of CORE) {
    const toggle = `moduleRow.loadToggle.${m}`;
    await waitFor(app, async () => { await idOf(app, toggle); }, 15000, `${m} listed`);
    log({ module: m, before: await evalIn(app, toggle, "text") });
    if ((await evalIn(app, toggle, "text")) === "Load") await clickName(app, toggle);
    await waitFor(app, async () => {
      const t = await evalIn(app, toggle, "text");
      if (t !== "Unload") throw new Error(`${m}: ${t}`);
    }, 60000, `${m} loaded`);
    log({ module: m, after: "Unload (loaded)" });
  }
  await shot(app, "p03-core-modules-loaded");
  writeJson("interactive-modules.json", await app.listInteractive());
});

test("the view calls ferrata_bridge_module through Basecamp's logos object", async (app) => {
  await clickName(app, "sidebar.app.ferrata_bridge_ui");
  await sleep(500);
  const req = JSON.stringify({ method: "info", params: {} });
  const expr = `(function () { var raw = logos.callModule("ferrata_bridge_module", "wallet", [${JSON.stringify(req)}]);`
    + ` return JSON.stringify({ type: typeof raw, raw: raw }); })()`;
  const r = JSON.parse(await evalIn(app, "tab.deposit", expr));
  log({ call: "wallet info (sync callModule)", reply: r });
  const hasAsync = await evalIn(app, "tab.deposit", "typeof logos.callModuleAsync");
  log({ callModuleAsync: hasAsync });
  writeJson("call-info.json", { request: req, reply: r, callModuleAsync: hasAsync });
  // The module's own answer (no session yet), whatever the envelope: the call reached it.
  if (!String(r.raw).includes("wallet session")) throw new Error(`unexpected reply: ${String(r.raw).slice(0, 300)}`);
  await shot(app, "p04-view-after-call");
});

run();
