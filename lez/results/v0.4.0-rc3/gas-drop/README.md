# The gas drop on LEZ v0.4.0-rc3: M0 check 2

M0's check 2 asks for "the gas drop: a private claim that sweeps a sealed native credit and pays its fee from it".
LEZ's newest tag on 16 November 2026 is still `v0.4.0-rc3` (`bafd07be…`; `gh api repos/logos-blockchain/logos-execution-zone/tags`
and `/releases`, read that day). Everything below ran on 16 November 2026 on a Modal build box (16 physical cores, the
provers at `RAYON_NUM_THREADS=32`), on **rc3's own `sequencer_service`** (tree unmodified) with **its Logos Blockchain node
0.3.5-rc.1**, one validator, leader proofs in dev mode as LEZ's container runs them, the sequencer **without
`RISC0_DEV_MODE`** (`session/config/sequencer-process.txt`): it verified both receipts.

**Result: passed, 8 of 8 checks** (`summary.json`, written by `lez/scripts/gas-drop-summary.py` from the records;
`python3 -I scripts/m0.py` prints M0.2 GO from it).

| | what the records show |
|---|---|
| the claimant before | a new wallet (`hook/keys.json`): its private account (deposit j1's destination), a private one-time account and a public account. At block 48: both private accounts 0 in its wallet, its public account and both private ids 0 on the chain, nothing pending to it (`hook/prepare.json`) |
| negative control | the same `ClaimPrivately`, fee payer the one-time account, before any credit: LEZ's wallet refused it before proving, nothing sent: "Fee reserve of 6188624 from payer 6r8S… fails, though the transaction runs without it: Invalid native transfer: sender 6r8S… holds less than the transferred amount" |
| the receipt | the claimant's gas-drop receipt (guest `gasdrop`, `0x89580416…`, docs/image-ids.md) proven for real in 12.9 s, 76,230 user cycles, 583,923 B (JSON), verified; its journal binds `N` `0x41436b7f…`, the one-time address and the relayer |
| the relayer | `ferrata-gasdrop relay` runs the relayer's built `ferrata_cli::relayer::gas_drop`: the receipt checked by `check_gas_drop` (image id, this relayer, chain and vault, the imported roots, `N` unclaimed, no drop yet), the drop recorded, then one credit of **20,000,000** sent at 17:19:39 UTC. Refused afterwards: the same receipt again ("this N already had its drop"), at another relayer ("bound to another relayer"), and after the claim ("already claimed") |
| the sealed credit | from the relayer's private account to the one-time account, **proven for real in 253.8 s**, 253,340 B (receipt 238,233 B), applied in **block 76**, final 12.5 s after it was published; the claimant's wallet reads it from the chain as a `Publication::Sealed` of 20,000,000, with no recovery binding at the one-time address |
| the claim | `ClaimPrivately` of deposit 1 on the final bridge (`0xccde6171…`), fee payer the one-time account, relayer named: the wallet's dry run claims that credit before the root (1 claim, 20,000,000 swept, reserve 10,056,514); **proven for real in 308.7 s**, 252,589 B (receipt 237,760 B, 7 private actions), applied in **block 109**, final on Bedrock 14.5 s after it was published; `N` spent |
| after | fee charged **6,188,624**; the credit's remainder: **9,943,486** in the one-time account's note, **3,867,890** refunded to a fresh blinded alias of it (a pending credit its next send sweeps): 20,000,000 = 9,943,486 + 3,867,890 + 6,188,624. The claimant's private account holds 0.001 ETH wrapped and no native LEZ; its public account 0. The relayer's holder account received the relay fee, 2,000,000,000,000 wei of wrapped ETH, paid by the claim itself |

Transaction hashes: the credit `36960385…bbfb`, the claim `426a246c…56a6` (`hook/credit.json`, `hook/claim.json`).

## How it ran

`lez/live/run.sh run` (phases `info hook export`) with the hook `lez/live/hooks/gasdrop.sh`: crown-driver `crown-a`
deploys the attestation program and the bridge (the rc1 builds of docs/image-ids.md, sha256 and `r0vm --id` checked);
`gas-drop-keys` makes the claimant's wallet; a local anvil gets the vault tests' mock verifier, FerrataVault v5 and two
deposits (j1 to the claimant's private account), each captured; `crown-b --operator-only yes` (the updates, `Init`,
`Register` ETH, `ImportFees`, `ImportRoot` n=1, n=2); then `gas-drop-prepare` (balances, negative control),
`ferrata-gasdrop prove`, `ferrata-gasdrop relay` (its credit command is crown-driver `gas-drop-credit`), `gas-drop-claim`
(`lez/crown/driver/src/gasdrop.rs`, `cli/src/bin/gasdrop.rs`). Session 16:50–17:30 UTC.

Two things the code had to do that the built pieces did not:
- **The credit.** The built `impl Credit for lez::Sequencer` sends a public transfer to `PublicNoSign(oneTimeAddress)`:
  a public balance, which no private claim can sweep. A sealed credit (LEZ's `Publication::Sealed`, "contents and
  metadata shielded") is made only by private execution (`program_facades/mod.rs` 25–47 at the tag), so the relayer
  sends it from a private account, as LEZ's own test `integration_tests/tests/sealed_messages.rs` does, and needs the
  one-time account's public keys (npk, 32 B; vpk, 1,184 B), not only its address: `gas-drop-credit` refuses keys that
  do not derive the receipt's one-time address. Here the relayer's private account is LEZ's test private account 1,
  funded by its genesis pending credit, which the same send claims; LEZ's `NativeTokenTransfer::transfer` checks the
  sender's note before such claims count, so the send is the facade's message without that pre-check.
- **The sweep** is LEZ's wallet's own (PR 985): a private send claims the pending native credits to its private
  accounts and its fee payer before the root, and the proof's fee transfer runs after the root
  (`lez/wallet/src/lib.rs` 1615–1644, 1774–1808; `execution_state.rs` 1066–1106). The claim names the one-time
  account as its fee payer (`FeeOnly`: it authorizes only the fee entry, `account_manager.rs` 474–493).

**The relay fee.** The final bridge pays it: `ClaimPrivately { relayer: Some(R) }` credits the relay fee of the claimed
schedule to `holder(R)` in the wrapped token (`lez/guests/bridge/src/main.rs`, `issue`), in the same transaction;
nothing in the programs was changed.

## What the relayer and an observer learn (from this run)

- **The relayer**: at 17:19:39 UTC the receipt's journal (the imported root, the chain and vault, `N`, the one-time
  address, itself) and the one-time account's public keys; its own credit transaction (block 76). From the chain, the
  claim (block 109): it spends `N`, names the relayer and pays it the relay fee; the claim's public steps carry the token
  and the denomination, as every claim's do. It never received the deposit's leaf, depositor or secret, nor the
  claimant's private account (the deposit's destination).
- **An observer**: the credit is a privacy-preserving transaction with one sealed publication (no address, no amount)
  and a fee alias; the claim a privacy-preserving transaction that spends a message (its receipt nullifier) and
  publishes a refund of 3,867,890 to a blinded alias (amount and alias public; the account behind it is not). Nothing
  links the two but timing: here 33 blocks apart, and this zone held one drop and one claim, so in this run the pair
  is the only candidate. The anonymity set is the number of drops and claims in a window, not measured here.

## What this does not show

- One local validator, dev-mode leader proofs, not Logos's devnet; the Ethereum side is a local anvil with the vault
  tests' mock verifier, finalized by a stand-in committee; nothing was sent to a public network.
- The relayer's check ran offline in `ferrata-gasdrop relay`: the imported roots and the claimed nullifiers were read
  from the bridge's accounts by crown-driver and handed over as files; the daemon `ferrata-relayer` was not used (its
  zone links LEZ v0.4.0-rc1's wallet).
- The relayer's funds come from LEZ's test private account (a genesis credit), not from a relayer's own account.
- The proofs ran on 32 threads of a cloud box, not a laptop (M0 check 1 measures that).
- One drop, one claim: no anonymity set; burns using the same drop are not shown.
- `verify-run.sh` (the live driver's replay) refuses this session ("no program header": it expects the live driver's own
  deployment, which this session skipped); the blocks, `chain.tsv` and crown-driver's export are recorded.
- The rc3 node stalled once more (LEZ issue 1109): storage gas price 2→3 at 16:57:11, nothing final after block 34
  for 20 min, then `Head rewound from 154 to 34` (17:17:03); the drivers waited through it and the transactions were
  included again.

## The first attempt (`session-try1/`)

15:38–15:56 UTC: the negative control refused the same way; the receipt proven in 11.7 s; the credit proven in
224.8 s and sent, but crown-driver read the sequencer's head before the proof, so its 4-block wait had expired at
submission: it called the credit dropped, and the sequencer validated and included it 5 s later
(`session-try1/logs/sequencer.log`). The relayer's book recorded the drop as failed, as `gas_drop` does for a credit
that may have been sent. Fixed (commit `06ddc09f`: the head read after the send, as crown-b does) and the session
ended. A second run was lost with the box (no snapshot) before its records were copied; this session is the third.

## Files and the re-check

`summary.json`; `session/`: the zone (`environment.md`, `config/`, `logs/` with the sequencer's and the node's logs,
`chain.tsv`, `blocks.borsh`) and `hook/`: each step's JSON and log (`keys`, `prepare`, `receipt-proof`, `relay*`,
`credit`, `claim`), the proven transactions (`credit-tx.borsh`, `claim-tx.borsh`), the receipt (`gasdrop-receipt.json`),
the relayer's drop book, the wallets' RPC transcripts (`private/`), crown-driver's records (`crown-*.log`, `export/`)
and the Ethereum side (`eth/`). The claimant's wallet (its mnemonic) and the guest's input (the deposit's secret) are
not recorded. `environment.md`: the binaries.

```sh
python3 -I lez/scripts/gas-drop-summary.py lez/results/v0.4.0-rc3/gas-drop   # exit 0: passed, 8 of 8
python3 -I scripts/m0.py                                                     # M0.2 from summary.json
```

Run it again (a build box; about 45 min, two real proofs of about 5 min): `spike/tmp/m0-gasdrop/boxG.sh` beside the
repository does the builds and `lez/live/run.sh run` with `FERRATA_LIVE_HOOK=lez/live/hooks/gasdrop.sh`,
`FERRATA_LIVE_PHASES="info hook export"`, `GASDROP_ELF` (the `gasdrop` guest), `GASDROP_TOOL` (`ferrata-gasdrop`), the
final `ferrata_lez.bin` and `ferrata_bridge.bin`, LEZ v0.4.0-rc3 selected (`lez/scripts/lez-select.sh v0.4.0-rc3`).
