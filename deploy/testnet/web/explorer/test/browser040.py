#!/usr/bin/env python3
"""Drive the explorer in headless Chromium against replay040.py's real v0.4.0 blocks.

    TMPDIR=<a short path> python3 deploy/testnet/web/explorer/test/browser040.py --out DIR

Starts its own loopback replay, writes screenshots, DIR/replay040.log and DIR/record.json, stops the replay, and exits
1 on any failed check. Expected values come from the recorded run's facts and block data, never from DOM-generated
expectations. Run with ordinary python3 (Playwright is installed in the user's site-packages).
"""
import argparse
import json
import re
import subprocess
import sys
import time
import urllib.request
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
RUN = REPO / "cli" / "results" / "v040-standalone-1"


class Record:
    def __init__(self, out):
        self.out, self.checks, self.t0 = Path(out), [], time.time()

    def check(self, name, ok, detail=""):
        detail = str(detail)
        self.checks.append({"name": name, "ok": bool(ok), "detail": detail, "t": round(time.time() - self.t0, 1)})
        print(("PASS " if ok else "FAIL ") + name + (f": {detail}" if detail else ""), flush=True)
        return bool(ok)

    def save(self, extra):
        passed = sum(c["ok"] for c in self.checks)
        record = {"passed": passed, "total": len(self.checks), "checks": self.checks, **extra}
        (self.out / "record.json").write_text(json.dumps(record, indent=1) + "\n")
        print(f"{passed} of {len(self.checks)}", flush=True)
        return passed == len(self.checks)


def native_units(value):
    """The config's 9-decimal LGO formatting, computed from an integer in the run record."""
    whole, fraction = divmod(int(value), 1_000_000_000)
    return f"{whole:,}" + (f".{fraction:09d}".rstrip("0") if fraction else "") + " LGO"


def rpc(base, method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    request = urllib.request.Request(base + "rpc", data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=5) as response:
        answer = json.load(response)
    if "error" in answer:
        raise RuntimeError(f"{method}: {answer['error']}")
    return answer["result"]


def wait_server(log_path, process):
    deadline = time.time() + 10
    while time.time() < deadline:
        text = log_path.read_text() if log_path.exists() else ""
        match = re.search(r"on (http://127\.0\.0\.1:\d+/)", text)
        if match:
            return match.group(1)
        if process.poll() is not None:
            raise RuntimeError(f"replay exited {process.returncode}: {text[-2000:]}")
        time.sleep(0.05)
    raise TimeoutError(f"replay did not announce its port: {log_path.read_text()[-2000:]}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True, help="output directory for record.json, screenshots and replay log")
    parser.add_argument("--port", type=int, default=0, help="loopback replay port (default: an available port)")
    parser.add_argument("--run", default=str(RUN), help="recorded v0.4.0 run")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    run = Path(args.run)
    facts = json.loads((run / "facts.json").read_text())
    wallet = json.loads((run / "wallet.json").read_text())
    recorded3 = json.loads((run / "lez-read-block-3.json").read_text())
    send = next(s for s in facts["sends"] if s["block"] == 3)
    transfer = next(t for t in recorded3["transactions"] if t["hash"] == send["hash"])
    sender, recipient = wallet["payer"], wallet["recipient"]
    amount = int(send["amount"])
    assert transfer["signers"] == [sender]
    assert transfer["call"]["args"]["amount"] == amount
    record = Record(out)
    errors, outside = [], []
    process, browser, base = None, None, None
    log_path = out / "replay040.log"

    try:
        with log_path.open("w") as replay_log:
            process = subprocess.Popen(
                [sys.executable, str(HERE / "replay040.py"), "--port", str(args.port), "--run", str(run)],
                cwd=REPO, stdout=replay_log, stderr=subprocess.STDOUT)
            base = wait_server(log_path, process)
            status = rpc(base, "getSequencerStatus")
            head = rpc(base, "getLastBlockId")
            block3_wire = rpc(base, "getBlock", [3])
            receipt = rpc(base, "getTransactionReceipt", [send["hash"]])
            source_wire = json.loads((run / "blocks" / "block-3.json").read_text())
            record.check(
                "replay RPC: v0.4.0 status, head, exact recorded block, and transfer receipt",
                status == {"state": "online", "height": facts["last_height"], "isOurTurn": True}
                and head == facts["last_height"] and block3_wire == source_wire and receipt == send["receipt"],
                f"head={head}, transfer={send['hash'][:12]}…")

            with sync_playwright() as playwright:
                browser = playwright.chromium.launch()
                page = browser.new_page(viewport={"width": 1360, "height": 900})
                # No fixture or check needs an off-host resource. Enforce that even if the config regresses.
                def guard(route):
                    parsed = urlparse(route.request.url)
                    if parsed.scheme in ("http", "https") and parsed.hostname != "127.0.0.1":
                        outside.append(route.request.url)
                        return route.abort()
                    return route.continue_()
                page.route("**/*", guard)
                page.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
                page.on("console", lambda m: m.type == "error" and errors.append(f"console: {m.text}"))
                page.on("requestfailed", lambda r: parsed_local(r.url, base)
                        and "ERR_ABORTED" not in (r.failure or "")
                        and errors.append(f"requestfailed: {r.url} {r.failure}"))

                # Home: both the latest-block table and the decoded activity feed contain all three recorded blocks.
                page.goto(base + "explorer/")
                page.wait_for_function(
                    "() => document.querySelectorAll('ul.feed li.act').length === 3 && "
                    "[3,4,5].every(n => [...document.querySelectorAll('main a')].some(a => "
                    "(a.getAttribute('href') || '').endsWith('#/block/' + n)))",
                    timeout=20000)
                home_text = page.inner_text("main")
                home_hashes = page.eval_on_selector_all(
                    "ul.feed li.act a[href*='#/tx/']", "links => links.map(a => a.href.split('/').pop())")
                record.check(
                    "home: latest activity and blocks show recorded blocks 3, 4, and 5",
                    all(f"#{n}" in home_text for n in (3, 4, 5))
                    and set(home_hashes) == {s["hash"] for s in facts["sends"]},
                    f"activity transactions={len(home_hashes)}")
                expected_amounts = [native_units(s["amount"]) for s in facts["sends"]]
                feed_text = page.inner_text("ul.feed")
                record.check("home: activity shows all three recorded transfer amounts",
                             all(a in feed_text for a in expected_amounts), ", ".join(expected_amounts))
                page.screenshot(path=str(out / "home.png"), full_page=True)

                # Block 3: the transaction table comes from the recorded block and its receipts.
                page.goto(base + "explorer/#/block/3")
                page.wait_for_function(
                    "() => document.querySelectorAll('main table.t tbody tr').length === 3 && "
                    "document.body.innerText.includes('recomputed in this browser: equal')",
                    timeout=20000)
                rows = page.locator("main table.t tbody tr")
                table_rows = rows.evaluate_all(
                    "rows => rows.map(r => [...r.querySelectorAll('td')].map(td => td.innerText))")
                hash_titles = [rows.nth(i).locator("td").nth(1).locator("a").get_attribute("title")
                               for i in range(rows.count())]
                expected_hashes = [t["hash"] for t in recorded3["transactions"]]
                record.check("block 3: all three recorded transactions appear in block order",
                             hash_titles == expected_hashes and len(table_rows) == 3,
                             ", ".join(h[:12] + "…" for h in hash_titles))
                block_text = page.inner_text("main")
                record.check("block 3: recorded block hash recomputes equal in the browser",
                             recorded3["hash"] in block_text and "recomputed in this browser: equal" in block_text,
                             recorded3["hash"])
                record.check("block 3 transfer: outcome is applied, as receipt success=true records",
                             send["receipt"]["success"] is True and "applied" in table_rows[0][7]
                             and "reverted" not in table_rows[0][7], table_rows[0][7])
                charged = send["receipt"]["fee"]["fee_base"] + send["receipt"]["fee"]["tip"]
                expected_fee = f"fee {native_units(charged)} · refund {native_units(send['receipt']['fee']['refund'])}"
                record.check("block 3 transfer: charged base fee plus tip and refund match the receipt",
                             expected_fee in table_rows[0][7], f"expected {expected_fee!r}; row says {table_rows[0][7]!r}")
                system_ok = (len(table_rows) == 3
                             and table_rows[1][4] == "fee" and table_rows[1][5] == "distribute" and table_rows[1][7] == "system"
                             and table_rows[2][4] == "clock" and table_rows[2][5] == "tick" and table_rows[2][7] == "system")
                record.check("block 3: trailing fee-distribute and clock-tick transactions are system transactions",
                             system_ok, f"rows 1-2 statuses={[row[7] for row in table_rows[1:]]}")
                page.screenshot(path=str(out / "block-3.png"), full_page=True)

                # The getTransaction route and the native-token IDL expose exactly the recorded transfer's participants.
                page.goto(base + f"explorer/#/tx/{send['hash']}")
                page.wait_for_function(
                    "() => document.body.innerText.includes('Signers (1)') && "
                    "document.body.innerText.includes('Public transaction') && "
                    "!document.body.innerText.includes('loading…')",
                    timeout=20000)
                tx_text = page.inner_text("main")
                tx_accounts = page.eval_on_selector_all("main a[data-acct]", "links => links.map(a => a.dataset.acct)")
                record.check("block 3 transfer transaction page: sender matches the recorded signer",
                             sender in tx_accounts and sender in tx_text, sender)
                record.check("block 3 transfer transaction page: recipient matches the recorded native transfer argument",
                             recipient in tx_accounts and recipient in tx_text, recipient)
                expected_amount = f"{amount:,}"  # callPanel renders the transfer IDL's raw u128, unlike the LGO feed
                amount_text = page.locator("main dt", has_text="amount").locator(
                    "xpath=following-sibling::dd[1]").inner_text()
                record.check("block 3 transfer transaction page: amount matches the recorded 1,000 native units",
                             amount_text == expected_amount, f"call argument amount={amount_text}")
                record.check("block 3 transfer transaction page: receipt outcome and charged fee are shown",
                             "applied" in tx_text and "receipt success true" in tx_text and expected_fee in tx_text,
                             expected_fee)
                page.screenshot(path=str(out / "transaction-block-3.png"), full_page=True)

                # This sentence is selected by the decoder's v0.4.0 probe, not by the config's version string.
                page.goto(base + "explorer/#/features")
                page.wait_for_function("() => document.body.innerText.includes('What LEZ v0.4.0 does here')", timeout=20000)
                features_text = page.inner_text("main")
                record.check("features page: the probed network version is LEZ v0.4.0 wherever named",
                             "What LEZ v0.4.0 does here" in features_text and "LEZ v0.4.0-rc3" not in features_text,
                             "What LEZ v0.4.0 does here")
                page.screenshot(path=str(out / "features.png"), full_page=True)

                record.check("no script errors, failed local requests, or off-loopback requests",
                             not errors and not outside, (errors + outside)[:8])
                browser.close()
                browser = None
    except Exception as exc:  # keep a machine-readable record even when a driver step itself fails
        # Leaving sync_playwright already closes Chromium after an exception, so its event loop is no longer usable.
        browser = None
        record.check("browser driver completes without an unexpected exception", False, f"{type(exc).__name__}: {exc}")
    finally:
        if browser is not None:
            browser.close()
        if process is not None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
    return record.save({"url": base, "run": str(run), "errors": errors, "off_loopback_requests": outside,
                        "transfer": {"hash": send["hash"], "sender": sender, "recipient": recipient,
                                     "amount_units": amount, "receipt": send["receipt"]}})


def parsed_local(url, base):
    """Page request failure filter, kept separate for Python 3.14's conditional-expression grammar clarity."""
    parsed, local = urlparse(url), urlparse(base)
    return parsed.scheme in ("http", "https") and (parsed.hostname, parsed.port) == (local.hostname, local.port)


if __name__ == "__main__":
    # Record.save returns True only when every check passed; main deliberately returns that boolean for shell exit.
    sys.exit(0 if main() else 1)
