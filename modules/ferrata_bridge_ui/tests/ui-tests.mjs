// Basecamp's headless UI test of the mini-app (`nix run .#ui-tests`, logos-qt-mcp's framework): the
// module loads in Basecamp's standalone mode and every screen renders. The flows themselves are driven
// by qmltest/tst_miniapp.qml against the devnet backend (scripts/box-test.sh).
import { resolve } from "node:path";

const root = process.env.LOGOS_QT_MCP || new URL("../result-mcp", import.meta.url).pathname;
const { test, run } = await import(resolve(root, "test-framework/framework.mjs"));

const screens = [
  ["tab.deposit", ["Deposit: Ethereum to LEZ", "Private account (recommended)"]],
  ["tab.claim", ["Claim on LEZ", "Private claim (recommended)"]],
  ["tab.redeem", ["Burn on LEZ, release on Ethereum"]],
  ["tab.positions", ["Positions, recovered from the seed"]],
  ["tab.registry", ["Token registry: amounts, caps and utilisation"]],
  ["tab.admin", ["SetCap (admin)", "Freeze (freeze authority)"]],
  ["tab.privacy", ["What is public and what is private"]],
  ["tab.settings", ["Ethereum RPC", "LEZ sequencer or zone"]],
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

run();
