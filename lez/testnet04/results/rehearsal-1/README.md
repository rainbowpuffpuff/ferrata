# Rehearsal 1: day one, on our own LEZ v0.4.0-rc3 zone as if it were the public network

18 November 2026, on a Modal build box (CPU container, gVisor). The command of day one, `lez/testnet04/day1.sh`,
ran against a zone we started (LEZ v0.4.0-rc3's `sequencer_service`, built from the unmodified tag `bafd07be`, and its
Logos Blockchain node 0.3.5-rc.1 `f6d4267f`, one validator, leader proofs in dev mode, 10 s blocks), reached by URL
through this machine's own non-loopback address, with the sequencer stopped and started again twice. Nothing was sent to
any public network: Sepolia's beacon and execution APIs were only read.

## What is real and what is a stand-in

| | in this rehearsal | on day one |
|---|---|---|
| the network | **stand-in**: our own rc3 zone (`lez/live/run.sh run`, phase `hook`), LEZ's `local.json` network | testnet 0.4 at the URL Logos publishes |
| the URL | **stand-in**: `http://<this machine's address>:3041`, `forward.py` to the sequencer's loopback RPC: the script and the daemon reach it as a remote endpoint (the read and send guards in force: `FERRATA_SEQUENCER_REMOTE`, `FERRATA_TESTNET_SEND=yes`) | the public RPC |
| funding | **stand-in**: LEZ's published local test account pays the payer and the daemon key (`ferrata-day1 fund`, 5·10^10 each) | the owner's deposit through the zone's bridge ([`../../NOTES.md`](../../NOTES.md) §2) |
| the programs | **real**: the final binaries, checked by sha256 and `r0vm --id` against `docs/image-ids.md` before signing | the same |
| Sepolia | **real**: a fresh finalized checkpoint (Lodestar's bootstrap, publicnode's header), the daemon following Sepolia live, `eth_getProof` of the live vault `0x04eE…e44c` from publicnode's execution API | the same |
| keys | fresh random keys made by `day1.sh plan`, beside the repository (not committed) | the same |

## What ran (UTC; `out/events.tsv`, `out/run.log`, `out/run-2.log`, `state/`)

| | when | what happened |
|---|---|---|
| discover | 17:48:40 | failed: `getBlock` answers base64 of the block's borsh, which discover read as JSON (fixed in `36cd3f5a`; run again at 17:52:19 and inside `run` at 17:53:58: `out/discover-2.txt`, `state/discover.json`) |
| plan | 17:48:40-17:48:42 | fresh keys; every stage signed in a scratch state and priced at the zone's base fees (8, 8): payer to hold at least 918,676,600, the daemon key at least 32,839,200,000 for two days (`state/plan.txt`) |
| fund (stand-in) | 17:48:42-17:49:00 | 5·10^10 to each, in 8.0 s and 10.0 s (`out/fund.log`) |
| (a) the attestation program | 17:49:00-17:52:16 | 8 stored transactions, applied in blocks 5-20; **the sequencer stopped (SIGTERM) at 17:50:04 after the third, down until 17:50:36**: the stage failed three times ("connection closed before message completed"), was tried again, found the three applied and sent the other five's stored bytes |
| (b) `Init` | 17:52:32-17:52:36 | a fresh finalized checkpoint, slot 11,607,488 (period 1416), root `0x430bcdd1…8d1d`, Lodestar's bootstrap and publicnode's header agreeing (`state/fetch/`); applied in block 22 |
| (c) the daemon | 17:52:52 | refused its configuration (`[policy]` written without the fields that have no default): `run` stopped there. Fixed in `day1.sh` (`36cd3f5a`) and `day1.sh run` started again at 17:53:58: (a) and (b) found applied (nothing sent again), the daemon started at 17:54:00 |
| (d) the attestation | 18:39:47-18:40:05 | once the instance had applied an update after its activation: the newest record (slot 11,607,488, execution block 12,162,964 `0x4683d675…83a5`), `eth_getProof` of the vault's slots 0 and 1 there, the verdict computed natively, sent, applied in block 67 |
| (e) the bridge program | 18:40:27-19:02:47 | 7 stored transactions, applied in blocks 69-80; the owner-side Sepolia step written (`state/bridge-owner-step.md`), not sent |
| `run` | 19:02:47 | "run done in 4128 s" (from 17:53:58; three node stalls of 20 minutes inside it, below) |
| the daemon follows | 17:54:00-19:19:17 | 85 minutes; the sequencer stopped again at 18:10:54 (head block 131) and started at 18:11:44 |
| verify | 19:17:13-19:17:17 | 23 of 23 checks, and the instance's finalized header canonical at publicnode: exit 0 (`out/verify.txt`, `state/verify.json`) |
| end | 19:19:17 | the daemon stopped (SIGTERM), the zone stopped by `run.sh` |

## Figures (from the stored results, `state/tx/<stage>/NN.result.json`, and the daemon's `state/daemon/state/ledger/run.tsv`)

| | transactions | blocks | execution gas | storage gas | fees |
|---|---:|---|---:|---:|---:|
| (a) attestation program, 7 segments and an immutable header | 8, all applied | 5-20 | 1,091,406 | 599,500 | 13,527,248 |
| (b) `Init` (Sepolia, Gloas layout, 2 adoption parts) | 1, applied | 22 | 114,811 | 1,699 | 932,080 |
| (d) `Attest`: the vault's account and slots 0 and 1 | 1, applied, 1 event | 67 | 251,015 | 6,409 | 2,059,392 |
| (e) bridge program, 6 segments and an immutable header | 7, all applied | 69-80 | 1,022,882 | 564,664 | 12,700,368 |
| (c) the daemon (its own key) | 11 in its journal, all applied (and one more, below) | 28-93 | 34,862,007 | 101,464 | 279,707,768 |

- (a) equals, to the unit, what LEZ v0.4.0-rc3's sequencer charged the live driver for the same binary on 14 November
  (EVIDENCE row 112; `lez/live/README.md`, "LEZ v0.4.0-rc3": 1,091,406 execution gas, 599,500 storage gas, 13,527,248 in base fees), from a different account
  layout and payer: the cost of the deployment is the binary's.
- (d)'s verdict (`state/tx/attest/stage.json`), the program's own function on the record the zone holds, equal to what
  the settled transaction emitted and to `eth_getProof`'s values (`state/fetch/proofs/11607488/proof.json`): the vault
  `0x04eE…e44c` at Sepolia block 12,162,964 has nonce 1, balance 6,000,000,000,000 wei, slot 0 (the deposit root)
  `0xa67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469`, slot 1 (the leaf count) 2, the crown run's two
  deposits. The record was the checkpoint's own (slot 11,607,488); its block was about an hour old when publicnode served
  its proofs.
- (c): the daemon applied two updates: one adopting period 1417's committee (finalized slot 11,599,872, 506 of 512
  signers; `Open`, `Miller`, two `Adopt` parts, `Finish`) and a finality update to slot 11,607,712 (493 of 512 signers;
  `Open` 2,507,676, `Miller` 3,305,374, `Finish` 3,721,523: 9,534,573 gas); a third (to 11,607,839) had `Open` and
  `Miller` applied when it was stopped. The instance's finalized header at the end, slot 11,607,712, root
  `0xbc10e751…9803`, is publicnode's canonical block at that slot. Every fee equals what the payer's balance fell by.
  `/health` read every minute: 85 probes, 83 answered (64 `ok`, 19 `degraded`), none `down`; the two unanswered were
  before the second start listened (`out/health.tsv`).

## The stalls and the restarts

The node (0.3.5-rc.1, one validator) stopped mining the zone's inscriptions three times, each for 20 minutes, then
the sequencer gave up on them and rewound its head to the last final block (`zone/logs/sequencer.log`):

| no block final | for | the sequencer's rewind |
|---|---:|---|
| 17:53:15 (block 24) to 18:13:19 | 1,204 s | 18:13:03, head 138 to 24 |
| 18:17:07 (block 48) to 18:37:27 | 1,220 s | 18:37:07, head 169 to 48 |
| 18:41:07 (block 71) to 19:01:27 | 1,220 s | 19:01:07, head 192 to 71 |

and a fourth began at 19:05:07 (block 94) and lasted to the end. This is the stall rc3's sessions met (EVIDENCE row 112:
"120 only after 1,214–1,216 s"; row 122: "Head rewound from 148 to 28"), here three times in an hour. Nothing was
done by hand to the zone, the script or the daemon: transactions caught in a stall were included again after the
rewind and settled (the daemon: "moved from block 54 to block 49: the head was rewound", settled after 1,179.4 s;
`Miller` after 1,223.4 s). `day1.sh run` and the daemon rode both sequencer restarts: the script by trying its
stage again and sending the stored bytes, the daemon by failing a round on the closed connection (five rounds) and
on the head disagreeing with the last final block (seven rounds), then carrying on.

One thing the restart did show: the daemon's first `Adopt` part, sent at 17:54:36 into the first stall, was waiting
to settle when the sequencer stopped at 18:10:54; the wait ended with the connection, so the daemon's journal has no
row for it, while the sequencer included it again after its rewind (18:13:04, its log) and the daemon read it as
done from the chain and sent the second part (block 28). The journal of a daemon restarted mid-wait can miss a
transaction the chain holds; the chain stays right.

**The operator's one intervention**: the rehearsal's hook (`../../rehearse.sh`) was paused (SIGSTOP) from 18:15:01
to 19:17:13, because the first stall had taken 20 of its 35 minutes of following; `/health` was sampled by a
separate loop meanwhile (`out/events.tsv` says so). The zone, the script and the daemon were not touched.

## Found by the rehearsal, fixed (host code; commit `36cd3f5a`)

1. `discover` read `getBlock`'s answer as JSON: LEZ serves the block as base64 of its borsh (header first, the Bedrock
   status last). It now decodes the header's id, hash and time and the status, and fails on its own error.
2. The daemon's configuration: `[policy]` has no per-field defaults; `day1.sh` now writes the whole section.
3. `ferrata-day1` read state through the live driver's `actor_state`, which also requires the sequencer's head to
   equal the follower's last final block: true when nothing is in flight, false while the daemon sends. The tool now
   reads the follower's state (the chain replayed from the endpoint's blocks to the newest final one) for `inspect`,
   `target`, `sign` and `verify`. The new binary replaced the old one at about 17:53 (atomically; no stage was running).

## What it does not show

- A public network: our zone, one validator, leader proofs in dev mode; LEZ's `local.json` with the test accounts it
  funds. The funding step, the endpoint's TLS, rate limits and other users' transactions were not exercised.
- Seven days, or even an hour of uninterrupted following: blocks became final during about 11 of the daemon's 85 minutes
  (18:13:19-18:17:07, 18:37:27-18:41:07, 19:01:27-19:05:07); the rest were the node's stalls.
- A network on a later LEZ than rc3 (`../../NOTES.md` §6): the tool and the daemon were built against rc3.
- The bridge's configuration (it needs the vault, the owner's separate Sepolia step) and any private transaction.

## Re-check

```sh
python3 -I -m json.tool lez/testnet04/results/rehearsal-1/state/verify.json   # the 24 checks as the run recorded them
lez/testnet04/day1.sh verify URL    # against a running zone that holds this run (needs the keys, not committed)
```

The records here are the run's own: `out/` (the hook's logs and events), `state/` (what `day1.sh` kept: the stored
transactions' metadata and results without their signed bytes, the plan, the checkpoint and the proofs fetched, the
daemon's configuration, log, journal and manifest; no key and no wallet), `zone/` (the zone's configuration and the
sequencer's log), [`environment.md`](environment.md).
