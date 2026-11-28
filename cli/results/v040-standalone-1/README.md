# The v0.4.0 clients against LEZ v0.4.0's standalone sequencer (28 Nov 2026)

**Run:** `cli/v040/live.sh SRC OUT` on a Modal build box, 28 November 2026, 04:11–04:13 UTC ([`logs/live.log`](logs/live.log),
[`facts.json`](facts.json)), from `lez-040-clients` at 6e163884 with the live script's third transfer (this commit),
SRC a copy of the tree prepared by `cli/v040/build.sh`'s rules. (A first run at 03:24Z, two transfers, gave the same
results; this one replaces its record.) LEZ's own `sequencer_service`, built with its `standalone`
feature (its mock Bedrock actor; no Logos Blockchain node) from LEZ's tree at v0.4.0 (`a255b42c`, unmodified): sha256
`8dce25a8ad4d1ba9…`, the same binary as P2's run `e2e/runs/zone-v040-standalone-1` (lez-040-zone); r0vm 3.1.0 (LEZ
PR 1124); LEZ's `local.json` with gossip and metrics on loopback. Nothing left the box; the sequencer was stopped at
the end. Paths of the box are written `<box>/` and `~/` here.

| Client | What it did | Result |
|---|---|---|
| `ferrata lez-version` (cli/v040/build.sh's release build, feature `lez-040`) | asked getSequencerStatus | `this ferrata: LEZ v0.4.0` / `the sequencer …: LEZ v0.4.0` / `match`, exit 0 ([`logs/lez-version.out`](logs/lez-version.out)) |
| the web wallet's v0.4.0 codec (`deploy/testnet/web/wallet/lez040.js`, through `deploy/testnet/web/wallet/test/live040.mjs`) | built and signed two public transfers from LEZ's published local test account (`account import public --secret` in LEZ v0.4.0's Justfile) to account /0 of BIP-39's all-`abandon` test phrase: 1,000 at nonce 0 (the witness carries the account's initial key, 129 bytes), then 2,345 at nonce 1, signed with the key the first rotated to (96 bytes) | both applied: receipts `success: true`, base fee 4,968 and 4,432, storage gas 621 and 554 bytes; the recipient's balance 0 → 3,345; each block's hash recomputed by the wallet's and the explorer's v0.4.0 decoders ([`wallet.json`](wallet.json), [`logs/wallet.out`](logs/wallet.out), [`blocks/`](blocks/)) |
| the web wallet page's network layer (`deploy/testnet/web/wallet/net.js`, as app.js uses it) | `connect()` (v0.4.0, signing allowed); the payer imported as the page imports a raw key (on v0.4.0, the account secret); a fresh account (/1) named by its receive code's admission key (`parseAdmissionKey`); 777 sent at nonce 2 with `net.buildTransfer` | applied, read by `net.outcome` from the receipt RPC; the fresh account holds 777 (block 5) |
| `scripts/lez-read.py --lez URL --json` (`--lez-version auto`) | head, blocks 3, 4 and 5 | decoded as v0.4.0 (the probe), hashes recomputed, each transfer `applied` with its receipt's fee, the trailing fee and clock transactions `system` ([`lez-read-head.json`](lez-read-head.json), [`lez-read-block-3.json`](lez-read-block-3.json), [`lez-read-block-4.json`](lez-read-block-4.json), [`lez-read-block-5.json`](lez-read-block-5.json)) |

Checked by `python3 -I scripts/test_lez040_decode.py` (class `LiveRun`: the recorded blocks hold the recorded
transfers, receipts and witness sizes; lez-read's output agrees). The page itself (its DOM) was not driven: its
network layer was.

**Not in this run:** a claim or a burn on this sequencer (they need the bridge program deployed here and an Ethereum
side; claims, burns and releases ran on the v0.4.0 harness instead: `cli/results/v040-e2e-1`), `ferrata attest` on it
(the attestation program is not deployed here), the CLI's own sending commands (they need a vault; each runs
`lez-version`'s check before it signs: the zone commands always, `deposit` whenever a sequencer is named, and a
v0.4.0 build sends no deposit without one), a private transaction (P2's run proves one with LEZ's wallet), a Logos Blockchain
node, the page's DOM.

**Earlier attempts (not records):** the first launch stopped as the box's r0vm was 3.0.5 (LEZ v0.4.0's genesis needs
3.1.0); the next two stopped in the wallet's script on the box's node 18 (a missing import, then no global Web Crypto;
both fixed in b2de7a2c).
