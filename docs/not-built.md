# Specified but not built, in full

The README's list in full, moved here unchanged on 16 November 2026 (only the links are rewritten for this folder).
The README keeps a short form of it.

The design notes specify these; there is no code for them here:
- in the bridge program on LEZ (`lez/BRIDGE-RESULTS.md` §9): a transfer that credits a private recipient
  privately, the admin's own handover, the switch to a v0.4 token program when one is released; the nullifier
  account keyed by the configuration id, a holder account per configuration and destination, and an oversized state
  refused with a code: the change that closes the two known issues of 18 November 2026 (a second configuration of
  the same vault can freeze a public-kind deposit whose `s` it knows; a destination's shared holder account can be
  filled; [`bridge.md`](design/bridge.md) §3, [EVIDENCE](EVIDENCE.md) row 136). It changes the bridge program's
  image id, so deploying it is milestone B1's; it is built in the LEZ v0.3.1 port (branch `testnet-v03-bridge` at
  `ff453dcc`) and, for v0.4, in the bridge program since milestone B1 (`lez/guests/bridge`, image `0x847a4336…7515` since ADR 0032, `0xac1fb59b…442e` before,
  [`image-ids.md`](image-ids.md): the two keyed accounts, `Init` refusing a freeze authority that is the admin, the red
  team's F1, and `SetDepth` and `AttestListing`; v0.4 has no state limit, so it has no oversized-state code; its first
  build, `lez/guests/bridge-next`, image `0xc5cad2cd…d6da`, [EVIDENCE](EVIDENCE.md) row 143, is at `f13faee2`), which
  no vault pins yet; the deployed program keeps both issues, and F1;
- rule (B): the watchers' `monitor` reading attestations (the four-sequencer zone has run the bridge program, both
  burns, their releases through a vault on a local anvil and a rotation its committee proposed,
  `checkpoint/attestor/RESULTS.md` §8, §9; `ferrata_attestor` is built as a program and as a Logos module, which ran
  beside each sequencer of a second run of that zone, `modules/RESULTS.md`);
- of the relayer, its Logos module's root imports on a running LEZ sequencer (the standalone daemon's ran on LEZ
  v0.4.0-rc1's sequencer, `e2e/runs/seqnet-robust-1/`), and its Logos module's LEZ side through Logos Core's
  own modules (the daemon `ferrata-relayer`, its wrapping, the gas drop, its root imports and its Logos module are
  built; the imports and the module ran on the mini-app's devnet, whose zone the module reaches, and on a fork of
  Sepolia, `modules/results/relayer-2026-11-13/`); the watchers' `monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs (the
  program and its Logos module are built, `monitor/`, `modules/ferrata_monitor_module`; node views come through
  an adapter); the attestation side's monitor against public providers (built and tested on recorded answers: `monitor/attest/`);
- in the Logos modules (`modules/README.md`, "Left out"; [`modules/LOGOS-CORE-MODULES.md`](../modules/LOGOS-CORE-MODULES.md)):
  Ethereum transactions through Logos Core's own modules beyond the two daemons on a devnet (the monitor's and the
  relayer's sends go through `tx_sender_module`, signed by `keystore_module` through `signer_manager_module`, with
  `--send-via logos`, on anvil only: 7 of 7 equal to the plain path, EVIDENCE rows 119, 120; that needs our patch of
  `eth_rpc_module` 6965f32 for our builder's code generator, a draft upstream PR, and rests on the keystore's and signer
  manager's `configure` being ungated upstream to name our modules custodians; the bridge module's own Ethereum sends,
  user and admin, go through it too since, on anvil and approved at Logos's headless `evm_signer_cli` (EVIDENCE rows
  198, 199), not inside Basecamp with Logos's approval app, and nothing went to a public network through the send stack, [`modules/results/send-stack-2026-11-14/`](../modules/results/send-stack-2026-11-14/README.md)); LEZ through `lez_core` is
  built for the bridge module's wallet session only (its LEZ reads and its private claim and burn, through `lez_core` at
  its merged v0.4 state with a patch of ours, on a devnet: EVIDENCE rows 113, 114), not for the other modules' LEZ
  reads and sends, nor the zone's history, which `lez_core` serves only for a sequencer it runs itself; verified reads
  beyond the attestation module's (`verified_proxy_module`, built from its source, served that module's reads on public
  Sepolia in `required` mode, and the updater module's light-client data came through `beacon_api_module`,
  [`modules/results/verified-proxy-2026-11-14/`](../modules/results/verified-proxy-2026-11-14/README.md); not the bridge
  module's, the relayer's or the monitor's reads; the proxy serves no devnet and no `eth_getProof`, so storage proofs
  come unverified and the programs check them against a record; the private claim's private execution and proof are the LEZ wallet's: the module builds and checks
  `ClaimPrivately`; the release's Groth16 wrap is the relayer's); the A3 gate's three daemons on a running
  sequencer (`ferrata-updater` is built as a binary and as a Logos module that drives the same daemon with either
  backend, `lez/updater/`, `modules/ferrata_updater_module`: the in-process LEZ chain, or a running LEZ sequencer
  through `lez/updater/sequencer/`'s library, which the module ran inside a headless Logos Core on a crown zone
  following Sepolia, EVIDENCE row 104, and the binary for eight hours, row 99; the gate's three daemons run in
  process, row 80);
- the user's round trip on a LEZ sequencer through Logos Core's own modules: on a running LEZ sequencer and
  node (a devnet with the final programs, the vault on a local anvil) the private round trip has run through the
  `ferrata` tool's sequencer path and through the mini-app driving `ferrata_bridge_module` inside a headless
  Logos Core (`e2e/seqnet/RESULTS.md`, EVIDENCE rows 102, 103), and since with the module's Ethereum reads through
  `eth_rpc_module` and its user and admin sends through `tx_sender_module`, driven headless (row 199); a stand-in
  service (`seqnet-chain`, not a module) still gives the session's seed and accounts, the zone's history, the
  release's stand-in prover and the LEZ admin signer (its LEZ reads and private sends go through `lez_core` when the
  session says so, rows 113, 114); the view has run inside
  Logos Basecamp (`b642a9a`) with `ferrata_bridge_module` and `lez_core` loaded there, the whole private round
  trip clicked through Basecamp and checked on the chains (row 125,
  [`modules/ferrata_bridge_ui/results-basecamp-roundtrip/`](../modules/ferrata_bridge_ui/results-basecamp-roundtrip/README.md)),
  also with the private claim and burn proven for real by `lez_core` there (row 126), its Ethereum side through the
  stand-in service there: Basecamp still cuts a view's
  call to a core module at 20 s, so the module runs the two as jobs the view polls (an upstream fix in
  logos-view-module-runtime would make that unnecessary); that devnet's Ethereum side is anvil finalized by a stand-in
  committee, with the vault tests' mock verifier (no Groth16 proof) and a stand-in for the gas drop;
- in the attestation program: consumers bound to the root id (the bridge, `ferrata_holdings` and the
  reference consumer still pin an instance). The F.8 full-diff test (`docs/design/attestation.md` §6.5) is built
  (EVIDENCE row 152). Chains of
  EIP-2935 hops are built (up to 32 in one transaction on real Sepolia data, four on mainnet; EVIDENCE row 145);
  a private consumer's proof carrying hops is measured at the x86-64 minimum's shape (at most 2 hops declared there, ADR 0056; EVIDENCE row 298), not on the Apple M1 or a laptop.
- a cost for forging a light-client record ([`trust.md`](trust.md) §2.2, "What a forger loses", design only, added 18
  November 2026): bonded or restaked backing a consumer can require of a record (a co-signature slashed on Ethereum on
  proof that the header is not canonical), and anchoring to Ethereum's FFG finality instead of, or beside, the sync
  committee; sync-committee slashing itself (EIP-7657) is Stagnant upstream and not Ferrata's to build;
- for Logos Blockchain 0.3.5-rc.1 (the node LEZ v0.4.0-rc3 pins, which Logos's devnet runs): a zone of several
  sequencers on it, Logos's devnet itself, and following a Bedrock era change (a deployment pins one fork digest). The
  reading itself is built, as a second build of the two checkpoint guests, and ran end to end over an rc3 zone of one
  sequencer and one local validator: the rule (B) checkpoint proven for real, wrapped to Groth16 and accepted by vault
  v5 on a local anvil, both burns released ([`lez/results/v0.4.0-rc3/return-leg/`](../lez/results/v0.4.0-rc3/return-leg/README.md),
  [EVIDENCE](EVIDENCE.md) rows 129, 130); a vault for such a zone runs on Sepolia (the earlier vaults' checkpoint ids
  read the `d480efc6` layout and refuse it): the whole round trip with an rc3 zone we ran (rc3's sequencer and node, one
  validator) through a new vault v5 pinning the second build's `checkpoint_b`, rehearsed on a fork and then live on
  18 Nov 2026 as `crown-live-4` ([EVIDENCE](EVIDENCE.md) rows 141, 142, [`e2e/runs/crown-live-4/`](../e2e/runs/crown-live-4/README.md)).

Changes in [the v3 specification](design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- rule (A) (D13): Δ_g of 3 h when participation is partial (a deployment parameter);
- inbound (D13): the gas drop for burns (the relayer daemon's sealed credit and its sweep inside the `ferrata` tool's
  private claim ran on LEZ v0.4.0-rc1's sequencer with dev-mode proofs, EVIDENCE row 205); the sealed credit to a private one-time account and its sweep inside
  the private claim ran on LEZ v0.4.0-rc3 with real proofs (EVIDENCE row 124: `ferrata-gasdrop`, crown-driver; the
  gas-drop receipt binding `(N, oneTimeAddress, relayer)` is built and the relayer checks it; refunds drawing on the release cap are in the vault; `ImportRoot` attesting slots 0 and 1, and claims refusing
  `leafIndex >= count`, are in the bridge program).
