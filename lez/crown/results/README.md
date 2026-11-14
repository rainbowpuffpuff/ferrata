# The crown run's LEZ side: rehearsals on the build box (11 Nov 2026)

All on the Modal build box (`spike/modal-build/mrun`, our Modal workspace, worktree box ferrata-wt-crown-lez; up to
32 physical cores, 64 GiB), with the guests built at the crown path (attestation program image `0x2228bb46…67cf`,
merged bridge `0x0be48090…25df`), LEZ v0.4.0-rc1's sequencer and Logos Blockchain node (dev-mode leader proofs).
Numbers are measured unless marked derived.

## rehearsal-1: sessions A and B, STAND-IN (anvil fork of Sepolia, beacon committee held by the driver)
LABELLED STAND-IN: the beacon side is a 512-key committee whose keys the driver holds (`crown-driver --chain
stand-in`), finalizing the fork's blocks; the vault, its deposits, storage proofs and block headers are the fork's own,
except that anvil's fork mode writes state root 0 into its blocks, so the stand-in header carries the root of the trie
its `eth_getProof` answers from (`capture-anvil.py`, fixture.json `state_root_patched`).
- Session A (`session.sh a`, ~4 min): new devnet, both programs deployed, stand-in instance Init (Init + 2 adoption
  parts); T0 = block 17; generation 1 = 31,645,210 B.
- Fork (Sepolia block 12,108,564): FerrataVault v5 in mode (B) at 0xfE3199fa…9E54 (`e2e/crown/sepolia.sh deploy`,
  e2e/runs/crown-lez-rehearsal) with session A's configId `0xdd0387ff…206e` (`crown-driver config-id`) and genesisState
  `0x9268f347…9320` (`e2e/crown/identity.py` over T0 and the channel key); deposits j0 (public) and j1 (private),
  a capture after each (6 vault slots).
- Session B (`session.sh b`, ~20 min) after the restore of generation 1 (the chain continued at block 17): two
  stand-in finality updates, then every step applied (`session-b.json`):

| step | block | execution gas |
|---|---|---|
| bridge Init (pairing: configId in the vault's slot 2) | 21 | 218,962 |
| bridge Register ETH (entry, schedules 0 and 1, leaf count) | 22 | 454,499 |
| bridge ImportRoot n=1 | 23 | 300,003 |
| bridge Claim of deposit 0 (public) | 24 | 234,628 |
| bridge ImportFees (ETH schedule 1 absent at n=2) | 28 | 311,588 |
| bridge ImportRoot n=2 | 29 | 300,003 |
| bridge ClaimPrivately of deposit 1 (real proof: 274.1 s on the box's CPU, 252,283 B) | 58 | 539,905 |
| bridge Burn (public), note 0 | 59 | 124,997 |
| bridge Burn (private), note 1 (real proof: 320.7 s, 255,031 B) | 92 | 510,364 |

  T1 = block 92; generation 2 = 278,589,527 B; 132 blocks, all Finalized on Bedrock; 31 transactions, 31 applied.
  `node/`: the node's final blocks for the checkpoint (`session.sh dump` on generation 2: slots 8,865,159..=8,869,616,
  9 windows, 4,858,291 B).
- Re-run of session B on a copy of generation 1 with `CROWN_SAMPLE=1` (rehearsal-1b, 823 s): every step applied
  again, proofs 277.2 s and 325.5 s, T1 = 93; peak summed RSS 10,414 MiB (r0vm 9,381 MiB, node 532 MiB, sequencer
  279 MiB); mrun's CPU accounting 4.67 core-hours over 888 s = 18.9 busy cores on average (derived: about 27 cores
  while proving). The sampler's core figure of that run was wrong (`nproc` = 2 on the box); fixed after it.

## rehearsal-2: session A on the real Sepolia path
`CROWN_CHAIN=sepolia`: the period-1378 bootstrap of `crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10` and
its 32 updates 1378..=1409 (Electra, then Gloas). The driver ran 29.5 min: 176 transactions, 176 applied; instance
`0x6d65e4ef…f6b4` active with 33 jobs, finalized slot 11,542,528; T0 = block 177; 177 blocks, all Finalized;
generation 1 = 408,744,174 B. Before it, `crown-driver check --chain sepolia` ran natively over crown-cloud's REAL
Sepolia capture (vault 0xd0b5…, block 12,108,569, period 1410, the 6 slots): the light client reaches its finality
update, the deposit opens to the run's plan, and the import request (vault account with the header in front, slots 0
and 1) is accepted by the attestation program's own evaluation under that real Gloas record. Session B on the real
path needs captures of a vault paired with a session A's configId: not possible before the owner's run, which did it
([`e2e/runs/crown-live-1/`](../../../e2e/runs/crown-live-1/README.md)).
