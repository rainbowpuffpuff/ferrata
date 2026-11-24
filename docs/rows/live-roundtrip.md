# The first full round trip on the test network: proposals for the RFP-022 rows (for the integrator)

Branch `testnet-r13` (from `98375f82`). The evidence is
[`e2e/runs/live-roundtrip-2026-11-24/`](../../e2e/runs/live-roundtrip-2026-11-24/README.md): a 0.001 ETH deposit on
Sepolia, its root imported on the LEZ v0.4.0-rc3 network we run, claimed and burned there, the zone's first rule (B)
checkpoint proven on a GPU, submitted and finalized, and the burn released, 23-24 November 2026. Every fact is public
chain data, re-read by `python3 -I scripts/verify-live-roundtrip.py --check` (the record rebuilt from the chains and
compared field by field, the links checked on the chains' answers: 24 checks, all PASS on 24 Nov 2026) and, offline, by
`scripts/test_verify_live_roundtrip.py` (proposed for CI's `links` job, in this branch). This stream edits
neither `docs/EVIDENCE.md` nor `docs/requirements-status.md` nor `docs/traceability.md`; the integrator applies the
texts below. **No status changes:** each row below stays where it is, and the reason is given; only the texts and
evidence lists grow.

## The new EVIDENCE row (the next free number; 250 at `98375f82`)

| N | **The first full round trip on our public LEZ v0.4.0-rc3 test network (RFP-022 R.10, S.1; ours, not LEZ testnet 0.4)**: a 0.001 ETH deposit into the Sepolia vault `0x19499061…2787` (`0x44709f7c…e9e1`), its root imported on the zone under the attested record of Sepolia slot 11,643,072 (LEZ block 2,278), claimed by a plaintext claim (block 3,746) and burned (block 3,749); the zone's first rule (B) checkpoint, LEZ blocks 1,226–3,767, proven on a GPU (1,307.0 s from the prover client's pickup to the proof's return, of which the prover 87.1 s), submitted (`0xcaa00efa…a0d1`) and finalized (`0x643d0c68…90f8`) by the host's relayer; the burn released (`0x48b78ad4…6f65`; `release_31` proven on a GPU in 425.6 s, of which the prover 9.9 s): the recipient +0.000997 ETH, the relay fee 0.000002 ETH and the checkpoint fee 0.000001 ETH to the relayer, the latter claimed (`0xe2c264e6…1239`). The legs are linked by chain data alone: the claim's own opening gives the deposit's commitment, its path the vault's root, which the import wrote; the burn's leaf is the checkpoint's only one; both proofs' image ids, seals and journals are the ones the vault verified; the claim's and the burn's LEZ blocks are on the hash chain to the checkpoint's tip. Not a latency measurement (the finalisation waited 10.5 h on a deployment fault of the relayer, since fixed); not the private path (plaintext claim, public burn). | `e2e/runs/live-roundtrip-2026-11-24/`; `scripts/verify-live-roundtrip.py --check`; `scripts/test_verify_live_roundtrip.py` |

Row 248, its bridge sentence, from "submitted by the host's relayer (`0xcaa00efa…a0d1`, block 12,198,162), not
finalized: the relayer stopped after the submission (replayed and fixed in the repository; on the host at the
redeploy)" to:

> submitted by the host's relayer (`0xcaa00efa…a0d1`, block 12,198,162); the relayer stopped after the submission
> (replayed and fixed in the repository), and on the host after the redeploy of 24 Nov it finalized the checkpoint
> (`0x643d0c68…90f8`, block 12,201,456) and released the burn (row N)

## Rows

### R.10 — stays **in part**

The RFP asks for end-to-end round-trip tests against a standalone sequencer and an Ethereum testnet or fork, in CI.
CI still runs none against a sequencer; this run adds a third live round trip, run by the test network's daemons
rather than by hand, with a committed record CI re-derives offline.

- Current: "CI runs the round trip over the state transition and anvil; sequencer round trips, twice live on Sepolia,
  are by hand."
- Proposed: "CI runs the round trip over the state transition and anvil; sequencer round trips, three times live on
  Sepolia, run by hand or by the test network's daemons, not in CI: the third on the network we run, its checkpoint
  proven, submitted and finalized and its release sent by the host's prover client and relayer, its record
  re-derived offline in CI and re-checked from public RPCs by a script."
- Evidence: add N.

### S.1 — stays **waits on Logos**

LEZ testnet 0.4 does not exist; the row's "shown instead" now ends with a complete round trip.

- Current, its last sentence: "…, a deposit and the zone's first checkpoint submitted, public on Sepolia; the round
  trip is not complete: the checkpoint was not finalized (row 248)."
- Proposed: "…, a deposit and the zone's first checkpoint submitted, public on Sepolia; the round trip is complete: the
  deposit was imported, claimed and burned on the zone and released on Sepolia once the checkpoint was finalized, each
  leg public and linked to the next by chain data (row N)."
- Evidence: add N.

### F.5 — stays **shown** (text)

The live burn's message, decoded from its bytes, carries no Ethereum address in the clear (r, memo, mac, token, amount,
COut): the recipient is masked in `memo`, XORed with a pad from the holder's view key (`docs/design/bridge.md` §12.1,
`memo = recipient XOR HMAC(kView, "ferrata/memo/v1" ‖ r)[0..20]`), and hidden and bound in `COut`; the burn was
released.

- Current: "Burns write the note once with no Ethereum address; both live burns released on Sepolia."
- Proposed: "Burns write the note once with no Ethereum address in the clear (on the network we run, the live burn's
  message: r, memo, mac, token, amount and COut; the recipient masked in memo for the holder's view key and hidden in
  COut); every live burn released on Sepolia, three, the third on the network we run."
- Evidence: add N.

### F.3 — stays **shown** (text)

A live claim under a root imported through the attestation, on the network we run; a second import of the same root,
in the same block, reverted. This claim was a plaintext one (it publishes its opening), which the row's last sentence
does not describe; the text below says so.

- Current, its first sentence: "Mints only on a claim under a root imported through the attestation; repeat and
  second-root claims refused (250); live on Sepolia."
- Proposed: "Mints only on a claim under a root imported through the attestation; repeat and second-root claims
  refused (error 250); live on Sepolia, also on the network we run (a plaintext claim, row N)."
- Evidence: add N.

### R.6 — stays **in part** (text)

RFP: off-chain components survive RPC failure without duplicating work. The run shows the relayer across a stall and a
restart sending each step once; it shows no RPC failure the record could prove (the endpoint's lag errors are in the
host's log only), so the row's missing parts stay missing.

- Current: "The relayer daemon recovers from kills and endpoint faults on both chains without duplicating work;
  `monitor` restarted cancels once; its own endpoint failing, the prover and the attestors not shown."
- Proposed: "The relayer daemon recovers from kills and endpoint faults on both chains without duplicating work, and
  on the network we run, stalled by a deployment fault and restarted on the fixed layout, it sent each of its four
  transactions once (nonces 0–3: candidate, finalisation, release, fee); `monitor` restarted cancels once; its own
  endpoint failing, the prover and the attestors not shown."
- Evidence: add N.

### P.6 — stays **in part** (text)

Two GPU proofs on a live deployment, as the prover client timed them: from its pickup of the request to the proof's
return (Modal's start included), and the prover's own run on the GPU (time only: no CPU or memory figure). The importer
as a daemon, `ferrata_attestor` and `monitor`'s LEZ traffic are still not measured.

- Current: "Provers, `monitor` (0.0004 of a core, 14.0 MiB) and the relayer (0.0007 of a core, 20.3 MiB) measured with
  their bandwidth; …"
- Proposed: "Provers, `monitor` (0.0004 of a core, 14.0 MiB) and the relayer (0.0007 of a core, 20.3 MiB) measured with
  their bandwidth, and on the network we run the GPU provers' time (the checkpoint over 2,542 blocks 1,307.0 s from
  pickup to return, of which the prover 87.1 s; the release 425.6 s, of which the prover 9.9 s; the prover client's
  records, committed beside the run); …" (the rest unchanged)
- Evidence: add N.

### P.3 — stays **shown** (text, optional)

- Current: "Deposit, candidate, finalisation and release gas measured with the real verifier on a local EVM and paid
  live on Sepolia."
- Proposed: "Deposit, candidate, finalisation and release gas measured with the real verifier on a local EVM and paid
  live on Sepolia, again on the vault of the network we run (614,222 / 729,373 / 844,349 / 732,686 gas; the fee
  claim 44,728)."
- Evidence: add N.

## Not proposed

- **U.2**: the run used the site's deposit page, the web wallet and the `ferrata` CLI, not the mini-app or Basecamp;
  it shows nothing for the mini-app.
- **P.5**: the outbound leg waited 10.5 h on the relayer's stall, so it measures nothing; the inbound leg (the deposit's
  block to the imported root: 20 min 42 s) is one more sample beside P.5's recorded run, not a replacement for it.
- **F.7, Sec.2–Sec.3, the privacy rows**: one sequencer key; a plaintext claim and a public burn. Nothing here moves
  them.
- **RFP-021**: the import read the attestation program's record of Sepolia slot 11,643,072 on the live network; RFP-021
  S.1 already names the network (row 248), so no RFP-021 row changes.
