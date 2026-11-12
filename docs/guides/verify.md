# Check it yourself

Two scripts let anyone check the crown run's round trip on Sepolia and the programs behind it without trusting
anything we say: one reads the chain, the other rebuilds the programs. Neither signs or sends a transaction.

| Script | Needs | Answers |
|---|---|---|
| [`scripts/verify-sepolia.py`](../../scripts/verify-sepolia.py) | Python 3.9 or later (standard library only), any Sepolia JSON-RPC | Did the run's transactions, deposits, checkpoint and releases happen on Sepolia as its records say, on a vault configured as we say? |
| [`scripts/reproduce-image-ids.sh`](../../scripts/reproduce-image-ids.sh) | the pinned toolchains (or Docker), ~30 GB of disk, an x86-64 machine | Are the programs the vault accepts proofs of really built from this repository's source? |
| [`admin/index.html`](../../admin/index.html) (or `node admin/dump.mjs`) | a browser (or node 18+), any JSON-RPC endpoint | What state is a deployed vault in now: configuration, roles, the emergency stop, freezes, caps, fees owed, deposits, checkpoints, candidates, releases (§3)? |

## 1. The round trip: `scripts/verify-sepolia.py`

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3                 # public endpoint (publicnode)
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3 --rpc http://localhost:8545   # your own node
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-1                 # the first live run, earlier programs
```

It reads the run's records (`identity.json`, `deploy.json`, `params.json`, `run.json`, `manifest.json`, `crown.json`,
`releases/`, `lez/session-a/`, `lez/session-b/`), asks the chain, and prints one line per check: `PASS`, `FAIL`
(exit status 1) or `PENDING` (the run has not reached that stage, on chain and in the records; not a failure). An
endpoint that does not answer as Sepolia stops it after the first check with exit status 2. A stage the chain has
reached but the records predate (records fetched mid-run) is checked from the chain. It
uses only read methods (`eth_chainId`, `eth_getCode`, `eth_getBalance`, `eth_getTransactionCount`,
`eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_getLogs`, `eth_getBlockByNumber`, `eth_call`), computes
Keccak-256 itself, and needs nothing from this repository but the records (and, if present,
[`docs/image-ids.md`](../image-ids.md), the final programs' ids, and `e2e/crown/README.md`, whose tables name the
programs of earlier runs). A run's image ids are the ones its own records give; the two live vaults are pinned to the
programs they were deployed with (crown-live-3's `0x04eE…e44c` to `docs/image-ids.md`, crown-live-1's `0xfE31…9E54`
to its table in `e2e/crown/README.md`), so records of either that name other ids fail.

| Check | What a PASS proves | What it still trusts |
|---|---|---|
| chain | the endpoint serves Sepolia (chain id 11155111); prints head and finalized block | the endpoint (use your own node to remove it) |
| image ids | the run's own records give each program one id: `params.json` (`checkpoint_b`, `checkpoint`, release), `programs/ids.jsonl` as the run used them, and the ids LEZ session A deployed (attestation, bridge); a run of the final programs (any of its ids in [`docs/image-ids.md`](../image-ids.md)) has docs/image-ids.md's id for every program it names, or the check fails; a run of earlier programs is named by the table of `e2e/crown/README.md` that lists its ids (`crown-live-1`: "The programs of crown-live-1"); the two live vaults must hold theirs: crown-live-3's the final programs, crown-live-1's its own | nothing on chain yet: the next checks tie them to the vault, and section 2 ties them to source |
| deployment | each contract is at CREATE(deployer, N + i) for deploy.sh's seven nonces, code exactly where a contract was created: the emergency stop, the FerrataRegistry library, the vault and the test token were deployed by the deployer in that order | |
| vault immutables | the vault's `checkpointImageId` is `checkpoint_b`'s id and its `releaseImageId` the release program's (the run's, as its records give them); `mode` is rule (B); `lezConfigId` = slot 2 = the run's configId; `genesisState` the run's; `nMin`/`tMin`; its verifier is the deployed emergency stop wrapping RISC Zero's Groth16 verifier; the FerrataRegistry library's address is linked into the vault's code; the fee recipient | that `0x8f0A…A2A0` is RISC Zero's verifier: its code can be compared with `risc0-ethereum` v3.0.0's build (not done here) |
| configId and genesisState recomputed | sha256 of the 261-byte configuration preimage from LEZ session A's records equals the configId the vault holds; `bedrockGenesisId` = sha256 of the deployment settings; the genesis state recomputed from the tip T0 and the channel's key set equals the vault's | the LEZ devnet's records (no public LEZ network exists to read them from): the proof in the next checks is what binds them |
| every recorded transaction | each transaction in `manifest.json` has a receipt with status 1, in the recorded block, from the deployer, to the vault, with the recorded gas used; says whether its block is finalized | |
| deployer's nonce | the deployer sent exactly the seven deployment transactions plus the recorded ones (or reports how many later ones) | |
| deposits | each recorded deposit's commitment, recomputed from the depositor, j, the kind and the destination, is in exactly one `Deposit` event, from the deployer, in ETH, with index j (deposits by anyone else are counted, not failed: the vault is public); `nextLeafIndex` = the number of `Deposit` events; the ETH every deposit transaction sent and the vault's balance | the commitments are recomputable only because this run used the **published test seed** of `bridge/e2e`; a real user's seed is secret |
| captures | each capture's `header.rlp` hashes to the chain's block of that number, which is final; the recorded `eth_getProof` answer verifies against that header's state root by Merkle proof (checked here, not asked of the endpoint): the vault's code hash and every slot LEZ imported (deposit root, leaf count, configId, the ETH registry entry, fee schedules) | |
| checkpoint journal fields | the 17-word journal names this chain, this vault, this configId, nMin = tMin = 1, parent = genesisState, as many imported leaves as deposits, as many burns as LEZ burn notes, and tip T1 = session B's last block (id and hash); session B starts at session A's tip | |
| checkpoint submitted | the `CandidateSubmitted` event of the submit transaction carries the journal's new state, parent, burn root, counts and tip; **the recorded seal verifies for the vault's own `checkpointImageId` and sha256(journal) in the vault's own verifier** (`eth_call`), and a seal changed in one byte is refused | RISC Zero's proof system |
| checkpoint final | after the wait W: `finalState` = the journal's new state, `checkpointCount` ≥ 1, the burn root accepted; before it, PENDING with the time finalization opens | |
| releases | each release's journal verifies with its seal for the vault's `releaseImageId`; once paid: the nullifier is spent, the `Release` event names the recorded recipient, token, amount and the checkpoint's burn root, and the vault's balance is at most the deposits minus what the releases paid out (a release's fees are credited to `owed` and stay in the vault) and at least `escrowed` + `totalOwed` | |

Measured on 11 November 2026 against crown-live-1's records while the run waited W (submit at Sepolia block
12,111,746; finalization opens 13:27:48Z): **10 passed, 2 pending, 0 failed**, 48 RPC calls to the public endpoint
(publicnode).
After the wait (records fetched again at 13:34Z), with the captures' check: **12 passed, 1 pending, 0 failed**:
finalization and release 0 on chain, release 1 not yet sent; the same on a second, independent endpoint
(`rpc.sepolia.ethpandaops.io`). One byte of a capture's proof changed: the captures' check fails. The same records with one commitment, one byte of the seal and the genesis state
changed: **5 failed**, exit status 1 ([`e2e/runs/verify-anyone/`](../../e2e/runs/verify-anyone/README.md)).
After DONE, on the records committed in [`e2e/runs/crown-live-1/`](../../e2e/runs/crown-live-1/README.md): **13 passed, 0 pending, 0
failed**, 61 RPC calls, on publicnode and on ethpandaops (release 1 final). The releases check had first failed there:
it counted each `Release` event's gross amount as paid out, but the vault credits the checkpoint fee to the prover's
`owed`; it now subtracts what each release credited. Controls on the final records: 5, 1 and 1 failed (a commitment, a
seal byte and the genesis state; a capture's proof node; a release's recipient).

On 12 November 2026, after DONE, on the records of the live run on the final programs committed in
[`e2e/runs/crown-live-3/`](../../e2e/runs/crown-live-3/README.md): **13 passed, 0 pending, 0 failed**, 61 RPC calls, on
publicnode and on ethpandaops; its image-id check reads "crown-live-3's vault: 6 of 6 = docs/image-ids.md", and
crown-live-1's records again give 13 passed, 0 failed on both. Controls: crown-live-3's records with crown-live-1's six
ids, and crown-live-1's with the final ids: 3 failed each (the pin, the vault's immutables, configId), exit status 1.

## 2. The programs: `scripts/reproduce-image-ids.sh`

A RISC Zero image id is a hash of the program. A guest built outside RISC Zero's Docker build would embed where it was
built: the absolute paths of its sources (panic locations in the tree's shared crates, the upstream LEZ sources and the
cargo registry) and, through cargo's `-C metadata`, the paths of its path dependencies, which name every symbol and so
order the code. This tree's guest builds do not: [`scripts/guest-rustc`](../../scripts/guest-rustc) (cargo's
`build.rustc-wrapper`, set in [`.cargo/config.toml`](../../.cargo/config.toml)) remaps the tree, the upstream sources,
the cargo home and the RISC Zero toolchains to fixed names and replaces cargo's path-dependent `-C metadata`, for the
zkVM target only (every other rustc call is passed through). So any checkout, at any path and with any `$HOME`, builds
the same guests to the same ids, given the same toolchains:

```sh
scripts/reproduce-image-ids.sh build --out out    # in your checkout (toolchains and ../upstream as below)
scripts/reproduce-image-ids.sh compare --out out [--vault 0x…]   # rebuilt ids beside the recorded ones
scripts/reproduce-image-ids.sh docker            # the same inside scripts/reproduce/Dockerfile's pinned toolchains
```

- Toolchains: host Rust 1.93.0 (`checkpoint/`, `bridge/`) and 1.98.1 (`lez/`); RISC Zero 3.0.5 with guest Rust 1.88.0
  (`~/.risc0`) for `checkpoint/` and `bridge/` and guest Rust 1.91.1 in its own RISC Zero home for `lez/`
  (`RISC0_HOME_LEZ`, default `../risc0-home-lez`); LEZ v0.4.0-rc1 at commit 1806e87 and the Logos Blockchain revision
  d480efc6 beside the tree in `../upstream/` (`scripts/fetch-upstream.sh`). From the root,
  `RISC0_HOME_LEZ=$PWD/../risc0-home-lez scripts/ci/install-risc0.sh` installs the RISC Zero toolchains, the LEZ
  home where `build` looks for it (without `RISC0_HOME_LEZ` the script installs it in `~/risc0-home-lez`, CI's place,
  and `build` then stops at "missing …/risc0-home-lez").
  [`scripts/reproduce/Dockerfile`](../../scripts/reproduce/Dockerfile) installs exactly these.
- `build`: the three `methods` builds in place (`lez`: `cargo build -p methods --locked` with the LEZ RISC Zero home;
  `checkpoint`, `bridge`: `cargo build --release -p methods --locked`), with `RUSTC_WRAPPER` set to the wrapper (an
  inherited one, such as sccache, would bring the paths back); `out/ids.tsv` lists all 14 guests (risc0-build's id,
  the R0BF binary's sha256 and size, the user ELF's sha256, `r0vm --id` as a cross-check); the crown run's six are
  also copied out, the LEZ programs' ids printed with `r0vm --id` and the other four turned into R0BF programs with
  `ferrata-e2e program-binary` (the v1compat kernel, as risc0-build 3.0.5 does).
- `compare` ([`scripts/reproduce/compare_ids.py`](../../scripts/reproduce/compare_ids.py)): the crown run's six
  programs beside [`e2e/crown/README.md`](../../e2e/crown/README.md)'s table and, with `--vault`, a crown vault's
  immutables read over RPC; every guest beside [`docs/image-ids.md`](../image-ids.md); exit 1 if one differs.
  Two v5 vaults are live on Sepolia. crown-live-3's, `0x04eE…e44c`, holds the final programs: `--vault` on it gives 6
  of 6 (below). crown-live-1's, `0xfE3199fa…9E54`, ran programs built before the wrapper (below): `--vault` on it
  prints DIFFER for `checkpoint_b` and `release` and exits 1, as expected.

| Program | Where its id is used |
|---|---|
| attestation (`ferrata_lez.bin`) | deployed on the LEZ devnet; part of the attestation root in configId |
| bridge (`ferrata_bridge.bin`) | deployed on the LEZ devnet; hashed into configId, which the vault holds |
| `checkpoint_b` | the vault's `checkpointImageId` (rule (B)) |
| `checkpoint` (rule A), `segment` | the other finality rule and its segment program; not on this mode-(B) vault |
| release | the vault's `releaseImageId` |

What equal ids prove: the vault accepts checkpoint and release proofs of exactly the programs this source builds, and
LEZ ran the bridge program this source builds, so reading the source is reading what runs. What they do not prove: that
the source is correct (that is the tests', the reviews' and an audit's job).

**Proven on our build box** (a Modal CPU container; it has no Docker, so `build` ran there directly; the Dockerfile
was built later, below): run on the Modal build box, 11 Nov 2026 23:03–23:09Z (`spike`-side script `twopath.sh`, logs outside the repository): build A with the tree at `/build/a/ferrata` and `HOME=/build/home-a`, build B with the tree at `/build/elsewhere/deeper/checkout-b`, `HOME=/build/users/other-home` and real copies of `~/.risc0` and of the LEZ RISC Zero home at other paths, both from a fresh target directory, in parallel (225 s and 227 s once the cargo registry was warm); their dep-info files show each compiled from its own paths (1,363 and 1,368 references to the tree, 17,190 and 17,322 to the cargo home), no binary names either build's tree, `HOME` or the box's own paths, and `ids.tsv` of A and B are identical: 14 of 14 guests with the same image id, R0BF binary and user ELF, byte for byte; the crown run's six programs and the four R0BF files compared with `cmp`, equal.

**Again at a third path** (walkthrough, 12 Nov 2026, the Modal build box, the tree at another directory, the LEZ
RISC Zero home freshly installed by `install-risc0.sh` into a new directory): `build` 3 min 32 s, `compare` 14 of 14
guests equal to `docs/image-ids.md` and 6 of 6 crown programs equal to `e2e/crown/README.md`'s table; with
`--vault 0xfE3199fa…9E54`, 4 of 6 (`checkpoint_b` and `release` are crown-live-1's, as said above).

**The live vault of the final programs** (12 Nov 2026, `crown-live-3`; `build` on the Modal build box at this
worktree's path, 2 CPUs, 563 s, then `compare --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` against publicnode):
**6 of 6** crown programs equal `e2e/crown/README.md`'s table, the vault's `checkpointImageId` and `releaseImageId`
equal to the rebuilt `checkpoint_b` and release, and **14 of 14** guests equal `docs/image-ids.md`
([`e2e/runs/crown-live-3/compare-vault.txt`](../../e2e/runs/crown-live-3/compare-vault.txt)).

**In the pinned image** (12 Nov 2026, [`scripts/reproduce/`](../../scripts/reproduce/README.md) on Modal: the
Dockerfile built as an image, the tree at `/work/ferrata`): `build` and `compare`, 14 of 14 guests and 6 of 6 crown
programs equal.

**The first live run, `crown-live-1`**, ran programs built before the wrapper existed, at the maintainer's checkout
path on the build box: those ids (`e2e/crown/README.md`, "The programs of crown-live-1") reproduce only from that
commit (db9f034) at that path, and did so on a cold box, 6 of 6 equal to the vault's immutables
([`e2e/runs/verify-anyone/`](../../e2e/runs/verify-anyone/README.md)); the same build at another path gave 0 of 6 (the
control there), which is what the wrapper removes. Runs from `crown-live-2` on use the path-independent programs;
`crown-live-3` deployed them live.

## 3. A vault's state: the admin view

[`admin/index.html`](../../admin/index.html) is one static page with its JavaScript inline: open it from disk, type
any JSON-RPC endpoint and the vault's address, and it reads the vault with read-only methods only (it refuses any
other) and shows its whole state, each value with the getter, event or storage slot it came from, and what the
contract does not expose. It rebuilds the deposit root from the `Deposit` events and compares it with the vault's,
and checks the published registry and fee hashes against the events. `node admin/dump.mjs <rpc> <vault>
<from-block>` runs the same code without a browser; `node admin/dump.mjs --selftest` checks its keccak, SHA-256 and
ABI code (232 checks). **Proven** on the live vault `0xfE3199fa…9E54` at Sepolia block 12,113,610 (node) and
12,113,760 (headless Chromium from `file://`), and on a local anvil vault with freezes in force and a pending
candidate ([`admin/README.md`](../../admin/README.md), [`admin/results/`](../../admin/results/)).
