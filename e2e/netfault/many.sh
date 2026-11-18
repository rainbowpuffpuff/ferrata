#!/usr/bin/env bash
# e2e/netfault/many.sh <first seed> <last seed> [parallel]: run.sh for each seed (some side by side), then a summary.
set -uo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
first="${1:?first seed}"; last="${2:?last seed}"; par="${3:-4}"
out="$here/e2e/netfault/results"; mkdir -p "$out"
( cd "$here/vault" && forge build script/Crown.s.sol script/DeployVault.s.sol script/ForkRehearsal.s.sol ) > "$out/build.log" 2>&1 \
  || { tail -n 20 "$out/build.log"; exit 1; }
seq "$first" "$last" | xargs -P "$par" -I{} bash -c '"$0/e2e/netfault/run.sh" {} > "$0/e2e/runs/netfault-{}.out" 2>&1 || echo "seed {}: run.sh exited $?"' "$here"
python3 -I - "$out/seeds.jsonl" "$first" "$last" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
rows = [r for r in rows if int(sys.argv[2]) <= r["seed"] <= int(sys.argv[3])]
by = {}
for r in rows: by.setdefault(r["outcome"], []).append(r["seed"])
print(f"{len(rows)} seeds: " + ", ".join(f"{k} {len(v)}" for k, v in sorted(by.items())))
print("violations:", sum(len(r["violations"]) for r in rows), [ (r["seed"], r["violations"]) for r in rows if r["violations"]])
f = {}
for r in rows:
    for k, v in r["faults"].items(): f[k] = f.get(k, 0) + v
print("faults injected:", dict(sorted(f.items())))
print("stage reruns:", sum(r["stage_reruns"] for r in rows), "resumes:", sum(r["resumes"] for r in rows))
PY
