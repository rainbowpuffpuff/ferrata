# Release 0.1.0-rc.1: how it was built and tested

Built on the Modal build box (`spike/modal-build/mrun`, workspace ws-14) on 11 November 2026, 19:01 UTC, from commit
`fa1d453cb831c9303abedc27f04082d1974b4966` with `just release 0.1.0-rc.1 <out>` (just 1.36.0): the six guests at the
crown build path (`scripts/reproduce-image-ids.sh build`), the vault (forge 1.3.6, solc 0.8.26, sources only), the CLI
(Rust 1.98.1). Tarball `ferrata-0.1.0-rc.1-x86_64-linux.tar.gz`, sha256
`617c0117400aa1422c5d6c120b17d5ff7ce70b87253316d32137dca0a9a5fcd7` (not committed; on the box and
beside the repository).

| Program | Image id | Equal to the crown run's (e2e/crown/README.md) |
|---|---|---|
| attestation program `ferrata_lez.bin` | `0x2228bb462b83032ade1c5de62d381b8dcffe9d10d9e977dce9e9fc8da31467cf` | yes |
| bridge program `ferrata_bridge.bin` | `0x0e9dcd79482f9e0b83762b965d7e1d4319ffeba8a4bdfbdff148bbeaba79bbb6` | yes |
| `checkpoint` (rule (A)) | `0xcf38b945a70b66d9132fb493b4baf1db308de73ccb8b1108e1c1f8a4363525e6` | yes |
| `checkpoint_b` (rule (B)) | `0xa475e8f9aaa41d0ae3bf9a20ae7d937ebc637a961808765f0ff175158760d3b2` | yes |
| `segment` | `0xd2b30667292418ccf3cf68ee4fa2dc9ed7873de4034fcf1fb2ed3a6305a61cca` | yes |
| release | `0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd` | yes |

FerrataVault runtime 24,254 bytes, FerrataRegistry 12,504 bytes (both under EIP-170's 24,576; MANIFEST.json).

`scripts/release.sh verify`: 10 files of the manifest, 0 bad; every line of SHA256SUMS; the two LEZ programs' image ids
recomputed with r0vm 3.0.5: equal. The tarball fetched to the laptop: its .sha256 checks, and `release.sh verify` of
the extracted directory there gives the same (10 files, 0 bad; image ids equal).

`scripts/test-deploy-local.sh` (anvil, chain id 31337, on the build box; nothing left the box), run on this cut and on
the earlier cut of the same commit's sources (identical output):

```
PASS 1. plan only: exit 0, block number still 0
PASS 2. --send on anvil: exit 0, 2 of 2 image ids read back equal (vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9)
PASS 3. tampered release refused: exit 1, programs/release.bin: FAILED
PASS 4. wrong chain id refused: exit 1,   chain id 31337 (expected 11155111)
PASS 5. chain id 1 refused: exit 1, deploy: chain id 1 (Ethereum mainnet) is refused: the code is a prototype, unaudited
0 failed
```

Check 2 deployed the library, the Groth16 verifier behind RISC Zero's emergency stop and FerrataVault (mode B) with
`vault/script/deploy.sh`, after confirming that forge's build of the tree is byte-identical to the release's bytecode,
and read `checkpointImageId` (= checkpoint_b) and `releaseImageId` back from the chain. The LEZ side ran in plan mode
only (the wallet commands printed, nothing sent): no LEZ sequencer or wallet binary was run in this test.
