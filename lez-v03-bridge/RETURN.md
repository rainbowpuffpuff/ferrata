# The return leg on the public LEZ testnet: a design note (not built)

EXPERIMENTAL port, design only. What a release on Sepolia for a burn made by `lez-v03-bridge` on the public LEZ
testnet (v0.3.1, `https://testnet.lez.logos.co`) would need, what an honest stand-in would be, what the rival
did, and a recommendation. The v0.4 round trip that ran end to end is `../e2e/runs/crown-live-3/` (a LEZ devnet).

**What the vault accepts.** FerrataVault v5 releases a burn only against the burn root of a *finalised LEZ
checkpoint* (`../vault/README.md`, "LEZ checkpoints"): a Groth16 proof of a checkpoint guest, then a release proof
of the burn's leaf under that root. Rule (A) (`checkpoint`) proves that at least `t ≥ tMin` sequencer keys other
than the tip's producer built on the tip, then waits and echoes a beacon nonce; rule (B) (`checkpoint_b`) proves
that `t` keys of the configuration's key set signed final tips at or after it, each through an attestation key
*certified by a channel key* (`("ferrata/attest-key/v1", channelId, attKey, …)`, `../checkpoint/README.md`). Both
guests rebuild the burn tree from the LEZ blocks themselves (LEZ blocks carry no state root) with a strict parser
of **v0.4.0-rc1 blocks** and the v0.4 bridge's messages (`checkpoint-core/src/lez.rs`, `wire.rs`).

**What is missing on the public testnet.**
1. *A v0.3.1 block reader.* A v0.3.1 block holds plan/apply transactions; this port's burns are effects
   (`RecordBurn`, `AppendBurn`), in the clear for a public burn and as deferred public effects of a private one. A
   new guest would parse v0.3.1 blocks and count them. One v0.3.1 fact makes public burns unsound to count from
   blocks: a reverted public transaction stays in its block unmarked (the rival found the same, below), so the
   guest could count private burns only (a failed private transaction invalidates its block), or the program must
   make its public burns unrevertable after inclusion. The on-chain `burns(cid)` tree of this port does not help a
   proof by itself: no block commits to state.
2. *Signers.* Rule (B) needs the zone's sequencer to run the attestor, i.e. its channel key (Logos's, channel
   `0101…01` by `getChannelId`) to certify an attestation key for this vault and configuration. Rule (A) needs
   several sequencer keys building on the tip; the public testnet has one sequencer, Logos's. Neither rule's premise
   holds there without Logos's participation.
3. *A vault paired with both*: the checkpoint image id of the new guest and the testnet's genesis key set, in a new
   deployment (image ids are immutable in the vault).

**An honestly-labelled stand-in.** A vault in rule (B) whose key set is a key *we* hold, and an attestation key
certified by that key, not by any sequencer: "release authorised by the Ferrata operator's key, certified by
nobody". It would still need the v0.3.1 block reader of item 1 for the burn root, or else it degenerates into a
signature over a burn root we assert, i.e. a one-key bridge. Either way the release's security is one key we hold.

**What the rival did (#274, Portage, from its own text).** Both directions between Sepolia and the public testnet
v0.3.1; the outbound releases "through the committee verifier, whose keys one operator holds today"; its
zero-knowledge release (burn tree rebuilt from blocks, the tip inscribed on Bedrock, a k-of-n committee attesting
the top Bedrock header) ran locally only, on anvil and a Bedrock devnet; on v0.3.1 its tree leaves public burns out
for the reason in item 1.

**Recommendation.** Do not build the stand-in. Its trust is the rival's (one operator's key), it needs a new guest
(item 1) and a new vault, and it would show nothing the crown run did not: the v0.4 round trip already released two
burns on Sepolia from a real `checkpoint_b` proof over real LEZ blocks (with the attestation signed by the run's
driver, a stand-in labelled as such there). Keep the public testnet to the leg that is trustless today, the inbound
one (`testnet.sh`), and run the return leg where its rule can hold: on the v0.4 public network, with
`ferrata_attestor` beside the zone's sequencer (rule (B)), which needs Logos to certify one attestation key per
deployment. If a public-network release is wanted before that, label it exactly as above and count it as a
demonstration of the vault's release path, not of the bridge's trust model. The two 0.001 ETH test deposits the
inbound run escrows stay locked until then: refunds also need a finalised (sunset) checkpoint.
