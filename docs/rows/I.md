# Stream I: the soft requirements, proposed statuses

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `rfp-softs`
from `f13faee2`. Box runs: the Modal build box, RISC Zero dev mode, LEZ v0.4.0-rc1's state machine in process (the
harness), nothing sent to any network. Every number below is measured unless it says derived.

## RFP-021

| ID | Proposed | Clause by clause | Evidence (path; command; where, commit) |
|---|---|---|---|
| Soft.1 | shown | *A consumption path that evaluates a predicate and records the resulting verified statement to a public account*: `ferrata_statements`' `Record` sends the attestation program's own `Attest` (predicate and all) and keeps the reply, once, at `statement(key)`; `Read` returns it to any program. *So cost can be amortised when many consumers care about the same Ethereum state*: two consumers use one recorded statement; each use 63,438 cycles against 432,128 for a direct attestation; the recorded path is cheaper from two consumers (derived). *Sharing a single verification core with the direct path, following the pattern in RFP-020*: the recorder is a thin wrapper that calls the shared verifier and writes a canonical account (RFP-020's push aggregator); the test asserts the recorded reply equals the attestation program's own answer and carries the identifier the direct path's consumer keys on. *Document that a consumer reading such an account trusts whoever wrote it*: [`docs/guides/recorded-statements.md`](../guides/recorded-statements.md), "What a consumer of a recorded statement trusts" (and the module comment of `lez/statements-wire`). | `lez/harness/tests/statements.rs`, 2 of 2: `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- --nocapture` (from `lez/`); box, 21 Nov 2026, commit `e90b9249`; record `lez/results/statements.md`. Refusals: false predicate (the attestation program's 130), wrong actor 62, second `Record` 65, `Use` before `Record` 66, wrong instance or chain 67, absent statement 68, second use 69. |
| Soft.5 | shown | *Propose a mechanism that rewards whoever advances the consensus state*: `ferrata_rebate` pays a fixed rebate from a pool anyone funds (the bridge's fee recipient by policy) to whoever lands an advance through `Land`, which sends the update's own `Finish` and pays only if the finalized slot advanced in that transaction. *So that liveness does not rest solely on interested parties choosing to spend the gas*: the rebate is paid out of the pool's own account (`custody_transfer`), not by the updater. *Must preserve the permissionless property of Functionality #11; may make operating attractive, but not exclusive*: anyone may call `Land` naming any payee; `Finish` sent directly still applies the update (tested: nobody paid, nothing refused); an empty pool blocks nothing. Design and its limit (the last step's sender is paid): [`docs/guides/operator-rebate.md`](../guides/operator-rebate.md). | `lez/harness/tests/rebate.rs`, 2 of 2: `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture`; box, 21 Nov 2026, commit `53c281de`; record `lez/results/rebate.md`. On five real mainnet updates (periods 1899–1903): operator 1 paid 1,000,000 for the advance 15,556,608 → 15,564,800; a direct update paid nobody; operator 2 paid for its own advance only; `Land` 3,842,337 execution gas against 3,736,661 for a direct `Finish`. |

## RFP-022

| ID | Proposed | Clause by clause | Evidence |
|---|---|---|---|
| Soft.1 | in part | *Record transferred values on the LEZ side as value commitments*: **not implemented.** *Designed so this can be adopted later without redeploying the vault or resetting accumulated anonymity; document the intended migration path even if it is not implemented*: [`docs/design/committed-amounts.md`](../design/committed-amounts.md). Inbound: adoptable without the vault (each deposit leaf binds its amount; `N` does not depend on the LEZ program, so a second LEZ program claims the same tree from leaf 0 and refuses any `N` the retired program spent: no reset). Outbound: committed burns change the burn leaf, hence the release guest, whose image id the vault pins, so a new vault: the part of the clause the deployed vault does not meet, stated with what would have avoided it. | The two properties the inbound path rests on are tested: `bridge/crates/ferrata-bridge-core/tests/inbound.rs` (every single-field change of a claim fails), `tests/vectors.rs`. Commit `ff64e72f`. |
| Soft.2 | in part | *Optional viewing keys allowing a user to voluntarily disclose their own bridge activity to a chosen third party*: a disclosure document from the user's own keys: `kView` for the burns (existing scan), per deposit the declared destination and, once its nullifier is spent, the deposit secret `s` and `N`; the third party verifies it against public chain data alone. *Without weakening privacy for anyone else*: tested (nothing of another user's is learnt). *Without any protocol-level disclosure capability*: wallet-side only; no guest, program or vault change. Why `s` only after the claim (before it, `s` would let the third party submit the claim and take the relay fee): [`docs/privacy/viewing-keys.md`](../privacy/viewing-keys.md). **Why in part:** the tests build the public view with the core's own functions; the disclosure has not been verified against a recorded round trip's chain data. Next: replay it on the CI round trip (fixed test seed, anvil's deposit events, the harness's nullifiers and burn notes). | `bridge/crates/ferrata-disclose/tests/disclose.rs`, 5 of 5, clippy clean: `cd bridge && cargo test -p ferrata-disclose`; laptop, 21 Nov 2026, commit `b5c54fda`. |
| Soft.5 | shown (with a stand-in EIP-1193 wallet; see the note) | *A supplementary approach compatible with common Ethereum wallet software, such as a web application*: [`web/depositor/index.html`](../../web/depositor/index.html), one static page (no server, no build step, no dependency) that makes a deposit from the account in any wallet injecting `window.ethereum` (EIP-1193), ETH or an ERC-20 with its exact `approve`; the user's funds need not move to the Logos Ethereum module first. *In addition to the Logos modules*: deposits only; claims, burns, releases and recovery stay in the `ferrata` tool or Basecamp; the page takes the commitment `C` and index `j` from the tool's dry run, so the seed and the LEZ destination never enter the browser. *Document its security trade-offs (malicious or compromised extensions, injected scripts, phishing front ends)*: [`web/depositor/README.md`](../../web/depositor/README.md), "Security trade-offs" (each threat, what it could do, what limits it, what the design gives up against the module). **Note:** the test's wallet is a stand-in EIP-1193 provider forwarding to anvil, not a wallet extension such as MetaMask; if the integrator requires a real extension, the row is in part. | `bash web/depositor/test/run.sh`: headless Chromium (Python Playwright), the page from `file://`, a local anvil (chain id 11155111) with the vault and a registered ERC-20; 9 case groups, 53 checks, all passed: ETH and ERC-20 deposits checked on chain (event, `depositCount`, balances, allowance exactly the total), refusals for wrong chain, other account, stale index, zero `C`, unregistered token, a wallet rejection (4001, nothing retried), and the page's own traffic (nothing but its `file://` URL; only allowlisted wallet methods). Its Python `s` and `C` reproduce `bridge/vectors/inbound-v2.json`. Laptop, 21 Nov 2026, commit `57ba5a7e`; record `web/depositor/results/anvil-2026-11-21.txt`. |

## Proposed EVIDENCE rows

1. **Recorded statements (RFP-021 Soft.1).** `ferrata_statements` records the attestation program's own `Attest`
   reply once at `statement(key)` (the key is the request without its proofs); `ferrata_statements_reader` uses one
   statement of it once per trust configuration (recorder, attestation program, instance, chain, namespace). Stage 4
   chain, mainnet block 26,384,851, WETH balance of Uniswap v2's USDC/WETH pair ≥ 1 WETH: the recorded reply equals the
   program's own answer and its storage statement carries the identifier `ferrata_holdings` keys on; two consumers use
   it; seven refusals change nothing. Cycles: direct path 432,128, `Record` 393,331, one `Use` 63,438. 2 of 2. Box,
   21 Nov 2026, dev mode. `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements --
   --nocapture`. `lez/results/statements.md`, `docs/guides/recorded-statements.md`.
2. **The operator rebate (RFP-021 Soft.5).** `ferrata_rebate`: a pool funded by an ordinary native transfer pays a
   fixed rebate to whoever lands an advance through `Land` (`Finish` relayed, the instance read before and after,
   paid out of the pool's own account). Five real mainnet updates: no advance, no pay; operator 1 paid 1,000,000 for
   15,556,608 → 15,564,800; a direct update pays nobody and is not refused; operator 2 paid for its own advance only;
   an empty pool blocks nothing; half a rebate pays half; `Configure` twice 75, no pool 76, no job refused. `Land`
   3,842,337 execution gas, a direct `Finish` 3,736,661. 2 of 2. Box, 21 Nov 2026, dev mode. `RISC0_DEV_MODE=1 cargo
   test -p harness --features rebate --locked --test rebate -- --nocapture`. `lez/results/rebate.md`,
   `docs/guides/operator-rebate.md`.
3. **Voluntary disclosure (RFP-022 Soft.2).** `bridge/crates/ferrata-disclose`: Alice's document lists her deposits
   with destinations, the claim of each claimed one, and her burns with recipients; nothing of Bob's; six kinds of
   tampering refused by name; no `kAuth`, seed or `sOut` in it; JSON round trip. 5 of 5. Laptop, 21 Nov 2026. `cd bridge
   && cargo test -p ferrata-disclose`. `docs/privacy/viewing-keys.md`.
4. **A web depositor for common Ethereum wallets (RFP-022 Soft.5).** `web/depositor/index.html`, a static page using
   the user's injected wallet (EIP-1193): ticket (`C`, `j`) from the Logos tool's dry run, checks before sending,
   the tool's pricing, exact calldata shown, exact `approve`, the `Deposit` event decoded; CSP `default-src 'none'`.
   Headless Chromium against anvil with a stand-in EIP-1193 wallet: 9 case groups, 53 checks passed. Laptop, 21 Nov
   2026. `bash web/depositor/test/run.sh`. `web/depositor/README.md` (security trade-offs),
   `web/depositor/results/anvil-2026-11-21.txt`.

## Files outside this stream's own

New outside `lez/` and `bridge/`: `web/depositor/`, `vault/script/WebDepositorLocal.s.sol` (a local deployment for the
page's test; the vault unchanged), `docs/guides/recorded-statements.md`, `docs/guides/operator-rebate.md`,
`docs/design/committed-amounts.md`, `docs/privacy/viewing-keys.md`. Changed, minimally: `lez/Cargo.toml` (four workspace members), `lez/harness/Cargo.toml` (optional dependencies and the
features `statements` and `rebate`, off by default), `lez/harness/src/lib.rs` (`Instance::update_in_with`, the job's
last step as a closure; `update_in` calls it with `Finish`, unchanged), `lez/harness/src/scenarios.rs`
(`World::apply_with`, the same for `apply`), `bridge/Cargo.toml` (one member), `bridge/Cargo.lock`, `lez/Cargo.lock`.
No deployed guest or program changed; no image id of `docs/image-ids.md` moved. The new programs (`ferrata_statements`,
`ferrata_statements_reader`, `ferrata_rebate`) are not deployed anywhere; their exit codes (59–69, 70–76) are apart
from every other program's (attestation 1xx and 200–229, holdings 230–239, bridge 240–255).
