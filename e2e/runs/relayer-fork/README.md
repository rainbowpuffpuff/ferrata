# relayer-fork: the relayer daemon on an anvil fork of Sepolia, with real proofs

`cli/tests/relayer_fork.rs`, run by `cli/tests/relayer.sh` on the Modal build box (CPU), 11 Nov 2026. Nothing was
sent to Sepolia: anvil forks it at block 12,111,745 (the block before `crown-live-1`'s `submitCandidate`), reading
public state through `rpc.sepolia.ethpandaops.io` (publicnode and drpc's free plan serve no state that far back).
On the fork the live FerrataVault v5 `0xfE31…9E54` holds its two deposits, RISC Zero's Groth16 verifier sits behind
its emergency stop, and no candidate exists.

Proofs, all real and checked by the deployed verifier on the fork:
- `crown-live-1`'s rule (B) checkpoint (`checkpoint/receipt/`) and its two release seals (relayer = recipient
  `0x79a9…5c07`, relay fee 0);
- `paid/`: a new release of the same burn `public-0`, proven for relayer `0x…c0ffee` with the registered relay fee
  (2 × 10¹² wei): `notes-paid.json` is `crown-live-1/lez/session-b/burn-notes.json`'s note 0 (the test wallet's, the
  holder's side) with `relayer` and `relay_fee` added; `ferrata-e2e crown-release` built the statement, `prove` the
  succinct receipt (77,736 cycles, 14.4 s), `p254` the identity_p254 step (21.1 s), RISC Zero's prover image the
  Groth16 proof (15 s, Modal function on the testerer workspace), `groth16` the seal (`seal.hex`, selector
  `0x73c457ba`), `verify` checked it against the release image `0x17257f91…f5cd` (`journal_equals_native`).

What happened (`results.json`, `test.log`; the same transactions and gas in every run):

| Step | Who | Result |
|---|---|---|
| forged checkpoint (burn root changed, real seal) | a misbehaving relayer | refused by the verifier (`0x439cc0cd`) |
| checkpoint job | the daemon (as `0x79a9…5c07`) | `submitCandidate`, 375,281 gas; a second copy skipped |
| release before the root is final | the daemon | deferred (`BurnRootNotAccepted`), kept in the inbox |
| after W = 3,600 s | the daemon | `finalizeAttested`, 218,385 gas |
| releases with the relayer, the recipient or the fee changed | a misbehaving relayer | refused by the verifier |
| the release at a daemon that is not the named relayer; an unpaid release by default | other daemons | refused before sending |
| the paid release (real proof naming `0x…c0ffee`) | the daemon as `0x…c0ffee` | sent, 407,052 gas, relay fee 2 × 10¹² wei received; the copy naming another relayer refused |
| the other burn (`private-1`); the deferred and recorded proofs of `public-0` | the daemon | `private-1` released (358,908 gas); `public-0`'s other proofs refused: already released |
| fees | the daemon | the checkpoint fees of both releases, 2 × 10¹² wei, withdrawn with `claimOwed` |
| replay | a misbehaving relayer; the daemon | refused (`0xcad2ae02`); refused before sending |
