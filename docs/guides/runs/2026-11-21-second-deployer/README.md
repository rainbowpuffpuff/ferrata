# A deployment of its own, from a fresh clone: the deployer guide's walkthrough (21 November 2026)

The deployer guide's §11 walkthrough ([`deployer-guide.md`](../../deployer-guide.md#11-walkthrough-a-second-deployers-own-configuration-on-anvil-and-a-local-lez-zone)),
run twice from a fresh clone of this branch on our build box (a machine already set up for building: the deployer
guide's step 0, *What was tested*): its own settings, one command, and the settings read back. RFP-022 Reliability #14. Both runs show that the steps work as written; neither was a stranger following them.

- **The first run** (below, `second-deployer-1`) had the live run's LEZ bundle and the R0BF programs placed beside the
  clone, files that are not published, and LEZ roles derived from a label anyone can compute.
- **The second run** ([at the end](#the-second-run-every-program-built-in-the-clone-and-fresh-random-lez-roles),
  `second-deployer-2`) built every input in the clone as step 0 now says (every Ferrata program from source; LEZ's
  sequencer and the Logos Blockchain node from their pinned sources, whose builds download prebuilt zk-circuit
  libraries), with nothing placed beside the clone, and gave the LEZ configuration fresh random roles.

## The first run: where and what

- **The clone.** `git clone` of a bundle of this repository, branch `rfp-docs-final` at `6a191ce0`, 0 files changed
  ([`clone.txt`](clone.txt)), on the Modal build box (Linux x86-64), LEZ v0.4.0-rc1 beside it as step 0 then said,
  and the live run's LEZ bundle and R0BF programs placed beside it (not published: the second run builds them).
- **The settings** of step 1, exactly as the guide prints them: admin `0x5ec0d…ad001` after the handover, freeze
  authority `0x5ec0d…f2001`, fee recipient `0x5ec0d…fee01`, two watchers, Δ 5,400 s, release delay 600 s within the
  floor 300 s and the revocation window 172,800 s, the LEZ roles under the label `deployer-two` (keys anyone can derive
  from the label: fit for a local zone only, as the guide's §5 now says), LEZ caps 3,000,000,000 per token and
  9,000,000,000 global.
- **The command** of step 2: `bash e2e/seqnet/run.sh deployer second-deployer-1`.
- **The chains:** a local anvil (chain id 11155111; nothing sent to any public network) and a local LEZ v0.4.0-rc1 zone
  (LEZ's sequencer and a Logos Blockchain node), RISC Zero dev mode for the private proofs.

## The first run's result (measured)

- `run.sh deployer exit 0 after 702 s` ([`walkthrough.log`](walkthrough.log)): the build of the tools from the clone,
  the zone, the vault by `vault/script/deploy.sh` with RISC Zero's own verifier, the smoke deposit, the LEZ
  configuration's `Init`, ETH's `Register`, the root import, the deployer's own token, caps, fees and admin handover.
- **27 settings read back, 27 PASS, 0 FAIL**: [`checks.txt`](../../../../e2e/runs/seqnet-second-deployer-1/checks.txt)
  (admin, freeze authority, fee recipient, both watchers, Δ and its floor, revocation window, minimum and per-token
  release delays, mode (B), `lezConfigId`, the second token's denominations, fees and deposit cap, ETH's release cap,
  ETH's fee schedule 1's values, the LEZ admin and freeze authority differing from the default label's, the roles and
  caps inside `configId`, both LEZ caps, the configuration initialised under that `configId`). Two of these were
  weaker than their names, as a review found: "fee schedule 1 in force" read the announced values, which hold from the
  announcement on (the second run also checks `feeState`); the five LEZ lines compare what `crown-driver config-id`
  computes, bound on chain only through `configId` (the second run also reads the global cap from the zone).
- 83 LEZ transactions (zone blocks 1 to 26) and 18 Ethereum transactions, 7 user commands: the run's records in
  [`e2e/runs/seqnet-second-deployer-1/`](../../../../e2e/runs/seqnet-second-deployer-1/SUMMARY.md) (`SUMMARY.md`,
  `deploy.json`, `identity.json`, `params.json`, `programs.txt`, the zone's and anvil's transactions, logs).
- The final programs deployed on the zone are the images of [`image-ids.md`](../../../image-ids.md): attestation
  `0x816d1fef…`, bridge `0xccde6171…` (`programs.txt`).
- Box cost (estimate by `mrun`): 769 s, 1.84 core-hours, about 0.17 USD.

## What the first run found

The first attempt from a fresh clone (the same day, an hour earlier) stopped while building the `ferrata` tool: a fresh
machine lacks libpcsclite's development files, which LEZ's wallet (linked by the tool) needs for keycard support. The
guide's step 0 did not say so. Step 0 now names the package (commit `6a191ce0`); the second attempt, above, ran from a
clone of that commit with the package installed as step 0 says. Machine paths in the logs are replaced by
`<the clone>` and `<beside the clone>`.

## What it does not show

As the guide's §11 says: no public network, no real proofs, the deployer's own token not registered on the LEZ side,
the new fee schedule not imported on LEZ, the LEZ admin's `SetCap` and the freeze authority's `Freeze` (those are
tested in the harness, EVIDENCE row 74).

## The second run: every program built in the clone, and fresh random LEZ roles

After a review found that the first run needed files only the author had, and LEZ roles anyone could derive, step 0
was rewritten to build every input in the clone (Ferrata's programs from source; LEZ's sequencer and the Logos
Blockchain node from their pinned sources, their zk-circuit libraries downloaded prebuilt by their builds) and the
walkthrough to make fresh random LEZ roles by default
(commits `3573205a`, `26ad3564`). This run executed the guide's §11 as printed: a script took the three `sh` blocks
from the clone's own guide and ran them in order (step 0 without its `git clone` line, done first from a bundle of the
branch, which is not pushed); nothing was placed beside the clone ([`second-run/clone.txt`](second-run/clone.txt) lists
the directory: the clone alone).

- **The clone.** Branch `rfp-docs-final` at `26ad3564` (`second-run/clone.txt` prints `203db290`, its id before
  the branch's messages-only rewrite of 21 November 2026; the tree is the same), 0 files changed, on the build box (Linux x86-64, Ubuntu 24.04
  with the packages of step 0's *What was tested*; `nproc` 2, 64 GB of memory, libpcsclite 2.0.3 as step 0 names it;
  Foundry 1.3.6, cargo 1.93.0; RISC Zero already in `~/.risc0`, a filled cargo registry).
- **Step 0**, 21 November 2026, 18:02–19:04 UTC: exit 0 after 3,696 s ([`second-run/step0-trace.txt`](second-run/step0-trace.txt),
  each command with its start time): the upstream sources fetched (17 s), RISC Zero 3.0.5 and both guest toolchains
  installed into a fresh `../risc0-home-lez` (67 s), the vault's libraries and solc (70 s), every guest rebuilt (607 s)
  and compared: **20 of 20 MATCH**, the 14 programs of [`image-ids.md`](../../../image-ids.md) and the six of
  `e2e/crown/README.md`'s table ([`second-run/image-ids-compare.txt`](second-run/image-ids-compare.txt)); LEZ's
  sequencer built (1,352 s), the Logos Blockchain node LEZ pins built by LEZ's own resolver (1,430 s; both builds
  download prebuilt zk-circuit libraries from logos-blockchain-circuits v0.5.7's release, unverified), the zone's
  bundle packed (153 s). The two R0BF programs it built are byte for byte the ones the first run was given (sha256
  `7b5c3735…`, `7be1349c…`; [`programs.txt`](../../../../e2e/runs/seqnet-second-deployer-2/programs.txt)). Disk after
  step 0: 14 GB beside and in the clone ([`second-run/disk.txt`](second-run/disk.txt)).
- **Steps 1 and 2**, the same day, 20:34–20:46 UTC: the settings of step 1 as printed (no LEZ roles given), then
  `bash e2e/seqnet/run.sh deployer second-deployer-2`: exit 0 after 699 s ([`second-run/walkthrough.log`](second-run/walkthrough.log),
  [`second-run/step12-trace.txt`](second-run/step12-trace.txt)). The run made two fresh random keys for the LEZ
  configuration's admin and freeze authority (accounts `0xfc74ff62…d6c6`, `0x55e06755…8833`), kept in its work
  directory and in no record; the vault used RISC Zero's Groth16 verifier, which DeployVault deployed first
  (`0x25Fa4286…F4b1`, named in `identity.json`).
- **29 settings read back, 29 PASS, 0 FAIL** ([`checks.txt`](../../../../e2e/runs/seqnet-second-deployer-2/checks.txt)):
  the first run's 27, with ETH's fee schedule now also checked in force (`feeState`), the LEZ admin and freeze
  authority compared with the deployer's own accounts, and the LEZ global cap also read from the zone (the gate's mint
  and burn buckets, 9,000,000,000 each). The other LEZ controls are compared as `crown-driver config-id` computes
  them; they reach the chain as `configId`, which the vault pins (`lezConfigId`, read from anvil) and under which the
  zone accepted `Init`.
- 83 LEZ transactions (zone blocks 1 to 26) and 18 Ethereum transactions, 7 user commands: the run's records in
  [`e2e/runs/seqnet-second-deployer-2/`](../../../../e2e/runs/seqnet-second-deployer-2/SUMMARY.md); `git status`
  afterwards showed only that directory new. Two of its log lines (`logs/session-x.log`, `logs/hook.log`) kept the
  clone's path in the `~/…` form LEZ's session logs write, which `drive.sh`'s rewrite and its check of the records
  missed; they were rewritten here as the corrected `drive.sh` rewrites them (`<repo>/`, `<work>/`), nothing else
  changed.
- Box cost (estimates by `mrun`): step 0 3,822 s, 4.08 core-hours, about 0.40 USD; steps 1 and 2 838 s, 2.11
  core-hours, about 0.19 USD.

What it still does not show: a person other than the author following the guide, on a machine of their own. The
build outputs of step 0 could be published (as release assets with their sha256) to spare a deployer that hour; that
is the maintainer's step and has not been done.
