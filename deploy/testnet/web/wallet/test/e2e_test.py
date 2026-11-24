"""The web wallet end to end in headless Chromium against a running LEZ sequencer (run by e2e.sh, or by hand with the
same environment). Every step the page reports is checked again against the sequencer's RPC directly.

Environment: WALLET_URL (the page, with /rpc and /faucet on its origin), WALLET_RPC (the sequencer, for the checks),
WALLET_KEY_B (LEZ's published local test key B, hex; optional), WALLET_OUT (the record's directory), WALLET_SHOTS
(screenshots), WALLET_ROOT (the bundle). Exit 0 when every check passed.
"""
import hashlib
import json
import os
import re
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

URL = os.environ["WALLET_URL"]
RPC = os.environ["WALLET_RPC"]
KEY_B = os.environ.get("WALLET_KEY_B")
OUT = os.environ["WALLET_OUT"]
SHOTS = os.environ["WALLET_SHOTS"]
ORIGIN = "{0.scheme}://{0.netloc}".format(urllib.parse.urlparse(URL))
UNIT = 10**9
PASS = "correct horse battery staple"
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

checks, numbers, labels = [], {}, {}
requests_seen = []


def b58(b: bytes) -> str:
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def account_id_of(pk_hex: str) -> str:
    return b58(hashlib.sha256(b"/LEE/v0.3/AccountId/Public/".ljust(32, b"\0") + bytes.fromhex(pk_hex)).digest())


def label(text: str) -> str:
    """The record names accounts and hashes by role, not by value."""
    def sub(m):
        v = m.group(0)
        if v not in labels:
            labels[v] = f"<{'hash' if re.fullmatch('[0-9a-f]{64}', v) else 'id'}{len(labels) + 1}>"
        return labels[v]
    return re.sub(r"\b[0-9a-f]{64}\b|\b[1-9A-HJ-NP-Za-km-z]{43,44}\b", sub, text)


def check(name, ok, detail=""):
    checks.append({"check": name, "ok": bool(ok), "detail": label(str(detail))[:300]})
    print(("PASS " if ok else "FAIL ") + name + (f" -- {label(str(detail))[:200]}" if detail else ""), flush=True)
    return ok


def rpc(method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    with urllib.request.urlopen(urllib.request.Request(RPC, data=body, headers={"Content-Type": "application/json"}), timeout=15) as r:
        j = json.loads(r.read())
    if "error" in j:
        raise RuntimeError(f"{method}: {j['error']}")
    return j["result"]


def bal(i):
    return int(rpc("getAccountBalance", [i]))


def nonce(i):
    return int(rpc("getAccountsNonces", [[i]])[0])


def wait_for(fn, timeout=120, step=1.0):
    end = time.time() + timeout
    while time.time() < end:
        v = fn()
        if v:
            return v
        time.sleep(step)
    return fn()


def text(page, sel):
    return page.locator(sel).inner_text().strip()


def shot(page, name):
    page.screenshot(path=os.path.join(SHOTS, name + ".png"), full_page=True)


def track(page):
    page.on("request", lambda req: requests_seen.append(req.url))


def unlock(page, navigate=True):
    if navigate:
        page.goto(URL)
    page.wait_for_selector("#v-unlock:not([hidden])", timeout=15000)
    page.fill("#unlock-pass", PASS)
    page.click("#unlock-go")
    page.wait_for_selector("#v-main:not([hidden])", timeout=30000)


def send(page, to, amount):
    page.goto(URL + "#send")
    page.fill("#send-to", to)
    page.fill("#send-amount", amount)
    page.click("#send-review")
    page.wait_for_function("() => !document.getElementById('send-confirm').hidden || document.getElementById('send-err').textContent", timeout=30000)


ANVIL = os.environ.get("WALLET_ANVIL") or None
VAULT = os.environ.get("WALLET_VAULT") or None
DEPOSITOR = (os.environ.get("WALLET_DEPOSITOR") or "").lower()


def eth_rpc(method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    with urllib.request.urlopen(urllib.request.Request(ANVIL, data=body, headers={"Content-Type": "application/json"}), timeout=30) as r:
        return json.loads(r.read())


def b58decode(s):
    n = 0
    for ch in s:
        n = n * 58 + B58.index(ch)
    return n.to_bytes(32, "big")


def expected_commitment(words, chain_id, vault, depositor, j, kind, dest32):
    """SPEC-v2 with the ferrata tool's bridge seed, recomputed here from the phrase with Python's standard library."""
    import hmac as _h
    seed64 = hashlib.pbkdf2_hmac("sha512", " ".join(words).encode(), b"mnemonic", 2048)
    hs = lambda k, m: _h.new(k, m, hashlib.sha256).digest()  # noqa: E731
    k_auth = hs(hs(seed64, b"ferrata/bridge-seed/v1"), b"ferrata/auth/v1")
    s_ = hs(k_auth, b"ferrata/s/v1" + chain_id.to_bytes(32, "big") + bytes.fromhex(vault[2:]) + bytes.fromhex(depositor[2:]) + j.to_bytes(8, "big"))
    return "0x" + hashlib.sha256(b"ferrata/C/v1" + s_ + bytes([kind]) + dest32).hexdigest()


STAND_IN = """
window.__chain = '0xaa36a7';
window.ethereum = { isStandIn: true, async request({ method, params }) {
  if (method === 'eth_requestAccounts' || method === 'eth_accounts') return [DEPOSITOR];
  if (method === 'eth_chainId') return window.__chain;
  if (method === 'eth_sendTransaction' && params[0].chainId && params[0].chainId !== window.__chain) {
    const e = new Error('the transaction names chain ' + params[0].chainId); e.code = 4901; throw e; }
  const r = await window.__walletRpc(method, params || []);
  if (r.error) { const e = new Error(r.error.message); e.code = r.error.code; throw e; }
  return r.result;
} };
"""


def bridge_steps(browser, words, id1):
    ctx = browser.new_context(viewport={"width": 1280, "height": 900})
    ctx.expose_function("__walletRpc", eth_rpc)
    ctx.add_init_script(STAND_IN.replace("DEPOSITOR", json.dumps(DEPOSITOR)))
    pg = ctx.new_page()
    track(pg)
    pg.goto(URL)
    pg.click("#go-import")
    pg.fill("#import-phrase", " ".join(words))
    pg.fill("#import-pass", PASS)
    pg.click("#import-go")
    pg.wait_for_selector("#v-main:not([hidden])", timeout=30000)
    pg.goto(URL + "#bridge")
    pg.click("[data-testid=bridge-connect]")
    pg.wait_for_function("() => /Connected/.test(document.querySelector('[data-testid=bridge-status]').textContent) || document.querySelector('[data-testid=bridge-out]').textContent", timeout=30000)
    check("bridge: the page connects to the browser wallet and reads the vault (j = 0)", "j = 0" in text(pg, "[data-testid=bridge-status]"), text(pg, "[data-testid=bridge-status]") + " " + text(pg, "[data-testid=bridge-out]"))
    pg.evaluate("() => { window.__chain = '0x1'; }")
    pg.click("[data-testid=bridge-review]")
    pg.wait_for_function("() => document.querySelector('[data-testid=bridge-out]').textContent", timeout=30000)
    check("bridge: refused while the browser wallet is on another chain (nothing to sign)", "on chain 1" in text(pg, "[data-testid=bridge-out]") and pg.locator("[data-testid=bridge-go]").is_disabled(), text(pg, "[data-testid=bridge-out]"))
    pg.evaluate("() => { window.__chain = '0xaa36a7'; }")
    pg.click("[data-testid=bridge-connect]")
    pg.wait_for_timeout(1500)
    pg.select_option("[data-testid=bridge-kind]", "public")
    pg.click("[data-testid=bridge-review]")
    pg.wait_for_function("() => /Commitment C/.test(document.querySelector('[data-testid=bridge-summary]').textContent) || /rror|No |not/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=30000)
    summary = text(pg, "[data-testid=bridge-summary]")
    m = re.search(r"Commitment C\s*(0x[0-9a-f]{64})", summary)
    want = expected_commitment(words, 11155111, VAULT.lower(), DEPOSITOR, 0, 2, b58decode(id1))
    check("bridge: C in the review equals SPEC-v2's C recomputed from the phrase (public destination: this account)", m and m.group(1) == want, (m.group(1) if m else summary[:200], want))
    shot(pg, "08-bridge-review")
    pg.click("[data-testid=bridge-go]")
    pg.wait_for_function("() => /Deposited|failed|rror|changed/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=120000)
    res = text(pg, "[data-testid=bridge-out]")
    check("bridge: the browser wallet's deposit is mined and its Deposit event checked by the page", "Deposited: leaf 0" in res, res)
    logs = eth_rpc("eth_getLogs", [{"address": VAULT, "fromBlock": "0x0", "toBlock": "latest"}]).get("result", [])
    dep = [l for l in logs if len(l["topics"]) == 3 and l["topics"][1][-40:] == DEPOSITOR[2:] and l["data"][-64:] == want[2:]]
    check("bridge: anvil's vault logged exactly that C for this depositor", len(dep) == 1, len(dep))
    # The private account /0 the page derives (LEZ's private key tree, ML-KEM-768), then a deposit to it (kind 0x01).
    pg.goto(URL + "#private")
    pg.wait_for_selector("[data-testid=private-id]", timeout=20000)
    priv = text(pg, "[data-testid=private-id]")
    check("the Private tab shows this phrase's private account /0 and its keys", re.fullmatch(r"Private/[1-9A-HJ-NP-Za-km-z]{43,44}", priv) and len(text(pg, "[data-testid=private-npk]")) == 64, priv)
    shot(pg, "10-private")
    pg.goto(URL + "#bridge")
    pg.click("[data-testid=bridge-connect]")
    pg.wait_for_function("() => /j = 1/.test(document.querySelector('[data-testid=bridge-status]').textContent)", timeout=30000)
    pg.select_option("[data-testid=bridge-kind]", "private")
    check("bridge: no field for someone else's private account (such a deposit could never be claimed; review M-W2)",
          pg.locator("[data-testid=bridge-dest]").count() == 0)
    pg.click("[data-testid=bridge-review]")
    pg.wait_for_function("() => /Commitment C/.test(document.querySelector('[data-testid=bridge-summary]').textContent) || /rror/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=30000)
    m2 = re.search(r"Commitment C\s*(0x[0-9a-f]{64})", text(pg, "[data-testid=bridge-summary]"))
    want2 = expected_commitment(words, 11155111, VAULT.lower(), DEPOSITOR, 1, 1, b58decode(priv.split("/", 1)[1]))
    check("bridge: a deposit to this wallet's private account commits to it (kind 0x01, j = 1)", m2 and m2.group(1) == want2, (m2.group(1) if m2 else None, want2))
    pg.click("[data-testid=bridge-go]")
    pg.wait_for_function("() => /Deposited|failed|rror|changed/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=120000)
    check("bridge: the private-destination deposit is mined (leaf 1)", "Deposited: leaf 1" in text(pg, "[data-testid=bridge-out]"), text(pg, "[data-testid=bridge-out]"))
    pg.goto(URL + "#home")
    pg.goto(URL + "#bridge")
    pg.wait_for_timeout(800)
    lst = text(pg, "[data-testid=bridge-deposits]")
    check("bridge: both deposits are kept (encrypted) in the wallet's list", lst.count("deposited") == 2 and "private LEZ account" in lst, lst[:300])
    shot(pg, "09-bridge-deposited")
    ctx.close()


def main():
    try:
        steps()
    except Exception as e:  # noqa: BLE001  the record still names how far the run got
        check("the run completed without an unexpected error", False, repr(e)[:300])
    ok = all(c["ok"] for c in checks)
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "record.json"), "w") as f:
        json.dump({"passed": sum(c["ok"] for c in checks), "failed": sum(not c["ok"] for c in checks),
                   "numbers": numbers, "checks": checks}, f, indent=1)
    print(f"{sum(c['ok'] for c in checks)} passed, {sum(not c['ok'] for c in checks)} failed; numbers {numbers}")
    return 0 if ok else 1


def steps():
    with sync_playwright() as p:
        browser = p.chromium.launch()
        ctx = browser.new_context(viewport={"width": 1280, "height": 900})
        page = ctx.new_page()
        track(page)
        console = []
        page.on("console", lambda m: console.append(f"{m.type}: {m.text}"))
        # An uncaught error names the step it followed and where it was thrown.
        page.on("pageerror", lambda e: console.append(f"pageerror: {e} (after: {checks[-1]['check'] if checks else 'start'}; "
                                                      f"{(getattr(e, 'stack', '') or '').splitlines()[1:3]})"))

        # -- 1. The page and its policy
        resp = page.goto(URL)
        csp = resp.headers.get("content-security-policy", "")
        check("page served with a CSP that allows only its own origin", "default-src 'none'" in csp and "connect-src 'self'" in csp and "frame-ancestors 'none'" in csp, csp)
        page.wait_for_function("() => /block \\d+/.test(document.getElementById('net-status').textContent)", timeout=30000)
        check("the page reads the head block through /rpc", True, text(page, "#net-status"))
        check("first visit offers create or import", page.locator("#v-welcome").is_visible())
        check("the welcome screen says the wallet shares its web origin with the host's other pages (review M-W1)",
              page.locator("[data-testid=shared-origin-welcome]").is_visible() and "shares its web" in text(page, "[data-testid=shared-origin-welcome]"))

        # -- 2. Create
        page.click("#go-create")
        words = [w.strip() for w in page.locator("#mnemonic-words li").all_text_contents()]
        check("a new wallet shows a 24-word recovery phrase", len(words) == 24, len(words))
        page.fill("#create-pass", PASS)
        page.fill("#create-pass2", PASS)
        page.click("#create-go")
        check("creating is refused until the words are confirmed written down", "Write the 24 words" in text(page, "#create-err"))
        page.check("#saved-words")
        page.click("#create-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        id1 = text(page, "#home-id")
        check("the wallet opens on its first account", re.fullmatch("[1-9A-HJ-NP-Za-km-z]{43,44}", id1), id1)
        stored = page.evaluate("() => localStorage.getItem('ferrata-wallet/v1')")
        check("the stored record is encrypted (no phrase word, no account id in it)",
              stored and not any(w in stored for w in words) and id1 not in stored and '"ciphertext"' in stored)
        check("a fresh account is absent on LEZ", rpc("getAccount", [id1]) is None)
        page.goto(URL + "#receive")
        pk1 = text(page, "#recv-pk")
        check("its public key hashes to its account id (LEZ's AccountId::from)", account_id_of(pk1) == id1)
        shot(page, "01-receive")

        # -- 2b. LEZ itself: a faucet that names a never-used account by id alone (no public key) cannot credit it
        req = urllib.request.Request(URL + "faucet", data=json.dumps({"account": id1}).encode(), headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=200) as r:
                fr = (r.status, json.loads(r.read()))
        except urllib.error.HTTPError as e:
            fr = (e.code, json.loads(e.read() or b"{}"))
        check("LEZ refuses to credit a never-used account named by id alone (no admission evidence)",
              fr[0] != 200 and bal(id1) == 0 and rpc("getAccount", [id1]) is None, fr)
        numbers["faucet_id_only_refusal"] = (fr[1].get("detail") or fr[1].get("error") or "")[:160]
        time.sleep(6)  # the stand-in's per-account interval

        # -- 3. Faucet (LEZ's own wallet CLI sends: interoperability with LEZ's signer)
        page.goto(URL + "#home")
        t0 = time.time()
        page.click("#faucet")
        page.wait_for_function("() => /Received|failed|not arrived|HTTP|error/i.test(document.getElementById('faucet-msg').textContent)", timeout=200000)
        numbers["faucet_seconds"] = round(time.time() - t0, 1)
        check("the faucet button funds the new account", "Received" in text(page, "#faucet-msg"), text(page, "#faucet-msg"))
        check("the RPC agrees: 5 LGO", bal(id1) == 5 * UNIT, bal(id1))
        page.wait_for_function("() => document.getElementById('balance').textContent.startsWith('5 ')", timeout=20000)
        check("the page shows the balance", text(page, "#balance") == "5 LGO", text(page, "#balance"))
        shot(page, "02-home-funded")

        # -- 4. A second account from the same phrase
        page.click("#add-account")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent !== a", arg=id1, timeout=10000)
        page.goto(URL + "#receive")
        code2, id2, pk2 = text(page, "#recv-code"), text(page, "#recv-id"), text(page, "#recv-pk")
        check("account 2 has its own id and a receive code with its key", id2 != id1 and code2 == f"lez:{id2}?pk={pk2}" and account_id_of(pk2) == id2)
        page.select_option("#account-select", "0")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id1, timeout=10000)

        # -- 5. Refusals: nothing is sent
        n_before = nonce(id1)
        send(page, code2, "1000")
        check("refused: more than the balance", "Not enough" in text(page, "#send-err"), text(page, "#send-err"))
        fresh = b58(secrets.token_bytes(32))
        send(page, fresh, "1")
        check("refused: a never-used account without its key (asks for the receive code)", "receive code" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, "hello", "1")
        check("refused: a malformed recipient, in plain words", "not an account id" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, code2, "0.0000000001")
        check("refused: more decimals than the token has", "decimals" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, id1, "1")
        check("refused: a transfer to the same account", "this account" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, code2, "1,5")
        check("refused: an amount with a comma, never read as 15 or 1.5 (review L-W3)", "no commas" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, code2, "0.1")
        page.fill("#send-amount", "0.2")
        check("a review is dropped when the form changes after it (review L-W4)", not page.locator("#send-confirm").is_visible() and text(page, "#send-summary") == "", text(page, "#send-summary"))
        check("the refusals sent nothing (nonce unchanged)", nonce(id1) == n_before, nonce(id1))

        # -- 6. Send to a new account (its key goes as admission evidence)
        send(page, code2, "1.25")
        check("the review shows the transfer before signing", page.locator("#send-confirm").is_visible() and "First credit" in text(page, "#send-summary"), text(page, "#send-summary"))
        shot(page, "03-send-review")
        t0 = time.time()
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror|refused/.test(document.getElementById('send-result').textContent)", timeout=200000)
        numbers["send1_seconds_to_block"] = round(time.time() - t0, 1)
        res = text(page, "#send-result")
        check("the page reports the transfer in a block, nonce moved", "In block" in res and "nonce moved to 1" in res, res)
        check("the RPC agrees: account 2 holds 1.25 LGO", bal(id2) == 1_250_000_000, bal(id2))
        fee1 = 5 * UNIT - 1_250_000_000 - bal(id1)
        numbers["send1_fee_atomic"] = fee1
        check("the RPC agrees: account 1 paid 1.25 LGO and a fee", nonce(id1) == 1 and 0 < fee1 < 10_000_000, f"fee {fee1}")
        hashes = re.findall(r"\b[0-9a-f]{64}\b", res)
        tx = rpc("getTransaction", [hashes[0]]) if hashes else None
        check("the transaction hash the page shows is the sequencer's", tx is not None, hashes[:1])
        check("the page computed the same hash as the sequencer (no warning)", not [c for c in console if "named the transaction" in c], console[-3:])
        shot(page, "04-send-done")

        # -- 7. Send back by account id alone (the account exists now)
        page.select_option("#account-select", "1")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id2, timeout=10000)
        b1 = bal(id1)
        send(page, id1, "0.5")
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror/.test(document.getElementById('send-result').textContent)", timeout=200000)
        check("account 2 sends to account 1 by id", "In block" in text(page, "#send-result"), text(page, "#send-result"))
        check("the RPC agrees: +0.5 LGO exactly", bal(id1) == b1 + 500_000_000 and nonce(id2) == 1, bal(id1) - b1)

        # -- 7b. A transfer that reverts (review L-W17): the RPC overstates account 2's balance at review time (as if a
        # payment to it were undone between the review and the signature), so the page signs 5 LGO it does not hold; LEZ
        # includes the transaction, charges the fee and lists it as reverted, and the page must say so.
        real2 = bal(id2)

        def overstate(route):
            try:
                body = json.loads(route.request.post_data or "{}")
                if body.get("method") == "getAccountBalance" and (body.get("params") or [None])[0] == id2:
                    resp = route.fetch()
                    data = resp.json()
                    data["result"] = int(data["result"]) + 10 * UNIT
                    return route.fulfill(response=resp, json=data)
            except Exception as e:  # never leave a request of the page hanging
                print(f"overstate: {e!r}", flush=True)
            return route.fallback()

        page.route("**/rpc", overstate)
        b1, n2 = bal(id1), nonce(id2)
        send(page, id1, "5")
        reviewed = page.locator("#send-confirm").is_visible()
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror|reverted/.test(document.getElementById('send-result').textContent)", timeout=200000)
        page.unroute("**/rpc", overstate)
        res = text(page, "#send-result")
        fee_r = real2 - bal(id2)
        numbers["revert_fee_atomic"] = fee_r
        check("a transfer LEZ reverts is reported as reverted, not applied (review L-W17)", reviewed and "reverted" in res and "applied" not in res, res)
        check("the RPC agrees: the nonce moved, the fee was charged, nothing moved to account 1",
              nonce(id2) == n2 + 1 and bal(id1) == b1 and 0 < fee_r < 10_000_000, f"nonce {nonce(id2) - n2}, fee {fee_r}, to {bal(id1) - b1}")

        # -- 8. Activity
        page.select_option("#account-select", "0")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id1, timeout=10000)
        page.goto(URL + "#activity")
        page.wait_for_function("() => /read/.test(document.getElementById('act-status').textContent)", timeout=60000)
        page.wait_for_timeout(500)
        items = page.locator("#act-list li[data-testid=act-item]").all_inner_texts()
        joined = "\n".join(items)
        check("Activity lists the faucet's credit, the send and the return", len(items) >= 3 and "+5 LGO" in joined and "−1.25 LGO" in joined and "+0.5 LGO" in joined, joined)
        reverted_rows = [i for i in items if "reverted" in i]
        check("Activity marks the reverted transfer reverted, and only it (its block's reverted list)",
              len(reverted_rows) == 1 and "5 LGO" in reverted_rows[0] and all("reverted" not in i for i in items if "+0.5 LGO" in i or "−1.25 LGO" in i), joined)
        shot(page, "05-activity")

        # -- 9. Lock, a wrong passphrase, unlock; then a reload
        page.goto(URL + "#home")
        page.click("#lock")
        check("Lock forgets the keys (unlock screen)", page.locator("#v-unlock").is_visible())
        page.fill("#unlock-pass", "not the passphrase")
        page.click("#unlock-go")
        page.wait_for_function("() => document.getElementById('unlock-err').textContent", timeout=30000)
        check("a wrong passphrase is refused", "wrong passphrase" in text(page, "#unlock-err"), text(page, "#unlock-err"))
        page.fill("#unlock-pass", PASS)
        page.click("#unlock-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        check("the right passphrase opens it again", text(page, "#home-id") == id1)
        page.reload()
        unlock(page, navigate=False)  # the reloaded page itself (a second navigation would cut its loading short)
        check("after a reload: both accounts, the same ids", page.locator("#account-select option").count() == 2 and text(page, "#home-id") == id1)

        # -- 10. Backup
        page.goto(URL + "#settings")
        page.fill("#reveal-pass", PASS)
        page.click("#reveal-phrase")
        page.wait_for_function("() => document.getElementById('reveal-out').textContent", timeout=30000)
        check("the recovery phrase can be shown again, with the passphrase", text(page, "#reveal-out") == " ".join(words))
        page.wait_for_function("() => /Head block/.test(document.getElementById('net-info').textContent)", timeout=20000)
        check("network info names the head and the privacy circuit", "Privacy circuit" in text(page, "#net-info"), text(page, "#net-info"))

        # -- 11. Import the phrase elsewhere: the same accounts (determinism)
        ctx2 = browser.new_context(viewport={"width": 1280, "height": 900})
        p2 = ctx2.new_page()
        track(p2)
        p2.goto(URL)
        p2.click("#go-import")
        p2.fill("#import-phrase", " ".join(words).upper() + "  ")
        p2.fill("#import-pass", PASS)
        p2.click("#import-go")
        p2.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        check("importing the phrase gives the same first account", text(p2, "#home-id") == id1)
        p2.click("#add-account")
        p2.wait_for_function("(a) => document.getElementById('home-id').textContent !== a", arg=id1, timeout=10000)
        check("and the same second account", text(p2, "#home-id") == id2)
        ctx2.close()

        # -- 12. Import a raw key (LEZ's published local test key B) and send with it
        if KEY_B:
            ctx3 = browser.new_context(viewport={"width": 1280, "height": 900})
            p3 = ctx3.new_page()
            track(p3)
            p3.goto(URL)
            p3.click("#go-import")
            p3.click("[data-import=key]")
            p3.fill("#import-key", KEY_B)
            p3.fill("#import-pass", PASS)
            p3.click("#import-go")
            p3.wait_for_selector("#v-main:not([hidden])", timeout=30000)
            idb = text(p3, "#home-id")
            p3.wait_for_function("() => document.getElementById('balance').textContent !== '—'", timeout=20000)
            check("an imported raw key opens its account, balance from the RPC", bal(idb) > 0 and text(p3, "#balance").replace(",", "").split(" ")[0] == str(bal(idb) // UNIT) if bal(idb) % UNIT == 0 else True, f"{text(p3, '#balance')}")
            b1 = bal(id1)
            send(p3, id1, "0.1")
            p3.click("#send-go")
            p3.wait_for_function("() => /In block|Not in a block|rror/.test(document.getElementById('send-result').textContent)", timeout=200000)
            check("a raw key signs a transfer LEZ applies", bal(id1) == b1 + 100_000_000, bal(id1) - b1)
            ctx3.close()

        # -- 13. On a phone-sized screen
        ctx4 = browser.new_context(viewport={"width": 390, "height": 844}, device_scale_factor=2, is_mobile=True, has_touch=True)
        p4 = ctx4.new_page()
        track(p4)
        p4.goto(URL)
        p4.click("#go-import")
        p4.fill("#import-phrase", " ".join(words))
        p4.fill("#import-pass", PASS)
        p4.click("#import-go")
        p4.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        p4.wait_for_function("() => document.getElementById('balance').textContent !== '—'", timeout=20000)
        box = p4.locator("#tabs").bounding_box()
        check("on a phone the tabs sit at the bottom of the screen", box and box["y"] > 700, box)
        width = p4.evaluate("() => document.documentElement.scrollWidth")
        check("no sideways scrolling on a 390 px screen", width <= 390, width)
        shot(p4, "06-phone-home")
        p4.goto(URL + "#send")
        shot(p4, "07-phone-send")
        ctx4.close()

        # -- 13b. The Ferrata tabs on a network without the programs: they say so and send nothing
        page.goto(URL + "#prove")
        check("Prove says when no attestation program is configured", page.locator("[data-testid=prove-unconfigured]").count() == 1 or page.locator("[data-testid=prove-record]").count() == 1)
        page.goto(URL + "#private")
        page.wait_for_function("() => document.getElementById('private-body').textContent.length > 50", timeout=10000)
        check("the Private tab states what runs in the page and what proving needs", "prov" in text(page, "#private-body").lower(), text(page, "#private-body")[:200])

        # -- 13c. The bridge's deposit from a browser wallet (a stand-in EIP-1193 wallet over anvil as Sepolia)
        if ANVIL and VAULT:
            bridge_steps(browser, words, id1)

        # -- 14. The page's own traffic
        others = sorted({u for u in requests_seen if not u.startswith(ORIGIN + "/")})
        check("every request the page made went to its own origin", not others, others)
        errors = [c for c in console if c.startswith("pageerror")]
        context = [console[max(0, i - 4):i + 1] for i, c in enumerate(console) if c.startswith("pageerror")]
        for i, c in enumerate(console):  # the whole neighbourhood of an uncaught error, for the log
            if c.startswith("pageerror"):
                print("== console around an uncaught error:\n" + "\n".join(label(x)[:600] for x in console[max(0, i - 8):i + 3]), flush=True)
        check("no uncaught script error", not errors, context or errors)
        browser.close()


if __name__ == "__main__":
    sys.exit(main())
