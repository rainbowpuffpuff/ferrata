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
stand-in"). No LEZ node ran, the bridge program (`lez/BRIDGE-RESULTS.md`, built later) did not run
on this zone, the blocks that carry calls to the bridge are not executed, and the zone's 16 blocks are 10 s apart in their own timestamps, not hours. The
wallet is a test seed. The proofs are about that stand-in zone. A rule (B) checkpoint over REAL sequencer
blocks (the lab's devnet: one validator, dev-mode leader proofs, n = 1; no burns, no bridge program) was proven
later and accepted by the v5 vault: [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md), "Real LEZ blocks".

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
| `beacon.json` | the block whose beacon the finalisation echoes: number, `prevrandao`, nonce. Here a chosen number and a labelled test `prevrandao`, which the Foundry test sets with `vm.roll` and `vm.prevrandao`; on a chain, the block that mined `recordBeacon` |
| `receipts/<proof>/` | succinct and Groth16 receipts, the on-chain seal, the journal, timings |
| `fixture.json` | what [`vault/test/real`](../vault/test/real/) and [`vault/script`](../vault/script/) read |

[`runs/fork/`](runs/fork/README.md) is the copy used to rehearse the Sepolia scripts on a local
fork, with that fork's beacon and the finalisation built for it;
[`runs/fork-driver/`](runs/fork-driver/README.md) the same for the one-command driver;
[`runs/cloud-fork/`](runs/cloud-fork/README.md) the same driver run in the cloud ("Running it in the cloud");
[`runs/sepolia-live/`](runs/sepolia-live/README.md) the first attempt on Sepolia, which ran out of gas at its first
transaction; [`runs/sepolia-live-2/`](runs/sepolia-live-2/README.md) the prepared run that replaces it (deployer nonce
1, paired with the LEZ deployment of the inbound plan); [`runs/cloud-fork-2/`](runs/cloud-fork-2/README.md) its
rehearsal in the cloud and its dry pass against Sepolia's node.

The zone, by block id (the producer of a block is the key at index id mod 4):

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
sources were the ones in this repository until the checkpoint's `ImportRoot` fix (below): checkpoint
`0x95b816ed2614e340c85fed3ba3dad1f70e7dff2da1363406ade87e1333583caf` (the image `gpu-prover/`
measured), release `0xdda76672e6e05535117ed6880227a414420344cba9c47b6e112c7dabc5b027c5` (the image
`bridge/RESULTS.md` measured). `runs/sepolia/programs.json` has their sha256.

**After the checkpoint's `ImportRoot` fix.** The bridge program sends its `ImportRoot` with the
import's proofs after the four fields, and the checkpoint now decodes exactly that and refuses the
bare four fields (`checkpoint/README.md`, `lez/BRIDGE-RESULTS.md` §5). This fixture's block 2 holds
the bare 81-byte form, which the program of this run counted. Executed on the recorded candidate and
finalisation inputs, the fixed checkpoint image commits other journals: `importedLeafCount` is 0
instead of 2 and `newState` differs; the other fifteen words are equal (`host bench` on the build
box, image `0xd3c22b18…19c0`). So the fixture, both checkpoint proofs, their seals and the vault's
`test/real/` fixture are of the program before the fix; `checkpoint/fidelity/src/bin/e2e.rs` now
writes the import with an empty proof, and the fixture, the two proofs, the wraps and the vault
fixture have not been made again.

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

**Elsewhere.** All four seals of `runs/sepolia/` verify, by read-only `eth_call`, on RISC Zero's own
`RiscZeroGroth16Verifier` on Sepolia (`0x2a098988600d87650Fb061FfAff08B97149Fa84D`, directly and
through RISC Zero's emergency stop `0x724d375B5b622e15f0E64e9Deb76a4cB17877797`; checked at block
12,105,706). The ETH release's seal with the token release's journal digest, and the candidate's
seal with the finalisation's, are refused there with `VerificationFailed()`. No transaction was
sent for this.

**Nothing of this run is on a public network.** On Sepolia the first attempt
([`runs/sepolia-live/`](runs/sepolia-live/README.md)) sent one transaction, which ran out of gas ("What the round
trip costs on today's Sepolia" below); the round trip is prepared again as
[`runs/sepolia-live-2/`](runs/sepolia-live-2/README.md) (commands in `vault/README.md`) and has not been broadcast.

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

## What the round trip costs on today's Sepolia

The first attempt on Sepolia (10 November 2026, [`runs/sepolia-live/`](runs/sepolia-live/README.md)) ran out of gas
at its first transaction: Sepolia has run Glamsterdam since 6 October 2026, and its state gas (EIP-8037: 1,530 gas
per byte of new code, 183,600 per new account, 97,920 per new storage slot) is not in Foundry's EVM, so forge, anvil
and every rehearsal before it priced the round trip about six times too low (`vault/GAS.md`, "On Sepolia since
Glamsterdam", with the specifications and the constants measured on the node). Since then no gas figure of the run
comes from Foundry: each limit is Sepolia's own `eth_estimateGas` times 1.25, and each stage is priced by the node
before it is sent ([`gasplan.py`](gasplan.py), `vault/README.md`, "Sepolia round trip").

The sixteen transactions of [`runs/sepolia-live-2/`](runs/sepolia-live-2/README.md), as its cloud rehearsal sent
them ([`runs/cloud-fork-2/`](runs/cloud-fork-2/README.md)), each priced by Sepolia's node (reth 2.10.0, block
12,107,415) on the state of a fork of Sepolia at the deployer's real nonce, in order, on 10 November 2026 about 21:50
UTC (`runs/cloud-fork-2/gas-sepolia.json`). "Foundry" is anvil's receipt of the same transaction.

| Nonce | Stage | Transaction | Foundry | Sepolia, gas used | Sepolia `eth_estimateGas` | Gas limit sent (x 1.25) | Sepolia / Foundry |
|---:|---|---|---:|---:|---:|---:|---:|
| 1 | deploy | create RISC Zero's Groth16 verifier | 1,179,986 | 7,732,792 | 7,795,331 | 9,744,164 | 6.55 |
| 2 | deploy | create its emergency stop | 451,756 | 2,953,040 | 2,977,644 | 3,722,055 | 6.54 |
| 3 | deploy | create the test token | 508,036 | 3,315,490 | 3,342,971 | 4,178,714 | 6.53 |
| 4 | deploy | create the vault | 5,501,261 | 38,170,341 | 38,474,447 | 48,093,059 | 6.94 |
| 5 | deploy | registerToken (ETH) | 472,211 | 2,136,791 | 2,154,917 | 2,693,647 | 4.53 |
| 6 | deploy | registerToken (test token) | 473,049 | 2,138,129 | 2,156,266 | 2,695,333 | 4.52 |
| 7 | deploy | mint (test token) | 68,368 | 238,408 | 241,467 | 301,834 | 3.49 |
| 8 | deploy | approve (test token) | 46,331 | 128,251 | 129,343 | 161,679 | 2.77 |
| 9 | deposit0 | deposit 0: 0.001 ETH (public, to LEZ) | 193,072 | 552,152 | 557,701 | 697,127 | 2.86 |
| 10 | deposit1 | deposit 1: 0.001 ETH (private, to LEZ) | 143,906 | 260,526 | 263,761 | 329,702 | 1.81 |
| 11 | deposit2 | deposit 2: 100 test tokens | 175,429 | 375,237 | 399,156 | 498,945 | 2.14 |
| 12 | submit | submitCandidate | 507,370 | 1,381,770 | 1,393,904 | 1,742,380 | 2.72 |
| 13 | beacon | recordBeacon | 77,823 | 262,063 | 265,310 | 331,638 | 3.37 |
| 14 | finalize | finalize | 475,288 | 1,183,452 | 1,263,884 | 1,579,855 | 2.49 |
| 15 | release | release (ETH) | 367,957 | 619,477 | 625,561 | 781,952 | 1.68 |
| 16 | release | release (test token, relayer fee) | 388,219 | 648,447 | 675,619 | 844,524 | 1.67 |
| | | **all 16** | **11,030,062** | **62,096,366** | | | **5.63** |

The vault's creation needs more than the 2^24 gas a transaction may spend on execution and is valid all the same:
since EIP-8037 that cap binds execution gas only, and 36.4M of the 38.5M are state gas for its 23,650 bytes of code.
At the 74 wei per gas of the first attempt the whole run costs about 0.000000005 ETH; at 1 gwei it would cost 0.062
ETH. Mainnet has not scheduled Glamsterdam: until it does, mainnet charges the Foundry column.

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

For the round trip on Sepolia, [`sepolia-live.sh`](sepolia-live.sh) drives these steps and the
transactions of `vault/script/` as one resumable command (`vault/README.md`, "Sepolia round trip").

The finalisation proof can only be made after the beacon exists: `beacon.json` gives the nonce,
`run.sh inputs` rebuilds the zone with the echo block (blocks 1–9 are the same bytes), and only
`cp-e2e-finalize` is proven again. For the Foundry run the beacon block is chosen by the test
(`vm.roll`, `vm.prevrandao`); on a chain it is the block that mined `recordBeacon`.

## Running it in the cloud

[`sepolia-cloud.sh`](sepolia-cloud.sh) runs `sepolia-live.sh` itself on Modal, so that nothing is needed
from the laptop between `start` and `fetch`: the laptop can be switched off. The app is
[`cloud/app.py`](cloud/app.py) (`ferrata-sepolia`), in the workspace that `MODAL_PROFILE` of
`e2e/sepolia-live.env` names (always given per command, never activated):

| Function | What runs there |
|---|---|
| `runner` | the driver, every `forge script` and `cast` call, the cargo steps (`run.sh inputs`, `p254`, `groth16`, `fixture`) and r0vm (`identity_p254` on its CPU); one container per start, not preemptible, 2 cores and 4 GiB reserved, up to 16 cores and 32 GiB used, at most 4 h, no retries |
| `gpu_prove` | the checkpoint proof on one NVIDIA L4 (on an L40S when no L4 is free: also Ada, compute capability 8.9, the same binary), with the image recipe of [`gpu-prover/`](../gpu-prover/) and the CUDA prover it built (its Volume `ferrata-prover`, mounted read-only) |
| `groth16_wrap` | RISC Zero's prover image `risczero/risc0-groth16-prover:v2025-04-03.1`, run as a function |

```sh
e2e/sepolia-cloud.sh setup     # once (idempotent): the Secret with the key, then deploy the app
e2e/sepolia-cloud.sh start     # prints sender, nonce, balance, gas price; waits for a typed yes; returns at once
e2e/sepolia-cloud.sh status    # any time: the runner's state, the stages done, the last lines of the log
e2e/sepolia-cloud.sh fetch     # the run directory back into e2e/runs/sepolia-live-2/ (the runner's logs in work/cloud/)
e2e/sepolia-cloud.sh stop      # cancels a running runner and stops the app (`setup` again before the next start)
```

A real run takes about an hour and three quarters plus the wait for a GPU: 4 to 7 minutes of building (236 to
408 s in the rehearsals), then the driver: the deployment, a deposit, the finality of its block (about 15 minutes)
and the capture for LEZ, the second deposit and its finality and capture, the third deposit, the candidate, the
wait W (40 minutes), the beacon, the finalisation proof, the release delay, the releases. The default run is
`sepolia-live-2`. Then `fetch`, read the DONE line at the end of `e2e/runs/sepolia-live-2/sepolia-live.log`, give
`lez-capture-1/` and `lez-capture-2/` to the LEZ session, and commit the run directory as for the local run
(`vault/README.md`, "Sepolia round trip"). Source verification stays a separate step, from the laptop.

**The key.** `setup` reads the keystore file and the password file named by `ETH_KEYSTORE` and
`ETH_PASSWORD` by path, in its own process, and stores their contents, with `SEPOLIA_RPC_URL` and
`ETH_FROM`, in the Modal Secret `ferrata-sepolia-key`. The runner takes all four out of its environment
before anything else runs; only on a live run does it write the keystore and the password as two files
(mode 600) inside its own container, for forge to read through `ETH_KEYSTORE` and `ETH_PASSWORD` as on the
laptop. They are never on the Volume, in a log or on a command line. Whoever can use that workspace can sign
with this key, so it must stay a throwaway testnet key; after the run,
`MODAL_PROFILE=<profile> modal secret delete ferrata-sepolia-key` removes it.

**A start.** `start` checks the run directory and the inputs (the guest programs against the run's
`programs.json`, `vault/lib`), then uploads to the Volume `ferrata-sepolia`: a git bundle of `HEAD` (the
GPU manifest records the commit), the files git does not ignore as they are in the checkout, and the two
ignored inputs `e2e/programs/` and `vault/lib/` (never `e2e/sepolia-live.env`); the run directory only
if the cloud has no copy of it yet, since afterwards the cloud's copy is the run's state. It spawns the
runner and returns. The runner rebuilds the repository at the same absolute path, builds
`ferrata-e2e`, the zone builder and the vault before the first transaction (a build error sends nothing),
and runs the driver with `FERRATA_MRUN` unset and its two hooks pointing at `gpu_prove` and
`groth16_wrap` ([`cloud/hook.py`](cloud/hook.py)). It mirrors the run directory and forge's records of
the stages to the Volume every 20 s and when the driver stops. Started again after an interruption, the
driver reads the chain and continues after the last complete stage, exactly as on the laptop, with forge's
records back from the Volume.

**The GPU.** The hook queues the proof once and waits up to two hours for it (`FERRATA_GPU_WAIT_S`); Modal
keeps it queued until a GPU is free. It never cancels and queues again: a cancelled input keeps its place
in Modal's queue and is handed the next free GPU, only to be dropped there. In the rehearsal no L4 was free
for 50 minutes, two re-queues of an earlier version of the hook were lost that way, and the proof ran on
the L40S fallback. On Sepolia the wait only delays `finalize`: the vault sets no age limit on a beacon.

**What differs from the local run.** The driver is the same file; its one addition is the hook
`FERRATA_GPU_CMD` (unset, the GPU step is the `modal run` of the rehearsals, unchanged). Foundry in the
runner is the laptop's 1.3.6-nightly of 18 September 2025 (`nightly-a4d66e8…`; forge, cast and anvil have
the same sha256 as the laptop's): the 1.3.6 release refuses `forge script --unlocked` while `ETH_FROM` is
set. `identity_p254` runs on the runner's CPU instead of the build box.

**Rehearsed in the cloud again for the second run** ([`runs/cloud-fork-2/`](runs/cloud-fork-2/README.md)): `start
--fork cloud-fork-2` from a clean copy of `runs/sepolia-live-2/`, to `ALL CHECKS PASSED` and `DONE: 16 transactions`,
every stage priced by Sepolia's node before it was sent; then the dry pass of its sixteen transactions against the node.

**Rehearsed in the cloud** ([`runs/cloud-fork/`](runs/cloud-fork/README.md)): `start --fork cloud-fork` from a
clean copy of `runs/sepolia-live/` on 10 November 2026. The runner built for 304 s, started anvil on a fork
of Sepolia and ran the driver: deploy, submit and beacon in 23 s (the fork's clock moved), the finalisation
input in 18 s, 74 minutes of GPU queue, the proof in 7.9 s on an L40S, `identity_p254` in 30 s on the
runner's CPU, the prover image in 22 s, then finalize, release and `verify-sepolia.sh` in 10 s: ALL CHECKS
PASSED and `DONE: 15 transactions on chain 11155111, 10898492 gas`, every stage's gas equal to the laptop's
rehearsal. `start` returned 22 and 64 s after it was called (its two runs); `status` and `fetch` worked
during the build, during the GPU wait and after DONE.

**Cost.** Modal billed the app 1.46 USD for all of this work on 10 November (setup, a self-test, a first
start that forge stopped before any transaction, the rehearsal): CPU 0.76 and memory 0.23 (the runner is
non-preemptible and holds 2 cores and 4 GiB through every wait), GPU 0.46 (L40S 0.33, L4 0.13, two of the
three GPU containers spent on the cancelled inputs). Deployed and idle the app runs no container; `stop`
removes it, and `setup` brings it back in under a minute once its images are built.
