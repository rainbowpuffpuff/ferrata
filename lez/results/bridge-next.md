# The bridge program's second build: nullifier and holder accounts keyed by the configuration (18 November 2026)

The fix of the two known issues of the deployed v0.4 bridge program ([`docs/design/bridge.md`](../../docs/design/bridge.md)
§3, "Known issues, 18 November 2026"; [`tenant.md`](tenant.md)), built as a **second build** of the program beside
the deployed one and run through LEZ v0.4.0-rc1's state machine and fee settlement (the harness). **Not deployed: no
vault pins it.** The deployed program (`0xccde6171…71dd`, the image the live Sepolia vault's `configId` commits to)
still has both issues, until milestone B1 deploys this one with a new vault (a new `configId`: the image id is part
of it).

- **Where:** the Modal build box `ferrata-wt-bridge-next`, 18 November 2026 15:01–15:48 UTC (four jobs:
  [`bridge-next/job1-tests.log`](bridge-next/job1-tests.log), [`bridge-next/reproduce/`](bridge-next/reproduce/status.txt),
  [`bridge-next/job3-tests.log`](bridge-next/job3-tests.log), clippy);
  dev mode (the harness executes every transition and meters its cycles as LEZ does; no proofs).
- **What is placed, not executed:** as in [`tenant.md`](tenant.md), the rogue configuration's record and the other
  configurations' holdings of R2 (below). Everything else is executed.

## What changed

| | deployed program, `lez/guests/bridge` | second build, `lez/guests/bridge-next` |
|---|---|---|
| nullifier account | `nullifier(chain, vault, N)`, one for every configuration | `nullifier(cid, chain, vault, N)` |
| holder account | `holder(account)`, one for every configuration | `holder(cid, account)` |
| image id | `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` | `0x7c7d2b3acc2f18e5714fa9fc543015bd4dadc5ce625d2b2bbdec50b56ab71e2f` |
| binary (R0BF) | 592,840 bytes | 593,224 bytes |

- [`bridge-wire/src/config_keyed.rs`](../bridge-wire/src/config_keyed.rs): the keyed seeds and actors (the crate
  root's tags with `cid` in front, as the LEZ v0.3.1 port has them), behind bridge-wire's cargo feature
  `config-keyed`, off by default. It is declared on the last lines of `lib.rs`, so the deployed program compiles the
  same sources line for line (a panic's location is in the binary). Its unit test checks that two configurations get
  two accounts and that the tagging is the deployed program's.
- [`guests/bridge-next/src/main.rs`](../guests/bridge-next/src/main.rs): `guests/bridge/src/main.rs` with the keyed
  addresses wherever a nullifier or holder account is named: `Mint`'s sender and receiver checks, `Issue`'s spend
  target, `Spend`'s receiver check and mints, `Burn`'s forward to and check at the holder account, `Transfer`'s
  credit, check and forward, `Credit`'s receiver check (`diff lez/guests/bridge/src/main.rs
  lez/guests/bridge-next/src/main.rs`: those lines and comments only). Every message, state and other check is the
  deployed program's. Its lockfile is `guests/bridge`'s with the package's name changed; `methods-bridge-next`
  builds it apart from `methods`.
- What the fix rests on: `holder(cid, ·)` is credited only by `Mint` from `nullifier(cid, …)` and by `Credit` from
  `token(cid, token)`, and each checks that it runs at `holder(cid, to)` of the message's own `cid`. A holder account
  therefore holds at most one holding per token registered under its configuration, which only that
  configuration's vault's registry extends; no other configuration can lengthen it.
- Not added: the v0.3.1 port's refusal of an oversized state (255). LEZ v0.4 has no limit on an actor's state
  ([`tenant.md`](tenant.md), R2), and with the keyed holder account no state that others can lengthen remains.
- The harness runs every bridge suite against either build: its feature `bridge-next` selects the second build,
  `harness::bridge::{holder_of, nullifier_of}` give either build's address (the tests use them), `World::holder` and
  `World::nullifier` take the configuration, and the results go to `lez/results/bridge-next/`, so the deployed
  program's records are never overwritten.

## The deployed guests are unchanged: a real rebuild in fresh target directories

On the box, after the second build's tests (job 1), job 2 moved the target directories job 1 had filled out of the
tree (`lez/target`; `checkpoint/` and `bridge/` had none on this box) and wrote
[`reproduce/fresh.txt`](bridge-next/reproduce/fresh.txt) at 15:15:50 UTC: `lez/target`, `checkpoint/target` and
`bridge/target` absent. Then:

| step | result |
|---|---|
| `scripts/reproduce-image-ids.sh build --out …` | 319 s, every guest compiled from source ([`repro-steps.txt`](bridge-next/reproduce/repro-steps.txt)); every guest binary in the three target directories is newer than `fresh.txt` ([`rebuild-evidence.txt`](bridge-next/reproduce/rebuild-evidence.txt)) |
| `scripts/reproduce-image-ids.sh compare --out … --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` | **6 of 6** of the crown run's programs equal the recorded ids, the vault's `checkpointImageId` and `releaseImageId` equal to the rebuilt ids; **14 of 14** guests equal `docs/image-ids.md` ([`compare.log`](bridge-next/reproduce/compare.log)) |
| `cargo build -p methods-bridge-next --locked` in the same fresh `lez/target`, through the same wrapper | 34 s; `ferrata_bridge_next` `0x7c7d2b3a…1e2f` |
| `guest_ids.py`, then `compare` against `docs/image-ids.md` | **15 of 15**, the second build's row included ([`compare15.log`](bridge-next/reproduce/compare15.log), [`ids-with-next.tsv`](bridge-next/reproduce/ids-with-next.tsv)) |

The deployed bridge program rebuilt as `0xccde6171…71dd`, binary sha256 `11d83248…6070`, 592,840 bytes, as recorded.
The same id came out of job 1's build of the second build in the box's working target (`7c7d2b3a…1e2f`), and the
id `tenant.rs` asserts. Job 2 then put job 1's target directories back and re-ran the deployed program's eight bridge
suites (19 tests, all pass): their records came out byte for byte as committed (`mrun` copies back only changed
files, and copied back none of them); job 1 had done the same for the deployed tenant test (3 pass, records
unchanged).

## R1 against the second build: the paired configuration's claim applies after the rogue configuration's

[`bridge-next/tenant/r1.txt`](bridge-next/tenant/r1.txt): the deployed program's test with the same rogue
configuration B (configuration A's fields, another attestation instance whose record is placed) and the same order.

| step | deployed program ([`tenant/r1.txt`](tenant/r1.txt)) | second build: outcome | execution gas |
|---|---|---|---:|
| B: `Init`, `Register` ETH, `ImportRoot` n = 9 | applied | applied | 170,246; 328,650; 229,165 |
| B: claim of A's public-kind deposit (leaf 2) with its `s` | applied | applied, at `nullifier(B, …)` and `holder(B, dest)` | 260,855 |
| A: `Init`, `Register`, `ImportRoot` n = 8, two `ImportFees` | applied | applied | 171,027 to 331,790 |
| **A: claim of the same deposit by its owner** | **refused, 250** (213,992 gas) | **applied**: 1 ETH under A at `holder(A, dest)` | **264,728** |
| A: the same claim again | (not run) | refused, 250 `ALREADY_CLAIMED`: once per configuration | 215,563 |
| A: claim of leaf 6 (10 ETH) | applied | applied | 294,762 |
| B: claim of the invented deposit, the payer's burn of it | applied; burn under `configId(B)` only | applied; `burn(configId(B), H(note))` written, `burn(configId(A), H(note))` empty | 265,190; 160,765 |
| B: A's private-kind deposit (leaf 5): `s` guessed; as public kind; plaintext with its `s`; private claim as B's own account | 249, 249, 252, 252 | 249, 249, 252, 252; its nullifier unspent under A and under B | 94,768; 94,768; 98,394; 157,256 |

After it: `nullifier(A, …, N)` records `configId(A)`, `nullifier(B, …, N)` `configId(B)`; `holder(A, dest)` holds 1 ETH
under A and `holder(B, dest)` 1 ETH under B. **What the rogue gains:** its mints and burns are under `configId(B)`,
which no vault pins; the vault releases only against the burn root of a finalised checkpoint of the configuration it
pins and a release digest that binds its `lezConfigId` (as in [`tenant.md`](tenant.md)). Within A a deposit still mints
once (250 on the second claim).

**Front-run** ([`bridge-next/tenant/r1-frontrun.txt`](bridge-next/tenant/r1-frontrun.txt)): B's claim with the
witness copied from the payer's pending plaintext claim (1,271 bytes, `s` in it), ordered first: applied (260,840
gas). The payer's own claim after it: **applied** (290,427 gas; refused with 250 on the deployed program). The payer
holds its 10 ETH under A, and B's copy under B, which no vault pays.

## R2 against the second build: other configurations' holdings cost a claim nothing

[`bridge-next/tenant/r2.txt`](bridge-next/tenant/r2.txt). **Placed:** `k` holdings of other configurations for the same
destination as the second build would leave them, each at its own `holder(cid, dest)` (one account per configuration;
on the deployed program they were one list in `holder(dest)`). Then a real claim to `dest` under A, and a real burn by
an owner of 1 ETH it holds under A.

| k other configurations' holdings | bytes of their k accounts | claim to `dest` | execution gas | cycles at `holder(A, dest)` | deployed program, same k |
|---:|---:|---|---:|---:|---|
| 0 | 0 | applied | 260,855 | 18,761 | applied, 260,775 |
| 1,505 | 111,370 | applied | 260,855 | 18,761 | applied, 2,683,111 |
| 6,049 | 447,626 | applied | 260,855 | 18,761 | **failed** at the 10,000,000 gas limit |
| 12,098 | 895,252 | applied | 260,855 | 18,761 | (failed from 6,049 on) |

The owner's burn: 165,055 gas and 30,337 cycles at `holder(A, owner)` with 0, 6,110 and 12,098 other configurations'
holdings (on the deployed program it failed from 6,110). The test asserts that gas, fee and cycles are equal for
every `k`. In the program's executor, `holder(A, dest)` refuses with 243 (`WRONG_SENDER`) a `Mint` under B from
`nullifier(B, …)`, a `Mint` naming A from `nullifier(B, …)`, a `Credit` under B from `token(B, ETH)` and a `Credit`
naming A from `token(B, ETH)`, and accepts a `Mint` from `nullifier(A, …)` and a `Credit` from `token(A, ETH)` (0);
`holder(B, dest)` accepts B's. So the attacker's 3.3 × 10¹⁰ gas of [`tenant.md`](tenant.md) now fills accounts of
its own configurations only.

## Every bridge suite against the second build

`--features bridge-next`, job 1 ([`bridge-next/job1-tests.log`](bridge-next/job1-tests.log)) and job 2 (`tenant`):

| suite | tests passed against the second build | its record (second build) |
|---|---:|---|
| `bridge_inbound.rs` | 3 of 3 | [`bridge-inbound.md`](bridge-next/bridge-inbound.md), [`bridge-gloas.md`](bridge-next/bridge-gloas.md) |
| `bridge_private.rs` | 2 of 2 | [`bridge-private-claim.md`](bridge-next/bridge-private-claim.md) |
| `bridge_burn.rs` | 1 of 1 | [`bridge-burn.md`](bridge-next/bridge-burn.md) |
| `bridge_controls.rs` | 1 of 1 | [`bridge-controls.md`](bridge-next/bridge-controls.md) |
| `bridge_vault_storage.rs` | 1 of 1 | [`bridge-vault-storage.md`](bridge-next/bridge-vault-storage.md) |
| `bridge_roles.rs` | 2 of 2 | [`bridge-roles.md`](bridge-next/bridge-roles.md), [`bridge-transfer-private.md`](bridge-next/bridge-transfer-private.md) |
| `bridge_secfix.rs` | 8 of 8 | (asserts only) |
| `bridge_time.rs` | 1 of 1 | [`bridge-time.md`](bridge-next/bridge-time.md) |
| `privacy_suite.rs` (the LEZ half) | 1 of 1: 82 records (20 private claims, 20 burns, 20 plaintext claims, 20 naive), the bridge core recovers the 20 burn notes | `target/privacy-next/` on the box (not committed, as for the deployed program) |
| `tenant.rs` | 3 of 3 (the second build's three tests above) | [`bridge-next/tenant/`](bridge-next/tenant/r1.txt) |
| bridge-wire's unit tests with `config-keyed` | 2 of 2 | |

Job 3 re-ran all of it, both builds, on the harness's final address helpers (commit `7b92fc25`, one test at a time):
the same counts pass, and every record came out byte for byte as committed
([`bridge-next/job3-tests.log`](bridge-next/job3-tests.log)). Clippy on `harness` and `ferrata-lez-bridge-wire`,
all targets, both builds (with `RISC0_SKIP_BUILD=1`: under clippy-driver risc0-build's guest builds fail, which is
not this code): no warning in either crate after one `allow(dead_code)` on a field that only the deployed program's R2
test reads (job 4, on the final commit); rustfmt clean.

One assertion differs between the builds and says so: `bridge_inbound.rs` expects the payer's holder account to hold
two configurations' holdings on the deployed program and one each in `holder(A, payer)` and `holder(B, payer)` on the
second build.

**The figures that change** ([`bridge-next/compare.md`](bridge-next/compare.md), every cell, by
`lez/scripts/compare-tables.py`; execution gas, deployed → second build):

- a plaintext claim into an empty holder account: +3,057 (`bridge-inbound.md` #18: 261,671 → 264,728); with a mint
  fee and a relay fee, three holder accounts: +3,800 (#31: 290,962 → 294,762); a replay refused with 250: +1,571
  (213,992 → 215,563); a private claim: +2,314 (553,641 → 555,955). The nullifier's seed hashes 32 more bytes, and
  the image differs;
- a claim to a destination whose deployed `holder(dest)` already held another configuration's holding: −6,189
  (#32, vault B's deposit to the payer: 267,044 → 260,855), since `holder(B, payer)` holds B's alone;
- a public burn: −1,422 (`bridge-burn.md` #1: 166,477 → 165,055): `holder(A, payer)` holds A's holding only, 74
  bytes of state where the deployed program's `holder(payer)` held 142 (A's and B's);
- a transfer: +429 (`bridge-roles.md` #12: 94,664 → 95,093); `Register`: −26; `Init` and `ImportRoot`: unchanged.

## Not done

- **Not deployed**, not proven for real (dev mode only), not run on a LEZ sequencer; no vault pins it.
- The CLI, the relayer and the bridge module still compute the deployed program's `holder(account)` and
  `nullifier(chain, vault, N)` (`cli/src/lez.rs`, which the relayer's `relayerd.rs` uses, `cli/wallet/src/claim.rs`,
  `cli/src/flow.rs`, `cli/src/bin/seqnet_chain.rs`, `cli/examples/miniapp_devnet.rs`;
  `modules/ferrata_bridge_module/rust-lib/calls/src/lez.rs`, `session.rs`): no test drives the second build through
  them, and driving it needs the configuration id at those call sites (B1).
- The privacy suite's Foundry half and its adversary (`scripts/privacy/run.sh`) were not run against the second
  build (no `vault/lib` on this box); its LEZ half, the part that runs the bridge program, passes.

## Re-check

In `lez/`, with `RISC0_HOME` the LEZ RISC Zero home ([`RESULTS.md`](../RESULTS.md) §6):

```
RISC0_DEV_MODE=1 cargo test -p harness --locked --features bridge-next --test tenant -- --nocapture --test-threads 1
RISC0_DEV_MODE=1 cargo test -p harness --locked --features bridge-next --test bridge_inbound --test bridge_private \
  --test bridge_burn --test bridge_controls --test bridge_vault_storage --test bridge_roles --test bridge_secfix \
  --test bridge_time --test privacy_suite -- --nocapture
cargo test -p ferrata-lez-bridge-wire --locked --features config-keyed --lib
python3 -I scripts/compare-tables.py results results/bridge-next "deployed program (ccde6171…71dd)" \
  "second build (7c7d2b3a…1e2f)" > results/bridge-next/compare.md
```

The ids, from the root (a fresh checkout has no target directories): `scripts/reproduce-image-ids.sh build --out OUT`,
`scripts/reproduce-image-ids.sh compare --out OUT --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`, then
`(cd lez && cargo build -p methods-bridge-next --locked)` and `python3 -I scripts/reproduce/guest_ids.py . OUT/ids.tsv`
and `compare` again (15 rows). The box jobs' scripts: [`bridge-next/scripts/`](bridge-next/scripts/job2.sh).
