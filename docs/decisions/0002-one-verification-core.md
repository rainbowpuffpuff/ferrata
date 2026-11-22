# ADR 0002: One `no_std` verification core, shared by the program, private consumers, the tools and the modules; reuse audited cryptography, transcribe the light client

- **Date:** 10 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 R.1, R.6, U.2, U.5

## Context

The same verdict must come from the program, from a private consumer's own proof, from the CLI's dry run and from the
Logos module, with the same typed codes (U.2). RFP-021 R.6 asks to reuse mature, audited implementations and to say
what is rewritten.

## Options considered

1. Adopt Helios `consensus-core` as code. *Recorded:* docs/design/attestation.md §14 (Helios "not adopted as code;
   kept as a test oracle").
2. Separate implementations for the program and the tools. *Reconstructed for contrast* (not recorded as an option at
   the time); the reason against it is docs/design/attestation.md §5's: "One crate everywhere means verdicts and error
   codes cannot drift."
3. One core (`ferrata-core`, `no_std`) with `attest::evaluate` in the wire crate, called by every path; `blst` (NCC
   Group audit, 2021) for BLS in RISC Zero's accelerated fork; RISC Zero's own components; an own, strict
   transcription of the light-client specification, with Helios and alloy kept as differential oracles.

## Decision

The third.

## Rationale and trade-offs

Verdicts and codes cannot differ between paths, which is what makes dry runs meaningful. Helios needs `std`, could not
decode Altair-layout data and accepts two inputs the specification refuses (filed upstream); `blst` was 2.45 times
cheaper per finality update than accelerated zkcrypto. The cost: the transcription, SSZ, Merkle-Patricia and RLP code
are new and in the audit's scope.

## Sources

- docs/design/attestation.md §5, §14
- lez/RESULTS.md §5, "The interface"
- crates/ferrata-core/README.md
