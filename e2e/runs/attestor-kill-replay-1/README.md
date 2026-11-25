# attestor-kill-replay-1: the four zone attestors SIGKILLed in their rounds and started again (RFP-022 R.6)

`checkpoint/attestor/kill-replay.sh` on the Modal build box (branch recovery-022, `ferrata_attestor` of this tree,
r0vm 3.1.0 for the executor), over the recorded four-sequencer LEZ v0.4.0-rc1 zone (`checkpoint/attestor/runs/zone`,
upstream's devnet keys; its LEZ answers stood in by `devnet-lez-standin`, as `zone-attest.sh` replays it). Per
attestor 12 rounds killed after 1-12 ms (`timeout -s KILL`; a whole round takes 10-11 ms), then one to its end
([rounds.tsv](rounds.tsv): attestor, round, ms, exit code, state before/after, attestation written, state whole).

Measured ([verdict.json](verdict.json)): 31 of the 48 killed rounds died mid-run (exit 137; 8, 7, 9, 7 per attestor),
2 of them after the attestation was written; 0 torn state files; each attestor's every attestation, killed run or
not, carries one signed message (`encoded`), equal to the zone run's own record; the collector's 3 of 4 is
byte-identical to the proven `rbz-zone-n4-t3.input`, and checkpoint_b's executor accepts it
([exec.json](exec.json): journal equal to the native one, 6,261,183 user cycles). A replay: no endpoint fails here;
the live attestor under kills and endpoint outages is the robust run's (`e2e/runs/seqnet-robust-6/`).
