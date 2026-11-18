#!/usr/bin/env bash
# e2e/netfault/many.sh <first seed> <last seed> [lanes, at most 4]: run.sh for each seed, then a summary.
set -uo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
first="${1:?first seed}"; last="${2:?last seed}"; par="${3:-4}"; [ "$par" -le 4 ] || par=4
out="$here/e2e/netfault/results"; mkdir -p "$out"
( cd "$here/vault" && forge build script/Crown.s.sol script/DeployVault.s.sol script/ForkRehearsal.s.sol ) > "$out/build.log" 2>&1 \
  || { tail -n 20 "$out/build.log"; exit 1; }
# Lanes side by side, one per chain id DeployVault accepts (forge's broadcast files are per chain id: two seeds on the
# same id must not run at once); the seeds of a lane one after another.
ids=(31337 11155111 560048 17000)
for lane in $(seq 0 $((par - 1))); do
  (
    for seed in $(seq "$first" "$last"); do
      [ $(( (seed - first) % par )) = "$lane" ] || continue
      NETFAULT_CHAIN_ID="${ids[$lane]}" "$here/e2e/netfault/run.sh" "$seed" > "$here/e2e/runs/netfault-$seed.out" 2>&1 \
        || echo "seed $seed: run.sh exited $?"
    done
  ) &
done
wait
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
