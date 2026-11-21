# A zone of four LEZ v0.4.0-rc3 sequencers (RFP-022 F.7: validity with more than one sequencer key)

**Run:** from `checkpoint/`, `ZONE_BEDROCK=bundle bash attestor/zone.sh <crown-rc3 bundle> OUT 1080` on a Modal build
box, 21 November 2026, 17:49–18:07 UTC (`zone.log`). The bundle is crown-rc3's (`spike/tmp/crown-rc3/lez/bundle.tar.zst`
beside the repository: LEZ v0.4.0-rc3's `sequencer_service`, its Logos Blockchain node 0.3.5-rc.1, r0vm, and its
`lez-config/` with rc3's `deployment-settings.yaml` and `node-config.yaml`); the zone is upstream's own four-sequencer
devnet (`lez/configs/docker-all-in-one/devnet`: one configuration whose genesis stakes four channel signing keys, and
the four keys; byte-identical in rc1 and rc3), run without Docker, one validator with dev-mode leader proofs as LEZ's
container runs it. Nothing was sent to any public network.

**Result:** the channel accredits the four keys (configuration threshold 3, posting timeframe 60 slots: 240 s at
rc3's 4 s slots), the turn rotates in the accredited order, and **every key inscribes**: 93 channel inscriptions in
the node's final blocks, seq-0 24, seq-2 23, seq-3 23, seq-1 23, in four consecutive turns
([`inscriptions.json`](inscriptions.json): every inscription's slot and signer, from the node's blocks). The four
sequencers' heights were equal at every poll through the rotation.

**Seen at the end:** in the last ~2 minutes, after seq-1's turn, the channel's tip did not advance (tip slot 2447817)
while seq-0's own height went on rising (101 → 105, the others at 93–94): seq-0's blocks of its second turn were not
inscribed before the run stopped. The cause was not determined (the logs keep their last 300 lines); rc3's known
stalls (LEZ issue 1109: storage gas price step at an epoch, then a rewind) match the pattern, and our single-sequencer
rc3 runs met them too ([`lez/results/v0.4.0-rc3/gas-drop/README.md`](../../../lez/results/v0.4.0-rc3/gas-drop/README.md)). A run that relies on the zone must ride through it, as the crown
drivers do.

**What this does and does not show for F.7:** LEZ rc3 allows a multi-key zone with rotating turns, so the t-of-n
attestation of rows 65 and 100–101 (done on rc1) has a zone to run on with the current release candidate. Not run
here: the bridge program on this zone, a `checkpoint_b_0_3_5` proof over its blocks with an attestor beside each
sequencer (3 of 4), and the releases through RISC Zero's verifier.
