# Hops inside the user's proof, at the declared minimum (RFP-021 P.4)

RFP-021 P.4: "Any proving the user's own device must perform has to be practical on the desktop hardware Basecamp runs
on. Measure and document wall-clock time and peak memory on a mid-range laptop and on the lowest specification the
team declares as supported, and state that minimum explicitly. This includes running the attestation in a private
transaction." The declared minimum is 16 GB of memory and 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB
(`docs/traceability.md`, P.4). The private attestation of a fact in the record's own block was measured there before
(EVIDENCE rows 50, 88, 168, 264). This directory measures the other case: a fact older than the record's block,
reached through EIP-2935 hops verified inside the user's own proof.

## What was proven

The reference consumer's private claim (`ClaimDepositPrivately`) of the vault fixture's deposit 0 (Sepolia block
12,111,477) under the record of block 12,112,773 that the program's instance wrote, the deposit's block reached
through a chain of `K` hops (`prove_vault_claim --hops K --shape wallet`): the consumer's two private transitions, the
fee transfer and LEZ's privacy-preserving circuit over them, proven by r0vm 3.0.5 on the CPU, the transaction padded as
LEZ v0.4.0-rc1's wallet pads every private transaction, then settled again from its file by LEZ's private settlement
with the receipt verified, and refused with one bit flipped (`lez/results/p4-minimum/minimum.py`, jobs
`claim-hopsK-pad`). In the reference consumer only this claim carries hops; the plain private attestation
(`AttestPrivately`) verifies a fact of the record's own block.

The chains (`harness::hops::claim_hops`): one hop is the vault fixture's own; two hops the first chain of
`sepolia-hops-2026-11-18`; three, four and five hops the chains of `sepolia-hops-2026-11-27`, fetched for this run
(`crates/ferrata-spectests/fixtures/sepolia-hops-2026-11-27/SOURCE.md`). They are short hops (the deposit is 1,296
blocks before the record): a hop's proofs are the history contract's account proof and the proof of one of its 8,191
slots in the state of the block it starts from, and the hopped block's header, whatever the distance, so `K` short hops
cost what `K` full-window hops (up to 8,191 `K` blocks back) cost.

## Results

From [`table.md`](table.md) (`bench/perf/summarize_p4_hops.py` on the records in `bench/perf/results/p4-hops/`):

| hops | container | `execute_and_prove` (s) | the binary's wall (s) | user cycles proven (segments) | peak GiB: r0vm RSS / container | receipt (B) | transaction (B) | settled, receipt verified | one flipped bit: settlement's exit |
|---:|---|---:|---:|---:|---:|---:|---:|---|---|
| 1 | 16 vCPU, 16 GiB (AMD Zen 3, AZURE eu-west) | 926.6 | 1032.7 | 4,665,022 (7) | 9.19 / 9.24 | 239,551 | 255,976 | yes | 101 |
| 2 | 16 vCPU, 16 GiB (GenuineIntel family 6 model 173, AWS eu-west) | 1035.6 | 1099.7 | 5,859,554 (8) | 9.19 / 9.24 | 239,551 | 255,976 | yes | 101 |
| 2 | 8 vCPU, 16 GiB (AMD Zen 4, AZURE ap-northeast) | 2031.7 | 2116.5 | 5,858,771 (8) | 9.19 / 9.24 | 239,551 | 255,976 | yes | 101 |
| 3 | 16 vCPU, 16 GiB (AMD Zen 3, AZURE us-west) | 1233.4 | 1340.7 | 7,132,443 (9) | 9.20 / 9.24 | 239,551 | 255,976 | yes | 101 |
| 5 | 16 vCPU, 16 GiB (AMD Zen 3, GCP us-central) | 1511.3 | 1604.3 | 9,713,774 (12) | 9.19 / 9.24 | 239,551 | 255,976 | yes | 101 |
| 5 | 8 vCPU, 16 GiB (AMD Zen 3, AZURE eu-west) | 3255.1 | 3365.7 | 9,714,281 (12) | 9.19 / 9.23 | 239,551 | 255,976 | yes | 101 |

Machines: Modal containers with CPU and memory request = limit (gVisor): AMD Zen 3 and Zen 4, and an Intel part for
the two-hop 16-vCPU run (the table names each), one run per row. 16 vCPU are the declared
x86-64 minimum's 16 threads; 8 vCPU are a mid-range laptop's shape held to half of them. `execute_and_prove` is the
proof's own time; the binary's wall adds rebuilding the fixture's state (the instance follows the recorded Sepolia
chain to the record) and settling once. Peak memory: r0vm's resident set (GNU time) and the container's (cgroup).

The consumer's private transitions in the executor, per hop (`lez/results/vault-claim-hops.md`, from
`lez/harness/tests/vault_claim_hops.rs`, CI): 1,505,009 user cycles at one hop, 2,549,468 at two, 3,631,108 at three,
4,717,173 at four and 5,872,696 at five, so a hop costs 1.04M to 1.16M user cycles in a user's proof. The consumer hashes
with software Keccak (`lez/RESULTS.md` §5), so the earlier estimate of 0.18M to 0.23M cycles a hop, which used the
accelerated figure, does not hold for a user's proof.

## Declared

**A user's own proof carries at most two hops** (16,382 blocks, about 2.3 days of Ethereum blocks at 12 s, behind the
record it reads; ADR 0056). At the declared minimum's x86-64 shape (16 vCPU, 16 GiB) the padded private claim settles
within M0's 20 minutes (`scripts/m0.py`, `LIMIT_S`) through one hop (926.6 s) and two (1,035.6 s), and not through
three (1,233.4 s) or five (1,511.3 s), the proof growing from 7 to 8, 9 and 12 segments. Peak memory stays at 9.19–9.20 GiB (r0vm), and the
transaction (255,976 B) and its receipt (239,551 B) are the same at every hop count, so the transaction does not say
how many hops the proof carried. A consumer's client with an older fact reads an older record, one within two hops of
the fact (records never expire), whose slot is public and narrows when the fact happened, or uses the public path.
The programs are unchanged and no client enforces the limit yet.

## Not measured

- **The Apple M1 alternative of the declared minimum, with hops.** Every earlier proof took 1.35–1.36 times as long
  on the M1 as at 16 vCPU (`lez/results/p4-macos/README.md`); at that ratio the claim would take about 1,250–1,265 s
  at one hop and 1,400–1,410 s at two, above 20 minutes (*derived*). The owner's action: the p4-macos pipeline with
  `prove_vault_claim --hops 1` and `--hops 2` (`--shape wallet`).
- **A laptop.** The 8-vCPU containers are a mid-range laptop's shape held to half the minimum's threads (2,031.7 s
  at two hops, 3,255.1 s at five); no laptop ran the claim with hops.
- **Four hops**, and more than one run of each: one run per row; the containers' CPUs differ (AMD Zen 3 and Zen 4,
  an Intel part for the two-hop 16-vCPU run), so a row is one machine's figure, not a median.
- **The plain private attestation through hops**: the reference consumer's `AttestPrivately` takes no hops; a
  consumer that reads a fact through hops privately costs about what the claim's verification costs per hop (the
  executor figures above), plus its own logic.

## Reproduce

On the build box (`lez/`): `cargo build -p harness --locked --bin prove_vault_claim`, then
`bash scripts/stage-prove-vault-claim.sh OUT/claim-hops` and r0vm 3.0.5's CPU binary in `OUT/r0vm/r0vm`. On the
laptop, with a Modal profile: `modal volume put <volume> OUT/claim-hops /bundle/claim-hops` (and `OUT/r0vm`
`/bundle/r0vm`), then from `lez/results/p4-minimum/`: `P4_VOLUME=<volume> P4_CPU=16 modal run --detach
minimum.py::main --job claim-hops5-pad --runs 1 --tag <tag>` (and `P4_CPU=8`, other `K`); `modal volume get <volume>
/results/<tag> bench/perf/results/p4-hops/`, each `prove.log` compressed with `gzip -9 -n`. The table:
`python3 -I bench/perf/summarize_p4_hops.py bench/perf/results/p4-hops > lez/results/p4-hops/table.md`; CI checks it
byte for byte (`python3 -I bench/perf/test_summarize_p4_hops.py`, the links job). The cycles: `RISC0_DEV_MODE=1 cargo
test -p harness --locked --test vault_claim_hops` (CI's harness-4 shard).

The binary proven was built from this branch's tree at commit `9a528421` before its formatting-only changes (the stage's
manifest names it `t-021-wip`): consumer image `0x8464603b…`, program image `0x816d1fef…`, the deployed ones
(`docs/image-ids.md`).
