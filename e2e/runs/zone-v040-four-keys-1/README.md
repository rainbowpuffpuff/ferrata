# A zone of four LEZ v0.4.0 sequencers on a Logos Blockchain 0.4.0-rc.2 node; rule (B)'s rc3-era attestor refuses its LEZ blocks

**Run:** from `checkpoint/`, `ZONE_LEZ=../../upstream/lez-v0.4.0 attestor/zone.sh <bundle> OUT 1080` on a Modal build box,
28 November 2026, 02:11–02:30 UTC ([`zone.log`](zone.log)); then `ZONE_LEZ=../../upstream/lez-v0.4.0
attestor/zone-attest.sh OUT OUT/attest` ([`attest-run.txt`](attest-run.txt)). The bundle is
`checkpoint/attestor/zone-bundle.sh v0.4.0`'s (as `zone-v040-standalone-1/`'s): LEZ v0.4.0's `sequencer_service`
(`a255b42c`, unmodified), the Logos Blockchain node 0.4.0-rc.2 that LEZ v0.4.0 pins (built from source by LEZ's own
resolver: no release asset matched), r0vm 3.1.0. The zone is upstream's own four-sequencer devnet at v0.4.0
(`lez/configs/docker-all-in-one/devnet`: one configuration whose genesis stakes four channel signing keys, and the
four keys, regenerated at v0.4.0), run without Docker on loopback; the node runs LEZ v0.4.0's `bedrock/` settings:
[`deployment-settings.yaml`](deployment-settings.yaml) in the **V2 era schema** (LEZ PR 1154), one validator with
dev-mode leader proofs as LEZ's container runs it. The sequencers' genesis has `finality_depth` 35 and no
`exit_delay` (LEZ PR 1026; checked by the script). Nothing was sent to any public network.

**Result: the zone works on v0.4.0.** The channel accredits the four keys from its creation (configuration threshold
3, posting timeframe 60 slots, posting timeout 25: [`answers/channel.json`](answers/channel.json)); the turn rotates in
the accredited order and **every key inscribes**: 107 channel inscriptions in the node's 270 final blocks, seq-0 38
(two turns), seq-3 23, seq-2 23, seq-1 23, in five consecutive turns ([`inscriptions.json`](inscriptions.json):
`python3 -I checkpoint/attestor/zone_inscriptions.py <this directory>`). At each of the 17 polls with four heights
that `zone.log` kept (02:12:31, then 02:18:57–02:24:00; see below) the sequencers' heights were within one block of each
other (70/70/70/70 at the last kept poll). After the sequencers stopped, the node's LIB was at the channel's tip: `lib_slot` 2584999 in
[`answers/info.json`](answers/info.json) and `tip_slot` 2584999 in [`answers/channel.json`](answers/channel.json); the
node's answers from the first slot to its LIB are in [`answers/`](answers/) (`/cryptarchia/info`, `/channel/<id>`,
`/cryptarchia/blocks`: 518 KB). The rc3 run of the same zone (`../zone-rc3-four-keys/`) ended in a stall of the channel's
tip; this one did not over its 18 minutes (seq-0 published block 107 at 02:30:22, three seconds before the four shut
down: [`logs/seq-0.log`](logs/seq-0.log)), which does not show that the rc3 stall cannot happen on v0.4.0.

**`zone.log` has two gaps:** it holds no lines between 02:12:31 and 02:18:57, and none after the 02:24:00 poll, six
minutes before the run ended (all four sequencers shut down at 02:30:25, as their logs show). The box tool's syncs brought
back early copies of the file while the run was still writing it, and those copies were written back over the box's
file. So the end state above comes from the node's answers and the sequencers' logs, not from `zone.log`. The files in
[`logs/`](logs/) are the last 300 lines of each process's log.

**Its paths:** `zone.sh` does not scrub its records. The box's runner scrubbed this directory afterwards with
`scripts/scrub_records.py` (after `zone.sh`, and again after `zone-attest.sh`), and none of its files names the machine.

**Rule (B) over these blocks: refused by this tree's rc3-era attestor.** `ferrata_attestor tier --answers answers`
stops at the first LEZ block in the channel: `block 2: parser refused it: NotStrictlyOrdered { what: "actors" }`
([`attest-run.txt`](attest-run.txt)). The Bedrock layer was read (0.4.0-rc.2's `/cryptarchia/blocks` JSON, its mantle
operations and inscription payloads); the LEZ block inside the inscription was not: v0.4.0 encodes a transaction's
actors as a map from actor to authority (LEZ PR 1144) where rc3's decoder (`checkpoint/crates/checkpoint-core`'s LEZ
layout, which `checkpoint_b_0_3_5_31` and the attestor share) reads its own order. So neither the attestors' signatures
nor a `checkpoint_b` execution over a v0.4.0 zone exists yet: the rule (B) guest and the attestor need v0.4.0's LEZ
transaction and block layout (receipts, LEZ PR 1107; the fee on the message, PR 1080; keyed witnesses, PR 1135) as a
second build beside the 0.3.5 one. These answers are the test vector for that port.

**Not in this run:** the bridge program (its run on a zone set up the same way, which got only part of the way round:
`../zone-bridge-v040/`); the committee's parameter vote (`../zone-v040-vote-1/`, `../zone-v040-vote-2/`: the devnet's
founding stakes' ownership secrets are random and discarded by upstream's generator, so these four keys cannot vote; the
vote runs stake keys from their own wallet); an unstake under L1 finality (`../zone-v040-vote-2/`; vote-1's was refused).
