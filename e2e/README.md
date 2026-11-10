# The return leg, end to end

One run of LEZ → Ethereum in which the three prototypes of the return leg meet: the checkpoint
program ([`checkpoint/`](../checkpoint/)), the release program and the bridge core
([`bridge/`](../bridge/)) and the v3 vault ([`vault/`](../vault/)). Before this, the vault's tests
mocked every proof, the release proof had only met the v2 vault, and no contract had verified a
checkpoint proof.

Prototype evidence for a grant proposal. **Not audited. Not for production.**

**What is real.** On the Ethereum side nothing is mocked: RISC Zero's unmodified
`RiscZeroGroth16Verifier` (risc0-ethereum v3.0.0) behind its unmodified
`RiscZeroVerifierEmergencyStop`, the v3 `FerrataVault`, and real Groth16 proofs of the two guest
programs for every call that needs one.

**What is stand-in.** The LEZ side. The blocks and their Logos Blockchain inscriptions are built by
[`checkpoint/fidelity/src/bin/e2e.rs`](../checkpoint/fidelity/src/bin/e2e.rs) in their real formats
with test sequencer keys, as the bench fixtures are (`checkpoint/README.md`, "Real format vs
stand-in"). No LEZ node ran, no bridge program exists, the blocks that carry calls to the bridge
are not executed, and the zone's 16 blocks are 10 s apart in their own timestamps, not hours. The
wallet is a test seed. The proofs are about that stand-in zone.

## The fixture: `runs/sepolia/`

One consistent return leg for the vault that deployer `0x79A9…5C07` creates at its fourth nonce on
chain id 11155111 (Sepolia). Journals bind the chain id and the vault address, so a fixture is made
for one deployment; this one serves the Foundry run, a rehearsal on a fork and the testnet itself.

| File | What |
|---|---|
| `params.json` | the deployer, its four CREATE addresses (Groth16 verifier, emergency stop, test token, vault), the deposits and the burns |
| `deployment.json` | the stand-in zone's deployment: the preimage of `configId`, four sequencer keys (n = 4, t = 3; `nMin` = 3, `tMin` = 2), posting timeframe 900 slots, the genesis block and the genesis state `S` (the vault's `genesisState`) |
| `wallet.json` | the wallet: two deposits (ETH and the test token) with their commitments and the deposit root the zone imports; five burn notes in publication order, two of them the wallet's |
| `lez.json` | the zone as an observer reads it: 16 blocks with every message to the bridge, the state at `T`, both checkpoints |
| `cp-e2e-candidate.{input,json}`, `cp-e2e-finalize.{input,json}` | the checkpoint guest's two inputs and the journals the rule produces natively |
| `release-eth.{input,json}`, `release-erc20.{input,json}` | the release guest's inputs for the wallet's two burns, against the checkpoint's burn root |
| `programs.json` | sha256 and image id of the guest program binaries that were executed and proven |
| `executor.jsonl` | every guest executed on its input: cycles, segments, journal equal to the native one |
| `beacon.json` | the Ethereum block whose beacon the finalisation echoes: number, `prevrandao`, nonce |
| `receipts/<proof>/` | succinct and Groth16 receipts, the on-chain seal, the journal, timings |
| `fixture.json` | what [`vault/test/real`](../vault/test/real/) and [`vault/script`](../vault/script/) read |

The zone (block id = position, producer = id mod 4):

| Block | Contents |
|---|---|
| 1 | genesis |
| 2 | `ImportRoot` with the vault's deposit root after both deposits, leaf count 2 |
| 3 | burns 0 and 1 (1 is the wallet's: one ETH denomination to a fresh recipient, no relayer) |
| 4 | a burn of another configuration, burn 2, a burn in a reverted transaction: the first and last are ignored |
| 5 | burns 3 and 4 (3 is the wallet's: one token denomination, a relayer and the registered fee) |
| 6 | empty: `T` |
| 7–9 | empty: the candidate's support, by the three keys other than `T`'s producer |
| 10–11 | empty |
| 12 | `Echo{nonce}`: `Y` |
| 13–16 | empty: the finalisation's support (13, 15, 16; 14 is by `T`'s producer and is not counted) |

The candidate checkpoint proves blocks 2–9 with `echoNonce = 0`; the finalisation proves blocks
2–16 and echoes the beacon nonce. Both commit the same state: 5 burns, imported leaf count 2.

## Where the three prototypes had never met, and what the run found

The two sides exchange files only. The wallet side (`bridge/e2e`, on the bridge core) recomputes
every value both sides can compute and compares it with the checkpoint side's output; the vault's
tests then compare both with the contract.

| # | Compared | Result |
|---|---|---|
| 1 | `configId`: bridge core and checkpoint core, from the same preimage | equal |
| 2 | Deposit root after two deposits: bridge core and the vault (`depositRoot`, slot 0) | equal |
| 3 | Burn root over the five burns: the bridge core's full tree and incremental tree, and the checkpoint core's frontier | equal |
| 4 | Checkpoint state `S` at genesis and at `T`, and the checkpoint journal: bridge core and checkpoint core | **differed, fixed.** The bridge core had only the SPEC-v2 layouts: a 13-word journal under the domain `ferrata/vault/checkpoint/v2` and a key-set hash without the posting timeframe. The checkpoint program and the vault are at SPEC-v3 D3 (17 words, domain v3, `ferrata/keyset/v2`). No state or journal of the bridge core could equal the program's. The bridge core gained `CheckpointJournalV3` and `key_set_hash_v2` (`bridge/crates/ferrata-bridge-core/src/checkpoint.rs`, tested); with them both states and both journals are equal byte for byte. The v2 layouts stay for the v2 vault that `bridge/evm-verify` pins. |
| 5 | The checkpoint program's 17-word journal and the v3 vault's own encoding of `submitCandidate` and `finalize` arguments | equal byte for byte, both journals (`test_checkpointJournals_areTheVaultsEncoding`) |
| 6 | The release program's 11-word journal and the v3 vault's encoding | equal byte for byte, both releases (`test_releaseJournals_areTheVaultsEncoding`); the release program is unchanged from the one that met the v2 vault |
| 7 | The vault's `turnSeconds` and the zone's `posting_timeframe` | **differed between the prototypes' fixtures, aligned here.** The checkpoint bench fixtures use 30 slots, the vault's tests 900 s. The key-set hash binds the timeframe (D3), and the deployment check wants `turnSeconds = posting_timeframe × slot`. This fixture's zone has 900 slots of one second, LEZ's testnet value, and the vault 900 s. Nothing in the code changed. |
| 8 | Burn-tree capacity | **differs at one unreachable leaf, not changed.** The checkpoint core refuses the 2^32-th burn (its frontier cannot hold a full tree); the bridge core's tree accepts it. The program decides which burns exist, so no release can be proven for that leaf. |
| 9 | A comment in the checkpoint guest (`guests/checkpoint/src/main.rs`) still says it commits a 416-byte journal | **stale, not changed.** It commits 544 bytes. The guest's sources are left byte for byte as they were proven. |

Changes of the v3 specification that the prototypes do not implement (D11–D13) were not needed
for the run and were not touched.

**Image ids.** The guests were not rebuilt. Their program binaries embed the absolute paths of the
machine that built them, so a build anywhere else has other image ids
(`checkpoint/README.md`, UNVERIFIED 7). The run uses the recorded binaries, whose sources are the
ones in this repository, unchanged: checkpoint
`0x95b816ed2614e340c85fed3ba3dad1f70e7dff2da1363406ade87e1333583caf` (the image `gpu-prover/`
measured), release `0xdda76672e6e05535117ed6880227a414420344cba9c47b6e112c7dabc5b027c5` (the image
`bridge/RESULTS.md` measured). `runs/sepolia/programs.json` has their sha256. The new SPEC-v3
encodings were added to the bridge core without moving a line of `encoding.rs`, whose line numbers
the release guest's binary carries.

## Steps

`run.sh` has one subcommand per step; `<run>` is a directory under `runs/`.

| Step | Command | Runs |
|---|---|---|
| addresses and amounts | `run.sh params <run> <chain id> <deployer> <first nonce>` | anywhere (`cast`) |
| inputs, native journals, executor check | `run.sh inputs <run>` | `cargo` (Rust 1.98.1 for the LEZ crates, 1.93 for the rest), r0vm 3.0.5 executor |
| release proofs | `run.sh prove-release <run>` | r0vm 3.0.5 on a CPU |
| checkpoint proofs | `run.sh stage-gpu <run>`, then [`gpu-prover/`](../gpu-prover/) with `FERRATA_INPUTS=runs/<run>/work/gpu-inputs` | one NVIDIA L4 |
| Groth16 wrap, step 1 | `run.sh p254 <run> <proof>` | r0vm (`identity_p254`), CPU |
| Groth16 wrap, step 2 | RISC Zero's prover image on `work/<proof>/`: `docker run --rm -v <dir>:/mnt risczero/risc0-groth16-prover:v2025-04-03.1` | x86-64, about 8 GiB |
| receipt and seal | `run.sh groth16 <run> <proof>` | anywhere |
| fixture | `run.sh fixture <run>`, then copy `fixture.json` to `vault/test/real/fixtures/return-leg.json` | anywhere |

The two wrap steps are exactly what risc0-zkvm 3.0.5 does in `succinct_to_groth16`; they are
split because the first needs r0vm and no Docker and the second is one container run.
`ferrata-e2e wrap` does both through r0vm where Docker is at hand.

The finalisation proof can only be made after the beacon exists: `beacon.json` gives the nonce,
`run.sh inputs` rebuilds the zone with the echo block (blocks 1–9 are the same bytes), and only
`cp-e2e-finalize` is proven again. For the Foundry run the beacon block is chosen by the test
(`vm.roll`, `vm.prevrandao`); on a chain it is the block that mined `recordBeacon`.
