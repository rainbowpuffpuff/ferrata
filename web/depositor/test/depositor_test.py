#!/usr/bin/env python3
"""web/depositor: the headless-browser test of the deposit page.

Opens web/depositor/index.html from file:// in headless Chromium with a minimal EIP-1193 wallet injected
(a script that forwards `request` to the local anvil and records every method called), and drives the page
through one ETH deposit, one ERC-20 deposit (approve first), and six refusals, asserting each on the chain
through JSON-RPC from here.

Run it through web/depositor/test/run.sh, which starts anvil, deploys the vault
(vault/script/WebDepositorLocal.s.sol) and passes the addresses; it needs:

  FERRATA_RPC      the anvil JSON-RPC URL (http://127.0.0.1:18645)
  FERRATA_VAULT    the deployed vault's address
  FERRATA_TOKEN    the deployed TestToken's address
  FERRATA_ACCOUNT  anvil's account 1, the account the injected wallet answers for

It writes the run's output to web/depositor/results/anvil-2026-11-21.txt and one screenshot of the summary
screen to web/depositor/results/anvil-2026-11-21.png, and exits non-zero on the first failure.
"""
import hashlib
import hmac
import json
import os
import re
import sys
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

REPO = Path(__file__).resolve().parents[3]
PAGE = (REPO / "web" / "depositor" / "index.html").as_uri()
VECTORS = REPO / "bridge" / "vectors" / "inbound-v2.json"
RESULTS = REPO / "web" / "depositor" / "results"
OUT_TXT = RESULTS / "anvil-2026-11-21.txt"
OUT_PNG = RESULTS / "anvil-2026-11-21.png"

RPC = os.environ["FERRATA_RPC"]
VAULT = os.environ["FERRATA_VAULT"].lower()
TOKEN = os.environ["FERRATA_TOKEN"].lower()
ACCOUNT = os.environ["FERRATA_ACCOUNT"].lower()
ETH = "0x" + "00" * 20

# keccak256("Deposit(address,uint64,uint64,address,uint256,bytes32)"), computed with `cast keccak` outside
# this file, so the test does not depend on the page's own keccak.
DEPOSIT_TOPIC = "0x83d2d1c34c8825843c9af84bec80378e997084e8c1d6d5a3c2a6e758fe05538e"
SELECTOR_APPROVE = "095ea7b3"
SELECTOR_DEPOSIT_ETH = "f7edb857"
SELECTOR_DEPOSIT = "7fde1952"

# The wallet client of web/depositor/index.html allows exactly these EIP-1193 methods.
ALLOWED_METHODS = {
    "eth_requestAccounts", "eth_accounts", "eth_chainId", "eth_call", "eth_getCode",
    "eth_getBlockByNumber", "eth_blockNumber", "eth_sendTransaction", "eth_getTransactionReceipt",
}

# The local deployment's registration (vault/script/WebDepositorLocal.s.sol): denominations and relay fees of
# denomination 0, schedule 0 (no mint fee), for ETH and the 6-decimal test token.
ETH_TOTAL = 100_000_000_000_000_000 + 100_000_000_000_000  # 0.1 ETH + 0.0001 ETH relay fee
TOKEN_TOTAL = 1_000_000 + 10_000  # 1 TUSD + 0.01 TUSD relay fee

log_lines = []


def log(s=""):
    print(s)
    log_lines.append(s)


def fail(msg):
    raise AssertionError(msg)


def check(cond, msg):
    if not cond:
        fail(msg)
    log("    ok: " + msg)


# ---------------------------------------------------------------- JSON-RPC, straight to anvil
def rpc(method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(RPC, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        j = json.load(r)
    if "error" in j:
        raise RuntimeError(f"{method}: {j['error']}")
    return j["result"]


def call(to, data):
    return rpc("eth_call", [{"to": to, "data": data}, "latest"])


def word(x):
    return f"{x:064x}"


def enc_view(sig, args):
    """A view's calldata from the keccak selectors hardcoded above (no keccak needed in Python)."""
    if sig == "depositCount(address)":
        return "0xf4443d9a" + word(int(args[0], 16))
    if sig == "allowance(address,address)":
        return "0xdd62ed3e" + word(int(args[0], 16)) + word(int(args[1], 16))
    if sig == "balanceOf(address)":
        return "0x70a08231" + word(int(args[0], 16))
    raise KeyError(sig + " is not one of the view signatures this test hardcodes")


def deposit_count(who):
    return int(call(VAULT, enc_view("depositCount(address)", [who])), 16)


def allowance(owner, spender):
    return int(call(TOKEN, enc_view("allowance(address,address)", [owner, spender])), 16)


def token_balance(who):
    return int(call(TOKEN, enc_view("balanceOf(address)", [who])), 16)


def eth_balance(who):
    return int(rpc("eth_getBalance", [who, "latest"]), 16)


def deposit_logs():
    logs = rpc("eth_getLogs", [{"address": VAULT, "fromBlock": "0x0", "toBlock": "latest", "topics": [DEPOSIT_TOPIC]}])
    out = []
    for lg in logs:
        data = lg["data"][2:]
        assert len(data) == 64 * 4, "a Deposit log with unexpected data length"
        topics = lg["topics"]
        out.append({
            "tx": lg["transactionHash"],
            "depositor": "0x" + topics[1][-40:],
            "token": "0x" + topics[2][-40:],
            "j": int(data[0:64], 16),
            "leafIndex": int(data[64:128], 16),
            "amount": int(data[128:192], 16),
            "commitment": "0x" + data[192:256],
        })
    return out


# ------------------------------------------------- the commitment, exactly as the Logos tool derives it
def b20(addr):
    return bytes.fromhex(addr[2:])


def deposit_secret(k_auth, chain_id, vault, depositor, j):
    """s = HMAC-SHA256(kAuth, "ferrata/s/v1" ‖ evmChainId ‖ vault ‖ depositor ‖ j) (bridge/src encoding.rs)."""
    msg = b"ferrata/s/v1" + chain_id.to_bytes(32, "big") + b20(vault) + b20(depositor) + j.to_bytes(8, "big")
    return hmac.new(k_auth, msg, hashlib.sha256).digest()


def commitment(s, kind, dest):
    """C = sha256("ferrata/C/v1" ‖ s ‖ kind ‖ dest)."""
    return hashlib.sha256(b"ferrata/C/v1" + s + bytes([kind]) + dest).digest()


K_AUTH = hashlib.sha256(b"ferrata web depositor test: kAuth").digest()
DEST = hashlib.sha256(b"ferrata web depositor test: dest").digest()


def ticket(j, chain_id=None, token=None, commitment_hex=None, depositor=None):
    cid = 11155111 if chain_id is None else chain_id
    who = ACCOUNT if depositor is None else depositor
    c = "0x" + commitment(deposit_secret(K_AUTH, cid, VAULT, who, j), 1, DEST).hex() if commitment_hex is None else commitment_hex
    return {
        "version": 1, "chainId": cid, "vault": VAULT, "token": ETH if token is None else token,
        "denomIndex": 0, "commitment": c, "depositor": who, "index": j,
    }


# -------------------------------------------------------------------------------- the page driver
INIT_WALLET = r"""
(function(){
  window.__webCalls = [];
  var d = document.createElement('div'); d.id = '__wallet_calls'; d.style.display = 'none';
  function mount(){ document.documentElement.appendChild(d); window.__mounted = true; }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount); else mount();
  window.ethereum = { isMetaMask: true, request: async function (r) {
    window.__webCalls.push(r.method);
    if (window.__mounted) d.dataset.calls = window.__webCalls.join(' ');
    if (r.method === 'eth_requestAccounts' || r.method === 'eth_accounts') return [window.__account];
    if (r.method === 'eth_chainId') return '0xaa36a7';
    var x = await window.__ferrata_rpc(r.method, r.params || []);
    if (x && x.error) { var e = new Error(x.error.message || 'wallet error'); e.code = x.error.code; e.data = x.error.data; throw e; }
    return x.result;
  } };
})();
"""

reject_send = {"on": False}


def rpc_binding(source, method, params):
    if method == "eth_sendTransaction" and reject_send["on"]:
        return {"error": {"code": 4001, "message": "User rejected the request"}}
    try:
        return {"result": rpc(method, params)}
    except Exception as e:  # the wallet reports JSON-RPC errors the way anvil does
        m = re.sub(r"^RuntimeError: ", "", str(e))
        return {"error": {"code": -32000, "message": m}}


def wait_until(fn, what, timeout=90):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        last = fn()
        if last:
            return last
        time.sleep(0.25)
    fail(f"timeout waiting for: {what} (last: {last!r})")


class Page:
    def __init__(self, pw_page):
        self.p = pw_page
        self.errors = []

    def text(self, sel):
        return (self.p.locator(sel).text_content() or "").strip()

    def calls(self):
        raw = self.p.locator("#__wallet_calls").get_attribute("data-calls") or ""
        return [m for m in raw.split(" ") if m]

    def fill_ticket(self, t):
        self.p.fill("#ticket", json.dumps(t))

    def connect(self):
        self.p.click("#connect")
        wait_until(lambda: "Connected:" in self.text("#wnote"), "the connect line")

    def check(self):
        self.p.click("#check")
        wait_until(lambda: self.text("#status").startswith(("checked:", "refused")) or self.text("#refusal"),
                   "the check to finish")

    def refusal(self):
        return self.text("#refusal")

    def summary(self):
        return self.text("#summary")

    def send_approve(self):
        self.p.click("#sendapprove")
        wait_until(lambda: "the approve is in block" in self.text("#status") or self.refusal(),
                   "the approve receipt")

    def send_deposit(self):
        self.p.click("#senddeposit")
        wait_until(lambda: self.text("#status").startswith("done:") or self.refusal(),
                   "the deposit receipt")

    def assert_refused(self, needle, name):
        r = self.refusal()
        check(needle in r, f"{name}: the page refused ({needle!r} shown)")
        check("refused" in r or "failed" in r, f"{name}: the refusal says nothing was sent")
        return r


def main():
    RESULTS.mkdir(parents=True, exist_ok=True)
    chain_id = int(rpc("eth_chainId", []), 16)
    accounts = rpc("eth_accounts", [])
    log("# web/depositor: the deposit page in headless Chromium, against a local anvil (chain id " + str(chain_id) + ")")
    log("# run of 21 November 2026; page web/depositor/index.html opened from file://")
    log("# vault " + VAULT + ", test token " + TOKEN + ", the wallet answers for " + ACCOUNT)
    assert chain_id == 11155111, "this test runs against the anvil run.sh starts (chain id 11155111)"
    assert ACCOUNT == accounts[1].lower(), "FERRATA_ACCOUNT must be anvil's account 1"

    # ---- the Python derivations reproduce the bridge's own vectors first
    cases = json.loads(VECTORS.read_text())["cases"]
    for c in cases:
        s = deposit_secret(bytes.fromhex(c["k_auth"][2:]), int(c["evm_chain_id"], 16), c["vault"], c["depositor"], int(c["j"]))
        assert "0x" + s.hex() == c["s"], "s does not reproduce vector j=" + c["j"]
        C = commitment(s, c["kind"], bytes.fromhex(c["dest"][2:]))
        assert "0x" + C.hex() == c["C"], "C does not reproduce vector j=" + c["j"]
        assert hashlib.sha256(bytes.fromhex(c["C_preimage"][2:])).digest() == C, "the vector's C_preimage disagrees"
    log(f"# the s and C derivations reproduce all {len(cases)} cases of bridge/vectors/inbound-v2.json")

    with sync_playwright() as sw:
        browser = sw.chromium.launch()
        page = browser.new_page(viewport={"width": 1200, "height": 900})
        page.expose_binding("__ferrata_rpc", rpc_binding)
        page.add_init_script(INIT_WALLET.replace("window.__account", json.dumps(ACCOUNT)))
        requests, errors = [], []
        page.on("request", lambda r: requests.append(r.url))
        page.on("pageerror", lambda e: errors.append("page error: " + str(e)))
        page.on("console", lambda m: errors.append("console: " + m.text) if m.type == "error" else None)
        pg = Page(page)
        page.goto(PAGE)
        log("# " + browser.version + ", page opened from file://")

        # ---- (a) the ETH happy path, anvil's account 1 at index 0
        log("\n(a) ETH deposit, index 0")
        bal0, count0 = eth_balance(VAULT), deposit_count(ACCOUNT)
        check(bal0 == 0 and count0 == 0, "the vault starts with no ETH and the depositor's count is 0")
        ta = ticket(0)
        pg.fill_ticket(ta)
        pg.connect()
        pg.check()
        check(pg.refusal() == "", "a: no refusal")
        s = pg.summary()
        expected = "0x" + SELECTOR_DEPOSIT_ETH + word(0) + ta["commitment"][2:] + word(0)
        check(expected in s, "a: the summary shows the exact depositETH calldata")
        check(("value 0x" + hex(ETH_TOTAL)[2:]) in s, "a: the summary shows the exact ETH value")
        check(ta["commitment"] in s and f"{ETH_TOTAL} base units" in s, "a: the summary shows C and the total")
        check("stays private" in s and "public on Ethereum" in s, "a: the summary states what is public and what is private")
        page.screenshot(path=str(OUT_PNG))
        size = OUT_PNG.stat().st_size
        check(size < 300_000, f"a: the summary screenshot is small ({size} bytes)")
        pg.send_deposit()
        check(pg.text("#result").startswith("the deposit is in: leaf index 0"), "a: the page shows leaf index 0")
        check("match the ticket (checked)" in pg.text("#result"), "a: the page checked the logged commitment and depositor")
        logs = deposit_logs()
        check(len(logs) == 1, "a: exactly one Deposit event on chain")
        d = logs[0]
        check(d["commitment"] == ta["commitment"] and d["depositor"] == ACCOUNT, "a: the event carries the ticket's C and depositor")
        check(d["leafIndex"] == 0 and d["j"] == 0 and d["amount"] == 100_000_000_000_000_000 and d["token"] == ETH,
              "a: the event's leaf index, j, denomination and token are right")
        check(deposit_count(ACCOUNT) == 1, "a: depositCount is 1")
        check(eth_balance(VAULT) == ETH_TOTAL, "a: the vault's balance is up by the exact total")
        rc = rpc("eth_getTransactionReceipt", [d["tx"]])
        check(rc["status"] == "0x1" and rc["from"].lower() == ACCOUNT and rc["to"].lower() == VAULT, "a: the receipt is a success from the depositor to the vault")
        log("    deposit tx " + d["tx"])

        # ---- (b) the ERC-20 happy path: approve the exact total, then deposit at index 1
        log("\n(b) ERC-20 deposit, approve then deposit, index 1")
        tok_bal0, vault_tok0 = token_balance(ACCOUNT), token_balance(VAULT)
        check(tok_bal0 == 1_000_000_000 and allowance(ACCOUNT, VAULT) == 0, "b: the depositor was minted test tokens and the vault has no allowance")
        tb = ticket(1, token=TOKEN)
        pg.fill_ticket(tb)
        pg.check()
        check(pg.refusal() == "", "b: no refusal")
        s = pg.summary()
        approve_data = "0x" + SELECTOR_APPROVE + word(int(VAULT, 16)) + word(TOKEN_TOTAL)
        deposit_data = "0x" + SELECTOR_DEPOSIT + word(int(TOKEN, 16)) + word(0) + tb["commitment"][2:] + word(1)
        check(approve_data in s, "b: the summary shows the exact approve calldata (the total, not unlimited)")
        check(deposit_data in s, "b: the summary shows the exact deposit calldata")
        check(page.locator("#sendapprove").is_visible(), "b: the page asks to approve first")
        pg.send_approve()
        check(allowance(ACCOUNT, VAULT) == TOKEN_TOTAL, "b: the allowance is exactly the total")
        check(page.locator("#senddeposit").is_visible(), "b: the deposit can now be sent")
        pg.send_deposit()
        check("leaf index 1" in pg.text("#result"), "b: the page shows leaf index 1")
        logs = deposit_logs()
        d = logs[1]
        check(d["commitment"] == tb["commitment"] and d["j"] == 1 and d["leafIndex"] == 1, "b: the event carries this ticket's C at j=1, leaf 1")
        check(d["amount"] == 1_000_000 and d["token"] == TOKEN, "b: the event's denomination and token are right")
        check(deposit_count(ACCOUNT) == 2, "b: depositCount is 2")
        check(token_balance(ACCOUNT) == tok_bal0 - TOKEN_TOTAL and token_balance(VAULT) == vault_tok0 + TOKEN_TOTAL,
              "b: the depositor paid and the vault escrowed the total")
        check(allowance(ACCOUNT, VAULT) == 0, "b: the deposit spent the allowance down to zero (the approve was for the exact total)")
        log("    deposit tx " + d["tx"])

        # ---- (c) a wrong chain id in the ticket: refused, nothing sent
        log("\n(c) wrong chain id in the ticket")
        sends_before = len([m for m in pg.calls() if m == "eth_sendTransaction"])
        pg.fill_ticket(ticket(2, chain_id=999))
        pg.check()
        pg.assert_refused("chain", "c")
        sends = [m for m in pg.calls() if m == "eth_sendTransaction"]
        check(len(sends) == sends_before, "c: no eth_sendTransaction was called")

        # ---- (d) the ticket's depositor is not the connected account: refused
        log("\n(d) ticket depositor is not the connected account")
        pg.fill_ticket(ticket(0, depositor=accounts[2].lower()))
        pg.check()
        pg.assert_refused("depositor", "d")
        check(len([m for m in pg.calls() if m == "eth_sendTransaction"]) == sends_before, "d: no eth_sendTransaction was called")

        # ---- (e) a stale index: refused
        log("\n(e) stale index (already used)")
        pg.fill_ticket(ticket(0))  # j = 0 was used by (a); the vault's count is 2 now
        pg.check()
        pg.assert_refused("stale", "e")
        check(len([m for m in pg.calls() if m == "eth_sendTransaction"]) == sends_before, "e: no eth_sendTransaction was called")

        # ---- (f) a zero commitment: refused (this one through the fields, not the JSON)
        log("\n(f) zero commitment (typed one field at a time)")
        t_f = ticket(2)
        page.fill("#ticket", "")
        for key, sel in [("chainId", "#t-chainid"), ("vault", "#t-vault"), ("token", "#t-token"),
                         ("denomIndex", "#t-denom"), ("depositor", "#t-depositor"), ("index", "#t-index")]:
            page.fill(sel, str(t_f[key]))
        page.fill("#t-commitment", "0x" + "00" * 32)
        pg.check()
        pg.assert_refused("zero", "f")
        check(len([m for m in pg.calls() if m == "eth_sendTransaction"]) == sends_before, "f: no eth_sendTransaction was called")

        # ---- (g) an unregistered token: refused
        log("\n(g) unregistered token")
        pg.fill_ticket(ticket(2, token="0x" + "12" * 20))
        pg.check()
        pg.assert_refused("not registered", "g")
        check(len([m for m in pg.calls() if m == "eth_sendTransaction"]) == sends_before, "g: no eth_sendTransaction was called")

        # ---- (h) the wallet rejects the send (EIP-1193 code 4001): shown, nothing further sent
        log("\n(h) the wallet rejects the deposit")
        rejects_before = len([m for m in pg.calls() if m == "eth_sendTransaction"])
        pg.fill_ticket(ticket(2))
        pg.check()
        check(pg.refusal() == "", "h: the checks passed (the refusal comes from the wallet)")
        reject_send["on"] = True
        pg.send_deposit()
        pg.assert_refused("4001", "h")
        time.sleep(1.5)
        rejects_now = len([m for m in pg.calls() if m == "eth_sendTransaction"])
        check(rejects_now == rejects_before + 1, "h: exactly one eth_sendTransaction, retried nothing")
        check(not page.locator("#senddeposit").is_visible(), "h: the send button is gone; nothing more can be sent")
        check(deposit_count(ACCOUNT) == 2, "h: the vault still holds 2 deposits")
        reject_send["on"] = False

        # ---- (i) the page's hygiene: no request of its own, only allowed wallet methods
        log("\n(i) the page's own traffic and the wallet methods")
        origins = sorted({"file:" if u.startswith("file:") else u.split("/")[0] + "//" + u.split("/")[2] for u in requests})
        check(origins == ["file:"], f"i: the page requested nothing but its own file:// URL (origins: {origins})")
        methods = sorted(set(pg.calls()))
        check(all(m in ALLOWED_METHODS for m in methods), f"i: only allowed wallet methods were called ({', '.join(methods)})")
        check(errors == [], f"i: no script or console errors ({errors[:2]})")
        refused = page.evaluate(
            "async () => { try { await FerrataDepositor.walletClient(window.ethereum).request('eth_getLogs', [{}]); return 'NOT REFUSED'; }"
            " catch (e) { return String(e.message); } }")
        check(refused.startswith("refused: eth_getLogs"), "i: the wallet client refuses a method outside its allowlist")
        log("    wallet methods called: " + ", ".join(methods))

        browser.close()

    log("\nall 9 case groups passed")
    OUT_TXT.write_text("\n".join(log_lines) + "\n")
    print(f"\nwrote web/depositor/results/{OUT_TXT.name}")
    print(f"wrote web/depositor/results/{OUT_PNG.name} ({OUT_PNG.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        log("\nFAILED: " + repr(e))
        try:
            OUT_TXT.write_text("\n".join(log_lines) + "\n")
        except Exception:
            pass
        OUT_PNG.parent.mkdir(parents=True, exist_ok=True)
        print(f"partial output in web/depositor/results/{OUT_TXT.name}", file=sys.stderr)
        raise
