#!/usr/bin/env python3
"""The wallet as an app, in headless Chromium (Playwright) with a phone's screen, against test/pwa_serve.py (the
bundle with the host's headers). Read-only towards the network: no faucet, no transaction.

    python3 -I test/pwa_e2e.py --out DIR [--upstream https://HOST] [--base http://localhost:18660]

Checks: service worker and installability (Chrome's own verdict), the receive QR (read back by zbar, an independent
decoder), scanning by camera (a fake camera playing a QR) and from a photo, PIN and passkey unlock (Chrome's virtual
authenticator with and without PRF), lock when hidden, the offline shell, a new build's update banner, a tampered build
refused by the service worker, notifications' setting, block hashes followed live (with --upstream), same-origin only.
Writes DIR/record.json and screenshots.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

# Full Chromium keeps Unix sockets under TMPDIR and a socket path holds at most 107 bytes: under a long TMPDIR (as the
# build box's runner sets) Chromium aborts at launch with SIGTRAP. A short one is used then.
if len(os.environ.get("TMPDIR", "/tmp")) > 40:
    os.environ["TMPDIR"] = os.path.join("/var/tmp", f"wallet-pwa-{os.getuid()}")
    os.makedirs(os.environ["TMPDIR"], mode=0o700, exist_ok=True)
    tempfile.tempdir = None

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
PIN, PASS = "135790", "correct horse battery 1"
UA = ("Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Mobile "
      "Safari/537.36")


class Record:
    def __init__(self, out):
        self.out, self.checks, self.t0 = out, [], time.time()

    def check(self, name, ok, detail=""):
        self.checks.append({"name": name, "ok": bool(ok), "detail": detail, "t": round(time.time() - self.t0, 1)})
        print(("PASS " if ok else "FAIL ") + name + (f": {detail}" if detail else ""), flush=True)
        return ok

    def save(self, extra):
        n = sum(c["ok"] for c in self.checks)
        with open(os.path.join(self.out, "record.json"), "w") as f:
            json.dump({"passed": n, "total": len(self.checks), "checks": self.checks, **extra}, f, indent=1)
        print(f"{n} of {len(self.checks)}")
        return n == len(self.checks)


def node(js, *args):
    return subprocess.run(["node", "--input-type=module", "-e", js, *args], cwd=ROOT, check=True, capture_output=True,
                          text=True).stdout


def test_code():
    """The receive code (with an amount) of a fixed key, made by the page's own lez.js."""
    return node("import * as lez from './lez.js'; const n = lez.rawKeyNode('42'.repeat(32));"
                "console.log(`lez:${lez.idToString(n.accountId)}?pk=${lez.hex(n.pk)}`)").strip()


def y4m(text, path, w=640, h=480, frames=10):
    """A fake camera's video (YUV4MPEG2, 4:2:0): the QR of `text` on white, centred."""
    m = json.loads(node("import { encodeQR } from './vendor/qr.js'; console.log(JSON.stringify(encodeQR(process.argv[1], 'raw', { border: 4 })))", text))
    n = len(m)
    s = min(w, h) * 9 // 10 // n
    ox, oy = (w - n * s) // 2, (h - n * s) // 2
    y = bytearray([235]) * (w * h)
    for r in range(n):
        for c in range(n):
            if m[r][c]:
                for dy in range(s):
                    o = (oy + r * s + dy) * w + ox + c * s
                    y[o:o + s] = bytes([16]) * s
    uv = bytes([128]) * (w * h // 4)
    with open(path, "wb") as f:
        f.write(f"YUV4MPEG2 W{w} H{h} F10:1 Ip A1:1 C420jpeg\n".encode())
        for _ in range(frames):
            f.write(b"FRAME\n" + bytes(y) + uv + uv)


def wait_js(page, expr, timeout=15000):
    """Polls a JS expression with page.evaluate (the page's CSP forbids the eval behind wait_for_function)."""
    end = time.time() + timeout / 1000
    last = None
    while time.time() < end:
        try:
            if page.evaluate(expr):
                return True
        except Exception as e:  # noqa: BLE001 - a navigation in between
            last = e
        time.sleep(0.25)
    raise TimeoutError(f"{expr} (last error: {last})")


def zbar(png):
    if not shutil.which("zbarimg"):
        return None
    r = subprocess.run(["zbarimg", "--raw", "-q", png], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--upstream")
    ap.add_argument("--port", type=int, default=18660)
    ap.add_argument("--deployed", help="https://HOST: test the deployed /wallet/ itself (no local server; no update/tamper steps)")
    a = ap.parse_args()
    a.out = os.path.abspath(a.out)  # the fake camera's video file is handed to Chromium by this path
    os.makedirs(a.out, exist_ok=True)
    rec = Record(a.out)
    base = a.deployed.rstrip("/") if a.deployed else f"http://localhost:{a.port}"
    cmd = [sys.executable, "-I", os.path.join(HERE, "pwa_serve.py"), "--port", str(a.port)]
    if a.upstream:
        cfg = os.path.join(a.out, "config.json")
        with urllib.request.urlopen(a.upstream.rstrip("/") + "/wallet/config.json", timeout=30) as r, open(cfg, "wb") as f:
            f.write(r.read())
        cmd += ["--upstream", a.upstream, "--config", cfg]
    code = test_code()
    cam = os.path.join(a.out, "camera.y4m")
    y4m(code + "&amount=3.25", cam)
    errors, foreign, bip = [], [], None
    server = None if a.deployed else subprocess.Popen(cmd)
    time.sleep(1.0)
    try:
        with sync_playwright() as p:
            # Full Chromium in its new headless mode with a real (not incognito) profile: the headless shell refuses
            # notifications, and Chrome does not offer installation in an incognito profile.
            ctx = p.chromium.launch_persistent_context(
                tempfile.mkdtemp(prefix="pwa-", dir=os.environ.get("TMPDIR")), channel="chromium",
                args=["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", f"--use-file-for-fake-video-capture={cam}"],
                base_url=base, viewport={"width": 412, "height": 915}, device_scale_factor=2.625, is_mobile=True, has_touch=True,
                user_agent=UA)
            ctx.grant_permissions(["notifications", "camera"], origin=base)
            page = ctx.pages[0] if ctx.pages else ctx.new_page()
            page.on("pageerror", lambda e: errors.append(str(e)))
            page.on("request", lambda r: foreign.append(r.url) if not r.url.startswith(base) and not r.url.startswith("data:") else None)
            page.add_init_script("window.__bip = false; addEventListener('beforeinstallprompt', () => { window.__bip = true; });")
            cdp = ctx.new_cdp_session(page)
            vis = lambda sel, t=15000: page.wait_for_selector(sel, state="visible", timeout=t)

            # 1. Service worker, installability
            page.goto("/wallet/")
            vis("#v-welcome")
            wait_js(page, "navigator.serviceWorker.controller !== null", 20000)
            rec.check("service worker controls the page", True)
            inst = cdp.send("Page.getInstallabilityErrors").get("installabilityErrors", [])
            rec.check("installable (Chrome's Page.getInstallabilityErrors is empty)", inst == [], json.dumps(inst))
            man = cdp.send("Page.getAppManifest")
            rec.check("manifest parsed without errors", man.get("url", "").endswith("/wallet/manifest.json") and not man.get("errors"),
                      json.dumps(man.get("errors")))
            cached = page.evaluate("caches.keys().then(async (ks) => { const k = ks.find((x) => x.startsWith('ferrata-wallet-') && x !== 'ferrata-wallet-config'); return k ? (await (await caches.open(k)).keys()).length : 0; })")
            nfiles = len(json.loads(node("import { listing } from './test/pwa-files.mjs'; console.log(JSON.stringify(Object.keys(listing().files)))")))
            rec.check("offline copy holds every file of the build", cached == nfiles, f"{cached} of {nfiles}")

            # 2. Create a wallet
            page.click("#go-create")
            page.check("#saved-words")
            page.fill("#create-pass", PASS)
            page.fill("#create-pass2", PASS)
            page.click("#create-go")
            vis("#v-main", 30000)
            me = page.text_content("#home-id")
            rec.check("wallet created", bool(me), me)

            # 3. Receive QR, read back by zbar
            page.goto("/wallet/#receive")
            vis("#recv-qr svg")
            own = page.text_content("#recv-code")
            qr_png = os.path.join(a.out, "receive-qr.png")
            page.locator("#recv-qr").screenshot(path=qr_png)
            z = zbar(qr_png)
            rec.check("receive QR holds the receive code (zbar reads it back)", page.get_attribute("#recv-qr", "data-text") == own and (z is None or z == own),
                      f"zbar: {z!r}" if z is not None else "zbar not installed")
            page.fill("#recv-amount", "2.5")
            wait_js(page, "document.getElementById('recv-qr').dataset.text.endsWith('&amount=2.5')")
            page.locator("#recv-qr").screenshot(path=os.path.join(a.out, "receive-qr-amount.png"))
            z2 = zbar(os.path.join(a.out, "receive-qr-amount.png"))
            rec.check("receive QR with an amount asked for", z2 is None or z2 == own + "&amount=2.5", repr(z2))
            page.screenshot(path=os.path.join(a.out, "receive.png"))

            # 4. Scan: camera (a fake camera plays the QR of another receive code with an amount), then a photo
            page.goto("/wallet/#send")
            vis("#send-scan")
            page.click("#send-scan")
            wait_js(page, "document.getElementById('send-to').value.length > 0", 20000)
            rec.check("camera scan fills recipient and amount", page.input_value("#send-to") == code and page.input_value("#send-amount") == "3.25",
                      f"{page.input_value('#send-to')[:24]}… {page.input_value('#send-amount')}")
            rec.check("camera released after the scan", page.is_hidden("#scanner") and page.evaluate("document.getElementById('scan-video').srcObject === null"))
            page.fill("#send-to", "")
            page.set_input_files("#send-scan-file", qr_png)
            wait_js(page, "document.getElementById('send-to').value.length > 0", 15000)
            rec.check("photo scan fills the recipient", page.input_value("#send-to") == own)

            # 5. App lock: PIN, passkey (virtual authenticator with PRF), a passkey without PRF refused
            page.goto("/wallet/#settings")
            vis("#lock-settings [data-testid=pin-set]")
            page.fill("[data-testid=lock-pass]", PASS)
            page.fill("[data-testid=pin-new]", PIN)
            page.fill("[data-testid=pin-again]", PIN)
            page.click("[data-testid=pin-set]")
            wait_js(page, "document.querySelector('[data-testid=lock-msg]')?.textContent.includes('PIN unlock is on')", 20000)
            rec.check("PIN set", True)
            cdp.send("WebAuthn.enable", {"enableUI": False})
            opts = {"protocol": "ctap2", "ctap2Version": "ctap2_1", "transport": "internal", "hasResidentKey": True,
                    "hasUserVerification": True, "isUserVerified": True, "automaticPresenceSimulation": True}
            auth = cdp.send("WebAuthn.addVirtualAuthenticator", {"options": {**opts, "hasPrf": True}})["authenticatorId"]
            page.fill("[data-testid=lock-pass]", PASS)
            page.click("[data-testid=passkey-on]")
            wait_js(page, "document.querySelector('[data-testid=lock-msg]')?.textContent.includes('Passkey unlock is on')", 20000)
            q = json.loads(page.evaluate("localStorage.getItem('ferrata-wallet/v1/quick')"))
            rec.check("passkey (PRF) set; stored: wraps only", bool(q.get("passkey")) and bool(q.get("pin")) and "ct" in q["passkey"],
                      ", ".join(sorted(q["passkey"])))
            page.click("#lock")
            vis("#unlock-passkey")
            page.click("#unlock-passkey")
            vis("#v-main", 20000)
            rec.check("unlocked with the passkey", True)
            page.click("#lock")
            vis("#unlock-pin")
            page.fill("#unlock-pin", "000000")
            page.click("#unlock-pin-go")
            wait_js(page, "document.getElementById('quick-err').textContent.includes('4 tries')", 20000)
            page.fill("#unlock-pin", PIN)
            page.click("#unlock-pin-go")
            vis("#v-main", 20000)
            rec.check("wrong PIN counted, right PIN unlocks", True)
            page.click("#lock")
            vis("#use-pass")
            page.click("#use-pass")
            vis("#unlock-pass")
            page.fill("#unlock-pass", PASS)
            page.click("#unlock-go")
            vis("#v-main", 20000)
            page.click("#lock")
            vis("#unlock-passkey")
            rec.check("the passphrase still unlocks; the next lock offers the quick methods again", page.is_visible("#unlock-pin"))
            page.fill("#unlock-pin", PIN)
            page.click("#unlock-pin-go")
            vis("#v-main", 20000)
            page.goto("/wallet/#settings")
            vis("[data-testid=passkey-off]")
            page.click("[data-testid=passkey-off]")
            cdp.send("WebAuthn.removeVirtualAuthenticator", {"authenticatorId": auth})
            auth2 = cdp.send("WebAuthn.addVirtualAuthenticator", {"options": {**opts, "hasPrf": False}})["authenticatorId"]
            vis("[data-testid=passkey-on]")
            page.fill("[data-testid=lock-pass]", PASS)
            page.click("[data-testid=passkey-on]")
            wait_js(page, "document.querySelector('[data-testid=lock-msg]')?.className.includes('bad')", 20000)
            q = json.loads(page.evaluate("localStorage.getItem('ferrata-wallet/v1/quick')"))
            rec.check("a passkey without PRF is refused, nothing saved", not q.get("passkey"), page.text_content("[data-testid=lock-msg]")[:90])
            cdp.send("WebAuthn.removeVirtualAuthenticator", {"authenticatorId": auth2})
            page.select_option("[data-testid=lock-hidden]", "0")
            page.evaluate("""() => { const set = (v) => Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => v });
              set('hidden'); document.dispatchEvent(new Event('visibilitychange'));
              set('visible'); document.dispatchEvent(new Event('visibilitychange')); delete document.visibilityState; }""")
            vis("#v-unlock")
            rec.check("locks when the app leaves the screen (setting: right away)", True)
            page.fill("#unlock-pin", PIN)
            page.click("#unlock-pin-go")
            vis("#v-main", 20000)

            # 6. Notifications' setting
            page.goto("/wallet/#settings")
            vis("[data-testid=notify-on]")
            nstate = page.evaluate("[typeof Notification, globalThis.Notification?.permission ?? null, document.querySelector('[data-testid=notify-state]')?.textContent]")
            if page.is_enabled("[data-testid=notify-on]"):
                page.check("[data-testid=notify-on]")
                wait_js(page, "document.querySelector('[data-testid=notify-state]')?.textContent.includes('granted')")
            rec.check("notifications on (permission granted)", (json.loads(page.evaluate("localStorage.getItem('ferrata-wallet/v1/prefs')")).get("notify") is True), json.dumps(nstate)[:200])
            sw_text = page.text_content("[data-testid=sw-state]")
            rec.check("More shows the offline copy's build", "build" in sw_text and f"{nfiles} files" in sw_text, sw_text)

            # 7. Blocks followed live, each hash recomputed (needs the public network behind /rpc)
            if a.upstream or a.deployed:
                page.goto("/wallet/#home")
                time.sleep(40)
                page.goto("/wallet/#settings")
                t = page.text_content("[data-testid=checks-blocks]")
                m = re.search(r"(\d+) new blocks read.*?(\d+) of (\d+) hashes.*?(\d+) of (\d+) links", t)
                ok = bool(m) and int(m.group(1)) >= 2 and m.group(2) == m.group(3) and m.group(4) == m.group(5) and int(m.group(5)) >= 1
                rec.check("new blocks followed: every hash recomputed and equal, every link checked", ok, t[:160])

            # 8. Offline: reload with no network, unlock, receive QR
            ctx.set_offline(True)
            page.reload()
            vis("#v-unlock", 20000)
            page.fill("#unlock-pin", PIN)
            page.click("#unlock-pin-go")
            vis("#v-main", 20000)
            page.goto("/wallet/#receive")
            vis("#recv-qr svg")
            rec.check("offline: the app opens, unlocks and shows the receive QR", page.text_content("#recv-code") == own,
                      page.text_content("#net-status"))
            page.screenshot(path=os.path.join(a.out, "offline.png"))
            ctx.set_offline(False)

            # 9. A new build: banner, reload into it; a tampered build: refused (local server only)
            if not a.deployed:
                urllib.request.urlopen(base + "/__bump").read()
                page.evaluate("navigator.serviceWorker.getRegistration().then((r) => r.update())")
                vis("#update-banner", 20000)
                with page.expect_navigation(timeout=20000):
                    page.click("#update-go")
                wait_js(page, "navigator.serviceWorker.controller !== null", 20000)
                rec.check("a new build shows the banner and reloads into it", True)
                page.screenshot(path=os.path.join(a.out, "after-update.png"))
                urllib.request.urlopen(base + "/__tamper").read()
                urllib.request.urlopen(base + "/__bump").read()
                state = page.evaluate("""() => navigator.serviceWorker.getRegistration().then((r) => new Promise((done) => {
                    r.addEventListener('updatefound', () => { const w = r.installing; w.addEventListener('statechange', () => {
                      if (w.state === 'redundant' || w.state === 'installed') done(w.state); }); });
                    r.update().catch((e) => done('error ' + e.message)); setTimeout(() => done('timeout'), 20000); }))""")
                rec.check("a build whose file does not match its listed SHA-256 is refused", state == "redundant" and page.is_hidden("#update-banner"), state)
                urllib.request.urlopen(base + "/__tamper").read()

                # 10. Settings that parse but lack what the page reads: it says so and stops, with no script error
                for body, why in (("{}", "rpc is missing"), ("null", "not a settings object"), ('{"rpc": "/rpc", "token": {"symbol": "LGO"}}', "token.decimals")):
                    urllib.request.urlopen(base + "/__config?json=" + urllib.parse.quote(body)).read()
                    p2, errs2 = ctx.new_page(), []
                    p2.on("pageerror", lambda e, errs2=errs2: errs2.append(str(e)))
                    p2.goto("/wallet/")
                    p2.wait_for_selector("[data-testid=config-error]", state="visible", timeout=20000)
                    t = p2.text_content("[data-testid=config-error]")
                    rec.check(f"config.json {body}: the page says it could not read the settings, no script error",
                              "could not read the network's settings" in t and why in t and not errs2, t[:110])
                    p2.close()
                urllib.request.urlopen(base + "/__config").read()

            rec.check("no request left this origin", not foreign, ", ".join(foreign[:3]))
            rec.check("no script errors", not errors, " | ".join(errors[:3]))
            bip = page.evaluate("window.__bip")
            ctx.close()
    finally:
        if server:
            server.terminate()
    ok = rec.save({"base": "deployed" if a.deployed else base, "upstream": bool(a.upstream), "beforeinstallprompt_fired": bip})
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
