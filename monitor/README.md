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

- **Key.** The watcher key is given through `--key-env VAR` (hex in the environment) or through
  `--keystore FILE --password-file FILE`, never on the command line. A monitor without a key, or with
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

All runs were on the Modal build box (`mrun --profile yasmeen`), never on the laptop.

- `cargo test --release`: 14 tests in [`tests/check.rs`](tests/check.rs). They cover the rule (B)
  and (A) verdicts, the margin before the deadline, key-set rotation, the state fields, several
  nodes, the audit of finalised checkpoints, the attestation audit with real Ed25519 certificates
  and final-tier messages (good, conflicting, stale key set, dead channel key, another deployment's
  certificate, malformed), the node-view JSON, the state file, and the node view built from the lab's
  real record (258 `Finalized` blocks of a LEZ v0.4.0-rc1 sequencer; the same record with blocks 257
  and 258 made `Safe` and `Pending` ends the final tier at 256; a gap is refused).
- [`tests/anvil-e2e.sh`](tests/anvil-e2e.sh): a local anvil chain (chain id 31337, no fork, nothing
  sent to any public network). It deploys a v5 vault through
  [`vault/script/MonitorFixture.s.sol`](../vault/script/MonitorFixture.s.sol), with an accept-all
  verifier (the monitor never reads proofs) and the monitor's fresh key as the watcher. All 38 checks
  pass ([`results/anvil-e2e/summary.md`](results/anvil-e2e/summary.md)).

The scenarios:

| | Scenario | Outcome |
|---|---|---|
| S1 | honest candidate (rule B) | passed, untouched for 3 polls, finalised after the wait |
| S2 | tip conflicting with the final tier | cancelled in the first poll; `finalizeAttested` refused after the wait |
| S3 | stale key set (n = 3, t = 2 against four keys) | cancelled (`keySetNotLive`) |
| S4 | honest and wrong pending together | only the wrong one cancelled; the honest one finalised |
| S5 | monitor SIGKILLed while waiting on a tip its node lacks | a second monitor on the same state file is refused; after the restart from the state file, cancelled 601 s after the submission, 2,999 s before the deadline |
| S6 | state file lost | a fresh monitor rescans from the deployment block and makes exactly one cancellation (the pending wrong one); the two finalised checkpoints are audited as good |
| S7 | wrong checkpoint finalised while no monitor ran | the watcher raises `finalized-wrong` with the deadline; the freeze authority's monitor revokes checkpoint 2, and `finalState` returns to its parent |
| S8 | cancelled state submitted again | cancelled again, `persistent-candidate`, a 12 h pause; a submission during the pause is refused |
| A1–A3 | rule (A) vault (turnSeconds 900, Δ 3 h 01 min) | honest candidate passed and finalised with a beacon after W = 12,660 s; stale key set cancelled at once; tip not final cancelled 7,202 s after the submission (Δ_g = 7,200 s), 5,458 s before the deadline |
| A4 | final tier stalled (rule A) | `final-tier-stalled` and the 12 h pause |
| R1–R4 | the lab's real record as the node view | final tier 258; real tip passes, other hash cancelled, tip 259 waits |
| | alerting | every critical alert reached the `--alert-cmd` pager |

A `cancelCandidate` sent by the monitor used **41,477 gas** (41,468 in S5), from the receipts
(measured, anvil). This includes the 21,000 base and the calldata. The vault's Foundry table gives
37,438 ([`vault/GAS.md`](../vault/GAS.md)). On anvil the chain time moves only by the script's warps,
so "0 s after submission" means the cancellation landed in the first poll after the submission.

## Not built

- A client for the LEZ sequencer's RPC and the Logos Blockchain node's channel answer, to read the
  key sets live; today they come through the node-view adapter.
- The computation of `states` from the node's own blocks (the checkpoint program's state at a tip).
- The Logos module packaging (`logosctl daemon`).
- Fee bumping of a stuck transaction (today: the receipt timeout and an alert).
- The attestation side's `monitor` duties (comparing records with several providers; the
  weak-subjectivity alarm, `docs/design/attestation.md` §3.6).
- A run against a public network.
