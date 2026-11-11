# `ferrata`: the command-line tool of the Ferrata bridge

**Prototype, unaudited. Run it only against a local chain or a test deployment.**

`ferrata` is the tool a user or a relayer runs for the Ferrata bridge between Ethereum and a LEZ
zone. It replaces the test drivers (`bridge/e2e`, `lez/live/driver`, the vault scripts) for the
user's steps: derive the keys, deposit, find one's deposits and burns, claim, burn, release, and read
the bridge's configuration. It is a thin layer over libraries that already exist:

| Part | Library |
|---|---|
| Commitments, nullifiers, trees, the claim and release statements, recovery | `ferrata-bridge-core` (`bridge/crates/`) |
| The bridge program's messages, states, accounts and checks (`verify_claim`, fees, ranges) | `ferrata-lez-bridge-wire` (`lez/bridge-wire/`) |
| The attestation program's accounts and records | `ferrata-lez-wire` (`lez/wire/`), `ferrata-core` |
| LEZ: keys, the encrypted wallet, signing, private execution, the sequencer's RPC | LEZ v0.4.0-rc1's own `wallet` crate (`WalletCore`), as `lez/live/driver` uses it |
| Ethereum: keystore files, signing, JSON-RPC, the vault's ABI | `alloy` 1.x |
| The release guest (execute or prove) | `risc0-zkvm` 3.0.5 client, `r0vm` 3.0.5 over IPC |

## Installation

Linux x86-64. The tool links LEZ's crates by path, like `lez/`:

```sh
# LEZ v0.4.0-rc1 (commit 1806e87) beside the repository, at ../upstream/lez-v0.4.0-rc1 (see lez/RESULTS.md §8)
cd cli
cargo build --release          # Rust 1.98.1 from cli/rust-toolchain.toml (LEZ's host toolchain)
./target/release/ferrata --help
```

`release --prove` also needs `r0vm` 3.0.5 (`rzup install r0vm 3.0.5`). Building the tests needs
more (see [Tests](#tests)). `ferrata` has its own Cargo workspace, because it links LEZ's wallet and
node-side crates and alloy, which the rest of the repository avoids.

## Files and settings

Every command takes these options; each has an environment variable.

| Option | Variable | What |
|---|---|---|
| `--seed FILE` | `FERRATA_SEED` | The seed file: the BIP-39 mnemonic of the LEZ wallet. Keep it `chmod 600`. Everything is derived from it. |
| `--eth-rpc URL` | `FERRATA_ETH_RPC` | Any Ethereum JSON-RPC endpoint. The tool checks what it downloads against the vault's own roots. |
| `--vault ADDR` | `FERRATA_VAULT` | The vault's address. |
| `--from-block N` | `FERRATA_FROM_BLOCK` | The vault's deployment block (where its events start). |
| `--chunk N` | | Blocks per `eth_getLogs` request (default 10,000). |
| `--sequencer URL` | `FERRATA_SEQUENCER` | The LEZ sequencer's RPC endpoint. |
| `--lez-wallet DIR` | `FERRATA_LEZ_WALLET` | The LEZ wallet's home: its config and its storage, encrypted under its password. This is the LEZ keystore. |
| `--lez-password-file FILE` | `FERRATA_LEZ_PASSWORD_FILE` | The LEZ wallet's password. |
| `--bridge-account HEX` | `FERRATA_BRIDGE_ACCOUNT` | The bridge program's account id on LEZ. |
| `--gap N` | | Accounts of each kind searched for deposit destinations (default 8). |

Commands that sign on Ethereum take `--keystore FILE` (a Web3 Secret Storage file, the format of
`cast wallet import` and geth) and `--password-file FILE`. **No key is ever passed on the command line
or in an environment variable.** A command that sends does nothing unless it gets both `--send` and
`--yes`. Without `--send` it only prints what it would send. With `--send` but no `--yes` it refuses,
after printing what would become public.

**Keys.** The LEZ wallet's seed gives its public and private key trees. The tool uses account `/a` of
each tree as deposit destinations, for `a` below `--gap`. The bridge keys come from one label of its
own: `bridgeSeed = HMAC-SHA256(LEZ seed, "ferrata/bridge-seed/v1")`. `kAuth` (authorises claims and
releases) and `kView` (shows burns and recipients) are derived from `bridgeSeed`. The label is
disjoint from LEZ's own derivation paths.

**Amounts.** Every amount is printed in the token's units, followed by base units and decimals, for
example `0.1 ETH (100000000000000000 base units, 18 decimals)`. Decimals and the symbol are read from
the token contract (18 and `ETH` for ETH). Amounts typed by the user (`burn --amount 0.1`) are parsed
exactly; extra decimal places are refused.

**Refusals.** A command checks what the program or the vault would refuse before it sends:
- On LEZ it runs the bridge program's own checks from `ferrata-lez-bridge-wire` on the accounts it
  reads.
- On Ethereum it runs `eth_call` first and decodes the vault's custom error from its selector.

Every refusal names its source, its code and the code's meaning:

```text
refused: LEZ bridge program exit code 250 ALREADY_CLAIMED (checked before sending): the deposit's nullifier is spent: it was claimed already
refused: vault error WrongDepositIndex() (eth_call before sending): the index j is not depositCount[sender]: another deposit from this address came first
```

All 16 program codes (240–255) and 40 vault errors are listed in `src/codes.rs`. A unit test checks
the program codes against the bridge wire's constants, and another decodes every vault error from its
selector.

## Commands

Unless marked as a format, the outputs below come from one end-to-end test run on the build box
(local anvil, chain id 1; LEZ harness; 11 November 2026). The `keys` output comes from an earlier
run of the binary on the same box. Addresses and hashes are those runs'.

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
ferrata deposit --denomination 2 --kind public --account 0 --from 0xDepositor     # print only
ferrata deposit --denomination 2 --kind private --account 1 \
    --keystore depositor.json --password-file depositor.pass --send --yes          # sign and send
```

The command:
1. Reads the denomination and the fees in force from the vault's registry, and the depositor's
   `j = depositCount[depositor]`.
2. Derives `s = HMAC(kAuth, chain id ‖ vault ‖ depositor ‖ j)` and the commitment
   `C = H(s ‖ kind ‖ dest)` for account `/a` of the chosen kind (private by default).
3. Builds `depositETH` (or `deposit` for an ERC-20) and prints the call.

It refuses an account at or above `--gap`, because a recovering wallet would not find it. `--token`
picks an ERC-20; that deposit needs an allowance for the vault first.

```text
deposit to the vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9 on chain 1
  denomination  10 ETH (10000000000000000000 base units, 18 decimals)
  mint fee      0 ETH (0 base units, 18 decimals)  (schedule 0)
  relay fee     0.01 ETH (10000000000000000 base units, 18 decimals)
  total         10.01 ETH (10010000000000000000 base units, 18 decimals)
  depositor     0x4045fA762a262d69e723a115E13086B9adC5D0EE   j = 0
  commitment C  0x99605fbde82bee3d6c2a53594a85755473da45a8b6e70f96000cc7ed9d5693f1
public on Ethereum: the depositor, the token, the denomination and fees, the time, C.
hidden: the LEZ destination (public account /0); C commits to it.
note: a public destination becomes visible when it is claimed (which deposit paid it stays hidden in a private claim).
call: to 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9  value 10010000000000000000  data 0xf7edb857…
sent: 0xee6d459bb004918af8809921b1085fc4127ab1fb775c28010b6f9fd897246bcc in block 6 (leaf Some(0))
```

**Reveals on Ethereum:** the depositor, the token, the denomination and fees, the time and `C`. It
does not reveal the LEZ destination or its kind.

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

With the zone options it also reads the bridge program's public history from the sequencer's blocks.
That history gives the imported roots and the plaintext claims, so each deposit can be marked:
claimed, refunded, claimable under `root(n)`, or not yet imported.

```text
vault 0xCf7E…0Fc9: 16 deposits at block 21, root 0x2432…e0f5 (equal to depositRoot: checked)
deposits of this seed: 11
  leaf      0  j    0  10 ETH (10000000000000000000 base units, 18 decimals)  to public /0  N 0x39e320c2d960c154  claim state unknown (no --sequencer)
  leaf      2  j    1  0.1 ETH (100000000000000000 base units, 18 decimals)  to private /0  N 0x86c3db04ac984b9a  claim state unknown (no --sequencer)
  …
```

**Reveals:** only that someone downloaded the vault's whole list and the zone's blocks. It sends no
per-deposit query, so no provider learns which deposits are this seed's.

### `ferrata recover`

```sh
ferrata recover --sequencer URL --lez-wallet DIR --lez-password-file FILE --bridge-account HEX
```

Runs everything `scan` does. It then finds this seed's burns among the burn notes published on the
zone (burn counters `i = 0, 1, …` from `bridgeSeed`, up to a gap of 20 unused counters), and marks
each one released or not from the nullifiers in the vault's release events. It ends with the counts.
This is the output's format; the test runs recovery through the library, not through this command:

```text
burns of this seed: 11 (next burn counter 11)
  i    0  burn      0  0.1 ETH (…)  to 0x…  released
  i    1  burn      1  0.1 ETH (…)  to 0x…  not released
recovered: 10 unclaimed deposits, 10 unreleased burns
```

It needs only the seed file and the endpoints, with no earlier local state. It reveals what `scan`
reveals.

### `ferrata claim`

```sh
ferrata claim --leaf 3 --bridge-program bridge.bin --send --yes                       # private (default)
ferrata claim --leaf 0 --plaintext --i-accept-plaintext-links-deposit --send --yes    # plaintext
```

Picks the deposit (`--leaf`, or the oldest claimable one) and finds the newest imported root that
covers it. It builds the claim (the path, the opening of `C`, the fee schedule's range, `--relayer`
if a relayer is to be paid) and runs the program's checks before anything is sent:
- `verify_claim`;
- the token and its denominations;
- the fee range;
- the destination's kind;
- whether the nullifier account is already written, read from the zone.

The claim is then submitted through LEZ's wallet library:
- **Private** (default): LEZ's wallet runs the bridge program (`--bridge-program`, its binary)
  privately and proves it locally. The proof is not sent to a remote prover.
- **Plaintext**: the wallet sends a public transaction. A public destination that is not yet on the
  zone is created on its public key. The tool refuses this path without
  `--i-accept-plaintext-links-deposit`.

**Reveals.**
- Private claim: `N`, the token, the denomination, the fee schedule's range, the root read, the
  relayer and the time. It does not reveal the deposit, the path, `s` or a private destination.
- Plaintext claim: the whole witness, which links the mint to its deposit for everyone.

### `ferrata burn`

```sh
ferrata burn --account 0 --amount 0.1 --recipient 0xRecipient --send --yes
```

Burns one denomination held by public account `/a` so that it can be released to an Ethereum
recipient. The note comes from `bridgeSeed` and the next unused burn counter:
- `r`;
- `COut`, which binds the recipient, token and amount;
- the recipient, encrypted under `kView` in the memo, with a MAC.

Before sending, the tool checks the denomination and the holding. The refusal's format (the test
checks for `254 INSUFFICIENT_BALANCE`):

```text
refused: LEZ bridge program exit code 254 INSUFFICIENT_BALANCE (checked before sending: the holding is … base units): the holding is smaller than the denomination burned
```

**Reveals:** the holder's account (a public holding), the token, the denomination, the time and the
note. It does not reveal the Ethereum recipient.

### `ferrata release`

```sh
ferrata release --input-out release.input                                  # hand the input to a prover
ferrata release --program release.bin --prove                              # execute (RISC0_DEV_MODE=1) or prove locally
ferrata release --seal seal.hex --keystore relayer.json --password-file relayer.pass --send --yes
```

1. Picks the burn (`--burn i`, or the oldest unreleased one).
2. Finds a finalized checkpoint whose burn root covers it. If there is none, it refuses with
   `BurnRootNotAccepted`. If the burn is already released, it refuses with `NullifierAlreadyUsed`.
3. Builds the release statement natively, computes `NOut` and the journal, and prints them.

`--input-out` writes the guest's input for a remote prover. `--prove --program` runs the release
guest in `r0vm` and checks that its journal equals the native one. The vault verifies a Groth16 seal,
so a send needs `--seal` (the wrapped proof). `--relayer` and `--relay-fee` name a relayer and claim
the registered relay fee.

**Reveals on Ethereum:** the recipient, the token, the denomination, the time, `NOut`, the burn root
and the relayer. It does not reveal which burn is released.

### `ferrata status`

```sh
ferrata status [--token 0xToken …]                       # the vault
ferrata status --sequencer URL … --bridge-account HEX    # and the zone
```

On the vault, it shows:
- the deposit count and root, and `configId`;
- retirement and freezes;
- checkpoints and the final imported leaf count;
- the global caps;
- for each token: its registry entry, every denomination with the mint, relay, burn and checkpoint
  fees in force, and the token's caps.

On the zone, it shows:
- the configuration and its gate;
- the light-client instance it reads: the finalized slot in the instance's store, and that slot's
  header record;
- the newest imported root, and the record it was read from;
- the number of burns published.

```text
vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9: 16 deposits, root 0x2432…e0f5, configId 0xc106…2610
  retired false; deposits frozen until 0; releases frozen until 0
  checkpoints 0; final imported leaf count 0
  global deposit cap: capacity unlimited, refill 0 value units/s, available unlimited
token 0x0000000000000000000000000000000000000000 (ETH): registered true, deposits enabled true, lezUnit 1000000000
  [0] 0.1 ETH (100000000000000000 base units, 18 decimals)  mint fee 0 ETH (…)  relay fee 0.0001 ETH (100000000000000 base units, 18 decimals)  burn fee 0 ETH (…)  checkpoint fee 0 ETH (…)
  deposit cap: capacity unlimited, refill 0 ETH (0 base units, 18 decimals)/s, available unlimited
zone: configuration ready, sunset false; attestation program 0xd32c…aa37, instance 0x1d1d…1d1d
  instance(id) is not on this zone (its records are read where written)
  newest imported root: root(cid, 16) (1 imports), from header(id, 9000064): execution block 9000064 0x07d6…6573 (time 1808000768)
  burns published: 11
```

In the harness the attestation program's records are placed at genesis, with no `instance(id)`
account, which is why the test prints that line. **Reveals:** nothing. These are reads of public
state.

## Tests

```sh
cd cli
cargo test --lib       # 4 unit tests
bash tests/e2e.sh      # the end-to-end test
```

The unit tests cover amounts shown and parsed exactly, the program codes equal to the wire's, every
vault error decoded from its selector, and derivations that are deterministic, with `kAuth` distinct
from `kView` and distinct accounts.

The end-to-end test (`tests/e2e.rs`) needs Foundry (`anvil`, `forge`), and `vault/lib`, which
`e2e.sh` installs. It also needs RISC Zero's Rust 1.91.1 for LEZ's guests in `../../risc0-home-lez`,
or `RISC0_HOME` set to it (`lez/RESULTS.md` §8). It runs:
- **Ethereum:** the v5 vault on a local anvil (chain id 1, never a public network), with the vault
  tests' `MockRiscZeroVerifier` behind the emergency stop.
- **LEZ:** the zone in the LEZ harness: LEZ's own state transition, with the bridge guest in RISC Zero
  dev mode.

The steps:
1. **Deposits through the binary**, signed from keystore files: one 10 ETH deposit to public `/0`, ten
   of 0.1 ETH to accounts of both kinds, and five from another seed. A deposit with a stale `j` is
   refused with `WrongDepositIndex` before sending.
2. **`scan` and `status` through the binary**: 16 deposits, the root equal to `depositRoot`, 11 of
   them this seed's.
3. **Claim, burns and release through the library functions the binary calls.** The harness stands
   in for LEZ's sequencer client: it signs and settles the call the tool built.
   - The plaintext claim of leaf 0 mints 10 ETH.
   - A second claim is refused with 250 by the tool's check, and by the program itself when sent
     unchecked.
   - Eleven 0.1 ETH burns, then a burn of more than the holding, refused with 254.
   - The release of the first burn pays 0.1 ETH once, and is refused with `NullifierAlreadyUsed` the
     second time.
4. **Recovery**: a fresh client with only the seed file finds 10 of 10 unclaimed deposits and 10 of
   10 unreleased burns. The zone's `status` lines name the newest import and the record it read.

The relayer's steps are stand-ins:
- `Init`, `Register` and `ImportRoot` run through the harness, from a synthetic execution block that
  holds the anvil vault's real storage.
- The checkpoint and Groth16 proofs are replaced by the mock verifier accepting the exact journals.
  No real proof is made in this test.

Measured on the Modal build box (CPU), 11 November 2026: 4 of 4 unit tests and 1 of 1 end-to-end test
passed, the end-to-end test in 15.8 s:

```text
RECOVERY: 10 of 10 unclaimed deposits (1 claimed), 10 of 10 unreleased burns (1 released); 16 deposits in the vault
```

## Not built yet

- **Private claims are untested.** The private path (LEZ's wallet runs the program privately and
  proves it) is built, but no test runs it: the harness's private accounts use test keys, not the
  wallet's derived keys.
- **Remote proving of a claim.** The LEZ wallet proves locally. For a release, a remote prover is
  reached through files: `--input-out`, then `--seal`. There is no client for a proving service.
- **Private claims in `scan` and `recover`.** These commands see plaintext claims only. A deposit
  claimed privately still shows as claimable. A new claim of it is still refused with 250, because
  `claim` reads its nullifier account first. Reading the nullifier accounts written by private
  transactions from the blocks is not built.
- **The ERC-20 allowance.** `deposit` says how much allowance is needed but does not send the
  `approve`. ETH needs none.
- **No live sequencer or public network.** `ferrata` has not been run against LEZ's sequencer or any
  public network. The sequencer path is the wallet library's, as in `lez/live/driver`.
- **Relayer work, admin functions, Logos module.** The relayer's root imports and checkpoint proofs,
  the admin functions, and a Logos module or `ui_qml` mini-app (proposal §B7) are not part of the
  tool.
