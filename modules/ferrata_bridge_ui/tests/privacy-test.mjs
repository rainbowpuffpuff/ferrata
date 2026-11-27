// The view's leak indicator (qml/js/privacy.js) under Node, as QML's JavaScript engine runs it (a `.pragma library` file),
// and the defaults Main.qml gives it (RFP-022 U.05, Priv.7): no relayer pays a claim's LEZ fee on any of the module's
// paths, so the indicator must say who does (the user's own private account, the fee-payer correlation, "Elevated") and
// never read "Minimal" for it; with a relayer that pays (a path that has one) the same choices read "Minimal".
//   node modules/ferrata_bridge_ui/tests/privacy-test.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("../qml/js/privacy.js", import.meta.url), "utf8").replace(/^\.pragma library\s*$/m, "");
const P = vm.createContext({});
vm.runInContext(src, P);
const qml = readFileSync(new URL("../qml/Main.qml", import.meta.url), "utf8");
let failed = 0;
function ok(name, cond, detail) {
  if (cond) console.log(`ok   ${name}`);
  else { failed++; console.log(`FAIL ${name}${detail ? ": " + detail : ""}`); }
}
const defaults = { kind: "private", delay: true, delayHours: 3, account: 0, denominationText: "0.001 ETH" };

// The deposit screen on the view's defaults: the fee payer is the user's own private account (level 1).
const dep = P.leaks({ ...defaults, relayer: false });
const fee = dep.find((l) => /pays the claim's LEZ fee/.test(l.text));
ok("deposit: the fee-payer line names the user's own private account", fee && fee.level === 1 && /Your own private account/.test(fee.text), JSON.stringify(fee));
ok("deposit: no line says a relayer pays", !dep.some((l) => /relayer submits and pays/.test(l.text)));
ok("deposit: the indicator reads Elevated, not Minimal", /^Elevated/.test(P.exposure(dep)), P.exposure(dep));
// With a relayer that pays (a path that has one), the same choices read Minimal.
const depR = P.leaks({ ...defaults, relayer: true });
ok("deposit with a paying relayer: Minimal", /^Minimal/.test(P.exposure(depR)), P.exposure(depR));

// The claim screen: a private claim, held by its delay, no relayer: the fee-payer line is there too.
const claim = P.claimLeaks({ mode: "private", kind: "private", held: true, skipDelay: false, relayer: false });
ok("claim: the fee-payer line is shown", claim.some((l) => l.level === 1 && /Your own private account/.test(l.text)), JSON.stringify(claim));
ok("claim: Elevated", /^Elevated/.test(P.exposure(claim)), P.exposure(claim));
const claimR = P.claimLeaks({ mode: "private", kind: "private", held: true, skipDelay: false, relayer: true });
ok("claim with a paying relayer: Minimal", /^Minimal/.test(P.exposure(claimR)), P.exposure(claimR));

// Main.qml: the relayer flag off by default and its box disabled (nothing in the view turns it on), and the claim's
// approval names who pays.
ok("Main.qml: relayerOn false by default", /property bool relayerOn: false/.test(qml));
const box = qml.split("\n").find((l) => l.includes('objectName: "deposit.relayer"')) || "";
ok("Main.qml: the relayer box disabled and says the mini-app has none yet", /enabled: false/.test(box) && /not in the mini-app yet/.test(box), box.trim());
ok("Main.qml: nothing sets relayerOn true", !/relayerOn\s*=\s*(true|checked)/.test(qml));
ok("Main.qml: the claim's approval names the user's private account as the fee payer", qml.includes('"you: your private account pays the LEZ fee"'));
ok("Main.qml: the defaults note no longer lists a permissionless relayer among the defaults", !/random delay before the claim, a permissionless relayer/.test(qml));

console.log(failed ? `${failed} FAILED` : "all passed");
process.exit(failed ? 1 : 0);
