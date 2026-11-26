# zone-bridge-rc3-multi: the return leg over four LEZ v0.4.0-rc3 sequencers, three of four keys (RFP-022 F.7)

26 November 2026, the Modal build box: the bridge on a zone of four LEZ v0.4.0-rc3 sequencers, its rule (B)
checkpoint signed by three of the four sequencers' attestors, proven under RISC Zero 3.1, and both burns released
through RISC Zero's Groth16 verifier 3.1.0. `checkpoint/attestor/zone-bridge.sh` with `ZB_LEZ` = LEZ v0.4.0-rc3 (tag
`v0.4.0-rc3`, `bafd07be`), `ZB_CHECKPOINT_A=checkpoint_0_3_5 ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=4 ZB_T_MIN=3`,
the RISC Zero 3.1 programs of [`docs/image-ids.md`](../../../docs/image-ids.md) (`checkpoint_b_0_3_5_31`
`0x8917f58e…`, `release_31` `0xf279af41…`).

**What ran, real.** Upstream's four-sequencer devnet configuration of LEZ v0.4.0-rc3, without Docker, on one Logos
Blockchain 0.3.5-rc.1 node (one validator, leader proofs in dev mode): the channel accredits four keys with threshold 3
(posting timeframe 60 slots; `zone/channel-end.json`), the turn rotates, all four inscribe (46, 46, 38 and 35 of the
165 final inscriptions of LEZ blocks 2..=166, each checked by its channel key: `checkpoint/realblocks-crown.txt`).
crown-driver's sessions A and B (`lez/`): the bridge program `ferrata_bridge_b1` deployed, two deposits imported, a
public and a private claim (the private one with LEZ's real proof), a public and a private burn. Beside each sequencer a
`ferrata_attestor` (`zone/attestor-{0..3}/`: certificate by its channel key, every round, its resources). The checkpoint
`zone-28-164` (136 blocks, 287 transactions; two burns, two imported leaves) takes three of the four attestations of
tip 164 (`checkpoint/attestor.json`: n = 4, t = 3; the last attestor's, key `90a548e0…`, left out); RISC Zero 3.1's executor
matches the native journal (22,149,862 user cycles: `checkpoint/exec.json`). Refused by the rule natively and by the
deployed guest in the executor, each with the same error (`checkpoint/refusals/`): two of four attestations
(`InsufficientAttestations { have: 2, need: 3 }`), an attestation by a channel key outside the set
(`NotAccredited`), one attestor twice (`DuplicateAttestor`) and an earlier round of an accredited attestor
(`NotADescendant { tip_id: 29 }`).

**Proofs on Modal's GPU, through the bridge's prover client** (`deploy/testnet/host/bin/ferrata-bridge-prover`,
unchanged; `prover/`): the checkpoint and both releases on one L4, each wrapped to a 260-byte Groth16 seal with RISC
Zero 3.1's selector `0xe71e6620`, each journal the statement's. The client was killed (SIGKILL) while it waited for the
checkpoint's proof and started again: it resumed the same Modal call (same call id, one attempt) and wrote each job
once (`prover/calls.tsv`, `prover/summary.txt`: PROVER KILL-RESUME OK).

**The Ethereum side** (`vault-checks.json`, `manifest.json`, `logs/zone-bridge.log`): FerrataVault v5 in mode (B)
with floors nMin 4, tMin 3, its genesis the zone's state over its four keys at T0, behind RISC Zero's verifier 3.1.0
and emergency stop, on a local anvil with Sepolia's chain id. The checkpoint submitted and, after W, final (the
vault's final state is the journal's new state, its burn root accepted); both releases paid to the recipient their
burns name, 999,000,000,000,000 wei each (0.001 ETH less the 0.1 % checkpoint fee), each nullifier then used; each
release's and the checkpoint's mined calldata called again: refused (`NullifierAlreadyUsed` twice, `ParentNotFinal`).
The same seals in forge, `vault/test/real/ZoneBridgeRc3MultiTest` (7 of 7): submitCandidate 343,581 gas,
finalizeAttested 181,337, the releases 377,860 and 273,760.

**On a fork of Sepolia** (`fork-rehearsal-31/transcript.txt`; `vault/script/fork-rehearsal-31.sh` with
`FERRATA_REHEARSAL_RUN` and `FERRATA_REHEARSAL_FIXTURE` naming this run): anvil forked Sepolia at block 12,216,620;
the run's deployer has nonce 0 there, so the vault, with its own verifier 3.1.0 behind RISC Zero's emergency stop
(owner renounced), lands at the address the run's configuration and proofs bind; the binding checked; the run's two
deposits; the three-of-four checkpoint submitted with its 3.1 seal (375,245 gas) and final after W (218,361); a release
refused while releases are frozen; both releases paid with their seals (421,342 and 384,654 gas), a replay refused;
then decision D1 (a) against a forgery (the verifier's code replaced on the fork by one that accepts every seal): a
watcher's pause refuses a forged checkpoint and a freeze a forged release; lifted, the forgery is paid. FORK REHEARSAL
31 OK. Nothing was sent to Sepolia: anvil reads its state and keeps every transaction to itself.

`vault-attempt-1/`: the vault stage's first attempt, kept. Its sends were the same transactions (the same hashes), but
its replay check went through `e2e/crown/sepolia.sh`, which skips a stage its journal already holds, so the "replay"
sent nothing and exited 0, and the check stopped with "release 0 sent again was paid" (`logs/zone-bridge.log`): a
false alarm of the check, not a second payment. The stage now calls the mined calldata again (`cast call`) and expects
the vault's revert, and a rerun starts again from the zone stage's saved chain with the earlier attempt's records set
aside here.

**Stand-ins**, as in every `zone-bridge.sh` run: the beacon chain on LEZ (a committee crown-driver holds finalizes the
anvil's blocks), the Ethereum side (anvil; the vault's wait W and the release delay warped), leader proofs in dev
mode. Validity rests on t of n accredited sequencer keys (rule (B)).

Layout: `identity.json`, `params.json`, `crown.json`, `deploy.json`, `manifest.json`, `vault-checks.json` (the
Ethereum side), `vault-attempt-1/`; `lez/` (crown-driver's sessions, the captures); `zone/` (the fork digest, the channel, each attestor's
certificate, rounds and resources, the node's answers `node-answers.tar.zst`); `checkpoint/` (the input and meta, the
blocks and inscriptions checked, `burns.json`, the collector's and executor's records, the refusals, `receipts/`);
`releases/`; `prover/`; `fork-rehearsal-31/`; `net/`; `logs/`.

Re-check: `ZB_PROGRAMS=<the R0BF programs of docs/image-ids.md, RISC Zero 3.1> ZB_CHECKPOINT_A=checkpoint_0_3_5
ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=4 ZB_T_MIN=3 bash checkpoint/attestor/zone-bridge.sh verify
e2e/runs/zone-bridge-rc3-multi`; `cd vault && forge test --match-contract ZoneBridgeRc3MultiTest` (fixture
`test/real/fixtures/zone-bridge-rc3-multi.json`); `python3 -I checkpoint/attestor/resources.py
e2e/runs/zone-bridge-rc3-multi` (the attestors' resources, RFP-022 P.6).
