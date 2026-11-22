# Decision records held back until their work is merged

Records 0027 to 0031 record decisions of 21 November 2026 whose work is not in this tree: it is on the branches
`rfp-softs` (0027 recorded statements, 0028 the operator rebate, 0029 voluntary disclosure, 0030 the web depositor,
0031 the committed-amounts path) and, for evidence 0016 once cited, `rfp-ci-release`. In `docs/decisions/` they
pointed at files this tree does not have and said the opposite of `docs/trust.md`, `docs/design/attestation.md` and
`docs/traceability.md` here, so they wait here, outside the log (review of 21 November 2026, M-5).

When `rfp-softs` is merged, in the same commit:

1. move the five files back into `docs/decisions/` (their numbers follow 0026), and put back at the end of 0013's
   rationale the sentence "ADR 0029 (21 November 2026) adds the disclosure of deposits and claims, wallet-side."
   (taken out because 0029 is not in the log until then);
2. drop "(branch rfp-softs)" from their statuses and the "branch …, commit …:" prefixes from their sources, citing the
   files instead;
3. update `docs/trust.md` (the rebate "design only"), `docs/design/attestation.md` §9 (the rebate) and the RFP-021
   Soft.1, Soft.5 and RFP-022 Soft.1, Soft.2, Soft.5 rows of `docs/traceability.md`, which describe this tree without
   that work;
4. run `python3 -I scripts/check-decisions.py`: it also requires a source or a "reconstructed" mark on every option
   that is not chosen, which 0027, 0030 and 0031 do not yet give for their options 1 and 3 (0028 and 0029 were
   corrected here: options no source recorded were replaced by the recorded alternatives).
