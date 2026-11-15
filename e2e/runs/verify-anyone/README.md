# verify-anyone: the deployed programs rebuilt, the live run checked from public data (11 Nov 2026)

## The six guests rebuilt at the crown build path (`reproduce-box/`)

`scripts/reproduce-image-ids.sh build` on the build box (Modal CPU container, workspace deeferentleeg, through
`mrun`; no Docker there, so its steps ran directly in the box's image, which the Dockerfile mirrors), branch
`verify-anyone` from `integration6` c52eee4, the tree copied to the crown build path
`spike/ferrata-wt-crown` inside the box only. A cold box (no cargo registry, no target directory): copy 13:09:25Z,
the three `methods` builds done 13:16:52Z (7 min 27 s), `mrun` 475 s in all, 1.45 core-hours, about 0.09 USD
(mrun's estimate).

| Program | Rebuilt image id | = README table | = vault immutable (eth_call) | = risc0-build's own id |
|---|---|---|---|---|
| attestation `ferrata_lez.bin` | `0x2228bb462b83032ade1c5de62d381b8dcffe9d10d9e977dce9e9fc8da31467cf` | yes | not on the vault (LEZ) | |
| bridge `ferrata_bridge.bin` | `0x0e9dcd79482f9e0b83762b965d7e1d4319ffeba8a4bdfbdff148bbeaba79bbb6` | yes | not on the vault (LEZ; in configId) | |
| `checkpoint_b` | `0xa475e8f9aaa41d0ae3bf9a20ae7d937ebc637a961808765f0ff175158760d3b2` | yes | `checkpointImageId` | yes |
| `checkpoint` (rule A) | `0xcf38b945a70b66d9132fb493b4baf1db308de73ccb8b1108e1c1f8a4363525e6` | yes | not on a mode-(B) vault | yes |
| `segment` | `0xd2b30667292418ccf3cf68ee4fa2dc9ed7873de4034fcf1fb2ed3a6305a61cca` | yes | not on a mode-(B) vault | yes |
| release | `0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd` | yes | `releaseImageId` | yes |

6 of 6 equal (`compare.txt`, `scripts/reproduce/compare_ids.py`, vault `0xfE3199fa2634413CbAC317371076Cae3EB2d9E54` read
on Sepolia). Stronger than equal ids: the six user ELFs and the four R0BF programs are **byte-identical** to the
crown build's (`user-elfs.sha256`, `r0bf.sha256` equal the sha256 the crown build recorded for the programs the live
run uses). Files: `ids.txt` (r0vm's ids of the LEZ programs), `r0bf/ids.jsonl` (`ferrata-e2e program-binary`),
`methods-ids.txt` (the `[u32; 8]` ids risc0-build wrote for the methods crates), `steps.txt` (times, toolchains).

## The live run crown-live-1 checked against Sepolia (`verify-sepolia-*.txt`)

`python3 -I scripts/verify-sepolia.py <records>` on the laptop (light: JSON-RPC reads to the public endpoint
`ethereum-sepolia-rpc.publicnode.com`), the records fetched read-only from the run's Volume while it waited W:

- `verify-sepolia-crown-live-1-wait.txt` (14:13 CET): **10 passed, 2 pending, 0 failed**, 48 RPC calls; pending: the
  finalization (opens 13:27:48Z) and the two releases (their seals already verify for the vault's `releaseImageId`).
- `verify-sepolia-crown-live-1-after-finalize.txt` (14:40 CET, records fetched again at 13:34Z; with the captures'
  check added): **12 passed, 1 pending, 0 failed**, 54 RPC calls: both captures' `header.rlp` are Sepolia's final
  blocks 12,111,509 and 12,111,602 (keccak = the chain's block hash) and all 6 slots of each (deposit root, leaf count
  1 then 2, configId, the ETH registry entry, fee schedules 0 and 1) verify by Merkle proof against their state roots,
  with the vault's code hash; 5 recorded transactions (deposits, submit, `finalizeAttested` at block 12,112,027,
  release 0 at 12,112,040), the checkpoint final (`finalState` = the journal's new state, `checkpointCount` 1, the burn
  root accepted), release `public-0` paid (0.001 ETH to the recipient, nullifier spent); `private-1` pending: the
  runner stopped itself at 13:30:49Z with "release 0: the nullifier is not used after the release" although release 0
  had succeeded (its read of `nullifierUsed` right after the transaction, `e2e/crown/cloud/drive.py` `st_release`,
  most likely reached a public-endpoint backend one block behind; the chain shows the nullifier spent). Release 1's seal
  already verifies for the vault's `releaseImageId`; `e2e/crown.sh start` resumes and skips release 0.
- `verify-sepolia-crown-live-1-after-finalize-ethpandaops.txt`: the same records against a second, independent
  endpoint (`rpc.sepolia.ethpandaops.io`): the same **12 passed, 1 pending, 0 failed**. (`sepolia.drpc.org` answered
  HTTP 400 to every request; the script now stops after the chain check with exit status 2 when the endpoint does not
  answer as Sepolia.)
- `verify-sepolia-crown-live-1-tampered-capture.txt`: one byte of a node of capture 1's account proof changed (and
  a slot value of capture 2): the captures' check fails ("proof node 1 does not hash to its parent's reference"),
  exit status 1.
- `verify-sepolia-crown-rehearsal-3-against-sepolia.txt`: the fork rehearsal's committed records (an anvil fork; the
  same deployer, nonces and vault address as the live run) against real Sepolia: **6 failed**, exit 1 (the vault's
  configId and genesisState are the live run's, the rehearsal's transactions and checkpoint are not on chain). Its
  first try also showed the public endpoint answering head 12,112,098 and, a moment later, refusing an `eth_getLogs`
  to 12,112,098 with "head 12,112,097": a load-balanced endpoint can answer from a node one block behind, the likely
  cause of the runner's stop after release 0. The script's last log window now ends at `latest`.
- `verify-sepolia-crown-live-1-tampered.txt`: the same records with one deposit commitment, one byte of the
  checkpoint seal and the genesis state changed: **5 failed**, exit status 1.

## Control: the same source at another path (`control-other-path/`)

The same script, source and box, with `FERRATA_BUILD_PATH` set to `spike/ferrata-wt-integ5` (the path the brief
named; inside the box only): the three builds done in 6 min 49 s (`mrun` 421 s, about 0.11 USD), and **0 of 6** ids
equal the deployed ones (attestation `0xcebccdee…c4dc`, bridge `0xcf41b7e5…5390`, `checkpoint_b` `0xcd336146…4ca8`,
`checkpoint` `0xd82f6d4c…2c36`, `segment` `0x7c71ecb8…0ebf`, release `0x7609f7b2…c82f`; `compare.txt`). Why
(`embedded-paths.txt`, counted with `grep -a -o` in the rebuilt user ELFs): every guest holds its build path 1 to 9
times (`spike/ferrata-wt-crown/…`, the panic locations of the tree's own crates), the cargo home 8 to 12 times
(`~/.cargo/…`, the registry and git checkouts) and the two LEZ programs the upstream LEZ tree twice
(`spike/upstream/…`). The deployed programs were built at `spike/ferrata-wt-crown` (the crown build's own record:
"tree copied to the crown path", then the builds; `e2e/crown/README.md`: "built … at the crown build path
`spike/ferrata-wt-crown`"), so that is the path the recipe fixes.
