# The test network's site: explorer, attestation status, programs, agents

**Prototype, unaudited.** A static bundle (no build step, no dependency, no external CDN) that the test network's host
serves at its root. Every page reads the chain from the visitor's browser over the sequencer's JSON-RPC and decodes it
there.

| Path | Page | What it does |
|---|---|---|
| `/` | `index.html`, `assets/landing.js` | what the network is, its live state in one row, where to go |
| `/explorer/` | `assets/explorer.js` | blocks, transactions (private ones: their public parts), accounts, programs, search; Ferrata's messages and accounts decoded through `idl/`; every block hash recomputed; an attest transaction's facts re-verified |
| `/explorer/` (home) | `assets/explorer.js`, `activity.js`, `features.js`, `names.js` | stats (height, latest block, measured block time, finality on the Logos Blockchain node, user transactions, chain id = genesis hash), a notice when the genesis changed since the last visit, a live activity feed decoded into sentences (transfers and the faucet, names, encrypted messages by size only, light-client updates, attestations, bridge, deployments; refreshed every 10 s), a faucet box (account id or @name), LEZ v0.4's features with evidence, links to the wallet, the Basecamp mini-app and AGENTS.md |
| `/explorer/?name=alice`, `#/name/alice`, `#/messages`, `#/features` | | a name's record and transactions; recent encrypted messages (sender, recipient, time, envelope size; never content); every LEZ feature with its evidence on this chain and in LEZ's source |
| `/status/` | `assets/status.js`, `attest.js`, `facts.js`, `chart.js` | each light-client instance's finalized slot, lag behind Ethereum's finality, committee period, weak-subjectivity end and alarms; its record checked against public beacon APIs and an execution RPC from the browser; the latest updates and attested facts, each re-verified; the host's own monitor (`status/status.json`, `history.json`) with 24 h charts |
| `/registry/` | `assets/registry.js` | Ferrata's programs (image ids, binaries, sources, IDLs, each checked against its loader header on chain), LEZ's built-ins, the apps and tools, how to deploy a program |
| `/agents/`, `/AGENTS.md` | `assets/agents.js`, `md.js`; `AGENTS.md` | the agents' guide, rendered and raw |
| `/agents/ferrata_check.py`, `.mjs` | | the guide's minimal scripts (read-only) |

## What it decodes, and from where

- `assets/lez.js`: LEZ v0.4.0-rc3's `Block` and `LeeTransaction` (public and privacy-preserving) in borsh, from the
  tag's sources; block hash and transaction hash; account ids of signers and PDAs; small IDLs of LEZ's built-in
  programs (native token, program loader, clock, fee, bridge, sequencer stake).
- `assets/lez040.js`: LEZ v0.4.0's (receipts in place of the reverted list, actor authorities, witness entries with key
  states, the privacy-preserving execution's grants, clear publications and proven-fee tip), with the same object
  shapes; `assets/rpc.js` picks it when the network answers `getSequencerStatus` (LEZ v0.4.0 added it), rc3's
  otherwise; a v0.4.0 page shows each transaction's receipt and charged fee (`assets/outcome.js`). Tested on vectors
  LEZ v0.4.0's crates made: `node --test deploy/testnet/web/explorer/test/decode040.test.mjs`.
- `assets/idl.js`: SPEL's IDL format (as `idl/check.py`); `idl/` here is a copy of the repository's `idl/`, the bridge's from `idl/deployed/` (the test network runs B1's program; since ADR 0048 the repository's `idl/ferrata_bridge.idl.json` is the next program's, `ferrata_bridge_lezfee`).
- `assets/ferrata.js`: ferrata-core's `CompactStore` and `HeaderRecord`, the beacon header's `hash_tree_root`, the
  attestation program's PDAs.
- `assets/eth.js`: Keccak-256, RLP, Merkle-Patricia proofs (state, storage, receipts), an attest request's conditions.

## Configuration

`config.json`: the RPC (`rpc`, default same-origin `/rpc`), the public URLs, the faucet, the native token, the program
catalogue (`programs`, `known_images`), light-client `instances`, the beacon APIs and execution RPCs the status page
checks against (they must send `Access-Control-Allow-Origin: *`), alarm thresholds. The deployment's `programs.json`
(written by `deploy/testnet/deploy-programs.sh publish`) is merged at load: deployed program accounts by image id,
instances by id. `?rpc=URL` on any page reads another endpoint for the session.

Names and messages: `idl/ferrata_names.idl.json` and `idl/ferrata_msg.idl.json` are the names and messages programs' IDLs
(on main since `eccc02a4` and `acf6fce0`; the deployed images `0xee9578ca…` and `0x04ae1988…`). Their second versions
(names v2 `0x28d63f20…`, messages v2 `0x480f5c4e…`, deployed beside them on 24 November 2026) are what the wallet and
`/msg/` use; this explorer still reads the first versions only. `agents/sections/` are the streams' own AGENTS sections
for the second versions, copied whole (`names/AGENTS.md`, `deploy/testnet/web/msg/AGENTS-section.md`).
A message's envelope is never rendered: the transaction page, the feed, the messages page and inbox summaries show
its size only.

The RPC client pages `getBlockRange` at the gateway's limit (`range_limit`, lowered at once if the gateway names a
smaller one) and waits out HTTP 429 (`Retry-After`) up to three times.

## Tests (all local; nothing leaves the machine)

```sh
bash deploy/testnet/web/explorer/test/run.sh
```

1. `test/decode.test.mjs`: the 182 blocks of the recorded rc3 run (`lez/live/results/v0.4.0-rc3`) decode with every
   byte used, each hash recomputed equals the header's (the 17 blocks whose program chunks the record elided differ,
   exactly those), transaction hashes equal `run.tsv`'s, every built-in message decodes, 73 attestation messages
   decode with the IDL, signers equal the nonces' accounts; 11 recorded privacy-preserving transactions decode; the
   PDA vectors of `idl/vectors/` derive again. Also runs on the rc1 run (`lez/live/results`, 191 blocks).
2. `test/eth.test.mjs`: Keccak vectors; Sepolia block 12,105,936's header hashes to its hash; the six recorded attest
   messages of `docs/guides/runs/2026-11-21-lez/attest-messages/`: every proof verifies, the storage value equals the
   fixture's, and the browser's verdict equals the program's (two refused with 130, four applied).
3. `test/alarms.test.mjs`: the status page's 14 alarm rules.
4. `test/fixtures.mjs` builds the offline world `test/replay_rpc.py` serves: the recorded blocks, account states built
   in the programs' encodings from real Sepolia data, three **synthetic** blocks carrying the recorded attest messages,
   a mock beacon API and execution RPC with the fixture's real answers, `programs.json` and the host's status files in
   their formats.
5. `agents/ferrata_check.py` and `.mjs` against that world; `test/browser_test.py`: every page in headless Chromium,
   with the replay holding one block back so the feed's refresh is seen, a stand-in faucet recording what the faucet box
   sends, and a changed genesis for the reset notice. `test/live-snapshot.json` (written by `test/snapshot_live.mjs`,
   read-only from the test network) supplies real names, messages and faucet transactions and their accounts.

Against a live network, read-only: serve the bundle with a config whose `rpc` is the network's, then
`python3 test/browser_test.py --url http://127.0.0.1:PORT/ --live`.

`agents/build_agents_md.py --record SITE --label TEXT` records every read method once from a live RPC (never
`sendTransaction`) and rewrites `AGENTS.md` from `config.json`, those responses and the scripts.
