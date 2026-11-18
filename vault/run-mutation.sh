#!/usr/bin/env bash
# The vault's Gambit mutation run (vault/MUTATION.md). Run from vault/ with forge 1.3.6, solc 0.8.26 as `solc` on PATH,
# and Gambit (Certora/gambit tag v1.0.7, commit 072ff4c6) as `gambit` on PATH. WORKERS parallel copies (default 28).
# RUN names the result files (default run2). IDS, a file of mutant names, re-runs only those, on the mutants already
# generated in notes/gambit_out (run3: the survivors of run2, after the tests that kill them were added).
set -euo pipefail
cd "$(dirname "$0")"
OUT=results/mutation
RUN=${RUN:-run2}
mkdir -p notes "$OUT"
if [ -z "${IDS:-}" ]; then
  rm -rf notes/gambit_out notes/gambit-ws
  gambit mutate --json gambit.conf.json > notes/gambit-mutate.log 2>&1
  tail -n 2 notes/gambit-mutate.log
fi
DIRS=notes/gambit_out/vault,notes/gambit_out/registry
# The results are written under notes/, which no sync touches, and copied into results/ at the end.
rm -f "notes/$RUN.jsonl"
python3 -I tools/gambit_run.py run "$DIRS" "notes/$RUN.jsonl" --workers "${WORKERS:-28}" ${IDS:+--ids-file "$IDS"}
cp "notes/$RUN.jsonl" "$OUT/$RUN.jsonl"
python3 -I tools/gambit_run.py summary "$DIRS" "$OUT/$RUN.jsonl" --survivors "$OUT/$RUN-survivors.md" \
  | tee "$OUT/$RUN-summary.txt"
if [ -z "${IDS:-}" ]; then
  for d in vault registry; do
    cp "notes/gambit_out/$d/mutants.log" "$OUT/gambit-$d-mutants.log"
    gzip -9 -n -c "notes/gambit_out/$d/gambit_results.json" > "$OUT/gambit-$d-results.json.gz"
  done
fi
