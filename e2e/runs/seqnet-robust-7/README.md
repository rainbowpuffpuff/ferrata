# seqnet-robust-7: the robust run with the fault cases of RFP-022 R.6 and R.7 (33 of 33)

`bash e2e/seqnet/run.sh robust robust-7` on the Modal build box, branch recovery-022 at `6483625b`
(`e2e/seqnet/hook-robust.sh`, `deposit-leg.sh`, `robust-r06.sh`; cases a-l in the hook's header), 24-25 Nov 2026:
LEZ v0.4.0-rc1 sequencer and node (one validator, dev-mode leader proofs), anvil with chain id 11155111, the vault
over risc0-ethereum 3.1.0's verifier address with the mock's code (`anvil_setCode`), dev-mode private proofs, the
stand-in prover behind the prover client. Verdicts: [robust.md](robust.md); facts: [robust.json](robust.json); the
attestor's attestations: `attestor/rounds.tar.zst`; the prover client's and the stand-in prover's lines: `prover/`.
Earlier attempts on this branch (not committed): robust-5 (30 of 33: the harness's kills reached a subshell, not the
relayer) and robust-6 (32 of 33: a check counted the relayer's fee withdrawal as a second release send).
