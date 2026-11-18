# mainnet-risc0-verify: our real Groth16 seals, verified by RISC Zero's own verifier on Ethereum mainnet

18 November 2026, from the laptop (`scripts/verify-mainnet-risc0.py`, Python standard library), **read-only**:
Ethereum mainnet block **26,443,464**, two free public endpoints without keys,
`https://ethereum-rpc.publicnode.com` (run started 17:32:27Z) and `https://eth.drpc.org` (17:33:26Z). Nothing was
deployed and nothing was sent: the only JSON-RPC methods were `eth_chainId`, `eth_blockNumber`, `eth_getCode` and
`eth_call` (354 requests per endpoint; the two endpoints received the same requests and gave the same answers). Cost: $0.

**Result.** The tree records 86 distinct Groth16 seals, all with selector `0x73c457ba` (RISC Zero's Groth16 verifier
3.0.0, the one the vault pins). 66 of them are recorded with their image id and journal. **All 66 verify on mainnet**,
through RISC Zero's verifier router and directly on its Groth16 verifier 3.0.0. For each one, three changed inputs
were also tried: the seal with its last byte changed, the image id with its last byte changed, and the digest of the
journal with its last byte changed. **Every one reverted** with `VerificationFailed()` (`0x439cc0cd`): 198 of 198.
The other 20 seals are the privacy suite's releases (`docs/privacy/real/proofs.json`), which are recorded without
their journals, so `verify` cannot be called for them. Each endpoint: **74 PASS, 0 FAIL**
([`verify-publicnode.txt`](verify-publicnode.txt), [`verify-drpc.txt`](verify-drpc.txt)).

## RISC Zero's deployment on Ethereum mainnet

Source: risc0-ethereum tag `v3.0.0`, the release the vault pins ([`vault/README.md`](../../../vault/README.md)). The
tag resolves to commit `32aa0b6f23ddd02dd93fc71717667606e5c7db86` (`git ls-remote`). Two files from it were fetched
on 18 November 2026 and copied here verbatim:

- `contracts/deployment.toml` is [`upstream/deployment.toml`](upstream/deployment.toml), sha256
  `77201508843f9a1a56162cd680b1af389b0deadc3ad788e0a8c6360838741b3d`;
- `contracts/src/groth16/ControlID.sol` is [`upstream/ControlID.sol`](upstream/ControlID.sol), sha256
  `753535e844a84a28b275fe21bd4c30022266b27bcdaa9b189220dece220accfa`.

| Contract | Address | `deployment.toml` | On chain at block 26,443,464 |
|---|---|---|---|
| Verifier router | `0x8EaB2D97Dfce405A1692a21b3ff3A172d593D319` | line 12 | 2,356 bytes of code; `getVerifier(0x73c457ba)` = the 3.0.0 emergency stop; `owner()` = the timelock |
| Groth16 verifier 3.0.0 | `0x2a098988600d87650Fb061FfAff08B97149Fa84D` | lines 51–55 (selector `0x73c457ba` line 54) | 5,313 bytes; `SELECTOR()` `0x73c457ba`, `VERSION()` "3.0.0"; `CONTROL_ROOT_0`/`_1` and `BN254_CONTROL_ID` match `ControlID.sol` lines 23 and 25 (the root split as the constructor splits it) |
| Its emergency stop | `0x9F9994Eb4Cb5200198FEfb470f8b50301662e696` | line 56 | 2,384 bytes; `verifier()` = the 3.0.0 verifier; `paused()` = false; `owner()` = the record's `admin` `0xF616A4f81857CFEe54A4A049Ec187172574bd412` (line 7) |
| Timelock controller (owns the router) | `0x0b144E07A0826182B6b59788c34b32Bfa86Fb711` | lines 10–11 (delay 259,200 s, 3 days) | 6,712 bytes |

The keccak256 of each contract's code is in the `verify-*.txt` files. The router's other Groth16 entries are listed
for context only; the script does not count them as checks:

- 2.0.0-rc.3 (`0x9f39696c`) and 2.1.0 (`0xf536085a`) are registered and their stops are paused, as the record says
  (`stopped = true`).
- 2.2.0 (`0xbb001d44`) is registered and not paused.
- 1.1.0-rc.3 (`0x50bd1769`) and 1.2.0 (`0xc101b42b`): `getVerifier` reverts `SelectorRemoved`. The router has removed
  them since; the v3.0.0 record does not show that.

## The seals

The script finds each seal and pairs it with an image id and a journal, all from the committed records:

- crown runs: `crown.json`, `params.json` and `releases/release-<name>.json`;
- receipt directories: `seal.hex` beside `journal.hex`, with the image id from that run's `groth16.json`,
  `prove.json`, `run.json`, `scenario.json`, `proofs.json` or README;
- JSON fixtures that carry the seal, the journal and the image id together.

The journal digest is sha256 of the journal bytes, as the vault computes it (`FerrataVault.sol` hashes the journal
words it checks; [`scripts/verify-sepolia.py`](../../../scripts/verify-sepolia.py) does the same). Where one seal has
several records, all of them agree. The main ones are below; the other 53 are in `verify-*.txt`, one line each.

| Record | Program (image id) | Journal sha256 (bytes) | Router, verifier | Seal / image id / journal changed |
|---|---|---|---|---|
| `e2e/runs/crown-live-3` checkpoint (the live round trip) | `checkpoint_b` `0xf8116722…626e` | `0x84494ac4…404e` (544) | returns, returns | VerificationFailed ×3 |
| `crown-live-3` release public-0 | release `0xb64900bc…29e9` | `0x895b8076…43d4` (352) | returns, returns | VerificationFailed ×3 |
| `crown-live-3` release private-1 | release `0xb64900bc…29e9` | `0xa517749e…78aa` (352) | returns, returns | VerificationFailed ×3 |
| `e2e/runs/zone-bridge-rc3` checkpoint (rc3 return leg) | `checkpoint_b_0_3_5` `0xafbf4023…3bcf` | `0x2d3b8566…b1a9` (544) | returns, returns | VerificationFailed ×3 |
| `zone-bridge-rc3` release public-0 | release `0xb64900bc…29e9` | `0xcb42efea…c415` (352) | returns, returns | VerificationFailed ×3 |
| `zone-bridge-rc3` release private-1 | release `0xb64900bc…29e9` | `0xee948733…8165` (352) | returns, returns | VerificationFailed ×3 |
| `e2e/runs/testnet-return-fork-2` release public-0 | release `0xb64900bc…29e9` | `0xb01a0ef1…5611` (352) | returns, returns | VerificationFailed ×3 |
| `testnet-return-fork-2` release private-1 | release `0xb64900bc…29e9` | `0x25dbdf33…32fa` (352) | returns, returns | VerificationFailed ×3 |
| `bridge/artifacts/eth` (release guest v2) | `0xdda76672…27c5` | `0x58581b63…7fe2` (352) | returns, returns | VerificationFailed ×3 |
| `bridge/artifacts/erc20` (release guest v2) | `0xdda76672…27c5` | `0xcb236a60…c586` (352) | returns, returns | VerificationFailed ×3 |
| `bridge/artifacts/ethpull` (release guest v2) | `0xdda76672…27c5` | `0x3d706f2b…9ecb` (352) | returns, returns | VerificationFailed ×3 |
| `bridge/artifacts/v1/eth` (release guest v1) | `0x2ca5b06e…4c41` | `0xbd09000e…cbd2` (352) | returns, returns | VerificationFailed ×3 |
| `bridge/artifacts/v1/erc20` (release guest v1) | `0x2ca5b06e…4c41` | `0xcab00ffb…0a0f` (352) | returns, returns | VerificationFailed ×3 |

The other 53 have the same result. They are crown-live-1 and the crown rehearsals 1, 3, 4 and 5 (checkpoint and both
releases each), zone-bridge-1 and -2, testnet-return-fork-1's releases, the fork, cloud-fork and Sepolia fixtures'
candidates, echo proofs and releases, the relayer's paid release, the P4 minimum and bench release runs, the
`cp-empty-1` checkpoints, the crown R4 releases and the vault's real fixtures (`vault/test/real/fixtures/`). Image ids
are named in [`docs/image-ids.md`](../../../docs/image-ids.md), [`e2e/crown/README.md`](../../crown/README.md),
[`bridge/RESULTS.md`](../../../bridge/RESULTS.md) and [`checkpoint/RESULTS.md`](../../../checkpoint/RESULTS.md).

Not checked, and why:

- 20 seals in [`docs/privacy/real/proofs.json`](../../../docs/privacy/real/proofs.json) (the release image id is
  recorded, the journal is not).
- Stand-ins, which are not Groth16 seals:
  - `crown-fork-3r`: 4-byte seals, because that rehearsal ran with an accept-all verifier;
  - `testnet-return-fork-1` and `-2` checkpoints: 130-byte stand-in seals starting `0x962e7684`. Their releases are
    real and verify (above).

## What it shows, and what it does not

**What it shows.** The bytes we recorded, under the image ids and journals we recorded, are accepted by the contracts
RISC Zero itself deployed and operates on Ethereum mainnet. These are not contracts we deployed: the Sepolia vaults
check seals with our own deployment of the same verifier source, `0x8f0A…` (EVIDENCE row 122). The selector of the
prover we use (RISC Zero 3.0.5) is registered on the mainnet router and its stop is not paused. A mainnet deployment
of the vault could therefore pin this Groth16 verifier, `0x2a09…a84D`, behind its own emergency stop. That is the
design: the vault pins the base verifier, never the router, which RISC Zero's timelock can change
([`docs/trust.md`](../../../docs/trust.md) §5). The removed 1.x entries are an example of such a change.

**What it does not show.** Nothing of Ferrata runs on mainnet: no vault is deployed there and no transaction was
made. The vault's own rules (journal fields, nullifiers, the checkpoint wait) are not exercised here; rows 91–92 do
that on Sepolia. `eth_call` returns what the endpoint's node computed. Two independent endpoints agree, and anyone can
repeat the calls against a node of their own.

## Re-check

```
python3 -I scripts/verify-mainnet-risc0.py                      # publicnode, at its latest block
python3 -I scripts/verify-mainnet-risc0.py --rpc https://eth.drpc.org --block 26443464   # the block above (archive state)
python3 -I scripts/verify-mainnet-risc0.py --list               # the seals and their records; sends nothing
```

The script sends only the four methods above; any other method raises before anything is sent. It stops with exit
status 2 unless the endpoint answers chain id 1. publicnode serves recent state only and refuses an old `--block`
without a token, so replay old blocks on an archive endpoint. `--out DIR --tag NAME` writes `verify-NAME.txt`,
`calls-NAME.jsonl` (every request with its full calldata and the answer, revert data included) and
`summary-NAME.json`. One seal by hand, with Foundry's `cast`:

```
R=e2e/runs/crown-live-3/checkpoint/receipt
D=0x$(python3 -I -c 'import hashlib,sys; print(hashlib.sha256(bytes.fromhex(open(sys.argv[1]).read().strip().removeprefix("0x"))).hexdigest())' $R/journal.hex)
cast call 0x8EaB2D97Dfce405A1692a21b3ff3A172d593D319 'verify(bytes,bytes32,bytes32)' "$(cat $R/seal.hex)" \
  0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e $D --rpc-url https://ethereum-rpc.publicnode.com
```

This prints `0x` (the call returned). With the other checkpoint program's id, `0xb9c1e6a8…`, it reverts with data
`0x439cc0cd` (both tried on 18 November 2026).
