# The wallet's Android app

**Test network only. Prototype, unaudited.** The web wallet (`../wallet/`) as an Android app: a Trusted Web Activity
(TWA). Android opens the network's `/wallet/` in Chrome's engine, full screen, once it has checked that the site names
the app's signing certificate in `/.well-known/assetlinks.json` (Digital Asset Links). The app holds no wallet code of
its own: the app and the web wallet are one program, reviewed once.

| | |
|---|---|
| Package | `io.github.rainbowpuffpuff.ferrata.wallet`, version 0.1.0 (code 1) |
| Android | 7.0 (API 24) or later, with Chrome or another browser with TWAs (Edge, Samsung Internet); without one it opens in a Custom Tab |
| Permissions | `POST_NOTIFICATIONS` (the page's notifications, delegated to the app), and AndroidX's internal receiver permission; no internet permission of its own (Chrome does the networking) |
| Size | 1,133,430 bytes signed (v2 and v3 schemes); the unsigned APK is 1,085,123 bytes |
| Signing | RSA 4096, PKCS#12 with a random password, never in the repository; certificate SHA-256 in `published/apk.json` and on the `/android/` page. **The key of version 0.1.0 was made on the build box and stays in its scratch volume, beside its password**: whoever holds that build workspace's token can sign an update every installed copy accepts. See "Signing" below for keeping the next key offline |
| Rebuilds | `build.sh` builds the project twice from two separately generated directories and requires the unsigned APKs to be equal byte for byte (they are: `dfc14b16…`). Both builds ran on one machine with one Gradle cache and one SDK: this shows the build is deterministic there, not that an independent machine gets the same bytes |

## Files

- `twa.json`: the app's settings for Bubblewrap's generator, with `{{HOST}}` (the host's name is not in the repository)
  and `{{ICONS}}` (where the generator fetches the icons: the build serves `../wallet/icons/` on 127.0.0.1).
- `gen.mjs`: runs `@bubblewrap/core`'s `TwaGenerator` (version pinned by `package-lock.json`) on `twa.json`.
- `build.sh`: SDK (Google's command-line tools, SHA-1 checked against Google's repository list; platform 36, build
  tools 35.0.0), generator, two builds and their comparison, the key (made on first use), `zipalign`, `apksigner sign`
  and `verify`, `aapt2 dump badging`; writes `apk.json`, `assetlinks.json`, `SHA256SUMS` to its output directory.
- `site/`: the `/android/` page. It reads `apk.json` and `measure.json` beside it and runs its own checks in the
  visitor's browser: the APK's SHA-256 and size, Digital Asset Links, the newest 20 blocks (hashes recomputed, links
  checked, with the wallet's own `lez.js`), the wallet's security headers, and every wallet file against its service
  worker's list.
- `published/`: what was published for the current version (`apk.json`, `assetlinks.json`, `SHA256SUMS`,
  `apksigner-verify.txt`, `measure.json`).

## Build

```sh
M=../modal-build   # from the worktree
$M/mrun -- 'HOST=<host> COMMIT=$(...) bash deploy/testnet/web/android/build.sh'
```

The host serves the page at `/android/` (`site/` plus the APK, `apk.json`, `SHA256SUMS`, `measure.json`) and
`assetlinks.json` at `/.well-known/assetlinks.json`.

## Signing: where the key lives, rotation, loss

- `build.sh` no longer makes a key unless asked (`MAKE_KEY=1`): make the release key offline (`keytool -genkeypair
  -keystore release.p12 -storetype PKCS12 -keyalg RSA -keysize 4096 ...` on a machine you control), bring it into
  `KEYDIR` for the build only, and set `DROP_KEY=1` so the copy is removed when the APK is signed. The password file
  travels with it, so treat the pair as one secret; a hardware or cloud key store is better still.
- Rotation: APK signature scheme v3 carries a proof of rotation (`apksigner rotate --old-signer ... --new-signer ...`
  makes a lineage; sign the next version with `--lineage`), so installed copies accept the new key; publish both
  certificates in `assetlinks.json` during the change, then only the new one.
- Loss: without the key no update installs over the app. The way out is a new package name or users uninstalling and
  installing an APK signed by a new key (and `assetlinks.json` naming it). For a test-network app that is acceptable;
  for value it is not, which is one more reason to keep the key offline and backed up.

## What it checks, and what it does not

It runs no node on the phone. Against an app that carries a light client of the chain, it is weaker on one point: who
made a block and whether it is final are trusted to the network's RPC, as are account balances (LEZ's RPC gives no state
proofs). What it checks itself: every block it reads (hash recomputed from the block's contents, link to the block
before), transaction outcomes in the block itself, Ethereum facts by Merkle-Patricia proof against the attested block
hash, receive codes (key against account id), its own build (each file against its SHA-256), and, on the `/android/`
page, the APK against its published hash. What it does not isolate: the wallet shares its web origin with the
network's other pages (build page, explorer, status, trust, Basecamp, this page), so a script on any of them could read the
wallet's encrypted record and PIN wrap, ask for its passkey, or drive an unlocked wallet; only the host's
`script-src 'self'` policy and those pages' own code stand in the way (the wallet's README, Security trade-offs). Messages are hybrid post-quantum (X25519 and ML-KEM-768) and padded, but have
no forward secrecy (the messaging keys derive from the account's key).
