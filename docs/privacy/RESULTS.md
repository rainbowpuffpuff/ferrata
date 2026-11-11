# Privacy suite: results (proposal milestone B2; RFP-022 Privacy #1–#7, F.9, R.5)

Milestone B2's gate asks that, for 20 deposit and claim pairs and 20 burn and release pairs, the public
transaction bytes, block bodies, events and state diffs differ only in fresh randomness and nullifiers,
that a deliberately linkable variant fail the same test, and that a wiped client recover its unclaimed
deposits and unreleased burns from the seed alone. This suite measures that against what exists: the
bridge program on LEZ v0.4.0-rc1 (through LEZ's own state machine, private paths in RISC Zero dev mode)
and `FerrataVault` v3 (Foundry, mock verifier).

**Where it ran.** Everything below ran on the Modal build box of this worktree (`spike/modal-build/mrun`,
2 vCPU), `scripts/privacy/run.sh` at this commit's code: the LEZ half in 74.8 s, the Foundry half in
0.34 s, the analysis in about 4 s; the whole run 1 min 27 s. A repeat run wrote a byte-identical
`results.json` (the run is deterministic). The numbers are the box's output, copied from
[`generated/summary.md`](generated/summary.md), [`generated/b6.md`](generated/b6.md) and
[`generated/results.json`](generated/results.json), which `scripts/privacy/adversary.py` wrote. The bridge
program's image in this build is `23cc5025…9132` (built at this worktree's path; the ELF holds its
build paths, so it differs from `BRIDGE-RESULTS.md`'s `b5d177bb…`; no guest source changed).

**Re-run on the integrated tree (11 November 2026).** On branch `integration3` the suite runs against the
merged bridge program (registry, fees, the Gloas header; image `5943b750…42f3` at that worktree's path) and
`FerrataVault` v5, whose deposits must carry the denomination plus the mint and relay fees in force (the test
now sends that total). `scripts/privacy/run.sh` ran there on the build box (83 s) and the files in
[`generated/`](generated/) are that run's. Unchanged: the gate, the adversary's rates on both private paths
and on the plaintext and naive variants, the hidden column, the recovery. Changed: the Ethereum gas (v5:
deposits 1,355 more, releases 1,680 more, below), v5's storage slots in the field table, and the
informational claim → burn rate, 4.5% → 4.8% (its candidate sets depend on the build's ids). The figures
below are the re-run's where they differ.

## Results in brief

| check | default private path | linkable variants |
|---|---|---|
| differential gate (identical once nullifiers, fresh randomness, a user's own address and time are cut; receipts included) | identical: deposits, private claims (8,953 bytes each), private burns (7,857), releases (324 bytes of calldata) | plaintext claim: **differs** (the destination's account, the witness); naive `N = fid` claim: identical (the gate cannot see that a nullifier is computable; the adversary does) |
| adversary, inbound (deposit → claim), chance 5.0% | **5.0%**: no shared value at all; every candidate set is all 20 deposits | naive `N = fid`: **100%** (20 of 20 identified); plaintext claim: **100%** (20 of 20) |
| adversary, outbound (burn → release), chance 5.0% | **5.0%**: every candidate set is all 20 burns | naive `NOut = burnId`: **100%** (20 of 20) |
| §B6's hidden column, searched in every record's public bytes | 0 of 320 values found (destination, `s`, `C`, leaf, `fid`, depositor; holder, recipient; `r`, `COut`, memo, mac, burn leaf, `burnId`) | – |
| recovery from the seed alone, after 10 claims and 10 releases | unclaimed deposits **10 of 10**, unreleased burns **10 of 10**; claimed and released ones seen as such, 10 and 10; no other user's item taken; recovered `NOut` equal to the vault's for 20 of 20 | – |

Informational: deposit → release over the round trip, 5.0%; claim → burn on LEZ (the same private
accounts), 4.8%. Including the order-bearing state the main adversary leaves out (below) changes none of
the rates.

## Method

**Scenario** (`lez/harness/tests/privacy_suite.rs`). Twenty fresh users, each with its own bridge seed,
LEZ private account, Ethereum depositor and Ethereum recipient. Vault P (`0xa0…a0`, chain 1) receives one
1 ETH deposit per user (leaves 0–19), whose `C` hides a private destination; one `ImportRoot` brings the
root of all 20 to LEZ; each user then claims privately into its own private account
(`ClaimPrivately`, dev mode), reading that newest root; later each burns its 1 ETH from that private
holding (`Burn` as the root of its own private execution), with a note for its own recipient; each burn
is then released on Ethereum through one relayer with the registered relay fee. Claims, burns and
releases each go out in an independent uniformly random order (seeded Fisher–Yates), one transaction per
block, one minute apart: the delay the client proposes, with every deposit before every claim. Token
and denomination are the same for all, so the anonymity set is the 20. Every private transaction draws
a fresh alias opening and a fresh witness seed (`Chain::prove_and_settle_private_fresh`; deterministic
per run so that a run repeats byte for byte). The fee reserve is fixed (10^8) and refunded to the
harness's one payer.

**Linkable variants.** (a) The plaintext claim: the same users deposit to public LEZ accounts in a
second universe and claim on the plaintext path (`Claim`, the witness in the transaction). (b) The
naive nullifier, the RFP's pattern: inbound, `N = fid` (the statement identifier). The bridge program
computes `N = H(s ‖ fid)` inside its frozen image, so this variant is the private claims' public data
with `fid` written wherever `N` and the nullifier account derived from it stood (a model, not a run).
Outbound, `NOut = burnId` runs for real: the same 20 releases on the same vault state with that
nullifier (the mock verifier accepts any registered journal).

**Collectors.** LEZ: per transaction, its bytes (`borsh(LeeTransaction)`), the block body (read back
with the checkpoint program's parser, which also checks that each burn is in the clear exactly once),
the events, the state diff (every declared public account and the fee accounts, before and after, and
the commitment-set root before and after), what settlement charged (execution and storage gas, cycles,
base fee) and every field the test can name. Ethereum (`vault/test/PrivacySuite.t.sol`): the vault
deployed at P with P's configuration id; per transaction, the sender, value, calldata, events, every
storage write, every balance change and the gas used. Two cross-checks
tie the halves: the vault's deposit root after the 20 deposits equals the harness's tree root, in both
universes; and the vault accepts each release only if the journal the bridge core's release statement
built (`verify_release`, what the release guest runs) is byte for byte the one it rebuilds. Both hold.

**The gate.** Within each step's 20 records, the values of fields classified as nullifiers (`N`, the
nullifier account, LEZ's private-account nullifiers, `NOut`), fresh randomness (`C`, the burn note, LEZ
commitments, encrypted notes, aliases, the proof), a user's own public address (depositor, recipient)
or time and order (blocks, timestamps, the claim's time, commitment-set roots, the leaf index, the
vault's tree and escrow, the base fee) are cut out where they vary; the rest, receipts included, must be
byte for byte identical. Gas counts as order only where the data show that it follows the operation's
position and not its user (below).
Accounts every record of a step changes (the fee accounts, the token's and the configuration's gates,
the relayer's balance, the payer) are compared as cumulative state: the same accounts in every record,
their contents left to the adversary.

**The adversary** (`scripts/privacy/adversary.py`). It sees all public data of both sides and the
ground truth not at all. It does not use token, denomination or time and order: block ids, timestamps,
the claim's time, nonces, commitment-set roots and the contents of the shared cumulative accounts are
removed, and so is the proof (a dev-mode fake receipt here). Evidence for a pair is a shared 16-byte
window with at least 12 non-zero bytes (hash-like content, not padded small numbers), taken per named
field, weighted by how rare it is on both sides (zero for a window every record has). To the public
bytes it adds every identifier it can compute from them: for a deposit its leaf, `fid` and their
hashes; for a burn its leaf, `burnId` and note hash (from the burn's position in the checkpoint's
order); for a plaintext claim the `C`, leaf and `fid` its witness opens. Its guess for a record is the
set of best-scoring candidates; success is the mean of 1/|set| over the records whose partner is in it.
Chance is 1/20. A second run keeps the order-bearing state in; a third drops the derived identifiers.

**Recovery.** An independent wallet (the script's own HMAC and SHA-256 code, from the encodings table
of `bridge/README.md` and D12/D13) starts from each user's bridge seed and the accounts its LEZ seed
gives, and reads only whole lists: every deposit event of the vault, every `N` spent on LEZ, every burn
note in the checkpoint's order, every `NOut` the vault marked; its interface has no per-item query.
The lists are cut after the 10th claim and the 10th release in submission order. Inbound it recomputes
`s` and `C` per deposit; outbound it regenerates `r_i` up to its gap, checks D12's mac, decrypts the
memo, checks `COut` and computes `NOut` (v1, the release guest's).

## The generated tables

### Differential gate and sizes ([generated/summary.md](generated/summary.md))

| records | bytes (LEZ transaction / Ethereum calldata), min–max | identical once nullifiers, fresh randomness, own addresses and time are cut |
|---|---|---|
| deposit (A: private destinations) | 100–100 | yes |
| deposit (B: public destinations) | 100–100 | yes |
| claim, private (default) | 8,953–8,953 | yes |
| claim, naive nullifier N = fid | 8,953–8,953 | yes |
| claim, plaintext | 2,061–2,061 | no: from byte 5 of the transaction (the declared actors: the destination's account), then the witness |
| burn, private holding | 7,857–7,857 | yes |
| release, NOut of the release statement | 324–324 | yes |
| release, naive NOut = burnId | 324–324 | yes |

### Adversary

| game | success | exactly identified | candidate-set sizes (size: records) | without derived identifiers | with order-bearing state | order only (informational) |
|---|---:|---:|---|---:|---:|---:|
| inbound, private (default) | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% | 0.0% |
| inbound, naive nullifier N = fid | 100.0% | 20 of 20 | 1: 20 | 5.0% | 100.0% | 0.0% |
| inbound, plaintext claim | 100.0% | 20 of 20 | 1: 20 | 100.0% | 100.0% | 5.0% |
| outbound, private (default) | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% | 5.0% |
| outbound, naive NOut = burnId | 100.0% | 20 of 20 | 1: 20 | 5.0% | 100.0% | 5.0% |
| round trip: deposit to release | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% | 5.0% |
| on LEZ: claim to burn | 4.8% | 0 of 20 | 1: 1, 20: 19 | 4.8% | 4.8% | 5.0% |

"Order only" pairs the k-th of each side in time order: what the client's delay has to defeat, at
chance here because the orders are random by construction. The naive variants are found only through a
derived identifier (`fid`, `burnId`): without them the adversary is at chance, which is why the
adversary computes them.

### Receipts

Gas is public, and it varies; every variation follows the operation's position, not its user:
- **LEZ claims.** Execution gas, minus the first claim's: `[0, 5188, 5189, 5189, 5187, 5188, 5188, 5186,
  5186, …]` by position, the same sequence in the private and the plaintext universe, whose users stand
  in different orders, so it is the shared public part (Issue, Admit, Spend, the mints) that varies, with
  the state the claims before it left; the cause of the 5,186 was not isolated. Storage gas is constant
  (224,063 private, 2,061 plaintext).
- **LEZ burns.** The first burn costs 578 gas more than the 19 others, which cost the same.
- **Ethereum releases.** The first costs 105,461 gas, the 19 others 80,461: 25,000 more, the EVM's charge
  for a value transfer that creates an account, here the relayer's (a fresh address) on its first relay
  fee. The naive releases show the same.
- **Ethereum deposits.** Gas follows the leaf index: 139,595 for leaf 0; 99,917 to 100,283 for leaves 1, 3,
  7 and 15, which write a level of the tree's frontier for the first time; 80,017 to 80,261 otherwise. The
  sequence is the same in both universes, whose commitments and destinations differ.

So the receipts carry order, as the base fee does (it moves with the chain's history), and nothing that
tells one user from another; a client that pays for the first use of a relayer, or that is the first to
claim, is visible as such, which §B6's timing residual already covers.

### What is public at each step, against §B6

The full generated table, every field and state change with how it varies across the 20 records and its
class, is [generated/fields.md](generated/fields.md); [generated/b6.md](generated/b6.md) lays it against
§B6. Every item §B6 lists as public is found in the data (deposit: depositor, token, denomination, time,
`C`; claim: `N`, token, denomination, time, the root read, the bridge program, the fee alias and its
reserve, and no destination, all destinations being private; burn: token, denomination, time, the
note; release: recipient, token, denomination, time, `NOut`, the root, the relayer; the root import). Every item §B6 lists as hidden was searched for in
every record's public bytes and not found (0 of 320). Everything else that varies is a nullifier, fresh
randomness, a user's own address or time. The differences, each a finding:

1. **The claim names its relayer, and the relay fee is minted to it publicly.** `Issue.relayer` and the
   relayer's wrapped balance, which grows by the relay fee in every claim's state diff. §B6's claim row
   lists no relayer (the release row does). Constant across users here; a client that lets users pick
   relayers splits the claim's anonymity set by relayer.
2. **The claim names its fee schedule's leaf range** (`Issue.range`). Known (`BRIDGE.md` §2: the mint
   fee already shows the schedule); constant here, since all 20 leaves are under schedule 0. §B6 omits it.
3. **The fee reserve's refund account is public, and so is the refund.** `fee.refund_to` in every
   private claim and burn, and that account's balance in the state diff. §B6 omits both. In the harness
   it is one shared account; a wallet that named an account of its own would tie all its private
   transactions to that account (not to a deposit). The client must name none (LEZ then returns the
   unspent reserve to the alias as presented) or a shared one, and §B6 should say so.
4. **The burn shows a fee alias with its fixed reserve too** (the `Credit` of 10^8 to the fee inbox,
   from an alias). §B6's burn row lists only token, denomination, time and the note.
5. **LEZ's own private-transaction fields.** Every private claim and burn carries two private actions
   (the main one and the fee fallback), each with a nullifier, a new commitment, an encrypted note (a
   ciphertext of 137 to 247 bytes and a 1,088-byte ephemeral public key) and the commitment-set root it was
   proven against. They are nullifiers, fresh randomness and a state reference, and §B6 lists none of
   them. The root shows which state the proof read: 80 of 80 private actions here were proven against
   the root just before their transaction, the newest. A wallet that proved a burn against an older root
   would point at the transaction that made the spent commitment, its own claim (inferred from the
   data's structure, not run).
6. **The deposit's `j` and leaf index are public** (`Deposit.index`, `Deposit.leafIndex`, and the write
   to `depositCount[depositor]`). §B1 says so; §B6's deposit row omits them. `j` is 0 for a fresh
   depositor; a repeat depositor's `j` counts its deposits.
7. **The release shows the registered relay fee** (constant per token and denomination). §B6 omits it.

Everything public about the root import is §B6's "the vault's root at a block" and the submitter.

## Other findings

8. **The bridge core's burn recovery cannot find LEZ's notes.** `ferrata_bridge_core::outbound::recover_burns`
   checks the v1 mac (without the memo); the notes LEZ records carry D12's v2 mac (over the memo). Run
   over the 20 burns, it found 0 of 20; the suite's wallet, with the v2 mac, finds 20 of 20. The fix
   belongs in a wallet-side crate: the core is compiled into the release guest, whose image is frozen
   (adding to the core changed that image before, `bridge/README.md`).
9. **LEZ's test helpers make aliases and ephemeral keys deterministic.** With `ALIAS_OPENING = [9; 32]`
   and a zero witness seed, as the harness's other tests and LEZ's own use, an alias is
   `SHA-256(prefix ‖ account ‖ factor)` (`lee` core `program/mod.rs`, `blinded`) and the note's ephemeral
   key a function of the account, the seed and the nonce, so the same private account's claim and burn
   would carry the same alias. Derived from LEZ's source, not run; this suite draws both fresh per
   transaction, and the wallet has to.
10. **Method traps met, for whoever extends this** (not properties of the bridge): LEZ's declared actors
    are a sorted set, so a per-record account moves the others' byte positions (the gate cuts whole
    actors, the adversary reads fields as separate items); a block id such as `05 00…00` also occurs
    inside the 10^8 reserve's encoding, so short time values must not be cut out of a transaction; and
    the commitment-set root and the shared accounts chain each transaction to the previous one, which an
    adversary must treat as order, not identity.

## What is not covered

- **Real proofs.** Private transactions ran in dev mode; the proof (a fake receipt here) is left out
  of the comparison and the adversary. The zero-knowledge property of RISC Zero receipts is assumed, as
  §B3 says; the real receipt of an earlier private claim was searched for private inputs in
  `lez/BRIDGE-RESULTS.md` §7. The vault ran with the mock verifier; a real Groth16 seal is fresh
  randomness the comparison did not see.
- **The Ethereum side of LEZ.** The vault's state reaches LEZ through synthetic records at genesis, as in
  all bridge-program tests; the deposit root is cross-checked equal, not attested from a real block.
- **Timing and amounts.** One token, one denomination, one relayer, every deposit before every claim and
  random orders by construction: the allowed signals are held uninformative, not measured. Real traffic,
  the fee-schedule split, cap-rejection retries and the root-age residual of §B6 are not exercised.
- **Naive inbound nullifier** is a substitution model, not an execution.
- **Privacy #4–#7.** Failure paths (Priv.5) are covered only by the vault's `Privacy.t.sol` (failures
  carry a selector only), not here; the gas drop and relayer switching (Priv.6) are not built; client
  defaults (Priv.7) and component documentation (Priv.4) are not software yet. Network metadata is out
  of scope.
- **LEZ's anonymity set.** The 20 private accounts exist from genesis in an otherwise empty commitment
  set; the private accounts' own recovery (LEZ's encrypted notes) is LEZ's and is not tested.
- **The adversary is one strong class**, exact shared values and derived identifiers. The fields it
  cannot match exactly are each constant or classified by the gate; low-entropy fields are therefore
  covered by the gate's structural check, not by a statistical attack.

## Reproduce

```sh
# from the repository root, on a machine with lez/'s and vault/'s toolchains (vault/lib installed)
export RISC0_HOME=$PWD/../risc0-home-lez        # as lez/BRIDGE-RESULTS.md §10
scripts/privacy/run.sh                          # LEZ half, Foundry half, analysis -> docs/privacy/generated/
```

Raw data (about 4.4 MB: `lez.json`, `eth.jsonl`, `scenario.json`, `truth.json`) goes to
`target/privacy/` and is not committed; the run is deterministic, so it regenerates byte for byte.
