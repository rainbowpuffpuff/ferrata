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

**The critical review of tip `67b4aa77` (21 November 2026) and its fixes.** The review found the B1 program change
sound and every cited test green, and blocked on one High and three Medium findings: B1's entry in `docs/image-ids.md`
took the deployed program's name, which broke the public re-check of the live Sepolia runs (H1); the client's
anonymity set counted deposits a plaintext claim had already published (M1, F.8); the deployment script's outbound
wait defaulted to 600 s, not the recommended depth (M2, F.12); and a crown run as documented would have failed at its
LEZ claims after its Sepolia deposits (M3). Each status below is the review's verdict; where a fix since answers the
review's reason, the row says so, with its commit, and proposes the status for the next review. Fix commits:
`b8fd47b1` (H1), `fb5d340f` (M3), `c7299b2b` and `3300df8a` (M1: the client and the measurements), `a8e93009` (M2,
with L3, L4, L13), `9a365920` (L2), `04253a36` (L1, L6, L7, L9, L10), `8f5944b7` (the both-halves test's deployments
one transaction at a time), `beaf57b7` (run 9's records), and this file's.

Runs, all on the build box on 21 November 2026 (the run numbers below):

| Run | What | Outcome |
|---|---|---|
| 3 | `cd lez && FERRATA_LEZ_RESULTS_DIR=$PWD/results/b1 RISC0_DEV_MODE=1 cargo test -p harness --locked --test tenant --test bridge_depth --test bridge_delist --test bridge_refusals --test bridge_inbound --test bridge_controls --test bridge_burn --test bridge_private --test bridge_roles --test bridge_secfix --test bridge_time --test bridge_vault_storage --test redteam_bridge --test exit_codes`; the bridge-wire crate's tests; `--test successor prints_the_image_ids` | 39 of 39 harness tests, 8 of 8 bridge-wire tests; the image id `0xac1fb59b…442e` |
| 4 | `scripts/privacy/run.sh lez/results/b1/privacy`; `cli/tests/e2e.sh`; `e2e/ci/run.sh` (CI's round trip); `cd lez/crown/driver && cargo test` | the privacy suite at chance; the CLI's e2e test passed; CI's round trip passed; the crown driver 8 of 8 |
| 5 | `cd vault && forge test --no-match-contract PrivacySuite` | 587 passed, 0 failed (58 suites), every invariant at 256 runs × 100 calls |
| 6 | `FERRATA_BOTH_HALVES_SEEDS=1,2,3,4,5,6,7,8 FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh`; `scripts/security-tools.sh slither aderyn`; `cd fuzz/programs && cargo +1.98.1 check --all-targets`; the bridge module's calls (`cargo test -p ferrata-bridge-calls` on the staged crate) | randomised and fees passed (the migration test could not bind its port: run 7); static analysis as `vault/results/sectools-b1/README.md`; the fuzz programs compile; the module's calls 18 of 18 |
| 7 | Slither on this branch's and on `f13faee2`'s vault; `cli/tests/e2e.sh`; `e2e/both-halves/run.sh migration` | the same 38 results; the e2e test passed with the anonymity assertion; the migration rehearsal passed |
| 8 (fixes) | `python3 -I scripts/test_image_ids.py`; `cd lez/crown/driver && cargo test`; `cd vault && forge test --no-match-contract PrivacySuite`; `script/DeployVault.s.sol` run step by step on a local anvil (chain 31337) for rule (B), rule (A) and `FERRATA_DELTA=600`; `scripts/reproduce-image-ids.sh build` then `compare` (no `--vault`); `ferrata-e2e image-id` on the rebuilt LEZ programs and `lez/crown/build.sh`'s bundle check on them; `scripts/privacy/real.sh build` and its dev step's LEZ half; the wallet's and the tool's unit tests; `cli/tests/e2e.sh`; `e2e/both-halves/run.sh`; `redteam_bridge` into `lez/results/b1/`; `e2e/ci/run.sh` | 6 of 6; 9 of 9; 589 passed, 0 failed (58 suites; FerrataVault 24,369 bytes, 207 under EIP-170); complete with the script's own checks, Δ read back 3,600 s, 10,860 s and 600 s, the release floor's refill 3,473 a second; 14 of 14 guests equal `docs/image-ids.md`, 5 of the crown run's 6 programs equal its records and the bridge equal to B1's (`ferrata_bridge_b1`), exit 0; both ids equal the docs' rows, B1's binary accepted, another refused; the expected bridge `0xac1fb59b…442e`, the suite's id assertion passed; 4 and 17 passed; passed; the fee and randomised tests passed, the migration test hung on two transactions anvil never mined (run 9); 9 of 9; passed |
| 9 (fixes) | `e2e/both-halves/run.sh` with the deployments one transaction at a time; `scripts/privacy/real.sh dev`'s three steps, the analysis into scratch (not `docs/privacy/real/dev`, the deployed program's baseline) | the three tests passed (389 s with the build): the migration rehearsal (five deposits, burns before and after `Sunset`, two refunds whole, A's emergency stop refusing a proven release and a refund while B goes on; the books checked 6 times on A and 2 on B, 9 forgeries refused), the randomised runs (the same tables as run 8) and the fee test; the privacy suite's LEZ half with the expected bridge `0xac1fb59b…442e`, its Foundry half and the analysis passed: the private games at 5.0 % among 20, the naive variants at 100 %, recovery 10 of 10 and 10 of 10 |
| 10 (second review's Lows) | `cargo test -p ferrata-cli --lib` (and `--no-default-features`), `-p ferrata-wallet`; `cli/tests/e2e.sh`; `FERRATA_BOTH_HALVES_SEEDS=1,2,3,4,5,6,7,8 FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh`, then the same `randomised` with `FERRATA_BOTH_HALVES_MINT_CAP_ETH=20`; `e2e/ci/run.sh` ten times; `scripts/privacy/real.sh build` and `dev`; `forge test --match-contract DeployDefaultsTest` (22 Nov 2026, commits `06f6f15e`..`b1e7439c`) | 19 and 17 passed (the new `history_of_counts_refused_plaintext_claims`, `the_anonymity_line_names_each_case` among them), 4 passed; the e2e test passed; the three tests passed, the same tables as run 8; the capped runs passed (30 claims refused by the bucket, 18 claimed later); CI's round trip 10 of 10 (47 to 50 s each, none hung); the privacy suite's dev pass on B1 at chance (0.05), naive 1.0, recovery 10 of 10 and 10 of 10, written under `target/privacy/real/b1/`, `docs/privacy/real/` unchanged; 4 passed |

## The image id change (B1)

| | |
|---|---|
| Bridge program, before | `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` (the live runs of November 2026) |
| Bridge program, since B1 | `0xac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`, 607,608 bytes, binary sha256 `5f47b065…7fd8` ([image-ids.md](../image-ids.md)); the harness build of run 3 (21 Nov, build box) computed the same id |
| What moved it | `nullifier(cid, chain, vault, N)` and `holder(cid, account)` (F.16); `Init` refuses a freeze authority that is the admin (the red team's F1); `SetDepth` (F.12); `AttestListing` / `SetListing` (F.10) |
| Unchanged ids | the attestation program `0x816d1fef…03df`, the checkpoint guests, the release guest: the vault pins the checkpoint and release ids; the bridge's id enters only `configId` |
| Consumers updated | the CLI and its wallet, the crown driver, the fuzz programs, the bridge module's calls, the IDL and its vectors (`9fa33b7e`); `docs/image-ids.md`, `e2e/seqnet/drive.sh` and `run.sh`'s inputs, `e2e/crown/cloud/app.py`, `lez/testnet04/day1.sh` (`2e6a35a6`). Since the review (H1, M3): `docs/image-ids.md` records B1 as `ferrata_bridge_b1` and keeps the deployed program's `ferrata_bridge` row as it was; `scripts/verify-sepolia.py` holds the live vaults to the deployed row and a run of B1 to its own, `scripts/reproduce/compare_ids.py` and `scripts/privacy/real.sh` hold this tree's build to B1's (`b8fd47b1`); the crown runner (`e2e/crown/cloud/drive.py`), the crown driver and `lez/crown/build.sh` refuse a bridge program other than B1's before anything is deployed or sent (`fb5d340f`); `scripts/test_image_ids.py` (CI job `image-ids`) checks all of it offline |
| Live deployments | The four Sepolia v5 vaults and the LEZ programs on our devnets pin configuration ids that commit to `0xccde6171…`: they keep that program and its two known issues ([`bridge.md`](../design/bridge.md) §3). B1 takes effect only with a new vault and a new configuration. This branch's CLI and module address B1's accounts, so operating the old deployments needs the tools at `f13faee2`. Their public re-check works on this tree: `verify-sepolia.py`'s image-id check passes for crown-live-1, -3 and -4 (offline: `scripts/test_image_ids.py` covers -3 and -4; -1 passed the review's offline run), and a rebuild of this tree gives the crown run's other five programs and the vault's two immutables; the deployed bridge program rebuilds from `f13faee2`, where `compare --vault 0x04eE…e44c` gives 6 of 6 (EVIDENCE row 92's command: on this tree 5 of 6 and B1's own record) |

## Rows

### F.16: strictly separated configurations; documented client switching

**Proposed: in part, now only for the document** (the review agrees; the switching text is in the deployer guide, a first draft); every
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

**Proposed: shown** (the review: shown, with the wording fixed below, L12).

| Clause | Shown by |
|---|---|
| Total wrapped supply on LEZ never exceeds the vault's holdings | `cli/tests/both_halves.rs`, `randomised_runs_keep_supply_within_holdings`: eight seeds of 60 random steps (runs 6 and 8, 21 Nov, build box: 141 deposits of the three denominations from three depositors, 44 root imports, 81 claims, 59 burns, one checkpoint per seed, 25 releases); the invariants held at all 496 checks (62 per seed: before the first step, after every step, at the end). After every step: read from the vault on anvil, its ETH balance, `escrowed` and `totalOwed`; read from the zone, the wrapped balances, under the configuration, of the ten accounts the test mints to (the user's eight public accounts, the fee recipient and the relayer: every mint of these runs lands there), which make the supply; and the test's own books, kept from the transactions it sends and checked against those reads: minted, burnt, the deposits not yet minted and the burns not yet released. Checked: balance = `escrowed + totalOwed`; supply = minted − burnt; supply ≤ `escrowed`; supply + unminted + unreleased = `escrowed`. Stand-ins: the vault's verifier is the tests' mock behind RISC Zero's real emergency stop, accepting exactly the journals the guests committed; LEZ reads the vault through synthetic attestation records filled with the vault's real storage (the light client stood in). Limits: plaintext claims to public destinations only (no private claim or burn), one checkpoint per seed, no bucket filled (0 capped claims), ETH only, one user seed. On the vault alone: `vault/test/invariant/` (256 runs × 100 calls, runs 5 and 8) |
| Minting without a valid deposit fails | the runs' forged claims: a tampered witness (another `s`) refused with 249; `bridge_inbound.rs` and `redteam_bridge.rs` (claims under another root, vault or configuration) |
| Minting twice from one deposit fails | within a configuration: the runs' replayed claims, refused (250, or 255 when the bucket is spent first: the program admits the value before it reaches the nullifier); `bridge_refusals.rs`. Since B1 a second configuration of the same vault can mint for the same deposit, once, its own token, which no vault releases (a release binds the vault's `lezConfigId`; the red team's BR-TEN-04, by design): the soundness above is per vault-pinned configuration, and a client takes the configuration from the vault |
| Releasing without a valid burn fails | the runs' releases no guest proved (refused by the verifier), replayed releases and releases redirected to another recipient (refused); `vault/test/Release.t.sol` |

Command: `e2e/both-halves/run.sh` (builds what CI's round trip builds; `FERRATA_BOTH_HALVES_SEEDS=1,2,3,4,5,6,7,8
FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh randomised`). Results: `e2e/both-halves/results/randomised.md`.

Proposed EVIDENCE row: **Supply never exceeds the vault's holdings, over both real halves at once (Sec.6).**
Numbers: eight seeds of 60 random steps: 141 deposits, 81 claims, 59 burns, 25 releases, 93 forgeries refused
(tampered and replayed claims; unproven, replayed and redirected releases); at all 496 checks the vault's balance was
its escrow plus what it owes, the zone's supply (the wrapped balances of the ten accounts the runs mint to) was what
was minted less what was burnt, and supply + unminted deposits + unreleased burns was the vault's escrow; per
vault-pinned configuration (since B1 another configuration of the vault mints only its own, unredeemable token). Where
it ran: anvil (chain id 1) and the in-process harness, guests in dev mode, build box, 21 Nov 2026. Proofs: dev mode
(the vault's verifier is the tests' mock behind RISC Zero's emergency stop, accepting exactly the journals the guests
committed; LEZ reads the vault through synthetic attestation records of its real storage). Limits: plaintext claims to
public destinations, ETH, no bucket filled. Re-check: as above. Details: `e2e/both-halves/results/randomised.md`.

### Priv.5: refused, repeated and capped claims indistinguishable as to deposit

**Proposed: shown** (the review: shown).

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

**Status: in part** (the review of `67b4aa77`, M1: the client counted deposits a plaintext claim had already
published, the measured medians were inflated, and the user guide still said the view was not built). **Fixed since**
(`c7299b2b`, measured again in `3300df8a`): the set leaves out every deposit a plaintext claim has published, under
any configuration of the program, as the user guide defines it; the tables are measured again both ways and labelled
for what they are; the user guide says the view is built. **Shown** at the second review (`47b614ee`). Its L-A since
(`06f6f15e`, measured in run 10, `485b06f7`): a plaintext claim refused on chain stays in its block with its witness, so
the client now leaves its deposit out too (`Ledger::included`, `History::note_refused`) and shows 1 for an own deposit
so published.

| Clause | Shown by |
|---|---|
| Amounts visible on Ethereum do not identify which mint or burn | fixed denominations on both sides (`vault/test/Deposit.t.sol`; 251 for a non-denomination on LEZ); the privacy suite on the program since B1 (run 4; `lez/results/b1/privacy/`): an adversary reading every public byte of 20 users' round trips pairs deposits with private claims, and burns with releases, at chance (0.05); the naive variants at 1.0 |
| State the mechanism and its effect on anonymity-set size | the client's view: `ferrata claim` prints the claim's anonymity set, the deposits of the same token and amount under the root and fee schedule range it names, less those a plaintext claim has published (`ferrata_wallet::claim::anonymity_set`; unit tests `the_anonymity_set_is_the_same_token_and_amount_under_the_root_and_range`, now with two published leaves and one of another vault, and the tool's `plaintext_claims_open_their_leaves_under_every_configuration`), with a warning when the deposit is the only one of its amount (`cli/tests/e2e.rs` asserts the line: its only 1 ETH deposit, set 1; runs 7 and 8). Measured per denomination over both halves (`randomised.md`, run 8; every claim of these runs is plaintext, so the table gives, for each of the 81 claims, the set a private claim of that deposit would have had at that moment): as run, median 2, 2 and 2 for 0.1, 1 and 10 ETH (largest 7, 6, 5), and 27 of the 81 sets are 1; had every earlier claim been private (the default, which publishes no deposit), median 4, 4 and 3 (largest 10, 7, 6) and 10 sets of 1; the plaintext claims made 57 of the 81 sets smaller (run 10 again: the same; none of these claims was refused with a valid witness). With refused claims (`randomised-cap-20eth.md`, run 10: ETH's mint bucket at 20 ETH, 30 claims refused by it, each publishing its deposit, 18 claimed later with set 1): 63 claims, as run medians 3, 2, 2 and 20 sets of 1; all private 4, 4, 2 and 11; unit tests `plaintext_claims_open_their_leaves_under_every_configuration` (a refused claim opens its leaf, spends no nullifier) and `history_of_counts_refused_plaintext_claims`. A fee change splits a set at its first leaf: `fees.md` (6 deposits of 1 ETH: before any claim every deposit's set is 3 under either schedule) |

Proposed EVIDENCE row: **The anonymity set of a claim, shown by the client and measured per denomination (F.8).**
Numbers: over 81 claims of eight randomised runs, the set a private claim of each deposit would have had at that
moment: with every claim plaintext, as the runs made them, median 2 for each of 0.1, 1 and 10 ETH and 27 sets of 1
(the amount alone links the claim, which the client warns of); had every earlier claim been private, the default,
medians 4, 4 and 3 and 10 sets of 1; before any claim, a fee change at leaf 3 of 6 deposits of 1 ETH leaves sets of 3
and 3; the privacy suite on B1 pairs at chance (0.05 among 20). Where it ran: anvil and the in-process harness, build
box, 21 Nov 2026. Details: `e2e/both-halves/results/randomised.md`, `fees.md`; `lez/results/b1/privacy/summary.md`.

### Sec.4: caps bound value at risk; recommended defaults

**Proposed: in part, now only for the document** (the review: in part, agreed). The deployments' default is capped
since B1; the reasoning for its size is in the deployer guide §2 since the review (L3), a first draft.

| Clause | Shown by |
|---|---|
| Caps bound the value at risk in any rolling window | the buckets on both sides (`vault/test/Caps.t.sol`; `bridge_controls.rs`, `bridge_time.rs`; row 138's broken-verifier invariants) |
| A capped default, deployed by default | `vault/script/DeployVault.s.sol` (what `script/deploy.sh`, the crown's Sepolia stage and `scripts/deploy-testnet.sh` send): per token and day 100 times its largest denomination, globally their sum, an immutable release floor of one largest denomination a day (its refill rounded up since L13: 3,473 a second, as the crown driver's); `FERRATA_UNCAPPED=1` for the uncapped testnet deployments of November 2026. This is the ceiling part of the design's recommendation, "the smaller of 10 % of escrow and a ceiling": a new vault has no escrow, so the 10 % term is the admin's to apply as escrow grows (deployer guide §2, "Why the caps are 100 largest denominations a day"). `vault/test/DeployDefaults.t.sol` (runs 5 and 8) builds a vault from exactly what the script sends and fills the day: 99 deposits of 0.1 ETH (each escrowing 0.1002 ETH) pass, the 100th is refused, the bucket is whole a day later; no release cap below the floor; run 8 deployed it step by step on a local anvil, the script's own checks passing. LEZ: the crown driver's configuration is capped likewise (`the_configuration_is_capped_by_default`, runs 4 and 8). Not capped: `vault/script/RoundTrip.s.sol`, the single-script round trip of the earlier testnet runs (`e2e/sepolia-live.sh`, `script/roundtrip.sh`), as its header says (L4) |
| Document the defaults and the reasoning | the deployer guide §2 (this branch updates its row), `docs/design/bridge.md` §13, `docs/trust.md` §2 ("What this means for the bridge"): the loss accepted before a freeze. First draft |

Proposed EVIDENCE row: **Capped by default (Sec.4).** Numbers: the deployment script's vault refuses the deposit past
its ETH bucket (10 ETH a day: 99 deposits of 0.1 ETH and their relay fees), refills it in a day, and keeps every
release cap at or above 300,000,000 value units a day (one 0.1 ETH); the LEZ configuration of the crown driver: 3×10^10
value units a day per token and globally, floor 3×10^8. Where it ran: local EVM; Rust unit test; build box, 21 Nov 2026.
Re-check: `cd vault && forge test --match-contract DeployDefaultsTest`. Details: `vault/test/DeployDefaults.t.sol`.

### F.12: configurable finality depth; changes never invalidate valid claims

**Status: in part** (the review of `67b4aa77`, M2: the deployment script deployed the outbound wait Δ at its testnet
600 s, not the recommended depth). **Fixed since** (`a8e93009`): the script deploys the design's wait for the rule by
default, 3 h 01 min under rule (A) and 1 h under rule (B), and `FERRATA_DELTA` sets another for a test run. **Proposed:
shown, for the next review.**

| Clause | Shown by |
|---|---|
| Inbound depth configurable by the admin per deployment, defaulting to the recommended depth | `lez/harness/tests/bridge_depth.rs` (run 3; `lez/results/b1/bridge-depth.md`): depth 0 (Ethereum finality) at `Init`; `SetDepth` by the freeze authority or an outsider refused (253), above 216,000 slots refused (240); at 65 an import under a record 32 or 64 slots behind the configuration's tip held (242), at 64 applied |
| Outbound depth configurable, defaulting to the recommended one | the vault's wait Δ: `script/DeployVault.s.sol` deploys `deltaFloor` = initial `delta` = 10,860 s under rule (A) and 3,600 s under rule (B) unless `FERRATA_DELTA` names another, reads it back in its checks and records `delta` and `designDelta` (`DeployDefaults.t.sol`, `test_deploymentDefault_waitIsTheDesignsForTheRule`, `test_delta_otherOnlyWhenGiven`; run 8: deployed step by step on a local anvil, read back 3,600, 10,860 and, with `FERRATA_DELTA=600`, 600 with a warning). The admin configures it per deployment upward only (`raiseDelta`, at most 30 days; the immutable floor is deliberate: a lower wait would shorten the window to react to a wrong candidate, deployer guide §2): `Checkpoint.t.sol` (`test_finalize_deltaRaiseAppliesToLaterCandidates`, `test_raiseDelta_onlyUpAndBelowTheCeiling`). The per-token release delay, a hold while a checkpoint can still be revoked, is configurable too (`ReleaseDelay.t.sol`); the scripts deploy its testnet 120 s, not the design's 24 h or more |
| A change never invalidates a valid claim or release | `bridge_depth.rs`: at the maximum depth, claims against roots imported at depths 0 and 64 apply; a Δ raise does not touch a submitted candidate (`test_finalize_deltaRaiseDoesNotAffectASubmittedCandidate`) |

Proposed EVIDENCE row: **Finality depth configurable in both directions, recommended by default (F.12).** Numbers:
inbound, `SetDepth` only by the admin, at most 216,000 slots, default 0 (Ethereum finality); an import held (242) until
the configuration's tip is that many slots past its record; claims under roots imported earlier apply at any depth.
Outbound, the deployment script's vault has Δ = 10,860 s under rule (A) and 3,600 s under rule (B) unless told
otherwise (a candidate's wait is Δ + (t − 1) turns, `docs/design/bridge.md` §10), read back from a deployment on a local anvil; raise-only afterwards. Where it ran: in-process harness, local
EVM and anvil, build box, 21 Nov 2026. Re-check: `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test
bridge_depth`; `cd vault && forge test --match-contract DeployDefaultsTest`. Details: `lez/results/b1/bridge-depth.md`.

### F.10: register and deregister tokens; document registry changes

**Proposed: in part, now only for the document** (the review: in part, agreed; delisting is in the deployer and user
guides, first drafts). The behaviour is shown; on LEZ the delisting is a recorded mirror (`TokenState.delisted`) that
no program path and no client of this tree reads yet (the `ferrata` tool reads the vault's `depositsEnabled`; L1).

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

**Proposed: in part** (unchanged; the review agrees), with a narrower reason: every later act of any party only delays (caps, freezes,
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

**Proposed: in part** (unchanged; the review agrees), with what the tests now show over both halves and what remains a design question
for the owner: within one fee schedule every claim and every release of a denomination pays the same fees, so the fee
singles no user out; across a fee change a claim shows which schedule its deposit falls under (its range), which
splits the anonymity set at the change, by deposit time, as the RFP's timing signal does. The client counts it.

| Clause | Shown by |
|---|---|
| A fee on minting and on burning, in the wrapped token, at a rate the admin configures per deployment | `fee_schedules_over_both_halves` (run 6; `e2e/both-halves/results/fees.md`): the admin announces mint 0.5 %, burn 0.2 % and checkpoint 0.1 % fees on the vault; seven days later they are in force from leaf 3 and attested on LEZ (`ImportFees`; a forged copy a second early refused); each claim mints its deposit's escrow: under schedule 0 the denomination and the relay fee, under schedule 1 the mint fee too, to the fee recipient on LEZ; each release pays 1 ETH less 0.003 ETH, the burn and checkpoint fees credited on the vault (`totalOwed`). The burn fee is taken at the release (the RFP's Fee structure §3 allows "at burn or at release"). `vault/test/Fees.t.sol`; `bridge_vault_storage.rs` |
| The fee value does not distinguish a user's transaction from others | the same test: every claim of 1 ETH under one schedule mints the same fees, every release pays the same, and before any claim a private claim of any deposit of a schedule has the same anonymity set (3 and 3; run 8). The test's claims are plaintext, so each publishes its deposit and the next one of its schedule has one fewer (3, 2, 1), which private claims would not do. Across a change, see above |
| The admin can set either fee to zero | `Fees.t.sol` (`test_release_zeroFeesCreditNothing`; schedule 0 charges no protocol fee); the 1 % ceiling fuzzed |

Proposed EVIDENCE row: **Fees over both halves (F.13).** Numbers: schedule 1 in force from leaf 3, seven days and
1 s after its announcement; claims under it mint 0.005 ETH more, to the fee recipient on LEZ; each release
pays 0.997 ETH, and the vault's `totalOwed` rises by 0.003 ETH a release; before any claim, sets of 3 and 3. Where it ran: anvil and the
in-process harness, guests in dev mode, build box, 21 Nov 2026. Details: `e2e/both-halves/results/fees.md`.

### Sec.8: verification logic cannot be swapped; migration and in-flight positions

**Proposed: in part, now only for the document** (the review agrees; the deployer guide §9, a first draft): the migration was not run
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

**Proposed: in part, now only for the document** (the review agrees; `docs/trust.md` §6, a first draft, says why the failsafe is
stop-only); the stop's behaviour is shown on both halves.

| Clause | Shown by |
|---|---|
| The failsafe stops; it never moves funds | `migration_rehearsal_honours_positions_in_flight`: RISC Zero's emergency stop on vault A's verifier (as a proof of unsoundness would trip it): a proven release and a refund refused, A's escrow untouched and its books still balanced; configuration B's vault goes on. `EmergencyStop.t.sol`; Halmos: nothing released after the stop (row 35); the stop's owner renounced at deployment (rows 41, 91) |
| Why no recovery | `docs/trust.md` §6. First draft |

## For the integrator

- EVIDENCE row 143 names `lez/guests/bridge-next`, which this branch removed (the program is `lez/guests/bridge`
  since B1; the second build's sources are at `f13faee2`, as `lez/results/bridge-next.md` now says).
- EVIDENCE rows 91, 92 and 142 re-check the live runs with `scripts/verify-sepolia.py`, which works on this tree again
  (H1); row 92's `scripts/reproduce-image-ids.sh compare --vault 0x04eE…e44c` gives 6 of 6 at `f13faee2`, and on this
  tree the other five programs and the vault's two immutables equal, with the bridge equal to B1's own record.
- The red team's records on B1 (`lez/results/b1/redteam-2026-11-18/`): BR-TEN-04 stays MISMATCH by design (its
  vector encodes one nullifier namespace per vault, which B1 drops); its note now names B's own
  `nullifier(configId(B), chain, V, N)` (L9). The vector's expectation is the 18 November corpus's and is not changed.
- Every Low of the review is addressed; three by wording rather than code: L1 (the LEZ delisting flag, which no client
  reads, is described as such), L9 (the vector's expectation is kept), L11 (the tools say they speak only B1's accounts;
  they do not check the program's image id).
- The second review's L-D: the merge makes four shared passages wrong, each naming `ccde6171…` "this tree's": EVIDENCE
  row 74 ("re-recorded with this tree's program `ccde6171…`"), the limitations paragraph (`docs/EVIDENCE.md:194`, "this
  tree's build (`0xccde6171…71dd`)") and `docs/traceability.md:30` ("the tables are of this tree's program (image
  `ccde6171…`"): read "the deployed program" (at `f13faee2`) in each. Row 89's re-check (`scripts/privacy/real.sh`) runs
  B1's program on this tree and writes under `target/privacy/real/b1/`; its records reproduce from `f13faee2`
  (`docs/privacy/real/RESULTS.md` now says so: L-C).
