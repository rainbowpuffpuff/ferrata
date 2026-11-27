# The Ferrata web wallet for the LEZ v0.4 test network

**Test network only (its coins have no value). Prototype, unaudited.** A static page (no build step, no server of its
own) that the test network serves at `/wallet/`, beside its RPC at `/rpc` and its faucet at `/faucet`.

## What it does

| Flow | How | What is public, what stays private |
|---|---|---|
| Create or import | A 24-word BIP-39 phrase made in the page, or imported; or a raw LEZ private key. Keys are derived as LEZ's own wallet derives them (`/LEE-Keys/v1/Master/Public` tree, the tweak), so the phrase opens the same accounts in LEZ's wallet CLI and in the `ferrata` tool | Nothing leaves the page; the record in `localStorage` is encrypted (PBKDF2-SHA-256, 600,000 iterations; AES-256-GCM) |
| Balance, nonce | `getAccountBalance`, `getAccountsNonces`, `getAccount` | Public on LEZ |
| Send, receive | Native transfers signed in the page (LEZ's public transaction, BIP-340 over its message hash). The receive code `lez:<id>?pk=<key>` carries the public key LEZ needs to credit an account the first time (admission evidence) | Sender, recipient, amount, fee, nonce are public |
| Outcome | The block that holds the transaction, read for its `reverted` list (rc3's blocks carry it): applied or reverted, not just "included"; "outcome unknown" if the block the RPC names does not list it | |
| Faucet | `POST /faucet` with the account id and its public key | Public |
| Activity | The page reads blocks (`getBlockRange`, 50 at a time) and keeps the transfers touching its accounts, encrypted | Only in this browser |
| Prove an Ethereum fact (RFP-021) | Pick a Sepolia contract and storage slot (or a mapping entry, or an account); the page reads the attestation instance's newest record from LEZ, fetches `eth_getProof` through the user's browser wallet (or the network's Ethereum endpoint), checks the Merkle-Patricia proof and the condition itself, then sends the `Attest` request and reads accepted or refused from the LEZ block | The fact is a public statement on LEZ |
| Bridge deposit (RFP-022) | ETH or the network's ERC-20 test token (approval of exactly the total, then the deposit) to FerrataVault, signed by the user's own browser wallet (MetaMask or any EIP-1193 wallet). `C` is derived in the page from the phrase exactly as the `ferrata` tool derives it, so the tool can claim it. Destinations: this phrase's own accounts only (the selected public account, or its private account /0); a deposit to anyone else's account could never be claimed (its owner lacks `s`), so the page offers none | Public on Ethereum: address, amount, `C`, time. Private: the LEZ destination inside `C` |
| Private accounts | The phrase's private account /0 and its receive keys (nullifier key, ML-KEM-768 viewing key), derived in the page with LEZ's private key tree (checked against LEZ's pinned vectors); a bridge deposit can commit to it | The keys never leave the page; a private account's balance is not public |
| Bridge claim, public (RFP-022) | A deposit to one of the wallet's public accounts: "Find my deposits on the vault" (the vault's Deposit events, opened with the phrase's `s` and `C`), then "Claim on LEZ", the bridge program's plaintext `Claim` signed in the page (no proof). When the live bridge lacks them, the page first runs the two steps anyone may run for any deposit: the fee-range import (`ImportFees`, "no further schedule") and the root import (`ImportRoot`), both proven under the attestation instance's newest record with `eth_getProof` through the user's browser wallet (`claim.js`) | Public: the deposit (leaf, depositor, amount) linked to the destination account, and `s` (it opens this deposit only) |
| Bridge burn, public | One denomination from a public account's holding at `holder(cid, account)`: the note (r, memo, mac, COut) built as the `ferrata` tool builds it, so the tool releases it from the same phrase. "Review the burn" shows the account, the denomination, the Ethereum recipient with its source (typed, or the browser wallet's address) and its EIP-55 check (a mixed-case address that fails it is refused), the counter and the fee cap; "Sign and burn" sends exactly that plan | Public: the account, asset, amount, time and the note. Private: the Ethereum recipient (encrypted in the memo under the phrase's `kView`) |
| Private claim, private burn, release; private transfers | Shown as the `ferrata` tool's steps from the same phrase: a private claim or burn needs a proof of LEZ's privacy circuit with the account's keys, and the release proof's private input is the burn's index, path, `sOut` and `r` (which burn is released), so a prover service would learn the link; the tool proves on the user's machine (see the Private tab: 49.5 s on an L4 GPU, 264-887 s on 32 CPU threads; no wasm32 build of RISC Zero's prover) | |

## Files

The site is exactly the files `node test/pwa-files.mjs --list` prints (the service worker keeps the same list, with
SHA-256): `index.html`, `app.js` (the page), `lez.js` (LEZ rc3's keys, borsh, signing, block and transaction decoding,
RPC), `keystore.js`, `applock.js`, `history.js`, `prove.js` + `attest.js` + `eth.js` (attestation, RLP, header rebuild,
MPT proofs, EIP-55), `bridge.js` + `claim.js`, `private.js`, `inbox.js` + `msg/` (the messages client), `notify.js`,
`pwa.js`, `qr.js`, `sw.js`, `manifest.json`, `icons/`, `style.css`, `vendor/` (`noble.js`: noble-curves, -hashes,
-ciphers, -post-quantum and scure-base, -bip39 at the versions of `vendor/package-lock.json`, rebuilt by
`vendor/build.sh`; `names.js`, `qr.js`), and the host's own `config.json`. `test/` and this README are not part of it.

The page's Content-Security-Policy allows scripts, styles and connections to its own origin only.

## Tests

```sh
node --test test/*.test.mjs                       # 53: LEZ's pinned vectors, recorded Attest messages, SPEC-v2 and IDL vectors, the PWA list, the review's findings, the bridge tab's plain words
bash test/box.sh build                            # LEZ rc3's sequencer + wallet CLI (LEZ's source beside the repository)
bash test/box.sh e2e NAME                         # with Foundry and vault/lib present, the bridge steps too
bash test/box.sh testers NAME                     # the host's layout (serve.py --site): the testers' path from the build page
```

`box.sh` keeps its build in `WALLET_WORK` (default `ferrata-scratch/wallet/box` beside the repository); on a build
machine that saves only some directories between runs, point `WALLET_WORK` at one of them.

- `test/ux.test.mjs`: the bridge tab against ethskills' frontend-ux, qa and wallets checklists (27 November 2026): a
  browser wallet's errors (declined, a request already waiting, not allowed, disconnected, no Sepolia ETH) and the
  vault's refusals by name (each held to its declaration in `vault/src/FerrataVault.sol`, wherever the wallet nests
  the revert data) in plain words next to the action; the review's cost priced live (`eth_estimateGas`,
  `eth_gasPrice`, rounded up to the gwei) with the Sepolia ETH it needs, refused before any signature when the
  balance falls short; a name typed as the burn's recipient refused with its reason. In the page, each button holds
  one request at a time, and the review shows the addresses EIP-55 checksummed.
- `test/test_serve_site.py`: `serve.py --site` serves the bundle at `/` and the wallet at `/wallet/`, and nothing
  outside the bundle (`..`, an encoded `..`, a symlink out, a file type it does not serve), in CI's `links` job.
- `test/testers_path.py` (`box.sh testers`): what a visitor does, in headless Chromium against a local rc3 sequencer
  and anvil, with the pages laid out as the host serves them (`serve.py --site BUILD_DIR`: the build page at `/`, the
  wallet at `/wallet/`): the build page's reviewer link to the testers' page, its link to the wallet, a new wallet,
  the faucet, a deposit declined in the browser wallet and then made, the private destination's note and its link to
  step 6e, a deposit to the private account, a browser wallet without Sepolia ETH refused at the review, "Find my
  deposits". Not the claim: that needs the attestation and bridge programs on the zone (the record says so).
- `test/review.test.mjs`: the findings of the independent review that unit tests can hold: amounts with a comma are
  refused, a transaction missing from its block is "outcome unknown", EIP-55 (the EIP's own examples), a record past
  200 KB, the record's iteration count kept on save, no second wallet over the first, embedded trie nodes (a forged
  extra proof node is never used), the Activity cap.
- `test/conformance.test.mjs`: key derivation (LEZ's `keys_public.rs` master and child vectors, `tweak`), the public
  message's pinned layout and hash, BIP-340 signing, a transfer through the decoder.
- `test/attest.test.mjs`: the `Attest` request equals, byte for byte, the messages `ferrata-attest` recorded for
  Sepolia USDC slot 11 (with and without a predicate; `docs/guides/runs/2026-11-21-lez/attest-messages/`); the header
  rebuilt from `eth_getBlockByHash` JSON equals the recorded `header.rlp`; the proofs hold, a tampered node is refused.
- `test/bridge.test.mjs`: `kAuth`, `s`, `C` equal every case of `bridge/vectors/inbound-v2.json`.
- `test/claim.test.mjs`: the deposit leaf, `fid` and `N` of every `inbound-v2.json` case, the claim case's tree root and
  path; the burn note (`r`, `sOut`, memo, `COut`, mac, `NOut`) of every `outbound-v2.json` case; the `Claim`, `ImportRoot`,
  `ImportFees` and `Burn` messages equal, byte for byte, the program's own IDL vectors.
- `test/private.test.mjs`: the private key tree (ssk, ccc, ask, nsk, npk, the 1,184-byte viewing key) against LEZ's
  `keys_private.rs` `master_key_generation`, read from LEZ's source when it is beside the repository.
- `test/e2e.sh` + `test/e2e_test.py`: LEZ v0.4.0-rc3's standalone sequencer (2 s blocks), a faucet stand-in signing
  with LEZ's own wallet CLI from LEZ's published local test key, anvil as Sepolia with the vault, a stand-in EIP-1193
  wallet; headless Chromium; `test/serve.py` serves every file a deployment installs (`test/pwa.test.mjs` checks it
  against the service worker's list). Record `test/results/local-9/record.json`: **61 of 61** (fee of a transfer
  4,352 atomic units, 3.4 s to its block; a transfer LEZ reverts, reported as reverted, its fee
  804,088). It also shows LEZ refusing a credit to a never-used account named without its key. (`local-7`, 55 of 55, ran
  before the app lock and PWA files existed.)
- `test/fork_bridge.py`: the bridge tab against the deployed Sepolia vault on an anvil fork of Sepolia (nothing reaches
  Sepolia; `--fork-url`, `--deployment vault/deployments/sepolia-31-B.json` (on main since `b9057be6`),
  `--config-id`, and `--out`, the directory the record is written to; all four are required):
  deposits refused while `bridge.lezReady` is false and when `bridge.configId` is not the vault's `lezConfigId`; an ETH
  deposit and a test-token deposit (mint, exact approval, deposit), each checked against the fork's state. Record
  `test/results/fork-31B-2026-11-23/record.json`: **22 of 22**.
- config.json's `bridge`: `program`, `vault`, `evmChainId`; `configId` (checked against the vault's `lezConfigId()` on
  connect); `lezReady` (`false` closes deposits until the LEZ side is set up); `token` (`address`, `symbol`, `decimals`,
  `testMint`: offer the test token's open mint).
- `test/public_read.py`: read-only against the public network (no faucet, no transaction). Record
  `test/results/public-read-2026-11-23/record.json`: 7 of 7.

The run records of 23 November (`live-*`, `live-claim-2026-11-23`) and `../android/published/apk.json` name commits and bundle ids of the wallet's
development history, which is not public; the merged state is `e8e2c007`, whose served files equal those reviewed at the
end of that history but for comments.

## As an app: installable, offline, on Android

The same page is an installable app (a PWA) and, wrapped as a Trusted Web Activity, an Android app
(`../android/`: one program, one review).

| Piece | How |
|---|---|
| Install | `manifest.json` and `icons/` (rendered from the SVGs by `test/make_icons.py`); Chrome, Edge and Android offer installation (`beforeinstallprompt`: the header's Install button); iPhone and iPad: Share, then Add to Home Screen (the More tab says so) |
| Offline | `sw.js` keeps exactly one build: the files `test/pwa-files.mjs` lists with their SHA-256, each checked before it is kept (a half-deployed or swapped build is refused and the previous one stays). `config.json` is fetched from the network first. RPC and faucet calls are POSTs and pass straight through. A new build waits for the "Reload" banner |
| Unlock (`applock.js`) | Passphrase as before; or a **passkey** (WebAuthn PRF: the authenticator releases a secret only after fingerprint, face or screen lock; HKDF turns it into the key that wraps the record's AES key; no server, nothing to guess offline); or a **PIN** of 6-12 digits (PBKDF2-SHA-256, 600,000 iterations; erased after 5 wrong tries, a limit of the page's own: someone who copies the browser's storage can try every 6-digit PIN offline). Optional lock when the app leaves the screen |
| QR (`qr.js`, `vendor/qr.js`) | The receive code as a QR (with `&amount=` when one is asked for); scanning by camera (BarcodeDetector where the browser has one, paulmillr's decoder otherwise) or from a photo fills Send's recipient and amount; frames stay in the page |
| Notifications (`notify.js`) | No push service. While open and unlocked the page reads each new block itself (every 15 s on screen, 60 s off), recomputes its hash, checks its link to the block before, and picks out transfers and messages to its accounts: Inbox badge, a line on screen, a notification (amount, or "new encrypted message": never a text or a sender). Account state is refreshed when a block touches the accounts, and every 30 s otherwise (was 6 s) |
| Block hashes (`lez.js`) | `decodeBlock` recomputes LEZ rc3's block hash from the block's own bytes (`hashOk`); More, "What this page checks", counts them |

**Deploying** this bundle: run `node test/pwa-files.mjs` (it rewrites `sw.js`'s list) and install every file of
`node test/pwa-files.mjs --list` except `config.json`, which is the host's own. A deployment that changes files without
the new `sw.js` leaves installed apps on the previous build (the service worker serves what it checked).

Tests: `node --test test/*.test.mjs` (with `test/pwa.test.mjs`: the list, the manifest and icons, block hashes of
the public network's blocks, the PIN wrap, the passkey wrap, scanned codes, QR round trip);
`test/pwa_e2e.py` (record `test/results/pwa-e2e-2026-11-24-r6-local/record.json`: 27 of 27 at the reviewed tip;
Chromium, phone screen, `test/pwa_serve.py` with the host's headers; it moves `TMPDIR` to a short
directory when the inherited one is long, as full Chromium's socket paths need: install verdict, QR by zbar,
fake-camera scan, virtual-authenticator passkey with and without PRF, PIN, offline, update, tampered build refused);
`test/pwa_live.py` (two wallets on the public test network: message and transfer notifications);
`test/pwa_measure.py` (cold start, unlock times, data per minute).

## Security trade-offs (RFP-022 soft requirement 5 asks for them)

This page holds the recovery phrase in the browser, so it trades safety for convenience in a way the repository's
other web page does not: `web/depositor/` takes only `C` and `j` from the Logos tool and never sees the seed.

| Threat | What it could do here | What limits it |
|---|---|---|
| A compromised or phishing copy of the page | Read the phrase as it is typed or unlocked, and with it every LEZ account and the bridge keys of that phrase | Test network only; open the page from the network's own host; the page loads no third-party script (CSP `script-src 'self'`) and talks only to its own origin (`connect-src 'self'`, checked by the e2e); use a phrase made for the test network, never one that holds value |
| Another page of the same host (build page, explorer, status, trust, basecamp, the Android page): the wallet shares one web origin with them | Read the encrypted record and the PIN wrap (a 6-digit PIN then falls offline in about a minute), trigger the passkey prompt (same `rpId`), open `/wallet/` in a same-origin window and script it once unlocked | Only the host's `script-src 'self'` on every page and those pages' own code (no script gadget found in them by the review). The page says so on its welcome screen and in More. For value, serve the wallet from an origin of its own (e.g. `https://wallet.<host>`, with `/rpc` and `/faucet` on it too; `config.json` `ownOrigin` naming that origin hides the notice there only): a path-scoped storage key would not help, the origin is the boundary |
| A malicious browser extension or injected script | The same, once the wallet is unlocked | Auto-lock after idle (5-60 min); the record at rest is encrypted (PBKDF2-SHA-256 600,000 iterations, AES-256-GCM); on lock the page drops its references to keys, reviews and plans (JavaScript cannot erase memory; the collector frees it) |
| Someone with the device | Copy the encrypted record and try passphrases offline | The passphrase's strength and PBKDF2's cost; minimum 8 characters |
| Someone with the device, with PIN unlock on | Copy the storage and try every PIN offline (6 digits: about a minute on one GPU, estimated from hashcat's PBKDF2-SHA-256 rate) | Passkey or passphrase unlock instead; the PIN's 5-try limit only stops someone using the screen |
| The passkey's provider (the phone's secure hardware, a password manager, a security key) | Release the PRF secret that unwraps the wallet's key | It does so only after user verification; a synced passkey is as safe as that account; turning passkey unlock off deletes the wrap |
| A swapped deposit commitment or vault | Send the deposit to an attacker's `C` or contract | The review shows `C`, the vault and the calldata; the browser wallet's own confirmation shows the real calldata; chain, account and index are checked again right before signing; a review is dropped when its form or the LEZ account changes |
| A mistyped burn recipient | The burn's note binds the Ethereum recipient: the release pays it, with no way back | A review before signing shows the recipient with its source; a mixed-case address must pass EIP-55; an all-lowercase one is marked "compare every character" |
| The network's RPC or faucet | Lie about balances, blocks or outcomes | It cannot sign for you; the page verifies what it can (Ethereum proofs against the record's block hash; the vault's Deposit event against `C`); LEZ's RPC itself is trusted for reads, as in LEZ's own wallet |

For value, use the Logos modules or the `ferrata` tool, where the seed stays outside the browser.
