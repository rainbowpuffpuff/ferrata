#!/usr/bin/env python3
"""Compares two builds of the Basecamp release (build.json files of basecamp-release.sh build, made on two machines from
the same commit) package by package: SHA-256, size, rootHash.

    python3 -I modules/scripts/basecamp_compare.py A.json B.json OUT.json [LABEL_A LABEL_B]

Writes OUT.json ({"source_commit", "a", "b", "packages": [{file, size_a, size_b, sha256_a, sha256_b, same}], "same",
"differ"}) and prints one line per package. Exits 0 whatever the result (the record says it). Standard library only.
"""
import json
import sys

a_path, b_path, out_path = sys.argv[1:4]
labels = sys.argv[4:6] if len(sys.argv) >= 6 else [a_path, b_path]
a, b = (json.load(open(p, encoding="utf-8")) for p in (a_path, b_path))
if a["source_commit"] != b["source_commit"]:
    sys.exit(f"basecamp_compare.py: different commits {a['source_commit']} and {b['source_commit']}")
pa = {p["file"]: p for p in a["packages"]}
pb = {p["file"]: p for p in b["packages"]}
rows = []
for f in sorted(set(pa) | set(pb)):
    x, y = pa.get(f), pb.get(f)
    row = {"file": f, "size_a": x and x["size"], "size_b": y and y["size"], "sha256_a": x and x["sha256"],
           "sha256_b": y and y["sha256"], "rootHash_a": x and x["rootHash"], "rootHash_b": y and y["rootHash"]}
    row["same"] = bool(x and y and x["sha256"] == y["sha256"])
    rows.append(row)
    print(f"{'same  ' if row['same'] else 'DIFFER'} {f} {row['sha256_a']} {row['sha256_b']}")
out = {"source_commit": a["source_commit"], "a": labels[0], "b": labels[1], "packages": rows,
       "same": sum(r["same"] for r in rows), "differ": sum(not r["same"] for r in rows)}
json.dump(out, open(out_path, "w", encoding="utf-8"), indent=1)
print(f"{out['same']} of {len(rows)} byte-identical")
