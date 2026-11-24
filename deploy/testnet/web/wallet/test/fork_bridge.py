#!/usr/bin/env python3
"""The bridge tab against the deployed Sepolia vault, on a local fork: anvil forks Sepolia (chain id 11155111,
loopback only) so the page talks to the real FerrataVault and test token at their real addresses, with their real
denominations, fees and caps, and nothing reaches Sepolia. A stand-in EIP-1193 wallet (as test/e2e_test.py's) signs
with anvil's first unlocked account. The LEZ side is not needed: /rpc points at a closed port.

    python3 -I test/fork_bridge.py --fork-url <Sepolia RPC> --deployment vault/deployments/sepolia-31-B.json --out DIR

Checks, each against the fork's own state as well as the page: an ETH deposit to the wallet's public account (C equal
to SPEC-v2 recomputed here, the Deposit event, the vault's ETH balance); the test token: mint, the exact approval, the
deposit (event names the token, the vault's token balance grows by the total, the allowance goes back to zero); the
deposit index j counts both.
"""
import argparse
import hashlib
import hmac
import json
import os
import re
import subprocess
import sys
import time
import urllib.request

from playwright.sync_api import sync_playwright

HERE = os.path.dirname(os.path.abspath(__file__))
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
PASS = "correct horse battery staple"
DEPOSITOR = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"  # anvil's first dev account (unlocked on the fork)
ZERO = "0x" + "0" * 40
checks, numbers = [], {}


def check(name, ok, detail=None):
    checks.append({"name": name, "ok": bool(ok), "detail": None if ok else str(detail)[:400]})
    print(("ok    " if ok else "FAIL  ") + name + ("" if ok else f": {str(detail)[:300]}"), flush=True)


def b58decode(s):
    n = 0
    for ch in s:
        n = n * 58 + B58.index(ch)
    return n.to_bytes(32, "big")


def expected_commitment(words, chain_id, vault, depositor, j, kind, dest32):
    seed64 = hashlib.pbkdf2_hmac("sha512", " ".join(words).encode(), b"mnemonic", 2048)
    hs = lambda k, m: hmac.new(k, m, hashlib.sha256).digest()  # noqa: E731
    k_auth = hs(hs(seed64, b"ferrata/bridge-seed/v1"), b"ferrata/auth/v1")
    s_ = hs(k_auth, b"ferrata/s/v1" + chain_id.to_bytes(32, "big") + bytes.fromhex(vault[2:]) + bytes.fromhex(depositor[2:]) + j.to_bytes(8, "big"))
    return "0x" + hashlib.sha256(b"ferrata/C/v1" + s_ + bytes([kind]) + dest32).hexdigest()


STAND_IN = """
window.ethereum = { isStandIn: true, async request({ method, params }) {
  if (method === 'eth_requestAccounts' || method === 'eth_accounts') return [DEPOSITOR];
  if (method === 'eth_chainId') return '0xaa36a7';
  const r = await window.__walletRpc(method, params || []);
  if (r.error) { const e = new Error(r.error.message); e.code = r.error.code; throw e; }
  return r.result;
} };
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fork-url", required=True)
    ap.add_argument("--deployment", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--config-id", required=True, help="the bridge configuration id the LEZ side holds (bridge.configId)")
    ap.add_argument("--anvil-port", type=int, default=18745)
    ap.add_argument("--port", type=int, default=18751)
    a = ap.parse_args()
    dep = json.load(open(a.deployment))
    vault, token = dep["vault"].lower(), dep["token"].lower()
    os.makedirs(a.out, exist_ok=True)
    anvil_url = f"http://127.0.0.1:{a.anvil_port}"

    def eth(method, params):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        with urllib.request.urlopen(urllib.request.Request(anvil_url, data=body, headers={"Content-Type": "application/json"}), timeout=60) as r:
            return json.loads(r.read())

    def call(to, data):
        return int(eth("eth_call", [{"to": to, "data": data}, "latest"])["result"], 16)

    BAL = "0x70a08231"  # balanceOf(address)
    ALW = "0xdd62ed3e"  # allowance(address,address)
    pad = lambda x: x[2:].rjust(64, "0")  # noqa: E731

    cfg = json.load(open(os.path.join(os.path.dirname(HERE), "config.json")))
    cfg["network"] = "local stand-in; Sepolia fork"
    cfg["bridge"].update({"vault": dep["vault"], "evmChainId": 11155111,
                          "configId": a.config_id, "lezReady": True,
                          "token": {"address": dep["token"], "symbol": "TUSD", "decimals": 6, "testMint": True}})
    cfg_path = os.path.join(a.out, "config.json")
    json.dump(cfg, open(cfg_path, "w"), indent=1)

    procs = []
    try:
        # A few blocks behind the endpoint's head: a load-balanced public endpoint may not yet serve its newest block.
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "eth_blockNumber", "params": []}).encode()
        with urllib.request.urlopen(urllib.request.Request(a.fork_url, data=body, headers={"Content-Type": "application/json", "User-Agent": "curl"}), timeout=30) as r:
            fork_at = int(json.loads(r.read())["result"], 16) - 5
        procs.append(subprocess.Popen(["anvil", "--fork-url", a.fork_url, "--fork-block-number", str(fork_at), "--chain-id", "11155111", "--host", "127.0.0.1",
                                       "--port", str(a.anvil_port)], stdout=open(os.path.join(a.out, "anvil.log"), "w"), stderr=subprocess.STDOUT))
        procs.append(subprocess.Popen([sys.executable, "-I", os.path.join(HERE, "serve.py"), "--port", str(a.port), "--rpc", "http://127.0.0.1:9",
                                       "--config", cfg_path], stdout=open(os.path.join(a.out, "serve.log"), "w"), stderr=subprocess.STDOUT))
        for _ in range(120):
            try:
                eth("eth_chainId", []); break
            except Exception:  # noqa: BLE001
                time.sleep(0.5)
        numbers["fork_block"] = int(eth("eth_blockNumber", [])["result"], 16)
        check("anvil forks Sepolia with the vault's code", len(eth("eth_getCode", [vault, "latest"])["result"]) > 2)
        url = f"http://127.0.0.1:{a.port}/"
        with sync_playwright() as p:
            browser = p.chromium.launch()
            ctx = browser.new_context(viewport={"width": 1280, "height": 1000})
            ctx.expose_function("__walletRpc", eth)
            ctx.add_init_script(STAND_IN.replace("DEPOSITOR", json.dumps(DEPOSITOR)))
            pg = ctx.new_page()
            # First as the network serves it before the LEZ side is set up (lezReady false), and with a configId that
            # is not the vault's: both refused before anything is signed. Then the real configuration.
            closed = dict(cfg, bridge=dict(cfg["bridge"], lezReady=False))
            wrong = dict(cfg, bridge=dict(cfg["bridge"], configId="0x" + "11" * 32))
            served = {"cfg": closed}
            pg.route("**/config.json", lambda route: route.fulfill(status=200, content_type="application/json", body=json.dumps(served["cfg"])))
            pg.goto(url)
            pg.click("#go-create")
            words = [w.strip() for w in pg.locator("#mnemonic-words li").all_text_contents()]
            pg.fill("#create-pass", PASS); pg.fill("#create-pass2", PASS)
            pg.check("#saved-words"); pg.click("#create-go")
            pg.wait_for_selector("#v-main:not([hidden])", timeout=30000)
            id1 = pg.locator("#home-id").inner_text().strip()
            check("a wallet is created from a new phrase", len(words) == 24 and re.fullmatch("[1-9A-HJ-NP-Za-km-z]{43,44}", id1), id1)
            txt = lambda s: pg.locator(s).inner_text()  # noqa: E731
            wait_out = lambda pat, t=60000: pg.wait_for_function(  # noqa: E731
                f"() => /{pat}/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=t)

            def reopen(conf):
                served["cfg"] = conf
                pg.goto(url + "#bridge"); pg.reload()
                pg.wait_for_selector("#v-unlock:not([hidden])", timeout=15000)
                pg.fill("#unlock-pass", PASS); pg.click("#unlock-go")
                pg.wait_for_selector("#v-main:not([hidden])", timeout=30000)
                pg.goto(url + "#bridge")

            pg.goto(url + "#bridge")
            check("lezReady false: the tab says the LEZ side is still being set up", pg.locator("[data-testid=bridge-not-ready]").is_visible())
            pg.click("[data-testid=bridge-review]")
            wait_out("not open yet", 30000)
            check("lezReady false: the review is refused, nothing to sign", pg.locator("[data-testid=bridge-go]").is_disabled(), txt("[data-testid=bridge-out]"))
            reopen(wrong)
            pg.click("[data-testid=bridge-connect]")
            wait_out("lezConfigId", 30000)
            check("a configId that is not the vault's lezConfigId is refused on connect", "nothing will be sent" in txt("[data-testid=bridge-out]"), txt("[data-testid=bridge-out]"))
            reopen(cfg)
            check("lezReady true: no set-up notice", pg.locator("[data-testid=bridge-not-ready]").count() == 0)

            # -- ETH, to the public account
            pg.click("[data-testid=bridge-connect]")
            pg.wait_for_function("() => /Connected/.test(document.querySelector('[data-testid=bridge-status]').textContent) || document.querySelector('[data-testid=bridge-out]').textContent", timeout=30000)
            st = txt("[data-testid=bridge-status]")
            check("ETH: the page reads the deployed vault (j = 0)", "j = 0" in st, st + txt("[data-testid=bridge-out]"))
            opts = pg.locator("[data-testid=bridge-denom] option").all_text_contents()
            numbers["eth_options"] = opts
            check("ETH: the vault's three denominations with their fees", len(opts) == 3 and opts[0].startswith("0.001 ETH (+ fees 0.000002 ETH)"), opts)
            eth_before = int(eth("eth_getBalance", [vault, "latest"])["result"], 16)
            pg.select_option("[data-testid=bridge-kind]", "public")
            pg.click("[data-testid=bridge-review]")
            pg.wait_for_function("() => /Commitment C/.test(document.querySelector('[data-testid=bridge-summary]').textContent) || /rror|No |not|less/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=30000)
            m = re.search(r"Commitment C\s*(0x[0-9a-f]{64})", txt("[data-testid=bridge-summary]"))
            want = expected_commitment(words, 11155111, vault, DEPOSITOR, 0, 2, b58decode(id1))
            check("ETH: C is SPEC-v2 for (chain, vault, depositor, j = 0, public, the account)", m and m.group(1) == want, (m and m.group(1), want, txt("[data-testid=bridge-out]")))
            pg.screenshot(path=os.path.join(a.out, "01-eth-review.png"), full_page=True)
            pg.click("[data-testid=bridge-go]")
            wait_out("Deposited|failed|rror|changed|short")
            res = txt("[data-testid=bridge-out]")
            check("ETH: the deposit is mined and its Deposit event checked by the page", res.startswith("Deposited: leaf"), res)
            gained = int(eth("eth_getBalance", [vault, "latest"])["result"], 16) - eth_before
            numbers["eth_vault_gain_wei"] = gained
            check("ETH: the vault holds 0.001 ETH + 0.000002 ETH relay fee more", gained == 10**15 + 2 * 10**12, gained)

            # -- the test token
            pg.select_option("[data-testid=bridge-asset]", "1")
            pg.wait_for_function("() => /TUSD balance/.test(document.querySelector('[data-testid=bridge-status]').textContent) || document.querySelector('[data-testid=bridge-out]').textContent", timeout=30000)
            st = txt("[data-testid=bridge-status]")
            check("TUSD: the page reads the token's denominations and the balance (j = 1, balance 0)", "j = 1" in st and "balance: 0 TUSD" in st, st + txt("[data-testid=bridge-out]"))
            opts = pg.locator("[data-testid=bridge-denom] option").all_text_contents()
            numbers["tusd_options"] = opts
            check("TUSD: 100, 1000, 10000 TUSD with their fees", len(opts) == 3 and opts[0].startswith("100 TUSD (+ fees 0.25 TUSD)"), opts)
            pg.click("[data-testid=bridge-review]")
            wait_out("less than", 30000)
            check("TUSD: a deposit above the balance is refused before anything is signed", pg.locator("[data-testid=bridge-go]").is_disabled(), txt("[data-testid=bridge-out]"))
            pg.click("[data-testid=bridge-mint]")
            wait_out("Minted|rror|failed|not mined", 60000)
            bal = call(token, BAL + pad(DEPOSITOR))
            check("TUSD: the mint button mints 100.25 TUSD to the browser wallet's address", bal == 100_250_000, (bal, txt("[data-testid=bridge-out]")))
            tok_before = call(token, BAL + pad(vault))
            pg.select_option("[data-testid=bridge-kind]", "public")
            pg.click("[data-testid=bridge-review]")
            pg.wait_for_function("() => /Commitment C/.test(document.querySelector('[data-testid=bridge-summary]').textContent) || /rror|less/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=30000)
            summ = txt("[data-testid=bridge-summary]")
            m = re.search(r"Commitment C\s*(0x[0-9a-f]{64})", summ)
            want = expected_commitment(words, 11155111, vault, DEPOSITOR, 1, 2, b58decode(id1))
            check("TUSD: C is SPEC-v2 for j = 1", m and m.group(1) == want, (m and m.group(1), want))
            check("TUSD: the review names the exact approval (100.25 TUSD) before the deposit", "approving the vault for exactly 100.25 TUSD" in summ, summ[:400])
            pg.screenshot(path=os.path.join(a.out, "02-tusd-review.png"), full_page=True)
            pg.click("[data-testid=bridge-go]")
            wait_out("Deposited|failed|rror|changed|short", 120000)
            res = txt("[data-testid=bridge-out]")
            check("TUSD: approval then deposit mined, the Deposit event checked by the page (asset included)", res.startswith("Deposited: leaf"), res)
            gained = call(token, BAL + pad(vault)) - tok_before
            numbers["tusd_vault_gain"] = gained
            check("TUSD: the vault holds 100.25 TUSD more", gained == 100_250_000, gained)
            check("TUSD: the allowance is back to zero (the approval was exact)", call(token, ALW + pad(DEPOSITOR) + pad(vault)) == 0)
            logs = eth("eth_getLogs", [{"address": vault, "fromBlock": hex(numbers["fork_block"]), "toBlock": "latest"}])["result"]
            mine = [l for l in logs if len(l["topics"]) == 3 and l["topics"][1][-40:] == DEPOSITOR[2:]]
            check("the fork's vault logged two deposits by this depositor: ETH then TUSD",
                  [l["topics"][2][-40:] for l in mine] == [ZERO[2:], token[2:]], [l["topics"] for l in mine])
            lst = txt("[data-testid=bridge-deposits]")
            check("both deposits are in the wallet's list", lst.count("deposited") == 2 and "TUSD" in lst and "ETH" in lst, lst[:300])
            pg.screenshot(path=os.path.join(a.out, "03-deposits.png"), full_page=True)
            browser.close()
    except Exception as e:  # noqa: BLE001
        check("the run completed without an unexpected error", False, repr(e))
    finally:
        for pr in procs:
            pr.terminate()
    passed = sum(c["ok"] for c in checks)
    json.dump({"vault": vault, "token": token, "passed": passed, "failed": len(checks) - passed, "numbers": numbers, "checks": checks},
              open(os.path.join(a.out, "record.json"), "w"), indent=1)
    print(f"{passed} passed, {len(checks) - passed} failed")
    return 0 if passed == len(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
