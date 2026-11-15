# crown-rehearsal-4: the final programs on an anvil fork of Sepolia, 18 of 19 stages, then stopped (12 Nov 2026)

`e2e/crown.sh start --fork crown-rehearsal-4` on our Modal workspace (app `ferrata-crown`, `runner_rehearsal`),
branch `integration7` at commit 0409be9: the final programs (path-independent builds, `docs/image-ids.md`): attestation
`0x816d1fef…03df`, bridge `0xccde6171…71dd`, `checkpoint_b` `0xf8116722…626e`, release `0xb64900bc…29e9`; LEZ bundle
sha256 `4dd813e2…`, 139,304,302 bytes. Started 23:11:17Z (11 Nov). Every number below is from this run's files.

**STAND-IN, labelled** as in crown-rehearsal-3: anvil fork of Sepolia block 12,114,906, the deployer impersonated,
nothing sent to a public network; the stand-in beacon committee on LEZ; W and the release delay warped.

| Stage | Result | Time |
|---|---|---|
| lez-a | new devnet, both programs, stand-in instance; T0 = block 19 | 215 s |
| identity | deployer nonce 23 (the fork's), vault CREATE(n+2) = `0x04eE…de44c`, configId `0xbba1…c51e` | 1 s |
| deploy, deposits, captures | vault deployed (mode B); deposit 0 public, deposit 1 private; fork captures, 6 slots proven each | 0-4 s each |
| lez-b | blocks 20..86: 2 claims (one private, proven by LEZ's wallet), 2 burns; T1 = 86 | 735 s |
| checkpoint-input, exec | crown-19-86: 594,476 B; journal = native | 1 s |
| checkpoint-prove | an L40S: 30.9 s prover wall, 20 segments, 18,465,623 user cycles, ids and inputs equal the manifest | 42 s |
| checkpoint-wrap, submit | Groth16 seal `0x73c457ba…`, journal 544 B; `submitCandidate` accepted | 57 s |
| releases-prove | both releases proven and wrapped | 151 s |
| wait, finalize, release0, release1 | W warped; finalised; both releases paid | 2 s |
| verify | **11 of 14**: the three seal checks failed (`verify.txt`) | — |

Why verify failed: `e2e/crown/verify.py` pinned its own ids of the vault-side programs, still crown-live-1's, and
checked the final programs' seals against them (RISC Zero's verifier rejected them in `eth_call`; the local Groth16
check found another claim digest). The run's own stages had verified the same receipts against the final ids, and the
vault on the fork accepted them. Fixed in ed97db5 (ids pinned per run). The resumed run (call `fc-01M76FMD…`) stopped
at once: a resumed fork re-forks Sepolia at the run's first block, and the public endpoint now refuses that state as an
archive request (HTTP 403, `logs/anvil-1.log` of the full fetch; drive.py's `CROWN_FORK_ARCHIVE` would name another
endpoint). The rehearsal therefore ran again from the start as `crown-rehearsal-5`
([`../crown-rehearsal-5/`](../crown-rehearsal-5/README.md)).

Files: the records crown-rehearsal-3 kept (identity, deployment, parameters, captures' summaries in `crown.json`,
the checkpoint's input summary, journal and seal, the release records and seals, the LEZ sessions' summaries,
`stages.done`, `verify.txt`); the full fetch (logs, receipts, blocks) stays outside the repository.
