#!/usr/bin/env bash
# The vault's Gambit mutation run (vault/MUTATION.md). Run from vault/ with forge 1.3.6, solc 0.8.26 as `solc` on PATH,
# and Gambit (Certora/gambit tag v1.0.7, commit 072ff4c6) as `gambit` on PATH. WORKERS parallel copies (default 28).
set -euo pipefail
cd "$(dirname "$0")"
OUT=results/mutation
mkdir -p notes "$OUT"
rm -rf notes/gambit_out notes/gambit-ws
gambit mutate --json gambit.conf.json > notes/gambit-mutate.log 2>&1
tail -n 2 notes/gambit-mutate.log
DIRS=notes/gambit_out/vault,notes/gambit_out/registry
python3 -I tools/gambit_run.py run "$DIRS" "$OUT/${RUN:-run1}.jsonl" --workers "${WORKERS:-28}"
python3 -I tools/gambit_run.py summary "$DIRS" "$OUT/${RUN:-run1}.jsonl" --survivors "$OUT/${RUN:-run1}-survivors.md" | tee "$OUT/${RUN:-run1}-summary.txt"
for d in vault registry; do
  cp "notes/gambit_out/$d/mutants.log" "$OUT/gambit-$d-mutants.log"
  gzip -9 -n -c "notes/gambit_out/$d/gambit_results.json" > "$OUT/gambit-$d-results.json.gz"
done
