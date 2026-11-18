# The malicious-tenant test (RFP-022 F.16), 18 November 2026

`lez/harness/tests/tenant.rs`, three tests, run through LEZ v0.4.0-rc1's own state machine and fee settlement (the
in-process harness) with the frozen bridge program: each test asserts that the program it runs has the image id
`ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` of [`docs/image-ids.md`](../../docs/image-ids.md),
the image the live Sepolia vault's `configId` commits to. No guest was changed. The tests record the behaviour: each
asserts the outcomes below, so a change of behaviour fails them.

- **Where:** the Modal build box `ferrata-wt-tenant` (profile `ohyou`), 18 November 2026 about 13:45 UTC; dev mode
  (no proofs: the harness executes every transition and meters its cycles as LEZ does). `3 passed`, 195 s.
- **Re-check:** `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test tenant -- --nocapture --test-threads 1`.
  The tests rewrite [`tenant/r1.txt`](tenant/r1.txt), [`tenant/r1-frontrun.txt`](tenant/r1-frontrun.txt) and
  [`tenant/r2.txt`](tenant/r2.txt), the source of every figure here.
- **What is placed, not executed:** the attacker's records (R1) and the filled holder accounts (R2), as said below.
  Everything else is executed.

Both properties were found by an independent review of the LEZ v0.3.1 port of the bridge, which stated that the v0.4
program has them too. Both hold on v0.4 in kind; R2's figures differ (below).

## R1: a second configuration of the same vault freezes a public-kind deposit (holds on v0.4)

The cause, in the v0.4 source: a configuration's `attestation` and `instance_id` are free fields
(`lez/bridge-wire/src/lib.rs`, `BridgeConfig`), and the nullifier account's seed is `(chain, vault, N)` without the
configuration id (`nullifier_seed`). The attacker's configuration B copies the paired configuration A's fields and
names another instance, `0xbebe…be`, of the real attestation program. Anyone can create such an instance (its `Init`
takes any checkpoint with a matching bootstrap, `lez/guests/program/src/main.rs`, `init`), and its records are then
whatever the creator's own sync committee signs. **Placed:** B's record, at `header(0xbebe…be, 9,100,000)` of the
attestation program, as that program writes a record and as this harness places every record of the honest
instance. It says that vault A holds the root and count of A's eight real leaves plus one invented leaf, that slot 2
holds `configId(B)`, and ETH's registry entry.

| step (`tenant/r1.txt`) | outcome | execution gas |
|---|---|---:|
| B: `Init` (its own record: slot 2 = `configId(B)`) | applied | 170,246 |
| B: `Register` ETH | applied | 328,676 |
| B: `ImportRoot` n = 9 (8 real leaves + 1 invented) | applied | 229,165 |
| B: claim of A's public-kind deposit (leaf 2) with its `s` | applied | 257,798 |
| A: `Init`, `Register`, `ImportRoot` n = 8, two `ImportFees` | applied | 171,027 to 331,816 |
| **A: claim of the same deposit by its owner** | **refused, 250 `ALREADY_CLAIMED`** (fee charged: 1,728,360 base) | 213,992 |
| A: claim of A's other deposit (leaf 6, 10 ETH) | applied | 291,250 |
| B: claim of the invented deposit (1 ETH to the payer) | applied | 268,225 |
| B: the payer's burn of that 1 ETH | applied: `burn(configId(B), H(note))` written, `burn(configId(A), H(note))` empty | 162,063 |
| B: claim of A's private-kind deposit (leaf 5), `s` guessed | refused, 249 `CLAIM_INVALID` | 94,768 |
| B: the same as a public-kind deposit to the attacker, `s` guessed | refused, 249 | 94,768 |
| B: plaintext claim of leaf 5 with its real `s` | refused, 252 `WRONG_DESTINATION` | 98,394 |
| B: `ClaimPrivately` of leaf 5 with its real `s`, run as the attacker's own account | refused, 252 | 157,256 |

After it, `nullifier(chain, vault, N)` of leaf 2 records `configId(B)`, `holder(dest)` of leaf 2 holds one holding,
1 ETH under `configId(B)`, and the private-kind deposit's nullifier is unspent.

**Front-run** (`tenant/r1-frontrun.txt`). The harness settles one transaction per block, in the order submitted. The
payer's own plaintext claim of its deposit (leaf 6) is a 1,271-byte message that contains `s`. The attacker decodes
it, copies the witness into B's claim (B's root, imported beforehand, holds leaf 6 as well), and that claim is
ordered first: applied (257,783 gas). The payer's claim is then refused with 250 (214,975 gas). The payer holds
10 ETH under `configId(B)` and nothing under A.

**What it means.**
- *Who:* whoever knows the deposit's `s`: the depositor, and anyone who sees a plaintext claim before it is included.
  A private claim does not publish `s`: `bridge_private.rs` searches the private claim's transaction for it, for a
  private-kind and for a public-kind deposit. The CLI claims privately unless given
  `--i-accept-plaintext-links-deposit` (`cli/src/flow.rs`; that refusal has no test of its own). A private-kind
  deposit cannot be taken: 249 without `s`, 252 with it.
- *Cost to the attacker:* 728,087 execution gas once (B's `Init`, `Register`, `ImportRoot`), then about 258,000 per
  deposit, plus running an instance of its own.
- *No theft, no double mint.* B's mints and burns are under `configId(B)`, which no vault pins. The vault releases
  only against the burn root of a finalised checkpoint (`vault/src/FerrataVault.sol`, `_checkRelease`) and a release
  digest that binds its immutable `lezConfigId` (`_releaseJournalDigest`; the checkpoint digest binds it too,
  `_checkpointDigest`).
- *A freeze, and what the vault gives a frozen depositor.* The deposit can no longer be claimed under A. `refund`
  pays only a leaf at or above `sunsetLeafCount`, the imported count of the first sunset checkpoint
  (`FerrataVault.sol`, `refund`, and the finalisation that sets it), which counts only the paired configuration's
  imports (`checkpoint/crates/checkpoint-core/src/rule.rs`, `apply_message`). Once A has imported a root covering the
  deposit, which every claim under A needs, its escrow stays in the vault for good. It is refunded only if the vault
  is retired and sunset before A ever imports that leaf. The destination holds B's tokens, which nothing redeems.

## R2: a destination's holder account can be filled (holds on v0.4, with other figures)

The review's figure for v0.3.1 (1,505 holdings, then a panic at the 100 KiB shard limit) does **not** hold on v0.4:
LEZ v0.4.0-rc1 has no limit on an actor's state (`lee/state_machine/core/src/account/actor_state.rs` tests a state
beyond the former limit), and the v0.4 program's write has no size check. What holds: `holder(dest)` is one
account for every configuration, and every mint, burn and transfer there decodes, searches and re-encodes its whole
list, so its cost grows with the list until a transaction exceeds LEZ's 10,000,000 execution gas
(`MAX_GAS_EXEC`, the most a block can use). **Placed:** the `k` other holdings (0.1 ETH under `k` distinct
configuration ids each) at `holder(dest)`, then a real claim (A's leaf 2, 1 ETH) or a real burn by the owner.

| k other holdings | state bytes | claim to it | gas charged | base fee |
|---:|---:|---|---:|---:|
| 0 | 6 | applied | 260,775 | 2,102,624 |
| 1,505 | 102,346 | applied | 2,683,111 | 21,481,312 |
| 3,010 | 204,686 | applied | 5,108,009 | 40,880,496 |
| 6,048 | 411,270 | applied | 9,999,010 | 80,008,504 |
| **6,049** | 411,338 | **failed, no exit code** ("Execution exceeded its cycle budget"): nothing minted, the deposit unspent | 10,000,000 | 80,016,424 |

Bisection between 6,020 and 12,040 (every probe in `tenant/r2.txt`). The failure is not a refusal: the claim's
sender pays the whole limit, about 38 times the fee of the same claim to an empty holder. The owner's burn of a
holding it keeps there applies up to 6,109 other holdings (9,999,626 gas) and fails from 6,110.

**The attacker's cost** (derived from the measured gas): the attacker adds one holding per (configuration, token)
pair, each through a claim to `dest` under a configuration it creates. A claim costs 260,775 gas plus 1,610.2 per
holding already in the list (linear between 0 and 6,048 in the probes), and each pair needs a `Register`
(331,816). Reaching 6,049 then takes 6,049 claims (3.10 × 10¹⁰ gas) and 6,049 `Register`s (2.0 × 10⁹): about
**3.3 × 10¹⁰ execution gas**, at least 3,304 blocks at their maximum. The review's estimate for v0.3.1 was about
4.5 × 10⁸. Claims cannot push the list past 6,049, since their own cost rises with it. Not tested: whether transfers
into `dest`, whose other steps cost less than a claim's, can push it past 6,109, where the owner's burns fail too.

**What it means.** While the list is that long, no claim to `dest` applies, under any configuration. So a public-kind
deposit to `dest`, whose destination is fixed in its commitment, stays unclaimed. Private accounts' balances are
not in `holder(dest)`.

## The planned change

The nullifier account keyed by the configuration id as well, a holder account per configuration and destination,
and an oversized state refused with a code. It changes the bridge program's image id, so it is milestone B1's, and
this test becomes its regression test (the B2 gate's malicious-tenant test). The LEZ v0.3.1 port has already been
changed this way: branch `testnet-v03-bridge` at `ff453dcc` (`lez-v03-bridge/core/src/lib.rs`, its tests
`harness/tests/review_regressions.rs`), where the oversized state is refused with 255. See
[`docs/design/bridge.md`](../../docs/design/bridge.md) §3, "Known issues, 18 November 2026".
