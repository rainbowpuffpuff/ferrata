# Internal security review of the Ethereum half, 27 November 2026

**This is not an external audit.** It is the project's own review, made with coding agents, of code the same project
wrote. RFP-022 Security #1 (a third-party audit of both contracts, its report published) and RFP-022 S.8 stay
**not yet**; the scope an auditor would quote is [`audit-scope.md`](audit-scope.md). Nothing here is evidence that
the code is safe: it is a list of what was looked at, what was found, and what was changed.

## 1. Scope, method and result

**Scope.** The tree at `50b396a8` (public main, 27 November 2026): `vault/src/FerrataVault.sol` and
`vault/src/FerrataRegistry.sol` (the linked library), their use of RISC Zero's `RiscZeroGroth16Verifier` 3.1.0 and
`RiscZeroVerifierEmergencyStop` (risc0-ethereum v3.1.0, `vault/lib.lock`) and of OpenZeppelin 5.4.0; caps, freezes,
retirement and sunset, refunds, fees, and token handling (fee-on-transfer, rebasing, ERC-777 hooks, non-standard
returns, pause and blocklist); the deployment and admin scripts under `vault/script/`; the EVM calls of the relayer
(`cli/src/relayer.rs`, `relayerd.rs`, `eth.rs`) and the monitor (`monitor/src/`). The LEZ programs are out of scope
except where an Ethereum-side outcome depends on them (section 5).

**Method.** The ethskills audit method (`audit`, `security`, `testing`, `standards`): the pinned checklists of
austintgriffith/evm-audit-skills at `ffe4b670e78e1945bcf275f79d4b7b0481bcff35` (general, precision-math, access-control,
dos, erc20, bridges, chain-specific, signatures), one specialist per group of domains reading the code independently
(three Claude subagents: vault logic; token handling and cross-chain binding; EVM clients and deployment scripts),
the reviewer's own line-by-line reading of both contracts, and every claimed finding confirmed against the code and,
where it could be, by a test that fails on `50b396a8`. Earlier reviews were not re-reported: the review of
11 November 2026 and its fixes ([`../reviews/security/`](../reviews/security/)), the vault review of 16 November
(F-1 … F-9) and the red team of 18 November. Line numbers are at this branch's tip unless a commit is named
(`vault/src/FerrataVault.sol` is three lines longer than at `50b396a8` after line 690, from S-1's fix).

**Result.** No critical or high finding. Four findings fixed and two in part, each fix with a test that fails on
`50b396a8` and passes after it: one medium (the monitor's nonce), five low (the vault, the deployment default, the
verifier check, and in part the monitor's command environment and `admin.sh`). Three
fee-code findings go to the fee rewrite (ADR 0048, branch `f13-lez`) unchanged. The rest are documented below with
the reason they were not fixed tonight.

| ID | Severity | Where | Finding | Status |
|---|---|---|---|---|
| C-1 | Medium | monitor | One failed send leaves a nonce gap: every later cancel, pause or revoke waits behind it for good | **fixed** (`cb6f0120`) |
| C-2 | Medium | monitor | A candidate read as gone or stale once, from one unpinned RPC read, is never checked again | open |
| C-3 | Medium | relayer | No gas ceiling or profitability check: a recipient that burns gas costs the relayer about ten times a release | open |
| S-1 | Low | vault | A token hook can retire the vault or finalise a sunset checkpoint inside `deposit`, which then lands anyway | **fixed** (`ad2e91d5`) |
| S-2 | Low | deploy script | The default release floor is below the largest exit: an admin can set caps that refuse a 10,000-token release or a 0.1 ETH refund for good | **fixed** (`1da2fd28`) |
| S-3 | Low | verifier check | `check-verifier.sh` trusted `SELECTOR()`: a fake verifier, a fake stop, an owned or tripped stop all passed | **fixed** (`68b3e124`) |
| F-1 | Low | vault, fee code | A cancelled or revoked candidate's public seal can be resubmitted by anyone, who then takes its checkpoint fees | for the fee rewrite |
| V-1 | Low | vault | A token the admin registers with a large `unitWeight` can use the shared global release bucket up, floor or not | documented |
| C-4 | Low | monitor | The watcher key in `--key-env` is inherited by every `sh -c` child; node and alert commands have no timeout | **key fixed** (`b1be13ab`); timeouts open |
| C-5 | Low | `deploy.sh` | A Hoodi or Holesky RPC is routed through the Sepolia proxy | open |
| C-6 | Low | `admin.sh` | No chain or code check (an action on an empty address reports success); `--password` on argv; global caps shown as ETH | **code check fixed** (`17c95788`); the rest open |
| C-7 | Low | relayer | No replacement of a stuck transaction; a job is booked at the first receipt | open |
| C-8 | Low | monitor, relayer, `admin.sh` | The chain id is not pinned | open |
| V-2 … V-6, F-2, F-3, I-* | Info | various | Section 4 | documented |

Severity: by demonstrated impact and exploitability, as the audit skill ranks it. A finding that needs the admin to
register a token of a given kind, or a deployer to misconfigure, is ranked by what follows if it does.

## 2. Fixed

### C-1 (Medium) — one failed send leaves the watcher a nonce gap for good

- **Where.** `monitor/src/chain.rs:155-161` at `50b396a8`: `ProviderBuilder::new().wallet(...)`. alloy-provider
  1.8.3's recommended fillers use `CachedNonceManager` (`fillers/nonce.rs`), which counts the nonce up at every fill,
  failed sends included, and never asks the node again; the nonce is filled concurrently with the gas estimate.
- **Path.** The monitor (long-running, one provider) tries a `cancelCandidate`; the send fails after its nonce was
  filled: a 429 or timeout at the estimate or the send, a candidate another watcher cancelled between the
  simulation and the estimate, no funds. Every later cancel, pause or revoke goes out one number too high and waits
  in the node's queue behind a number nothing fills; each times out after 180 s, an alert every five minutes. A
  transaction the same key sends elsewhere (the Logos module's route, an operator) makes the next one reuse a number.
- **Impact.** Under rule (B) a wrong candidate is final after Δ unless a watcher cancels it; the watcher that can no
  longer send is a watcher missing. Safety then rests on the other watchers and on `revokeFrom` inside the window.
- **Fix.** `signing_provider` builds the provider without the recommended fillers: gas estimation, the chain id, and
  `with_simple_nonce_management()` (the node's pending nonce at every send).
- **Test.** `monitor/tests/nonce.rs` (anvil): a send from an unfunded key fails, the key is funded, the next send must
  take nonce 0 and be mined; then the same key sends through another provider and the monitor's next send must follow
  it. On `50b396a8` the second send times out behind the gap (`TxWatcher(Timeout)`); now it passes, and the monitor's
  own suite (27 tests) still does.

### S-1 (Low) — a token hook lands a deposit in a retired vault

- **Where.** `vault/src/FerrataVault.sol:679-693`: `_checkDeposit` checks `retired || finalSunset`, then
  `safeTransferFrom` calls the token, then `_recordDeposit` appends the leaf.
- **Preconditions.** The admin registered a token that calls a hook of the sender inside `transferFrom` (ERC-777's
  `tokensToSend`) and either keyless retirement is open or a sunset candidate's wait is over (rule (B)) or its beacon
  recorded (rule (A)).
- **Path.** The depositor's hook calls the permissionless `retire()` or `finalizeAttested(c)` of a sunset checkpoint;
  neither is `nonReentrant`; the deposit then completes and a leaf is appended to the tree of a retired vault, or
  after the final sunset checkpoint that review L-3 made deposits stop at.
- **Impact.** The vault's statement that deposits revert forever after `retire()` (and after a final sunset) does not
  hold for such a token. No funds are lost: the leaf lies above the refund boundary and is refundable, and LEZ, after
  `Sunset`, imports nothing; a refund path computed against the earlier root must be recomputed.
- **Fix.** The check is made again after the pull (`FerrataVault.sol:691-693`), 63 bytes of the 153 left under
  EIP-170 (90 left; the fee rewrite on `f13-lez` changes the size too, so the merge must be measured again).
- **Test.** `vault/test/InternalReview.t.sol`: `test_S1_aTokenHookCannotLandADepositAfterRetirement`,
  `test_S1_aTokenHookCannotLandADepositAfterASunsetCheckpoint` (both "next call did not revert" on `50b396a8`), and a
  control in which the hook changes nothing. Two reviewers checked that no other input `_checkDeposit` reads can be
  changed from an unprivileged hook.

### S-2 (Low) — the default release floor lets an admin refuse the largest exits for good

- **Where.** `vault/script/DeployVault.s.sol:277-285` at `50b396a8`: the default immutable floor was one largest
  denomination of the token whose largest is worth least (0.1 ETH, 300,000,000 value units a day).
- **Path.** The floor exists so that no admin can throttle exits below it (the script's header). Caps the floor allowed
  were below the largest single exit: `setGlobalCap(Release, 300,000,000, …)` left a 10,000-token release
  (10,000,000,000 value units) refused for good, and `setTokenCap(ETH, Release, 0.1 ether, …)` (the default test's own
  call) a refund of a 0.1 ETH leaf, which returns its relay fee too (0.1002 ETH).
- **Impact.** A compromised or hostile admin key could stop the largest releases and refunds of a default deployment,
  within the rules the floor was meant to enforce. Not theft. The live Sepolia 3.1 vault was deployed with the old
  default; its floor is immutable.
- **Fix.** The default is the largest single exit over both tokens, a largest leaf's denomination plus its relay fee
  (10,025,000,000). `releaseFloorDailyValue` still overrides it.
- **Test.** `vault/test/DeployDefaults.t.sol` `test_S2_atTheLowestCapsTheFloorAllowsEveryExitStillPasses`: the admin
  sets every release bucket as low as the floor allows, each token's largest exit must fit, and a 10,000-token
  release goes through (on `50b396a8`: 0.1002 ETH > 0.1 ETH).

### S-3 (Low) — the verifier check trusted getters any contract can answer

- **Where.** `vault/script/check-verifier.sh` read `vault.verifier()`, the stop's `verifier()` and `SELECTOR()`, and
  passed on 0xe71e6620. The vault's constructor checks only that the verifier has code and answers `paused()` false.
  `DeployVault._check` compared the Groth16 verifier's code but only the stop's getters.
- **Path.** A deployer (a trust root already) wires a vault to a contract answering `SELECTOR()` 0xe71e6620 and
  `VERSION()` "3.1.0" that accepts every seal, behind RISC Zero's real stop; or to a contract that answers like the
  stop; or keeps the stop's owner, who can trip it and end releases and refunds for good. The script, which
  `docs/cli-coverage.md` offers third parties and the online check `sepolia-verifiers` runs, says "ok".
- **Impact.** An assurance gap, not an exploit of a deployed vault: the live 3.1 vault went through DeployVault's
  code-hash check. A vault "verified" this way could lose its whole escrow to forged proofs, up to the caps.
- **Fix.** For 3.1.0 the script requires the Groth16 verifier's code hash and the stop's code hash, its one
  immutable (the wrapped verifier's address) masked, to equal `vault/script/params/verifier-31.json`, the stop to
  have no owner and not be tripped. `vault/test/VerifierCode.t.sol` holds both pins to vault/lib.lock's build (the
  Groth16 hash equals the one the recorded DeployVault runs report). `DeployVault._checkStop` compares the stop's
  code with RISC Zero's built in simulation. The 3.0.0 branch (the four November vaults, capped at zero) is unchanged.
- **Test.** `vault/script/test-check-verifier-local.sh` (anvil; CI `vault`) deploys `script/CheckVerifierFixture.s.sol`:
  the honest wiring passes; a fake verifier, a fake stop, an owned stop and a tripped stop fail, each for its own
  reason. The checker of `50b396a8` passed all five. `DeployVaultChecks.t.sol` `test_S3_theStopIsCheckedByItsCode`.

## 3. Not fixed, for the fee rewrite (ADR 0048, `f13-lez`)

Read on `f13-lez` at `26cec72e`: none is changed there; the checkpoint fee is still credited to the prover the
candidate recorded (`FerrataRegistry.releaseFeesAndPayouts`).

- **F-1 (Low).** `submitCandidate` binds no sender and refuses only a live candidate (`FerrataVault.sol:764`). After
  `cancelCandidate` (a watcher or the freeze authority), `revokeFrom` (the freeze authority) or a candidate gone
  stale, the seal is public in the original transaction's calldata and anyone can resubmit the same candidate and
  become its `prover`, credited the checkpoint fee of every release against that root. For a watcher it is
  deterministic: cancel and resubmit in one bundle. Confirmed with a throwaway test, not committed because it asserts
  the defect (a watcher cancels the prover's rule (B) candidate, resubmits its seal, and after one release its `owed`
  is the checkpoint fee and the prover's 0). Impact: checkpoint fees (each at most 1% of a release) go to someone
  else, and watchers gain a motive to cancel. The known open item (copying a pending submission from the mempool,
  `vault/README.md`) needs a mempool; this needs none. Fix: the prover's address in the checkpoint journal (a new
  checkpoint program); a vault-only stopgap is a write-once `firstProver[digest]` set at the first verified submission.
- **F-2 (Info).** A prover that submits through a public router (Multicall3's `aggregate`) makes the router the prover;
  anyone can then claim its fees through the router. Document: submit from one's own account.
- **F-3 (known).** Outbound fees are those in force at release (review L-2, not fixed by design).

## 4. Not fixed, documented

- **C-2 (Medium), monitor.** `monitor/src/daemon.rs:267-281`: one unpinned `candidates()` or `finalState()` read marks a
  candidate Gone or Stale for good, and only Pending candidates are read again; the log fields other than
  `submittedAt` and `deltaAtSubmit` are not checked against the vault's stored `journalDigest` (the relayer does,
  `relayer.rs:365-372`); one `eth_getLogs` answer moves the cursor. Behind a load-balanced RPC with a lagging backend,
  or a node that answers logs short, a wrong candidate is missed with no alert. Fix: pin each poll's reads to its
  block, re-check Gone and Stale until the deadline, compare the journal digest, rescan a sliding window. Not done
  tonight: a change of the daemon's state machine and its anvil rehearsal.
- **C-3 (Medium), relayer.** `cli/src/relayerd.rs:567-572`: gas limit the estimate plus 30% and 20,000, no ceiling, no
  fee ceiling, no comparison of cost with the relay fee, no check of the recipient's code. A release whose recipient
  (bound in the proof) burns all the gas it is sent costs about 3.9M gas instead of about 0.4M, and the burner
  recovers its payout through `owed`. Fix: `--max-gas` and `--max-fee-per-gas`, a profitability check, refusing
  recipients with code unless opted in. Not done tonight: the CLI's build.
- **V-1 (Low), vault.** The release floor holds against the admin's caps, not its registrations: a token the admin
  registers with a large `unitWeight` and releases itself consumes the shared global bucket, the floor included
  (`FerrataRegistry.sol:68`, `:448`; `FerrataVault.sol:1486-1497`). Needs a malicious admin and its own token on LEZ.
  Fix needs a design decision (per-token reservations of the floor, or a weight bound against it); written into
  `vault/README.md`'s note on the floor.
- **C-4 (Low), monitor; the key fixed, the timeouts open.** `monitor/src/cli.rs:114-116` read `--key-env` and left
  the variable set; `alert.rs` and `node.rs` ran `sh -c` with the environment inherited, so an alert or node command
  saw the watcher key. Fixed: every command runs through `alert::shell`, without the variables `cli::signer` hid;
  `monitor/tests/child_env.rs` fails on `50b396a8` (the alert command saw the key) and passes now, a control variable
  still reaching the commands. Open: neither command has a timeout, so a hung one stops the poll loop without an
  alert.
- **C-5 (Low), `vault/script/deploy.sh:52-59`.** The chain id is read from the RPC but the network proxy is always
  `pubnet_up sepolia`, so a Hoodi or Holesky deployment would be routed to Sepolia's endpoints (by reading
  `scripts/pubnet.py`; not run). Fix: map the chain id to its network, refuse others.
- **C-6 (Low), `vault/script/admin.sh`; the code check fixed, the rest open.** No code check before an action: the
  dry run is a `cast call`, which an address without code answers with nothing, printed "OK (no return value)", and
  `--send` reported success. Fixed: `run_action` refuses an address without code before the dry run; the anvil
  scenario's new case (run by `vault/script/test-admin.sh`, CI `vault`) exits 0 on `50b396a8` and 1 now. Open: no
  chain id is pinned; `--password` is passed on cast's command line; global caps are parsed and shown as
  18-decimal ETH although they are value units. Fix: require code at `--vault`, a chain id, a password file, raw value
  units.
- **C-7 (Low), relayer.** A stuck transaction stalls every duty (no replacement by fee); a job is booked at its first
  receipt, so a reorg can leave a nonce gap; a mined revert is final even when transient. **C-8 (Low).** The monitor
  and `admin.sh` take no chain id, the CLI and relayer optionally; state directories are not tied to a chain.
- **V-2 (Info).** The registered `decimals` is never compared with the token's `decimals()`; the test base registers
  6-decimal USDC and USDT as 18 and every suite accepts it. Amounts are unaffected (the CLI reads `decimals()`), but
  the write-once registry entry LEZ attests carries a wrong value. Not fixed: it changes every registration fixture
  and the registry vectors the LEZ side shares, which the fee rewrite also changes.
- **V-3 (Info).** A release whose recipient or relayer is the vault (or the token) is accepted and its payout strands:
  ETH in `owed[vault]`, an ERC-20 as surplus. The journal binds the recipient, so only the burn-note builders can
  refuse it.
- **V-4 (Info).** While an issuer pauses a token or blocklists a payee, a relayed release completes with the payout
  credited to `owed`, which only the payee can claim, by a funded transaction of its own. A `claimOwedFor(payee)` that
  pays only the payee would let a relayer finish delivery; no room under EIP-170 tonight.
- **V-5 (Info).** A token that rebases down, or charges its transfer fee to the sender on top, leaves the books above the
  balance silently; the last payees get `owed` credits that cannot be paid. Rebasing tokens are unsupported
  (`vault/README.md`). A monitor alert on `balanceOf < escrowed + totalOwed` would show it.
- **V-6 (Info).** Payouts forward all gas (`_trySendEth`, OpenZeppelin's `trySafeTransfer`); with C-3 this is the
  relayer's cost, not the vault's.
- **Clients, info.** The relayer re-scans all logs from `--from-block` every round; the monitor fails open when blind;
  `deploy-testnet.sh` writes the RPC URL, which may hold an API key, into a tracked log path; the CLI and monitor send
  with alloy's unpadded estimate; an ERC-20 deposit has no maximum if a fee change comes into force while it is
  pending (review I-3, known).

## 5. Security #9, LEZ's receipt check, and what the Ethereum side shows

Not a finding of this review but its third task: [`lez-receipt-unsound.md`](lez-receipt-unsound.md) writes, position
by position, what happens if LEZ's check of private-transaction receipts is unsound, and why no failsafe meets
Security #9 there; `vault/test/LezReceiptUnsound.t.sol` (8 tests) shows the Ethereum side. Reading the LEZ bridge
program for it found one more reach of that premise, now in [`../trust.md`](../trust.md) §5: `config(cid)` authorizes
the role messages (`Sunset`, `Freeze`, `SetCap`, …) by the sender actor, which a replayed call can name.

## 6. Checked and found sound

- **Journals and verifier.** The 17-word checkpoint journal and the 11-word release journal bind the domain, the chain
  id read at call time, the vault and the configuration; every word of both, flipped alone, is refused by RISC Zero's
  verifier 3.1.0 on the real seals of two LEZ v0.4.0-rc3 runs (`vault/test/real/Rc3JournalWords.t.sol`, four tests on each). Image ids
  immutable and distinct; rules kept apart by image and entry point; Groth16 malleability irrelevant (nothing is keyed
  on a seal); the selectors 0x73c457ba and 0xe71e6620 re-derived from the verifying keys (`verify r8-risc0-selectors`).
- **Replay and roots.** Nullifiers global, set before any external call, never cleared; burn roots accepted only by a
  finalisation, re-pointed after a revocation; a candidate live only in its epoch (review L-1 holds: the epoch strictly
  increases).
- **Accounting.** `escrowed` and `totalOwed` against custody; checks, effects, then payouts; failed payouts credited;
  no zero-amount transfer; refunds pay the leaf's depositor; the earlier fixes M-1, M-2, L-1, L-3 and I-7 re-checked.
- **Tokens.** Fee-on-transfer refused at deposit (exact delta); no-return tokens through SafeERC20; returns-false
  credited; ERC-777 sender hooks: S-1; no ERC-1820 registration, so ERC-777 `send` to the vault reverts; ERC-677 and
  ERC-1363 callbacks hit `fallback`.
- **Access control.** Every state change is the admin's, the freeze authority's, a watcher's or deliberately anyone's;
  no admin path moves escrow; roles distinct at all three entry points; timelocks as specified.
- **Precision.** Multiplication before division; value units never round to zero; floor and ceilings round the strict
  way; no overflow at the registry's bounds.
- **Deployment.** Chain allowlist; anvil accounts refused off anvil; the Groth16 verifier's code checked; the stop
  renounced before registration; the library's code checked.
- **Clients.** The CLI's deposit journal and same-nonce resend; the relayer's job checks, pinned reads and in-flight
  journal; the monitor's simulation before sending and its deadline including (t − 1) turns.

## 7. The three searches (testing skill)

- **Fuzz** over owner-settable value math: caps, fees and delays (`Caps.t.sol`, `Fees.t.sol`, `ReleaseDelay.t.sol`,
  `Admin.t.sol`, `Roles.t.sol`, `Merkle.t.sol`, `bound()` throughout).
- **Invariant** tying accounting to custody: `VaultInvariants` (256 × 100, both rules), `BrokenVerifierInvariants`, and
  since this review `EntitlementInvariants` (later activity only delays an outstanding release, owed balance or refund:
  4 invariants, 128 × 64, 0 reverts; mutations of `_checkRelease` make it fail: two run by its author, refusing a
  delisted token and accepting only the newest root, and one by this review, refusing releases once the tree holds
  more than ten leaves, which no unit test reaches).
- **Fork** against the real deployment: the Sepolia-fork suites of row 231 (RISC Zero's real verifier and stop) and the
  real-seal suites. Not run tonight: a fork against real USDC and USDT (pause, blocklist), and Slither or Aderyn (not
  installed on this machine; their last runs are EVIDENCE rows 35 and 54).

## 8. Reproducing

```bash
cd vault && forge test --match-path 'test/{InternalReview,LezReceiptUnsound,DeployDefaults,DeployVaultChecks,VerifierCode}.t.sol'
cd vault && forge test --match-path test/invariant/EntitlementInvariants.t.sol
cd vault && forge test --match-path test/real/Rc3JournalWords.t.sol
bash vault/script/test-check-verifier-local.sh          # anvil
bash vault/script/test-admin.sh                          # anvil; C-6
cd monitor && cargo test --locked --test nonce --test child_env   # nonce: anvil
python3 -I scripts/verify.py r8-risc0-selectors r8-phase-split && python3 -I scripts/test_verify_sec.py
just security-review                                     # the first, the selectors and their tamper tests
```
