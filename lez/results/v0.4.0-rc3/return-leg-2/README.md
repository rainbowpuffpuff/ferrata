# The return leg on LEZ v0.4.0-rc3 again: the attestor checks every tip against the sequencer's own LEZ node

The run of [`../return-leg/`](../return-leg/README.md) (its stage 2 and 3: a rule (B) checkpoint over the real blocks
of an rc3 zone, proven, wrapped and paid out by FerrataVault v5 on a local anvil) made again on 22 November 2026 on a
Modal build box, at the branch `fix-attestor`: the attestor beside the sequencer now asks the sequencer's JSON-RPC
`getBlock` and signs a tip only when it is served `Finalized` and byte for byte as inscribed (review H-2), and the hook
passes that RPC (review M2). The same builds as there: rc3's `sequencer_service` (sha256 `be2ac354…`, built from
`bafd07be`), the 0.3.5-rc.1 node (release asset of `f6d4267f`, sha256 `2f301729…`), the final LEZ programs, the
second build's `checkpoint_b_0_3_5` (`0xafbf4023…`, rebuilt here: the same id). Records: [`zone/`](zone/environment.md)
(the zone's), [`e2e/runs/zone-bridge-rc3-2/`](../../../../e2e/runs/zone-bridge-rc3-2/README.md) (the run's). Results and
numbers: [`checkpoint/attestor/RESULTS.md`](../../../../checkpoint/attestor/RESULTS.md) §10.1.
