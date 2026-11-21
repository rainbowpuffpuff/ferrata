# Rows of stream rfp-D: bridge security and privacy (RFP-022)

Branch `rfp-bridge-hardening`, from `f13faee2`. For each row of this stream: the proposed status under
[requirements-status.md](../requirements-status.md)'s rules, what shows each clause of the RFP's text (path, command,
commit), and a proposed row for [EVIDENCE.md](../EVIDENCE.md). The integrator applies them; this file edits neither.

Where everything ran: the LEZ side in LEZ v0.4.0-rc1's own state machine and a copy of its fee settlement (the
in-process harness), RISC Zero dev mode, on the build box; the Ethereum side in Foundry (local EVM) or on anvil (chain
id 1, local); the two halves together in `cli/tests/both_halves.rs` (the vault on anvil, the bridge program in the
harness, the rule (B) checkpoint guest and the release guest executed in dev mode, the vault's verifier the tests'
mock behind RISC Zero's emergency stop, accepting exactly the journals the guests committed). Nothing was sent to any
public network. Nothing is audited.

Commits: `9fa33b7e` (the bridge program since B1: config-keyed accounts, `SetDepth`, `AttestListing`; its consumers),
`2e6a35a6` (its records in `lez/results/b1/`), `d3fb2415` (the privacy suite on it), `8adc8155` (the crown driver's
capped configuration), `5274dcf2` (the vault: listing slot, capped deployment, refusal test), `259b14b9` (static
analysis, vault README), `bddb26e9` (the client's anonymity view), `a6b54add` (both halves at once), `a7bd249a`
(documents), and this file's.

Runs, all on the build box on 21 November 2026 (the run numbers below):

| Run | What | Outcome |
|---|---|---|
| 3 | `cd lez && FERRATA_LEZ_RESULTS_DIR=$PWD/results/b1 RISC0_DEV_MODE=1 cargo test -p harness --locked --test tenant --test bridge_depth --test bridge_delist --test bridge_refusals --test bridge_inbound --test bridge_controls --test bridge_burn --test bridge_private --test bridge_roles --test bridge_secfix --test bridge_time --test bridge_vault_storage --test redteam_bridge --test exit_codes`; the bridge-wire crate's tests; `--test successor prints_the_image_ids` | 39 of 39 harness tests, 8 of 8 bridge-wire tests; the image id `0xac1fb59b…442e` |
| 4 | `scripts/privacy/run.sh lez/results/b1/privacy`; `cli/tests/e2e.sh`; `e2e/ci/run.sh` (CI's round trip); `cd lez/crown/driver && cargo test` | the privacy suite at chance; the CLI's e2e test passed; CI's round trip passed; the crown driver 8 of 8 |
| 5 | `cd vault && forge test --no-match-contract PrivacySuite` | 587 passed, 0 failed (58 suites), every invariant at 256 runs × 100 calls |
| 6 | `FERRATA_BOTH_HALVES_SEEDS=1,2,3,4,5,6,7,8 FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh`; `scripts/security-tools.sh slither aderyn`; `cd fuzz/programs && cargo +1.98.1 check --all-targets`; the bridge module's calls (`cargo test -p ferrata-bridge-calls` on the staged crate) | randomised and fees passed (the migration test could not bind its port: run 7); static analysis as `vault/results/sectools-b1/README.md`; the fuzz programs compile; the module's calls 18 of 18 |
| 7 | Slither on this branch's and on `f13faee2`'s vault; `cli/tests/e2e.sh`; `e2e/both-halves/run.sh migration` | the same 38 results; the e2e test passed with the anonymity assertion; the migration rehearsal passed |

## The image id change (B1)

| | |
|---|---|
| Bridge program, before | `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` (the live runs of November 2026) |
| Bridge program, since B1 | `0xac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`, 607,608 bytes, binary sha256 `5f47b065…7fd8` ([image-ids.md](../image-ids.md)); the harness build of run 3 (21 Nov, build box) computed the same id |
| What moved it | `nullifier(cid, chain, vault, N)` and `holder(cid, account)` (F.16); `Init` refuses a freeze authority that is the admin (the red team's F1); `SetDepth` (F.12); `AttestListing` / `SetListing` (F.10) |
| Unchanged ids | the attestation program `0x816d1fef…03df`, the checkpoint guests, the release guest: the vault pins the checkpoint and release ids; the bridge's id enters only `configId` |
| Consumers updated | the CLI and its wallet, the crown driver, the fuzz programs, the bridge module's calls, the IDL and its vectors (`9fa33b7e`); `docs/image-ids.md`, `e2e/seqnet/drive.sh` and `run.sh`'s inputs, `e2e/crown/cloud/app.py`, `lez/testnet04/day1.sh` (`2e6a35a6`) |
| Live deployments | The four Sepolia v5 vaults and the LEZ programs on our devnets pin configuration ids that commit to `0xccde6171…`: they keep that program and its two known issues ([`bridge.md`](../design/bridge.md) §3). B1 takes effect only with a new vault and a new configuration. This branch's CLI and module address B1's accounts, so operating the old deployments needs the tools at `f13faee2`. |

## Rows

### F.16: strictly separated configurations; documented client switching

**Proposed: in part, now only for the document** (the switching text is in the deployer guide, a first draft); every
other clause shown. If the owner takes the guide as final, **shown**.

| Clause | Shown by |
|---|---|
| Multiple entities, each under its own configuration (registry, fee recipients and rates, admin, caps, finality depth), on the same or different program pairs | `lez/harness/tests/tenant.rs`: configurations A and T of one vault, each with its own roles, caps, depth and registry; T's admin sets T's caps and depth (applied), A's are untouched; `bridge_inbound.rs`, `bridge_controls.rs`: configurations of two vaults side by side. The depth is per configuration since B1 (`SetDepth`, F.12) |
| Strict separation: no privileged access over another's configuration or funds, as if fully separate deployments | `tenant.rs` on the program since B1 (4 tests, run 3, 21 Nov, build box; `lez/results/b1/tenant/`): R1, a second configuration of the vault (its own attestation instance) claims a public-kind deposit whose `s` it knows, and the paired configuration's claim still applies; R1 front-run, a copied plaintext claim ordered first does not block its owner's; R2, a claim to a destination and its burn cost the same with 0, 1,505, 6,049 and 12,098 other configurations' holdings for it; the separation matrix, 12 privileged messages of T's roles naming A refused (253) and 15 internal messages from T's accounts at A's refused (243), A's state identical after each. Vault side: a release binds the vault's `lezConfigId` (`attack_burn_from_another_config_cannot_satisfy_a_release`, `bridge/crates/ferrata-bridge-core/tests/outbound.rs`). Command: `cd lez && FERRATA_LEZ_RESULTS_DIR=$PWD/results/b1 RISC0_DEV_MODE=1 cargo test -p harness --locked --test tenant -- --test-threads 1`. Commits `9fa33b7e`, `2e6a35a6` |
| Document how a client identifies and switches between configurations | [`deployer-guide.md`](../guides/deployer-guide.md) §7, "Identifying and switching" (this branch): a vault pins one configuration, the tool reads `lezConfigId` from the vault it is given; another vault is another configuration. First draft |

Proposed EVIDENCE row: **The bridge program since milestone B1 keeps configurations strictly apart (F.16).**
Numbers: on the program `0xac1fb59b…442e` the malicious-tenant attacks of row 136 fail: a second configuration of the
same vault no longer blocks the paired configuration's claim (R1) or front-runs it, and other configurations'
holdings cost a claim or a burn nothing (R2: the same gas at k = 0 and k = 12,098); 12 privileged messages of one
configuration's roles and 15 internal messages of its accounts are refused at another's (253, 243). Where it ran:
in-process harness, build box, 21 Nov 2026. Proofs: none (dev mode). Re-check: as above. Details:
`lez/results/b1/tenant/`, `lez/results/b1/README.md`.

### Sec.6: soundness of supply

**Proposed: shown.**

| Clause | Shown by |
|---|---|
| Total wrapped supply on LEZ never exceeds the vault's holdings | `cli/tests/both_halves.rs`, `randomised_runs_keep_supply_within_holdings`: eight seeds of 60 random steps (run 6, 21 Nov, build box: 141 deposits of the three denominations from three depositors, 44 root imports, 81 claims, 59 burns, one checkpoint per seed, 25 releases); the invariants held at all 496 checks (62 per seed: before the first step, after every step, at the end). After every step, read from the vault on anvil and the zone: the vault's balance is `escrowed + totalOwed`; the zone's supply is what it minted less what it burnt; supply ≤ `escrowed`; supply + unminted deposits + unreleased burns = `escrowed`. On the vault alone: `vault/test/invariant/` (256 runs × 100 calls, run 5) |
| Minting without a valid deposit fails | the runs' forged claims: a tampered witness (another `s`) refused with 249; `bridge_inbound.rs` and `redteam_bridge.rs` (claims under another root, vault or configuration) |
| Minting twice from one deposit fails | the runs' replayed claims: refused (250, or 255 when the bucket is spent first: the program admits the value before it reaches the nullifier); `bridge_refusals.rs` |
| Releasing without a valid burn fails | the runs' releases no guest proved (refused by the verifier), replayed releases and releases redirected to another recipient (refused); `vault/test/Release.t.sol` |

Command: `e2e/both-halves/run.sh` (builds what CI's round trip builds; `FERRATA_BOTH_HALVES_SEEDS=1,2,3,4,5,6,7,8
FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh randomised`). Results: `e2e/both-halves/results/randomised.md`.

Proposed EVIDENCE row: **Supply never exceeds the vault's holdings, over both real halves at once (Sec.6, F.8).**
Numbers: eight seeds of 60 random steps: 141 deposits, 81 claims, 59 burns, 25 releases, 93 forgeries refused (tampered and replayed claims; unproven, replayed and redirected releases); the four cross-side invariants held at all 496 checks. Where it ran: anvil (chain id 1) and the in-process harness, guests in dev mode, build
box, 21 Nov 2026. Proofs: dev mode (the vault's verifier is the tests' mock behind RISC Zero's emergency stop,
accepting exactly the journals the guests committed). Re-check: as above. Details: `e2e/both-halves/results/randomised.md`.

### Priv.5: refused, repeated and capped claims indistinguishable as to deposit

**Proposed: shown.**

| Clause | Shown by |
|---|---|
| A rejected, a repeated and a cap-rejected claim reveal nothing of which deposit, on LEZ | `lez/harness/tests/bridge_refusals.rs` (run 3; `lez/results/b1/bridge-refusals.md`): on the private path, the default, each of the three is refused before any proof exists (249, 250, 255): no block, no fee, no public account changed, so on chain the three are the same, nothing. On the plaintext path, the explicit opt-out, each refusal names no more of its deposit than an applied plaintext claim (the rejected one less: its `s` is not the deposit's) |
| The same on Ethereum (the release of a burn, and a deposit) | `vault/test/RefusalPrivacy.t.sol` (run 5): a release refused as unproven, repeated or capped reverts with its error's four-byte selector and nothing else, from the calldata a successful release sends; the deposit's three refusals likewise |
| The client names no deposit in a refusal | `cli/tests/e2e.rs` ([EVIDENCE](../EVIDENCE.md) row 85; run 4, B1): every refusal named by source, code and meaning |

Proposed EVIDENCE row: **Refused, repeated and capped claims reveal nothing more than an applied one (Priv.5).**
Numbers: private path: 3 of 3 refusals add 0 blocks and change 0 public accounts; plaintext path: 249, 250 and 255 each
publish the deposit fields an applied plaintext claim publishes, or fewer; vault: 3 release refusals and 3 deposit
refusals revert with 4 bytes. Where it ran: in-process harness and local EVM, build box, 21 Nov 2026. Re-check:
`cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_refusals`; `cd vault && forge test
--match-contract RefusalPrivacyTest`. Details: `lez/results/b1/bridge-refusals.md`.

### F.8: amounts do not identify mints or burns; anonymity-set size

**Proposed: shown.**

| Clause | Shown by |
|---|---|
| Amounts visible on Ethereum do not identify which mint or burn | fixed denominations on both sides (`vault/test/Deposit.t.sol`; 251 for a non-denomination on LEZ); the privacy suite on the program since B1 (run 4; `lez/results/b1/privacy/`): an adversary reading every public byte of 20 users' round trips pairs deposits with private claims, and burns with releases, at chance (0.05); the naive variants at 1.0 |
| State the mechanism and its effect on anonymity-set size | the client's view: `ferrata claim` prints the claim's anonymity set, the deposits of the same token and amount under the root and fee schedule range it names (`ferrata_wallet::claim::anonymity_set`, unit test `the_anonymity_set_is_the_same_token_and_amount_under_the_root_and_range`), with a warning when the deposit is the only one of its amount (`cli/tests/e2e.rs` asserts the line: its only 1 ETH deposit, set 1; run 7). Measured per denomination over both halves: `randomised.md`'s table (over the 81 claims of the eight runs, 0.1 ETH: 33 claims, sets 1 to 10, median 4; 1 ETH: 28 claims, 1 to 7, median 4; 10 ETH: 20 claims, 1 to 6, median 3); a fee change splits a set at its first leaf: `fees.md` (6 deposits of 1 ETH, sets 3 and 3) |

Proposed EVIDENCE row: **The anonymity set of a claim, shown by the client and measured per denomination (F.8).**
Numbers: over 81 claims of eight randomised runs the median set is 4 (0.1 ETH), 4 (1 ETH) and 3 (10 ETH), the smallest 1 (a claim right after the first deposit of its amount, which the client then warns of); with a fee change at leaf 3 of 6 deposits of 1 ETH, every claim's set is 3; the privacy
suite on B1 pairs at chance (0.05 among 20). Where it ran: anvil and the in-process harness, build box, 21 Nov 2026.
Details: `e2e/both-halves/results/randomised.md`, `fees.md`; `lez/results/b1/privacy/summary.md`.

### Sec.4: caps bound value at risk; recommended defaults

**Proposed: in part, now only for the document** (the recommended default and its reasoning are in the deployer
guide §2 and the design note §13; the guide is a first draft). The deployments' default is capped since B1.

| Clause | Shown by |
|---|---|
| Caps bound the value at risk in any rolling window | the buckets on both sides (`vault/test/Caps.t.sol`; `bridge_controls.rs`, `bridge_time.rs`; row 138's broken-verifier invariants) |
| The recommended default, deployed by default | `vault/script/DeployVault.s.sol` (what `script/deploy.sh` and the crown's Sepolia stage send): per token and day 100 times its largest denomination, globally their sum, an immutable release floor of one largest denomination a day; `FERRATA_UNCAPPED=1` for the uncapped testnet deployments of November 2026. `vault/test/DeployDefaults.t.sol` (run 5) builds a vault from exactly what the script sends and fills the day: 99 deposits of 0.1 ETH (each escrowing 0.1002 ETH) pass, the 100th is refused, the bucket is whole a day later; no release cap below the floor. LEZ: the crown driver's configuration is capped likewise (`the_configuration_is_capped_by_default`, run 4) |
| Document the defaults and the reasoning | the deployer guide §2 (this branch updates its row), `docs/design/bridge.md` §13, `docs/trust.md` §2 ("What this means for the bridge"): the loss accepted before a freeze. First draft |

Proposed EVIDENCE row: **Capped by default (Sec.4).** Numbers: the deployment script's vault refuses the deposit past
its ETH bucket (10 ETH a day: 99 deposits of 0.1 ETH and their relay fees), refills it in a day, and keeps every
release cap at or above 300,000,000 value units a day (one 0.1 ETH); the LEZ configuration of the crown driver: 3×10^10
value units a day per token and globally, floor 3×10^8. Where it ran: local EVM; Rust unit test; build box, 21 Nov 2026.
Re-check: `cd vault && forge test --match-contract DeployDefaultsTest`. Details: `vault/test/DeployDefaults.t.sol`.

### F.12: configurable finality depth; changes never invalidate valid claims

**Proposed: shown.**

| Clause | Shown by |
|---|---|
| Inbound depth configurable by the admin per deployment, defaulting to the recommended depth | `lez/harness/tests/bridge_depth.rs` (run 3; `lez/results/b1/bridge-depth.md`): depth 0 (Ethereum finality) at `Init`; `SetDepth` by the freeze authority or an outsider refused (253), above 216,000 slots refused (240); at 65 an import under a record 32 or 64 slots behind the configuration's tip held (242), at 64 applied |
| Outbound depth configurable | the vault's wait Δ (admin, up only, at most 30 days) and per-token release delay: `Checkpoint.t.sol` (`test_finalize_deltaRaiseAppliesToLaterCandidates`, `test_raiseDelta_onlyUpAndBelowTheCeiling`), `ReleaseDelay.t.sol` |
| A change never invalidates a valid claim or release | `bridge_depth.rs`: at the maximum depth, claims against roots imported at depths 0 and 64 apply; a Δ raise does not touch a submitted candidate (`test_finalize_deltaRaiseDoesNotAffectASubmittedCandidate`) |

Proposed EVIDENCE row: **The admin's inbound finality depth (F.12).** Numbers: `SetDepth` only by the admin, at most
216,000 slots; an import held (242) until the configuration's tip is that many slots past its record; claims under roots
imported earlier apply at any depth. Where it ran: in-process harness, build box, 21 Nov 2026. Re-check: `cd lez &&
RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_depth`. Details: `lez/results/b1/bridge-depth.md`.

### F.10: register and deregister tokens; document registry changes

**Proposed: in part, now only for the document** (delisting is in the deployer and user guides, first drafts); the
behaviour is shown.

| Clause | Shown by |
|---|---|
| The admin registers a token (address, LEZ definition, decimals, permitted amounts, caps) | as before: `Admin.t.sol`, `RegistryStorage.t.sol`, `bridge_vault_storage.rs`, `bridge_controls.rs` |
| The admin deregisters a token, on both sides | the vault: `setDepositsEnabled(token, false)` stops deposits and now publishes the listing at `sha256("ferrata/vault/listing/v1" ‖ token)` (`vault/test/Listing.t.sol`, run 5); LEZ: `AttestListing` mirrors it in `token(cid, token)` (`delisted`, the record slot), never undone by an older record (`bridge_delist.rs`, run 3) |
| With minimum impact on users | `bridge_delist.rs`: a claim of a deposit made before the delisting, a burn and a transfer all apply after it; `Release.t.sol` (`test_release_worksForDelistedToken`) |
| Documented | [`deployer-guide.md`](../guides/deployer-guide.md) §5, "Delisting"; [`user-guide.md`](../guides/user-guide.md) §12, "Delisted" (this branch). First drafts |

Proposed EVIDENCE row: **Delisting on both sides (F.10).** Numbers: the vault writes 1 at the listing slot when it
delists and 0 when it relists; on LEZ the delisting is attested in one transaction (223,532 execution gas), an older
record refused (247); afterwards a claim (270,070), a burn and a transfer apply. Where it ran: local EVM; in-process
harness; build box, 21 Nov 2026. Details: `lez/results/b1/bridge-delist.md`.

### R.4: valid claims never invalidated by later chain activity

**Proposed: in part** (unchanged), with a narrower reason: every later act of any party only delays (caps, freezes,
a depth change, delisting, a fee change, a sunset of the root imports), over both halves; two disclosed exceptions
remain, neither closable without changeable verification logic, which Sec.8 forbids: after RISC Zero's emergency stop
(tripped only by a proof that its proof system is unsound) nothing more is released or refunded, and tokens not burned
before a LEZ change the immutable checkpoint program cannot follow lose their backing.

| What later activity | Shown by |
|---|---|
| a cap | `bridge_controls.rs`: a claim refused by the bucket (255) stays claimable and applies after the refill (the randomised runs' buckets never filled: 0 capped claims) |
| a freeze | `Freeze.t.sol`; `bridge_time.rs` |
| a depth change | `bridge_depth.rs` |
| a delisting | `bridge_delist.rs`; `Release.t.sol` |
| a fee change | `both_halves.rs`, `fee_schedules_over_both_halves`: every claim mints its own leaf's schedule (`fees.md`) |
| a retirement and `Sunset` | `both_halves.rs`, `migration_rehearsal_honours_positions_in_flight`: an imported deposit claimed after `Sunset`; a burn released under the sunset checkpoint, which also holds a burn made after `Sunset`; the deposits never imported refunded whole (they were never valid claims: a deposit is claimable once a root holding it is imported) |
| the emergency stop | the same test: after it, a proven release and a refund refused; the other configuration's vault goes on |

### F.13: protocol fees on mint and burn, not fingerprinting

**Proposed: in part** (unchanged), with what the tests now show over both halves and what remains a design question
for the owner: within one fee schedule every claim and every release of a denomination pays the same fees, so the fee
singles no user out; across a fee change a claim shows which schedule its deposit falls under (its range), which
splits the anonymity set at the change, by deposit time, as the RFP's timing signal does. The client counts it.

| Clause | Shown by |
|---|---|
| A fee on minting and on burning, in the wrapped token, at a rate the admin configures per deployment | `fee_schedules_over_both_halves` (run 6; `e2e/both-halves/results/fees.md`): the admin announces mint 0.5 %, burn 0.2 % and checkpoint 0.1 % fees on the vault; seven days later they are in force from leaf 3 and attested on LEZ (`ImportFees`; a forged copy a second early refused); each claim mints its deposit's escrow: under schedule 0 the denomination and the relay fee, under schedule 1 the mint fee too, to the fee recipient on LEZ; each release pays 1 ETH less 0.003 ETH, the burn and checkpoint fees credited on the vault (`totalOwed`). The burn fee is taken at the release (the RFP's Fee structure §3 allows "at burn or at release"). `vault/test/Fees.t.sol`; `bridge_vault_storage.rs` |
| The fee value does not distinguish a user's transaction from others | the same test: every claim of 1 ETH under one schedule mints the same fees and has the same anonymity set (3 and 3), every release the same payout. Across a change, see above |
| The admin can set either fee to zero | `Fees.t.sol` (`test_release_zeroFeesCreditNothing`; schedule 0 charges no protocol fee); the 1 % ceiling fuzzed |

Proposed EVIDENCE row: **Fees over both halves (F.13).** Numbers: schedule 1 in force from leaf 3, seven days and
1 s after its announcement; claims under it mint 0.005 ETH more, to the fee recipient on LEZ; each release
pays 0.997 ETH, and the vault's `totalOwed` rises by 0.003 ETH a release; sets 3 and 3. Where it ran: anvil and the
in-process harness, guests in dev mode, build box, 21 Nov 2026. Details: `e2e/both-halves/results/fees.md`.

### Sec.8: verification logic cannot be swapped; migration and in-flight positions

**Proposed: in part, now only for the document** (the deployer guide §9, a first draft): the migration was not run
before and is now rehearsed over both halves.

| Clause | Shown by |
|---|---|
| No party changes what the bridge accepts | as before (immutable image ids and verifier, read back on Sepolia; programs deployed immutable) |
| Change means a new version and a migration; in-flight deposits and burns honoured | `migration_rehearsal_honours_positions_in_flight` (run 7; `e2e/both-halves/results/migration.md`): vault A and its configuration with five deposits (one imported and claimed, one imported and not claimed, three never imported) and a burn; A's freeze authority retires A; a new deposit refused; the zone attests the retirement and anyone runs `Sunset`; `ImportRoot` after it refused (242); the imported deposit is then claimed; a second burn after `Sunset`; the sunset checkpoint (2 burns, 2 imported leaves) finalised and the first burn released on the retired vault; after the revocation window two of the never-imported deposits refunded whole to their depositor (refused inside the window, refused a second time; the imported ones refused); a new vault and configuration B take a deposit, its claim and a burn; 9 forgeries refused; the books checked 6 times on A and 2 on B |
| Document the mechanism | [`deployer-guide.md`](../guides/deployer-guide.md) §9 (cites the rehearsal), `docs/design/bridge.md` §16. First draft |

Proposed EVIDENCE row: **A migration rehearsed with positions in flight (Sec.8, R.4, Sec.9).** Numbers:
five deposits on the old vault: one imported and claimed before the retirement, one imported and claimed after `Sunset`, two never imported refunded whole after the sunset checkpoint's window, one left for the stop; a burn released under the sunset checkpoint; a new vault and configuration take a deposit, claim and burn; after RISC Zero's emergency stop the old vault's proven release and its refund are refused and its 1.1012 ETH of escrow stays, while the new vault goes on; the books balance at every check. Where it ran: anvil and the in-process harness, guests in dev mode, build box, 21 Nov 2026.
Details: `e2e/both-halves/results/migration.md`.

### Sec.9: a failsafe that is not a backdoor, or why none

**Proposed: in part, now only for the document** (`docs/trust.md` §6, a first draft, says why the failsafe is
stop-only); the stop's behaviour is shown on both halves.

| Clause | Shown by |
|---|---|
| The failsafe stops; it never moves funds | `migration_rehearsal_honours_positions_in_flight`: RISC Zero's emergency stop on vault A's verifier (as a proof of unsoundness would trip it): a proven release and a refund refused, A's escrow untouched and its books still balanced; configuration B's vault goes on. `EmergencyStop.t.sol`; Halmos: nothing released after the stop (row 35); the stop's owner renounced at deployment (rows 41, 91) |
| Why no recovery | `docs/trust.md` §6. First draft |
