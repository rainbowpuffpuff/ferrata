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

[`runs/fork/`](runs/fork/README.md) is the copy used to rehearse the Sepolia scripts on a local
fork, with that fork's beacon and the finalisation built for it.

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
| 4 | Checkpoint state `S` at genesis and at `T`, and the checkpoint journal: the bridge side and the checkpoint core | **differed, resolved beside the core.** The bridge core has only the SPEC-v2 layouts: a 13-word journal under the domain `ferrata/vault/checkpoint/v2` and a key-set hash without the posting timeframe. The checkpoint program and the vault are at SPEC-v3 D3 (17 words, domain v3, `ferrata/keyset/v2`), so no state or journal of the bridge core can equal the program's. The SPEC-v3 journal and key-set hash were written a second time from the specification text, in `bridge/e2e/src/checkpoint_v3.rs` (host only, tested); with them both states and both journals equal the checkpoint core's byte for byte. The bridge core itself was left at SPEC-v2: the guests are built from it, and adding the two encodings to it changed the release guest's program binary (see "Image ids"). Its v2 layouts are still what the v2 vault pinned in `bridge/evm-verify` reads. |
| 5 | The checkpoint program's 17-word journal and the v3 vault's own encoding of `submitCandidate` and `finalize` arguments | equal byte for byte, both journals (`test_checkpointJournals_areTheVaultsEncoding`) |
| 6 | The release program's 11-word journal and the v3 vault's encoding | equal byte for byte, both releases (`test_releaseJournals_areTheVaultsEncoding`); the release program is unchanged from the one that met the v2 vault |
| 7 | The vault's `turnSeconds` and the zone's `posting_timeframe` | **differed between the prototypes' fixtures, aligned here.** The checkpoint bench fixtures use 30 slots, the vault's tests 900 s. The key-set hash binds the timeframe (D3), and the deployment check wants `turnSeconds = posting_timeframe × slot`. This fixture's zone has 900 slots of one second, LEZ's testnet value, and the vault 900 s. Nothing in the code changed. |
| 8 | Burn-tree capacity | **differs at one unreachable leaf, not changed.** The checkpoint core refuses the 2^32-th burn (its frontier cannot hold a full tree); the bridge core's tree accepts it. The program decides which burns exist, so no release can be proven for that leaf. |
| 9 | A comment in the checkpoint guest (`guests/checkpoint/src/main.rs`) still says it commits a 416-byte journal | **stale, not changed.** It commits 544 bytes. The guest's sources are left byte for byte as they were proven. |

Changes of the v3 specification that the prototypes do not implement (D11–D13) were not needed
for the run and were not touched.

**The zone cannot be rebuilt bit for bit, so it is kept.** LEZ signs transactions with fresh
randomness: two runs of the builder give other block hashes from block 2 on, and so another
`newState`. A finalisation has to prove the very blocks the candidate proved (the vault looks the
candidate up by `newState`), and it is built after the beacon, in a second run. The builder
therefore takes the blocks already inscribed in the run directory (the candidate input is the
zone's record), signs only what is new, and stops if it cannot rebuild a stored block byte for
byte. Run again with nothing new, it reproduces every input exactly.

**Image ids.** The guests were not rebuilt and their sources were not touched. Their program
binaries embed the absolute paths of the machine that built them, so a build anywhere else has
other image ids (`checkpoint/README.md`, UNVERIFIED 7). The run uses the recorded binaries, whose
sources are the ones in this repository: checkpoint
`0x95b816ed2614e340c85fed3ba3dad1f70e7dff2da1363406ade87e1333583caf` (the image `gpu-prover/`
measured), release `0xdda76672e6e05535117ed6880227a414420344cba9c47b6e112c7dabc5b027c5` (the image
`bridge/RESULTS.md` measured). `runs/sepolia/programs.json` has their sha256.

How little it takes to move an image id was measured on the way. A first version of this work
added the SPEC-v3 encodings to `ferrata-bridge-core` in `checkpoint.rs`, a file the release guest
uses nothing of, and left `encoding.rs` untouched. Built twice at one path on one machine, the
release guest's program binary had sha256 `f7342a9a…170b` with the addition and `73a82f26…4695`
without it. The addition was moved out of the core, which is again byte for byte what the recorded
binaries were built from.

## Results

Measured on 10 Nov 2026. "Box" is a 16-core cloud machine (32 threads; r0vm 3.0.5, CPU only), "L4" one
NVIDIA L4 in the cloud ([`gpu-prover/`](../gpu-prover/)), "prover image" RISC Zero's
`risc0-groth16-prover:v2025-04-03.1` run as a cloud function. Nothing was proven on the laptop.

| Proof | User cycles, segments | Succinct receipt | Wrap step 1 (`identity_p254`) | Wrap step 2 (prover image) |
|---|---|---|---|---|
| `cp-e2e-candidate` | 3,786,158; 5 | 15.0 s on the L4 (execute 0.24 s, segments 10.7 s, lift and join 4.1 s) | 18.7 s on the box | 14 s |
| `cp-e2e-finalize` | 3,831,033; 5 | 15.4 s on the L4 (0.23 s, 11.0 s, 4.1 s) | 18.6 s on the box | 14 s |
| `release-eth` | 80,501; 1 | 15.1 s on the box | 19.5 s on the box | 18 s |
| `release-erc20` | 80,501; 1 | 14.7 s on the box | 19.3 s on the box | 13 s |

Every succinct receipt is 224 KB (223,930 or 224,314 bytes), every seal 260 bytes with selector
`0x73c457ba`; every Groth16 receipt verifies against its image id and carries the native journal
(`receipts/<proof>/groth16.json`). The L4 queue, not the proving, set the pace: the two checkpoint
proofs waited 26 minutes for a GPU and then took 31 seconds together.

**The run** ([`vault/test/real/ReturnLeg.t.sol`](../vault/test/real/ReturnLeg.t.sol), 15 tests, all
pass; `cd vault && forge test --match-path 'test/real/*'`): the deployment is the one the proofs
bind; the journals are the vault's encodings; candidate, wait, beacon, finalisation, release delay
and both releases succeed with balances checked; every negative case is refused (the list is in
`vault/README.md`, "Tests"). Gas of every step: `vault/GAS.md`, "With the real verifier".

**Elsewhere.** The two release seals verify, by read-only `eth_call`, on RISC Zero's own
`RiscZeroGroth16Verifier` on Sepolia (`0x2a098988600d87650Fb061FfAff08B97149Fa84D`, directly and
through RISC Zero's emergency stop `0x724d375B5b622e15f0E64e9Deb76a4cB17877797`); the ETH seal
with the token release's journal digest is refused there with `VerificationFailed()`.

**The Sepolia stages on a local fork** (`anvil --fork-url` of a public endpoint, the deployer
impersonated; `vault/script/`, commands in `vault/README.md`; record in [`runs/fork/`](runs/fork/README.md)):
`deploy` put the four contracts on the fixture's addresses in ten transactions (9.09M gas), `submit`
had the candidate's real seal accepted (507,370 gas), and after 2,400 s `beacon` recorded a beacon
whose nonce comes from the fork's own random `prevrandao`. Only then could the finalisation be made:
the zone builder produced its input for that nonce on the candidate's stored blocks, the L4 proved
it in 15.9 s (after 14 minutes in the queue), and it was wrapped as the others (20.2 s, 20 s).
`finalize` accepted it (475,312 gas), and 120 s later `release` paid both releases (363,169 and
388,243 gas). `verify-sepolia.sh` passes on the 15 transactions: receipts, wiring, configuration,
every seal checked again through the deployed stop, the recipients' balances, and what must be
refused. What a fork does not give: its beacon is not Ethereum's (anvil draws `prevrandao` at random
for each block and offers no way to set it, which is also why the finalisation has to be proven in
real order there, exactly as on Sepolia), and its clock is moved by hand. The vault reads no
EIP-4788 beacon root; nothing of that was exercised or needed.

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
