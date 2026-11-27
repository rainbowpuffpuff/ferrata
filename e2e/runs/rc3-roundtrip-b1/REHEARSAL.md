# rc3-roundtrip-b1: this tree's `ferrata` against the bridge program our test network runs (`ferrata_bridge_b1`)

27 November 2026, on a build box: CI's bridge round trip rebuilt against LEZ v0.4.0-rc3 (`e2e/rc3-roundtrip/run.sh`,
CI's hook `scripts/ci/sequencer-roundtrip-hook.sh`, the stages `run summary`; `e2e/runs/rc3-roundtrip-1/REHEARSAL.md`
describes that rebuild), with two programs put in place of this tree's:

- the bridge program: `ferrata_bridge_b1`'s binary, sha256 `537cbe7a…a036`, image `0x847a4336…7515` (the program our
  test network runs, `scripts/reproduce/deployed-programs.json`), through `FERRATA_RC3_RT_BRIDGE_BIN` and
  `FERRATA_CI_BRIDGE_ROW=ferrata_bridge_b1` (the hook checks the deployed program against that row of
  `docs/image-ids.md`; `run/lez/session-a.json` records the image the loader holds);
- the operator's `crown-driver`: the one that speaks B1 (this tree's speaks only `ferrata_bridge_f13`), built from the
  tree before ADR 0046 (`65acaa4a`) against LEZ v0.4.0-rc3, through `FERRATA_RC3_RT_DRIVER_BIN`.

Everything else is this tree's, built by the driver from the committed manifests: the vault (ADR 0046's), the release
and rule (B) checkpoint guests, `ci_zone`, and the user's tool `ferrata` (with `lez-rc3`), whose claim is the step under
test. `tools.sha256` lists every binary the run used (`ferrata_bridge.bin` the B1 binary, `ferrata_bridge.built.bin` the
one built here, `crown-driver` B1's, `crown-driver.built` this tree's).

What it shows (`run/steps.tsv`, 14 of 14 ok; CI's check stage passes on the record):

- `ferrata claim` read the image id the zone's program loader names for the bridge account, found B1, and sent the
  plaintext claim in B1's layout with the fee schedule range its token state gives (`run/user/08-claim.txt`:
  `program ferrata_bridge_b1, … names the fee schedule range 0 (leaves 0..1)`; `mint fee 0 ETH`); B1 applied it and
  the user's public account holds the denomination;
- the same claim again was refused before sending (250 `ALREADY_CLAIMED`): the history read B1's applied claim;
- the burn, the rule (B) checkpoint and the release as CI's round trip: paid, and the same release refused.

What it is not: a run against our test network (nothing was sent to it, or to Sepolia: anvil forks Sepolia); the claim
is the plaintext one CI's round trip makes (the private claim's B1 layout is tested in `cli/src/lez.rs`, not run); the
mint fee is 0 under the run's schedule, so B1's minting of a nonzero mint fee beside a vault of ADR 0046 (which credits
it on Ethereum instead) is not exercised, and must not be: the two are not to be paired
(`deploy/testnet/F13-REDEPLOY.md`). CI's stand-ins as in every run of the hook (a test committee for the light client,
the vault's verifier replaced on the fork by the tests' mock accepting the dev-mode journals, `ci_zone`'s keys).
`ferrata` came from the driver's copy of this tree made at commit `e5647aff` (the claim's B1 layout is that of
`6bc88550`; the later changes to `cli/` are a private call's binary check before a gas drop and a burn's journal, the
chain service's B1 claims, and the unknown-image refusal moved before the claim is built).

Re-check: `python3 -I scripts/verify.py rc3-roundtrip-b1-record` (CI's check stage on these records, and the claim's B1
layout in the transcript and the session's image). Re-run on a build machine: build B1's `crown-driver` from `65acaa4a`
against rc3 (`lez/scripts/lez-select.sh v0.4.0-rc3`, `cd lez/crown/driver && cargo build --release --locked`), then
`FERRATA_RC3_RT_BRIDGE_BIN=ferrata_bridge_b1.bin FERRATA_RC3_RT_DRIVER_BIN=crown-driver FERRATA_CI_BRIDGE_ROW=ferrata_bridge_b1
FERRATA_RC3_RT_STAGES="run summary" e2e/rc3-roundtrip/run.sh rc3-roundtrip-b1`.
