# Check it yourself

Two scripts let anyone check the crown run's round trip on Sepolia and the programs behind it without trusting
anything we say: one reads the chain, the other rebuilds the programs. Neither signs or sends a transaction.

| Script | Needs | Answers |
|---|---|---|
| [`scripts/verify-sepolia.py`](../../scripts/verify-sepolia.py) | Python 3.9 or later (standard library only), any Sepolia JSON-RPC | Did the run's transactions, deposits, checkpoint and releases happen on Sepolia as its records say, on a vault configured as we say? |
| [`scripts/reproduce-image-ids.sh`](../../scripts/reproduce-image-ids.sh) | Docker, ~30 GB of disk, an x86-64 machine | Are the programs the vault accepts proofs of really built from this repository's source? |

## 1. The round trip: `scripts/verify-sepolia.py`

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-1                 # public endpoint (publicnode)
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-1 --rpc http://localhost:8545   # your own node
```

It reads the run's records (`identity.json`, `deploy.json`, `params.json`, `run.json`, `manifest.json`, `crown.json`,
`releases/`, `lez/session-a/`, `lez/session-b/`), asks the chain, and prints one line per check: `PASS`, `FAIL`
(exit status 1) or `PENDING` (the run has not reached that stage, on chain and in the records; not a failure). It
uses only read methods (`eth_chainId`, `eth_getCode`, `eth_getBalance`, `eth_getTransactionCount`,
`eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_getLogs`, `eth_getBlockByNumber`, `eth_call`), computes
Keccak-256 itself, and needs nothing from this repository but the records (and, if present, `e2e/crown/README.md`,
whose id table it compares with the ids pinned in the script).

| Check | What a PASS proves | What it still trusts |
|---|---|---|
| chain | the endpoint serves Sepolia (chain id 11155111); prints head and finalized block | the endpoint (use your own node to remove it) |
| image ids | the README table, the run's `params.json`, `programs/ids.jsonl` and the ids LEZ session A deployed are one set of six ids | nothing on chain yet: the next checks tie them to the vault, and section 2 ties them to source |
| deployment | each contract is at CREATE(deployer, N + i) for deploy.sh's seven nonces, code exactly where a contract was created: the emergency stop, the FerrataRegistry library, the vault and the test token were deployed by the deployer in that order | |
| vault immutables | the vault's `checkpointImageId` is `checkpoint_b`'s id and its `releaseImageId` the release program's (the table's); `mode` is rule (B); `lezConfigId` = slot 2 = the run's configId; `genesisState` the run's; `nMin`/`tMin`; its verifier is the deployed emergency stop wrapping RISC Zero's Groth16 verifier; the FerrataRegistry library's address is linked into the vault's code; the fee recipient | that `0x8f0A…A2A0` is RISC Zero's verifier: its code can be compared with `risc0-ethereum` v3.0.0's build (not done here) |
| configId and genesisState recomputed | sha256 of the 261-byte configuration preimage from LEZ session A's records equals the configId the vault holds; `bedrockGenesisId` = sha256 of the deployment settings; the genesis state recomputed from the tip T0 and the channel's key set equals the vault's | the LEZ devnet's records (no public LEZ network exists to read them from): the proof in the next checks is what binds them |
| every recorded transaction | each transaction in `manifest.json` has a receipt with status 1, in the recorded block, from the deployer, to the vault, with the recorded gas used; says whether its block is finalized | |
| deployer's nonce | the deployer sent exactly the seven deployment transactions plus the recorded ones (or reports how many later ones) | |
| deposits | the `Deposit` events: depositor, ETH, leaf index = j, amount; each commitment recomputed from the depositor, j, the kind and the destination equals the event's and the record's; `nextLeafIndex` = the number of deposits; the ETH the deposit transactions sent and the vault's balance | the commitments are recomputable only because this run used the **published test seed** of `bridge/e2e`; a real user's seed is secret |
| checkpoint journal fields | the 17-word journal names this chain, this vault, this configId, nMin = tMin = 1, parent = genesisState, as many imported leaves as deposits, as many burns as LEZ burn notes, and tip T1 = session B's last block (id and hash); session B starts at session A's tip | |
| checkpoint submitted | the `CandidateSubmitted` event of the submit transaction carries the journal's new state, parent, burn root, counts and tip; **the recorded seal verifies for the vault's own `checkpointImageId` and sha256(journal) in the vault's own verifier** (`eth_call`), and a seal changed in one byte is refused | RISC Zero's proof system |
| checkpoint final | after the wait W: `finalState` = the journal's new state, `checkpointCount` ≥ 1, the burn root accepted; before it, PENDING with the time finalization opens | |
| releases | each release's journal verifies with its seal for the vault's `releaseImageId`; once paid: the nullifier is spent, the `Release` event names the recorded recipient, token, amount and the checkpoint's burn root, and the vault's balance is at most deposits minus releases | |

Measured on 11 November 2026 against crown-live-1's records while the run waited W (submit at Sepolia block
12,111,746; finalization opens 13:27:48Z): **10 passed, 2 pending, 0 failed**, 48 RPC calls to the public endpoint.
After the wait (records fetched again at 13:34Z): **11 passed, 1 pending, 0 failed**: finalization and release 0 on
chain, release 1 not yet sent. The same records with one commitment, one byte of the seal and the genesis state
changed: **5 failed**, exit status 1 ([`e2e/runs/verify-anyone/`](../../e2e/runs/verify-anyone/README.md)).

## 2. The programs: `scripts/reproduce-image-ids.sh`

A RISC Zero image id is a hash of the program, and a guest built outside RISC Zero's Docker build embeds the absolute
paths of its sources (panic locations in path dependencies, the cargo registry and git checkouts). The programs of the
live run were built on our build box (Ubuntu 24.04, `HOME=/home/q`) with the tree at the **crown build path**
`/home/q/Downloads/software_i_built/logos-rfp/spike/ferrata-wt-crown` (11 Nov 05:08–05:17Z, commit db9f034; no
guest source or lockfile changed between it and this branch; `build` below repeats that build's steps). To get the same ids anyone
must build at that path with the same toolchains, which is what the Dockerfile provides:

```sh
scripts/reproduce-image-ids.sh docker          # builds scripts/reproduce/Dockerfile, then build + compare inside it
```

- `scripts/reproduce/Dockerfile`: the build box's image (Ubuntu 24.04, `HOME=/home/q`; host Rust 1.93.0, 1.98.1 for
  `lez/`; RISC Zero 3.0.5 with guest Rust 1.88.0 for `checkpoint/` and `bridge/` and 1.91.1 under
  `spike/risc0-home-lez` for `lez/`; LEZ v0.4.0-rc1 at commit 1806e87 and the Logos Blockchain revision d480efc6 at
  `spike/upstream/`).
- `build` (inside the container only; it refuses elsewhere): copies the tree to the crown build path without its
  build outputs, runs the three `methods` builds exactly as the crown build did (`lez`: `RISC0_HOME=spike/risc0-home-lez
  RISC0_BUILD_LOCKED=1 cargo build -p methods --locked`; `checkpoint`, `bridge`: `cargo build --release -p methods
  --locked`), takes the six guests, prints the LEZ programs' ids with `r0vm --id` and turns the other four into R0BF
  programs with `ferrata-e2e program-binary` (the v1compat kernel, as risc0-build 3.0.5 does), whose ids it prints.
- `compare` ([`scripts/reproduce/compare_ids.py`](../../scripts/reproduce/compare_ids.py)): one line per program, the
  rebuilt id beside the README's and, for `checkpoint_b` and release, the vault's immutable read over RPC; exit 1 if
  one differs.

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

**Proven on our build box** (a Modal CPU container whose image the Dockerfile mirrors; it has no Docker, so the
script's `build` step ran there directly, on a cold box, with the tree copied to the crown build path inside the box
only): 7 min 27 s for the three builds, **6 of 6 ids equal** the README table, and `checkpoint_b` and release equal
the vault's immutables read on Sepolia; the six user ELFs and four R0BF programs are byte-identical to the ones the
live run uses ([`e2e/runs/verify-anyone/`](../../e2e/runs/verify-anyone/README.md)). The Dockerfile itself has not
been built yet (no Docker on the box, and our laptop stays free); its steps are those of the build box's image.

The same build at another path gives other ids: see the control in
[`e2e/runs/verify-anyone/`](../../e2e/runs/verify-anyone/README.md), which is why the path is part of the recipe. A
build independent of the path (RISC Zero's Docker guest build, `RISC0_USE_DOCKER`, or path remapping) would give new
ids, and so a new deployment; until then the path is fixed and published here.
