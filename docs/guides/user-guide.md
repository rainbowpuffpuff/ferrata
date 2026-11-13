# Using the Ferrata bridge: deposit, claim, burn, release, recover

**User guide, with the developer integration journey (RFP-022 U.5–U.8, R.12, Privacy #1–#7). First
draft, 11 November 2026.** For someone moving ETH or a registered ERC-20 from Ethereum to LEZ and
back without linking the two sides, and for a developer wiring those flows into a Logos module. It
says what each step does, what it reveals, what the defaults are, how to recover everything from
the wallet seed, and what is not built yet. Every statement cites its source file; paths are
relative to the repository root.

**Status.** Nothing here is deployed for public use. The vault (FerrataVault v5) and the LEZ bridge
program are prototypes, not audited ([`vault/README.md`](../../vault/README.md);
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md)). The user's steps exist as a command-line tool,
`ferrata` ([`cli/README.md`](../../cli/README.md); prototype, run only against a local chain or a test
deployment), and as a mini-app on a devnet ([`modules/ferrata_bridge_ui`](../../modules/ferrata_bridge_ui/README.md));
there is no wallet integration yet. The steps below were exercised by tests, harnesses and scripts, and each
section says where. Labels:
**measured** (a run in the repository, with where it ran), **derived**, **estimated**, **not built**
([`docs/design/bridge.md`](../design/bridge.md), opening section).

## 1. The round trip at a glance

```
 Ethereum                          LEZ v0.4
 1 deposit(token, denomination, C) ─► 2 ImportRoot (anyone; daemons)
                                      3 claim: prove your leaf under the root, open C, spend N; mint
                                      4 hold the wrapped token (private or public account)
 6 checkpoint (anyone; provers) ◄──── 5 burn one denomination, publish a burn note
 7 release(proof) after the wait and the release delay
```

You act at steps 1, 3, 5 and 7. Steps 2 and 6 are done by anyone (importers and checkpoint provers)
and need nothing from you ([`bridge.md`](../design/bridge.md) §2, §5.4, §8).

**With the `ferrata` tool, on the defaults** ([`cli/README.md`](../../cli/README.md) has every option and
output). Every command that sends is a dry run until `--send --yes`: it prints what it would send, what
becomes public on each chain with the exposure of your choices, and what it costs. The defaults are the
private ones; each weaker choice needs its own option and, for a public path, an acknowledgement.

```sh
export FERRATA_SEED=seed.txt FERRATA_ETH_RPC=… FERRATA_VAULT=0x… FERRATA_CHAIN_ID=…    # endpoints: flags or variables
export FERRATA_SEQUENCER=… FERRATA_LEZ_WALLET=lez-wallet FERRATA_LEZ_PASSWORD_FILE=… FERRATA_BRIDGE_ACCOUNT=0x…
ferrata deposit --denomination 1 --keystore me.json --password-file me.pass --send --yes   # 1: to private /0
ferrata claim --bridge-program bridge.bin --send --yes      # 3: private; waits for the deposit's claim delay
ferrata burn --amount 1 --recipient 0xFresh… --bridge-program bridge.bin --send --yes     # 5: from private /0
ferrata release --seal seal.hex --job-out job.json          # 7: newest root past its delay; any relayer sends it
#   (or --prove --program release.bin: a succinct receipt in the job, which the relayer wraps to Groth16)
ferrata recover                                             # everything, from the seed alone
```

This sequence on these defaults is the tool's end-to-end test, on a local anvil chain and the LEZ harness
(the private claim and burn proven in RISC Zero dev mode with the wallet's own keys; measured on the build
box, 12 November 2026: [`cli/README.md`](../../cli/README.md), "Tests").

## 2. Your keys

Everything derives from your LEZ wallet seed, under labels disjoint from LEZ's own key tree
([`bridge.md`](../design/bridge.md) §4):

| Key | Derivation | Lets its holder |
|---|---|---|
| `kAuth` | `HMAC(bridgeSeed, "ferrata/auth/v1")` | claim deposits and release burns |
| `kView` | `HMAC(bridgeSeed, "ferrata/view/v1")` | find your burns and read their recipients; **cannot** claim or release, and does not see deposits or claims |

Keep the seed; it is the only thing recovery needs (§8). No bridge operator holds anything of yours.

## 3. Deposit (Ethereum)

Call `depositETH(denomIndex, C, j)` or `deposit(token, denomIndex, C, j)` on the vault
([`vault/README.md`](../../vault/README.md), "Deposits"; [`bridge.md`](../design/bridge.md) §5.1–§5.3):

- the amount is exactly one registered denomination of the token (at most eight per token; the
  default registers three). The vault escrows the denomination **plus** the registered mint and relay
  fees, so your LEZ balance will be a whole denomination ([`vault/README.md`](../../vault/README.md),
  "Fees (v5, D13)");
- `j` must equal `depositCount[you]`: your deposits are numbered 0, 1, 2, … and that pair
  `(you, j)` is public;
- `C = H("ferrata/C/v1" ‖ s ‖ kind ‖ dest)` with `s = HMAC(kAuth, "ferrata/s/v1" ‖ evmChainId ‖ vault
  ‖ you ‖ j)`. `C` hides your LEZ destination and binds it and its kind (private or public account),
  so nobody can redirect your claim. `dest` is always an account derived from your seed;
- ERC-20s are pulled with `safeTransferFrom` and the vault's balance must grow by exactly the amount:
  USDT-style tokens work, fee-on-transfer tokens are refused. Plain ETH sent to the vault is refused
  (`DirectEthNotAccepted`).

The deposit appends a leaf `H(0x00 ‖ LEAF_TAG ‖ leafIndex ‖ depositor ‖ token ‖ amount ‖ C)` to a
SHA-256 Merkle tree of depth 32 whose root is in storage slot 0 ([`bridge.md`](../design/bridge.md) §5.2).

**Gas** (measured): on Foundry's rules (today's mainnet), v5 vault, 179,275 for a first ETH deposit
by an address and 159,285 for a later one; 197,762 for a first ERC-20 deposit
([`vault/GAS.md`](../../vault/GAS.md), mock-verifier table; deposits do not verify proofs). On
Sepolia, which runs Glamsterdam's state-gas pricing since 6 October 2026, the node charged the v3
vault 552,152 for the first ETH deposit of the tree, 260,526 for a later one and 375,237 for an
ERC-20 deposit ([`e2e/README.md`](../../e2e/README.md), "What the round trip costs on today's Sepolia";
priced by Sepolia's node on a fork's state, not sent). Gas paid on Ethereum is visible like any
transaction's.

**Reverts you may meet**: `InvalidDenominationIndex`, `WrongDepositIndex`, `ZeroCommitment`,
`WrongEthAmount`, `TransferAmountMismatch`, `TokenNotRegistered`, `DepositsDisabled`,
`DepositsAreFrozen` / `TokenDepositsAreFrozen`, `TokenDepositCapExceeded` /
`GlobalDepositCapExceeded`, `TreeFull`, `VaultRetired` (the custom errors of
[`vault/src/FerrataVault.sol`](../../vault/src/FerrataVault.sol)). A reverted deposit escrows nothing.

## 4. Root import (anyone)

Before your deposit can be claimed, someone sends `ImportRoot` on LEZ: the bridge program asks the
attestation program to prove the vault's slots 0 and 1 (root and leaf count) under a finalized
Ethereum record, inside its own transaction, and writes a write-once `root(cid, n)`
([`bridge.md`](../design/bridge.md) §5.4; [`lez/BRIDGE.md`](../../lez/BRIDGE.md) §2). Daemons import on a
fixed cadence; **a client never imports for itself**, because importing right after your own deposit
would date it. Your deposit is claimable once a root with a leaf count above its index exists:
Ethereum finality (about 13–19 minutes, [`docs/design/attestation.md`](../design/attestation.md)
§13) plus the next update and import. The importer (the relayer's root imports) is a prototype, run on the mini-app's devnet
(EVIDENCE rows 106, 107); imports also ran in the in-process harness, on a live sequencer on a stand-in beacon chain
([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2, §11) and in the crown runs, on Sepolia's real
state in the live one ([EVIDENCE](../EVIDENCE.md) row 41).

## 5. Claim (LEZ)

A claim proves that your leaf lies under an imported root, that its index is below that root's leaf
count, and that you can open `C`; it then spends the nullifier `N = H("ferrata/nf/v1" ‖ s ‖ fid)`,
where `fid` is the leaf's statement identifier, and mints one denomination
([`bridge.md`](../design/bridge.md) §5.3, §5.5; [`lez/BRIDGE.md`](../../lez/BRIDGE.md), "The claim
statement"). `N` contains your secret `s`, so nobody can compute it from public data; one deposit
lies under every later root but has one `N`, so it is claimed once
([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2: the same deposit under a second root is
refused, 250).

Three ways, the first preselected:

| Path | How | Public | Hidden | Measured |
|---|---|---|---|---|
| **Private claim to a private account** (default) | `ClaimPrivately` runs as your destination account inside a privacy-preserving transaction; it reads `root(cid, n)` and credits its own private state | `N`, the configuration, the token, the denomination, which root was read (`n`), the fee schedule's range, the relayer, the time, the bridge program, a one-time fee alias | which deposit (leaf, depositor, `s`, `C`, path) and your account | 553,641 execution gas, 224,063 storage gas; the prover proves 41,736 + 110,862 user cycles (in-process harness, dev mode, this tree's program) |
| Private claim to a public account | the same proof, from a private account of yours; the mint goes publicly to the destination | the above and the destination | which deposit | 603,981 execution gas |
| **Plaintext claim** | the witness is transaction data; one public transaction, for users who cannot prove | **everything, including which deposit**: the client must say so before allowing it | nothing | 261,671 execution gas (1 ETH), 2,053 bytes |

Sources: [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §1 (its last column: the merged program), §3;
[`lez/BRIDGE.md`](../../lez/BRIDGE.md), "What each path makes public". The private claim's 8,937 bytes were searched for its private inputs
(destination, `s`, `C`, the leaf, the depositor, the witness, the 32 siblings): all absent
(`a_private_claim_publishes_only_the_nullifier_the_denomination_and_the_root_read` in
[`lez/harness/tests/bridge_private.rs`](../../lez/harness/tests/bridge_private.rs)).

**Proving time** (measured): the private claim of the earlier program image was proven on one NVIDIA
L4 in 15.3 s (227,477-byte receipt, 0.78 GiB peak host memory for the claim and a burn together)
([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §7); on a live LEZ sequencer, LEZ's own wallet
proved a private claim on a Modal build box's CPU in 254.8 s (252,185-byte transaction), and the
sequencer, which verifies receipts, included it ([`lez/live/inbound/README.md`](../../lez/live/inbound/README.md);
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §11); the merged program's private claim was proven
by LEZ's wallet on a cloud CPU (16 cores reserved, up to 32) and sent in 276.8 s on a LEZ devnet, in the
crown rehearsal ([`e2e/runs/crown-rehearsal-3`](../../e2e/runs/crown-rehearsal-3/README.md)); as this repository's
harness builds it, it was proven in 905.4 s on cloud CPUs held to 8 CPUs and 16 GiB (half the declared minimum's threads)
and in 463.7 s at 16 CPUs ([`lez/results/p4-minimum`](../../lez/results/p4-minimum/README.md)). Under macOS 15.8 on Apple Silicon (Mac minis, CPU only) the padded private claim took
818.7 s on an M1 with 16 GB and 453.5 s on an M4 ([`lez/results/p4-macos`](../../lez/results/p4-macos/README.md), EVIDENCE row 88).
**Not measured:** the private claim on a user's laptop (declared minimum: 16 GB, 8 cores with 16 threads, x86-64,
or an Apple M1 with 16 GB, [`attestation.md`](../design/attestation.md) §13).

**Fees.** The claim mints your denomination to you and the escrowed fees to the fee recipient and to
the relayer it names ([`bridge.md`](../design/bridge.md) §5.5, §13). The LEZ transaction fee of a
privacy-preserving transaction is paid in native LEZ from a private account of the prover, inside the
proof; nobody else can pay it on v0.4. The **gas drop**, which lets a relayer fund that fee without
learning which deposit is yours, is built in part: its receipt binding `(N, oneTimeAddress, relayer)` and
the relayer's one credit per `N` (EVIDENCE rows 47, 48); the sealed credit to a private one-time account
and its sweep inside the private claim are **not built** (milestone M0 tests them); until it works, a first
claim needs native LEZ from a faucet or a public purchase, which is a correlation the client must
state ([`bridge.md`](../design/bridge.md) §6).

**A refused claim changes nothing** and the deposit stays claimable (R.1): a refusal reverts the whole
LEZ transaction, and a private claim that fails produces no receipt and no transaction
([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2–§3). A capped or frozen claim succeeds later
(§8 there: leaf 2 and B's 1 ETH were each claimed once, later). A valid claim never expires: records
and roots are write-once (R.4; [`attestation.md`](../design/attestation.md) §4.6).

## 6. Burn (LEZ)

To go back, burn exactly one denomination with a **top-level call** (public holding, signed by you) or
as the **root of a private execution** (private holding: the default). Any other route (a nested call
from another program, an asynchronous message) is refused (253), because the checkpoint proof could
not see it and your tokens would be destroyed without a burn leaf
([`lez/BRIDGE.md`](../../lez/BRIDGE.md), "D10"; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §4).

The burn publishes a **burn note** `(r, memo, mac, token, amount, COut)` ([`bridge.md`](../design/bridge.md) §12.1):

```
r_i  = HMAC(kView, "ferrata/burn-tag/v1" ‖ configId ‖ i)        i = your burn counter
sOut = HMAC(kAuth, "ferrata/s-out/v1" ‖ configId ‖ r)
COut = H("ferrata/Cout/v1" ‖ sOut ‖ evmChainId ‖ vault ‖ recipient)
memo = recipient XOR HMAC(kView, "ferrata/memo/v1" ‖ r)[0..20]
mac  = HMAC(kView, "ferrata/burn-mac/v2" ‖ r ‖ memo ‖ token ‖ amount ‖ COut)[0..16]
```

`COut` hides and binds your Ethereum recipient. The note is recorded once, keyed on its whole hash, so
a copy cannot block it. The wallet advances `i` on every published burn, even a reverted one, so a tag
never repeats.

**Public:** token, denomination, time, the note; for a private holding the holder appears as an alias.
**Hidden:** you and your recipient. **Measured** (in-process harness, this tree's program): a private burn
519,678 execution gas and 224,063 storage gas, 25,679 user cycles to prove; a public burn 166,477; the private
burn of the earlier image proven on one L4 in 12.4 s ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md)
§1, §7), the merged program's on a cloud CPU in 336.0 s with sending, in the crown rehearsal.

## 7. Checkpoint, wait, release (Ethereum)

**Checkpoint (anyone).** A prover proves the LEZ blocks since the last final checkpoint, collects every
non-reverted burn of this configuration into a burn tree, and submits the result to the vault as a
candidate. It becomes final after a wait: **1 hour under rule (B)**, `W = Δ + (t − 1) × turnSeconds`
under rule (A) (at least 3 h 16 min for three sequencers on template values), unless a watcher
cancels it ([`vault/README.md`](../../vault/README.md), "LEZ checkpoints"; [`bridge.md`](../design/bridge.md)
§9, §11.6, §13). You do nothing here; the operator guide covers it.

**Release delay.** Once the root holding your burn is final, the token's release delay must pass:
at least 24 hours outside testnets (0 allowed on testnets), at most the 7-day revocation window. It is
the window in which an accepted error can still be revoked ([`bridge.md`](../design/bridge.md) §13).

**Release.** You (or a relayer) call `release(r, seal)` with the request `{token, denomIndex,
recipient, relayer, relayFee, burnRoot, nullifier}`. The proof shows, in zero knowledge, that a burn
leaf lies under `burnRoot`, that you can open its `COut`, and that `NOut = H("ferrata/nf-out/v2" ‖
sOut)` is its nullifier; the journal binds the chain, vault, configuration, root, `NOut`, token,
amount, recipient, relayer and fee ([`bridge.md`](../design/bridge.md) §12.2). Prove against the
newest root whose release delay has passed. The vault pays the denomination less the relay fee your
proof names and the burn and checkpoint fees in force; if the transfer to the recipient fails, the
amount is credited to `owed[recipient][token]`, which only the recipient withdraws with
`claimOwed(token)` ([`vault/README.md`](../../vault/README.md), "Releases and pull payments",
"Fees (v5, D13)"). Name a relayer whenever the fee is non-zero: with `relayer = address(0)` the fee
goes to `msg.sender`.

**Proving** (measured, [`bridge.md`](../design/bridge.md) §18.2, §18.7): the release guest is 80,501
user cycles, the same for 64 random witnesses (a CI gate: a first version leaked the popcount of the
burn index); a succinct proof took 37.1–48.6 s on a 16-thread laptop CPU (up to 150 s under load),
peak 1.46 GB, receipt 223,930 bytes; 15.1 s and 14.7 s on 16 cores in the end-to-end run. The
**Groth16 wrap** runs only on x86 (Docker, about 7.3 GiB, 77.5–79.9 s on the laptop): any relayer can
wrap your succinct receipt and learns only what the release publishes.

**Gas** (measured): v5 vault with the real verifier on Foundry's rules, 373,657 for ETH and 403,523 for
an ERC-20 with a relayer's fee ([`vault/GAS.md`](../../vault/GAS.md), "v5 with the real verifier");
on Sepolia, priced by its node for the v3 vault, 619,477 and 648,447
([`e2e/README.md`](../../e2e/README.md)). Paid by whoever submits, normally the relayer.

**Public:** recipient, token, denomination, time, `NOut`, the root, the relayer. **Hidden:** which
burn. **Reverts you may meet:** `BurnRootNotAccepted`, `ReleaseDelayNotElapsed`,
`NullifierAlreadyUsed`, `InvalidRelayFee`, `ReleasesAreFrozen` / `TokenReleasesAreFrozen`,
`TokenReleaseCapExceeded` / `GlobalReleaseCapExceeded`, `InsufficientEscrow`, `VerifierStopped`.
None consumes your entitlement: the nullifier is marked only by a release that succeeds (R.2).

**Known gap:** the release guest that was proven implements SPEC-v2, whose `NOut` includes the burn
index; v2 of `NOut` (index-free, so a burn collected again after a revocation cannot be released
twice) is in `bridge-wire` and the specification, not in the proven release guest
([`bridge.md`](../design/bridge.md) §19; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §4).

## 8. Recovery from the seed alone (F.9, R.5)

A wallet restored from the seed, with no local state, finds everything
([`bridge.md`](../design/bridge.md) §5.6, §12.1):

- **Deposits.** Download the vault's whole deposit list (its `Deposit` events), recompute `C` for
  every `(depositor, j)` against a few seed-derived destinations (account indices `0..g` per kind),
  and keep the matches whose `N` is unused. Completeness is checked against `depositCount`. The client
  never queries per item, so no provider learns which deposits are yours. Measured (laptop CPU, one
  thread, bridge core v2): median 128.9 ms for 100,000 deposits, 1,308.6 ms for 1,000,000
  ([`bridge.md`](../design/bridge.md) §18.3).
- **Burns.** Regenerate the tags `r_0, r_1, …` from `kView`, find the notes that carry them, unmask
  each `memo` to read the recipient, and keep only notes whose `mac` verifies, so a copied note with
  another memo cannot mislead you. Then release those whose `NOut` is unused.
- **After retirement.** If the vault was retired before LEZ imported your deposit, recovery finds it
  among the refundable ones: anyone can call `refund(leafIndex, depositor, token, denomIndex, C, path)`
  once a final sunset checkpoint's revocation window has passed, and the vault pays the depositor named
  in the leaf: the denomination and the mint and relay fees ([`vault/README.md`](../../vault/README.md),
  "Refunds"; [`bridge.md`](../design/bridge.md) §16). Every leaf is either claimable on LEZ or
  refundable on Ethereum, never both.

`ferrata recover` does all of this from the seed file and the endpoints, with no earlier local state, and adds
the holdings: public ones read from the zone, private ones as the LEZ wallet decrypts them. Its end-to-end test
recovers 12 deposits (2 claimed, one of them privately), 12 burns (one private; 2 released) and the holdings
(measured, build box, 12 November 2026; [`cli/README.md`](../../cli/README.md)). In the test the harness stands
in for the LEZ wallet's sync of its private accounts. The module (`ferrata_bridge_module`) has `recover`, which finds the
wallet's deposits and burns among the events it is given (EVIDENCE row 59); reading those events through
Logos Core's own modules is **not built**; the scan's speed is measured in the bridge core.

## 9. Defaults, and what each weaker choice reveals (Privacy #7, U.5, U.6)

The design preselects the private choice everywhere; each weaker one needs an explicit confirmation,
and the client names what it would reveal ([`bridge.md`](../design/bridge.md) §17). The `ferrata` tool and the
mini-app implement these defaults and the same leak indicator (`cli/src/privacy.rs` is the mini-app's
`qml/js/privacy.js`): before sending, each command prints "privacy of these choices: Minimal / Elevated / High",
with a line per choice and a WARNING when a direct link would become public. The tool's options for the weaker
choices: `deposit --kind public --i-accept-public-account`; `claim --plaintext
--i-accept-plaintext-links-deposit`; `claim --no-delay`; `burn --from public --i-accept-public-account`;
`release --self-send`.

| Choice | Default | If you change it |
|---|---|---|
| Claim | private (privacy-preserving transaction) | plaintext: links your mint to your deposit, publicly and for good |
| Destination | a private account | public: the destination is visible (not which deposit paid it) |
| Amount | a fixed denomination | — (only denominations exist; choosing a rare one shrinks your anonymity set) |
| Submission | delayed: the client proposes a delay before claiming and before releasing (the tool holds a claim 1–12 h after its deposit, a delay derived from the seed per deposit; a release waits for the vault's release delay and uses the newest root past it) | immediate: timing links deposit to claim, or burn to release |
| Who submits | a relayer (any; you may pick another per operation) | yourself: your fee payer's identity and timing join the transaction |
| Burn from | a private holding | a public holding: the holder is visible |
| Root read | the newest imported root | an older root dates your deposit (it shows the deposit is at least that old) |

The client shows the **anonymity set** a claim would have: the deposits of that token and
denomination under the root it reads, less those already claimed in plaintext, which are public (derived; design:
[`docs/traceability.md`](../traceability.md), row F.8; **not built**).

## 10. What is public at each step (U.7)

From [`bridge.md`](../design/bridge.md) §17, with the measured dumps of
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §3:

| Step | Public | Hidden |
|---|---|---|
| Deposit (Ethereum) | depositor, token, denomination, time, `C`, the pair `(depositor, j)` | your LEZ destination |
| Root import (LEZ) | the vault's root and leaf count at a block; the same for everyone | — |
| Claim (LEZ) | `N`, token, denomination, time, the root read, the fee schedule's range, the relayer, the bridge program, a one-time fee alias with a fixed reserve; a public destination | which deposit; a private destination |
| Claim, plaintext | everything, including which deposit | — |
| Burn, private holding (LEZ) | token, denomination, time, the burn note | the holder; the recipient |
| Checkpoint (Ethereum) | the LEZ tip, `n`, `t`, the burn root and count | — |
| Release (Ethereum) | recipient, token, denomination, time, `NOut`, the root, the relayer | which burn |

**Residual correlations**, which no default removes ([`bridge.md`](../design/bridge.md) §17;
[`lez/BRIDGE.md`](../../lez/BRIDGE.md), "What each path makes public"):

- **amount and token**, bounded by the denominations;
- **timing**: deposit-to-claim and burn-to-release intervals; the client proposes a delay;
- **the root a claim reads**, which shows the deposit is at least that old; read the newest;
- **a retry after a cap rejection**, which shows the first attempt's time; the client checks caps
  before proving;
- **a fee-schedule change**, which splits sets by date: the claim's fee range shows which schedule
  applied (the mint fee is public and differs between schedules);
- **network metadata** (your IP to an RPC or sequencer): out of scope, disclosed;
- **an adversary's own deposits**, which it can subtract from any set;
- **the fee payer** until the gas drop exists (§5).

## 11. Errors (U.8, Privacy #5)

On LEZ a refusal is a bare exit code in `240–255` (the bridge's block) or the attestation program's
codes; none carries a leaf index, an amount beyond the public request, or a destination. A failed
private claim or burn produces no transaction at all ([`lez/bridge-wire/src/lib.rs`](../../lez/bridge-wire/src/lib.rs),
module `code`; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2–§4, §8).

| RFP-022 U.8 mode | Code | Name |
|---|---|---|
| unsupported token | 251 | `NOT_REGISTERED` (also an amount that is not a denomination) |
| invalid amount | 251; 254 | `NOT_REGISTERED`; `INSUFFICIENT_BALANCE` (a burn larger than the holding) |
| cap exceeded | 255 | `CAPPED` (the token's or the global mint or burn bucket) |
| verification failure | 249; 248; 252 | `CLAIM_INVALID` (path, opening, redirected destination, another configuration's vault); `LEAF_NOT_IMPORTED` (index not below the root's count, or outside the fee range named); `WRONG_DESTINATION` (a private-kind deposit by the plaintext path, or a private claim not run as its destination) |
| insufficient finality | 245 | `NOT_FOUND` (no imported root yet, or no configuration) |
| already claimed | 250 | `ALREADY_CLAIMED` |
| program or per-token frozen | 242 | `FROZEN` (also any import after `Sunset`) |
| other | 240, 241, 243, 244, 246, 247, 253 | `BAD_MESSAGE`, `BAD_STATE`, `WRONG_SENDER`, `ALREADY_WRITTEN`, `PAIRING_MISMATCH`, `IMPORT_MISMATCH`, `NOT_AUTHORIZED` |

On Ethereum the vault reverts with named custom errors (§3, §7). A rejected, a repeated and a capped
claim are indistinguishable in what they reveal about the deposit, because a private claim that
fails is never published, and a plaintext one already published everything. The `ferrata` tool runs the
program's own checks and an `eth_call` before it signs, and names each refusal with its source, code, meaning
and where it was found (`checked before sending`, or `returned by` the program or the vault); its messages do
not name the deposit or burn ([`cli/README.md`](../../cli/README.md), "Refusals"). **Not built:** the module's
messages for each code.

## 12. If the bridge is frozen, retired or stopped

([`vault/README.md`](../../vault/README.md), "Freezes", "Refunds", "Verifier and stop";
[`bridge.md`](../design/bridge.md) §14–§16, §20)

- **Frozen** (deposits, releases, mints or burns, globally or per token): your position waits. Every
  freeze lapses after at most 30 days unless renewed; the admin cannot unfreeze.
- **Retired** (`retire()` on the vault, `Sunset` on LEZ): deposits stop forever and root imports stop;
  claims against imported roots, burns and releases continue; unimported deposits become refundable
  after a final sunset checkpoint and its 7-day revocation window. Anyone may retire once no checkpoint
  has been finalised for 30 days.
- **Verifier stopped** (only by RISC Zero's own proof that its proof system is unsound): nothing more
  is released or refunded; owed balances stay claimable with `claimOwed` (vault v5; the sentence of
  [`bridge.md`](../design/bridge.md) §15 that refunds still work after a stop describes an earlier
  prototype: in v5 `refund` requires no verifier stop, [`vault/README.md`](../../vault/README.md), "Refunds").
- **A wrong checkpoint not revoked within 7 days halts the vault for good.** What bounds your loss is
  in [`docs/trust.md`](../trust.md).

## 13. For developers: integrating both flows (R.12)

The integration surface today is the program messages and the derivations; the Logos module
(`ferrata_bridge_module`: `deposit`, `claim`, `burn`, `release`, `recover`, `status`) is built and called
under a headless Logos Core (EVIDENCE row 59); it builds messages without signing or sending them, and its
private claim, `ImportRoot`, registry reads and release proving are **not built**
([`bridge.md`](../design/bridge.md) §1; [`modules/README.md`](../../modules/README.md)). Build against these:

- **Messages** of the bridge program, one borsh enum in [`lez/bridge-wire/src/lib.rs`](../../lez/bridge-wire/src/lib.rs),
  tabulated in [`lez/BRIDGE.md`](../../lez/BRIDGE.md) §2: `Init` (4), `ImportRoot` (1), `Register` (11),
  `ImportFees` (12), `Claim` (5, plaintext, to `root(cid, n)`), `ClaimPrivately` (6, at the claimant's
  private holder actor), `Burn` (0, top-level or private root), `Tick` (19), and the role messages
  `Freeze` (17), `SetCap` (18), `Sunset` (2). Variants 0–3 are decoded by the checkpoint program, so
  their bytes are fixed.
- **Actors and addresses**: `config(cid)`, `token(cid, token)`, `root(cid, n)`,
  `nullifier(chain, vault, N)` (no `cid`: one namespace per vault for the whole program), the holder,
  `burn(cid, H(note))`; seeds `H("ferrata-lez/v1/bridge/<role>" ‖ …)` ([`lez/BRIDGE.md`](../../lez/BRIDGE.md) §2).
- **Derivations**: `configId` ([`bridge.md`](../design/bridge.md) §3), `s`, `C`, `fid`, `N` (§5.3), the
  leaf (§5.2), `r_i`, `sOut`, `COut`, `memo`, `mac`, burn leaf (§12.1), `NOut` and the release journal
  (§12.2). Implementations: `ferrata-bridge-core` (SPEC-v2 encodings) and `bridge-wire` (`nf_out_v2`).
  Vectors: [`bridge/vectors/inbound-v2.json`](../../bridge/vectors/inbound-v2.json) (the claim tree whose
  `N` the program spends).
- **Vault ABI**: [`vault/src/FerrataVault.sol`](../../vault/src/FerrataVault.sol); storage slots and
  encodings in [`vault/README.md`](../../vault/README.md), "Storage-slot contract" and "Encodings".
- **Worked examples** (tests that run each flow through LEZ's state machine):
  `inbound_configuration_import_and_plaintext_claim` and `inbound_under_a_gloas_record`
  ([`lez/harness/tests/bridge_inbound.rs`](../../lez/harness/tests/bridge_inbound.rs)); the private
  claim and its dump ([`lez/harness/tests/bridge_private.rs`](../../lez/harness/tests/bridge_private.rs));
  burns ([`lez/harness/tests/bridge_burn.rs`](../../lez/harness/tests/bridge_burn.rs)); registry, fees,
  caps, freezes, `Sunset` ([`lez/harness/tests/bridge_controls.rs`](../../lez/harness/tests/bridge_controls.rs));
  the vault's own storage ([`lez/harness/tests/bridge_vault_storage.rs`](../../lez/harness/tests/bridge_vault_storage.rs));
  the return leg on Ethereum with real proofs ([`e2e/README.md`](../../e2e/README.md)).
- **Rules for a client**: prove at submission time against the newest root; check caps and freezes
  before proving (a retry after a cap rejection dates the first attempt); advance the burn counter on
  every published burn; keep the blocks a candidate was proven over if you are also a prover
  ([`bridge.md`](../design/bridge.md) §18.7, last paragraph).

## 14. What is not built

([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §9; [`bridge.md`](../design/bridge.md) §1,
§18.6, §19; [`vault/README.md`](../../vault/README.md), status paragraph)

- a wallet integration; in `ferrata_bridge_module`, the private claim, `ImportRoot`, registry reads and
  release proving (the module is built: EVIDENCE row 59); the anonymity-set view; the `ferrata` tool's LEZ commands
  against a live sequencer (its tests use the LEZ harness);
- the gas drop's sealed credit to a private one-time account and its sweep inside the private claim, and
  the relayer's root imports (the gas-drop receipt and the relayer daemon `ferrata-relayer`, which also
  wraps, are built: EVIDENCE rows 46–48);
- a transfer that credits a private recipient privately (transfers between holders are built: EVIDENCE
  rows 74, 75); the switch to LEZ's token program;
- a real proof of a private claim to a public destination, and laptop or GPU proofs of the current program
  image's private claim and burn (the crown rehearsal, the live round trip and `lez/results/p4-minimum` proved
  them on cloud CPUs, the latter held to 8 and 16 CPUs with 16 GiB); any user-side proof on a laptop (under macOS they were proven on M1 and M4 Mac minis, CPU only:
  EVIDENCE row 88);
- a public LEZ network under the round trip: the v5 vault made the whole round trip on Sepolia on 12 November
  2026 on the final programs, a private claim of a Sepolia deposit and a private burn included, with LEZ as a devnet we ran (one
  validator; [`e2e/runs/crown-live-3/`](../../e2e/runs/crown-live-3/README.md)), as on 11 November on the first live
  run's programs ([`e2e/runs/crown-live-1/`](../../e2e/runs/crown-live-1/README.md)); before them, the v3 vault of the end-to-end fixture
  got one deposit on Sepolia, claimed by the plaintext path
  ([`lez/live/inbound/README.md`](../../lez/live/inbound/README.md); [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §11);
- LEZ testnet 0.4 itself, which did not exist when this was written.
