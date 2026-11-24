// The status page's alarms (assets/attest.js `alarms`) on synthetic instance states: each rule fires when it should
// and only then. Times are relative to a fixed "now".
//
//   node deploy/testnet/web/explorer/test/alarms.test.mjs

import { alarms } from "../assets/attest.js";
import { beaconClock } from "../assets/ferrata.js";

const config = { chain_id: 11155111, slots_per_epoch: 32, epochs_per_sync_committee_period: 256, genesis_time: 1655733600, seconds_per_slot: 12, ws_period_epochs: 3532 };
const clock = beaconClock(config);
const NOW = Date.UTC(2026, 10, 23, 12, 0, 0);
const nowSlot = clock.slotAtMs(NOW);
let failures = 0;
const check = (cond, what, got) => {
  console.log(`${cond ? "ok  " : "FAIL"} ${what}${cond ? "" : ` -> ${JSON.stringify(got)}`}`);
  if (!cond) failures++;
};

function inst({ behind = 96n, next = true, active = true }) {
  const slot = nowSlot - behind;
  return {
    config, clock, chainId: 11155111, slot, record: { record: {} },
    state: { plain: { active }, store: { next: next ? {} : null, fork: "Gloas" } },
  };
}
const checks = (ref, { equal = true, execEqual = true } = {}) => ({ networkFinalized: ref, beacon: [{ host: "a", equal }], execution: [{ host: "e", equal: execEqual }] });
const levels = (a) => a.map((x) => x.level).join(",");
const has = (a, level, re) => a.some((x) => x.level === level && re.test(x.text));

let s = inst({ behind: 96n });
let a = alarms({}, s, checks(nowSlot - 64n), NOW);
check(levels(a) === "ok", "an instance 1 epoch behind Ethereum's finality: healthy", a);

a = alarms({}, s, checks(nowSlot - 64n + 200n), NOW);
check(a.every((x) => x.level !== "bad"), "a provider ahead of the wall clock is no alarm", a);

s = inst({ behind: 64n + 32n * 10n });
a = alarms({}, s, checks(nowSlot - 64n), NOW);
check(has(a, "warn", /10 epochs behind/), "10 epochs behind: warning", a);

s = inst({ behind: 64n + 32n * 100n });
a = alarms({}, s, checks(nowSlot - 64n), NOW);
check(has(a, "bad", /100 epochs behind/), "100 epochs behind: alarm", a);

s = inst({ behind: 96n });
a = alarms({}, s, checks(nowSlot - 64n, { equal: false }), NOW);
check(has(a, "bad", /another block root/), "a provider serving another root at our slot: alarm", a);

a = alarms({}, s, checks(nowSlot - 64n, { execEqual: false }), NOW);
check(has(a, "bad", /another execution block/), "an execution RPC serving another block: alarm", a);

a = alarms({}, s, { networkFinalized: null, beacon: [{ host: "a", error: "down" }], execution: [] }, NOW);
check(has(a, "warn", /No public beacon API answered/) && !has(a, "bad", /epochs behind/), "no provider: lag from the wall clock, a warning only", a);

a = alarms({}, s, checks(nowSlot - 32n * 200n), NOW);
check(has(a, "warn", /providers are behind/), "providers 200 epochs behind the wall clock: warning", a);

// next committee unknown near / past the period end
const periodEnd = clock.periodEndSlot(nowSlot);
s = { ...inst({ next: false }), slot: periodEnd - 1n - 32n }; // header in the period ending in ~6.4 min
a = alarms({}, s, checks(s.slot + 32n), clock.timeOfSlotMs(periodEnd) - 6.4 * 60000);
check(has(a, "warn", /next sync committee is unknown and the period ends/), "next committee unknown, period ends soon: warning", a);
a = alarms({}, s, checks(s.slot + 32n), clock.timeOfSlotMs(periodEnd) + 60000);
check(has(a, "bad", /period has ended/), "next committee unknown after the period end: alarm", a);

// weak subjectivity
s = inst({ behind: 3532n * 32n + 100n });
a = alarms({}, s, checks(nowSlot - 64n), NOW);
check(has(a, "bad", /weak-subjectivity period:/) || has(a, "bad", /older than the weak-subjectivity/), "header older than the weak-subjectivity period: alarm", a);
s = inst({ behind: 3532n * 32n - 600n });
a = alarms({}, s, checks(nowSlot - 64n), NOW);
check(has(a, "warn", /weak-subjectivity period of the finalized header ends in/), "weak-subjectivity end within a day: warning", a);

s = inst({ active: false });
a = alarms({}, s, checks(nowSlot - 64n), NOW);
check(has(a, "warn", /Not yet active/), "inactive instance: warning", a);

check(has(alarms({}, { missing: true }, null, NOW), "bad", /no state/), "missing instance account: alarm");

if (failures) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log("all checks passed");
