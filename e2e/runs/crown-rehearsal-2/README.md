# crown-rehearsal-2: the crown run on a fork again, stopped on purpose during LEZ session B and resumed (11 Nov 2026)

`e2e/crown.sh start --fork crown-rehearsal-2` on ws-3 from branch `crown-rehearsal` (crown-cloud + crown-lez
3f99f26 + crown-proofs 4f2f0be), the same STAND-IN set-up as crown-rehearsal-1 (anvil fork of Sepolia block
12,108,942, stand-in beacon committee on LEZ, W and the release delay passed with evm_increaseTime; everything else real).
Started 02:59:46Z, DONE 03:33:34Z; `verify.txt`: 14 of 14 checks passed. Records here: stages.done, run.json,
identity.json, verify.txt (the full run directory stays on the Volume ferrata-crown, runs/crown-rehearsal-2/).

What it showed, from crown.log:
- `e2e/crown.sh stop` at 03:09:53Z, 5 min 47 s into session B (during the private claim's proof). The stop cancelled
  the runner and, with it, the LEZ session it had called (Modal cancels the call too): no generation 2 was saved.
- `start` again at 03:10:59Z resumed at lez-b: lez_session found generation 1 and ran B again (900 s, T1 = 94), then the
  checkpoint (input 602,328 B, parent = genesisState; an L40S: 9.0 s, 5 segments; wrap 54 s).
- submit then stopped (03:27:27Z): anvil asked its upstream (the Secret's endpoint) for a storage slot at the fork block,
  by then ~28 min old, and the upstream had pruned that state ("failed to get storage", -32603). Fix: the fork's upstream
  is publicnode, which served state 2,000 blocks old (checked from the laptop, read-only), with anvil's retries at 10
  (drive.py). `start` again at 03:29:05Z: submit, the releases (CPU 27 s for private-1; wraps 15-16 s), W, finalize,
  both releases and verify, DONE.

Stage times (stages.done; the second start's): lez-a 198 s, lez-b 901 s, checkpoint-prove 15 s (GPU warm), wrap 54 s,
releases-prove 172 s, the Sepolia stages 1-5 s each on the fork.
