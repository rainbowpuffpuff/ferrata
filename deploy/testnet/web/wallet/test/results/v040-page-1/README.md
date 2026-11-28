# The wallet page on LEZ v0.4.0 in a browser, against a v0.4.0 stand-in (28 Nov 2026)

**Run:** `python3 deploy/testnet/web/wallet/test/v040_page.py --out DIR` on the laptop, 28 November 2026 about 05:55
UTC, on `lez-040-clients` after the final review's fixes (the commit that rewrote this README; first run 12 of 12 at
daa16c80 with the test's first version): headless Chromium (Playwright) on the
wallet as `test/pwa_serve.py` serves it, its `/rpc` passed to `test/v040_stub.py`, a loopback stand-in answering as a
LEZ v0.4.0 sequencer (`getSequencerStatus` with a `state`, receipts; it keeps balances and nonces and records each
wire it receives). Nothing left 127.0.0.1. Written by an e-INFRA Kimi worker and re-run here.

**13 of 13 checks** ([`record.json`](record.json)): the page says LEZ v0.4.0; imported from BIP-39's all-`abandon`
test phrase, its receive view shows a 66-hex admission key starting 00 and the receive code node computes with
`lez040.js` for /0; a send of 2.5 to /1's receive code produced one wire that node decodes as a v0.4.0 public transfer
at nonce 0, its actors exactly the payer's and the recipient's native-token actors (the payer's a signer, the
recipient's not, one witness), its witness carrying the payer's initial key, its signature valid over `messageWithKeys` with the nonce-0
key, /1's initial key state as admission evidence, the amount 2.5 × 10^9; the page reported it applied (from the
receipt); after lock and unlock the receive code is the same; no script errors, no request off the origin.

The stand-in refuses a native transfer with other actors or other signers: with the wallet's recipient declared a
signer, or both actors given another program id (mutations of `lez040.js`, undone), the run fails (the page never
reports the send applied).

**Not shown:** a real sequencer (the live run, `cli/results/v040-standalone-1`, sent through the page's network layer
in node), the claim and bridge tabs, the messages tab.
