# monitor: the watchers' node-side program

`monitor` follows one Ferrata vault and checks every candidate checkpoint against the watcher's own
nodes. When the design says a candidate is wrong, `monitor` cancels it before its wait ends. It also
pauses checkpoints when the design calls for a pause, raises an alert for a wrong checkpoint that has
already been finalised (and revokes it when its key is the freeze authority's), and does all of this
safely across restarts. The vault side (`watchers`, `cancelCandidate`, `pauseCheckpoints`) is in
[`vault/`](../vault/README.md). This program is the other half: SPEC-v3 D4, D11 and D13
([`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md)) and
[`docs/guides/operator-guide.md`](../docs/guides/operator-guide.md) §7.

Prototype evidence for a grant proposal. **Not audited. Not for production.**

The operator's journey (install, configure, run, monitor, stop) with the run that exercised each step:
[`docs/guides/operator-guide.md`](../docs/guides/operator-guide.md) §0; what a watcher's outage or malfunction does
and what bounds it: [`docs/trust.md`](../docs/trust.md) §4.3.

## What it decides

For each pending candidate (`CandidateSubmitted`, re-read from `candidates(newState)` at every poll),
against every node view the watcher reads (all of them must agree):

| Check | Rule (B) | Rule (A) |
|---|---|---|
| The node's final block at `tipId` has another hash | cancel at once | cancel at once |
| `n`, `t` are not the live key set's at the tip (`t(n)` as LEZ computes it) | cancel at once | cancel at once (D13: a stale, smaller set would shorten the wait) |
| The node computes the checkpoint fields at the tip (`newState`, `burnRoot`, `burnCount`, `importedLeafCount`, `sunset`) and one differs | cancel at once | cancel at once |
| The tip is not (yet) in the node's final tier | wait at most `--b-lag-grace` (300 s) after the submission, then cancel (D13: the tip must already be final when the candidate is submitted) | wait until `submittedAt + Δ_g` (`--delta-g`, 2 h), then cancel (D4) |
| Everything holds | leave the candidate alone (`candidate-passed`) | same |

A wait never runs past the deadline (`submittedAt + deltaAtSubmit + (t − 1) × turnSeconds`) minus
`--margin` (120 s): by then the monitor cancels. A state that has been cancelled once and is submitted again is
cancelled again, and the monitor then pauses checkpoints for 12 hours (D11: a persistent attacker gets
the checkpoint freeze; `--pause-after-cancels`, default 2). Under rule (A), a final tier that has not
advanced for `--stall-pause-after` seconds (D4: `(t − 1) × turnSeconds + S/2`; off by default) also
brings the pause. A finalised checkpoint whose tip fails the same checks (because the watcher was
down) raises `finalized-wrong` with `revokeFrom(index)` and its deadline `finalizedAt + revokeWindow`.
A monitor holding the freeze authority's key sends that `revokeFrom` itself; a watcher key cannot
revoke anything.

Under rule (B), with `--channel-id`, the monitor also audits the final-tier attestations the attestors
publish (in the node view), using [`attestor-core`](../checkpoint/crates/attestor-core/src/lib.rs). For
each attestation it checks that the channel key is in the live key set at the tip, that the certificate
was made for this deployment (chain, vault, `lezConfigId`, channel), and that the final-tier message v2
signature is valid over the live `keySetHash`. It then compares the signed tip with the final tier. A
certified key that signed a tip conflicting with the final tier is a critical alert (a corrupted
attestor). The `candidate-passed` alert reports how many distinct live keys attested the candidate's
tip.

What it cannot see: the candidate's journal carries `n` and `t` but not `keySetHash`. A forged
candidate with the right `n` but other keys is therefore caught by its tip, or by `newState` when the
node computes it, and not by the key-set check alone.

## The node view

A node adapter gives one JSON document per poll, either as a file it rewrites (`--node file:PATH`)
or as the standard output of a command (`--node cmd:COMMAND`). `--node` can be repeated, one per node.

```json
{
  "node": "lez-1",
  "finalHeight": 258,
  "finalBlocks": { "257": "0x…", "258": "0x…" },
  "keySets": [ { "fromBlock": 0, "keys": ["0x…", "0x…", "0x…", "0x…"], "postingTimeframe": 900 } ],
  "states": { "258": { "newState": "0x…", "burnRoot": "0x…", "burnCount": 3, "importedLeafCount": 2, "sunset": false } },
  "attestations": ["0x<SignedAttestation::encode, 264 bytes>"]
}
```

- `finalBlocks` must hold every final block above the vault's last checkpoint. A block at or below
  `finalHeight` that is missing counts as not final.
- `keySets`, `states` and `attestations` are optional. When one is absent, the check that needs it
  is skipped.

`monitor node-view --chain-tsv FILE [--key-sets FILE] [--label NAME]` builds the view from a LEZ
v0.4.0-rc1 sequencer's block record in the lab driver's `chain.tsv` format (`lez/lab/driver`: each
block's id, hash and `bedrock_status` as `getBlock` serves them). The final tier is the run of
`Finalized` blocks. It can be used as `--node 'cmd:monitor node-view --chain-tsv …'`. A client that
reads the sequencer's RPC directly is not built.

## Running it

```sh
cargo build --release            # its own workspace, like cli/ (alloy 1.8.3, attestor-core by path)
MONITOR_KEY=… target/release/monitor run --rpc http://127.0.0.1:8545 --vault 0x… \
  --node file:/var/lib/ferrata/node-view.json --state /var/lib/ferrata/monitor-state.json \
  --key-env MONITOR_KEY --alert-log /var/log/ferrata/alerts.jsonl --alert-cmd '/usr/local/bin/page-me' \
  --start-block <the vault's deployment block>
```

- **Key.** The watcher key is given through `--key-env VAR` (hex in the environment), `--key-file FILE`
  (hex in a file only the monitor's user reads) or `--keystore FILE --password-file FILE`, never on the
  command line. A monitor without a key, or with
  `--dry-run`, decides and raises alerts but sends nothing.
- **Alerts.** Each alert is one JSON line on standard error and in `--alert-log`. Alerts of at least
  `--alert-level` (warn) also go to `--alert-cmd`, which gets the line on its standard input. Kinds:
  `started`, `stopped`, `candidate-passed`, `candidate-waiting`, `cancelled`, `cancel-failed`, `late`,
  `would-cancel`, `blind`, `candidate-resubmitted`, `persistent-candidate`, `paused`, `would-pause`,
  `final-tier-stalled`, `finalized-wrong`, `revoked`, `revoke-failed`, `checkpoints-revoked`,
  `attestation-finding`, `node-unreadable`, `poll-failed`.
  - `poll-failed` is rate-limited and becomes critical after 5 minutes of failures.
  - `cancelled` carries the transaction, `secondsAfterSubmission` and `secondsBeforeDeadline`.
- **Restart safety.** The vault is the source of truth. The state file (`--state`) is rewritten
  atomically (temporary file, fsync, rename) after every poll and around every transaction.
  - A restarted monitor rescans the vault's events from its last block, less `--reorg-margin`. Without a
    state file it rescans from `--start-block`. Either way it re-reads every pending candidate from the
    vault before it acts.
  - Each cancellation is counted once, keyed by its transaction. A replayed submission counts only
    the cancellations before it.
  - An OS lock on `<state>.lock` allows one monitor per state file. The lock is released when the
    process ends, even by SIGKILL.
  - A sent transaction that is not mined within `--tx-timeout` (180 s) does not stop the loop. The
    next poll looks at the vault again.
- **Offline verdict.** `monitor check --rule B --candidate c.json --node file:view.json --now T`
  prints the verdict.

## Tested

All runs were on the Modal build box (`mrun --profile ws-13`), never on the laptop.

- `cargo test --release`: 15 tests in [`tests/check.rs`](tests/check.rs). They cover the rule (B)
  and (A) verdicts, the margin before the deadline, key-set rotation, the state fields, several
  nodes, the audit of finalised checkpoints, the attestation audit with real Ed25519 certificates
  and final-tier messages (good, conflicting, stale key set, dead channel key, another deployment's
  certificate, malformed), the node-view JSON, the state file, and the node view built from the lab's
  real record (258 `Finalized` blocks of a LEZ v0.4.0-rc1 sequencer; the same record with blocks 257
  and 258 made `Safe` and `Pending` ends the final tier at 256; a gap is refused). They also cover the live round trip's own rule (B) checkpoint (`e2e/runs/crown-live-1`): its real attestation passes the audit, with the certificate by the devnet sequencer's channel key and the final-tier message v2 over tip 252 and the live key set. Its 17-word journal passes as a candidate against the devnet's final tip. A flipped byte in the certificate, the signature or the signed tip is refused.
- [`tests/anvil-e2e.sh`](tests/anvil-e2e.sh): a local anvil chain (chain id 31337, no fork, nothing
  sent to any public network). It deploys a v5 vault through
  [`vault/script/MonitorFixture.s.sol`](../vault/script/MonitorFixture.s.sol), with an accept-all
  verifier (the monitor never reads proofs) and the monitor's fresh key as the watcher. All 46 checks
  pass ([`results/anvil-e2e/summary.md`](results/anvil-e2e/summary.md)).

- [`tests/fork-live-vault.sh`](tests/fork-live-vault.sh): an anvil fork of Sepolia at the live v5 vault
  of the round trip. anvil reads Sepolia over a public RPC; nothing is sent to Sepolia. All 8 checks
  pass ([`results/fork-live-vault/summary.md`](results/fork-live-vault/summary.md)). Two edits exist on
  the fork only: the monitor's fresh key is written into `watchers[0]` (storage slot 73; the live vault
  has no watchers), and for F1 and F2 the verifier's code is replaced by the accept-all stop.
  - **F0.** The monitor scans the vault's own events from its deployment block 12,111,048. It audits
    the live checkpoint as `ok` against the devnet's final tip, and finds the round trip's real
    attestation good.
  - **F1.** A forged candidate on the live final state (tip 252, another hash) is cancelled 1 s after its
    submission, 3,599 s before its deadline.
  - **F2.** A candidate on the real tip with the live key set (n = t = 1) passes untouched, attested
    by the live key.

The scenarios of the local-chain rehearsal:

| | Scenario | Outcome |
|---|---|---|
| S1 | honest candidate (rule B); the node view carries the attestors' published final-tier messages (real Ed25519, `examples/attestations.rs`) | passed, untouched for 3 polls, its tip attested by 3 live certified keys; the certified key that signed a forged tip 12 raised a critical `attestation-finding` (`conflictingTip`); finalised after the wait |
| S2 | tip conflicting with the final tier | cancelled in the first poll; `finalizeAttested` refused after the wait |
| S3 | stale key set (n = 3, t = 2 against four keys) | cancelled (`keySetNotLive`) |
| S4 | honest and wrong pending together | only the wrong one cancelled; the honest one finalised |
| S5 | monitor SIGKILLed while waiting on a tip its node lacks | a second monitor on the same state file is refused; after the restart from the state file, cancelled 600 s after the submission, 3,000 s before the deadline |
| S6 | state file lost | a fresh monitor rescans from the deployment block and makes exactly one cancellation (the pending wrong one); the two finalised checkpoints are audited as good |
| S7 | wrong checkpoint finalised while no monitor ran | the watcher raises `finalized-wrong` with the deadline; the freeze authority's monitor revokes checkpoint 2, and `finalState` returns to its parent |
| S8 | cancelled state submitted again | cancelled again, `persistent-candidate`, a 12 h pause; a submission during the pause is refused |
| A1–A3 | rule (A) vault (turnSeconds 900, Δ 3 h 01 min) | honest candidate passed and finalised with a beacon after W = 12,660 s; stale key set cancelled at once; tip not final cancelled 7,202 s after the submission (Δ_g = 7,200 s), 5,458 s before the deadline |
| A4 | final tier stalled (rule A) | `final-tier-stalled` and the 12 h pause |
| R1–R4 | the lab's real record as the node view | final tier 258; real tip passes, other hash cancelled, tip 259 waits |
| S9 | failure paths | a `--dry-run` monitor raises `would-cancel` and leaves the candidate; a watcher without gas money raises `cancel-failed` and the candidate stays; funded again, the next poll cancels it 3,599 s before the deadline |
| S10 | clean shutdown | SIGTERM: exit 0, a `stopped` alert, the state file saved; the next monitor takes the lock at once |
| S11 | two node views in one monitor, the second forked at the candidate's tip | cancelled on the forked node's contradiction (`tipConflict` in `lez-forked`): any own node's contradiction cancels |
| | alerting | all 23 critical alerts reached the `--alert-cmd` pager |
| | resources (build box) | the daemon held 34,216 kB resident (VmRSS) at its kill; a run of 2 polls with a full rescan took 31,980 kB max RSS and 1.04 s |

A `cancelCandidate` sent by the monitor used **41,477 gas** (41,468 in S5), from the receipts
(measured, anvil). This includes the 21,000 base and the calldata. The vault's Foundry table gives
37,438 ([`vault/GAS.md`](../vault/GAS.md)). Numbers are from the committed run; between runs the seconds differ by about 1 s and the memory by a few MB. On anvil the chain time moves only by the script's warps,
so "0 s after submission" means the cancellation landed in the first poll after the submission.

## As a Logos module

[`ferrata_monitor_module`](../modules/README.md#ferrata_monitor_module) runs this program in a headless Logos
Core (`logosctl daemon`): `run` (with `--max-polls`) and `start`/`status`/`stop` take `monitor run`'s
arguments and run the same daemon (`src/cli.rs`, `daemon::run_with`, which ends on its stop channel
instead of on signals, since the process's signals are the host's); `check` and `node_view` are
`monitor check` and `monitor node-view`. `MONITOR_CMD=../modules/scripts/monitor-module.sh bash
tests/anvil-e2e.sh` runs the scenarios above with the module in place of the binary
([`modules/RESULTS.md`](../modules/RESULTS.md)).

With `--send-via logos` its transactions go through Logos Core's `tx_sender_module`. When the monitor holds the
watcher key too, a transaction the module did not broadcast, or cannot account for (stuck, failed after its leg began
to leave, a withdrawal not confirmed, a request it no longer holds), goes the plain way at the node's next nonce:
when that send goes through, it fills a number the module burnt without broadcasting, which its own later sends would
otherwise wait behind; against a transaction of the module's that did leave it costs at most one extra call (a second
`cancelCandidate` or `revokeFrom` reverts; a second `pauseCheckpoints` moves the pause's end by the seconds between
the two). If the plain send fails once right after the module burnt a nonce (an error from the monitor's own endpoint,
or a watcher account out of gas money at that moment), the monitor does not recover: the module's later sends come
back broadcast, so the fallback does not run again, and they wait behind that number; no cancel is mined. Not handled
(the monitor would have to keep the burnt nonce in its state and send the plain way until the account's nonce is past
it). Without the key it is an error, retried at the next poll. `bash tests/logos-fallback.sh` runs this against a stand-in for
`tx_sender_module` on anvil ([`results/logos-fallback/`](results/logos-fallback/README.md): every cancel mined, at the
burnt number when the module failed and the plain send went through; with the previous code none was).

## Not built

- A client for the LEZ sequencer's RPC and the Logos Blockchain node's channel answer, to read the
  key sets live; today they come through the node-view adapter.
  - What it would read: `getLastBlockId`, then `getBlock(id)`. In LEZ v0.4.0-rc1 the block arrives as
    base64 of its borsh encoding (`lez/common/src/block.rs`, `borsh_base64`): the header first, with
    `block_id` (u64) at bytes 0..8 and the block hash at 40..72, as the checkpoint program's header
    parser reads them; `bedrock_status` (0 Pending, 1 Safe, 2 Finalized) is the last byte.
  - It is not built because no recorded `getBlock` response exists to test a decoder against. The lab
    driver's `chain.tsv` is the tested path.
- The computation of `states` from the node's own blocks (the checkpoint program's state at a tip).
- Fee bumping of a stuck transaction (today: the receipt timeout and an alert).
- The attestation side's `monitor` duties (comparing records with several providers; the
  weak-subjectivity alarm, `docs/design/attestation.md` §3.6): another program, [`attest/`](attest/README.md).
- A run against a public network (the fork reads Sepolia, but the monitor's transactions stay on the fork).
