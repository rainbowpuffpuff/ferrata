#!/usr/bin/env python3
"""Notifications without push, on the public test network: two wallets in two browsers (separate profiles), the bundle
served by test/pwa_serve.py with /rpc and /faucet passed to the network (or the deployed page itself with --deployed).
Sends on the test network: one faucet claim (A), a transfer A -> B, A's messaging key, a message B -> A, a transfer
B -> A.

    python3 -I test/pwa_live.py --out DIR --upstream https://HOST [--deployed]

A stays on its Home tab with notifications allowed. Checks: B's message raises A's Inbox badge, an on-screen line and a
notification that names no text or sender; A opens the Inbox and reads the text decrypted in its page; B's transfer
raises "Received 1 LGO" on A. Times are from B's "done in block" to A's badge or notification (the page follows the
chain every 15 s on screen; blocks every 10 s on this network). Writes DIR/record.json.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pwa_e2e import HERE, UA, Record, wait_js  # noqa: E402
from playwright.sync_api import sync_playwright  # noqa: E402

PASS = "correct horse battery 2"
TEXT = "hello from B: pwa_live"


def notes(page):
    return page.evaluate("navigator.serviceWorker.getRegistration().then((r) => r.getNotifications()).then((ns) => ns.map((n) => n.title + ': ' + n.body))")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--deployed", action="store_true", help="use HOST/wallet/ itself, not the local bundle")
    ap.add_argument("--port", type=int, default=18662)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    rec = Record(a.out)
    server = None
    if a.deployed:
        base = a.upstream.rstrip("/")
    else:
        base = f"http://localhost:{a.port}"
        cfg = os.path.join(a.out, "config.json")
        with urllib.request.urlopen(a.upstream.rstrip("/") + "/wallet/config.json", timeout=30) as r, open(cfg, "wb") as f:
            f.write(r.read())
        server = subprocess.Popen([sys.executable, "-I", os.path.join(HERE, "pwa_serve.py"), "--port", str(a.port),
                                   "--upstream", a.upstream, "--config", cfg])
        time.sleep(1.0)
    times, errors = {}, []
    try:
        with sync_playwright() as p:
            def browser():
                ctx = p.chromium.launch_persistent_context(
                    tempfile.mkdtemp(prefix="live-", dir=os.environ.get("TMPDIR")), channel="chromium", base_url=base,
                    viewport={"width": 412, "height": 915}, device_scale_factor=2.625, is_mobile=True, has_touch=True, user_agent=UA)
                ctx.grant_permissions(["notifications"], origin=base)
                page = ctx.pages[0] if ctx.pages else ctx.new_page()
                page.on("pageerror", lambda e: errors.append(str(e)))
                return ctx, page

            def create(page):
                page.goto("/wallet/")
                page.wait_for_selector("#v-welcome", state="visible", timeout=30000)
                page.click("#go-create")
                page.check("#saved-words")
                page.fill("#create-pass", PASS)
                page.fill("#create-pass2", PASS)
                page.click("#create-go")
                page.wait_for_selector("#v-main", state="visible", timeout=30000)
                page.goto("/wallet/#receive")
                page.wait_for_selector("#recv-qr svg", timeout=15000)
                return page.text_content("#home-id"), page.text_content("#recv-code")

            def send(page, to, amount):
                page.goto("/wallet/#send")
                page.fill("#send-to", to)
                page.fill("#send-amount", amount)
                page.click("#send-review")
                page.wait_for_selector("#send-confirm", state="visible", timeout=30000)
                page.click("#send-go")
                wait_js(page, "/applied|reverted|Not in a block/.test(document.getElementById('send-result').textContent)", 240000)
                return page.text_content("#send-result")

            ca, A = browser()
            cb, B = browser()
            a_id, a_code = create(A)
            b_id, b_code = create(B)
            rec.check("two wallets created", a_id != b_id, f"A {a_id}, B {b_id}")

            A.goto("/wallet/#home")
            A.click("#faucet")
            wait_js(A, "/Received|not arrived|Faucet|HTTP/.test(document.getElementById('faucet-msg').textContent)", 180000)
            fm = A.text_content("#faucet-msg")
            if not rec.check("faucet funds A", "Received" in fm, fm):
                raise SystemExit(rec.save({"times": times}) or 1)
            r = send(A, b_code, "3")
            rec.check("A sends 3 LGO to B's receive code (B new on LEZ)", "applied" in r, r[:120])

            A.goto("/wallet/#inbox")
            A.wait_for_selector("[data-testid=inbox-publish]", state="visible", timeout=30000)
            A.click("[data-testid=inbox-publish]")
            wait_js(A, "/done in block|refused|not in a block/.test(document.querySelector('[data-testid=inbox-out]').textContent)", 240000)
            rec.check("A publishes its messaging key", "done in block" in A.text_content("[data-testid=inbox-out]"), A.text_content("[data-testid=inbox-out]")[:100])
            A.goto("/wallet/#settings")
            A.check("[data-testid=notify-on]")
            wait_js(A, "document.querySelector('[data-testid=notify-state]')?.textContent.includes('granted')")
            A.goto("/wallet/#home")
            before = len(notes(A))

            B.goto("/wallet/#inbox")
            B.fill("[data-testid=inbox-to]", a_id)
            B.fill("[data-testid=inbox-text]", TEXT)
            B.click("[data-testid=inbox-send]")
            wait_js(B, "/done in block|refused|Not sent|not in a block/.test(document.querySelector('[data-testid=inbox-out]').textContent)", 240000)
            bo = B.text_content("[data-testid=inbox-out]")
            t_sent = time.time()
            rec.check("B sends A an encrypted message", "done in block" in bo, bo[:100])
            wait_js(A, "!document.getElementById('inbox-badge').hidden", 120000)
            times["message_to_badge_s"] = round(time.time() - t_sent, 1)
            rec.check("A's Inbox badge counts it (A on Home)", A.text_content("#inbox-badge") == "1", f"{times['message_to_badge_s']} s after B's block")
            wait_js(A, f"navigator.serviceWorker.getRegistration().then((r) => r.getNotifications()).then((ns) => ns.length > {before})", 30000)
            ns = notes(A)
            rec.check("A gets a notification naming no text and no sender", any("New encrypted message on Account 1" in n for n in ns)
                      and not any(TEXT in n or b_id in n for n in ns), " | ".join(ns))
            rec.check("A sees an on-screen line", "encrypted message" in (A.text_content("#toast") or ""), A.text_content("#toast"))
            A.goto("/wallet/#inbox")
            wait_js(A, f"document.querySelector('[data-testid=inbox-list]').textContent.includes({json.dumps(TEXT)})", 60000)
            wait_js(A, "document.getElementById('inbox-badge').hidden", 30000)
            rec.check("A reads the text, decrypted in its page; the badge clears", True)
            A.goto("/wallet/#home")
            before = len(notes(A))

            r = send(B, a_code, "1")
            t_sent = time.time()
            rec.check("B sends A 1 LGO", "applied" in r, r[:120])
            wait_js(A, f"navigator.serviceWorker.getRegistration().then((r) => r.getNotifications()).then((ns) => ns.length > {before})", 120000)
            times["transfer_to_notification_s"] = round(time.time() - t_sent, 1)
            ns = notes(A)
            rec.check("A is notified: Received 1 LGO", any("Received 1 LGO on Account 1" in n for n in ns), " | ".join(ns))
            A.screenshot(path=os.path.join(a.out, "a-home.png"))
            rec.check("no script errors", not errors, " | ".join(errors[:3]))
            ca.close()
            cb.close()
    finally:
        if server:
            server.terminate()
    ok = rec.save({"times": times, "deployed": a.deployed})
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
