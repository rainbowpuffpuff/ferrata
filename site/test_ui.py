#!/usr/bin/env python3
"""The build page in a real browser: the requirements overview, a requirement's sheet, deep links, the full table, and
the layout at five widths.

Serves deploy/testnet/web/build/ (or the bundle directory given) on 127.0.0.1 (a random port) and drives Chromium with
Playwright; every request to another origin is aborted, so it reads nothing from the network (for the layout, the live
readings are answered from the fixtures below: GitHub's API, /rpc, /status/status.json with attested facts whose values
are long hex words, /status/programs.json and /faucet). Checks:
  - the overview fits one desktop screen (1366 x 768) and its 137 tiles are keyboard-reachable links;
  - a click, Enter or Space on a tile opens that requirement's sheet (the RFP's words, status and reason, tests,
    milestone), sets #req-RFP-…, and Escape closes it, returning focus to the tile;
  - a deep link #req-RFP-021-F.12 opens its sheet on load; the sheet's previous/next buttons and arrow keys step;
  - "Show the full table" shows the table with its filters (?view=table) and "Back to the overview" returns;
    ?status=waits opens the table filtered (on a phone too); #rfp-021-s-8 opens the table at that row;
  - the legend highlights one status; without JavaScript the full table shows (assets/nojs.css) and the controls that
    need JavaScript (copy buttons, the filters, the faucet's form) stay hidden;
  - at 320, 375, 768, 1024, 1450 and 1920 px, light and dark, with the live readings in (one fact's statement a long
    hex sentence, its "Full values" open): the page does not scroll sideways; in the hero's
    summary no piece of text overlaps another and each stays inside its card, chip or bar part; no Live card has
    content wider than the card; no box clips its text;
  - the testers' page (testers/): linked from the build page's header and hero at every width; seven steps, the
    bridge's four, the report form's link, the note for AI agents (near the top and in How to report: /AGENTS.md,
    /agents/); no script at all, so the same without JavaScript; at the six widths, light
    and dark, it does not scroll sideways, nothing in it is wider than the screen, no table scrolls and no box clips
    its text;
  - no script error on any of these pages.

Usage: python3 site/test_ui.py [BUNDLE_DIR]  (exit 0 pass, 1 fail, 77 when Playwright with Chromium is not installed)
"""

import functools
import http.server
import json
import pathlib
import sys
import threading

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUNDLE = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "deploy" / "testnet" / "web" / "build"
WIDTHS = (320, 375, 768, 1024, 1450, 1920)

# The live readings, as the test network and GitHub's API give them (values from the test network's records)
TX = "a568dcb852637a001b772e258c260a1e9775aa47a987f7e62518198a6cc3674d"
FACT = {"address": "0x04ee8ff19cc11cee4ae65d03e12b9888043de44c", "predicate": "eq", "block": 12194951, "record_slot": 11641184,
        "tx": TX, "time": "2026-11-23T10:24:35Z"}
LIVE = {
    "/status/status.json": {"network": "ferrata-lez-testnet", "attestation": {"updater": [
        {"chain": "sepolia", "status": "ok", "finalized_slot": 11644864, "period": 1421, "sources_head_slot": 11644896,
         "reasons": [], "last_applied_utc": "2026-11-23T22:29:56Z"}]},
        "attestations": [dict(FACT, kind="storage", slot="0x" + "0" * 63 + "1",
                              value="0xa67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469"),
                         dict(FACT, kind="account"),
                         dict(FACT, statement="storage slot 0x" + "0" * 63 + "1 of 0x04ee8ff19cc11cee4ae65d03e12b9888043de44c equals "
                              "0xa67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469")]},
    "/status/programs.json": {"network": "ferrata-lez-testnet", "lez": "v0.4.0-rc3",
                              "programs": {"ferrata_attestation": {"program_account": "6f9zuRR63bSN5AEeffqZeACYCwFmqfe5JJhPshuJfYNg"}},
                              "instances": {"sepolia": {"instance_id": "0xc741a1f16012516529529814da7a7ebc2d18a8cdcba2a3e7f58710f52630782e"}}},
    "/faucet": {"faucet_account": "6f9zuRR63bSN5AEeffqZeACYCwFmqfe5JJhPshuJfYNg", "balance": 987654321000, "amount": 5000000000,
                "unit": "1 LGO = 10^9"},
}
RUN = {"workflow_runs": [{"status": "completed", "conclusion": "failure", "run_number": 63, "head_sha": "98375f82" + "0" * 32,
                          "html_url": "https://github.com/rainbowpuffpuff/ferrata/actions/runs/1", "updated_at": "2026-11-23T22:00:00Z"}]}
ISSUE = {"state": "open", "comments": 7, "updated_at": "2026-11-23T12:00:00Z"}

# The layout at one width: what overflows, overlaps or is clipped (an empty list when nothing is)
LAYOUT = """() => {
  const out = [], de = document.documentElement;
  if (de.scrollWidth > de.clientWidth + 1) out.push(`the page scrolls sideways (${de.scrollWidth} px wide in ${de.clientWidth})`);
  const shown = (e) => e.checkVisibility({ visibilityProperty: true, opacityProperty: true }) && !e.closest(".vh");
  const label = (t) => JSON.stringify(t.trim().replace(/\\s+/g, " ").slice(0, 32));
  const inside = (q, b) => q.left >= b.left - 1 && q.right <= b.right + 1 && q.top >= b.top - 1 && q.bottom <= b.bottom + 1;
  const isBox = (e) => { const c = getComputedStyle(e); return parseFloat(c.borderTopWidth) > 0 || c.backgroundColor !== "rgba(0, 0, 0, 0)"; };
  const textRects = (root) => {
    const list = [], walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      if (!n.nodeValue.trim() || !shown(n.parentElement)) continue;
      const r = document.createRange();
      r.selectNodeContents(n);
      const pb = n.parentElement.getBoundingClientRect();
      for (const q of r.getClientRects()) if (q.width > 0 && q.height > 0)
        list.push({ n, q, v: { left: q.left, right: q.right, top: Math.max(q.top, pb.top), bottom: Math.min(q.bottom, pb.bottom) } });
    }
    return list;
  };
  // The hero: no text over other text; each piece of text and each box inside its nearest box (card, chip, bar part)
  const hero = document.querySelector(".hero");
  const ht = textRects(hero);
  for (let i = 0; i < ht.length; i++) for (let j = i + 1; j < ht.length; j++) {
    const a = ht[i], b = ht[j];
    if (a.n === b.n) continue;
    const w = Math.min(a.v.right, b.v.right) - Math.max(a.v.left, b.v.left);
    const h = Math.min(a.v.bottom, b.v.bottom) - Math.max(a.v.top, b.v.top);
    if (w > 1 && h > 0.25 * Math.min(a.v.bottom - a.v.top, b.v.bottom - b.v.top)) out.push(`hero: ${label(a.n.nodeValue)} overlaps ${label(b.n.nodeValue)}`);
  }
  const boxOf = (e) => { for (let x = e.parentElement; x && x !== hero; x = x.parentElement) if (isBox(x)) return x; return null; };
  for (const t of ht) {
    const b = boxOf(t.n);
    if (b && !inside(t.q, b.getBoundingClientRect())) out.push(`hero: ${label(t.n.nodeValue)} runs out of its ${b.className || b.tagName}`);
  }
  for (const e of hero.querySelectorAll("*")) {
    if (!isBox(e) || !shown(e) || !e.getClientRects().length) continue;
    const b = boxOf(e);
    if (b && !inside(e.getBoundingClientRect(), b.getBoundingClientRect())) out.push(`hero: a ${e.className || e.tagName} runs out of its ${b.className || b.tagName}`);
  }
  // Live: no card holds anything wider than itself
  for (const card of document.querySelectorAll("#live .card")) {
    const c = card.getBoundingClientRect();
    const wide = [...card.querySelectorAll("*")].filter((e) => shown(e) && e.getClientRects().length).map((e) => e.getBoundingClientRect())
      .concat(textRects(card).map((t) => t.q)).filter((q) => q.left < c.left - 1 || q.right > c.right + 1);
    if (wide.length || card.scrollWidth > card.clientWidth + 1) out.push(`live: #${card.id} has content wider than the card (${Math.round(Math.max(card.scrollWidth, ...wide.map((q) => q.right - c.left)))} px in ${Math.round(c.width)})`);
  }
  // No box clips its text sideways (scrolling boxes and visually hidden text aside)
  for (const e of document.querySelectorAll("main *")) {
    const c = getComputedStyle(e);
    if ((c.overflowX === "hidden" || c.overflowX === "clip") && c.clip === "auto" && shown(e) && e.scrollWidth > e.clientWidth + 1)
      out.push(`clipped: ${e.tagName.toLowerCase()}.${e.className} ${label(e.textContent)} (${e.scrollWidth} px in ${e.clientWidth})`);
  }
  return out;
}"""


# The testers' page at one width: what overflows or is clipped (an empty list when nothing is)
TESTERS_LAYOUT = """() => {
  const out = [], de = document.documentElement, vw = de.clientWidth;
  if (de.scrollWidth > vw + 1) out.push(`the page scrolls sideways (${de.scrollWidth} px wide in ${vw})`);
  const shown = (e) => e.checkVisibility({ visibilityProperty: true, opacityProperty: true }) && !e.closest(".vh");
  const label = (t) => JSON.stringify(t.trim().replace(/\\s+/g, " ").slice(0, 32));
  for (const e of document.querySelectorAll("main *")) {
    if (!shown(e) || !e.getClientRects().length) continue;
    const r = e.getBoundingClientRect(), c = getComputedStyle(e);
    if (r.right > vw + 1 || r.left < -1) out.push(`wider than the screen: ${e.tagName.toLowerCase()}.${e.className} ${label(e.textContent)}`);
    if ((c.overflowX === "hidden" || c.overflowX === "clip") && c.clip === "auto" && e.scrollWidth > e.clientWidth + 1)
      out.push(`clipped: ${e.tagName.toLowerCase()}.${e.className} ${label(e.textContent)}`);
  }
  for (const s of document.querySelectorAll("main .scroll")) if (s.scrollWidth > s.clientWidth + 1) out.push(`a table scrolls sideways: ${label(s.textContent)}`);
  return out;
}"""


def serve():
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=str(BUNDLE))
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def main():
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("test_ui.py: skipped (needs Playwright: pip install playwright; playwright install chromium)")
        return 77
    httpd = serve()
    base = f"http://127.0.0.1:{httpd.server_address[1]}/"
    problems, errors = [], []

    def check(cond, what):
        if not cond:
            problems.append(what)

    with sync_playwright() as p:
        try:
            browser = p.chromium.launch()
        except Exception as e:
            print(f"test_ui.py: skipped (Chromium for Playwright is not installed: {str(e).splitlines()[0][:120]})")
            httpd.shutdown()
            return 77

        def live(route):
            """The live readings from the fixtures; the bundle from the server; nothing from anywhere else."""
            url = route.request.url
            if url.startswith(base):
                path = "/" + url[len(base):].split("?")[0].split("#")[0]
                if path == "/rpc":
                    method = json.loads(route.request.post_data or "{}").get("method")
                    result = 4447 if method == "getLastBlockId" else {"header": {"timestamp": 1795473192239}, "bedrock_status": "Finalized"}
                    return route.fulfill(json={"jsonrpc": "2.0", "id": 1, "result": result})
                if path in LIVE:
                    return route.fulfill(json=LIVE[path])
                return route.continue_()
            if url.startswith("https://api.github.com/"):
                return route.fulfill(json=RUN if "/actions/workflows/" in url else ISSUE)
            return route.abort()

        def page(js=True, width=1366, height=768, readings=False, scheme="light"):
            ctx = browser.new_context(viewport={"width": width, "height": height}, java_script_enabled=js, color_scheme=scheme)
            pg = ctx.new_page()
            pg.route("**/*", live if readings else lambda r: r.continue_() if r.request.url.startswith(base) else r.abort())
            pg.on("pageerror", lambda e: errors.append(str(e)))
            return pg

        pg = page()
        pg.goto(base + "#requirements", wait_until="load")
        pg.wait_for_timeout(300)
        box = pg.locator("#overview").bounding_box()
        check(box and box["height"] <= 768 - 56, f"the overview is {box and round(box['height'])} px high, more than one screen")
        tiles = pg.locator("a.tile")
        check(tiles.count() == 137, f"{tiles.count()} tiles")
        check(pg.locator("#req-full").is_hidden(), "the full table shows before it is asked for")
        first = tiles.first
        first.click()
        sheet = pg.locator("#req-dialog")
        check(sheet.is_visible(), "a click on a tile opens no sheet")
        check(pg.evaluate("location.hash") == "#req-RFP-021-F.1", f"hash after a click: {pg.evaluate('location.hash')}")
        check(pg.locator("#rq-title").inner_text() == "RFP-021 F.1", "the sheet's title")
        body = pg.locator("#rq-body").text_content()
        for part in ("In the RFP’s words", "Status and why", "Shown by", "Milestone", "Run the Ethereum light client"):
            check(part in body, f"the sheet lacks {part!r}")
        pg.keyboard.press("ArrowRight")
        check(pg.locator("#rq-title").inner_text() == "RFP-021 F.2", "ArrowRight does not step to F.2")
        pg.locator('#req-dialog [data-step="-1"]').click()
        check(pg.locator("#rq-title").inner_text() == "RFP-021 F.1", "the previous button does not step back")
        pg.keyboard.press("Escape")
        check(sheet.is_hidden(), "Escape does not close the sheet")
        check(pg.evaluate("document.activeElement.getAttribute('data-req')") == "RFP-021-F.1", "focus does not return to the tile")
        check(pg.evaluate("location.hash") == "#requirements", "the hash is not restored on close")
        pg.keyboard.press("Space")
        check(sheet.is_visible(), "Space on a tile opens no sheet")
        pg.keyboard.press("Escape")
        pg.keyboard.press("Enter")
        check(sheet.is_visible(), "Enter on a tile opens no sheet")
        pg.keyboard.press("Escape")

        pg.locator(".ov-legend button.lg[data-hl=notyet]").click()
        check(pg.locator("#overview").get_attribute("data-hl") == "notyet", "the legend does not highlight")
        pg.locator(".ov-legend button.lg[data-hl=notyet]").click()

        pg.locator("#view-toggle").click()
        check(pg.locator("#req-full").is_visible() and pg.locator("#filters").is_visible(), "the full table does not show")
        check("view=table" in pg.url, "?view=table is not set")
        check(pg.locator("#ov-grid").is_hidden(), "the overview stays beside the table")
        check(pg.locator("#req-count").inner_text() == "137 of 137 requirements", "the table's count")
        check(pg.locator("#view-toggle").inner_text() == "Back to the overview", "the toggle's label")
        pg.locator("#view-toggle").click()
        check(pg.locator("#ov-grid").is_visible() and pg.locator("#req-full").is_hidden(), "back to the overview fails")

        pg2 = page()
        pg2.goto(base + "#req-RFP-021-F.12", wait_until="load")
        pg2.wait_for_timeout(300)
        check(pg2.locator("#req-dialog").is_visible() and pg2.locator("#rq-title").inner_text() == "RFP-021 F.12",
              "the deep link #req-RFP-021-F.12 opens no sheet")
        pg2.goto(base + "?status=waits", wait_until="load")
        pg2.wait_for_timeout(200)
        check(pg2.locator("#req-count").inner_text() == "5 of 137 requirements", "?status=waits does not filter the table")
        pg2.goto(base + "#rfp-021-s-8", wait_until="load")
        pg2.wait_for_timeout(300)
        check(pg2.locator("#rfp-021-s-8").is_visible(), "#rfp-021-s-8 does not open the table at its row")

        pg4 = page()
        pg4.goto(base + "#%E0%A4%A", wait_until="load")
        pg4.wait_for_timeout(300)
        check(pg4.locator("button.copy:visible").count() > 0, "a malformed hash stops the script (no copy button shown)")

        pg3 = page(js=False)
        pg3.goto(base, wait_until="load")
        check(pg3.locator("#req-table").is_visible(), "without JavaScript the full table is hidden")
        check(pg3.locator("a.tile").count() == 137, "without JavaScript the tiles are missing")
        for sel in ("button.copy", "#filters", "#faucet-form"):
            check(pg3.locator(sel).first.is_hidden(), f"without JavaScript {sel} shows, though it needs JavaScript")

        mob = page(width=390, height=844)
        mob.goto(base + "#req-RFP-022-Sec.1", wait_until="load")
        mob.wait_for_timeout(300)
        sb = mob.locator("#req-dialog").bounding_box()
        check(sb and sb["width"] >= 380, "on a phone the sheet is not full width")
        mob.goto(base + "?status=waits", wait_until="load")
        mob.wait_for_timeout(200)
        n = mob.locator("#req-table tbody tr:visible").count()
        check(n == 5, f"on a phone ?status=waits shows {n} rows of the table, not 5")

        # The testers' page: its links from the build page, its steps, no script; the layout at every width
        report = "https://github.com/rainbowpuffpuff/ferrata/issues/new?template=tester-report.yml"
        for width, scheme in [(w, s) for w in WIDTHS for s in ("light", "dark")]:
            tp = page(width=width, height=900, scheme=scheme)
            tp.goto(base, wait_until="load")
            for sel, where in ((".top .top-cta", "header"), (".hero .testers-cta a", "hero")):
                link = tp.locator(sel)
                check(link.count() == 1 and link.is_visible() and link.get_attribute("href") == "testers/",
                      f"{width} px {scheme}: the build page's {where} shows no link to testers/")
            tp.locator(".top .top-cta").click()
            tp.wait_for_load_state("load")
            check(tp.url == base + "testers/", f"{width} px {scheme}: the header's link leads to {tp.url}")
            for what in tp.evaluate(TESTERS_LAYOUT):
                problems.append(f"testers/ at {width} px {scheme}: {what}")
            tp.context.close()
        tp = page(js=False)
        tp.goto(base + "testers/", wait_until="load")
        check(tp.locator("script").count() == 0, "testers/ loads a script")
        check(tp.locator("ol.t-steps > li").count() == 7, f"testers/: {tp.locator('ol.t-steps > li').count()} steps, not 7")
        check(tp.locator("#step-bridge ol.t-sub > li").count() == 4, "testers/: the bridge has not four steps")
        check(tp.locator("#report a.btn").get_attribute("href") == report, "testers/: the report link is not the tester-report form")
        check(tp.locator("#safety").is_visible() and tp.locator(".t-alert").is_visible(), "testers/: the safety notes do not show")
        for where in ("#agents", "#report"):
            check(tp.locator(f"{where} a[href='/AGENTS.md']").count() == 1 and tp.locator(f"{where} a[href='/agents/']").count() == 1,
                  f"testers/: {where} does not link /AGENTS.md and /agents/")
        tp.locator(".top-in .brand").click()
        tp.wait_for_load_state("load")
        check(tp.url == base, f"testers/: the brand leads to {tp.url}, not the build page")
        tp.context.close()

        for width, scheme in [(w, s) for w in WIDTHS for s in ("light", "dark")]:
            lp = page(width=width, height=900, readings=True, scheme=scheme)
            lp.goto(base, wait_until="load")
            try:
                lp.wait_for_function("document.querySelector('#live-facts .facts') && "
                                     "document.querySelector('[data-live=ci-chip]').textContent !== 'see below'", timeout=10000)
            except Exception:
                problems.append(f"{width} px: the live readings (CI, attested facts) did not show")
            lp.evaluate("document.querySelectorAll('#live details').forEach((d) => { d.open = true; })")
            lp.wait_for_timeout(200)
            for what in lp.evaluate(LAYOUT):
                problems.append(f"{width} px {scheme}: {what}")
            lp.context.close()
        browser.close()
    httpd.shutdown()
    problems += [f"script error: {e}" for e in errors]
    for prob in problems:
        print(f"FAIL {prob}", file=sys.stderr)
    print(f"test_ui.py: {'ok' if not problems else str(len(problems)) + ' problem(s)'}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
