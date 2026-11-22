# seqnet logoseth-4: the mini-app's round trip and admin, Ethereum through Logos Core's own modules (22 Nov 2026)

> Superseded by [`../seqnet-logoseth-5/`](../seqnet-logoseth-5/README.md) (the third review's fixes). Its "293 bridge states,
> all equal" counted 123 reads that `lez_core` answered nothing for (an absent account, or a failed read) as equal; logoseth-5
> counts them apart.

As `../seqnet-logoseth-3/` (a LEZ v0.4.0-rc1 zone we ran, anvil with chain id 11155111, the mock verifier, dev-mode
private proofs), with the second review's fixes: branch `rfp-logos-core-eth` at `bcfd983a`, the module's `.lgx` of
`modules/results/bridge-send-stack-r3/` (sha256 `be17842b…`, `miniapp/lgx-sha256.txt`). On the Modal build box, from the
worktree's root, as run (the crown-live-3 inputs in `$SEQNET_TMP/inputs`, as `run.sh`'s header lists them):

    export SEQNET_TMP=<scratch>/seqnet SEQNET_SHORT=/var/tmp/rfx
    bash e2e/seqnet/run.sh build-logoseth                                  # once: the tools, logosctl, the send stack, lez_core (patched), the runner
    D="$SEQNET_TMP/logos/lgx-ferrata_bridge_module"                        # build-logoseth's own module .lgx (it differs: its build directory),
    chmod -R u+w "$D"; rm -rf "$D"                                         # replaced as run.sh does: the source is a /nix/store symlink, so
    cp -rL <out>/check/lgx-ferrata_bridge_module "$D"                      # cp -rL; the r3 build's module .lgx
    bash e2e/seqnet/run.sh logoseth logoseth-4                             # rebuilds seqnet-chain first

(The copy step was written `cp -r <out>/check/... "$SEQNET_TMP/logos/"` until the third review found that it fails, "cannot
overwrite directory with non-directory", and would leave build-logoseth's own `.lgx` in place; the run itself copied it
as above (`rm -rf`, then `cp -rL`), and `miniapp/lgx-sha256.txt` shows the r3 hash.)

Run 529 s; Qt Quick Test 12 passed of 12 (10 test functions plus `initTestCase`/`cleanupTestCase`; `miniapp/qmltest.txt`,
238.5 s).

New in this run:
- **No LEZ role message without a person's approval.** The admin tab's `SetCap` (the module's message at the admin's own
  actor) was held by the role's wallet (its stand-in, `seqnet-chain` holding the run's role keys) as `lra_1`, shown in
  the view's header as waiting there (screenshot 26b), approved by the person with the role password for exactly that
  message (SHA-256 `0x0157e912…`, `approver/lez-setcap.*`), then signed, sent and accepted in LEZ block 40 and read back
  from `token(cid, ETH)`. A `Sunset` requested through the module was held as `lra_2` and rejected by the person
  (`approver/lez-sunset.*`): the module's job failed "rejected at the role wallet (lra_2): the person declined; nothing
  was signed or sent", no Sunset is among the zone's transactions (`lez/transactions.tsv`) and the configuration reads
  `sunset: false`. The same `SetCap` signed by the user's own public account (not a role key: not held) was refused by
  the bridge program (reverted, LEZ block 41).
- **The registry's LEZ side through `lez_core`.** Every registry entry was read again through Logos Core's `lez_core`
  and compared: `REGISTRY LEZ READS` 3 of 3 equal for each token. Over the run `lez_core` read 293 bridge states, all
  equal to the chain service's (`miniapp/lez-report.json`; logoseth-3: 154): 67 `config(cid)`, 50 `token(cid, ETH)`,
  17 `token(cid, <the second token>)` (none before), 62 `root(cid, n)`, 96 public holders, 1 nullifier; 15 private
  reads; 2 private sends.
- The release's step table on this path names the user's own release account, not a relayer (test_06).

As in logoseth-3: the private deposit, claim, burn and release (to a fresh address, sent from the user's release
account); the vault's admin through `tx_sender_module` (accept, cap, a cap forced to revert after approval and reported
in the admin tab, block 20, 29,997 of 30,000 gas; freeze and lift; delist and list the second token; a fee schedule; a
release delay of 7,200 s; each read back). 11 approvals at `evm_signer_cli` (`approver/`, `run.log`), every signed gas
limit equal to the module's recorded one and above the gas used (the review's `gascheck.py`, box log).

Every user and admin Ethereum send after the vault's deployment went through `tx_sender_module`, approved at
`evm_signer_cli` (`eth/transactions.tsv`, blocks 10 and 17-26). Outside the stack: the deployment (blocks 1-9: the
deployer's contract creations, `registerToken` twice, `renounceOwnership`, and `transferAdmin` offering the admin role
the admin accepts through the stack in block 18), the stand-in prover's `accept` to the mock verifier and the relayer's
checkpoint transactions. The LEZ admin message's signer is the stand-in above.

Record note: `seqnet-chain`'s call log (`miniapp/chain-calls.jsonl`) held the role password of the one
`module.approve_role` call (a local test password, set in `e2e/seqnet/hook-logoseth.sh`); it was replaced by
`<redacted>` in this record, and `seqnet-chain` no longer logs it.
