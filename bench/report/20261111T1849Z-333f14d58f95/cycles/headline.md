| update_1 (period 1900, 510/512 signers), user cycles | blst-accel |
|---|---:|
| `process_update`, finality update (no adoption) | 10.21 M |
| `process_update`, handoff update, `Adoption::OnChain` | 59.67 M |
|   of which `check_committee_onchain` (512 keys on-curve + not identity, sum) | 48.70 M |
|   of which committee `hash_tree_root` + next-committee branch | 1.13 M |
| one whole execution, finality update: decode saved state + update, policy, new root | 10.37 M (11 seg.) |
|   the same incl. paging + padding (Σ 2^po2) | 11.53 M |
| one whole execution, handoff update (OnChain adoption) | 59.83 M (60 seg.) |
|   the same incl. paging + padding (Σ 2^po2) | 62.91 M |
| split via the core, execution 1: finality checks + `Blst::miller_half` | 6.41 M (7 seg.) |
|   of which `Blst::miller_half` itself | 6.09 M |
| split via the core, execution 2: `Blst::final_half` | 3.60 M (4 seg.) |
|   of which `Blst::final_half` itself | 3.59 M |
| 3-step job, step 1: everything before the Miller loops | 2.31 M (3 seg.) |
| 3-step job, step 2: Miller loops | 4.20 M (5 seg.) |
|   of which the two Miller loops + Fp12 mul | 4.09 M |
| 3-step job, step 3: final exponentiation + check (`final_half`) | 3.60 M (4 seg.) |
| `initialize_with(.., Adoption::OnChain)` from the bootstrap | 49.94 M |
| `check_committee` (Full: per-key subgroup check), for comparison | 229.28 M |
| `process_update`, handoff update, `Adoption::Full` (for comparison) | n/a |
| `FinalizedStore::from_core` (restore; forces the Full check on both committees) | 458.60 M |
