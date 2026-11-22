# seqnet logoseth-5: the mini-app's round trip and admin, Ethereum through Logos Core's own modules (22 Nov 2026)

As `../seqnet-logoseth-4/` (a LEZ v0.4.0-rc1 zone we ran, anvil with chain id 11155111, the mock verifier, dev-mode
private proofs), with the third review's fixes: branch `rfp-logos-core-eth`, tree as of `6be33328`, the module's `.lgx`
of `modules/results/bridge-send-stack-r4/` (sha256 `65bbcf8a…`, `miniapp/lgx-sha256.txt`). On the Modal build box, from
the worktree's root, as run (the crown-live-3 inputs in `$SEQNET_TMP/inputs`, as `run.sh`'s header lists them):

    export SEQNET_TMP=<scratch>/seqnet SEQNET_SHORT=/var/tmp/rfx
    bash e2e/seqnet/run.sh build-logoseth                                  # once: the tools, logosctl, the send stack, lez_core (patched), the runner
    D="$SEQNET_TMP/logos/lgx-ferrata_bridge_module"                        # build-logoseth's own module .lgx (its build directory differs),
    chmod -R u+w "$D"; rm -rf "$D"                                         # replaced as run.sh does (the source is a /nix/store symlink):
    cp -rL <out>/check/lgx-ferrata_bridge_module "$D"                      # the r4 build's module .lgx
    bash e2e/seqnet/run.sh logoseth logoseth-5                             # rebuilds seqnet-chain first

Run 660 s; Qt Quick Test 13 passed of 13 (11 test functions plus `initTestCase`/`cleanupTestCase`; `miniapp/qmltest.txt`,
267.2 s).

New in this run:
- **The chain service holds no Ethereum key.** `seqnet-chain` was started with the user's address alone (`--user`, no
  keystore): `miniapp/chain-keys.json` records its own answer, `holds_eth_key: false`, and its role gate (two role
  accounts, approval by password, hold 900 s). Its call log has no `module.send_deposit` (`miniapp/chain-calls.jsonl`).
  It still holds the user's LEZ wallet (made from the seed: the session's bridge seed and accounts, the public account
  that pays the role messages' fee) and the two LEZ role keys behind its approval step.
- **The non-admin SetCap differs from the admin's in the signer alone.** The admin's `SetCap` (held as `lra_1`, approved
  for exactly its message, SHA-256 `0x0157e912…`) was accepted in LEZ block 40, signed by the admin's key `0x10debe6f…`
  with the fee paid by the user's public account /0 `0x8a7162a5…`. The same message with another capacity, signed by
  the user's public account /1 `0xc0fe2bd9…` (the run funds only public /0 and private /0, `seqnet-fund`) with the fee
  paid by the same /0, was included in LEZ block 41 and reverted; the mint cap read back unchanged (12,000,000,000).
  LEZ v0.4.0-rc1's sequencer records only that a block's transaction reverted, not the program's code; the code for
  this case, `NOT_AUTHORIZED`, is shown by the harness (`lez/harness/tests/bridge_controls.rs`, "SetCap by a signer
  with no role").
- **A deposit not mined in time, in the view.** With anvil's automine off (`ops.sh automine off`, block 26) and the
  view's receipt wait of 8 s, a deposit approved at `evm_signer_cli` was broadcast and not mined: the view said "Sent,
  not confirmed yet (not a failure): broadcast, not yet mined: transaction 0x355fbedd… … Do not send this deposit again"
  (screenshot 30), never "Not deposited". A second deposit was refused by the module before anything was asked: "Not
  deposited: refused: your earlier deposit, transaction 0x355fbedd…, was broadcast and is not mined yet …", and the
  view offered "Deposit again anyway" (screenshot 31). One block later (`ops.sh mine`) the first deposit was in the
  vault's list (Ethereum block 27); the control then sent a deposit with `again: true` (its approval prompt warns that
  both land if the earlier one is mined after all, screenshot 32), approved like any other and mined in block 28; both
  are in the positions (screenshot 33).
- **`lez_core`'s empty answers are no longer counted as agreeing.** Over the run `lez_core` read 346 bridge states:
  190 answered, all equal to the chain service's (69 `config(cid)`, 52 `token(cid, ETH)`, 69 `root(cid, n)`), and 156
  got no answer, because `lez_core` answers nothing for an account the zone does not have (and for a failed read, which
  it cannot tell apart): 128 public holders, 18 `token(cid, <the second token>)`, 5 `config(cid)` and 4
  `token(cid, ETH)` before the operator's first import, 1 nullifier before the claim. None failed; none differed
  (`miniapp/lez-report.json`). The registry's `REGISTRY LEZ READS`: ETH 3 of 3 equal; the second token 2 equal
  (`config(cid)`, the root) and its `token(cid, ·)` unanswered: it has no LEZ state, because it is not registered on LEZ
  in this run (`Register` needs the relayer's attested registry entry, which the run's operator makes for ETH only), so
  no non-ETH token's LEZ state is compared. (logoseth-4's "293 of 293 equal" counted such empty reads as equal: its 96
  holders, 17 second-token gates and the early reads.) 19 private reads, all equal; 2 private sends.
- Logos Core was started with `FERRATA_DEPLOYMENTS=11155111:<the run's vault>`; every send of the run went to it. The
  release's step table says the release account is unlinked from the depositor only if it is not funded from there
  (test_06).

As in logoseth-4: the private deposit, claim, burn and release (to a fresh address, sent from the user's release
account); the vault's admin through `tx_sender_module` (accept, cap, a cap forced to revert after approval and reported
in the admin tab, block 20, 29,997 of 30,000 gas; freeze and lift; delist and list the second token; a fee schedule; a
release delay of 7,200 s; each read back); the LEZ `Sunset` held as `lra_2` and rejected by the person (no Sunset among
the zone's transactions, `sunset: false`). 13 approvals at `evm_signer_cli` (`approver/`, `run.log`), every signed gas
limit above the gas used (the third review's `gascheck.py` over this record: 13 prompts, 13 transactions); the role password appears as `<redacted>` in the
chain service's call log and in no file of this record.

Every user and admin Ethereum send after the vault's deployment went through `tx_sender_module`, approved at
`evm_signer_cli` (`eth/transactions.tsv`, blocks 10 and 17-28). Outside the stack: the deployment (blocks 1-9), the
stand-in prover's `accept` to the mock verifier and the relayer's checkpoint transactions. The LEZ admin messages'
signer is the stand-in above. "export failed (1)" in `run.log` is the crown session's best-effort export
(`lez/crown/session.sh`), as in logoseth-3 and -4; it is not a check.
