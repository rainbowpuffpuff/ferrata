# ADR 0048: Keep the deployed B1 interface frozen; select B1 or the current claim layout by the zone's program image id

- **Date:** 27 November 2026
- **Status:** accepted; built and run against B1 on a standalone rc3 sequencer; the test network still runs B1
- **Requirements:** RFP-022 F.13, Sec.8, Priv.1, R.10

## Context

This record was written on 27 November 2026 from the commits, after the fact. Commits `05747a6c` and `6bc88550`
took the decision, and `b7bca405` extended its image-id check to the binary used for a private call.

ADR 0046 changed this tree's bridge program and IDL: its claims no longer carry the 24-byte fee-schedule range. The
test network's deployed bridge program is immutable and still runs B1, whose claims require that range. Moving every
reader to the new IDL therefore broke decoders of the deployed program and its committed round trip, while a client
that always emitted the new layout could not claim on that network. A redeploy procedure existed but had not run.

## Options considered

1. Use only the current IDL and make redeploying the test network a prerequisite for every tool. *Reconstructed for contrast*.
   It leaves one layout in the tools, but breaks the deployed interface and readers of B1's records until
   that separate deployment happens.
2. Keep both encodings but choose one by a user setting or by which tool is running. *Reconstructed for contrast*. A
   wrong selection can be signed or sent, and readers, the CLI and the module can silently disagree about the target.
3. Freeze B1's IDL and encoding vectors for deployed-program readers; have every writer read the zone's program image
   id, encode claims for that program, and refuse an image it does not know.

## Decision

The third. `idl/deployed/` holds B1's interface and vectors at their pre-ADR-0046 state. The live-round-trip verifier,
terminal reader, explorer and wallet tests use those frozen artifacts until the test network is actually redeployed.
The current source IDL continues to describe this tree's program.

The CLI, wallet and module read the bridge account's loader header and recognise B1 and this tree's program by image
id. Only `Claim` and `ClaimPrivately` need two encodings: for B1 the client derives and checks the imported schedule
range and inserts it in the message; otherwise it emits the current range-free claim. Burns, imports and account
states keep their common encoding. Plaintext claims of either layout can be decoded from blocks. An unknown image id
is refused before a claim or burn is built, signed or sent. A private claim or burn must also use a guest binary whose
image id equals the zone's, not merely a binary supplied under an expected filename.

## Rationale and trade-offs

The choice respects program immutability while keeping this tree's tools useful against both the deployed program and
its successor. Selection by the zone itself removes a layout flag that could disagree with the destination, and
failing closed prevents an unknown future interface from being misread as one of the two known ones. Frozen vectors
also keep old evidence decodable. A standalone rc3 round trip with the B1 binary ran all 14 steps through this tree's
CLI; that is not a send to the public test network.

The costs are a second claim encoder, explicit trusted image ids and a compatibility path in each client. A later
program requires another recognised id and, if its wire format changes, another encoder. Supporting B1 does not deploy
ADR 0046 or remove B1's disclosure of a fee-schedule range: on B1 the anonymity set remains bounded by that range.
The public network stays on B1 until its separate redeploy procedure is executed.

## Sources

- Commits `05747a6c` and `6bc88550`; idl/deployed/README.md (the frozen interface and its readers)
- `cli/wallet/src/deployed.rs` (image selection, both claim layouts and refusals) and `cli/src/lez.rs` (the loader
  header and private-binary check)
- `modules/ferrata_bridge_module/rust-lib/calls/src/net.rs` (the module's network path)
- docs/VERIFY.md, "Which bridge program the network runs" (the two layouts, status and evidence limits)
- `e2e/runs/rc3-roundtrip-b1/README.md` and docs/EVIDENCE.md row 281 (the standalone B1 round trip)
