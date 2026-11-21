# The guides' worked examples, run on 21 November 2026 (attestation side)

One session on the Modal build box (a CPU container; `env.txt`: Rust 1.98.1, r0vm 3.0.5, `nproc` 2), from `lez/`, on
branch `rfp-docs-final`: the tree of commit `406db4e8` (it adds to `26bfbb57` the two compile fixes of
`lez/updater/tests/every_party.rs` that the rerun used, and corrects one of that file's doc comments, which was done
after the run; no code differs from what ran). Everything ran in LEZ v0.4.0-rc1's state machine in process with RISC Zero in dev mode (executed, not
proven), on recorded Ethereum data. Nothing was sent to any network. `exits.txt` lists each step's exit status and
time.

| Guide and section | What ran | Result | Files |
|---|---|---|---|
| [Consumer guide](../../consumer-guide.md) §12, the worked example `ferrata_holdings` | `RISC0_DEV_MODE=1 FERRATA_LEZ_RESULTS_DIR=… cargo test -p harness --locked --test holdings -- --nocapture` | 1 of 1 passed (two acceptances, ten typed refusals); the table it writes is byte for byte the committed [`lez/results/holdings.md`](../../../../lez/results/holdings.md) | `holdings.log`, `holdings/holdings.md` |
| [`lez/ATTEST-CLI.md`](../../../../lez/ATTEST-CLI.md), the core user journey | `ferrata-attest --follow-to 1408 … config, advance (dry run, sent), advance --finality --send, slot (dry run, sent), slot --expect-eq 0x01 (dry run, sent), log (dry run, sent), status` | exit 3: the one refusal (130 `PredicateFailed`) predicted by its dry run and reproduced when sent; every sent transaction equal to its dry run; following Sepolia 86.9 s, 169 transactions | `attest-journey.txt`, `attest-journey.stderr`, `attest-messages/` |
| [Operator guide](../../operator-guide.md) §0 and §3.6, [`lez/updater/README.md`](../../../../lez/updater/README.md): configure, run, monitor, stop | `RISC0_DEV_MODE=1 bash updater/dev/demo.sh OUT` (the daemon on `updater/dev/dev.toml` for 45 s, `/health` and `/metrics` read, SIGTERM) | `/health` HTTP 200 `ok`, round 29, finalized slot 15,654,944; an unknown path 404; 29 transactions applied (`Init`, 12 `Adopt`, 6 `Open`, 5 `Miller`, 5 `Finish`; 6 jobs finished); exit 0 after SIGTERM; 30 journal lines kept | `updater-demo/` |
| [`docs/trust.md`](../../../trust.md) §2.4, RFP-021 F.12: every off-chain party walked | `RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --test every_party -- --nocapture` | 8 of 8 passed (below) | `every-party.log` |
| [Consumer guide](../../consumer-guide.md) §9.3, RFP-021 F.10: a consumer moves to a replacement | `RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --test replacement -- --nocapture` | 1 of 1 passed (below) | `replacement.log` |

The first attempt at `every_party` did not compile (two borrow errors, `exits.txt` line 4); fixed and run again
in the same box, 8 of 8.

**`every_party`, what each scenario printed** (`every-party.log`):

- submitters: the first operator stopped after `Init`; an account that had never sent anything sent 4 transactions
  and carried the instance to the fixture's finalized slot; the attestation of block 26,384,851 (2 statements)
  applied. A forged update sent unchecked was refused at `Open` (exit 80, `Witness`); the store did not move. That
  check was vacuous: the forger's update went out without its signer witness, so the program refused it for that
  before reading the signature. Fixed and run again below (`every-party-2.log`).
- provers: 6 transactions, all of the public job (`Init`, two `Adopt`, `Open`, `Miller`, `Finish`), none proven.
- beacon providers: two liars and a down node beside an honest one, 0 transactions refused; with third parties only,
  the instance held after 3 transactions (the real bootstrap, which the liar must serve), then the consumer's own
  node carried it and the attestation was applied.
- execution providers: a changed storage proof and a changed account proof refused with 110, the record and the
  instance unchanged; the honest proof attested (2 statements).
- checkpoint source: a false root gave no instance (10 of 10 rounds stuck, nothing sent); an instance from another
  real mainnet checkpoint (the lab's) refused the pinned consumer's request with 211; the checked instance attested.
- sequencers: 6 transactions of a censored submitter left out, the other carried the instance; 6 dropped connections
  cost 6 retries and no duplicate step.
- deployer: its own `Init` sent again refused (203), an `Init` with another chain id refused (201); the instance's
  state unchanged.
- `trust.md` §2.4: 7 parties, each walked by its scenario.

**`every_party` again, after review** (`every-party-2.log`; the same box, 21 November 2026, the tree of commit
`b8b0f256`, `RISC0_HOME` set to the LEZ guest toolchain as `lez/RESULTS.md` §6 says): 8 of 8 passed. What changed:

- submitters: the forger now sends its updates with the signer witness right (built from the store's committee, as
  an honest submitter builds it; the genuine update with the same witness passes every step natively). One signature
  byte flipped: refused at `Open` with 41 (`SignatureMalformed`). The committee's genuine signature over a false
  finalized header (another execution state root, its execution and finality branches recomputed so that both
  verify): `Open` and `Miller` applied, `Finish` refused with 40 (`SignatureInvalid`, the pairing check). Each code
  equal to the native steps'; the instance's state byte for byte unchanged; no record of the false header.
- beacon providers: a lagging node (it serves the checkpoint's bootstrap, not yet the newest finality update) added
  beside the liars and the down node, in both phases; 0 refused, the instance held with third parties only (3
  transactions), the consumer's own node carried it.
- deployer: the codes are now asserted (203, 201), not only printed.
- `trust.md` §2.4: each scenario named in the table must also be a test of the file (a renamed test fails).

**`replacement`** (`replacement.log`): a statement attested under an instance and under its replacement with the same
identifier (`7e5f4c87…555d`); each binding refused at the other's record (211); the old record still attests; the
reference consumer's nullifiers for the same transfer under the old and the new pool differ (`bc8dde6d…`,
`15cb99ed…`).

Box cost of the session: about 0.08 USD for the first command (611 s, a cold build), 0.00 for the rerun (36 s), by
`mrun`'s ledger estimate; the rerun after review about 0.01 USD (20 s, warm). Paths of the box are written `<repo>`.
