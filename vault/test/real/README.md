# The real-proof suites under RISC Zero 3.1

Since the move to RISC Zero 3.1 ([migration plan](../../../docs/design/risc0-3.1-migration.md), phase 1; RISC Zero's
advisories of 11 November 2026, [security note](../../../docs/security/risc0-groth16-advisory-2026-11-21/README.md)),
the vault is tested against RISC Zero's `RiscZeroGroth16Verifier` **3.1.0** (risc0-ethereum v3.1.0, `vault/lib.lock`;
selector `0xe71e6620`), which refuses every seal of risc0-zkvm 3.0.5 (`SelectorMismatch(0x73c457ba, 0xe71e6620)`,
`test/Verifier31.t.sol`). So every committed seal was made again.

## Proven again under RISC Zero 3.1

`scripts/reprove_fixtures.py` with the jobs of [`reprove-31.json`](reprove-31.json): for each seal, the recorded input
of its run, the guest of this tree built with risc0-zkvm 3.1.0 ([`docs/image-ids.md`](../../../docs/image-ids.md),
`*_31` rows), a succinct proof on the CPU through `r0vm` 3.1.0 (`ferrata-e2e prove`), `identity_p254`
(`ferrata-e2e p254`), RISC Zero's Groth16 prover image `v2026-10-20.1` (risc0-groth16 3.0.6) as a Modal function
(`e2e/crown/groth16_app.py`, `e2e/crown/wrap.py`), the Groth16 receipt verified against the program's id and the
journal the run recorded (`ferrata-e2e groth16`, `verify`), then the seal and the id written into the fixture. The
guests' sources did not change, so neither did the journals: every new receipt is held to the journal its run recorded,
and the fixtures' other fields are the runs' own.

| Fixture | Run and inputs | Seals |
|---|---|---|
| `zone-bridge.json` | `e2e/runs/zone-bridge-1` (LEZ v0.4.0-rc1 zone, four sequencers) | checkpoint (`checkpoint_b_31`), two releases (`release_31`) |
| `zone-bridge-rotation.json` | `e2e/runs/zone-bridge-2` (the same, a key rotation) | the same |
| `zone-bridge-3.json`, `zone-bridge-4.json` | `e2e/runs/zone-bridge-3`, `-4` | the same |
| `zone-bridge-rc3.json`, `zone-bridge-rc3-2.json` | `e2e/runs/zone-bridge-rc3`, `-rc3-2` (LEZ v0.4.0-rc3, Logos Blockchain 0.3.5-rc.1) | checkpoint (`checkpoint_b_0_3_5_31`), two releases |
| `rule-b-multi.json` | `checkpoint/attestor/runs/devnet-n3/rbm-lab-n3-t2` | checkpoint (`checkpoint_b_31`) |
| `rule-b-zone.json` | `checkpoint/attestor/runs/zone/rbz-zone-n4-t3` | checkpoint (`checkpoint_b_31`) |
| `crown-live-3-releases.json` (new) | `e2e/runs/crown-live-3`'s two releases: inputs rebuilt by `ferrata-e2e crown-release` from the run's `burns.json` and `burn-notes.json` ([`inputs/crown-live-3/`](inputs/crown-live-3/); both metas byte for byte the run's) | two releases, read by `test/standin/` and `test/Verifier31.t.sol` |

Which inputs reproduce their recorded journal under this tree's guests was checked first with the executor
(`reprove_fixtures.py survey`, r0vm 3.1.0): the ones above do.

## Kept as records of RISC Zero 3.0.5, skipped

These suites' fixtures are of earlier programs, or their inputs are not in the repository, so they cannot be proven
again with this tree's guests; their setUp skips them with the reason (`vm.skip`), and their fixtures stay as the
records of what ran under 3.0.5:

| Suite | Fixture | Why not proven again | Its cases under 3.1 |
|---|---|---|---|
| `CrownR1Test`, `CrownR4Test` | `crown-r1.json`, `crown-r4.json` | the crown programs of 10-11 November; inputs not recorded | rule (B) checkpoint and releases paid, replays refused: `ZoneBridge*Test` |
| `RealBlocksTest`, `RealBlocksWrongVaultTest`, `RealBlocksGas` | `real-blocks.json` | an earlier `checkpoint_b`; this tree's guest refuses the recorded input's layout (`BadLength`) | `RuleBMultiTest`, `RuleBMultiWrongVaultTest`, `RuleBMultiGas` |
| `ReturnLegTest`, `ReturnLegGas` | `return-leg.json` | earlier rule (A) and release programs; their inputs give other journals | rule (A) has no real-proof suite under 3.1 (below) |
| `RuleBRealTest`, `RuleBRealOnRuleAVaultTest`, `RuleBRealGas` | `rule-b.json` | input not recorded | `RuleBMultiTest`; the rule (A) refusal is `RuleBMultiOnRuleAVaultTest` |
| `RuleBRotationTest` | `rule-b-rotation.json` | input not recorded | `ZoneBridgeRotationTest` (a rotation, proven again) |

Not covered under 3.1: a real rule (A) proof (the return leg). A fresh one needs a run of `e2e/run.sh` on this tree's
programs (the LEZ side natively, `checkpoint/fidelity`), then its two checkpoints and two releases proven as above.
