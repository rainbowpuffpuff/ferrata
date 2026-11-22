# seqnet logoseth-1: the mini-app's round trip with its Ethereum side through Logos Core's own modules (21 Nov 2026)

`e2e/seqnet/run.sh logoseth logoseth-1` on the Modal build box (from the worktree's root through `spike/modal-build/mrun`, as run: `export SEQNET_TMP=<scratch>/seqnet SEQNET_SHORT=/var/tmp/rfpf; bash e2e/seqnet/run.sh build; bash e2e/seqnet/run.sh build-logoseth; bash e2e/seqnet/run.sh logoseth logoseth-1`, the crown-live-3 inputs put in `$SEQNET_TMP/inputs` with `mrun --put`; branch `rfp-logos-core-eth`
at `6a3ee5f9` + the SEQNET_SHORT fix), after `run.sh build` and `run.sh build-logoseth`. Local only: a LEZ v0.4.0-rc1 zone
we ran (the crown-live-3 bundle: one sequencer, one Logos Blockchain node, dev-mode leader proofs), anvil with chain id
11155111, the vault tests' mock verifier; private proofs in RISC Zero dev mode (`SEQNET_PROOFS=dev`).

**The mini-app's view (`Main.qml`), driven headless by Qt Quick Test (`tst_miniapp_logos.qml`, `SEQNET_ETH=logos`),
called `ferrata_bridge_module` in ONE headless Logos Core holding Logos's `eth_rpc_module`, `fee_module`,
`keystore_module`, `signer_manager_module`, `tx_sender_module`, `evm_signer_cli`, `evm_keystore_cli`, and `lez_core`
(patched): 11 of 11 test functions passed** (`miniapp/qmltest.txt`, 179 s), the whole run 468 s.

- Ethereum: every vault read through `eth_rpc_module` (`miniapp/eth-report.json`); the user's private deposit (block 10,
  from the user), the user's release (block 17, from the user's release account, 0.000997 ETH paid to the burn's fresh
  recipient), and the vault's admin calls from the admin tab and the module (`acceptAdmin` and `setTokenCap` from the
  admin, blocks 18-19; `setTokenDepositsFrozen` and its lifting from the freeze authority, blocks 20-21) all sent through
  `tx_sender_module`, each approved at `evm_signer_cli` (`approver/`: what the approver showed and the decision; every
  request "requested by tx_sender_module ... [asked by ferrata_bridge_module]"); a deposit while frozen refused by the
  module before anything was asked or signed (`TokenDepositsAreFrozen`). Keys imported by `evm_keystore_cli`
  (`miniapp/import-*.json`); the module holds none. `eth/transactions.tsv` lists every Ethereum transaction and its sender.
- LEZ: the session's LEZ reads (162, each equal to the chain service's), the private holding (15 reads, equal) and the
  private claim and burn (2 sends) through `lez_core` (`miniapp/lez-report.json`).
- Stand-ins, named: the stand-in committee finalizing anvil's blocks for LEZ's light client; the operator's import and the
  relayer's checkpoint (`ops.sh`, `submitCandidate` and `finalizeAttested` from the relayer's account, the plain path);
  the stand-in prover (`mock verifier` accepts, from the deployer) instead of Groth16 wraps; `seqnet-chain` for the
  zone's history (imported roots, published burns: `lez_core`'s wallet side serves no blocks); the approval given by a
  script at `evm_signer_cli` (not Basecamp's Signer app); the gas drop's stand-in (`seqnet-fund`).

`SUMMARY.md` is the generated summary; `miniapp/screenshots/` the view at each step (05b and 16b: the view waiting for the
Logos signer; 20-25: the vault's admin section).
