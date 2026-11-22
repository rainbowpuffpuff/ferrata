# ferrata_attest_module in a headless Logos Core on a running LEZ sequencer (run r6, 21 November 2026)

RFP-021 U.1 (LEZ reads and sends through Logos Core's modules) and F.9 (the checkpoint check's beacon reads through Logos
Core's `beacon_api_module`). `ATTEST_SEQ_HOOK=modules/ferrata_attest_module/scripts/logos-run-hook.sh bash cli/tests/attest-seq.sh r6`
on the Modal build box, tree `002ebb96`, with the packages of
`modules/ferrata_attest_module/scripts/logos-build.sh` (`logos/lgx-sha256.txt`): `ferrata_attest_module` (dev `.lgx`,
logos-module-builder 5f74cdb), Logos Core's `lez_core` (logos-execution-zone-module cc73ebf with
`modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch`, built from source in 1,620 s) and `beacon_api_module`
(logos-evm-beacon-api-module aca138e, builder 5f74cdb's locks, 152 s), `logosctl` 66d098d. The zone is U.2's (a LEZ
v0.4.0-rc1 zone we started, the final attestation program); the run first repeats U.2's 22 commands and 2 monitor reads (`cli/`, 22 of 22
commands with the expected exit status), then drives the module in the Logos Core. Nothing was sent to or read from a public network.

| Step | Through | Result (measured) |
|---|---|---|
| Logos Core | `logosctl daemon`; `install` of the three `.lgx`; `module load` | loaded (`logos/modules-loaded.json`) |
| a LEZ wallet inside Logos Core | `ferrata_attest_module.lez` → `lez_core.open(config, storage, password, force_rpc)` | opened (the zone's head 50); the payer is its public account holding LEZ's published local test key |
| reads | `lez_read` → `lez_core.get_account_public` | the Sepolia instance (585 bytes), the mainnet instance (505) and the mainnet record (365): **each byte-equal** to the plain path's (`ferrata attest state`, the zone's final blocks followed with LEZ's block application) (`logos/reads.tsv`) |
| a consensus-state update | `advance` (the module's planning, the updater's code, on the state read through `lez_core`, with the committee of period 1412) → `lez_submit` → `lez_core.send_generic_public_transaction` ×3, each waited for with `poll_transaction_status` | the job of Sepolia's finality update of 12 Nov (the lab's record, entry 65): `Open`, `Miller`, `Finish` **applied** on the zone (blocks 51–53: 2,209,930, 3,305,374 and 3,721,523 execution gas); the instance read again through `lez_core`: equal to the plain path's; finalized slot 11,567,104 → 11,567,392 |
| an attestation | `fetch_log_proof` (recorded mainnet JSON-RPC) → `dry_run_attest` on the record read through `lez_core` → `build_attest_tx` → `lez_submit` | the module's dry run: attested, code 0; the zone: **applied** (block 54, 93,316 execution gas, 1 event) |
| a refused attestation | the same with a byte of the receipt proof changed | the module's dry run: refused, 110 `ProofNodeMismatch`; the zone: **refused with 110** (block 55) |
| F.9 | `beacon_api_module.set_beacon_config` (three providers on loopback answering what Lodestar, publicnode and Nimbus answered in `verify_checkpoint`'s recorded live run of 13 Nov) → `verify_checkpoint` with `providers: {via: beacon_api_module}` | verdict **ok**: lodestar, publicnode and nimbus each `agrees` through `beacon_api_module.raw_get` (slot 11,562,112), 3 agreeing hosts (`logos/verify-checkpoint-beacon-api-module.json`) |

Every module call and its reply: `logos/calls.jsonl` (18). The zone's verdict on each transaction `lez_core` sent comes
from its receipts (`ferrata attest receipt`: `logos/cli-*-receipt*.txt`); `lez_core` itself reports inclusion
(`poll_transaction_status`), not outcomes, as LEZ's sequencer RPC has no receipts.
