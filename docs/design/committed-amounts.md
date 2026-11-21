# Committed amounts: the migration path (RFP-022 soft requirement 1)

The clause, verbatim: "**Committed amounts.** Record transferred values on the LEZ side as value commitments rather
than restricting transfers to a fixed set of permitted amounts, removing the amount-visibility constraint. This merges
all per-amount anonymity sets into one and reduces the need for users to split transfers. Whatever is delivered under
the hard requirements should be designed so this can be adopted later without redeploying the vault or resetting
accumulated anonymity; document the intended migration path even if it is not implemented."

**Status: not implemented, and the deployed design cannot adopt it without a new vault.** On either leg, committed
amounts need a new vault, and a new vault starts a new deposit tree, so the anonymity accumulated in the old one is not
carried over. This note is the migration path the clause asks for, checked against the code: why a second LEZ program
cannot take over claims from the deployed vault, the path that works for the deployed system (a new vault, the old one
retired), and what the next vault and program need so that a later switch is LEZ-side only. An earlier version of this
note (commit `a005299a`) said the inbound leg was adoptable without the vault; that was wrong, and the next section
says why.

## What shows an amount today

Every deposit, mint, burn and release is one registered denomination ([`bridge.md`](bridge.md) §13). What each step
publishes ([`bridge.md`](bridge.md) §17):

| Step | Chain | Shows the denomination because |
|---|---|---|
| Deposit | Ethereum | an ERC-20 transfer or `msg.value` is public on Ethereum by construction; no LEZ-side design changes it |
| Claim | LEZ | the bridge program mints the denomination and pays the mint and relay fees of that denomination in public; the LEZ caps count it |
| Burn | LEZ | the burn note carries `token, amount` and the burn leaf hashes them: `H(0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex ‖ token ‖ amount ‖ COut ‖ r)` |
| Release | Ethereum | the vault pays `amount`; public on Ethereum by construction |

So a claim hides among the deposits of its token **and denomination**, a release among the burns of its token and
denomination. Committed amounts remove the denomination from the two LEZ rows.

## Why the deployed system cannot hand claims to a second LEZ program

The withdrawn plan: deploy a second LEZ bridge program whose claims publish a value commitment instead of the amount,
let it claim against the same deposit tree from leaf 0, stop the deployed program's claims with `Sunset`, and have the
new program refuse any nullifier `N` the deployed one spent. Three facts of the deployed code make it unsound. Each is
shown by a test that exists and passes at this commit (run listed below):

| Fact of the deployed code | What it does to the withdrawn plan | Shown by |
|---|---|---|
| **The deployed program has no permanent stop for claims.** `Sunset` refuses `ImportRoot` only; claims against roots already imported, and burns, continue (`lez/guests/bridge/src/main.rs`, `import_root`; [`bridge.md`](bridge.md) §16 item 1). A freeze of mints lapses after `maxFreeze` (30 days of attested Ethereum time) unless the freeze authority renews it. | **Double mint.** A leaf below the deployed program's imported count is claimed on the new program first (the deployed program's nullifier for it is absent, so the new one mints), then on the deployed program, which checks only its own nullifiers and mints again. A renewed freeze would hold only for as long as someone keeps renewing it. | `lez/harness/tests/bridge_controls.rs`, `registry_fees_caps_freezes_and_sunset`: "ImportRoot B after Sunset" refused (242, `FROZEN`); "Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue" applied; "Burn 1 ETH (B) after Sunset: burns continue" applied; "Burn 1 ETH (A) after the Tick: the freeze has lapsed" applied. `lez/harness/tests/bridge_time.rs`, `freezes_and_caps_follow_attested_time_only`: "Claim B's 1 ETH: the freeze has lapsed" applied |
| **A `Sunset` the checkpoint program collects ends the vault.** The first finalised checkpoint with `sunset = true` fixes the refund boundary at its imported count; `revokeWindow` after it, anyone may `retire()` the vault, which stops deposits forever, and `refund` then pays every leaf at or above the boundary back on Ethereum. | The vault ends, so "without redeploying the vault" fails at once; and a new program that claims "from leaf 0" would also mint leaves at or above the boundary, which are refunded on Ethereum: the same deposit paid twice. | `bridge_controls.rs` (the checkpoint program's own parser collects the authority's `Sunset`); `vault/test/Checkpoint.t.sol`, `test_finalize_sunsetCheckpointSetsTheRefundBoundary`; `vault/test/Roles.t.sol`, `test_retireOpensAt_theEarlierOfKeylessAndSunset`; `vault/test/Refund.t.sol`, `test_retire_isOneWayAndStopsDepositsForever`, `test_refund_paysTheDepositorAndReducesEscrow` |
| **The vault releases only burns of its own configuration.** `configId` hashes the bridge program's image id and its header account (`checkpoint/crates/checkpoint-core/src/state.rs`, `Config::config_id`), and the checkpoint program collects only that configuration's burns (`rule.rs`, `apply_message`). | Tokens a second program mints can be burned only on that program, whose burns the deployed vault never releases: the escrow backing them would stay in the vault with no exit. | `checkpoint/crates/checkpoint-core/tests/redteam.rs`, `redteam_checkpoint`, case CKP-12: a burn of another configuration in the same block is accepted and changes neither the burn count nor the burn root |

So both legs need a new vault. The outbound leg needs one in any case: a committed burn publishes a value commitment
instead of `amount`, so its leaf changes, so the release guest that reads burn leaves changes, and the vault pins that
guest's image id as an immutable (Security #8).

## The path for the deployed system: a new vault, the old one retired

1. Deploy a new vault and a new LEZ bridge program built for committed amounts on both legs, with the hooks of the
   next section, so that no later change of the amount format needs another vault.
2. Retire the old configuration as [`bridge.md`](bridge.md) §16 describes: `retire()` on the old vault (deposits stop)
   and `Sunset` on the old program (imports stop). The old program's imported count is then fixed at some `X`.
3. Old deposits: every leaf below `X` stays claimable on the old program, denominated, as today; every leaf at or
   above `X` is refundable on Ethereum once the sunset checkpoint can no longer be revoked. Old holders burn on the
   old program and release from the old vault, which keeps finalising checkpoints after retirement.
4. New deposits go to the new vault. Committed claims hide among the new vault's deposits only.

What this path gives up: the vault is redeployed, and the anonymity accumulated in the old vault is not carried over.
Neither half of "without redeploying the vault or resetting accumulated anonymity" is met for the deployed vault.

Its safety claims, and what shows each:

| Claim | Shown by |
|---|---|
| After `Sunset`, the old program imports no root, so its imported count `X` no longer moves | `bridge_controls.rs`: "ImportRoot B after Sunset" refused (242); what the checkpoint collects of that refused import: nothing |
| A claim refuses a leaf at or above the imported count | `lez/harness/tests/bridge_inbound.rs`, `inbound_configuration_import_and_plaintext_claim`: "Claim: leaf 6 under root(A, 6) (block 102: index >= count)" refused (248, `LEAF_NOT_IMPORTED`) |
| A refund refuses a leaf below the boundary, and pays nothing before retirement or before the sunset checkpoint's revocation window has passed | `Refund.t.sol`: `test_refund_impossibleBelowTheSunsetLeafCount`, `test_refund_refusedBeforeRetirement`, `test_refund_opensRevokeWindowAfterTheSunsetCheckpoint` |
| Burns continue after `Sunset`; checkpoints and releases continue after `retire()` | `bridge_controls.rs`: "Burn 1 ETH (B) after Sunset: burns continue"; `Refund.t.sol`: `test_retire_leavesCheckpointsAndReleasesWorking` |
| The new vault's configuration is apart from the old one's | its `configId` hashes the vault address (`Config::config_id`); CKP-12 above |
| Every old leaf is claimable on LEZ or refundable on Ethereum, never both, over one whole retirement | **Not shown as one test.** It follows from the rows above ([`bridge.md`](bridge.md) §16, last paragraph); no test on this branch runs a sunset, a claim below `X` and a refund at or above `X` against one vault and one program together |

## What the next vault and program need, so that the switch is LEZ-side only

**None of this is built; every claim in this section is unproven.** The vault redeployment already required for the
deployed program's two known issues (B1, [`bridge.md`](bridge.md) §3, "Known issues, 18 November 2026") is the moment
to add it.

1. **A hand-over of claims that is not a sunset.** A one-way `HandOver { successor }` on the bridge program, approved
   as `Sunset` is, after which the program refuses every `Claim` and `ClaimPrivately` for good (unlike a freeze, it
   does not lapse) and keeps taking burns. The checkpoint program does not count it as a sunset, so the vault's refund
   boundary and retirement are untouched.
2. **The successor reads its predecessor's nullifiers.** Before minting, the successor reads the predecessor's
   nullifier account for `N` (the deployed program answers `Read`, `BridgeMessage::Read`) and refuses a spent `N`; it
   claims nothing until it has read the predecessor's hand-over to it. Since the predecessor refuses every claim after
   the hand-over, an `N` absent there stays absent: no double mint. The successor claims against the same deposit tree
   from leaf 0: no reset of anonymity. Two properties this rests on are already true and tested: each deposit leaf
   binds its amount, and `N` does not depend on the LEZ program (below).
3. **The vault follows the hand-over.** The checkpoint program collects the successor's burns, imports and sunset after
   the hand-over (today `configId` binds one bridge image and one header account), and the vault pins, from its
   deployment, a release image for committed burn leaves, unused until the switch.
4. **Caps.** The LEZ mint caps count public amounts. With committed mints they move to the vault's deposit caps (already
   per token and global on Ethereum, `vault/test/Caps.t.sol`), or the successor sums commitments homomorphically with
   a range proof per claim (Pedersen commitments; a larger proof).
5. **Fees.** A committed claim cannot pay fees proportional to a hidden amount in public: fees per claim become fixed,
   or are taken on Ethereum only, where the vault already escrows them per deposit.

The deposits stay fixed denominations on Ethereum (they are public there anyway); what changes is that a claim no
longer says which one.

## What is built and tested toward this

Nothing of the committed-amount programs. Two properties the next version's inbound path rests on are tested: the
deposit leaf binds the amount (`bridge/crates/ferrata-bridge-core/tests/inbound.rs`,
`every_single_field_change_fails_or_changes_the_nullifier`: a claim with any one field of its deposit changed either
fails or yields another nullifier) and the nullifier is a function of the deposit and the user's secret only
(`tests/vectors.rs`, against `bridge/vectors/`).

## The run that shows the tests cited here pass

Measured on the Modal build box, 21 November 2026, branch `rfp-softs` at `8b0a967e` with this note's text only
changed; nothing was sent to any network:

| Command (from the directory named) | Result |
|---|---|
| `lez/`: `RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_controls --test bridge_inbound --test bridge_time` | 1 + 3 + 1 passed |
| `vault/`: `forge test --offline --match-test` the eight vault tests named above | 8 passed (Checkpoint 1, Refund 6, Roles 1) |
| `checkpoint/`: `cargo test -p checkpoint-core --locked --test redteam` | 1 passed (`redteam_checkpoint`, CKP-12 among its cases) |
| `bridge/`: `cargo test -p ferrata-bridge-core --locked --test inbound --test vectors` | 9 + 7 passed |
