# The burn root's acceptance in the privacy suite: why it is still the mock

12 November 2026, branch `privacy-root` (from `4327f41`). The question: can the one stand-in left in the suite's
real-proof round trip ([RESULTS.md](RESULTS.md), EVIDENCE row 89) be closed? On Ethereum the vault accepts the burn root
of the 20 burns through the mock checkpoint seal. Closing it would take a rule (B) checkpoint proof (`checkpoint_b`
`0xf8116722…626e`, [docs/image-ids.md](../../image-ids.md)) over the harness's blocks that carry the burns, wrapped to
Groth16 and accepted by RISC Zero's Groth16 verifier, as the crown live runs and row 24 do.

**Answer: no, not with the harness's blocks as they are.** The checkpoint program's own code refuses them, and the
harness has none of the inputs that rule (B) trusts: no hash-linked chain, no channel key set, no inscription of the tip
and no attestation. With these blocks, the only way to make the proof pass would be to make up the chain and the
attestations with keys the test holds. That would be a real proof that nobody vouched for, so it was not done. The burn root stays on the mock;
every other proof in the round trip is real, as in RESULTS.md.

## What was checked

`checkpoint/realblocks/src/bin/privacy-blocks.rs` runs checkpoint-core's own functions, the ones the guest runs, over
universe A's blocks in the data of run `privacy-real-1`: `lez.json` (84,769,000 B, sha256 `313a88fa…d51d`) and
`scenario.json` (sha256 `a3774036…ab9a`). The blocks are the deposit-root import, the 20 private claims and the 20 burns.
It ran natively on the Modal build box (`mrun`, Modal, 35 s, build included). Its output is
[`root/check.json`](root/check.json), with one row per block.

| check | result |
|---|---|
| blocks (ids 4..=44: import 4, claims 5–24, burns 25–44; one transaction each; 9,261,761 B) parsed strictly (`parse_block`) | 41 of 41 |
| block id = predecessor's + 1 | 40 of 40 |
| stored `hash` = `hash_parsed_block` | **0 of 41**: every stored hash is zero |
| stored `prev_block_hash` = the predecessor's hash | **0 of 40**: every one is zero |
| `run_checkpoint_b`, `T` = block 44, with what the harness has (no key set, no inscription, no attestation) | refused: `KeySetTooSmall { n: 0, n_min: 3 }` |
| the same with a placeholder key set of three label keys, so that the rule reaches the blocks | refused: `HashMismatch { index: 0 }` |
| the same over copies whose `hash` and `prev_block_hash` are filled in and linked (this changes the blocks; diagnostic only) | refused: `InsufficientAttestations { have: 0, need: 2 }` |
| burns the rule counts (bridge calls executed in public by non-reverted transactions, `Burn` for the suite's configId) | 20, one in each of blocks 25–44 (origin `PrivateToPublic`); root `0x17007674…8632`, count 20: **equal** to the scenario's `burn_root` and `burn_count`, the root the 20 real release proofs bind |

The configuration used for the three runs has the harness's fields (bedrockGenesisId `0x0b…0b`, channel `0x01…01`, the
bridge program, its image id, chain 1, vault `0xa0…a0`) and the suite's vault floors (3, 2). Its attestationRootId and
salt are zero because the suite does not write them. That does not matter: every run refuses before a journal, and the
burn count above uses the scenario's configId. Nothing was proven and nothing was sent to any chain.

## Why, item by item

1. **The blocks are not a chain.** The harness runs LEZ's state transition in-process, with no sequencer. Its `block()`
   (`lez/harness/tests/privacy_suite.rs`) wraps each transaction in the block layout so that the checkpoint's parser can
   read it. It leaves `prev_block_hash` and `hash` zero, as its comment says ("the harness does not chain blocks").
   `link_block` (`checkpoint/crates/checkpoint-core/src/rule.rs`) recomputes every block's hash and links it to its
   predecessor's.
2. **There is no parent tip.** Blocks 1–3 (the harness's setup) are never written or hashed. So there is no tip hash for
   the vault's `genesisState` to start from.
3. **There is no channel and no key set.** No Logos Blockchain node exists. The configuration's bedrockGenesisId
   `0x0b…0b` is a placeholder, not the hash of any node's deployment settings, and the channel `0x01…01` exists on no
   node. With no channel there is no key set `K`, no posting timeframe and no configTipHash for the state to bind.
4. **There is no producer inscription of `T`.** Since §4a of `checkpoint/RESULTS.md`, rule (B) checks `T`'s inscription:
   a Mantle `ChannelInscribe` of `T`'s bytes on the configuration's channel, signed by a key of `K`.
5. **There are no attestations.** Rule (B) needs `t(n)` attestation keys, each certified by a channel key of `K`. Each
   must sign the final-tier message for `T` or a descendant, with the `MsgId` of `T`'s inscription, once `T` is final by
   the node's rule.
6. Smaller differences, not blockers by themselves. The timestamps are the harness's clock (1,700,000,000,000 ms, then
   one-minute steps). The status byte says Finalized, while inscribed blocks say Pending (`checkpoint/RESULTS.md`, "Real
   LEZ blocks", finding 2). A sequencer's blocks also carry its fee and clock transactions.

**What does match.** The burns are in the clear, and the rule's own extraction gives exactly the scenario's root and
count. The bridge wire builds configId from the same preimage as checkpoint-core's `Config::config_id`
(`realblocks-crown` checks that the two agree). So nothing on the program side stands in the way. What is missing is the
chain and the parties who vouch for it.

## What it would take

**Route 1, real, as the crown live runs do** (`e2e/crown/README.md`, stages 1–9):

- **Run the suite's LEZ half against a real devnet**, not in process: a LEZ v0.4.0-rc1 sequencer with a Logos Blockchain
  node (the lab's or the crown run's devnet), whose blocks the sequencer makes, hashes, links and inscribes.
- **Take the configuration from that devnet** (crown stage 2): bedrockGenesisId = sha256 of the node's deployment
  settings, plus the channel's key set, posting timeframe and configTipHash. This changes configId. Every claim, burn,
  nullifier and release binds configId, so all 20 + 20 LEZ transactions and all 20 release proofs must be made again. The
  previous pass spent 0.395 USD in all.
- **Import the deposit roots for real.** On a sequencer, the attestation program accepts Ethereum records only from
  light-client updates it verifies; the harness writes synthetic records straight into LEZ state, which a sequencer
  cannot do. So the 20 deposits must be on a beacon chain the light client follows. The crown runs use Sepolia, a public
  network, which is outside this brief and needs the owner's approval. The alternative is a local beacon-chain devnet,
  and this repository has none.
- **Run the attestor.** The channel key certifies an attestation key (attestor-core). That key signs `FinalTip(T)` once
  `T` is final. `checkpoint/realblocks/src/bin/realblocks-crown.rs` builds the input from the node's answers.
- **Make the proofs.** `checkpoint_b` must run over the blocks since the parent tip. ESTIMATED from §3's measured
  2,684 cycles per KB: the suite's 9,261,761 B of blocks come to about 24M user cycles. Rule (B) adds 1.69M per attestor
  and about 0.9M for an empty `T`'s inscription, or about 13.5M if `T` is a burn block. That makes about 27M to 40M user
  cycles, or about 43 to 63 data-heavy segments (631k user cycles each, §3): about 2.5 to 4 minutes on one L4 (row 17:
  219.3 s for an hour with 50 private transactions, 60 segments), about 0.05 USD. Then identity_p254 and Groth16, as for the releases.
- **Deploy the vault in mode (B)** with `checkpoint_b`'s id and `genesisState` = the state at the parent tip. Its floors
  must be ones the devnet's committee meets: a one-validator devnet has `n` = 1, so `nMin` = 1, not the suite's 3. RISC
  Zero's Groth16 verifier checks both images. Then `submitCandidate`, `finalizeAttested` after Δ (1 h), the releases, and
  the gate rerun (`scripts/privacy/real.sh`, `scripts/privacy/adversary.py`). The checkpoint's public data are
  aggregates (burnRoot, burnCount, tip, key set), with nothing per user. The gate would classify its 260-byte seal and
  journal like the other fields.

**Route 2, the harness alone (not done).** Chain the blocks in the test with LEZ's block hash and generate a channel key
set. Sign `T`'s inscription and the attestations with keys the test holds, as `host gen-b` makes its fixtures (EVIDENCE
row 16). Then prove, wrap, and deploy the vault in mode (B). The proof would be real, but only keys the test made would
vouch for the chain. It would show nothing beyond what the check above shows (the program derives the suite's root from
the suite's burns), while reading as if a sequencer and attestors had vouched for it. That is the fake the brief rules
out.

## What is still not real (unchanged)

- The burn root's acceptance: one mock checkpoint seal for all 20 burns (this page).
- The Ethereum records that LEZ reads: synthetic.
- The naive variants: models (the naive releases stay on the mock).

## Reproduce

```
cd checkpoint && cargo build --release --locked -p realblocks --bin privacy-blocks
./target/release/privacy-blocks --lez RUN/data/lez.json --scenario RUN/data/scenario.json --out ../docs/privacy/real/root/check.json
```

`RUN` is a run directory of `scripts/privacy/real.sh gpu RUN` (`target/privacy/real/RUN`, not committed). The checks
do not depend on the proofs: in dev mode (`real.sh dev`) the same `block()` builds the blocks, with fake receipts in
their transactions.
