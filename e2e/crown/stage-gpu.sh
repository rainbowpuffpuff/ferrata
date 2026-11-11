#!/usr/bin/env bash
# e2e/crown/stage-gpu.sh <fixture dir> <name> <programs dir with R0BF checkpoint_b.bin checkpoint.bin segment.bin + ids.jsonl> <out>
# The gpu-prover inputs layout (programs/, fixtures/, MANIFEST.json) for one rule (B) fixture.
set -euo pipefail
fx="$1"; name="$2"; prog="$3"; out="$4"; commit="$(git rev-parse HEAD 2>/dev/null || echo "not a git checkout")"
rm -rf "$out"; mkdir -p "$out/programs" "$out/fixtures"
cp -p "$prog/checkpoint_b.bin" "$out/programs/checkpoint-b.bin"; cp -p "$prog/checkpoint.bin" "$prog/segment.bin" "$out/programs/"
cp -p "$fx/$name.input" "$fx/$name.json" "$out/fixtures/"
id() { sed -n "s|.*\"program\":\"[^\"]*/$1.bin\".*\"image_id\":\"\([^\"]*\)\".*|\1|p; s|.*\"image_id\":\"\([^\"]*\)\".*\"program\":\"[^\"]*/$1.bin\".*|\1|p" "$prog/ids.jsonl" | head -n 1; }
python3 -I - "$out" "$commit" "$(id checkpoint_b)" "$(id checkpoint)" "$(id segment)" "$name" <<'PY'
import hashlib, json, os, sys
out, commit, cb, c, s, name = sys.argv[1:]
files = {}
for d in ("programs", "fixtures"):
    for f in sorted(os.listdir(os.path.join(out, d))):
        b = open(os.path.join(out, d, f), "rb").read()
        files[f"{d}/{f}"] = {"bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}
m = {"source": f"crown-proofs stage-gpu: {name}", "commit": commit,
     "programs_from": "the crown build's user ELFs (spike/tmp/crown/build/programs) as R0BF program binaries (ferrata-e2e program-binary)",
     "local_image_ids": {"checkpoint": c, "checkpoint-b": cb, "segment": s}, "files": files}
assert all(v.startswith("0x") and len(v) == 66 for v in (cb, c, s)), m["local_image_ids"]
json.dump(m, open(os.path.join(out, "MANIFEST.json"), "w"), indent=2)
print(json.dumps(m["local_image_ids"]))
PY
