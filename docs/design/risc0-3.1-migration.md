# Moving Ferrata to RISC Zero 3.1

Written 21 November 2026, after RISC Zero's advisories GHSA-g6qg-9j8p-7j87 and GHSA-mm6c-35cm-g4x2 (11 November
2026; [security note](../security/risc0-groth16-advisory-2026-11-21/README.md)). Nothing here is built yet: this is
the plan, its order, what waits on Logos and the tests that close it. Every count below comes from this tree at
`5cf1e8e7` (`git grep`, `git ls-files`, a scan of the lockfiles); every RISC Zero fact from its repositories and
crates.io (versions and yank flags), read on 21 November 2026.

## 1. What changes upstream

| | Today (3.0.x) | After (3.1) |
|---|---|---|
| zkVM | risc0-zkvm 3.0.5 (yanked); risc0-build 3.0.5 (not yanked); risc0-circuit-recursion 4.0.4 or 4.0.5 and risc0-groth16 3.0.4 or 3.0.5 (all yanked), locked in 15 and 27 of the 51 lockfiles; three `lez-v03` guest lockfiles already lock 4.1.0 and 3.0.6 beside risc0-zkvm 3.0.5 | risc0-zkvm 3.1.0, risc0-build 3.0.7 (there is no 3.1.0), risc0-circuit-rv32im 4.1.0, risc0-circuit-recursion 4.1.0, risc0-zkp 3.0.6, risc0-groth16 3.0.6 |
| Toolchains (rzup) | cargo-risczero 3.0.5, r0vm 3.0.5 | cargo-risczero 3.1.0, r0vm 3.1.0 |
| Groth16 prover image | `risczero/risc0-groth16-prover:v2025-04-03.1` | `v2026-10-20.1` (risc0 v3.1.0, `risc0/groth16/src/prove/docker.rs`), new trusted setup |
| Ethereum verifier | risc0-ethereum v3.0.0, `RiscZeroGroth16Verifier` 3.0.0, selector `0x73c457ba`, `CONTROL_ROOT` `a54dc85a…1f56` | risc0-ethereum v3.1.0 (commit `ff30da77`), verifier 3.1.0, selector `0xe71e6620` (not the stopped 3.1.0 `0x8b38038c`), `CONTROL_ROOT` `e6feef13…551d`, `BN254_CONTROL_ID` `0ac2eaf9…81de` |
| RISC Zero on Sepolia (`contracts/deployment.toml`, `main` at `13754b01`) | 3.0.0 at `0x2a09…a84D`, estop `0x724d…7797`: stopped | 3.1.0 at `0x8c915b5071206C6E328BC18529586c513408aB3b`, estop `0xA1DCB16cb765b3CA71605339F0C37Bfe0F60bA29`; router `0x925d…9187` (timelock 1 s) |

"The rv32im seal format and circuit identifier changed, so receipts produced by earlier versions do not verify under
3.1.0" (GHSA-g6qg-9j8p-7j87). Two consequences shape everything below:

- **Every proof is made again**: no 3.0.5 receipt or seal survives, on Ethereum or on LEZ.
- **One process, one version.** A process that proves for both sides uses one risc0-zkvm. The vault will accept only
  3.1 seals; a LEZ network accepts the version it was built with (v0.4.0-rc1 and rc3, and v0.3.1 on the public
  testnet: 3.0.5; LEZ `dev`: 3.1.0 since `6aaabe5`, untagged). LEZ pins `risc0-zkvm = "3.0.5"` (caret), our 40
  manifests `"=3.0.5"` to stay on the network's version. The CLI proves releases in process
  (`cli/src/release.rs`, `ferrata_wallet::release::prove`, shared with `ferrata_bridge_module`) and links LEZ's wallet
  crates, so until LEZ moves it cannot make a 3.1 release seal and a rc3-valid private LEZ transaction at once.

So the work splits in two phases: the Ethereum side, which needs nothing from Logos, and the LEZ side, which needs a
LEZ tag on 3.1.

## 2. What must move

### Phase 1: Ethereum side (no Logos dependency)

| Component | Where | Change |
|---|---|---|
| Checkpoint guests, rule (A) and (B), their 0.3.5-rc.1 builds, the segment guest (composed with `env::verify`), plain and micro builds | `checkpoint/guests`, `checkpoint/guests-0-3-5`, `checkpoint/guests-plain`, `checkpoint/methods*`, `checkpoint/host` | manifests to risc0-zkvm 3.1.0 and risc0-build 3.0.7, lockfiles regenerated, rebuilt: **ids move** (`checkpoint`, `checkpoint_b`, `checkpoint_0_3_5`, `checkpoint_b_0_3_5`, `segment`, `micro`) |
| Release guest | `bridge/guests` (`release`), `bridge/methods`, `bridge/host`, `bridge/e2e` | the same: **`release` id moves**. `claim` and `gasdrop` are built in the same workspace but verified by LEZ-linked processes (the relayer): their new ids are recorded now and used from phase 2 |
| Ethereum-side prover outside LEZ-linked processes | `bridge/host` (`prove`, `groth16`), `checkpoint/host`, `gpu-prover/prover`, `e2e/cloud/app.py`, the build box's `mwrap` | 3.1.0, prover image `v2026-10-20.1`; until phase 2 the CLI and the bridge module call such a separate prover for release seals instead of `ferrata_wallet::release::prove` |
| Verifier and vault | `vault/README.md:57`, `.github/workflows/ci.yml:147`, `e2e/ci/run.sh:32`, `cli/tests/e2e.sh:11` (`forge install risc0/risc0-ethereum@v3.0.0`); `vault/script/DeployVault.s.sol` (takes `ControlID` from the library) | risc0-ethereum v3.1.0 and a recorded commit (`ff30da77`); vault source unchanged unless decision D1 (section 4) changes the wiring; new deployments only (the four on Sepolia cannot change their immutable `verifier`) |
| Committed seals the tests read | `vault/test/real/fixtures/*.json` (11), `bridge/evm-verify/test/fixtures/v2/*.json` (3), `checkpoint/realblocks/out-crown-r4/` | proven again from their recorded inputs; `bridge/evm-verify/test/v2/Groth16Verify.t.sol:35,97` and every hard-coded `0x73c457ba` (`vault/test/real/ReturnLeg.t.sol`, `vault/script/verify-sepolia.sh`, `e2e/crown/cloud/drive.py`, `e2e/netfault/run.sh`, `e2e/testnet-return.sh`) to `0xe71e6620` |
| Toolchains | `scripts/ci/install-risc0.sh:22-24`, `scripts/reproduce/Dockerfile:39-43`, CI cache keys (`ci.yml:58,98`), the Modal build box image | cargo-risczero and r0vm 3.1.0 in the main RISC Zero home; the LEZ home stays on LEZ's version until phase 2 |
| Prover image pins | `bridge/scripts/prove-bench.sh:17`, `checkpoint/scripts/prove-bench.sh:15`, `e2e/sepolia-live.sh:345`, `e2e/testnet-return.sh:390`, `e2e/cloud/app.py:134,503`, `bridge/e2e/src/main.rs:26` | `v2026-10-20.1` |

### Phase 2: LEZ side (on the first LEZ tag with risc0 3.1)

| Component | Where | Change |
|---|---|---|
| LEZ programs | `lez/guests/{program, bridge, consumer, holdings, scripted, program-keccak-soft, consumer-keccak-accel, bridge-next, program-class, program-residue-experimental}` and the guests `rfp-softs` adds (`rebate`, `statements`, `statements-reader`), with their `lez/methods*` | rebuilt against the new LEZ tag: ids move if the loader's kernel or the id computation moved (`lez/testnet04/NOTES.md` §6); the binaries are deployed again on every network we run |
| LEZ-linked hosts | `cli`, `lez/harness`, `lez/live` (driver, hooks), `lez/lab`, `lez/crown`, `lez/updater` and `lez/updater/sequencer`, `modules/*/rust-lib` (6 lockfiles), `fuzz`, `bench` | rebased on the LEZ tag; the CLI and the bridge module prove releases in process again |
| Off-chain receipts | `claim`, `gasdrop` (`bridge/guests`; the relayer verifies gas-drop receipts, `cli/src/relayer.rs`) | their phase 1 ids in force |
| Id consumers | `lez/testnet04/day1.sh`, `lez/live/hooks/{crown,gasdrop}.sh`, `e2e/crown/cloud/{app.py,inputs.json}`, `e2e/seqnet/{drive.sh,run.sh}`, `e2e/testnet-return.sh`, `lez-v03-return/verify.py`, `modules/scripts/real_release_proof.sh`, `lez/harness/tests/{residue_handoff,tenant}.rs`, `vault/test/standin/StandInBase.sol`, `vault/test/real/ZoneBridge*.t.sol` and their fixtures | new ids |
| Docs | `docs/image-ids.md` (a new table; the 3.0.5 table kept as history), `components.md`, `design/attestation.md`, `design/bridge.md`, `guides/quickstart.md`, `performance.md`, `privacy/CONTROLS.md`, `traceability.md`, `e2e/crown/README.md`, `lez/testnet04/README.md`, `lez/updater/README.md`, `lez/live/README.md`; `trust.md` §4-5 and the specs' stop wording after D1 | rewritten with the new ids and versions |
| The v0.3.1 port | `lez-v03`, `lez-v03-bridge`, `lez-v03-return` | unchanged while the public LEZ testnet runs v0.3.1; moved only with it |

Not moved: run records (`e2e/runs/`, `*/results/`, `scripts/reproduce/runs/`, `bench/report/`) keep the ids and seals
they ran with; they are records of 3.0.5 runs, and the security note says what they still show. `idl/spel-check`
already resolves risc0 3.1.0 through SPEL.

In numbers: 67 manifests name a risc0 crate (40 pin `risc0-zkvm = "=3.0.5"`, 14 `risc0-build = "=3.0.5"`); 45 of
the 51 lockfiles lock risc0-zkvm 3.0.5; 18 guests in `docs/image-ids.md` plus the crown run's six programs.

## 3. Order

1. **Now, the owner** (no code): set the release and deposit caps of the four Sepolia vaults to zero and retire the
   v5 vaults when keyless retirement opens ([security note](../security/risc0-groth16-advisory-2026-11-21/README.md));
   tell Logos that our LEZ rc1/rc3 private transactions use risc0 3.0.5 and ask for a tag on 3.1.
2. **Merge the open branches first**: `rfp-bridge-hardening` (vault source and the bridge program: its id moves),
   `rfp-softs` (three new guests, lockfiles, `docs/image-ids.md`), `rfp-attest-integration` (`cli/Cargo.lock`),
   `rfp-measure` (`bench/precompile/Cargo.lock`), `rfp-ci-release` (`ci.yml`, `scripts/ci/`), `rfp-docs-final`,
   `rfp-logos-core-eth`, `rfp-relayer-robustness`, and this branch. Rebuilding ids before they merge would have to be
   done twice.
3. **Phase 1** on one branch from `main`: toolchains, manifests and lockfiles of the Ethereum-side workspaces, rebuilt
   ids recorded (old and new), regenerated fixtures, risc0-ethereum v3.1.0, the vault tests below; a fork rehearsal of
   the new deployment. The cargo-deny job of review M4 lands with it (the crates it refuses were yanked when read on 21 November
   2026; when they were yanked is not recorded).
4. **Phase 2** when Logos tags LEZ on 3.1: the LEZ programs and LEZ-linked hosts, real proofs on that tag, the day-one
   kit and the lab rebuilt.
5. **Live, the owner**: new vaults on Sepolia with the new ids and the D1 wiring, a round trip, `verify-sepolia.py`,
   Sourcify; then the README and EVIDENCE rows of the live runs replaced.

What waits on Logos: a LEZ tag on risc0 3.1 (phase 2 and everything LEZ-side after it); Logos's own answer for LEZ's
privacy-preserving transactions on rc1, rc3 and the public testnet (v0.3.1); testnet 0.4, which, built from `dev`,
would refuse our current 3.0.5 receipts, so the day-one kit must be rebuilt in phase 2 before it is used there; the
module SDK and `lez_core` versions the modules link.

## 4. Decision D1 for the new vaults: what stops a broken verifier

The four Sepolia vaults verify through their own stop around our own verifier, the stop's owner renounced (D13,
proposal §B5; on the v3 vault, the keyless `0xC820…`), and they were deployed with a freeze authority nobody holds and no caps. When the verifier proved
unsound, only the admin's caps could stop them, and only because the testnet floor is zero. Options:

- **(a) Keep our own verifier 3.1.0 behind our own stop, owner renounced.** No third party can halt the vault. A
  broken verifier is answered by the freeze authority (releases and checkpoints frozen at once, renewable every 30
  days), the caps bound what leaves before it acts, and `estop(receipt)` stays open to a proof of exploit.
- **(b) RISC Zero's own 3.1.0 estop** (`0xA1DC…bA29` on Sepolia). No vault change (the vault requires `paused()`, which
  a stop answers and the router does not); RISC Zero's guardian halts it on an advisory. But RISC Zero halted verifier
  3.0.0 8 days 22 hours after publishing the advisory, with its owner key and no proof of exploit (`estop()` on 19
  November 2026: mainnet 22:21:11 UTC, transaction `0xe308…f31c`; Sepolia 22:11:12 UTC, `0x6a1c…3286`); the owner of
  RISC Zero's stops, 3.1.0's included, is one externally owned account per network (`0xF616…d412` on mainnet,
  `0x3a54…387D` on Sepolia); and a halt is permanent: no release and no refund afterwards (D13), so the escrow stays
  locked.
- **(c) Our own stop with an owner (a multisig).** A permanent halt we control; the freeze authority already gives a
  reversible one.

**Recommended: (a), with the deployment parameters fixed**: a freeze authority with a key holder (a multisig in
production, a key the owner holds on a testnet), at least one watcher, release caps on every deployment that holds
anything (a non-zero `releaseCapFloor` in production), and a runbook: on an advisory against the pinned verifier,
the freeze authority freezes releases and checkpoints, the admin sets the deposit cap to zero, then the vault is retired
and replaced. The choice is the owner's and the spec's ([bridge spec v2](spec/bridge-spec-v2.md) §6, D13 of the
[v3 changes](spec/bridge-spec-v3-changes.md), [`trust.md`](../trust.md) §4-5).
Open for the spec either way: how escrow leaves a vault whose verifier is broken, since every exit (release, or a
refund after a sunset checkpoint) rests on a proof.

## 5. Tests that must pass

Phase 1:
- `scripts/reproduce-image-ids.sh build`, then `compare` against the new `docs/image-ids.md`: every Ethereum-side row
  equal, built twice from fresh target directories (with review L1's fix, so a stale `methods.rs` cannot match).
- `cargo deny check advisories` with `yanked = "deny"` on every guest workspace: no yanked RISC Zero crate.
- `cargo test --workspace --locked` at the root; `cargo test -p checkpoint-core --locked` (with `--test redteam`); the
  bridge workspace's tests (`ferrata-bridge-core`, `tests/secfix.rs`).
- A checkpoint and a release proven for real and wrapped with prover image `v2026-10-20.1`: seal selector
  `0xe71e6620`, `receipt.verify(ID)` under 3.1.0.
- `forge test` in `vault/` (all suites, `BrokenVerifier` and the invariants included) and `forge test --match-path
  'test/real/*'` on regenerated fixtures with RISC Zero's verifier 3.1.0; new: a test pinning the deployed verifier's
  `SELECTOR()` and control root to v3.1.0's `ControlID`, and one showing a 3.0.5 seal refused (`SelectorMismatch`).
  `bridge/evm-verify` with its updated selector.
- `e2e/ci/run.sh` (CI's round trip and witness-independence jobs) and `cli/tests/e2e.sh` on risc0-ethereum v3.1.0;
  the reproduction container (`scripts/reproduce/Dockerfile`): 5 of 5.
- A fork rehearsal of the new deployment (`vault/script/fork-rehearsal.sh`, `e2e/crown.sh --fork`) with real seals,
  and `docs/security/risc0-groth16-advisory-2026-11-21/fork-rehearsal.sh` adapted to it: its D1 controls stop a
  forged release.

Phase 2:
- The LEZ harness suites on the new tag (`RISC0_DEV_MODE=1 cargo test -p harness --locked`, every `--test` of
  `docs/evidence-suites.txt`), `lez/updater`, the modules' tests.
- Real proofs on the new tag: `just m0` (checks 2 and 4 GO), the privacy gate's 20 users (`docs/privacy/real/`), the
  rc3-style return leg with a real rule (B) checkpoint into the new vault on anvil.
- `lez/testnet04/day1.sh go` rehearsed on a zone of the new tag; `recheck.py` on its records.
- Live (the owner): the round trip on the new Sepolia vaults, `python3 -I scripts/verify-sepolia.py` on two
  endpoints, Sourcify exact matches; and a release checklist line: no RISC Zero advisory open against the pinned
  versions.
