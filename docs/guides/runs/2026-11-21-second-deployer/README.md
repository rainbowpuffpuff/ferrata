# A second deployer's own deployment, from a fresh clone (21 November 2026)

The deployer guide's §11 walkthrough ([`deployer-guide.md`](../../deployer-guide.md#11-walkthrough-a-second-deployers-own-configuration-on-anvil-and-a-local-lez-zone)),
followed as someone who is not the author would follow it: a fresh clone of this branch, its own settings, one command,
and every setting read back from both chains. RFP-022 Reliability #14.

## Where and what

- **The clone.** `git clone` of a bundle of this repository, branch `rfp-docs-final` at `6a191ce0`, 0 files changed
  ([`clone.txt`](clone.txt)), on the Modal build box (Linux x86-64), LEZ v0.4.0-rc1 beside it as step 0 says.
- **The settings** of step 1, exactly as the guide prints them: admin `0x5ec0d…ad001` after the handover, freeze
  authority `0x5ec0d…f2001`, fee recipient `0x5ec0d…fee01`, two watchers, Δ 5,400 s, release delay 600 s within the
  floor 300 s and the revocation window 172,800 s, the LEZ roles under the label `deployer-two`, LEZ caps 3,000,000,000
  per token and 9,000,000,000 global.
- **The command** of step 2: `bash e2e/seqnet/run.sh deployer second-deployer-1`.
- **The chains:** a local anvil (chain id 11155111; nothing sent to any public network) and a local LEZ v0.4.0-rc1 zone
  (LEZ's sequencer and a Logos Blockchain node), RISC Zero dev mode for the private proofs.

## Result (measured)

- `run.sh deployer exit 0 after 702 s` ([`walkthrough.log`](walkthrough.log)): the build of the tools from the clone,
  the zone, the vault by `vault/script/deploy.sh` with RISC Zero's own verifier, the smoke deposit, the LEZ
  configuration's `Init`, ETH's `Register`, the root import, the deployer's own token, caps, fees and admin handover.
- **27 settings read back, 27 PASS, 0 FAIL**: [`checks.txt`](../../../../e2e/runs/seqnet-second-deployer-1/checks.txt)
  (admin, freeze authority, fee recipient, both watchers, Δ and its floor, revocation window, minimum and per-token
  release delays, mode (B), `lezConfigId`, the second token's denominations, fees and deposit cap, ETH's release cap,
  ETH's fee schedule 1 in force, the LEZ admin and freeze authority being this deployer's, the roles and caps inside
  `configId`, both LEZ caps, the configuration initialised under that `configId`).
- 83 LEZ transactions (zone blocks 1 to 26) and 18 Ethereum transactions, 7 user commands: the run's records in
  [`e2e/runs/seqnet-second-deployer-1/`](../../../../e2e/runs/seqnet-second-deployer-1/SUMMARY.md) (`SUMMARY.md`,
  `deploy.json`, `identity.json`, `params.json`, `programs.txt`, the zone's and anvil's transactions, logs).
- The final programs deployed on the zone are the images of [`image-ids.md`](../../../image-ids.md): attestation
  `0x816d1fef…`, bridge `0xccde6171…` (`programs.txt`).
- Box cost (estimate by `mrun`): 769 s, 1.84 core-hours, about 0.17 USD.

## What the walkthrough found

The first attempt from a fresh clone (the same day, an hour earlier) stopped while building the `ferrata` tool: a fresh
machine lacks libpcsclite's development files, which LEZ's wallet (linked by the tool) needs for keycard support. The
guide's step 0 did not say so. Step 0 now names the package (commit `6a191ce0`); the second attempt, above, ran from a
clone of that commit with the package installed as step 0 says. Machine paths in the logs are replaced by
`<the clone>` and `<beside the clone>`.

## What it does not show

As the guide's §11 says: no public network, no real proofs, the deployer's own token not registered on the LEZ side,
the new fee schedule not imported on LEZ, the LEZ admin's `SetCap` and the freeze authority's `Freeze` (those are
tested in the harness, EVIDENCE row 74).
