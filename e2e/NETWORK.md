# Public networks: what fails, and what the scripts do about it

Written 18 Nov 2026, after the live run `crown-live-4` stopped in its deploy stage at 15:49Z the way `crown-live-1`
did on 11 Nov: the free endpoint (publicnode, the Secret's `SEPOLIA_RPC_URL`) load-balances over nodes a few blocks
apart, forge simulated against "latest", and its next read reached a node without that block ("block not found",
"non-archive node"). The script had retried 5 times, 24 s apart, and stopped; nothing was lost, because it never sends
after a failed read. That property is kept; the rest is below.

## What fails on a free public endpoint

| Failure | Seen | What it did to the old scripts |
|---|---|---|
| A read at a block another node of the same endpoint announced: `block not found`, `header not found`, `missing trie node`, `failed to get account/storage` | crown-live-1 (11 Nov, three stops), crown-live-3 (12 Nov), crown-live-4 (18 Nov 15:49Z) | forge stops mid-script; the stage was retried at most 5 times, 24 s apart, then stopped for a human |
| A read at "latest" answered by a node behind the transaction just sent | crown-live-1 and -3 (`deposit 1: ... leaves 1`; a nullifier read false right after its release) | a check right after a send fails or logs a stale value (`call_until` was the workaround) |
| History pruned: `eth_getLogs` → code 4444 "pruned history unavailable"; no receipt for a transaction of 12 Nov | publicnode, 18 Nov 16:37Z, `scripts/verify-sepolia.py e2e/runs/crown-live-3` | 4 of 13 checks of a finished run FAIL |
| A node far behind: `0xrpc.io/sep` answered head 12,162,210 while the others said 12,162,585 | 18 Nov 16:19Z probe | a nonce or state read from it is old |
| Free tiers that end or throttle: drpc and 1rpc "not available on free plan" for Sepolia, 1rpc.io/eth discontinued (410), onfinality and zan 429 "register for an API key", tenderly TLS timeouts | 18 Nov probe | an endpoint hard-coded in a script stops working |
| 429, 5xx, timeouts, dropped connections | always | Foundry retries only what alloy classes as retryable (429, -32005, "header not found" exactly, rate-limit texts); "block not found", "missing trie node" and timeouts are not |

The audit of every script that talks to a public network (18 tables, call by call) is in
`spike/tmp/rpc-robust/REPORT.md` beside the repository; its summary is in the commit history of this file.

## What the scripts do now (`scripts/pubnet.py`, `scripts/pubnet.sh`)

1. **An ordered endpoint list per network** ([`../scripts/pubnet-endpoints.json`](../scripts/pubnet-endpoints.json)):
   free, no API key, probed read-only (below). The script's own endpoint (the Secret's) goes first; a local endpoint
   (a fork, a devnet) is the only one, so a public node never answers for a local chain. Requests carry the generic
   User-Agent `curl/8` and nothing about who asks; logs name endpoints by scheme and host only.
2. **Errors classified.** Transient: lag (`block not found`, `header not found`, `missing trie node`, `pruned`, -32001,
   -32005), 429, 5xx, timeouts, connections, and Crown.s.sol's "the wait / release delay is not over" (true at a pinned
   block a few slots longer). Permanent: a revert, invalid input or signature, insufficient funds, nonce too low or
   high, an unknown message. Only transient ones are retried: on the next endpoint at once, then after an exponential
   backoff with jitter (x0.5 to x1.5), within a budget: a request 30 s, a read in bash 5 min, a stage 20 min.
3. **Reads pinned.** forge and cast talk to a local JSON-RPC proxy (`pubnet.py proxy`, started by `pubnet.sh`, one per
   crown run in `drive.py`). It answers "latest" (and a missing block parameter) with a block `lag` behind the
   highest head two endpoints report (3 on a public network, 0 locally), never below the block of the sender's last
   mined transaction and never below an earlier pin. A read at an explicit block is either right or refused by a node
   that does not have it, which then fails over; so every answer a client gets describes one block, and forge's
   simulation runs on that block. That is safe because the plan is checked against the head before anything is sent
   (4): if a transaction of the sender came after the pinned block, the nonce check refuses the send (nothing leaves)
   and the stage plans again. Time checks become conservative: a block 36 s old has an earlier timestamp.
4. **Before a raw transaction leaves**: its nonce must equal the sender's latest and its pending nonce on two
   endpoints (a node that shows an older nonce is behind and ignored; a pending one above the latest means a
   transaction in flight: wait); the balance must cover value + gas x max fee; no earlier transaction of the journal
   may be without a known fate. Then the signed bytes and their hash are appended to `<run>/net/journal.jsonl` and
   fsynced, and only then broadcast to every endpoint (the same bytes are one transaction however many nodes receive
   them; "already known" counts as accepted). Receipts are asked of every endpoint; the first one found wins.
5. **Fates before anything else.** `pubnet.py resolve` finds each journaled transaction's fate: mined (with its
   status), replaced (its nonce used by another transaction and no receipt anywhere for 60 s), or rejected by every
   endpoint; until then the same bytes are broadcast again. `pubnet_stage` resolves before a stage's first run and
   after every run, and runs a failed one again only for a transient reason; a stage that may have broadcast never
   sends again before it has found that transaction's fate. A mined transaction that reverted, a replaced or a
   rejected one stops every later send until an operator has looked (`pubnet.py ack --journal F --hash H`).
6. **Exit status** of `e2e/crown/sepolia.sh` and `vault/script/deploy.sh`: 0 done; 1 permanent (a human looks);
   75 transient with nothing in flight (the budget spent, or a fate still unknown).
7. **The runner** (`e2e/crown/cloud/drive.py`) resumes a transient stop (75, or a read that failed transiently) by
   itself after 2, 4, 8, 16 minutes (+-25 %), at most `CROWN_AUTO_RESUMES` (4) times, each in `crown.log` and in
   `run.json` `auto_resumes`; any other stop waits for the owner, as before.
8. **Records**: the manifest's rows come from the journal (a transaction whose forge run died before its receipt is
   recorded too); `<run>/net/events.jsonl` lists every pin and every error the proxy gave a client.

Knobs (environment): `PUBNET_LAG`, `PUBNET_PIN_TTL_S` (12 s), `PUBNET_TIMEOUT_S` (15 s per try),
`PUBNET_ATTEMPT_TIMEOUT_S` (900: a stage's run is ended after it), `PUBNET_REBROADCAST_S` (30),
`PUBNET_REQUEST_BUDGET_S` (30), `PUBNET_READ_BUDGET_S` (300), `PUBNET_STAGE_BUDGET_S` (1200),
`PUBNET_RESOLVE_BUDGET_S` (600), `PUBNET_BACKOFF_BASE_S` (15), `PUBNET_REPLACED_AFTER_S` (60), `CROWN_AUTO_RESUMES` (4),
`PUBNET_<NETWORK>_RPC` / `_BEACON` / `_BEACON_LC` (replace a list; the tests use them).

### Who uses it

| Script | How |
|---|---|
| `e2e/crown/sepolia.sh`, `vault/script/deploy.sh` | every forge and cast call through the proxy (`pubnet_up`, `pubnet_stage`, `pubnet_read`); the manifest from the journal |
| `e2e/crown/cloud/drive.py` | one proxy per run for its reads, `sepolia.sh` and `verify.py`; automatic resume of transient stops |
| `scripts/verify-sepolia.py` | `pubnet.Net`: failover over the public list, receipts and blocks asked of every endpoint, an outage never scored as a revert |
| `scripts/fetch-mainnet-e2e.py` and through it `fetch-sepolia-gloas.py`, `fetch-sepolia-checkpoint.py`, `lez/scripts/fetch-attest-fixture.py` (the crown's capture), `lez-v03/scripts/fetch.py` | a beacon request tried again on the same endpoint after a transient failure (provenance stays one endpoint per answer); execution providers in rounds |
| not yet: `e2e/sepolia-live.sh` (the first live round trip, superseded by the crown run), `lez-v03/testnet.sh` (the LEZ testnet's sequencer and explorer, one endpoint each), and the scripts that exist only on other branches (`e2e/testnet-return.sh`, `lez-v03-bridge/*.sh`, `lez-v03-return/standin.py`) | the audit lists their calls; they can adopt `pubnet.sh` / `pubnet.Net` the same way |

## Endpoints (probed read-only, 18 Nov 2026 16:17-16:20Z, from the laptop: `python3 -I scripts/pubnet.py probe <network>`)

Each RPC was asked `eth_chainId`, `eth_blockNumber`, the block 3 behind and a balance there; each beacon the head
header and the light-client finality update.

| Network | Endpoint | Answer | In the list |
|---|---|---|---|
| Sepolia RPC | ethereum-sepolia-rpc.publicnode.com | OK (head 12,162,589) | 1 |
| | rpc.sepolia.ethpandaops.io | OK | 2 |
| | ethereum-sepolia-public.nodies.app | OK | 3 |
| | sepolia.rpc.thirdweb.com | OK | 4 |
| | api.zan.top/eth-sepolia | OK at 16:19Z, then "cu limit exceeded" | 5 (last) |
| | sepolia.drpc.org, 1rpc.io/sepolia | HTTP 400 "not available on free plan" | no |
| | eth-sepolia.api.onfinality.io/public | 429, wants an API key | no |
| | sepolia.gateway.tenderly.co, gateway.tenderly.co/public/sepolia | TLS handshake timeout | no |
| | 0xrpc.io/sep | answers, 375 blocks behind | no |
| | rpc.sepolia.org (404), blockpi (503 "Apikey not found"), blastapi (403), omniatech (521), subquery and rockx (no DNS) | refused | no |
| Sepolia beacon | lodestar-sepolia.chainsafe.io | OK, light-client routes too | 1, and the only `beacon_lc` |
| | ethereum-sepolia-beacon-api.publicnode.com, beacon.sepolia.ethpandaops.io | headers OK, `light_client/finality_update` 404 | 2, 3 |
| Mainnet RPC | ethereum-rpc.publicnode.com, eth.drpc.org, rpc.mevblocker.io, eth.rpc.blxrbdn.com | OK | 1-4 |
| | 1rpc.io/eth | HTTP 410 "discontinued" | no |
| Mainnet beacon | ethereum-beacon-api.publicnode.com, lodestar-mainnet.chainsafe.io | OK, light-client routes too | 1, 2 |

## Tests

- `e2e/netfault/`: `faultnet.py` puts three bad endpoints in front of one local anvil (1 s blocks), their faults drawn
  from a seed: a flaky one (`header not found` / `block not found` / `missing trie node`, 429, 502/503, answers later
  than the client waits, a head that goes 1-4 blocks backwards), one 2-6 blocks behind, one that drops, duplicates
  or loses the answer of `eth_sendRawTransaction`, and 0-2 outages of all three (15-60 s). `run.sh <seed>` runs
  `sepolia.sh` deploy, deposit 0, deposit 1, submit, finalize, release 0, release 1 through them with a fresh keystore
  (forge signs: the live path), the stand-in identity and the accept-all verifier of `crown-fork-3r`, resuming a
  transient stop up to 4 times as `drive.py` does; `check.py` then compares the chain (read from anvil directly) with
  the journal. `many.sh 1 24 4` runs 24 seeds; results in [`netfault/results/`](netfault/results/README.md).
  On the build box (Modal), 18 Nov 2026, 48 seeds in two batches, **48 of 48 complete, 0 violations**: every seed
  13 transactions, each journaled before it left, nonces without a gap, all succeeded, 2 deposits, 2 nullifiers and
  6e12 wei left in the vault (crown-fork-3r's figures), no effect twice. Batch 1 (seeds 1-24, stage budget 90 s):
  8,331 faults injected, 13 stage reruns, no transient stop. Batch 2 (seeds 25-48, stage budget 30 s, so that an
  outage spends it): 7,453 faults, 6 stage reruns, 3 transient stops (exit 75) resumed and completed. With the
  crown runner image's own Foundry (nightly a4d66e8e): batch 3, 4 of 4; a batch 4 on a later change found two defects
  (a mined transaction looked up only on the lagging endpoint; a broadcast lost in an outage and never sent again,
  forge waiting 23 min), both fixed (067519c7: lookups ask every endpoint, the bytes go out again while a receipt is
  polled, a run ends after `PUBNET_ATTEMPT_TIMEOUT_S`); batch 4 again on the fixed code: 12 of 12, 0 violations.
  Batch 5 (seeds 65-88, same code): 23 of 24, 0 violations; seed 75 stopped with nothing wrong (forge gave up on a
  receipt of a transaction then found mined, and its message was classed permanent): since then a run that sent,
  with every fate known and good, counts as transient. Batch 6 on that final code (seeds 89-112, the runner's
  nightly, 30 s stage budget): **24 of 24 complete, 0 violations** (3,330 faults, 2 transient stops resumed).
- [`crown/test_send_guard.sh`](crown/test_send_guard.sh): a pending transaction of the deployer, a run killed right
  after its broadcast, a run started while that fate is unknown, the fate found, a replaced transaction and the
  operator's `ack` (anvil without mining, a stub forge that sends through the proxy). 8 of 8 pass.
- [`netfault/test_autoresume.py`](netfault/test_autoresume.py): `drive.py`'s automatic resume with fake stages.

## Limits

- A contract someone else deployed in the last `lag` blocks is not seen yet (a pinned read is 3 blocks old); every
  prerequisite of the crown run is older.
- `eth_getLogs` is answered by one node; a node that clamps a range to its own head returns fewer logs. The crown
  run reads logs only where its manifest lacks a row.
- `deploy.sh` compares forge's sent gas limit with the one DeployVault estimated; both estimates now run at the same
  pinned block, unless the pin moved between them (it moves at most every 12 s).
- `eth_sendTransaction` (a local fork that signs for an impersonated sender) goes to the one local endpoint; its hash
  is journaled when the node returns it.
- The journal holds the signed bytes of every transaction: public once broadcast; one that never left could still be
  broadcast by whoever holds the file, at its nonce, which is exactly the "fate unknown" state the scripts wait out.
