# CI: attestation against a LEZ sequencer in standalone mode, on fresh Sepolia data

RFP-021 Supportability #2 (job `sequencer-attest` of `.github/workflows/sequencer.yml`, script
`scripts/ci/sequencer-attest.sh`): LEZ v0.4.0-rc3's `sequencer_service` built with its `standalone` feature (its mock
Bedrock actor; no Logos Blockchain node), blocks every 3 s, `RISC0_DEV_MODE` unset; the Ethereum side is
Sepolia, read from public endpoints when the run started.

| | |
|---|---|
| checkpoint | Sepolia slot 11649024 (2026-11-24T12:04:48Z, period 1422, Gloas layout), block root `0xa5f802a576c15165dbf01b65bfe2b97a2327ea8638dd3a366f880aca1b20d006`, the second provider's; Init valid until 2026-12-10T04:56:00Z |
| freshness | checked at 2026-11-24T14:45:21Z: the finalized block 801 s old, the bootstrap 9633 s old (bounds 7200 and 115200 s) |
| period updates | 1422 (finalized slot 11649024; advanced False, adopted True) |
| finality update | finalizes slot 11649760 (2026-11-24T14:32:00Z), root `0xa31a73bbb64878fcad0fb07c6966c1e503ef4af34f3d142d55f424d10dfbd79e` (the second provider's), signature slot 11649825, 494/512 signers |
| its execution block | 12203185, hash `0x74a8942836cc69ef12c4eaad07abddb2d26d5ae8d4c1fd3dbd1a498ab3e47240` |
| storage fact | `0x1c7d4b196cb0c7b01d743fbc6116a902379c7238` slot `0x000000000000000000000000000000000000000000000000000000000000000b` = `0x000000000000000000000000000000000000000000000000951404984abb7b6c` (Sepolia USDC `totalSupply_`, equal to `totalSupply()` at the block) |
| log fact | ERC-20 Transfer, tx 1 log 0: token `0xfffffffffffffffffffffffffffffffffffffffe`, from `0x37ea8d73e68eeb1ec06fba8d8c369757313e8a5a` to `0x46bc78edd02638cd5c00211c2787c0cebd7a99ea`, amount 9590799996931000 |

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

