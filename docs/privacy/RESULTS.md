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
[`generated/`](generated/) were that run's until the re-run on the merged programs (below). Unchanged: the gate, the adversary's rates on both private paths
and on the plaintext and naive variants, the hidden column, the recovery. Changed: the Ethereum gas (v5:
deposits 1,355 more, releases 1,680 more, below), v5's storage slots in the field table, and the
informational claim → burn rate, 4.5% → 4.8% (its candidate sets depend on the build's ids). The figures
below are the latest re-run's, on the merged programs (after the next paragraph).

**Re-run with attested time (branch `lez-time-fix`, 11 November 2026).** The bridge program's freezes and
caps now run on attested Ethereum time and no message carries a caller's time (`../../lez/BRIDGE.md` §6;
image `d0c11833…23cd` at that worktree's path). `scripts/privacy/run.sh` ran on that worktree's build box
(89 s) into a scratch directory; [`generated/`](generated/) was **not** replaced then. Against it: the gate's verdicts, the inbound and outbound rates and the recovery are
unchanged; the private claim is 16 bytes shorter (8,937) and the plaintext claim 8 (2,053); the fields
`Issue.time_ms` (in every private claim's boundary message) and `Claim.time_ms` are gone, so the private
claim no longer publishes a caller's time (its block still dates it); claims' execution gas by position is
now `[0, 4937, 4937, …]` (the `5,186`–`5,189` wobble is gone) and burns' is constant (it was `[0, -578, …]`);
and the informational claim → burn rate is 5.0%, chance, with every candidate set of size 20 (it was 4.8%,
one record with a set of size 1; that rate also moves with the build's ids, above).

**Re-run on the merged programs (branch `integration6`, 11 November 2026).** The bridge program with both the
time fix and the security fixes (image `cf41b7e5…5390`, built at the `integration5` worktree's path: the image of
`../../lez/results/`) and `FerrataVault` v5 with its security fixes. `scripts/privacy/run.sh` ran on the build box
(77 to 80 s; 384 s to build it from cold), and [`generated/`](generated/) is now that run's. Two changes to the
suite came first: its seed-only wallet derives `NOut` as D13's v2, `H("ferrata/nf-out/v2" ‖ sOut)` (review M-3;
with v1 it still found every unreleased burn but saw none of the 10 released ones as released), and it names the
holder PDAs where wrapped balances now sit (review C-1). Against the `integration3` run: the gate's verdicts, the
inbound and outbound rates, the hidden column and the recovery are unchanged; the private claim is 8,937 bytes and
the plaintext claim 2,053, without the caller's time, as on `lez-time-fix`; the plaintext claims now differ from
byte 133 on (the declared accounts, after the program id), then the witness; LEZ execution gas is the same for
every claim after the first and for every burn; Ethereum deposits cost 145 more (the vault's security fixes); the
bridge core's `recover_burns` finds all 20 notes (D12's v2 mac in the core, review I-4: finding 8 below is
resolved); the informational claim → burn rate is 4.5% (two records drew a wrong singleton set; the rate moves
with the build's ids).

**Negative controls (18 November 2026, [`CONTROLS.md`](CONTROLS.md)).** The gate is shown to fail when it should:
272 test-only copies of the public data, each with a value an operation must hide planted in a public field (constant,
message, fresh-randomness or state; raw, byte-reversed, hex, first or last 16 bytes), are all caught, and each of 16
weakened detectors (a field skipped, a prefix compared, an encoding ignored, a class widened, the adversary's windows
lengthened ...) misses planted leaks the real one finds. The hidden-column search now looks for 680 values (the seed,
`kAuth`, `kView`, `sOut`, `NOut` before the release, the memo pad and the holder added) in four encodings and every
16-byte window; on this commit's run it finds 0 of 680. The tables below are the earlier runs' (320 values, verbatim).

**Regenerated for CI (branch `rfp-ci-coverage`, 22 November 2026; again at its integration).** [`generated/`](generated/)
is now the run of `scripts/privacy/run.sh` on the build box at this tree's programs (bridge image `ac1fb59b…442e`,
the build since milestone B1, `ferrata_bridge_b1` in [`../image-ids.md`](../image-ids.md); the coverage stream ran it
at the deployed `ccde6171…71dd`, with the same verdicts; the tables before were image `cf41b7e5…5390`'s); a second run
gave the same files byte for byte, the dev-mode proving times aside. CI's `lez-harness` job now runs the suite and
[`scripts/privacy/check-results.py`](../../scripts/privacy/check-results.py), which fails unless the gate passes on
every private default, the controls are linkable, the adversary is at chance on both legs, nothing hidden is
found, recovery is complete and these tables are reproduced. Every verdict is the earlier run's; the collector
now splits the witness set (signatures, proof length and kind, the receipt's claim) and names the calldata's gas,
and a claim's execution gas steps by 4,377 per position (4,335 at `ccde6171…`, 4,720 before).

## Results in brief

| check | default private path | linkable variants |
|---|---|---|
| differential gate (identical once nullifiers, fresh randomness, a user's own address and time are cut; receipts included) | identical: deposits, private claims (8,937 bytes each), private burns (7,857), releases (324 bytes of calldata) | plaintext claim: **differs** (the declared accounts, the witness); naive `N = fid` claim: identical (the gate cannot see that a nullifier is computable; the adversary does) |
| adversary, inbound (deposit → claim), chance 5.0% | **5.0%**: no shared value at all; every candidate set is all 20 deposits | naive `N = fid`: **100%** (20 of 20 identified); plaintext claim: **100%** (20 of 20) |
| adversary, outbound (burn → release), chance 5.0% | **5.0%**: every candidate set is all 20 burns | naive `NOut = burnId`: **100%** (20 of 20) |
| §B6's hidden column, searched in every record's public bytes | 0 of 320 values found (destination, `s`, `C`, leaf, `fid`, depositor; holder, recipient; `r`, `COut`, memo, mac, burn leaf, `burnId`) | – |
| recovery from the seed alone, after 10 claims and 10 releases | unclaimed deposits **10 of 10**, unreleased burns **10 of 10**; claimed and released ones seen as such, 10 and 10; no other user's item taken; recovered `NOut` equal to the vault's for 20 of 20 | – |

Informational: deposit → release over the round trip, 5.0%; claim → burn on LEZ (the same private
accounts), 4.5%. Including the order-bearing state the main adversary leaves out (below) changes none of
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
| claim, private (default) | 8,937–8,937 | yes |
| claim, naive nullifier N = fid | 8,937–8,937 | yes |
| claim, plaintext | 2,053–2,053 | no: from byte 133 of the transaction (the declared accounts, after the program id), then the witness |
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
| on LEZ: claim to burn | 4.5% | 0 of 20 | 1: 2, 20: 18 | 4.5% | 4.5% | 5.0% |

"Order only" pairs the k-th of each side in time order: what the client's delay has to defeat, at
chance here because the orders are random by construction. The naive variants are found only through a
derived identifier (`fid`, `burnId`): without them the adversary is at chance, which is why the
adversary computes them.

### Receipts

Gas is public, and it varies; every variation follows the operation's position, not its user:
- **LEZ claims.** Execution gas, minus the first claim's: `[0, 4377, 4377, …]` by position (4,335 at `ccde6171…`, `4720` on image `cf41b7e5`), the same
  sequence in the private and the plaintext universe, whose users stand in different orders, so it is the
  shared public part (Issue, Admit, Spend, the mints) that varies, with the state the claims before it left.
  Storage gas is constant (224,063 private, 2,053 plaintext).
- **LEZ burns.** Every burn costs the same.
- **Ethereum releases.** The first costs 105,461 gas, the 19 others 80,461: 25,000 more, the EVM's charge
  for a value transfer that creates an account, here the relayer's (a fresh address) on its first relay
  fee. The naive releases show the same.
- **Ethereum deposits.** Gas follows the leaf index: 139,740 for leaf 0; 100,062 to 100,428 for leaves 1, 3,
  7 and 15, which write a level of the tree's frontier for the first time; 80,162 to 80,406 otherwise. The
  sequence is the same in both universes, whose commitments and destinations differ.

So the receipts carry order, as the base fee does (it moves with the chain's history), and nothing that
tells one user from another; a client that pays for the first use of a relayer, or that is the first to
claim, is visible as such, which §B6's timing residual already covers.

### What is public at each step, against §B6

The full generated table, every field and state change with how it varies across the 20 records and its
class, is [generated/fields.md](generated/fields.md); [generated/b6.md](generated/b6.md) lays it against
§B6. Every item §B6 as first drafted listed as public is found in the data (deposit: depositor, token, denomination, time,
`C`; claim: `N`, token, denomination, time, the root read, the bridge program, the fee alias and its
reserve, and no destination, all destinations being private; burn: token, denomination, time, the
note; release: recipient, token, denomination, time, `NOut`, the root, the relayer; the root import). Every item §B6 lists as hidden was searched for in
every record's public bytes and not found (0 of 320). Everything else that varies is a nullifier, fresh
randomness, a user's own address or time. The differences, each a finding:

1. **The claim names its relayer, and the relay fee is minted to it publicly.** `Issue.relayer` and the
   relayer's wrapped balance, which grows by the relay fee in every claim's state diff. §B6's claim row
   as first drafted listed no relayer (the release row did; now listed). Constant across users here; a client that lets users pick
   relayers splits the claim's anonymity set by relayer.
2. **The claim names its fee schedule's leaf range** (`Issue.range`). Known (`BRIDGE.md` §2: the mint
   fee already shows the schedule); constant here, since all 20 leaves are under schedule 0. §B6 as first drafted did not list it (now listed).
3. **The fee reserve's refund account is public, and so is the refund.** `fee.refund_to` in every
   private claim and burn, and that account's balance in the state diff. §B6 as first drafted did not list either (now listed). In the harness
   it is one shared account; a wallet that named an account of its own would tie all its private
   transactions to that account (not to a deposit). The client must name none (LEZ then returns the
   unspent reserve to the alias as presented) or a shared one, and §B6 as first drafted did not say so (now said).
4. **The burn shows a fee alias with its fixed reserve too** (the `Credit` of 10^8 to the fee inbox,
   from an alias). §B6's burn row as first drafted listed only token, denomination, time and the note (now listed).
5. **LEZ's own private-transaction fields.** Every private claim and burn carries two private actions
   (the main one and the fee fallback), each with a nullifier, a new commitment, an encrypted note (a
   ciphertext of 137 to 247 bytes and a 1,088-byte ephemeral public key) and the commitment-set root it was
   proven against. They are nullifiers, fresh randomness and a state reference, and §B6 as first drafted listed none of
   them (now listed). The root shows which state the proof read: 80 of 80 private actions here were proven against
   the root just before their transaction, the newest. A wallet that proved a burn against an older root
   would point at the transaction that made the spent commitment, its own claim (inferred from the
   data's structure, not run).
6. **The deposit's `j` and leaf index are public** (`Deposit.index`, `Deposit.leafIndex`, and the write
   to `depositCount[depositor]`). §B1 says so; §B6's deposit row as first drafted did not list them (now listed). `j` is 0 for a fresh
   depositor; a repeat depositor's `j` counts its deposits.
7. **The release shows the registered relay fee** (constant per token and denomination). §B6 as first drafted did not list it (now listed).

Everything public about the root import is §B6's "the vault's root at a block" and the submitter.

## Other findings

8. **The bridge core's burn recovery could not find LEZ's notes (resolved).** `ferrata_bridge_core::outbound::recover_burns`
   checked the v1 mac (without the memo), while the notes LEZ records carry D12's v2 mac (over the memo): run
   over the 20 burns, it found 0 of 20 on `integration3`. The security fixes moved the core to the v2 mac
   (review I-4; the release image changed anyway with `NOut` v2, M-3), and on the merged programs it finds
   20 of 20, as the suite's own wallet does.
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
  randomness the comparison did not see. **Since measured** on the final programs with real proofs
  ([`real/RESULTS.md`](real/RESULTS.md), 12 November 2026): 20 of 20 claims and burns proven on one L4 and
  verified by LEZ's settlement, 20 of 20 releases checked by RISC Zero's Groth16 verifier; the gate passes,
  the adversary is at chance, and every part of the receipts and seals is classified and tested.
- **The Ethereum side of LEZ.** The vault's state reaches LEZ through synthetic records at genesis, as in
  all bridge-program tests; the deposit root is cross-checked equal, not attested from a real block.
- **Timing and amounts.** One token, one denomination, one relayer, every deposit before every claim and
  random orders by construction: the allowed signals are held uninformative, not measured. Real traffic,
  the fee-schedule split, cap-rejection retries and the root-age residual of §B6 are not exercised.
- **Naive inbound nullifier** is a substitution model, not an execution.
- **Privacy #4–#7.** Failure paths (Priv.5) are covered only by the vault's `Privacy.t.sol` (failures
  carry a selector only), not here; the gas drop and relayer switching (Priv.6) are outside this suite (the gas-drop receipt is built,
  [EVIDENCE](../EVIDENCE.md) rows 47, 48; its sealed credit is not; the `ferrata` tool names a claim's relayer per
  claim, rows 84, 85); client defaults (Priv.7: the `ferrata` tool and the mini-app, rows 84–86) and component
  documentation (Priv.4: [`docs/trust.md`](../trust.md) §8, the user guide §10) are covered there, not by this suite. Network metadata is out
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

The negative controls, on that run's raw data ([`CONTROLS.md`](CONTROLS.md); 68 s on 2 vCPU):

```sh
python3 -I scripts/privacy/controls.py target/privacy docs/privacy/controls
```

Raw data (about 4.4 MB: `lez.json`, `eth.jsonl`, `scenario.json`, `truth.json`) goes to
`target/privacy/` and is not committed; the run is deterministic, so it regenerates byte for byte.
