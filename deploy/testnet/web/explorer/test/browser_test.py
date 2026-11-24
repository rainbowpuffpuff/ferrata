#!/usr/bin/env python3
"""The pages in headless Chromium against the replay RPC (test/replay_rpc.py over test/out/world.json).

    python3 -I deploy/testnet/web/explorer/test/browser_test.py [--url http://127.0.0.1:18750/] [--shots DIR]
         [--live]   (against a real network: only checks that hold on any chain; reads only)

Each check prints `ok` or `FAIL`; any page error or failed same-origin request fails the run. Screenshots of each
page go to --shots (default test/out/shots). Exit 1 on any failure.
"""
import argparse
import json
import re
import sys
from pathlib import Path

from playwright.sync_api import sync_playwright

HERE = Path(__file__).resolve().parent
failures = []


def snap_program_hex(b58s):
    alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n = 0
    for c in b58s:
        n = n * 58 + alphabet.index(c)
    return n.to_bytes(32, "big").hex()


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what, flush=True)
    if not cond:
        failures.append(what)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:18750/")
    ap.add_argument("--shots", default=str(HERE / "out" / "shots"))
    ap.add_argument("--live", action="store_true")
    ap.add_argument("--world", default=str(HERE / "out" / "world.json"))
    ap.add_argument("--name", help="live mode: a registered name whose page must resolve")
    a = ap.parse_args()
    base = a.url if a.url.endswith("/") else a.url + "/"
    shots = Path(a.shots)
    shots.mkdir(parents=True, exist_ok=True)
    world = None if a.live else json.loads(Path(a.world).read_text())
    global INSTANCE
    INSTANCE = None if a.live else json.loads((HERE / "out" / "summary.json").read_text())["instance"]
    cfg = None if a.live else json.loads((HERE / "out" / "config.json").read_text())

    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1360, "height": 900})
        errors = []
        page.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
        # a 429 is the gateway's "slow down", which the page waits out and retries: not an error
        page.on("console", lambda m: m.type == "error" and "status of 429" not in m.text and errors.append(f"console: {m.text}"))
        # ERR_ABORTED is a request cut short by the test's own navigation to the next page; anything else is a failure
        page.on("requestfailed", lambda r: base in r.url and "ERR_ABORTED" not in (r.failure or "") and errors.append(f"requestfailed: {r.url} {r.failure}"))

        def visit(path, wait_text, shot, timeout=20000):
            page.goto(base + path)
            try:
                page.wait_for_function("t => document.body.innerText.includes(t)", arg=wait_text, timeout=timeout)
                ok = True
            except Exception:
                ok = False
            page.screenshot(path=str(shots / f"{shot}.png"), full_page=True)
            check(ok, f"{path}: shows {wait_text!r}")
            return page.inner_text("body")

        def served_head():
            # the RPC the pages read: the config's (absolute in a live run), else the server's own /rpc
            rpc = page.request.get(base + "config.json").json().get("rpc") or "/rpc"
            url = rpc if rpc.startswith("http") else base + rpc.lstrip("/")
            r = page.request.post(url, data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getLastBlockId", "params": []}), headers={"content-type": "application/json"})
            return r.json()["result"]

        # explorer home
        body = visit("explorer/", "Latest activity", "explorer-home")
        page.wait_for_function("() => document.querySelectorAll('ul.feed li').length > 0", timeout=20000)
        body = page.inner_text("body")
        head0 = served_head()
        # a live chain moves on between the page's read and this one: within 30 blocks (5 minutes)
        heads = [int(x.replace(",", "")) for x in re.findall(r"#([\d,]+)", body)]
        check(any(head0 - 30 <= h <= head0 for h in heads) if a.live else f"#{head0:,}" in body, "explorer home: the head block in the stats")
        check(all(k.lower() in body.lower() for k in ["Block height", "Latest block", "Block time", "Final on Logos Blockchain", "User transactions", "Chain id (genesis)"]), "explorer home: stats header (height, latest block, block time, finality on Bedrock, user transactions, chain id)")
        check("hash recomputed ✓" in body, "explorer home: the head's hash recomputed")
        # a block with an attestation call
        if world:
            body = visit("explorer/#/block/30", "Hash check", "explorer-block")
            check("recomputed in this browser: equal" in body, "block page: hash recomputed in the browser")
        # find an attestation transaction in the world and open it
        if world:
            import base64
            att_tx = None
            prog = cfg["programs"][0]["account"]
            for h, (b64, bid) in world["txs"].items():
                raw = base64.b64decode(b64)
                if raw[0] == 0 and len(raw) > 600:
                    att_tx = att_tx or (h, bid)
            # pick a tx whose page shows a decoded ferrata call
            found = False
            for h, (b64, bid) in list(world["txs"].items()):
                raw = base64.b64decode(b64)
                if raw[0] != 0:
                    continue
                page.goto(base + f"explorer/#/tx/{h}")
                try:
                    page.wait_for_function("() => document.body.innerText.includes('Decoded')", timeout=8000)
                except Exception:
                    continue
                t = page.inner_text("body")
                if "Ferrata attestation" in t and re.search(r"\n(open|adopt|finish|miller|init)\b", t):
                    page.screenshot(path=str(shots / "explorer-tx-attestation.png"), full_page=True)
                    found = True
                    check("applied" in t or "reverted" in t, f"tx page {h[:12]}: shows its outcome from the block's reverted list")
                    break
            check(found, "tx page: an attestation program call decoded through the IDL")
            # the twin deployment is not in the config: recognised by its loader header's image id
            body = visit("explorer/#/block/175", "Transactions", "explorer-block-twin")
            page.wait_for_timeout(1500)
            body = page.inner_text("body")
            check("Ferrata attestation (recorded run)" in body, "block page: a program not in the config recognised by its image id (known_images)")
            # accounts
            inst = INSTANCE
            body = visit(f"explorer/#/account/{inst}", "light-client instance", "explorer-account-instance")
            check("11,549,664" in body and "Gloas" in body, "instance account: finalized slot and layout decoded from the store")
            body = visit(f"explorer/#/account/{prog}", "Image id", "explorer-account-program")
            check(cfg["programs"][0]["image_id"] in body, "program account: the loader header's image id")
            # record account link from the instance page
            page.goto(base + f"explorer/#/account/{inst}")
            page.wait_for_function("() => document.body.innerText.includes('Its record')", timeout=15000)
            page.click("text=Its record >> xpath=following-sibling::dd[1]//a")
            page.wait_for_function("() => document.body.innerText.includes('attested header record')", timeout=15000)
            body = page.inner_text("body")
            page.screenshot(path=str(shots / "explorer-account-record.png"), full_page=True)
            check("12,105,936" in body and "5a2f5671d4d93d4e9a188ec55743ae20a86cce5f91e570f2e5dc22b2b34b27b6" in body, "record account: execution block and hash decoded")
            check("eed903288a43d417dad9e91d4bb01bfce99dbb2f8157274e3cc63cb499156657" in body, "record account: beacon block root recomputed (hash_tree_root) equals the beacon API's")
            # search
            page.goto(base + "explorer/")
            page.fill("input[aria-label=search]", "12")
            page.press("input[aria-label=search]", "Enter")
            page.wait_for_function("() => location.hash === '#/block/12'", timeout=10000)
            check(True, "search: a number opens that block")
        # the attestation dashboard
        if world:
            body = visit("status/", "Latest light-client updates", "status")
            check("Sepolia" in body and "Instance account" in body, "status: the instance from programs.json shown (the config names none)")
            check("The operator's view" in body and "height 30,210" in body and "Age of the newest LEZ block final on Bedrock" in body and page.locator("svg.chart-svg").count() == 2, "status: the host's monitor (status.json) and its two 24 h charts")
            page.locator(".chart-wrap").first.scroll_into_view_if_needed()
            box = page.locator(".chart-wrap").first.bounding_box()
            page.mouse.move(box["x"] + box["width"] * 0.5, box["y"] + box["height"] * 0.4)
            check(page.locator(".chart-tip").first.is_visible() and " s" in page.locator(".chart-tip").first.inner_text(), "status: the chart's crosshair tooltip shows a value under the pointer")
            check("1 warning(s)" in body and "providers are behind" in body, "status: 3 epochs behind the (mock) beacon API, whose own finalized checkpoint is days behind the wall clock: one warning")
            check(body.count("equal") >= 2, "status: beacon root and execution block hash equal the (mock) providers'")
            check("11,549,664" in body and "1,409" in body, "status: finalized slot and committee period")
            check(re.search(r"\bfinish\b", body) and re.search(r"\badopt part\b", body), "status: recent updates listed (adopt parts, finish)")
            check(len(re.findall(r"(holds|fails)\s+agrees with the chain", body)) == 3, "status: three attestations re-checked in the browser, each verdict agreeing with the chain's")
            check("slot 0xb = 0x9513ef76177584ee" in body, "status: the attested storage value read from the proof (totalSupply of 0x1c7d…7238)")
            check("Value = 0x1" in body and "fails" in body and "refused" in body, "status: the refused request's failing condition shown")
            # the refused attest's transaction page
            syn = world["synthetic_blocks"]
            page.goto(base + f"explorer/#/block/{syn[1]}")
            page.wait_for_function("() => document.body.innerText.includes('Transactions')", timeout=15000)
            page.click("table >> text=attest >> xpath=ancestor::tr//td[2]//a")
            page.wait_for_function("() => document.body.innerText.includes('Facts, re-checked in this browser')", timeout=20000)
            body = page.inner_text("body")
            page.screenshot(path=str(shots / "explorer-tx-attest-refused.png"), full_page=True)
            check("reverted" in body and "agrees with the chain" in body and "equals the record's block hash" in body, "refused attest tx page: header binds, verdict fails, agrees with the block's reverted list")
        # registry, agents, landing
        body = visit("registry/", "Built into LEZ", "registry")
        check("0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df" in body and "Deploy a program here" in body, "registry: image ids and the deployment guide")
        if world:
            check("deployed: header's image id equals the build" in body, "registry: a configured program's loader header checked live")
            check("IDL: 15 instructions" in body, "registry: the attestation IDL summarised")
        body = visit("agents/", "Every method", "agents")
        check(all(m in body for m in ["getBlockRange", "getAccountView", "sendTransaction", "Minimal scripts", "Never put a private key in a prompt"]), "agents: AGENTS.md rendered with every method, the scripts and the rules")
        r = page.request.get(base + "AGENTS.md")
        check(r.ok and r.text().startswith("# AGENTS.md"), "raw /AGENTS.md served")
        # the landing page's path: the site root, or config.landing where a front page holds the root
        landing = page.request.get(base + "config.json").json().get("landing") or ""
        body = visit(landing, "What this network shows", "landing")
        if world:
            page.wait_for_function("() => document.body.innerText.includes('slot 11,549,664')", timeout=20000)
            body = page.inner_text("body")
            page.screenshot(path=str(shots / "landing.png"), full_page=True)
            check(f"#{served_head():,}" in body and "Sepolia" in body, "landing: live head and the light client's slot")
        if not world:
            # live, read-only: the round-2 pieces on a real chain (no faucet request is made)
            page.goto(base + "explorer/")
            page.wait_for_function("() => document.querySelectorAll('ul.feed li').length > 0 && document.body.innerText.includes('per request')", timeout=60000)
            body = page.inner_text("body").lower()
            check(all(k in body for k in ["block height", "block time", "final on logos blockchain", "user transactions", "chain id (genesis)", "faucet", "per request"]), "live: stats header and faucet terms")
            check(page.locator("ul.feed li.act").count() > 0, "live: the activity feed decodes this chain's transactions")
            page.screenshot(path=str(shots / "live-explorer-home.png"), full_page=True)
            body = visit("explorer/#/features", "LEZ v0.4 on this network", "live-features", timeout=60000)
            page.wait_for_timeout(1500)
            check("latest tick" in page.inner_text("body"), "live: features page with live evidence")
            if a.name:
                body = visit(f"explorer/?name={a.name}", "Resolves to", "live-name", timeout=60000)
                check(f"@{a.name}" in body, f"live: @{a.name} resolves")
            body = visit("explorer/#/messages", "Encrypted messages", "live-messages", timeout=60000)
        if world:
            # round 2: the activity feed, names, messages, features, faucet, reset notice, live refresh
            snap = json.loads((HERE / "live-snapshot.json").read_text())
            page.goto(base + "explorer/")
            page.wait_for_function("() => document.querySelectorAll('ul.feed li.act').length > 5", timeout=30000)
            page.wait_for_timeout(1500)
            body = page.inner_text("body")
            page.screenshot(path=str(shots / "explorer-home-r2.png"), full_page=True)
            for want, what in [("LGO to", "a native transfer"), ("registered @", "a name registered"), ("sent an encrypted message to", "a message"),
                               ("published its messaging key", "a key publication"), ("contacts only", "an inbox policy"), ("Light client:", "a light-client update"),
                               ("attested Ethereum facts", "an attestation")]:
                check(want in body, f"activity feed: {what} in words")
            check("refused" in body, "activity feed: a refused transaction marked")
            # never the content: no 16-byte run of any message body appears anywhere on the home, messages or tx pages
            import base64
            bodies = []
            for t in snap["txs"]:
                raw = base64.b64decode(t["tx"])
                if raw.find(bytes.fromhex(snap_program_hex(snap["programs"]["ferrata_msg"]))) >= 0:
                    bodies.append(raw)
            needles = [b[-64:-48].hex() for b in bodies]
            check(not any(n in body for n in needles), "activity feed: no message content (no envelope bytes on the page)")
            # names: ?name= opens the name's page; the feed and account pages show @names
            nm = "wvuttdupb7"
            body = visit(f"explorer/?name={nm}", "Resolves to", "explorer-name")
            check(f"@{nm}" in body and "live" in body and "registered @" in body, "name page (?name=): record, status, its transactions")
            page.click("text=Resolves to >> xpath=following-sibling::dd[1]//a")
            page.wait_for_function("t => document.body.innerText.includes(t)", arg="its primary name, live and pointing back here", timeout=20000)
            check(True, "account page: the name verified both ways (primary name pointing back)")
            body = visit("explorer/#/search/%40" + nm, "Resolves to", "explorer-name-search")
            check(f"@{nm}" in body, "search: @name opens the name's page")
            # messages
            body = visit("explorer/#/messages", "Encrypted messages", "explorer-messages")
            page.wait_for_timeout(1000)
            body = page.inner_text("body")
            check(body.count("2.0 KiB") >= 4 and "delivered" in body, "messages page: sender, recipient, time and envelope size of each send")
            check(not any(n in body for n in needles), "messages page: no message content")
            # a send's transaction page: the envelope by size only
            send_tx = next(t["hash"] for t in snap["txs"] if b"\x04" and base64.b64decode(t["tx"]).find(bytes.fromhex(snap_program_hex(snap["programs"]["ferrata_msg"]))) >= 0 and len(base64.b64decode(t["tx"])) > 2000)
            body = visit(f"explorer/#/tx/{send_tx}", "Decoded", "explorer-tx-msg")
            check("encrypted envelope (not shown)" in body and not any(n in body for n in needles), "message transaction page: the envelope by size only")
            # an inbox
            inbox_owner = next(k for k, a in snap["accounts"].items() if a and snap["programs"]["ferrata_msg"] in a["data"]["actor_states"] and len(a["data"]["actor_states"][snap["programs"]["ferrata_msg"]]) > 4000)
            body = visit(f"explorer/#/account/{inbox_owner}", "encrypted inbox", "explorer-inbox")
            check("contacts only" in body and "envelope" in body.lower() and not any(n in body for n in needles), "account page: inbox summary (policy, senders, sizes), no content")
            # features
            body = visit("explorer/#/features", "LEZ v0.4 on this network", "explorer-features")
            page.wait_for_timeout(800)
            body = page.inner_text("body")
            for f in ["Clock", "Fee market", "Program loader", "Message passing between programs", "Program events", "Cross-zone messages", "in the code, not active", "latest tick"]:
                check(f in body, f"features page: {f}")
            check(page.locator("table.features a[href*='logos-execution-zone/blob/v0.4.0-rc3/']").count() >= 10, "features page: each feature linked to LEZ v0.4.0-rc3's source")
            # faucet box: by account id, then by @name; the stand-in records what the page sent
            page.goto(base + "explorer/")
            page.wait_for_function("() => document.body.innerText.includes('per request')", timeout=20000)
            target = snap["txs"][0]
            page.fill("input[aria-label='faucet account']", inbox_owner)
            page.click("form.faucet button")
            page.wait_for_function("() => document.querySelector('form.faucet + div').innerText.includes('sent')", timeout=20000)
            page.fill("input[aria-label='faucet account']", "@" + nm)
            page.click("form.faucet button")
            page.wait_for_timeout(2500)
            reqs = page.request.get(base + "__faucet_requests").json()
            owner_of_name = None
            check(len(reqs) == 2 and reqs[0]["account"] == inbox_owner and reqs[1]["account"] and reqs[1]["account"] != inbox_owner, "faucet box: requests sent for an account id and for @name (resolved to its account)")
            page.screenshot(path=str(shots / "explorer-faucet.png"), full_page=False)
            # live refresh: one block held back by the replay is revealed; the feed shows it within a refresh
            n_before = page.locator("ul.feed li.act").count()
            page.request.post(base + "__advance")
            page.wait_for_function("() => document.querySelector('ul.feed li.fresh') !== null", timeout=25000)
            check(page.locator("ul.feed li.fresh").count() >= 1 and "set its primary name to" in page.locator("ul.feed li.fresh").first.inner_text(), "activity feed: a new block's transaction (a primary name set) appears without reloading")
            # reset notice
            page.evaluate("() => localStorage.setItem('ferrata-explorer-genesis', JSON.stringify({id: '00'.repeat(32), seen: 1}))")
            page.goto(base + "explorer/?reset-test=1")
            page.wait_for_function("() => document.body.innerText.includes('was reset since your last visit')", timeout=20000)
            check(True, "reset notice: a different genesis than the last visit's is announced")
            page.screenshot(path=str(shots / "explorer-reset.png"), full_page=False)
        check(not errors, "no page errors or failed requests" + ("" if not errors else f": {errors[:5]}"))
        browser.close()
    if failures:
        print(f"{len(failures)} failure(s)")
        sys.exit(1)
    print("all checks passed")


if __name__ == "__main__":
    main()
