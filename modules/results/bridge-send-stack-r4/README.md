# ferrata_bridge_module through Logos Core's send stack, after the third review's fixes (22 Nov 2026)

Supersedes `../bridge-send-stack-r3/`. Code: branch `rfp-logos-core-eth` at `6be33328` (the module as of `80651bff`).
Module `.lgx` sha256 `65bbcf8a…`, built with `LOGOS_TMPDIR=/var/tmp/bss` (`logos-core/lgx-sha256.txt`; the module's
`.so` embeds Nix's build directory); the seven send-stack `.lgx` are byte-identical to r3's. Measured on the Modal build
box, from the worktree's root, as run:

    bash modules/scripts/bridge-send-stack.sh build-module <out>    # the module's .lgx and logosctl (<out>/stack from an earlier `build`)
    MODULES=ferrata_bridge_module bash modules/scripts/native.sh   # the module's unit tests: 40 passed
    bash modules/scripts/bridge-eth-native.sh <out>/native          # native/: 27 of 27
    bash modules/scripts/bridge-send-stack.sh run <out>             # three runs in a row: 55 of 55 each (runs.txt)

`logos-core/` is run 1: `checks.tsv`, every module call (`calls.jsonl`), the approver's prompts, each send's request,
reply and timing (`send-ms.tsv`), anvil's log, `summary.txt`.

What the 55 checks add to r3's 53 (step 1b of the script): Logos Core is started with `FERRATA_DEPLOYMENTS` naming this
run's vault on chain 11155111, the one deployment the module may send to (the third review's carried L1: the caller
chose the vault). A deposit request naming another address as its vault (the test token's) is refused before anything
is read from that address or asked of the signer: "refused before anything was asked: vault 0x59f2f1fc… on chain
11155111 is not a deployment this module may send to (FERRATA_DEPLOYMENTS: 11155111:0x712516e6…)", and
`tx_sender_module` holds no send afterwards (`deposit-other-vault.json`). Every purpose line the approver showed now
names the deployment, for example "Ferrata bridge admin (admin): set the deposit cap of 0x59f2f1fc… (vault 0x712516e6…
on chain 11155111) [asked by ferrata_bridge_module]" (`prompts.jsonl`).

Unchanged from r3 and passing in all three runs: the margin over the stack's estimate (the cap: estimate 51,089 in runs
1 and 3, 48,564 in run 2; limit 83,861 / 80,705; used 50,771 each run), the forced revert (the admin's own limit 30,000,
29,999 used, reported `reverted`, the cap unchanged), the unmined deposit (automine off, the request's 8 s receipt wait:
`unconfirmed`, no receipt, then status 1 after the next block), the delist/list, fee schedule, release delay, ERC-20 and
ETH deposits (ETH: estimate 200,572, limit 270,715, used 200,572), the refusal at the approver, the freeze and lift. The
gas cross-check (the third review's `gascheck.py`: the limit each approver prompt showed against the module's record)
found the signed limit equal to the recorded one for all 12 sends of every run (box log). Approver recoveries: 0 in all
three runs (r3: 0 in five; the recovery path is still not exercised live). None of the run's test passwords appears in
these files.
