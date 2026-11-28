# The wallet as an app with the network's LEZ version unread (round 2, H1), 28 Nov 2026

**Run:** `python3 test/pwa_e2e.py --out DIR` (headless Chromium through Playwright, the bundle served by
`test/pwa_serve.py` with no `--upstream`, so `/rpc` answers HTTP 404 and the page cannot read the network's LEZ
version), on the laptop, 28 November 2026 about 04:30 UTC, twice:

| Tree | Result |
|---|---|
| `lez-040-clients` d985b86f (before the fix: a failed version read made every account function throw) | stopped after its first four checks (service worker, installability, manifest, offline copy): creating a wallet never showed the main view (`#v-main` stayed hidden for 30 s; [`before.txt`](before.txt)) |
| `lez-040-clients` 5781824f (the fix: `net.js` `unconfirmed`, accounts as on the version last read, else v0.4.0-rc3; nothing signed until a re-read) | 27 of 27 checks, among them "offline: the app opens, unlocks and shows the receive QR" and "no script errors" ([`after.txt`](after.txt), [`record-after.json`](record-after.json)) |

zbar was not installed, so the receive QR was checked by its `data-text` only (as the record says). The offline
step's status pill read "RPC unreachable". Not run: `pwa_webkit.py`, `--upstream` (live block hashes).
