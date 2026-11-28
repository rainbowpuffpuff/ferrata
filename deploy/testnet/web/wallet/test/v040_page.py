#!/usr/bin/env python3
"""Browser-test the wallet's LEZ v0.4.0 path against the local v040_stub.py sequencer.

    python3 deploy/testnet/web/wallet/test/v040_page.py --out DIR      (TMPDIR: a short path, or none)

Only loopback is contacted.  Chromium imports BIP-39's public all-`abandon` phrase, sends 2.5 configured token units
from account /0 to /1, and Node independently decodes and verifies the wire recorded by the stub.  Checks are written
to OUT/record.json; any failed check (or an interrupted test) gives exit status 1.
"""
import argparse
import errno
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

from playwright.sync_api import sync_playwright

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
PHRASE = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
PASSPHRASE = "v040 browser test passphrase"


class Record:
    def __init__(self, out):
        self.out, self.checks, self.t0 = out, [], time.time()

    def check(self, name, ok, detail=""):
        self.checks.append({"name": name, "ok": bool(ok), "detail": str(detail), "t": round(time.time() - self.t0, 1)})
        print(("PASS " if ok else "FAIL ") + name + (f": {detail}" if detail else ""), flush=True)
        return bool(ok)

    def save(self, extra):
        passed = sum(check["ok"] for check in self.checks)
        with open(os.path.join(self.out, "record.json"), "w", encoding="utf-8") as f:
            json.dump({"passed": passed, "total": len(self.checks), "checks": self.checks, **extra}, f, indent=1)
        print(f"{passed} of {len(self.checks)}", flush=True)
        return passed == len(self.checks)


def node(js, *args):
    """Run the wallet codec exactly as pwa_e2e.py's helper does."""
    return subprocess.run(["node", "--input-type=module", "-e", js, *args], cwd=ROOT, check=True,
                          capture_output=True, text=True).stdout


def wait_js(page, expression, timeout=15000):
    """Poll without page.wait_for_function: the page CSP forbids its eval."""
    end = time.time() + timeout / 1000
    last = None
    while time.time() < end:
        try:
            if page.evaluate(expression):
                return True
        except Exception as error:  # a navigation can race a poll
            last = error
        time.sleep(0.25)
    raise TimeoutError(f"{expression} (last error: {last})")


def browser_tempdir(out):
    """Chromium keeps Unix sockets under TMPDIR, whose path must stay short (107 bytes for the whole socket path): TMPDIR
    when it is set, short and writable; otherwise OUT itself through a short /proc/PID/fd path (Linux), so that every
    browser write stays in --out and no other temporary folder is made.
    """
    given = os.environ.get("TMPDIR", "")
    if given and len(given) <= 40:
        try:
            probe = tempfile.mkdtemp(prefix="v040-probe-", dir=given)
            os.rmdir(probe)
            return given, given
        except OSError as error:
            if error.errno not in (errno.EROFS, errno.EACCES, errno.EPERM, errno.ENOENT):
                raise
    fd = os.open(out, os.O_RDONLY | os.O_DIRECTORY)
    return f"/proc/{os.getpid()}/fd/{fd}", out


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def rpc(port, method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    request = urllib.request.Request(f"http://127.0.0.1:{port}/rpc", data=body,
                                     headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(request, timeout=2) as response:
        return json.load(response)


def await_servers(rpc_port, web_port, processes):
    deadline = time.time() + 20
    while time.time() < deadline:
        for process in processes:
            if process.poll() is not None:
                raise RuntimeError(f"local server exited early with status {process.returncode}")
        try:
            if rpc(rpc_port, "getSequencerStatus").get("result", {}).get("state") != "online":
                continue
            with urllib.request.urlopen(f"http://127.0.0.1:{web_port}/wallet/config.json", timeout=2) as response:
                if json.load(response).get("token", {}).get("decimals") is not None:
                    return
        except (OSError, ValueError, urllib.error.URLError):
            time.sleep(0.1)
    raise TimeoutError("the loopback sequencer or web server did not start")


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


EXPECTED_JS = r"""
import * as lez from './lez040.js';
import { seedOf } from './lez.js';
import { readFileSync } from 'node:fs';
const phrase = process.argv[1], decimals = JSON.parse(readFileSync('./config.json', 'utf8')).token.decimals;
const payer = lez.deriveNode(seedOf(phrase), [0]), recipient = lez.deriveNode(seedOf(phrase), [1]);
const payerKey = lez.hex(lez.initialKey(payer.secret)), recipientKey = lez.hex(lez.initialKey(recipient.secret));
console.log(JSON.stringify({
  payer: lez.idToString(payer.accountId), payerKey,
  recipient: lez.idToString(recipient.accountId), recipientKey,
  payerCode: `lez:${lez.idToString(payer.accountId)}?pk=${payerKey}`,
  recipientCode: `lez:${lez.idToString(recipient.accountId)}?pk=${recipientKey}`,
  amount: lez.parseUnits('2.5', decimals).toString(), decimals,
}));
"""


VERIFY_JS = r"""
import * as lez from './lez040.js';
import { seedOf } from './lez.js';
const [phrase, wire, loggedHash, expectedAmount] = process.argv.slice(1);
const out = { public: false, hash: false, nonce: false, authorities: false, initial: false, signature: false, admission: false, amount: false };
try {
  const payer = lez.deriveNode(seedOf(phrase), [0]), recipient = lez.deriveNode(seedOf(phrase), [1]);
  const decoded = lez.decodeWire(wire), tx = decoded.tx, transfer = lez.nativeTransferOf(tx);
  out.hash = decoded.hash === loggedHash;
  out.public = decoded.kind === 'public' && transfer !== null && tx.entries.length === 1 && out.hash;
  out.decodedHash = decoded.hash;
  out.nonce = tx.nonces.length === 1 && tx.nonces[0][1] === 0n && lez.equalBytes(tx.nonces[0][0], payer.accountId);
  // LEZ's native transfer: exactly the payer's and the recipient's native-token actors, the payer's a signer and the
  // recipient's not, and one witness (the payer's).
  const same = (a, b) => lez.equalBytes(a.account, b.account) && lez.equalBytes(a.program, b.program);
  const declared = (id, authority) => tx.actors.some((a) => same(a.actor, lez.nativeActor(id)) && a.authority === authority);
  out.authorities = tx.actors.length === 2 && declared(payer.accountId, 'signer') && declared(recipient.accountId, 'none')
    && tx.entries.length === 1;
  const auth = tx.entries[0].auth_data, signing = lez.signingKey(payer.secret, 0n);
  out.initial = auth.length === 129 && lez.equalBytes(auth.subarray(0, 33), lez.initialKey(payer.secret))
    && lez.equalBytes(tx.entries[0].next, lez.keyState(payer.secret, 1n));
  const keyed = lez.messageWithKeys(tx.messageBytes, tx.entries.map((entry) => entry.next), 'public');
  out.signature = auth.length === 129 && lez.equalBytes(auth.subarray(97), signing.pk)
    && lez.verify(auth.subarray(33, 97), keyed, signing.pk);
  out.admission = tx.evidence.length === 1 && !!tx.evidence[0].key
    && lez.equalBytes(tx.evidence[0].key, lez.initialKey(recipient.secret))
    && lez.equalBytes(lez.accountIdForInitialKey(tx.evidence[0].key), recipient.accountId);
  out.amount = transfer !== null && transfer.amount === BigInt(expectedAmount)
    && lez.equalBytes(transfer.from, payer.accountId) && lez.equalBytes(transfer.to, recipient.accountId);
} catch (error) {
  out.error = String(error && error.message || error);
}
console.log(JSON.stringify(out));
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    rec = Record(out)
    extra = {"phrase": "BIP-39 all-abandon public test vector", "amount": "2.5"}
    processes, streams, context, profile = [], [], None, None
    interrupted = None
    try:
        rpc_port, web_port = free_port(), free_port()
        while web_port == rpc_port:
            web_port = free_port()
        wire_log = os.path.join(out, "wires.jsonl")
        open(wire_log, "w", encoding="utf-8").close()
        for name, command in (
            ("stub", [sys.executable, "-I", os.path.join(HERE, "v040_stub.py"),
                      "--port", str(rpc_port), "--log", wire_log]),
            ("web", [sys.executable, "-I", os.path.join(HERE, "pwa_serve.py"),
                     "--host", "127.0.0.1", "--port", str(web_port), "--upstream", f"http://127.0.0.1:{rpc_port}"]),
        ):
            stream = open(os.path.join(out, name + ".log"), "w", encoding="utf-8")
            streams.append(stream)
            processes.append(subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT))
        await_servers(rpc_port, web_port, processes)
        base = f"http://127.0.0.1:{web_port}"
        extra.update({"base": base, "rpc": f"http://127.0.0.1:{rpc_port}/rpc", "wire_log": wire_log})
        expected = json.loads(node(EXPECTED_JS, PHRASE))
        extra["expected"] = expected

        errors, foreign = [], []
        browser_tmp, physical_tmp = browser_tempdir(out)
        os.environ["TMPDIR"] = browser_tmp  # inherited by Playwright's driver and all Chromium processes
        tempfile.tempdir = None
        extra["browser_tmp"] = {"path": browser_tmp, "physical": physical_tmp}
        profile = tempfile.mkdtemp(prefix="v040-", dir=browser_tmp)
        with sync_playwright() as playwright:
            context = playwright.chromium.launch_persistent_context(
                profile, headless=True, base_url=base,
                viewport={"width": 412, "height": 915}, device_scale_factor=2.625,
                is_mobile=True, has_touch=True, env={**os.environ, "HOME": browser_tmp})
            page = context.pages[0] if context.pages else context.new_page()
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("request", lambda request: foreign.append(request.url)
                    if not (request.url == base or request.url.startswith(base + "/") or request.url.startswith("data:")) else None)
            page.goto("/wallet/")
            page.wait_for_selector("#v-welcome", state="visible", timeout=30000)
            page.click("#go-import")
            page.fill("#import-phrase", PHRASE)
            page.fill("#import-pass", PASSPHRASE)
            page.click("#import-go")
            page.wait_for_selector("#v-main", state="visible", timeout=30000)
            wait_js(page, "document.getElementById('home-nonce').textContent === '0'", 30000)

            page.goto("/wallet/#settings")
            wait_js(page, """(() => { const rows = [...document.querySelectorAll('#net-info dt')];
              return rows.some((dt) => dt.textContent === 'LEZ version' && dt.nextElementSibling.textContent.includes('v0.4.0')); })()""", 30000)
            version = page.evaluate("""(() => { const dt = [...document.querySelectorAll('#net-info dt')]
              .find((row) => row.textContent === 'LEZ version'); return dt ? dt.nextElementSibling.textContent : ''; })()""")
            rec.check("Settings reports LEZ v0.4.0", version == "v0.4.0", version)

            page.goto("/wallet/#receive")
            page.wait_for_selector("#recv-qr svg", state="visible", timeout=15000)
            admission = page.text_content("#recv-pk")
            own_code = page.text_content("#recv-code")
            rec.check("receive admission key is 66 hex digits starting 00", bool(re.fullmatch(r"00[0-9a-f]{64}", admission)), admission)
            rec.check("receive code equals Node's lez040.js account /0 derivation", own_code == expected["payerCode"], own_code)

            page.goto("/wallet/#send")
            page.fill("#send-to", expected["recipientCode"])
            page.fill("#send-amount", "2.5")
            page.click("#send-review")
            page.wait_for_selector("#send-confirm", state="visible", timeout=30000)
            page.click("#send-go")
            wait_js(page, "document.getElementById('send-result').textContent.includes('applied')", 60000)
            outcome = page.text_content("#send-result")
            extra["outcome"] = outcome

            with open(wire_log, encoding="utf-8") as f:
                wires = [json.loads(line) for line in f if line.strip()]
            if len(wires) != 1:
                raise AssertionError(f"the stub recorded {len(wires)} wires, expected one")
            logged = wires[0]
            extra["transaction_hash"] = logged.get("hash")
            verified = json.loads(node(VERIFY_JS, PHRASE, logged["wire"], logged.get("hash", ""), expected["amount"]))
            extra["wire_verification"] = verified
            rec.check("recorded wire decodes as the named v0.4.0 public native transfer", verified["public"], verified.get("error", verified.get("decodedHash", "")))
            rec.check("wire payer nonce is 0", verified["nonce"])
            rec.check("actors: the payer's and recipient's native-token actors, the payer alone signing, one witness", verified["authorities"])
            rec.check("nonce-0 witness carries the payer initial key and rotates its key", verified["initial"])
            rec.check("witness signature verifies over messageWithKeys with the nonce-0 key", verified["signature"])
            rec.check("recipient initial key state is the admission evidence", verified["admission"])
            rec.check("wire amount is 2.5 times 10^config.token.decimals", verified["amount"], expected["amount"] + " atomic units")
            rec.check("page reports the transaction outcome as applied", "applied" in outcome and "msg ok" in page.get_attribute("#send-result", "class"), outcome)

            page.click("#lock")
            page.wait_for_selector("#v-unlock", state="visible", timeout=15000)
            page.fill("#unlock-pass", PASSPHRASE)
            page.click("#unlock-go")
            page.wait_for_selector("#v-main", state="visible", timeout=30000)
            page.goto("/wallet/#receive")
            page.wait_for_selector("#recv-qr svg", state="visible", timeout=15000)
            unlocked_code = page.text_content("#recv-code")
            rec.check("lock and passphrase unlock preserve the receive code", unlocked_code == own_code, unlocked_code)
            rec.check("no page script errors or non-origin requests", not errors and not foreign,
                      " | ".join(errors + foreign))
            context.close()
            context = None
    except Exception as error:  # save all checks already made before an interrupted/failed UI step
        interrupted = error
        extra["interrupted"] = f"{type(error).__name__}: {error}"
        rec.check("browser test ran to completion", False, extra["interrupted"])
    finally:
        if context is not None:
            try:
                context.close()
            except Exception:
                pass
        for process in reversed(processes):
            stop(process)
        for stream in streams:
            stream.close()
        if profile:
            shutil.rmtree(profile, ignore_errors=True)
    ok = rec.save(extra)
    sys.exit(0 if ok and interrupted is None else 1)


if __name__ == "__main__":
    main()
