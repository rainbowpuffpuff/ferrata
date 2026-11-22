# The `ferrata` tool under every freeze (RFP-022 U.08)

**Run:** `cd cli && bash tests/e2e.sh` (the test `deposit_scan_claim_burn_release_and_recovery` of
[`cli/tests/e2e.rs`](../../../cli/tests/e2e.rs)), on a Modal build box (CPU), 21 November 2026, the sources of commit
`dfd4e2e4` (branch `rfp-relayer-robustness`). **Result:** `test result: ok. 1 passed; 0 failed` in 34.47 s
([`test.log`](test.log), the test's own output; the build's lines before it are left out and the checkout's path is
written `<repo>`). The library's unit tests at the same commit: 17 passed (`cargo test --lib`).

**Where it ran:** FerrataVault v5 on a local anvil (chain id 1, never a public network) with the vault tests'
`MockRiscZeroVerifier`; the zone in the LEZ v0.4.0-rc1 in-process harness (LEZ's own state transition with the bridge
guest, RISC Zero dev mode). Proofs are not what U.08 is about; none is real here. The vault's freeze authority is an
address no one holds a key for (`vault/script/CliLocal.s.sol`), impersonated by anvil; the bridge program's freeze
authority is the harness's role key (`hb::FREEZE_SEED`), as in the mini-app's admin view.

## What the tool says (verbatim, from `test.log`)

Each freeze is set by its freeze authority, the user's command is run, the refusal is checked (its source and code,
`frozen until`, what to do, and **no** hash, address, nullifier, leaf index or burn counter: the test's
`names_nothing`), the freeze is lifted, and the same command then goes through (the test continues with it).

| Freeze | Command | Refusal |
|---|---|---|
| vault deposits, whole vault (`setDepositsFrozen`) | `ferrata deposit` (the binary) | `refused: vault error DepositsAreFrozen() (checked before sending: frozen until Ethereum time 1797871892 (2026-12-21 16:51:32 UTC), unless the freeze authority lifts it sooner): deposits are frozen for the whole vault; nothing is sent: retry once the freeze ends (`ferrata status` shows it)` |
| vault deposits of ETH (`setTokenDepositsFrozen`) | `ferrata deposit` | `… TokenDepositsAreFrozen() …: deposits of this token are frozen; nothing is sent: retry once the freeze ends …` |
| bridge mints of ETH (`Freeze`, token) | `claim` (plaintext path) | `refused: LEZ bridge program exit code 242 FROZEN (checked before sending: mints (claims) of this token are frozen until Ethereum time 1808000769 (2027-04-17 22:26:09 UTC as the bridge's attested clock counts it; a freeze authority may lift it sooner); nothing is sent and nothing is consumed: the deposit stays claimable): … retry once the freeze ends (`ferrata status` shows the freezes)` |
| bridge mints, whole configuration (`Freeze`, no token) | `claim` (plaintext path) | `… mints (claims) of the whole configuration are frozen until … the deposit stays claimable …` |
| bridge burns of ETH | `burn` (public holding) | `… exit code 242 FROZEN (checked before sending: burns of this token are frozen until …; nothing is sent and nothing is burned: the holding stays yours) …` |
| bridge burns, whole configuration | `burn` | `… burns of the whole configuration are frozen until … the holding stays yours …` |
| vault releases, whole vault (`setReleasesFrozen`) | `release` (self-sent) | `refused: vault error ReleasesAreFrozen() (checked before sending: frozen until Ethereum time 1797872813 (2026-12-21 17:06:53 UTC), unless the freeze authority lifts it sooner): releases are frozen for the whole vault; the burn stays releasable: retry once the freeze ends (`ferrata status` shows it)` |
| vault releases of ETH (`setTokenReleasesFrozen`) | `release` | `… TokenReleasesAreFrozen() …: releases of this token are frozen; the burn stays releasable …` |
| bridge mints, whole configuration | `claim` (private path, before its proof) | `… mints (claims) of the whole configuration are frozen until … the deposit stays claimable …` |

The LEZ times are the bridge's attested clock (the gate's `now_ms`, moved only by records of Ethereum time), which
in this test follows anvil's clock after the test moved it forward; the vault's are Ethereum block time.

## What changed in the tool for this (commit `dfd4e2e4`)

- `cli/src/lez.rs` `gate_refusal`: before a claim or a burn is built (so before any proof), the token's gate and the
  configuration's gate are read from the zone and checked exactly as the program's `Gate::admit` checks them; a
  frozen gate is refused with code 242, its end, and what to do. `status` prints both gates' freezes in words.
- `cli/src/flow.rs`: the vault's deposit freezes (whole vault, token) are checked before a deposit is signed, as the
  release freezes already were; both name the freeze's end as a UTC date.
- `cli/src/codes.rs`: the freeze codes' meanings say what to do (retry once the freeze ends; nothing consumed).
