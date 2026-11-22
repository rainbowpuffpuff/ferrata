# ferrata_bridge_module through Logos Core's send stack, after the second review's fixes (22 Nov 2026)

> Superseded by [`../bridge-send-stack-r4/`](../bridge-send-stack-r4/README.md) (the third review's fixes: the deployment pin,
> 55 checks); kept as recorded.

Supersedes `../bridge-send-stack-r2/`. Code: branch `rfp-logos-core-eth` at `bcfd983a` (the module as of `fc422f09`).
Module `.lgx` sha256 `be17842b…`, built with `LOGOS_TMPDIR=/var/tmp/bss` (`logos-core/lgx-sha256.txt`: the module's
`.so` embeds Nix's build directory, so another directory gives another hash for the same source); the seven send-stack
`.lgx` are byte-identical to r2's. Measured on the Modal build box, from the worktree's root, as run:

    bash modules/scripts/bridge-send-stack.sh build <out>          # the send stack's seven .lgx, the module's .lgx, logosctl
                                                                   # (here `build-module <out>`, <out>/stack from an earlier `build`)
    MODULES=ferrata_bridge_module bash modules/scripts/native.sh   # the module's unit tests: 37 passed
    bash modules/scripts/bridge-eth-native.sh <out>/native          # native/: 27 of 27
    bash modules/scripts/bridge-send-stack.sh run <out>             # five runs in a row: 53 of 53 each (runs.txt)

`logos-core/` is run 1: `checks.tsv`, every module call (`calls.jsonl`), the approver's prompts, each send's request,
reply and timing (`send-ms.tsv`), anvil's log, `summary.txt` (with the count of approver recoveries, below).

What the 53 checks add to r2's 49 (step 9 of the script): with anvil's automine off and the request's own receipt wait
of 8 s, an approved ETH deposit is broadcast and not mined; the module answers `verdict: unconfirmed, pending: true,
settled: false` and "broadcast, not yet mined: transaction 0x45155a6b… was sent after your approval and no receipt was
seen within 8 s. Do not send this deposit again: it may still be mined" (never "failed"); `eth_getTransactionReceipt`
is null; after the next block the receipt has status 1. Unchanged from r2 and passing in all five runs: the margin over
the stack's estimate (the cap: estimate 48,564 in runs 1 and 3-5, 51,089 in run 2; limit 80,705 / 83,861; used 50,771
each run), the forced revert (the admin's own limit 30,000, 29,999 used, reported `reverted`, the cap unchanged), the
delist/list, fee schedule, release delay, ERC-20 and ETH deposits (ETH: estimate 200,572, limit 270,715, used 200,572),
the refusal at the approver, the freeze and lift. The review's gas cross-check (`gascheck.py`, the limit each approver
prompt showed against the module's record) found the signed limit equal to the recorded one for all 12 sends of every
run (box log, not kept here).

**Approver recoveries: 0 in all five runs.** The second review's rerun of r2's script lost one approval (its run 1: the
re-listing `setDepositsEnabled(true)` was prompted at `evm_signer_cli` at 22:51:24Z, then gone from the approver's queue
while `tx_sender_module` still answered `awaitingApproval`; the module withdrew it after 600 s; 43 of 49). The cause is
upstream and was not identified from the code (`evm_signer_cli` 2cb46d8, `signer_manager_module` bd5c84d; no daemon log
was kept). The script now waits as a person at the approver would: nothing on screen after 5 s, it reads the manager's
queue (`evm_signer_cli list`) and `tx_sender_module`'s live sends, shows the request by handle (`evm_signer_cli show`)
and approves it, recording each recovery with what each side said (`recoveries.jsonl`) and counting it in
`summary.txt`. No request was dropped in these runs, so that path has not been exercised live; a drop would now be
recovered or, if the manager no longer holds the request, diagnosed.
