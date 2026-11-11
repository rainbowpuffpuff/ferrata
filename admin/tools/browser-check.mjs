// The admin view in a real browser: opens admin/index.html from file:// in headless Chromium
// (Playwright), types the endpoint, the vault and the from-block into the page's own form, presses
// its Read button, waits for the read to finish, and saves the page's rendered text and a screenshot.
//
//   NODE_PATH=<a node_modules with playwright> node admin/tools/browser-check.mjs RPC VAULT FROM OUT
//
// writes OUT.txt and OUT.jpg; exit status 1 if the page reports a fatal error or a script error.
// Playwright is not a dependency of the repository: install it anywhere (npm install playwright),
// with its Chromium (npx playwright install --with-deps chromium).
import { createRequire } from 'node:module';
import { pathToFileURL, fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const [rpc, vault, from, out] = process.argv.slice(2);
if (!out) {
  console.error('usage: node admin/tools/browser-check.mjs RPC VAULT FROM OUT');
  process.exit(2);
}
const pageUrl = pathToFileURL(path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'index.html')).href;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
const errors = [];
page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()); });
page.on('pageerror', (e) => errors.push('page error: ' + String(e)));
const requests = [];
page.on('request', (r) => requests.push(r.url()));
await page.goto(pageUrl);
await page.fill('#rpc', rpc);
await page.fill('#vault', vault);
await page.fill('#from', from);
const t0 = Date.now();
await page.click('#go');
// The page's Content-Security-Policy has no 'unsafe-eval', so Playwright's waitForFunction (a string
// evaluated in the page) is refused there: poll through locators, which run in an isolated world.
let status = '';
let fatal = '';
for (const deadline = Date.now() + 180000; Date.now() < deadline; await new Promise((r) => setTimeout(r, 250))) {
  status = ((await page.locator('#status').textContent()) || '').trim();
  fatal = ((await page.locator('#fatal').textContent()) || '').trim();
  if (/^read /.test(status) || fatal) break;
}
const text = await page.locator('#out').innerText();
const hosts = [...new Set(requests.map((u) => (u.startsWith('file:') ? 'file:' : new URL(u).origin)))];
const header = [
  `# admin/index.html in headless Chromium ${browser.version()} (Playwright), opened from file://`,
  `# endpoint ${rpc}, vault ${vault}, from block ${from}; read took ${((Date.now() - t0) / 1000).toFixed(1)} s`,
  `# page status: ${status}`,
  `# page fatal error: ${fatal || '(none)'}`,
  `# origins the page requested: ${hosts.join(', ')}`,
  `# script and console errors: ${errors.length}`,
  ...errors.map((e) => `#   ${e}`),
  '',
].join('\n');
fs.writeFileSync(out + '.txt', header + text + '\n');
await page.screenshot({ path: out + '.jpg', type: 'jpeg', quality: 35, fullPage: true });
console.log(`${status} | fatal: ${fatal || '(none)'} | errors: ${errors.length} | origins: ${hosts.join(', ')} | text: ${text.length} chars`);
await browser.close();
process.exit(fatal || errors.length ? 1 : 0);
