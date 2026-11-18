# Blind red-team vectors, 18 November 2026

340 vectors (285 adversarial inputs, 55 positive controls) for Ferrata's light client, its attestations,
the reference consumer, the bridge program on LEZ, the vault, the checkpoint program and the bridge core.
Each states an input (a baseline and one mutation of it), the outcome the **specification** implies
(accepted, or refused with which codes) and the specification sentence it rests on.

| Area | Vectors |
|---|---:|
| `lc`: light client (signatures, participation, slots, periods, replays, finality, headers, committees, Init, steps, successors, forks and Gloas, weak subjectivity) | 110 |
| `attest`: attestations (binding, record headers, accounts, storage, logs, relations, history hops, accumulator leaves, identifiers, read-only and privacy) | 63 |
| `consumer`: the reference consumer (Recommended Consumer Pattern) | 15 |
| `bridge`: the bridge program on LEZ (pairing, imports, claims, burns, internal messages, controls, registry and fees, tenants, privacy) | 118 |
| `vault`: FerrataVault (EVM) | 16 |
| `checkpoint`: the checkpoint program (rules A and B) | 12 |
| `bridge-core`: commitments, nullifiers, burn notes | 6 |
| **Total** | **340** |

## Blindness

Phase 1 (these files) was written before reading any implementation of Ferrata. The commit that adds
this directory, `redteam: phase 1, blind vectors`, is the evidence: its time precedes every reading of
the implementation, which began only for phase 2 (the tests that run these vectors). Read in phase 1,
and nothing else:

- Logos's RFP texts, fetched read-only from `logos-co/rfp` at `920fad92` (6 Nov 2026):
  `RFPs/RFP-021-ethereum-state-attestation.md`, `RFPs/RFP-022-wrapped-erc20.md`.
- `ethereum/consensus-specs` at tag `v1.7.0-beta.4`: the light-client sync protocol (Altair; the Deneb
  and Gloas changes to header validity and generalized indices), `altair/bls.md`
  (`eth_fast_aggregate_verify`), and the IETF BLS draft 04's `KeyValidate`, `CoreVerify`,
  `FastAggregateVerify`.
- Ferrata's specification documents: [`docs/design/attestation.md`](../../docs/design/attestation.md),
  [`docs/design/bridge.md`](../../docs/design/bridge.md),
  [`docs/design/spec/bridge-spec-v2.md`](../../docs/design/spec/bridge-spec-v2.md),
  [`docs/design/spec/bridge-spec-v3-changes.md`](../../docs/design/spec/bridge-spec-v3-changes.md),
  [`docs/trust.md`](../../docs/trust.md).
- The wire formats, types and codes only: `lez/wire/src/lib.rs` without the bodies of
  `attest::evaluate`, `Pool::accept` and `VaultPool::accept` (their doc comments were read);
  `lez/bridge-wire/src/lib.rs` with function bodies elided by a filter. **Disclosure:** the first version
  of that filter failed on signatures that span several lines, so the bodies of five functions of
  `lez/bridge-wire/src/lib.rs` (`vault_slots`, `check_import`, `check_pairing`, `check_register`,
  `check_fees`, about 100 lines) were displayed. No vector's expectation was taken from them; each cites
  a document sentence.
- `crates/ferrata-core/src`: the `Error` enum with its doc comments (`error.rs`), the `StepError` enum
  (`steps.rs`), the module list (`lib.rs`).

Not read in phase 1: the light-client and execution-layer code of `crates/ferrata-core/src`, every guest
(`lez/guests`, `bridge/guests`, `checkpoint/guests`), `bridge/crates`, `checkpoint/crates`, `vault/src`,
every test, every result file.

## Files

- `vectors_lc.py`, `vectors_attest.py`, `vectors_bridge.py`: the vectors as written, one tuple each:
  `(id, target, kind, title, setup, mutation, expect, codes, spec, notes)`.
- `make_vectors.py`: assembles them into `vectors.json` and checks their shape (unique ids, every
  refusal with at least one code, every control expecting acceptance, every code name known from the
  documents, every vector with a specification sentence). `python3 -I -B make_vectors.py`.
- `vectors.json`: what the phase-2 tests read.

## How a vector reads

`expect.outcome` is `accepted` or `refused`. For a refusal, `expect.codes` lists every code the
specification allows, the first being the one it points to most directly; several codes are listed where
the documents do not fix the order of the checks. Namespaces: `core:` (`ferrata_core::Error`, 1–199),
`step:` (the step API, 80–99), `prog:` (the attestation program, 200–217), `cons:` (the reference
consumer, 218–229), `bridge:` (the bridge program, 240–255); `lez:validity-window` (LEZ refuses the
transaction for its timestamp window, with no exit code), `lez:any` (LEZ refuses the transaction itself),
`evm:revert`, and `any` (the specification names no code). Numbers are copied from the documents; the
step API's numbers other than `Phase` (81) are not in them and are resolved by name in phase 2.

Baselines (`B-LC-MAIN`, `B-LC-SEP`, `B-LC-SYN`, `B-AT-MAIN`, `B-AT-GLOAS`, `B-AT-ACC`, `B-CON`,
`B-CON-VAULT`, `B-BR`, `B-VLT`, `B-CKP`, `B-BC`) are described in `vectors.json`.

## Phase 2 rules

Each vector runs through the real programs on LEZ v0.4's state machine (the in-process harness, dev mode)
or, where it has no transaction form, through the core. A result is:

- **PASS**: the outcome matches; for a refusal, the observed code is in `expect.codes`;
- **MISMATCH**: accepted where refused was expected, refused where acceptance was expected, or refused
  with a code outside the list;
- **PANIC**: the program aborted without a coded refusal where a code was expected;
- **UNSUPPORTED**: no fixture or entry point lets a test build the vector as written.

Every mismatch is classified with its evidence: a **program bug** (a finding; programs are not changed),
a **specification ambiguity** (the documents allow both readings), or a **vector error** (the vector
misreads the specification). The expectations in these files are not edited after the phase-1 commit; a
correction is recorded in the review with its reason, never by rewriting the vector.
