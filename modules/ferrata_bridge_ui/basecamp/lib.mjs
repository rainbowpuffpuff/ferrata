// Helpers for driving the mini-app inside Logos Basecamp through Basecamp's QML inspector (logos-qt-mcp's test
// framework, the one `nix run .#ui-tests` uses). Every click is a real mouse event posted to the QQuickWidget at the
// item's centre (the inspector's `click`); reads go through the inspector's `evaluate` in the item's QML context;
// pictures are the inspector's `screenshot` of Basecamp's main window (its root widget, grabbed after the software
// Qt Quick frames are rendered). Records go to $BASECAMP_OUT (events.jsonl, shots/).
import { resolve } from "node:path";
import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";

export const fw = await import(resolve(process.env.LOGOS_QT_MCP, "test-framework/framework.mjs"));
export const OUT = process.env.BASECAMP_OUT || ".";
mkdirSync(`${OUT}/shots`, { recursive: true });

export function log(o) {
  appendFileSync(`${OUT}/events.jsonl`, JSON.stringify({ t: new Date().toISOString(), ...o }) + "\n");
}
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function idsOf(app, property, value) {
  const r = await app.findByProperty(property, value);
  if (r.error) throw new Error(`findByProperty ${property}=${value}: ${r.error}`);
  return (r.matches ?? []).map((m) => m.id);
}
export async function idOf(app, name) {
  const id = (await idsOf(app, "objectName", name))[0];
  if (!id) throw new Error(`no object named ${name}`);
  return id;
}
// The value of a JS expression in the QML context of the object named `name` (primitives; JSON.stringify objects).
export async function evalIn(app, name, expression) {
  const r = await app.inspector.send("evaluate", { objectId: await idOf(app, name), expression });
  if (r.error) throw new Error(`${name}: ${expression.slice(0, 120)}: ${r.error}`);
  return r.result;
}
export async function clickName(app, name) {
  const r = await app.inspector.send("click", { objectId: await idOf(app, name) });
  if (r.error) throw new Error(`click ${name}: ${r.error}`);
  log({ click: name });
  await sleep(150);
  return r;
}
export async function shot(app, name) {
  const r = await app.screenshot();
  if (r.error || !r.image) throw new Error(`screenshot ${name}: ${r.error ?? "no image"}`);
  writeFileSync(`${OUT}/shots/${name}.png`, Buffer.from(r.image, "base64"));
  log({ shot: name, width: r.width, height: r.height });
}
export async function waitFor(app, fn, timeout, description) {
  await app.waitFor(fn, { timeout, interval: 500, description });
}
// Basecamp's sidebar: the section button whose text is `label` (objectName sidebar.section.<n>).
export async function sidebarSection(app, label) {
  for (let n = 0; n < 8; n++) {
    const ids = await idsOf(app, "objectName", `sidebar.section.${n}`);
    for (const id of ids) {
      const r = await app.inspector.send("evaluate", { objectId: id, expression: "text" });
      if (!r.error && r.result === label) return `sidebar.section.${n}`;
    }
  }
  throw new Error(`no sidebar section labelled ${label}`);
}
export function writeJson(name, value) {
  writeFileSync(`${OUT}/${name}`, JSON.stringify(value, null, 2) + "\n");
}
