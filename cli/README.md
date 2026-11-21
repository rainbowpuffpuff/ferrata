# `ferrata`: the command-line tool of the Ferrata bridge

**Prototype, unaudited. Run it only against a local chain or a test deployment.**

`ferrata` is the tool a user or a relayer runs for the Ferrata bridge between Ethereum and a LEZ
zone. It replaces the test drivers (`bridge/e2e`, `lez/live/driver`, the vault scripts) for the
user's steps: derive the keys, deposit, find one's deposits and burns, claim, burn, release, recover
everything from the seed, and read the bridge's configuration. Every command that sends is a dry run
until `--send --yes`, and its defaults are the private choices (RFP-022 U.05–U.09). The commands' logic
is in `src/flow.rs`, which the binary and its end-to-end test both call; what each step makes public is in
`src/privacy.rs`, the same table and rules as the mini-app's `qml/js/privacy.js`. It is a thin layer over
libraries that already exist:

| Part | Library |
|---|---|
| Commitments, nullifiers, trees, the claim and release statements, recovery | `ferrata-bridge-core` (`bridge/crates/`) |
| The bridge program's messages, states, accounts and checks (`verify_claim`, fees, ranges) | `ferrata-lez-bridge-wire` (`lez/bridge-wire/`) |
| The attestation program's accounts and records | `ferrata-lez-wire` (`lez/wire/`), `ferrata-core` |
| LEZ: keys, the encrypted wallet, signing, private execution, the sequencer's RPC | LEZ v0.4.0-rc1's own `wallet` crate (`WalletCore`), as `lez/live/driver` uses it |
| Ethereum: keystore files, signing, JSON-RPC, the vault's ABI | `alloy` 1.x |
| The release guest (execute or prove) | `risc0-zkvm` 3.0.5 client, `r0vm` 3.0.5 over IPC |

## Installation

Linux x86-64. The tool links LEZ's crates by path, like `lez/`, and LEZ's wallet links the PC/SC library
(`pcsc-sys`): install `libpcsclite-dev` first (Debian and Ubuntu; `pcsc-lite-devel` on Fedora), or the build
stops at "The system library `libpcsclite` required by crate `pcsc-sys` was not found".

```sh
# LEZ v0.4.0-rc1 (commit 1806e87) beside the repository, at ../upstream/lez-v0.4.0-rc1 (see lez/RESULTS.md §8)
cd cli
cargo build --release          # Rust 1.98.1 from cli/rust-toolchain.toml (LEZ's host toolchain)
./target/release/ferrata --help
```

`release --prove` also needs `r0vm` 3.0.5 (`rzup install r0vm 3.0.5`). Building the tests needs
more (see [Tests](#tests)). `ferrata` has its own Cargo workspace, because it links LEZ's wallet and
node-side crates and alloy, which the rest of the repository avoids. LEZ's wallet comes with the feature
`lez-wallet` (default): without it (`cargo build --lib --no-default-features`) the library keeps the Ethereum side,
the zone's reads and calls over any `lez::Ledger` and the relayer daemon (`relayerd`) with the devnet's zone, which is
how `ferrata_relayer_module` builds it; the `ferrata` binary needs the feature, `ferrata-relayer` then lacks
`--sequencer`. The feature `seqnet` (with `lez-wallet`) adds the devnet tools of [`e2e/seqnet`](../e2e/seqnet/README.md):
`seqnet-fund` (the gas drop's stand-in), `seqnet-txs` (the zone's transactions) and `seqnet-chain` (the mini-app's
chain service).

## Files and settings

Every command takes these options; each has an environment variable. The endpoints are only these
options: nothing is built in.

| Option | Variable | What |
|---|---|---|
| `--seed FILE` | `FERRATA_SEED` | The seed file: the BIP-39 mnemonic of the LEZ wallet. Keep it `chmod 600` (the tool warns if others can read it). Everything is derived from it. |
| `--eth-rpc URL` | `FERRATA_ETH_RPC` | Any Ethereum JSON-RPC endpoint. The tool checks what it downloads against the vault's own roots. |
| `--chain-id N` | `FERRATA_CHAIN_ID` | Refuse to run unless the endpoint serves this chain (1 mainnet, 11155111 Sepolia). |
| `--vault ADDR` | `FERRATA_VAULT` | The vault's address (the tool checks there is a contract there). |
| `--from-block N` | `FERRATA_FROM_BLOCK` | The vault's deployment block (where its events start). |
| `--chunk N` | | Blocks per `eth_getLogs` request (default 10,000). |
| `--sequencer URL` | `FERRATA_SEQUENCER` | The LEZ sequencer's RPC endpoint. |
| `--lez-wallet DIR` | `FERRATA_LEZ_WALLET` | The LEZ wallet's home: its config and its storage, encrypted under its password. This is the LEZ keystore. |
| `--lez-password-file FILE` | `FERRATA_LEZ_PASSWORD_FILE` | The LEZ wallet's password. |
| `--bridge-account HEX` | `FERRATA_BRIDGE_ACCOUNT` | The bridge program's account id on LEZ. |
| `--gap N` | | Accounts of each kind searched for deposit destinations (default 8). |
| `--json` | | Print one JSON object instead of the report (see [For integrators](#for-integrators)). |

`claim` and `burn` also take `--bridge-program FILE` (`FERRATA_BRIDGE_PROGRAM`): the bridge program's
binary, which LEZ's wallet runs privately and proves locally for a private claim or burn.

Commands that sign on Ethereum take `--keystore FILE` (a Web3 Secret Storage file, the format of
`cast wallet import` and geth) and `--password-file FILE`. **No key is ever passed on the command line
or in an environment variable.**

**Dry runs.** A command that sends does nothing unless it gets both `--send` and `--yes`. Without
`--send` it prints, and stops:
- **what it would send**: the call, its target and its data (on Ethereum the exact `to`, `value` and
  calldata; on LEZ the message, the program and how it is signed or proven);
- **what becomes public**: what the step publishes on each chain and what it keeps private, then
  "privacy of these choices" (Minimal, Elevated or High) with one line per choice, and a `WARNING` when a
  direct link would become public;
- **its cost**: the amounts in the token's units, the fees in force, and on Ethereum the network fee from
  `eth_estimateGas` and `eth_gasPrice` (after an `eth_call` that would show a refusal).

With `--send` but no `--yes` it refuses, after printing all that.

**Defaults** (RFP-022 U.05, U.06). The private choice is preselected everywhere; each weaker one is an
option, and a public path also needs its own acknowledgement:

| Step | Default | The other choice | What it costs in privacy |
|---|---|---|---|
| Deposit destination | private account `/0` | `--kind public --i-accept-public-account` | the account and its balance become public when claimed (High) |
| Claim | private (`ClaimPrivately`, proven by LEZ's wallet) | `--plaintext --i-accept-plaintext-links-deposit` | links the mint to its deposit for everyone (High) |
| Claim timing | held 1–12 h after the deposit (from the seed) | `--no-delay` | the claim's time narrows which deposit it could be (Elevated) |
| Burn | from private account `/0` | `--from public --i-accept-public-account` | the holder, its balance and the burn are public (High) |
| Release | through any relayer, paid the registered relay fee | `--self-send` (from `--keystore`) | your gas payer is linked to the recipient (Elevated) |
| Release root | the newest accepted root whose release delay has passed | none | |

The claim delay is `1 + (HMAC-SHA256(bridgeSeed, "ferrata/claim-delay/v1" ‖ N) mod 12)` hours: the
mini-app draws 1–12 h at random per deposit; the tool derives it, so it is the same on every run and after
a restore, and no one without the seed can predict it. It is measured in Ethereum blocks (12 s each, a
lower bound of the time passed), from the deposit's block in the logs the tool downloads anyway, so no
provider is asked about any one deposit.

**Keys.** The LEZ wallet's seed gives its public and private key trees. The tool uses account `/a` of
each tree as deposit destinations, for `a` below `--gap`. The bridge keys come from one label of its
own: `bridgeSeed = HMAC-SHA256(LEZ seed, "ferrata/bridge-seed/v1")`. `kAuth` (authorises claims and
releases) and `kView` (shows burns and recipients) are derived from `bridgeSeed`. The label is
disjoint from LEZ's own derivation paths.

**Amounts.** Every amount is printed in the token's units, followed by base units and decimals, for
example `0.1 ETH (100000000000000000 base units, 18 decimals)`. Decimals and the symbol are read from
the token contract (18 and `ETH` for ETH). Amounts typed by the user (`burn --amount 0.1`) are parsed
exactly; extra decimal places are refused.

## For integrators

`--json` makes every command print one JSON object on stdout and nothing else: the command, the report's lines
(the same text, so what becomes public is still there) and a structured result. An error prints
`{"error": …, "report": […]}` and exits with status 1. From the end-to-end test of 12 November 2026 (shortened, the lists elided):

```text
$ ferrata --json … scan
{"command":"scan","report":["vault 0xCf7E…0Fc9: 16 deposits at block 22, root 0x9e1b…8936 (equal to depositRoot: checked)", …],
 "result":{"deposits":11,"claimed":0,"refunded":0,"unclaimed":[{"kind":"public","account":0}, …],"burns":0,"released":0,"next_burn_counter":0,"holdings":[]}}
$ ferrata --json … deposit --denomination 0 --from 0x…
{"command":"deposit","report":[…, "privacy of these choices: Minimal: only the residual correlations (token, denomination, time)", …],
 "result":{"commitment":"0xdd40b17e…61a4","nullifier":"0x…","j":11,"sent":null}}
$ ferrata --json … --vault 0x000000000000000000000000000000000000dEaD scan
{"error":"no contract at 0x000000000000000000000000000000000000dEaD on chain 1: check --vault and --eth-rpc","report":[]}
```

The results: `deposit` {commitment, nullifier, j, sent {tx, leaf}}; `scan` and `recover` {deposits, claimed,
refunded, unclaimed [{kind, account}], burns, released, next_burn_counter, holdings [{kind, account, token,
amount}]}; `claim` {nullifier, root_leaf_count, private, sent}; `burn` {burn_counter, r, c_out, sent}; `release`
{nullifier, burn_root, checkpoint, calldata, job, sent}. A Rust program can call the same functions as the
binary (`ferrata_cli::flow`), over any zone that implements `lez::Ledger`: the binary's is LEZ's wallet on a
sequencer, the test's is the LEZ harness.

## Refusals

A command checks what the program or the vault would refuse before it signs:
- on LEZ it runs the bridge program's own checks from `ferrata-lez-bridge-wire` on the accounts it reads;
- on Ethereum it reads the registry, freezes and roots, and runs `eth_call` and decodes the vault's custom
  error from its selector.

Every refusal names its source, its code, the code's meaning and where it was found. None names the
deposit or the burn it was about (U.08). All 16 program codes (240–255) and 40 vault errors are in
`src/codes.rs`; a unit test checks the program codes against the bridge wire's constants, another decodes
every vault error from its selector. The end-to-end test checks these, each through the binary or the
library function the binary calls (output of the run of 12 November 2026, below):

| RFP-022 U.08 | The tool's message |
|---|---|
| unsupported token | `refused: vault error TokenNotRegistered() (checked before sending): the token is not registered` |
| invalid amount | `refused: vault error InvalidDenominationIndex() (checked before sending): no denomination has this index`; `burn --amount 0.5`: `refused: LEZ bridge program exit code 251 NOT_REGISTERED (checked before sending: the amount is not one of the token's denominations) …`; `--amount 0.1000000000000000001: … has more than 18 decimals` |
| cap exceeded | a deposit's dry run under a 1-wei cap: `… the call does not pass eth_call now: refused: vault error TokenDepositCapExceeded() …` |
| verification failure | the program's `verify_claim` code (249, 248, 252), checked before sending |
| insufficient finality | a claim before its root import: 245 or 248 (`… no imported root covers this leaf yet; wait for the next root import`); a release: `refused: vault error ReleaseDelayNotElapsed() (checked before sending: the roots that cover this burn are in their release delay; the first opens at Ethereum time 1794439552, in 121 s) …`, or `BurnRootNotAccepted` when no checkpoint covers it |
| already claimed | `refused: LEZ bridge program exit code 250 ALREADY_CLAIMED (checked before sending): the deposit's nullifier is spent: it was claimed already` |
| already released | `refused: vault error NullifierAlreadyUsed() (checked before sending: the vault's Release events show it released) …` |
| frozen | `ReleasesAreFrozen`, `TokenReleasesAreFrozen` (checked before sending), program code 242 `FROZEN`; not exercised by the test (the test vault's freeze authority has no key) |
| over the holding | `refused: LEZ bridge program exit code 254 INSUFFICIENT_BALANCE (checked before sending: the holding is 0 base units) …` |
| a public path without its acknowledgement | `refusing a public destination without --i-accept-public-account: …`; `refusing the plaintext path …`; `refusing to burn from a public account …` |
| a claim within its delay | `held by your claim delay: this deposit can be claimed from Ethereum block 3606, about 717 min from now …; wait, or add --no-delay to claim now, less privately` |
| endpoints | `the Ethereum RPC http://127.0.0.1:9 does not answer: …`; `the Ethereum RPC … serves chain 1, not 11155111 (--chain-id): refusing to go on`; `no contract at 0x…dEaD on chain 1: check --vault and --eth-rpc`; and `the LEZ sequencer … does not answer` (not exercised: the tests use the harness) |

## Commands

The outputs below come from the end-to-end test's run on the build box (local anvil, chain id 1; LEZ
harness; 12 November 2026), shortened where
marked `…`. The `keys` output comes from an earlier run of the binary on the same box. Addresses and
hashes are those runs'.

### `ferrata keys`

```sh
ferrata --seed seed.txt keys [--accounts 3] [--show-secrets]
```

Lists the LEZ public and private account ids `/0`, `/1`, … and fingerprints of `kAuth` and `kView`.
Add `--show-secrets` to print the keys themselves.

```text
LEZ public accounts (wallet key tree, /a):
  /0  0x8a7162a5acc0f2c0beab5a9d07dfab129e7dc9b18cd9c14b3090fe2cc40154b0
LEZ private accounts (wallet key tree, /a):
  /0  0xd0029aa57943164511b1565faea8a7b0f0df439cc613c9ee5fc64366ac66fe85
kAuth  fingerprint 0x2837b820ce651723  (sha256 of the key; --show-secrets prints it)
kView  fingerprint 0x2e0b7663cf8942fd
```

**Reveals:** nothing. It is local and makes no request. Anyone with `kAuth` can claim and release
this seed's funds. Anyone with `kView` can see its burns and their recipients, but cannot move
anything.

### `ferrata deposit`

```sh
ferrata deposit --denomination 1 --from 0xDepositor                                   # dry run
ferrata deposit --denomination 1 --keystore me.json --password-file me.pass --send --yes   # private /0
ferrata deposit --denomination 2 --kind public --i-accept-public-account …             # the public path
```

It reads the denomination and the fees in force from the vault's registry and the depositor's
`j = depositCount[depositor]`, derives `s = HMAC(kAuth, chain id ‖ vault ‖ depositor ‖ j)` and the
commitment `C = H(s ‖ kind ‖ dest)` for account `/a` of the chosen kind, and builds `depositETH` (or
`deposit` for an ERC-20). An ERC-20 deposit needs an allowance for the vault: the dry run prints the
allowance in place and the amount needed, and `--approve` sends the `approve` first (the dry run then shows
both calls, and the approve's gas). It refuses an account at or above `--gap`, because a recovering wallet
would not find it.

```text
deposit to the vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9 on chain 1
  denomination  1 ETH (1000000000000000000 base units, 18 decimals)
  mint fee      0 ETH (0 base units, 18 decimals)  (schedule 0)
  relay fee     0.001 ETH (1000000000000000 base units, 18 decimals)  (paid to the relayer that submits your claim)
  total         1.001 ETH (1001000000000000000 base units, 18 decimals)
  depositor     0xA7318B84b36fE40a07f90cd4F4b9Db34CFC44594   j = 11
  destination   your private LEZ account /0 (hidden: C commits to it)
  commitment C  0x73558b4360249fcc51e42d22d105907e2de61cee43e9af2808a6759596283f7d
  claim delay   5 h after the deposit (from your seed; `ferrata claim` waits for it)
would send: to 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9  value 1001000000000000000  data 0xf7edb857…
cost: 1.001 ETH (…) to the vault, and a network fee of about 0.000137078705013 ETH (…) (134318 gas at 1020553500 wei, eth_estimateGas)
what becomes public (Deposit (Ethereum)):
  public on Ethereum: your depositor address; the token; the denomination and its fees; the time; the commitment C (random-looking)
  public on LEZ:      nothing
  stays private:      the LEZ account that will receive the tokens (C commits to it); which later claim spends it
privacy of these choices: Minimal: only the residual correlations (token, denomination, time)
  ok Fixed denomination: your deposit looks like every other deposit of 1 ETH (…).
  ok Private destination: the receiving account and its balance stay private on LEZ.
  ok Delayed claim: held for 5 h, so the claim's time does not point at your deposit.
  ok Private fee path: the claim's LEZ fee is paid inside its proof (or by a relayer); no fee payment ties the claim to you.
not sent: a dry run (add --send --yes to sign and send)
```

An ERC-20 shaped like USDT (its `transfer` and `transferFrom` return no boolean), with `--approve`:

```text
  denomination  1 USDT (1000000 base units, 6 decimals)
  …
  allowance     0 USDT (0 base units, 6 decimals) of 1.01 USDT (1010000 base units, 6 decimals) needed for the vault
would send first: approve(0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9, 1010000) to the token 0x59b670e9fA9D0A427751Af201D676719a970857b  data 0x095ea7b3…
would send: to 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9  value 0  data 0x7fde1952…
cost: 1.01 USDT (…) to the vault; the approve's network fee about 0.000046116000322812 ETH (…) (46116 gas, eth_estimateGas); the deposit's is estimated once the allowance is in place
…
approved: 0x49bf4e9649f8a3a3f15d02042250b34ce583372fdb4b5b2eb692876ce29c27ad in block 346
```

The public path, sent with its acknowledgement:

```text
  destination   your public LEZ account /0 (hidden: C commits to it)
…
privacy of these choices: High: a direct link is public
  ok Fixed denomination: your deposit looks like every other deposit of 10 ETH (…).
  !! Public destination: the account and its balance become public on LEZ when claimed; anyone can see it received 10 ETH (…) at that time.
  ok Delayed claim: held for 5 h, so the claim's time does not point at your deposit.
  ok Private fee path: …
WARNING: these choices make a direct link public on a chain, for everyone and for good. The private path is the default; drop the option that chose this path to use it.
```

### `ferrata scan`

```sh
ferrata scan                                   # Ethereum only
ferrata scan --sequencer URL --lez-wallet DIR --lez-password-file FILE --bridge-account HEX
```

1. Downloads the vault's whole deposit list with `eth_getLogs`, in chunks.
2. Rebuilds the tree from the list and checks its root against the vault's `depositRoot` at the same
   block, so a provider that leaves out or changes a deposit is caught.
3. For every `(depositor, j)` and every candidate destination, recomputes `C`. The matches are this
   seed's deposits.

With the zone options it also reads the zone's blocks from the sequencer, in one pass. They give the
imported roots and the plaintext claims (the bridge program's public calls), the calls private
transactions make in the clear (a private burn's note reaches `token(cid, token)` that way; the checkpoint
counts the same calls), and the public accounts private transactions name: a private claim names its
nullifier account `nullifier(cid, chain, vault, N)` there (since milestone B1), and the tool matches those against its own
deposits' `N` locally. Each deposit is then marked claimed, refunded, claimable under `root(n)` (and from
which block, if its claim delay has not passed), or not yet imported.

**Reveals:** only that someone downloaded the vault's whole list and the zone's blocks. It sends no
per-deposit query, so no provider learns which deposits are this seed's.

### `ferrata recover`

```sh
ferrata recover --sequencer URL --lez-wallet DIR --lez-password-file FILE --bridge-account HEX
```

Everything from the seed file and the endpoints, with no earlier local state: what `scan` does; this
seed's burns among the burn notes published on the zone (burn counters `i = 0, 1, …` from `bridgeSeed`,
up to a gap of 20 unused counters), each marked released or not from the nullifiers in the vault's
`Release` events; and the holdings: public ones read from the zone, private ones as LEZ's wallet decrypts
them after syncing. The end of the test's run (a fresh `Seed` from the seed file):

```text
vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9: 17 deposits at block 2142, root 0x91c5…8c86 (equal to depositRoot: checked)
deposits of this seed: 12
  leaf      0  j    0  10 ETH (…)  to public /0  N 0x82754624a78376e3  claimed
  leaf      2  j    1  0.1 ETH (…)  to private /0  N 0xb9ff46ecf1bf049c  unclaimed, claimable under root(17)
  …
  leaf     14  j    9  0.1 ETH (…)  to private /0  N 0xeac1934a20eadf48  unclaimed, claimable under root(17) from block 3620 (your delay)
  leaf     15  j   10  0.1 ETH (…)  to public /1  N 0x4453abc62fe81533  unclaimed, claimable under root(17)
  leaf     16  j   11  1 ETH (…)  to private /0  N 0xb571a3bdc08f0774  claimed
burns of this seed: 12 (next burn counter 12)
  i    0  burn      0  0.1 ETH (…)  to 0x1eD62688A37bcDA13Ba5842dfDdDbE092c2B139E  released
  i    1  burn      1  0.1 ETH (…)  to 0x1eD62688A37bcDA13Ba5842dfDdDbE092c2B139E  not released
  …
  i   11  burn     11  1 ETH (…)  to 0xe5CDBb11EDe3312CE0850E79BDca0B28875AD42a  released
  holding  public /0  8.9 ETH (8900000000000000000 base units, 18 decimals)
recovered: 10 unclaimed deposits, 10 unreleased burns, 1 holdings
```

Leaf 16 was claimed privately: the tool knows it from the nullifier account the private transaction
named, not from any plaintext. Burn 11 was burned from a private account. **Reveals:** what `scan` reveals.

### `ferrata claim`

```sh
ferrata claim --bridge-program bridge.bin                     # dry run: the oldest claimable deposit past its delay
ferrata claim --bridge-program bridge.bin --send --yes        # private (default)
ferrata claim --leaf 0 --no-delay --plaintext --i-accept-plaintext-links-deposit --send --yes   # the public path
```

It picks the deposit (`--leaf`, or the oldest claimable one whose claim delay has passed), finds the
newest imported root that covers it, builds the claim (the path, the opening of `C`, the fee schedule's
range, `--relayer` if a relayer is to be paid) and runs the program's checks before anything is sent:
`verify_claim`; the token and its denominations; the fee range; the destination's kind; whether the
nullifier account is already written. A deposit still in its claim delay is held (refused with the block
it opens at) unless `--no-delay`.

- **Private** (default): LEZ's wallet runs the bridge program (`--bridge-program`) privately and proves it
  locally; the fee is paid inside the proof. A private-kind deposit is proven by its destination; a
  public-kind one by the wallet's private account `/0` (the mint still lands at the public destination).
- **Plaintext**: the wallet sends a public transaction; refused without `--i-accept-plaintext-links-deposit`.
- **Anonymity set** (RFP-022 F.8): the `anonymity` line counts the deposits a private claim cannot be told apart from
  on chain: those of the same token and amount, under the root it reads and within the fee schedule range it names,
  less those a plaintext claim has already published (its witness names the leaf and `s`, under any configuration of
  the program: an observer sees which deposit it spent) (`ferrata_wallet::claim::anonymity_set`); a fee change starts
  a new range. When the deposit is the only one there, the line says that its amount alone links the claim to it
  (above: the test's only deposit of 1 ETH). A plaintext claim names its deposit, so its set is 1, and it takes its
  deposit out of every later claim's set: `e2e/both-halves/results/randomised.md` measures both ways.

```text
claim on LEZ (configuration 0xa9d67b71ac74fe9bad38696f91d539f82bc21b953b96dee046f02242d718a8c3):
  deposit      1 ETH (…) to your private account /0, under root(cid, 17)
  would send   ClaimPrivately, proven locally by LEZ's wallet (the witness stays in the proof) to the bridge program 0x0d9450db…a38e
  nullifier N  0xf2eeae6fc870ecda4f224349e5be620584f48adb1984c880e6bf38f151703286
  anonymity    1 deposit of 1 ETH (…) under root(cid, 17) in leaves 0..17: the only one, so its amount alone links this claim to it; a later root with more deposits of the amount widens the set
  cost         nothing more in ETH: the mint fee 0 ETH (…) and the relay fee 0.001 ETH (…) were paid with the deposit; the LEZ fee is paid inside the proof, by the private account that proves
  delay        5 h after the deposit: from Ethereum block 1531 (now 1531), passed
what becomes public (Claim (LEZ)):
  public on Ethereum: nothing
  public on LEZ:      the nullifier N (fresh randomness); the denomination; which imported root was read; a public destination and its balance, if you chose one
  stays private:      which deposit is claimed (no leaf index, no depositor); a private destination and its balance
privacy of these choices: Minimal: only the residual correlations (token, denomination, time)
  ok Private claim: proven without saying which deposit it spends (no leaf index, no depositor).
  ok Private destination: the account and its balance stay private.
  ok The LEZ fee is paid inside the proof (or by a relayer): no public fee payment ties the claim to you.
public: private claim: N, the token, the denomination, the fee schedule's range, the root read, the relayer, when (its block); not the deposit, the path, s or the private destination
not sent: a dry run (add --send --yes to sign and send)
```

**Reveals.** Private claim: `N`, the token, the denomination, the fee schedule's range, the root read,
the relayer and the time; not the deposit, the path, `s` or a private destination. Plaintext claim: the
whole witness, which links the mint to its deposit for everyone.

### `ferrata burn`

```sh
ferrata burn --amount 1 --recipient 0xFresh --bridge-program bridge.bin --send --yes     # from private /0
ferrata burn --from public --i-accept-public-account --amount 0.1 --recipient 0x… --send --yes
```

Burns one denomination held by account `/a` (private by default) so that it can be released to an
Ethereum recipient. The note comes from `bridgeSeed` and the next unused burn counter: `r`; `COut`, which
binds the recipient, token and amount; the recipient encrypted under `kView` in the memo, with a MAC. A
private burn is proven by LEZ's wallet at the private account, which debits its private holding and sends
the note to the token's account in the clear. Before sending, the tool checks the denomination and the
holding (the wallet's view of a private account). It also tells whether the recipient is fresh (not a
depositor of this vault).

```text
burn on LEZ (configuration 0xa9d6…a8c3):
  amount       1 ETH (…) from your private account /0
  recipient    0xe5CDBb11EDe3312CE0850E79BDca0B28875AD42a on Ethereum (bound into COut, encrypted in the memo)
  would send   Burn to the bridge program 0x0d94…a38e, proven locally by LEZ's wallet; note r 0xfa99…f192  COut 0xbd42…  burn counter 11
  cost         at release, the burn fee 0 ETH (…) and the checkpoint fee 0 ETH (…) in force now are deducted: the recipient receives 0.999 ETH (…) through a relayer (relay fee 0.001 ETH (…)), 1 ETH (…) if you send the release yourself; the LEZ fee is paid inside the proof
what becomes public (Burn (LEZ)):
  public on Ethereum: nothing
  public on LEZ:      the burn note: token, denomination, r, the recipient encrypted (c_out); the holder account, if it is public
  stays private:      the Ethereum recipient (encrypted to you); a private holder and its balance
privacy of these choices: Minimal: only the residual correlations (token, denomination, time)
  ok Burn from a private account: the holder stays private.
  ok Fresh recipient address: nothing on Ethereum ties the release to your depositor address.
  ok Released through a relayer: the recipient needs no ETH for gas.
public: private burn: the note (r, memo, mac, COut), the token, the denomination and the time; not the holder (a private account, shown as an alias) or the Ethereum recipient (encrypted in the memo)
not sent: a dry run (add --send --yes to sign and send)
```

**Reveals:** the note, the token, the denomination and the time; a public holder's account; not the
Ethereum recipient.

### `ferrata release`

```sh
ferrata release --seal seal.hex --job-out job.json        # default: any relayer sends it and is paid the relay fee
ferrata release --program release.bin --prove --job-out job.json   # a succinct receipt in the job; the relayer wraps it
ferrata release --relayer 0xRelayer --seal seal.hex --job-out job.json   # name one relayer
ferrata release --self-send --seal seal.hex --keystore me.json --password-file me.pass --send --yes
ferrata release --input-out release.input                 # the guest's input, for a remote prover
```

1. Picks the burn (`--burn i`, or the oldest unreleased one). A released one is refused with
   `NullifierAlreadyUsed`; frozen releases with `ReleasesAreFrozen` or `TokenReleasesAreFrozen`.
2. **The wallet's rule:** among the accepted burn roots that cover the burn, the newest whose release delay
   has passed (`finalizedAt + releaseDelayFor(token, checkpoint)`, as the vault opens it). None covering it:
   `BurnRootNotAccepted`; all covering it still in their delay: `ReleaseDelayNotElapsed`, with when the first
   opens. While a newer root is in its delay, an older one that covers the burn is used.
3. Builds the release statement natively, computes `NOut` and the journal, and prints the call.

How it reaches the vault: by default a relayer sends it (the journal names relayer address 0 and the
registered relay fee, so any relayer can send it and is paid by the vault; `--relayer` names one). The tool
writes the relayer's job file (`--job-out`): `{"kind":"release","journal","seal"}` with a Groth16 seal, or
`{"kind":"release-succinct","receipt"}` with `--prove` (a relayer with `--wrap-command` wraps it); the
job holds nothing secret. `--self-send` sends it from your keystore instead, without a relay fee, after an
`eth_call` and a gas estimate.

```text
release on Ethereum (vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9, chain 1):
  burn         1 ETH (…) to 0xe5CDBb11EDe3312CE0850E79BDca0B28875AD42a (your burn counter 11)
  burn root    0x99fa7e60989bab471a3cfd280089695f58a8026a53443c0229ffe0167196f5dc of checkpoint 1 (12 burns): the newest accepted root whose release delay (120 s) has passed
  NOut         0x1fdc26a27f5c862a43fc10414a570fcc97ba354abe227cc810ca564227ef44fb
  journal      sha256 0x57432f08d3c23d89eb924af0928699b01057273f4f93ecc63f188b9f1d6769d4
  cost         the burn fee 0 ETH (…) and the checkpoint fee 0 ETH (…) are deducted, the relay fee 0.001 ETH (…) goes to the relayer: the recipient receives 0.999 ETH (…)
  would send   release(...) to the vault 0xCf7E…0Fc9, submitted by any relayer (it pays the gas and is paid the relay fee); nothing is sent from your accounts
call: to 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9  data 0x772e8c8f…
what becomes public (Release (Ethereum)):
  public on Ethereum: the recipient address; the token and amount; NOut (fresh randomness); the burn root used; the time; the relayer that sent it (it pays the gas)
  public on LEZ:      nothing
  stays private:      which burn is released (no burn index); the LEZ holder
privacy of these choices: Minimal: only the residual correlations (token, denomination, time)
  ok Fresh recipient address: nothing on Ethereum ties the release to your depositor address.
  ok Released through a relayer: the recipient needs no ETH for gas.
relayer job written to …/release-job.json: the journal and the proof, nothing secret (hand it to a relayer's inbox)
```

The self-sent release of a burn to the depositor's own address shows its exposure:

```text
privacy of these choices: Elevated: a timing or fee-payer correlation
  !  The recipient is an address you used before: the release is linked to it.
  !  You send the release yourself: your gas payer is linked to the recipient.
```

**Reveals on Ethereum:** the recipient, the token, the denomination, the time, `NOut`, the burn root and
the relayer. It does not reveal which burn is released.

### `ferrata status`

```sh
ferrata status [--token 0xToken …]                       # the vault
ferrata status --sequencer URL … --bridge-account HEX    # and the zone
```

On the vault: the deposit count and root, and `configId`; retirement and freezes; checkpoints and the
final imported leaf count; the global caps; for each token its registry entry, every denomination with
the mint, relay, burn and checkpoint fees in force, its freezes, its release delay and its caps. On the
zone: the configuration and its gate; the light-client instance it reads and its newest record; the newest
imported root and the record it was read from; the number of burns published.

```text
vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9: 16 deposits, root 0x4977…8461, configId 0xa9d6…a8c3
  retired false; deposits frozen until 0; releases frozen until 0
  checkpoints 0; final imported leaf count 0
  global deposit cap: capacity unlimited, refill 0 value units/s, available unlimited
  global release cap: capacity unlimited, refill 0 value units/s, available unlimited
token 0x0000000000000000000000000000000000000000 (ETH): registered true, deposits enabled true, lezUnit 1000000000
  [0] 0.1 ETH (…)  mint fee 0 ETH (…)  relay fee 0.0001 ETH (…)  burn fee 0 ETH (…)  checkpoint fee 0 ETH (…)
  [1] 1 ETH (…)  mint fee 0 ETH (…)  relay fee 0.001 ETH (…)  burn fee 0 ETH (…)  checkpoint fee 0 ETH (…)
  [2] 10 ETH (…)  mint fee 0 ETH (…)  relay fee 0.01 ETH (…)  burn fee 0 ETH (…)  checkpoint fee 0 ETH (…)
  deposits frozen until 0; releases frozen until 0; release delay 120 s after a checkpoint
  deposit cap: capacity unlimited, refill 0 ETH (…)/s, available unlimited
  release cap: capacity unlimited, refill 0 ETH (…)/s, available unlimited
zone: configuration ready, sunset false; attestation program 0x7a98…1f57, instance 0x1d1d…1d1d
  instance(id) is not on this zone (its records are read where written)
  newest imported root: root(cid, 17) (2 imports), from header(id, 9000128): …
  burns published: 12
```

In the harness the attestation program's records are placed at genesis, with no `instance(id)`
account, which is why the test prints that line. **Reveals:** nothing. These are reads of public state.

## Tests

```sh
cd cli
export RISC0_HOME=$PWD/../../risc0-home-lez   # LEZ's guests: RISC Zero's guest Rust 1.91.1 (lez/RESULTS.md §8)
cargo test --lib       # 10 unit tests
bash tests/e2e.sh      # the end-to-end test
```

Both build LEZ's guests (the release build of the binary does not): with RISC Zero's default guest Rust 1.88.0
they stop at "ruint@1.21.1 requires rustc 1.90". `e2e.sh` finds the home beside the repository by itself;
`cargo test --lib` needs `RISC0_HOME`. `scripts/ci/install-risc0.sh` installs it (README, "Quickstart").

The unit tests cover amounts shown and parsed exactly, the program codes equal to the wire's, every
vault error decoded from its selector, derivations that are deterministic (with `kAuth` distinct from
`kView` and distinct accounts), the privacy indicator (defaults Minimal, public paths High with a warning),
and the relayer's journals and burn watch.

The end-to-end test (`tests/e2e.rs`) needs Foundry (`anvil`, `forge`) with solc 0.8.26 (forge is offline in
`vault/`: fetch it once with `FOUNDRY_OFFLINE=false forge build` there), and `vault/lib`, which `e2e.sh`
installs. It also needs RISC Zero's Rust 1.91.1 for LEZ's guests in `../../risc0-home-lez`,
or `RISC0_HOME` set to it (`lez/RESULTS.md` §8). It runs:
- **Ethereum:** the v5 vault on a local anvil (chain id 1, never a public network), with the vault
  tests' `MockRiscZeroVerifier` behind the emergency stop.
- **LEZ:** the zone in the LEZ harness: LEZ's own state transition, with the bridge guest in RISC Zero
  dev mode. The harness signs public calls, and **proves and settles private ones with the keys LEZ's
  wallet derives from the seed** (the authorization key and the viewing key's seed of private account
  `/a`, checked to give the wallet's account id), as LEZ's wallet does on a sequencer.

The steps, through the binary (deposits, scan, status, the refusals of the binary) or through the
library functions in `src/flow.rs` that the binary calls (claim, burn, release, recover):
1. **The public path, with its warnings**: a public deposit refused without `--i-accept-public-account`,
   then sent with it (High, WARNING); ten 0.1 ETH deposits of both kinds and five of another seed; a stale
   `j` refused (`WrongDepositIndex`); an unsupported token, an invalid denomination, a 1-wei deposit cap,
   an RPC that does not answer, another chain and no vault, each refused with its message.
2. `scan` and `status` through the binary: 16 deposits, the root equal to `depositRoot`, 11 of them this seed's.
3. The plaintext claim of leaf 0: held by its delay, refused without its acknowledgement (High, WARNING),
   then sent with `--no-delay` and the acknowledgement (10 ETH minted); a second claim refused with 250 by the
   tool and by the program. A public burn refused without its acknowledgement, then eleven 0.1 ETH burns
   from public `/0`; a burn of more than the holding refused with 254.
4. A checkpoint of the eleven burns (stand-in), then the release of burn 0 by `--self-send` (Elevated): a dry
   run, then sent; 0.1 ETH received; a second release refused by the tool and by the vault.
5. **The private round trip on the defaults**: a deposit with no options (private `/0`, 1 ETH; dry run, then
   sent); the relayer's import of `root(17)`; the claim on the defaults held by its 5 h delay, then (1,500
   blocks later) a private claim proven in the harness: 1 ETH in private `/0`, no plaintext claim names the
   deposit, a replay refused with 250 by the tool and by the program; a burn on the defaults from private
   `/0` to a fresh address (dry run, then proven); 254, 251 and too many decimals refused; a checkpoint of
   twelve burns still in its release delay: the private burn's release refused (`ReleaseDelayNotElapsed`)
   while burn 1 is planned against the older root; after the delay, the release on the defaults writes the
   relayer's job; the relayer (`src/relayer.rs`) checks it, sends it and is paid the relay fee: the fresh
   address receives 0.999 ETH.
6. **Recovery from the seed alone** (`recover`, a fresh `Seed` from the seed file): 12 deposits (2 claimed,
   one privately), the 10 unclaimed found with their kinds and accounts, 12 burns (2 released), the
   holding of 8.9 ETH at public `/0`; then the zone's `status` lines.
7. **An ERC-20 shaped like USDT** (the vault tests' `MockUSDT`, registered on the vault at 6 decimals): the
   deposit's dry run asks for the allowance; `--approve` sends the approve and the deposit through the binary;
   the vault holds 1.01 USDT; `scan` finds the deposit (13 of this seed). `--json` is checked on `scan`, a
   deposit's dry run and an error.

Stand-ins, named in the test: `Init`, `Register`, `ImportRoot` and `ImportFees` run through the harness,
from synthetic execution blocks that hold the anvil vault's real storage; the checkpoint and Groth16
proofs are the mock verifier accepting the exact journals; the private accounts' native balance for their
fees (the gas drop's credit) is placed at genesis; the private accounts' states that LEZ's wallet would
decrypt are kept by the harness, checked against the chain's commitment after every private transaction.
No real proof is made in this test.

Measured on the Modal build box (CPU), 12 November 2026 (eight runs, one after each change, two of them
failing on test code that was then fixed; the last at this branch's tip): 10 of 10 unit tests and, in the
six passing runs, 1 of 1 end-to-end test, in 24.59 to 27.33 s (26.06 s at the tip); `cargo fmt --check` clean; `cargo clippy --all-targets` with no finding in the files this
work changed. From the last run:

```text
PRIVATE ROUND TRIP: 1 ETH deposited to private /0, claimed privately after its delay, burned from private /0, released by a relayer to a fresh address (999000000000000000 wei; relay fee 1000000000000000)
RECOVERY: 12 deposits (2 claimed, 10 unclaimed), 12 burns (2 released), holdings [(Public, 0, 0x0000000000000000000000000000000000000000, 8900000000000000000)]; 17 deposits in the vault
```

## Not built yet

- **No public network.** On a running LEZ v0.4.0-rc1 sequencer and node (a devnet on the build box, the final
  programs, the vault on a local anvil) the tool's sequencer path has run the private round trip on its
  defaults (`e2e/seqnet`, [`e2e/runs/seqnet-cli-1/`](../e2e/runs/seqnet-cli-1/SUMMARY.md), 13 November 2026):
  `deposit`, `claim` (held by its delay, then `ClaimPrivately` executed and proven by LEZ's wallet and applied by
  the sequencer), `burn` from private `/0` (its note read back from the zone through `lez::visible_calls`),
  `release` through `ferrata-relayer`, and `recover` (the private holding decrypted by LEZ's wallet after its
  sync). That run's private receipts were dev-mode ones; the operator's imports were `crown-driver`'s, the
  checkpoint's and release's proofs the vault tests' mock verifier, the private account's fees a stand-in for
  the gas drop (`e2e/seqnet/README.md`). In `tests/e2e.rs` the harness still stands in for the sequencer and
  keeps the private accounts' states as the chain committed them.
- **Remote proving of a claim or a burn.** LEZ's wallet proves locally. For a release, a remote prover is
  reached through files: `--input-out`, then `--seal`, or a succinct receipt in the relayer's job.
- **ERC-20s on LEZ in the test.** The test deposits a USDT-shaped token on the vault, but registers only ETH
  on the zone, so the token's claim, burn and release are not exercised (they are the same code as ETH's).
- **Freezes** are checked before sending (releases) and decoded (all), but the test vault's freeze
  authority has no key, so no frozen state is exercised.
- **Timing in the anonymity-set view.** `ferrata claim`'s `anonymity` line (F.8) counts deposits by token, amount, root
  and fee schedule range, not by how soon after its deposit the claim comes; the claim delay spreads that.
- **Deployments of the bridge program before milestone B1.** The tool addresses B1's accounts (`holder(cid, account)`,
  `nullifier(cid, chain, vault, N)`) and does not check which program it talks to: against a deployment of the earlier
  program (the live runs of November 2026, `docs/image-ids.md` `ferrata_bridge`) `scan` misses that program's private
  claims and `recover` reads accounts it does not have (no funds at risk: that program refuses B1's accounts). Use the
  tool of `f13faee2` for those deployments.
- **Relayer work, admin functions, Logos module.** The relayer's root imports and checkpoint proofs are
  `ferrata-relayer`'s; the admin functions and the Logos core module (`ferrata_bridge_module`) are not part
  of the tool.
