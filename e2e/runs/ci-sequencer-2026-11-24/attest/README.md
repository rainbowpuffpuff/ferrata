# CI: attestation against a LEZ sequencer in standalone mode, on fresh Sepolia data

RFP-021 Supportability #2 (job `sequencer-attest` of `.github/workflows/sequencer.yml`, script
`scripts/ci/sequencer-attest.sh`): LEZ v0.4.0-rc3's `sequencer_service` built with its `standalone` feature (its mock
Bedrock actor; no Logos Blockchain node), blocks every 3 s, `RISC0_DEV_MODE` unset; the Ethereum side is
Sepolia, read from public endpoints when the run started.

| | |
|---|---|
| checkpoint | Sepolia slot 11640832 (2026-11-23T08:46:24Z, period 1421, Gloas layout), block root `0x49cd05dd2796f3e0539e53797dfd2a9f76c4887b5124eb9bbbf2b7fe8b3efaec`, the second provider's; Init valid until 2026-12-09T01:37:36Z |
| period updates | 1421 (finalized slot 11640832; advanced False, adopted True) |
| finality update | finalizes slot 11646336 (2026-11-24T03:07:12Z), root `0x18ee938f06b720bc37a2e00517063f8a1ddc2efbdeb032ff85a481acca82f4fe` (the second provider's), signature slot 11646401, 486/512 signers |
| its execution block | 12199909, hash `0xc19a5b4a93b314918604f2a3c5bcfe8fd21b924b697a650c1db427a62e21faa0` |
| storage fact | `0x1c7d4b196cb0c7b01d743fbc6116a902379c7238` slot `0x000000000000000000000000000000000000000000000000000000000000000b` = `0x000000000000000000000000000000000000000000000000951404989cd91159` (Sepolia USDC `totalSupply_`, equal to `totalSupply()` at the block) |
| log fact | ERC-20 Transfer, tx 0 log 0: token `0xd4778aa86d4ef3513a5a1c4d8d6e121f3ba58236`, from `0x81a1ed90703d0fbd8fa097d4a42420363f936cfc` to `0x9749ced506ad8431f793628615b0a1c351e9d718`, amount 2590814 |

| phase | sent | applied | refused (exit code) | reverted | other |
|---|---:|---:|---:|---:|---:|
| deploy | 20 | 20 | 0 | 0 | 0 |
| fresh-follow | 11 | 11 | 0 | 0 | 0 |
| fresh-scenarios | 41 | 23 | 16 | 2 | 0 |
| fresh-consume | 17 | 12 | 5 | 0 | 0 |
| fresh-private | 1 | 1 | 0 | 0 | 0 |

- phases: `info deploy fresh-follow fresh-scenarios fresh-consume fresh-private export`
- outcome: every phase ran to its end (the check step asserts each outcome)
- records: `records/` (run.tsv the manifest, summary.txt, chain.tsv, blocks.borsh, programs.tsv, instances.tsv,
  environment.md, logs/, private/), `fresh/` (the fetched data, each directory pinned by its SHA256SUMS; check.json,
  fetch.log), `sequencer-attest.log`

## This record

Made by `bash scripts/ci/sequencer-attest.sh e2e/runs/ci-sequencer-2026-11-24/attest` (the six stages in one call,
as the CI job runs them one per step) on the repository's Modal build box (Linux x86-64), after
`scripts/fetch-upstream.sh`, with the scripts of commit `dd8c878d` and the driver of `5b0630c7` (branch
`ci-sequencer`), on 24 November 2026, 03:08:23–03:51:38 UTC. The box reports 2 CPUs, so the script used 4 build
jobs; the run set `FERRATA_LIVE_PROOF_THREADS=4` (since `b10ea7b0` the script's default on a 4-core machine), as a
4-core GitHub runner would. Nothing was sent to any network: the sequencer listened on 127.0.0.1; the public
endpoints were read only (`fresh/*/SOURCE.md` lists every request).

Measured on the box (`sequencer-attest.log`, `records/logs/driver.log`, mrun's ledger):

| stage or phase | time |
|---|---|
| checkpoint (one attempt) | 2 s |
| build: the guests (5 min), LEZ v0.4.0-rc3's standalone sequencer (10 min), the driver (2 min) | 17 min |
| data (one attempt: its finality update advanced the store) | 2 s |
| run: deploy 60 s, fresh-follow 34 s, fresh-scenarios 122 s, fresh-consume 51 s, fresh-private 1,283 s (the real proof 1,222.1 s, 4 threads), export 4 s | 26 min |
| the whole call (mrun) | 2,638 s, 2.79 core-hours, about 0.27 USD |

The guests built are the images of `docs/image-ids.md`: `ferrata_lez` `0x816d1fef…` (sha256 `ddfde69b…`), `scripted`
`0x8cb0e3f5…` (`29249a3a…`), `ferrata_consumer` `0x8464603b…` (`d02e4aa3…`), as the sequencer's loader recorded
them (`records/programs.tsv`). The chain: 519 blocks; 90 transactions: 67 applied, 21 refused with the program's
exit code, 2 reverted otherwise (the wallet's default gas limit on an adoption part; an `Open` before its signature
slot on the instance whose clock runs ten years later), none dropped, rejected or unfinished (`records/summary.txt`).
