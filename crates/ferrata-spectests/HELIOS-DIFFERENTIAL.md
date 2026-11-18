# Helios as a differential oracle for the light-client core

a16z's Helios is an independent Rust light client. Its consensus core (`helios-consensus-core`, the
`ethereum/consensus-core` crate of [a16z/helios](https://github.com/a16z/helios) at tag `0.12.0`, rev
`2af9b41887e069144ee56cd710f3ddca195473f8`, the newest release tag on 18 November 2026) is a dev-dependency of this
test crate only: no guest, program, module or the core itself links it. Two test files run it next to `ferrata-core`
on the same bytes. Both are built and run by `cargo test --workspace` (CI included); the new file's four tests take 8 to 15 s on the
build box.

| Test file | Inputs | Compared after every update | Result (18 Nov 2026) |
|---|---|---|---|
| [`tests/helios_lab_record.rs`](tests/helios_lab_record.rs) (new) | every light-client update the lab recorded and applied on chain: [`lez/lab/results/record/net/`](../../lez/lab/results/record/), Sepolia **125** (39 period updates, 86 finality updates) and mainnet **95** (9 and 86), each from its kept bootstrap and checkpoint | the verdict of ferrata's policy store (`Adoption::OnChain`, what the program runs), its spec store and Helios; the finalized header (slot, beacon root, so the period), the execution block (hash; number where the layout carries the payload header) and execution root; the current and next committees; the optimistic header; whether the update rotated, advanced, adopted (read off each store, equal to the lab's `jobs.tsv` row) | **220 of 220 updates: the same on every update**; the end states equal the lab's `instances.tsv` |
| [`tests/helios_oracle.rs`](tests/helios_oracle.rs) (earlier) | the official consensus-specs `light_client/sync` vectors (v1.7.0-beta.4, minimal preset), step by step; real mainnet and Sepolia fixtures; mutated real data; fabricated participation from 0 to 32 of 32 | verdicts, finalized and optimistic headers (slot, beacon root, execution root) | **33 of the 52 cases** compared (129 steps: 111 updates accepted by both, 15 forced updates, 3 upgrades; by fork Capella 10, Deneb 8, Electra 5, Fulu 6, Gloas 4): all agree. The other **19** (Altair and Bellatrix bootstraps) Helios cannot decode (`NoMatchingVariant`: it decodes light-client data as an SSZ union of layouts in which the header is a variable-size field, so the fixed-size Altair header, used through Bellatrix, does not decode) |

The record replay, per chain (the run's own output, [`results/helios-differential-2026-11-18.txt`](results/helios-differential-2026-11-18.txt)):

| Chain | Updates | Layout upgrades | Rotations | Advances | Adoptions | Finalized periods | End state (equal to `instances.tsv`) |
|---|---|---|---|---|---|---|---|
| Sepolia | 125 (39 period, 86 finality) | 1 (Electra to Gloas) | 38 | 125 | 39 | 1378 to 1416 | slot 11,600,992, period 1416, root `a062b3c8…47fc`; execution block hash `d4179ad8…0283` (the Gloas layout has no block number) |
| mainnet | 95 (9 period, 86 finality) | 0 | 6 | 94 | 7 | 1907 to 1913 | slot 15,676,800, period 1913, root `4429b5f5…d9e7`; execution block 26,437,017, hash `f450ce6b…e49f` |

## The two inputs on which they part

Both are departures of Helios from the light-client specification; ferrata follows the specification. Neither
occurs in the 220 recorded updates nor in the official vectors (the comparisons above would have failed). Both are
reported upstream (a16z/helios issues 871 and 872). Each has a test that builds exactly the triggering data and pins
what each implementation does.

| | Triggering data | The specification | ferrata-core | Helios 0.12.0 | Test |
|---|---|---|---|---|---|
| **#871** identity key | a sync committee in which one member's key is the identity point (`0xc0` and 47 zero bytes, the compressed point at infinity), that member's participation bit set; the signature is the aggregate of the other participants' signatures | `eth_fast_aggregate_verify` → `FastAggregateVerify` runs `KeyValidate` on every participant's key, and `KeyValidate` refuses the identity: invalid | spec store: `Err(PubkeyInvalid)`; policy store: refuses the committee at bootstrap (`CommitteeInvalid`, under both committee checks) | accepts: it adds the participants' keys (with more than half signing, it takes the committee's `aggregate_pubkey` and subtracts the non-participants'), and the identity adds nothing. With 32 of 32 bits set it finalizes the update's header (slot 64 → 72); with 16 of 32 it moves its optimistic header to the attested slot 80 | `helios_871_the_identity_key_among_the_participants` (a fabricated minimal-preset chain whose 32 keys the test holds: no genuine committee holds the identity, whose deposit could not be signed; it does not involve LEZ) |
| **#872** empty finalized header | a genuine finality update (mainnet's kept `00002.finality.electra.ssz`) with its finalized header replaced by the empty `LightClientHeader()`, its genuine (non-zero) finality branch kept; the signature covers only the attested header, so anyone holding a genuine update can build it | a non-zero branch makes it a finality update; at finalized slot `GENESIS_SLOT` the header must be empty and the branch must prove the zero root at the finalized-root gindex; the kept branch proves the real root: invalid | both stores: `Err(FinalityBranchInvalid)`; nothing changes | accepts: `default_header_to_none` turns any finalized header at slot 0 into "no finalized header", and then no finality branch is checked. Its finalized header stays, its optimistic header moves to the attested header (slot 15,625,920 → 15,626,048) | `helios_872_a_finality_branch_under_an_empty_finalized_header`, which also shows the precise trigger (the genuine finalized header with only its slot set to 0: ferrata `FinalizedHeaderInvalid`, Helios accepts) and the control (branch zeroed too: no longer a finality update; the spec store and Helios accept, the policy store, which applies finality updates only, refuses with `NotFinalized`) |

`helios_oracle.rs`'s `where_helios_departs_from_the_specification` pins the same two inputs on fabricated data, with
the departures Helios 0.9.4 had and 0.12.0 no longer has.

## Re-run

```sh
scripts/fetch-vectors.sh            # the official vectors (helios_oracle.rs only)
cargo test --locked -p ferrata-spectests --test helios_lab_record --test helios_oracle -- --nocapture
```

Ran on the build box (a Modal CPU container), 18 November 2026, rustc 1.93.0: 4 + 9 tests passed
([`results/helios-differential-2026-11-18.txt`](results/helios-differential-2026-11-18.txt)). The record test reads
the lab's record as committed; when the lab's record grows, it replays the new updates too, and still requires the
end state of `instances.tsv`.
