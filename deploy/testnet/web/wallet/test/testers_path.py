#!/usr/bin/env python3
"""The testers' path through the pages as the host serves them, in headless Chromium, against a local LEZ v0.4.0-rc3
sequencer and anvil as "Sepolia" (run by `box.sh testers NAME`, which starts both and test/serve.py --site). It follows
what a visitor does: the build page's reviewer link to the testers' page, that page's link to the wallet, a new wallet,
the faucet, then the bridge's deposits through a stand-in browser wallet (EIP-1193, anvil's unlocked accounts; it can
decline a signature, or hold it open), checking each step against what the testers' page says it should show and
against the chains themselves; and the refusals before a signature (no Sepolia ETH; the deposit covered but not its
gas) and the lock while a deposit waits for its signature.

Not rehearsed here, and why: the claim (step 6b) needs the attestation and bridge programs deployed on the local zone
with a light-client record covering anvil's blocks, which this harness does not set up (EVIDENCE rows 244 and 250 record
it on the test network; e2e/rc3-roundtrip/ rehearses the `ferrata` tool's claim on rc3); the private claim (step 6e)
needs the tool, a pager and a real proof.

Environment: WALLET_URL (the origin: /, /testers/, /wallet/, /rpc, /faucet), WALLET_RPC, WALLET_ANVIL, WALLET_VAULT,
WALLET_DEPOSITOR, WALLET_OUT (the record), WALLET_SHOTS (screenshots). Exit 0 when every check passed.
"""
import json
import os
import re
from decimal import Decimal
import sys
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

ORIGIN = os.environ["WALLET_URL"].rstrip("/") + "/"
RPC = os.environ["WALLET_RPC"]
ANVIL = os.environ.get("WALLET_ANVIL") or None
VAULT = os.environ.get("WALLET_VAULT") or None
DEPOSITOR = (os.environ.get("WALLET_DEPOSITOR") or "").lower()
OUT = os.environ["WALLET_OUT"]
SHOTS = os.environ["WALLET_SHOTS"]
PASS = "correct horse battery staple"
checks, notes = [], {}
requests_seen, errors = [], []


def check(name, ok, detail=""):
    checks.append({"check": name, "ok": bool(ok), "detail": str(detail)[:300]})
    print(("PASS " if ok else "FAIL ") + name + (f" -- {str(detail)[:200]}" if detail else ""), flush=True)
    return ok


def post(url, method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    with urllib.request.urlopen(urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"}), timeout=30) as r:
        return json.loads(r.read())


def lez(method, params=None):
    j = post(RPC, method, params or [])
    if "error" in j:
        raise RuntimeError(f"{method}: {j['error']}")
    return j["result"]


def eth(method, params=None):
    return post(ANVIL, method, params or [])


def text(page, sel):
    return page.locator(sel).inner_text().strip()


def shot(page, name):
    page.screenshot(path=os.path.join(SHOTS, name + ".png"), full_page=True)


def deposits_of(addr):
    logs = eth("eth_getLogs", [{"address": VAULT, "fromBlock": "0x0", "toBlock": "latest"}]).get("result", [])
    return [log for log in logs if len(log["topics"]) == 3 and log["topics"][1][-40:] == addr[2:]]


# The stand-in browser wallet: anvil's unlocked accounts; window.__account picks one, window.__reject makes it decline
# the next transaction as MetaMask does when its user presses Reject (EIP-1193 code 4001).
STAND_IN = """
window.__chain = '0xaa36a7'; window.__account = DEPOSITOR; window.__reject = false; window.__hold = false;
window.ethereum = { isStandIn: true, async request({ method, params }) {
  if (method === 'eth_requestAccounts' || method === 'eth_accounts') return [window.__account];
  if (method === 'eth_chainId') return window.__chain;
  if (method === 'eth_sendTransaction' && window.__hold) {   // the signature screen left open until the test answers
    window.__held = true; await new Promise((r) => { window.__release = r; }); window.__held = false; }
  if (method === 'eth_sendTransaction' && window.__reject) {
    window.__reject = false; const e = new Error('MetaMask Tx Signature: User denied transaction signature.'); e.code = 4001; throw e; }
  const r = await window.__walletRpc(method, params || []);
  if (r.error) { const e = new Error(r.error.message); e.code = r.error.code; e.data = r.error.data; throw e; }
  return r.result;
} };
"""


def wait_out(page, pattern, timeout=120000):
    page.wait_for_function(
        "(p) => new RegExp(p).test(document.querySelector('[data-testid=bridge-out]').textContent)", arg=pattern, timeout=timeout)
    return text(page, "[data-testid=bridge-out]")


def review(page):
    page.click("[data-testid=bridge-review]")
    page.wait_for_function("() => /Commitment C/.test(document.querySelector('[data-testid=bridge-summary]').textContent) "
                           "|| document.querySelector('[data-testid=bridge-out]').classList.contains('bad')", timeout=60000)
    return text(page, "[data-testid=bridge-summary]"), text(page, "[data-testid=bridge-out]")


def steps():
    with sync_playwright() as pw:
        browser = pw.chromium.launch()
        ctx = browser.new_context(viewport={"width": 1280, "height": 900})
        ctx.expose_function("__walletRpc", lambda method, params: eth(method, params))
        ctx.add_init_script(STAND_IN.replace("DEPOSITOR", json.dumps(DEPOSITOR)))
        page = ctx.new_page()
        page.on("request", lambda req: requests_seen.append(req.url))
        page.on("pageerror", lambda e: errors.append(str(e)))

        # -- 1. The build page: a reviewer's four links, the third to the testers' page
        page.goto(ORIGIN)
        links = page.locator(".hero .paths a.path")
        keys = [links.nth(i).get_attribute("data-path") for i in range(links.count())]
        check("the build page opens with a reviewer's four links", keys == ["requirements", "verify", "testers", "runs"], keys)
        for key, anchor in (("requirements", "#requirements"), ("runs", "#runs")):
            check(f"the reviewer link '{key}' names a section of the page", page.locator(anchor).count() == 1, anchor)
        shot(page, "01-build-page")
        page.click(".hero .paths a[data-path=testers]")
        page.wait_for_load_state("load")
        check("one click leads to the testers' page", page.url == ORIGIN + "testers/", page.url)
        check("the testers' page lists seven steps, the bridge's with five, the private claim among them",
              page.locator("ol.t-steps > li").count() == 7 and page.locator("#step-bridge ol.t-sub > li").count() == 5
              and page.locator("#step-private").count() == 1)
        shot(page, "02-testers")

        # -- 2. Step 1: its link to the wallet, then a new wallet
        page.click("#step-wallet a[href='/wallet/']")
        page.wait_for_selector("#v-welcome", timeout=30000)
        check("step 1's link opens the wallet", page.url == ORIGIN + "wallet/", page.url)
        page.click("#go-create")
        words = [w.strip() for w in page.locator("#mnemonic-words li").all_text_contents()]
        page.fill("#create-pass", PASS)
        page.fill("#create-pass2", PASS)
        page.check("#saved-words")
        page.click("#create-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        id1 = text(page, "#home-id")
        try:  # "—" until the first read of the account answers
            page.wait_for_function("() => document.getElementById('balance').textContent.startsWith('0 ')", timeout=30000)
        except Exception:  # noqa: BLE001  the check below records what it shows
            pass
        check("step 1: Home with the account id and a public balance of 0 (as the page says)",
              len(words) == 24 and re.fullmatch("[1-9A-HJ-NP-Za-km-z]{43,44}", id1) and text(page, "#balance").startswith("0 "),
              text(page, "#balance"))

        # -- 3. Step 2: the faucet
        page.click("#faucet")
        page.wait_for_function("() => /Received|failed|not arrived|HTTP|error/i.test(document.getElementById('faucet-msg').textContent)", timeout=200000)
        got = int(lez("getAccountBalance", [id1]))
        check("step 2: \"Received: your balance is N LGO\", and the RPC agrees", text(page, "#faucet-msg").startswith("Received") and got > 0,
              f"{text(page, '#faucet-msg')} / RPC {got}")
        shot(page, "03-funded")

        if not (ANVIL and VAULT):
            check("the bridge steps ran (anvil and the vault are needed)", False, "no WALLET_ANVIL / WALLET_VAULT")
            return
        # -- 4. Step 6a: the deposit's review, a refusal in the wallet, then the deposit
        page.goto(ORIGIN + "wallet/#bridge")
        page.click("[data-testid=bridge-connect]")
        page.wait_for_function("() => /Connected/.test(document.querySelector('[data-testid=bridge-status]').textContent) || document.querySelector('[data-testid=bridge-out]').textContent", timeout=30000)
        status = text(page, "[data-testid=bridge-status]")
        m = re.search(r"Connected (0x[0-9a-fA-F]{40})", status)
        check("the connected address is shown EIP-55 checksummed", m and m.group(1).lower() == DEPOSITOR and m.group(1) != DEPOSITOR, status)
        page.select_option("[data-testid=bridge-kind]", "public")
        summary, out = review(page)
        gas = re.search(r"Gas\s*about ([0-9.]+) ETH \(([0-9,]+) gas at ([0-9.]+) gwei, read now", summary)
        price = int(eth("eth_gasPrice")["result"], 16)
        check("the review prices the gas now (an estimate from anvil, at its gas price)",
              gas and int(gas.group(2).replace(",", "")) > 21000 and abs(float(gas.group(3)) - price / 1e9) < 0.01 + price / 1e11, summary[:400])
        check("the review shows this address's Sepolia ETH and the vault's address checksummed",
              re.search(r"Your Sepolia ETH\s*[0-9.]+ ETH", summary) and re.search(r"Vault\s*0x[0-9a-fA-F]{40}", summary)
              and VAULT.lower() in summary.lower() and VAULT.lower() not in summary, summary[:400])
        shot(page, "04-review")
        before = len(deposits_of(DEPOSITOR))
        page.evaluate("() => { window.__reject = true; }")
        page.click("[data-testid=bridge-go]")
        res = wait_out(page, "declined|Deposited|rror")
        check("declined in the browser wallet: said in plain words beside the button, nothing deposited, the button usable again",
              res == "You declined it in your browser wallet, so it was not sent." and len(deposits_of(DEPOSITOR)) == before
              and page.locator("[data-testid=bridge-go]").is_enabled(), res)
        page.click("[data-testid=bridge-go]")
        res = wait_out(page, "Deposited|failed|rror|changed")
        check("step 6a: \"Deposited: leaf N in block M\", and anvil's vault logged it", res.startswith("Deposited: leaf 0 in block ")
              and len(deposits_of(DEPOSITOR)) == before + 1, res)

        # -- 5. The private destination: what it needs, and the link to the testers' step 6e
        page.select_option("[data-testid=bridge-kind]", "private")
        note = page.locator("[data-testid=bridge-kind-note]")
        href = note.locator("a").get_attribute("href") if note.locator("a").count() else None
        check("choosing the private account says what its claim needs and links the testers' step 6e",
              note.is_visible() and "16 threads" in note.inner_text() and href == "/testers/#step-private", note.inner_text())
        tp = ctx.new_page()
        tp.goto(urllib.parse.urljoin(ORIGIN, href or "/"))
        try:  # the page scrolls to its anchor smoothly (site.css)
            tp.wait_for_function("() => { const r = document.getElementById('step-private').getBoundingClientRect(); "
                                 "return r.top >= 0 && r.top < innerHeight; }", timeout=5000)
        except Exception:  # noqa: BLE001  the check below records where it is
            pass
        box = tp.locator("#step-private").bounding_box()
        check("that link lands on the private claim's step, in view", box is not None and 0 <= box["y"] < 900, box)
        tp.close()
        summary, out = review(page)
        page.click("[data-testid=bridge-go]")
        res = wait_out(page, "Deposited|failed|rror|changed")
        check("a deposit to the wallet's private account /0 (step 6e's first half)", res.startswith("Deposited: leaf 1 in block "), res)

        # -- 6. A browser wallet without Sepolia ETH: refused at the review, before any signature
        poor = eth("eth_accounts")["result"][2].lower()
        eth("anvil_setBalance", [poor, "0x0"])
        page.evaluate("(a) => { window.__account = a; }", poor)
        page.select_option("[data-testid=bridge-kind]", "public")
        page.click("[data-testid=bridge-connect]")
        page.wait_for_function("(a) => document.querySelector('[data-testid=bridge-status]').textContent.toLowerCase().includes(a)", arg=poor, timeout=30000)
        summary, out = review(page)
        check("no Sepolia ETH: refused at the review in plain words, nothing to sign, nothing deposited",
              out.startswith("Your Sepolia ETH, 0 ETH, does not cover") and "Nothing was signed" in out
              and page.locator("[data-testid=bridge-go]").is_disabled() and not deposits_of(poor), out)
        shot(page, "05-no-eth")
        # -- 6b. Enough for the deposit and its fees, not for the gas: refused at the review, the gas named
        tot = re.search(r"does not cover ([0-9.]+) ETH", out)
        if tot:
            eth("anvil_setBalance", [poor, hex(int(Decimal(tot.group(1)) * 10**18) + 1)])
            summary, out = review(page)
        check("enough for the deposit and its fees, not for the gas: refused at the review, before any signature",
              bool(tot) and out.startswith("Your Sepolia ETH,") and "(the deposit, its fees and the gas)" in out
              and page.locator("[data-testid=bridge-go]").is_disabled() and not deposits_of(poor), out)
        page.evaluate("(a) => { window.__account = a; }", DEPOSITOR)
        page.click("[data-testid=bridge-connect]")
        page.wait_for_timeout(800)

        # -- 6c. Review pressed while a deposit waits for the wallet's signature: connect, review and the deposit's
        # choices are locked, and the deposit sends the plan that was reviewed
        page.select_option("[data-testid=bridge-kind]", "public")
        summary, out = review(page)
        reviewed = re.search(r"Commitment C\s*(0x[0-9a-f]{64})", summary)
        page.evaluate("() => { window.__hold = true; }")
        page.click("[data-testid=bridge-go]")
        page.wait_for_function("() => window.__held === true", timeout=60000)
        locked = {x: page.locator(f"[data-testid={x}]").is_disabled() for x in ("bridge-review", "bridge-connect", "bridge-kind", "bridge-denom")}
        page.locator("[data-testid=bridge-review]").click(force=True)   # a disabled button ignores it, and the lock too
        page.evaluate("() => { window.__hold = false; window.__release(); }")
        res = wait_out(page, "Deposited|failed|rror|changed")
        sent = [log["data"][-64:] for log in deposits_of(DEPOSITOR)]
        check("Review pressed during a deposit's signature: the controls locked, the reviewed plan sent",
              all(locked.values()) and res.startswith("Deposited: leaf 2 ") and reviewed and reviewed.group(1)[2:] in sent,
              (locked, res[:120]))

        # -- 7. Step 6b's first half: the wallet finds both deposits on the vault
        page.click("[data-testid=bridge-find]")
        page.wait_for_function("() => /deposits are this wallet|rror/.test(document.querySelector('[data-testid=bridge-claim-out]').textContent)", timeout=60000)
        found = text(page, "[data-testid=bridge-claim-out]")
        mine = len(deposits_of(DEPOSITOR))
        check(f"\"Find my deposits on the vault\" finds this wallet's {mine}", found.startswith(f"{mine} of the vault's {mine} "), found)
        lst = page.locator("[data-testid=bridge-deposits]")
        check("the private deposit links the steps of a private claim",
              lst.locator("a[href='/testers/#step-private']").count() == 1, text(page, "[data-testid=bridge-deposits]")[:300])
        notes["claim"] = ("not rehearsed: no attestation or bridge program on this local zone, so the page offers no "
                          "\"Claim on LEZ\"" + (" (and it offers none)" if page.locator("[data-testid=bridge-claim]").count() == 0 else
                                                  " (but it offers one)"))
        shot(page, "06-found")

        origin = "{0.scheme}://{0.netloc}".format(urllib.parse.urlparse(ORIGIN))
        others = sorted({u for u in requests_seen if not (u.startswith(origin) or u.startswith("data:"))})
        check("every request went to the page's own origin (GitHub's badges and API aside, which the build page names)",
              all(u.startswith(("https://github.com/", "https://api.github.com/")) for u in others), others[:5])
        check("no uncaught script error", not errors, errors[:3])
        ctx.close()
        browser.close()


def main():
    os.makedirs(SHOTS, exist_ok=True)
    try:
        steps()
    except Exception as e:  # noqa: BLE001  the record still names how far the run got
        check("the run completed without an unexpected error", False, repr(e)[:300])
    os.makedirs(OUT, exist_ok=True)
    rec = {"passed": sum(c["ok"] for c in checks), "failed": sum(not c["ok"] for c in checks), "notes": notes, "checks": checks}
    with open(os.path.join(OUT, "record.json"), "w") as f:
        json.dump(rec, f, indent=1)
    print(f"testers_path: {rec['passed']} passed, {rec['failed']} failed", flush=True)
    return 0 if checks and not rec["failed"] else 1


if __name__ == "__main__":
    sys.exit(main())
