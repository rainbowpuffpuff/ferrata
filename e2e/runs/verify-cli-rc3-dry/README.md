# docs/VERIFY.md §5's commands, rehearsed against our public test network and Sepolia: dry runs, nothing sent

26 November 2026, on the build box (Modal), from this branch at the commit `transcript.txt` names: the `ferrata` tool
built against LEZ v0.4.0-rc3 by `deploy/testnet/cli-rc3/build.sh`, then each command of the round trip run with
`docs/VERIFY.md` §5's settings, read-only: a throwaway seed (the public BIP-39 test mnemonic of all-zero entropy) and
a throwaway Ethereum keystore made by `cast wallet new` for the run, neither holding anything; no command was given
`--send`. Paths of the build box are written `$T/` (its scratch); cargo's progress lines are left out.

| File | What ran | What it shows |
|---|---|---|
| `transcript.txt` | the build, then `--help`, `keys`, `status`, `scan`, `claim --help`, `claim`, `burn`, `release`, the tool reading Sepolia through publicnode's free endpoint and the zone through the network's gateway directly | `keys` and `status` work (the vault: 2 deposits, 1 checkpoint, its caps and denominations; the zone: the configuration, the instance active with its jobs applied, the newest imported root, one burn published). `scan`, `claim` and `release` stop at `eth_getLogs … pruned history unavailable`: that endpoint does not serve the vault's logs from its first block. `burn` stops at `syncing the LEZ wallet with the zone: Request rejected 400`: LEZ's wallet asks for 100 blocks at a time and the gateway serves at most 50 |
| `transcript-2.txt` | `scan`, `claim`, `claim --leaf 0 --no-delay`, `burn`, `release` through the private-claim kit's pager (`kit_helper.py serve`, branch `private-kit`) and `https://rpc.sepolia.ethpandaops.io` | every command reads what it needs: `scan` checks the vault's root (`equal to depositRoot: checked`) and finds no deposit of this seed; `claim` answers `nothing to claim`; `burn` syncs the wallet through the pager (25,747 blocks in 20 s) and is refused before sending by the bridge program's own check, `INSUFFICIENT_BALANCE`; `release` answers `no burn with counter 0 of this seed`; the pager waited out the gateway's and the endpoint's 429s |
| `transcript-3.txt` | `deposit --denomination 0.001` | refused by the tool's argument parser: `--denomination` takes the index `ferrata status` lists, not an amount (`docs/VERIFY.md` corrected) |
| `transcript-4.txt` | `deposit --denomination 0` through the archive endpoint | stopped by that endpoint's rate limit (HTTP 429) |
| `transcript-5.txt` | `deposit --denomination 0` through publicnode | the dry run in full: the vault and chain, the denomination and its fees (`total 0.001002 ETH`), the private destination and its commitment, the claim delay derived from the seed (10 h), the exact call, what becomes public, `not sent: a dry run`; from the unfunded keystore, also that the call would revert (`OutOfFunds`) |

What this does not show: any command with `--send` (nothing was signed or sent), a private claim or burn proven, a
checkpoint or a release of a burn of ours; those are in `docs/VERIFY.md` §5's "What has been run of this path".
