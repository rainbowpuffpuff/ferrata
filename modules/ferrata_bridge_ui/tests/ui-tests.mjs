// Basecamp's headless UI test of the mini-app (tabs are clicked by their labels: app.click matches text) (`nix run .#ui-tests`, logos-qt-mcp's framework): the
// module loads in Basecamp's standalone mode and every screen renders. The flows themselves are driven
// by qmltest/tst_miniapp.qml against the devnet backend (scripts/box-test.sh).
import { resolve } from "node:path";

const root = process.env.LOGOS_QT_MCP || new URL("../result-mcp", import.meta.url).pathname;
const { test, run } = await import(resolve(root, "test-framework/framework.mjs"));

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

// With the devnet backend running beside Basecamp (scripts/basecamp-ui-tests.sh starts it): the user
// switches the view to it in Settings, connects the wallet and makes a deposit through the approval prompt.
if (process.env.FERRATA_DEVNET === "1") {
  test("ferrata_bridge_ui: a private deposit through the devnet, inside Basecamp", async (app) => {
    await app.click("Endpoints");
    await app.click("devnet backend (HTTP)");
    await app.click("Apply and test");
    await app.waitFor(async () => { await app.expectTexts(["Connected: chain 1"]); },
      { timeout: 30000, interval: 500, description: "the devnet to answer" });
    await app.click("Deposit");
    await app.click("Connect wallet");
    await app.waitFor(async () => { await app.expectTexts(["Ferrata Bridge asks to connect"]); },
      { timeout: 15000, interval: 300, description: "the account selection" });
    await app.click("Ethereum 0x");
    await app.waitFor(async () => { await app.expectTexts(["Private account (recommended)", "Exposure: Minimal"]); },
      { timeout: 15000, interval: 300, description: "the deposit form with its defaults" });
    await app.click("Review deposit");
    await app.waitFor(async () => { await app.expectTexts(["Deposit to the Ferrata vault", "What it does"]); },
      { timeout: 15000, interval: 300, description: "the approval prompt" });
    await app.click("Approve");
    await app.waitFor(async () => { await app.expectTexts(["Deposited 0.1 ETH to a private account"]); },
      { timeout: 60000, interval: 500, description: "the deposit's transaction" });
  });
}

run();
