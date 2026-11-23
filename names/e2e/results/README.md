# Records of the names program's runs

| Record | Program | What |
|---|---|---|
| [`local-rc3-v2/`](local-rc3-v2/) | v2 (`0x28d63f20…be9d`) | `box.sh e2e local-rc3-v2`: deployed on LEZ v0.4.0-rc3's standalone sequencer, 42 CLI checks, the review's M1 race replayed, the fees |
| [`harness-rc3/`](harness-rc3/) | v2 | `box.sh harness`: `names` and `names_review` on rc3, their tables (identical to `lez/results/names-*.md`, the rc1 run) |
| [`harness-rc1/`](harness-rc1/) | v2 | the rc1 run's test summary (its tables are `lez/results/names-*.md`) |
| [`image-ids.tsv`](image-ids.tsv) | v2 | `box.sh ids`: the rc3 and rc1 builds (`names/image-ids.md`) |
| [`review-before/`](review-before/) | v1 | the review's findings as tests, run on v1 before the fixes: harness 8 of 9 fail, JS 6 of 6 fail |
| [`local-rc3-2/`](local-rc3-2/) | v1 (`0xee9578ca…f790`, live) | round 2's end-to-end run of v1 (31 checks); kept as v1's record |

`python3 -I names/e2e/claims.py` recomputes every count from these files.
