#!/usr/bin/env python3
"""Measures the wallet as a phone would load it, in headless Chromium on a build machine (not a phone): cold start on a
first visit and on a repeat visit (service worker), unlock times, and the data the open wallet uses per minute. Read-only
towards the network: no faucet, no transaction.

    python3 -I test/pwa_measure.py --out DIR --base https://HOST/wallet/ [--old-root DIR --upstream https://HOST]

Emulated phone: 412x915 CSS px at 2.625, Chrome's mobile user agent, CPU slowed 4x (Emulation.setCPUThrottlingRate),
network 150 ms round trip, 1.6 Mbit/s down, 0.75 Mbit/s up (Network.emulateNetworkConditions, page requests). Each
timing is the median of RUNS runs. Data: bytes on the wire (request bodies + encoded response bodies + headers) of every
request of the page and its service worker (Playwright's request sizes), split by kind. With --old-root, the idle data
of an older build (served locally, its RPC passed to the network) is measured the same way for comparison.
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pwa_e2e import HERE, UA, wait_js  # noqa: E402
from playwright.sync_api import sync_playwright  # noqa: E402

PASS, PIN = "measure passphrase 1", "246813"
READY = """(() => { const mark = () => { const w = document.getElementById('v-welcome'), u = document.getElementById('v-unlock');
  if ((w && !w.hidden) || (u && !u.hidden)) { window.__ready ??= performance.now(); } };
  new MutationObserver(mark).observe(document, { subtree: true, attributes: true, childList: true }); })();"""


class Bytes:
    """Every finished request of a context (page and service worker), sized later while the context is open. Events
    reach Python only while it calls into Playwright, so waits must use page.wait_for_timeout, and requests are placed
    in time by their own start, not by when their event arrived."""

    def __init__(self, ctx):
        self.pending, self.rows = [], []
        ctx.on("requestfinished", lambda r: self.pending.append(r))

    def settle(self):
        while self.pending:
            r = self.pending.pop(0)
            try:
                s = r.sizes()
                resp = r.response()
                sw = bool(resp and resp.from_service_worker)
                t = r.timing["startTime"] / 1000
            except Exception:  # noqa: BLE001 - a request of a closed page
                continue
            kind = "rpc" if r.url.split("?")[0].endswith("/rpc") else "file"
            self.rows.append({"t": t, "kind": kind, "sw_cache": sw,
                              "bytes": 0 if sw else s["requestBodySize"] + s["requestHeadersSize"] + s["responseBodySize"] + s["responseHeadersSize"]})

    def since(self, t0, kind=None, t1=None):
        self.settle()
        rows = [x for x in self.rows if x["t"] >= t0 and (t1 is None or x["t"] < t1) and (kind is None or x["kind"] == kind)]
        return sum(x["bytes"] for x in rows), len([x for x in rows if not x["sw_cache"]])


def throttle(page, ctx, cpu=4, net=True):
    cdp = ctx.new_cdp_session(page)
    cdp.send("Emulation.setCPUThrottlingRate", {"rate": cpu})
    cdp.send("Network.enable")
    if net:
        cdp.send("Network.emulateNetworkConditions", {"offline": False, "latency": 150, "downloadThroughput": 200000, "uploadThroughput": 93750})
    return cdp


def profile(p, base):
    ctx = p.chromium.launch_persistent_context(
        tempfile.mkdtemp(prefix="measure-", dir=os.environ.get("TMPDIR")), channel="chromium", base_url=base,
        viewport={"width": 412, "height": 915}, device_scale_factor=2.625, is_mobile=True, has_touch=True, user_agent=UA)
    ctx.add_init_script(READY)
    return ctx


def visit(ctx, url, cpu, net, until="#v-welcome, #v-unlock"):
    page = ctx.new_page()
    throttle(page, ctx, cpu, net)
    page.goto(url)
    wait_js(page, "window.__ready !== undefined", 60000)
    ready = page.evaluate("window.__ready")
    fcp = page.evaluate("performance.getEntriesByName('first-contentful-paint')[0]?.startTime ?? null")
    return page, ready, fcp


def timed(page, action, done, timeout=60000):
    t = time.time()
    action()
    wait_js(page, done, timeout)
    return round((time.time() - t) * 1000)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--base", required=True, help="the wallet's URL, ending in /wallet/")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--idle", type=int, default=180, help="seconds of idle data measurement")
    ap.add_argument("--old-root")
    ap.add_argument("--upstream")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    origin = a.base.split("/wallet/")[0]
    res = {"base_is_local": origin.startswith("http://localhost"), "runs": a.runs, "first": [], "repeat": [], "first_fast": [], "repeat_fast": []}
    with sync_playwright() as p:
        # Cold starts: a new profile each run (first visit), then a second page in it (repeat visit, service worker).
        for i in range(a.runs):
            for tag, cpu, net in (("", 4, True), ("_fast", 1, False)):
                ctx = profile(p, origin)
                b = Bytes(ctx)
                t0 = time.time()
                page, ready, fcp = visit(ctx, "/wallet/", cpu, net)
                wait_js(page, "navigator.serviceWorker.controller !== null", 60000)
                page.wait_for_timeout(2000)
                first = {"ready_ms": round(ready), "fcp_ms": round(fcp) if fcp else None}
                page.close()
                t1 = time.time()
                first["bytes"], first["requests"] = b.since(t0, t1=t1)
                res["first" + tag].append(first)
                page, ready, fcp = visit(ctx, "/wallet/", cpu, net)
                page.wait_for_timeout(1000)
                rb, rn = b.since(t1)
                res["repeat" + tag].append({"ready_ms": round(ready), "fcp_ms": round(fcp) if fcp else None, "bytes": rb, "requests": rn})
                ctx.close()

        # Unlock times (emulated phone, CPU 4x): create, passphrase, PIN, passkey (Chrome's virtual authenticator, PRF).
        try:
            ctx = profile(p, origin)
            page = ctx.new_page()
            cdp = throttle(page, ctx, 4, True)
            page.goto("/wallet/")
            wait_js(page, "window.__ready !== undefined", 60000)
            page.click("#go-create"); page.check("#saved-words"); page.fill("#create-pass", PASS); page.fill("#create-pass2", PASS)
            res["create_ms"] = timed(page, lambda: page.click("#create-go"), "!document.getElementById('v-main').hidden")
            page.goto("/wallet/#settings")
            page.wait_for_selector("[data-testid=pin-set]", state="visible", timeout=30000)
            page.fill("[data-testid=lock-pass]", PASS); page.fill("[data-testid=pin-new]", PIN); page.fill("[data-testid=pin-again]", PIN)
            res["pin_setup_ms"] = timed(page, lambda: page.click("[data-testid=pin-set]"), "document.querySelector('[data-testid=lock-msg]')?.textContent.includes('PIN unlock is on')")
            cdp.send("WebAuthn.enable", {"enableUI": False})
            cdp.send("WebAuthn.addVirtualAuthenticator", {"options": {"protocol": "ctap2", "ctap2Version": "ctap2_1", "transport": "internal", "hasResidentKey": True,
                                                                       "hasUserVerification": True, "isUserVerified": True, "automaticPresenceSimulation": True, "hasPrf": True}})
            page.fill("[data-testid=lock-pass]", PASS)
            res["passkey_setup_ms"] = timed(page, lambda: page.click("[data-testid=passkey-on]"), "document.querySelector('[data-testid=lock-msg]')?.textContent.includes('Passkey unlock is on')")
            unl = {"passphrase": [], "pin": [], "passkey": []}
            STATE = """JSON.stringify({ unlock: !document.getElementById('v-unlock').hidden, main: !document.getElementById('v-main').hidden,
              quick: !document.getElementById('quick-unlock').hidden, passkey: !document.getElementById('unlock-passkey').hidden,
              pin: !document.getElementById('unlock-pin-box').hidden, stored: Object.keys(JSON.parse(localStorage.getItem('ferrata-wallet/v1/quick') || '{}')),
              hash: location.hash })"""
            res["state_after_setup"] = page.evaluate(STATE)
            for _ in range(a.runs):
                page.click("#lock")
                try:
                    page.wait_for_selector("#unlock-passkey", state="visible", timeout=15000)
                except Exception:
                    res["state_at_failure"] = page.evaluate(STATE)
                    page.screenshot(path=os.path.join(a.out, "unlock-failure.png"))
                    raise
                unl["passkey"].append(timed(page, lambda: page.click("#unlock-passkey"), "!document.getElementById('v-main').hidden"))
                page.click("#lock"); page.wait_for_selector("#unlock-pin", state="visible")
                page.fill("#unlock-pin", PIN)
                unl["pin"].append(timed(page, lambda: page.click("#unlock-pin-go"), "!document.getElementById('v-main').hidden"))
                page.click("#lock"); page.wait_for_selector("#use-pass", state="visible"); page.click("#use-pass")
                page.fill("#unlock-pass", PASS)
                unl["passphrase"].append(timed(page, lambda: page.click("#unlock-go"), "!document.getElementById('v-main').hidden"))
            res["unlock_ms"] = {k: statistics.median(v) for k, v in unl.items() if v}
            res["unlock_runs_ms"] = unl
            ctx.close()
        except Exception as e:  # noqa: BLE001 - recorded, and the data measurement still runs
            res["unlock_error"] = str(e)[:300]

        # Data while open and unlocked on Home (no throttling), this build and, if given, an older one.
        def idle(url_origin, label):
            ctx = profile(p, url_origin)
            b = Bytes(ctx)
            page = ctx.new_page()
            page.goto("/wallet/")
            wait_js(page, "window.__ready !== undefined", 60000)
            page.click("#go-create"); page.check("#saved-words"); page.fill("#create-pass", PASS); page.fill("#create-pass2", PASS)
            page.click("#create-go")
            wait_js(page, "!document.getElementById('v-main').hidden", 60000)
            page.wait_for_timeout(20000)  # past the first refresh and the follower's first read
            t0 = time.time()
            page.wait_for_timeout(a.idle * 1000)
            t1 = time.time()
            rpc, n = b.since(t0, "rpc", t1)
            allb, alln = b.since(t0, None, t1)
            ctx.close()
            res[f"idle_{label}"] = {"seconds": a.idle, "rpc_bytes": rpc, "rpc_requests": n, "all_bytes": allb, "all_requests": alln,
                                    "rpc_bytes_per_min": round(rpc * 60 / a.idle), "rpc_requests_per_min": round(n * 60 / a.idle, 1)}
        idle(origin, "this_build")
        if a.old_root and a.upstream:
            cfg = os.path.join(a.out, "config.json")
            subprocess.run(["curl", "-sS", "-o", cfg, a.upstream.rstrip("/") + "/wallet/config.json"], check=True)
            srv = subprocess.Popen([sys.executable, "-I", os.path.join(HERE, "pwa_serve.py"), "--port", "18670", "--root", a.old_root,
                                    "--upstream", a.upstream, "--config", cfg])
            time.sleep(1)
            try:
                idle("http://localhost:18670", "old_build")
            finally:
                srv.terminate()

    med = lambda rows, k: statistics.median([r[k] for r in rows]) if rows and rows[0][k] is not None else None
    res["summary"] = {k: {"ready_ms": med(res[k], "ready_ms"), "fcp_ms": med(res[k], "fcp_ms"), "bytes": med(res[k], "bytes")} for k in ("first", "repeat", "first_fast", "repeat_fast")}
    with open(os.path.join(a.out, "measure-raw.json"), "w") as f:
        json.dump(res, f, indent=1)
    print(json.dumps({"summary": res["summary"], "unlock_ms": res.get("unlock_ms"), "create_ms": res.get("create_ms"), "unlock_error": res.get("unlock_error"),
                      "state_after_setup": res.get("state_after_setup"), "state_at_failure": res.get("state_at_failure"),
                      "idle_this": res.get("idle_this_build"), "idle_old": res.get("idle_old_build")}, indent=1))


if __name__ == "__main__":
    main()
