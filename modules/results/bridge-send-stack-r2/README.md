# ferrata_bridge_module through Logos Core's send stack, after the review's fixes (21 Nov 2026)

Supersedes `../bridge-send-stack-2026-11-21/` (its 37/37 did not reproduce: an approved `setTokenCap` reverted for want
of gas and the module reported reverts as done). Code: branch `rfp-logos-core-eth` at `787b5448`..`3ace689f` (H1 send
path, M1, registry), module `.lgx` sha256 `6b638097…` (`logos-core/lgx-sha256.txt`; the seven send-stack `.lgx` are
byte-identical to the 2026-11-21 build's). Measured on the Modal build box, from the worktree's root, as run:

    MODULES=ferrata_bridge_module PORTABLE=0 E2E=0 LOGOS_TMPDIR=/var/tmp/bssf RISC0_DEV_MODE=1 bash modules/scripts/check.sh <out>/check
    bash modules/scripts/bridge-eth-native.sh <scratch>/eth-native          # native/: 27 of 27
    bash modules/scripts/bridge-send-stack.sh run <out>                     # three times: 49, 49, 49 of 49 (runs.txt)

(`<out>/stack` holds the send stack's `.lgx` from `bridge-send-stack.sh build`.) `logos-core/` is run 1 (49/49):
`checks.tsv`, every module call (`calls.jsonl`), the approver's prompts, each send's request, reply and timing, anvil's log.

What the 49 checks add to the 37: every send carries the stack's own estimate (`tx_sender_module.prepare`) + 25% + 20,000
and uses less (the cap: estimate 51,089 / 51,089 / 48,564, limit 83,861 / 83,861 / 80,705, used 50,771 each run; in
run 3 the bare estimate would have reverted, as in the review); a send forced to revert after its approval (the
caller's own limit 30,000; 29,999 used) is reported `reverted` naming the call, block and gas, its receipt status 0,
the cap unchanged; `setDepositsEnabled` (delist, a deposit refused before signing while delisted, list again),
`announceFees` (a schedule pending) and `setReleaseDelay` (600 s for the next checkpoints), each read back with cast;
every other receipt successful with gas used below its limit. Reads are route `direct`: on anvil the verified proxy of
`eth_rpc_module` cannot run (the RFP's "verified proxy features" are not exercised here).
