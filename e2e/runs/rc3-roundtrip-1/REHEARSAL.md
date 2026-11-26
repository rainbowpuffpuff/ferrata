# CI's bridge round trip, rebuilt against LEZ v0.4.0-rc3: the rehearsal of `docs/VERIFY.md` §5

26 November 2026, on the build box (Modal), by `e2e/rc3-roundtrip/run.sh rc3-roundtrip-1`: the round trip of CI's
`sequencer-roundtrip` job (`scripts/ci/sequencer-roundtrip.sh` and its hook) with its sequencer, crown-driver and the
`ferrata` tool built against LEZ v0.4.0-rc3, the LEZ of our public test network, instead of v0.4.0-rc1. `README.md`
beside this file is the job's own summary; `run/user/` holds what each `ferrata` command printed; `tools.sha256` the
tools it ran.

The code: the product code of `40e8d799` (this branch changes none of it) with one fix this rehearsal needed, commit
`32216431`: crown-driver's gas-drop module, compiled only against rc3, still derived the holder accounts without the
bridge configuration's id that `crown.rs` has used since `c9894c39`, so crown-driver did not build against rc3 at
`40e8d799`. The CI scripts take the LEZ tag as a parameter (`FERRATA_CI_LEZ_TAG`, default v0.4.0-rc1, CI's). The
guests are built from the committed manifests, as CI builds them: the attestation and bridge programs deployed are
`0x816d1fef…` and `0x847a4336…` (`docs/image-ids.md`), as the zone step records.

What ran, every step ok (`run/steps.tsv`): Sepolia forked by anvil at its finalized block; the two programs deployed
through LEZ's loader on a standalone rc3 sequencer, with the stand-in light-client instance; the vault deployed on the
fork; the user's `ferrata deposit` (a dry run, then sent); the operator's root import; `ferrata scan`; `ferrata claim`
held by its delay, the delay let pass on the fork's clock, a dry run, then `claim --plaintext` sent and applied; the
same claim again refused (exit 250 `ALREADY_CLAIMED`); `ferrata burn` (a dry run, then sent); the rule (B) checkpoint
journal equal to the native one and accepted by the vault after its window; `ferrata release` held, then proven in
RISC Zero's dev mode and paid (997,000,000,000,000 wei to the fresh address); the same release again refused
(`NullifierAlreadyUsed`). Re-check the records offline with `python3 -I scripts/verify.py rc3-roundtrip-record`.

What it is not: the claim is the plaintext variant (the deposit is linked to the claiming account; CI's job claims
so), not the private claim of `docs/VERIFY.md` §5, which this rehearsal does not show on rc3; the stand-ins are CI's
(a test committee for the light client, the vault's verifier replaced on the fork by the tests' mock accepting the
dev-mode journals, `ci_zone`'s keys for the checkpoint's chain, `seqnet-fund` for the gas drop); the sequencer is a
local one in standalone mode, not our test network; nothing was sent to Sepolia or to any public network.

Six runs before this one stopped on the build box, each before the round trip's first step: (1) `crown-driver` did
not compile (the fix above); (2) the script's switch to rc3 stopped when run a second time (now idempotent); (3) the
tool's build found rc1's and rc3's `lee_core` together, because the switch had rewritten only `cli/`'s manifests (now
every manifest, as `deploy/testnet/cli-rc3/build.sh` does); (4) the connection to the box dropped; (5) the restored box
lacked LEZ's sources (uploaded again); (6) `r0vm` 3.1.0 was not installed (`scripts/ci/install-risc0.sh`, as CI
installs it). `run/user/16-release.txt` named the box's RISC Zero home on its command line: masked afterwards with
the rule the hook now applies (`RISC0_HOME=<risc0-home>`).
