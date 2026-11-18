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
  `ff453dcc`) and, for v0.4, in a second build of the bridge program that is not deployed (`lez/guests/bridge-next`,
  image `0xc5cad2cd…d6da`, [EVIDENCE](EVIDENCE.md) row 143: the two keyed accounts, and `Init` refusing a freeze
  authority that is the admin, the red team's F1; v0.4 has no state limit, so it has no oversized-state code); the
  deployed program keeps both issues, and F1;
- rule (B): the watchers' `monitor` reading attestations (the four-sequencer zone has run the bridge program, both
  burns, their releases through a vault on a local anvil and a rotation its committee proposed,
  `checkpoint/attestor/RESULTS.md` §8, §9; `ferrata_attestor` is built as a program and as a Logos module, which ran
  beside each sequencer of a second run of that zone, `modules/RESULTS.md`);
- of the relayer, its root imports on a running LEZ sequencer, and its Logos module's LEZ side through Logos Core's
  own modules (the daemon `ferrata-relayer`, its wrapping, the gas drop, its root imports and its Logos module are
  built; the imports and the module ran on the mini-app's devnet, whose zone the module reaches, and on a fork of
  Sepolia, `modules/results/relayer-2026-11-13/`); the watchers' `monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs (the
  program and its Logos module are built, `monitor/`, `modules/ferrata_monitor_module`; node views come through
  an adapter); the attestation side's `monitor` (records against several providers; the weak-subjectivity alarm);
- in the Logos modules (`modules/README.md`, "Left out"; [`modules/LOGOS-CORE-MODULES.md`](../modules/LOGOS-CORE-MODULES.md)):
  Ethereum transactions through Logos Core's own modules beyond the two daemons on a devnet (the monitor's and the
  relayer's sends go through `tx_sender_module`, signed by `keystore_module` through `signer_manager_module`, with
  `--send-via logos`, on anvil only: 7 of 7 equal to the plain path, EVIDENCE rows 119, 120; that needs our patch of
  `eth_rpc_module` 6965f32 for our builder's code generator, a draft upstream PR, and rests on the keystore's and signer
  manager's `configure` being ungated upstream to name our modules custodians; the bridge module's own Ethereum sends,
  the mini-app's deposit among them, still go through its stand-in chain service, and nothing went to a public network
  through the send stack, [`modules/results/send-stack-2026-11-14/`](../modules/results/send-stack-2026-11-14/README.md)); LEZ through `lez_core` is
  built for the bridge module's wallet session only (its LEZ reads and its private claim and burn, through `lez_core` at
  its merged v0.4 state with a patch of ours, on a devnet: EVIDENCE rows 113, 114), not for the other modules' LEZ
  reads and sends, nor the zone's history, which `lez_core` serves only for a sequencer it runs itself; verified reads
  beyond the attestation module's (`verified_proxy_module`, built from its source, served that module's reads on public
  Sepolia in `required` mode, and the updater module's light-client data came through `beacon_api_module`,
  [`modules/results/verified-proxy-2026-11-14/`](../modules/results/verified-proxy-2026-11-14/README.md); not the bridge
  module's, the relayer's or the monitor's reads; the proxy serves no devnet and no `eth_getProof`, so storage proofs
  come unverified and the programs check them against a record; the private claim's private execution and proof are the LEZ wallet's: the module builds and checks
  `ClaimPrivately`; the release's Groth16 wrap is the relayer's), and a published catalog (laid out,
  dry-run and installed from on 127.0.0.1 in `modules/catalog/`); the A3 gate's three daemons on a running
  sequencer (`ferrata-updater` is built as a binary and as a Logos module that drives the same daemon with either
  backend, `lez/updater/`, `modules/ferrata_updater_module`: the in-process LEZ chain, or a running LEZ sequencer
  through `lez/updater/sequencer/`'s library, which the module ran inside a headless Logos Core on a crown zone
  following Sepolia, EVIDENCE row 104, and the binary for eight hours, row 99; the gate's three daemons run in
  process, row 80);
- the user's round trip on a LEZ sequencer through Logos Core's own modules: on a running LEZ sequencer and
  node (a devnet with the final programs, the vault on a local anvil) the private round trip has run through the
  `ferrata` tool's sequencer path and through the mini-app driving `ferrata_bridge_module` inside a headless
  Logos Core (`e2e/seqnet/RESULTS.md`, EVIDENCE rows 102, 103), but the mini-app's calls (the module's `wallet`
  method) read the chains, sign and send through a stand-in service (`seqnet-chain`: LEZ's wallet library, a
  keystore, the relayer) instead of `eth_rpc_module` and `keystore_module`/`tx_sender_module` (the module's
  `import_root` reads the vault through `eth_rpc_module`, row 109; the `wallet` method does not use it yet; its LEZ
  reads and private sends go through `lez_core` when the session says so, rows 113, 114); the view has run inside
  Logos Basecamp (`b642a9a`) with `ferrata_bridge_module` and `lez_core` loaded there, the whole private round
  trip clicked through Basecamp and checked on the chains (row 125,
  [`modules/ferrata_bridge_ui/results-basecamp-roundtrip/`](../modules/ferrata_bridge_ui/results-basecamp-roundtrip/README.md)),
  also with the private claim and burn proven for real by `lez_core` there (row 126): Basecamp still cuts a view's
  call to a core module at 20 s, so the module runs the two as jobs the view polls (an upstream fix in
  logos-view-module-runtime would make that unnecessary); that devnet's Ethereum side is anvil finalized by a stand-in
  committee, with the vault tests' mock verifier (no Groth16 proof) and a stand-in for the gas drop;
- in the attestation program: consumers bound to the root id (the bridge, `ferrata_holdings` and the
  reference consumer still pin an instance); a chain of several EIP-2935 hops under a Gloas record (one hop
  is attested); the F.8 test (`docs/design/attestation.md` §6.5, §15).
- for Logos Blockchain 0.3.5-rc.1 (the node LEZ v0.4.0-rc3 pins, which Logos's devnet runs): a vault for such a
  zone where it runs (the deployed checkpoint guests, whose ids the vault on Sepolia pins, read the `d480efc6` layout
  and refuse it), a zone of several sequencers on it, and following a Bedrock era change (a deployment pins one fork
  digest). The reading itself is built, as a second build of the two checkpoint guests, and ran end to end over an rc3
  zone of one sequencer and one local validator: the rule (B) checkpoint proven for real, wrapped to Groth16 and
  accepted by vault v5 on a local anvil, both burns released ([`lez/results/v0.4.0-rc3/return-leg/`](../lez/results/v0.4.0-rc3/return-leg/README.md),
  [EVIDENCE](EVIDENCE.md) rows 129, 130).

Changes in [the v3 specification](design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- rule (A) (D13): Δ_g of 3 h when participation is partial (a deployment parameter);
- inbound (D13): the gas drop's sealed credit in the relayer daemon (`ferrata-relayer` sends a public transfer, which
  no private claim can sweep) and for burns; the sealed credit to a private one-time account and its sweep inside
  the private claim ran on LEZ v0.4.0-rc3 with real proofs (EVIDENCE row 124: `ferrata-gasdrop`, crown-driver; the
  gas-drop receipt binding `(N, oneTimeAddress, relayer)` is built and the relayer checks it; refunds drawing on the release cap are in the vault; `ImportRoot` attesting slots 0 and 1, and claims refusing
  `leafIndex >= count`, are in the bridge program).
