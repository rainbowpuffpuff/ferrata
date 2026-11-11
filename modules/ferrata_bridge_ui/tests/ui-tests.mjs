// Basecamp's headless UI test of the mini-app (tabs are clicked by their labels: app.click matches text) (`nix run .#ui-tests`, logos-qt-mcp's framework): the
// module loads in Basecamp's standalone mode and every screen renders. The flows themselves are driven
// by qmltest/tst_miniapp.qml against the devnet backend (scripts/box-test.sh).
import { resolve } from "node:path";

const root = process.env.LOGOS_QT_MCP || new URL("../result-mcp", import.meta.url).pathname;
const { test, run } = await import(resolve(root, "test-framework/framework.mjs"));

// One property of the object named `name`, through the QML inspector (primitives only), as Basecamp's own
// tests read them.
async function prop(app, name, expression) {
  const id = ((await app.findByProperty("objectName", name)).matches ?? [])[0]?.id;
  if (!id) throw new Error(`no object named ${name}`);
  const res = await app.inspector.send("evaluate", { objectId: id, expression });
  if (res.error) throw new Error(`${name}: ${expression}: ${res.error}`);
  return res.result;
}

const screens = [
  ["Deposit", ["Deposit: Ethereum to LEZ", "Private account (recommended)"]],
  ["Claim", ["Claim on LEZ", "Private claim (recommended)"]],
  ["Burn and release", ["Burn on LEZ, release on Ethereum"]],
  ["Positions", ["Positions, recovered from the seed"]],
  ["Registry", ["Token registry: amounts, caps and utilisation"]],
  ["Admin", ["SetCap (admin)", "Freeze (freeze authority)"]],
  ["Privacy", ["What is public and what is private"]],
  ["Endpoints", ["Ethereum RPC", "LEZ sequencer or zone"]],
];

test("ferrata_bridge_ui: loads in Basecamp", async (app) => {
  await app.waitFor(async () => { await app.expectTexts(["Ferrata Bridge", "Logos core: ferrata_bridge_module"]); },
    { timeout: 30000, interval: 500, description: "the mini-app to load" });
});

for (const [tab, texts] of screens) {
  test(`ferrata_bridge_ui: ${tab} renders`, async (app) => {
    await app.click(tab);
    await app.waitFor(async () => { await app.expectTexts(texts); },
      { timeout: 10000, interval: 300, description: `${tab}'s texts` });
  });
}

// Inside Basecamp a ui_qml module may not use the network: the sandbox refuses the view's HTTP call to the
// devnet backend (Basecamp logs "Blocked network POST ... sandboxed ui_qml modules may not use the
// network"), and the view says it was not switched. The flows run through ferrata_bridge_module in Logos,
// and against the devnet outside it (qmltest/tst_miniapp.qml).
test("ferrata_bridge_ui: the sandbox keeps the view off the network inside Basecamp", async (app) => {
  await app.click("Endpoints");
  if (await prop(app, "settings.sandboxNote", "visible") !== true) throw new Error("the sandbox note is not shown inside Logos");
  await app.click("devnet backend (HTTP)");
  await app.click("Apply and test");
  await app.waitFor(async () => {
    const s = await prop(app, "settings.status", "text");
    if (!String(s).startsWith("Not switched: Endpoint unreachable")) throw new Error(`status: ${s}`);
  }, { timeout: 30000, interval: 500, description: "the refused call to be reported" });
});

run();
