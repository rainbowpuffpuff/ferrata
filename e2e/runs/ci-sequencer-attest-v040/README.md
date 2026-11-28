# sequencer-attest on LEZ v0.4.0, rehearsed on a build box: every phase as designed up to the private attestation, whose fee credit the standalone sequencer dropped

**Run:** `FERRATA_CI_LEZ_TAG=v0.4.0 scripts/ci/sequencer-attest.sh e2e/runs/ci-sequencer-attest-v040` (stages checkpoint,
build, data, run) on a Modal build box (2 CPUs), 28 November 2026, 07:18–07:37 UTC. The tree was `lez-040-zone` at
`93c46f76`, which has `lez-040-core` `b24aff0d` merged: the live driver receives a private account's pending credit
before its private sessions. This was not on GitHub Actions: nothing was pushed from here. The summary below was
written afterwards by the script's own `summary` stage, run on a copy of this directory. Nothing was sent to any
network: the sequencer was on 127.0.0.1, and the Sepolia data was read from public endpoints.

**What it shows:**
- The guests were built after `lez/scripts/lez-select.sh v0.4.0`. Their ids, as r0vm 3.1.0 computes them
  ([`guest-ids.tsv`](guest-ids.tsv)), equal `lez/results/v0.4.0/image-ids.md`'s: `ferrata_lez` `0x95a93abf…`,
  `scripted` `0x0a37d9ba…`, `ferrata_consumer` `0x41a36bde…`.
- On LEZ v0.4.0's standalone sequencer (real proofs verified: `RISC0_DEV_MODE` unset), four phases ran as designed:
  deploy, fresh-follow, fresh-scenarios and fresh-consume. That is 89 transactions: 66 applied, 21 refused with the
  program's exit codes, and 2 reverted (the out-of-gas detour at a 2,000,000 limit, and a job opened before its signature
  slot). These are the counts of rc3's recorded run (`../ci-sequencer-2026-11-24/attest/`) without its private
  attestation.
- Instance A followed Sepolia from the checkpoint and holds the finalized slot 11676352, with the second provider's root.

The script's `check` stage, run on a copy with the clock at the run's end (`FERRATA_CI_NOW`), confirms every row it
names except the private attestation, and counts the one dropped transaction below.

**Where it stopped:** `fresh-private`. The live driver first received the private account's pending credit of
20,000,000,000,000 into its note: `executed, proven and sent in 219.3 s` (a real proof by LEZ's wallet). The sequencer did
not include it: `dropped … not included within 4 blocks` (transaction `a9fb4bb9…`). So `AttestPrivately` was not sent.
The same receiving step was included in a zone of four (`../zone-bridge-v040/`, block 66, crown driver), so the difference
lies with the standalone sequencer or its `local.json` private genesis. The sequencer's log here says nothing about the
drop. This is reported to the core port (`lez-040-core`), which owns the driver.

**Earlier rehearsals (replaced by this record):** `a37f3c74` stopped in `fresh-scenarios`. On v0.4.0,
`wallet::DEFAULT_GAS_LIMIT` is the protocol's cap, so the out-of-gas detour applied. `73c8fa30` passed the four phases and
stopped in `fresh-private`, whose dry run refused a fee payer with an empty note.

## The script's summary

RFP-021 Supportability #2 (job `sequencer-attest` of `.github/workflows/sequencer.yml`, script
`scripts/ci/sequencer-attest.sh`): LEZ v0.4.0's `sequencer_service` built with its `standalone` feature (its mock
Bedrock actor; no Logos Blockchain node), blocks every 3 s, `RISC0_DEV_MODE` unset; the Ethereum side is
Sepolia, read from public endpoints when the run started.

| | |
|---|---|
| checkpoint | Sepolia slot 11673600 (2026-11-27T22:00:00Z, period 1425, Gloas layout), block root `0x065d112a62a2b7d5249762cdbce437c2155425a8dc232c9b9b7c289af1ac5bb1`, the second provider's; Init valid until 2026-12-13T14:51:12Z |
| freshness | checked at 2026-11-28T07:28:15Z: the finalized block 1071 s old, the bootstrap 34095 s old (bounds 7200 and 115200 s) |
| period updates | 1425 (finalized slot 11673600; advanced False, adopted True) |
| finality update | finalizes slot 11676352 (2026-11-28T07:10:24Z), root `0xb4de784c95280f3c85f7c05839b1edbf3d4d77320859f32d49e0c0b7f60f36a5` (the second provider's), signature slot 11676417, 474/512 signers |
| its execution block | 12228553, hash `0xd1c0498bc8570f9e0849c6bacc46b1caba22e3b52ab9cec717f4ea2be847d9f8` |
| storage fact | `0x1c7d4b196cb0c7b01d743fbc6116a902379c7238` slot `0x000000000000000000000000000000000000000000000000000000000000000b` = `0x0000000000000000000000000000000000000000000000009512c99f57706d09` (Sepolia USDC `totalSupply_`, equal to `totalSupply()` at the block) |
| log fact | ERC-20 Transfer, tx 4 log 0: token `0xf6081c62c44e85a025101c99fb9f74afc36b23ca`, from `0x3ec84659862c59619e82f16b787b798bd423d038` to `0x8b2a1415b29c72bc22c4d74a8b899badef451d18`, amount 77433485091956308 |

| phase | sent | applied | refused (exit code) | reverted | other |
|---|---:|---:|---:|---:|---:|
| deploy | 20 | 20 | 0 | 0 | 0 |
| fresh-follow | 11 | 11 | 0 | 0 | 0 |
| fresh-scenarios | 41 | 23 | 16 | 2 | 0 |
| fresh-consume | 17 | 12 | 5 | 0 | 0 |
| fresh-private | 1 | 0 | 0 | 0 | 1 |

- phases: `info deploy fresh-follow fresh-scenarios fresh-consume fresh-private export`
- outcome: **== phase fresh-private FAILED (exit 1)**
- records: `records/` (run.tsv the manifest, summary.txt, chain.tsv, blocks.borsh, programs.tsv, instances.tsv,
  environment.md, logs/, private/), `fresh/` (the fetched data, each directory pinned by its SHA256SUMS; check.json,
  fetch.log), `sequencer-attest.log`

