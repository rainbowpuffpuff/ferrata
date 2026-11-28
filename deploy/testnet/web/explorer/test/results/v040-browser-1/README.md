# The explorer on LEZ v0.4.0 in a browser, over real v0.4.0 blocks (28 Nov 2026)

**Run:** `python3 deploy/testnet/web/explorer/test/browser040.py --out DIR` on the laptop, 28 November 2026 about
05:40 UTC, on `lez-040-clients` with the test's two files (this commit): headless Chromium (Playwright) on the
explorer as `test/replay040.py` serves it, a loopback replay of a LEZ v0.4.0 sequencer over the blocks LEZ v0.4.0's
own sequencer served in the recorded live run (`cli/results/v040-standalone-1/blocks/`, blocks 3-5) and their
receipts. Expected values come from that record (`facts.json`, the block JSON), not from the page. Nothing left
127.0.0.1. Written by an e-INFRA Kimi worker and re-run here.

**14 of 14 checks** ([`record.json`](record.json)): the home page's activity and block list show blocks 3-5 and the
three transfers' amounts; block 3 lists its three transactions in order and its hash recomputes equal in the browser;
the wallet's transfer is `applied` with the receipt's fee (4,968 base units, refund 0); the fee-distribute and clock
transactions show as system transactions; the transfer's page shows the recorded signer, recipient and amount (1,000)
and the receipt; the features page names LEZ v0.4.0; no script errors, no failed or off-loopback requests.

**Not shown:** a live v0.4.0 sequencer behind the page (a replay of recorded answers), the explorer's Ethereum-side
panels, private transactions.
