#!/usr/bin/env bash
# Watch for LEZ testnet 0.4: read-only, generic User-Agent, no token, nothing about who asks.
#
#   lez/testnet04/watch.sh            print what changed since the last run, and the 0.4 row as it stands
#
# Reads (GET only): logos-docs' "Testnet endpoints" page (docs/get-started/testnet-endpoints.md on main, raw) and its
# newest commit; LEZ's releases and tags (logos-blockchain/logos-execution-zone, GitHub's public API, unauthenticated).
# Keeps what it saw in FERRATA_WATCH_STATE (default: beside the repository, tmp/testnet04/watch/last.json).
# Exit: 0 nothing changed; 10 something changed; 20 the 0.4 row names an endpoint (no longer TBD) or a v0.4.0 tag
# without -rc appeared: day one (README.md here); 1 a source did not answer.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BESIDE="$(cd "$HERE/../../.." && pwd)"
STATE="${FERRATA_WATCH_STATE:-$BESIDE/tmp/testnet04/watch/last.json}"
mkdir -p "$(dirname "$STATE")"
python3 -I - "$STATE" <<'EOF'
import json, os, re, sys, time, urllib.request
state_path = sys.argv[1]
UA = "curl/8"
DOCS = "https://raw.githubusercontent.com/logos-co/logos-docs/main/docs/get-started/testnet-endpoints.md"
DOCS_COMMITS = "https://api.github.com/repos/logos-co/logos-docs/commits?path=docs/get-started/testnet-endpoints.md&per_page=1"
LEZ = "https://api.github.com/repos/logos-blockchain/logos-execution-zone"
def get(url, accept="application/vnd.github+json"):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read().decode()
now = {"read_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
errors = []
try:
    page = get(DOCS, "text/plain")
    rows = [l for l in page.splitlines() if l.startswith("|") and not set(l) <= set("|:- ")]
    row04 = next((l for l in rows if re.match(r"^\|\s*0\.4\b", l)), None)
    now["docs_row_04"] = row04
    now["docs_version_tip"] = (re.search(r"accurate for \*\*([^*]+)\*\*", page) or [None, None])[1]
    now["docs_rows"] = rows[1:]
except Exception as e:
    errors.append(f"docs page: {e}")
try:
    c = json.loads(get(DOCS_COMMITS))
    now["docs_commit"] = f"{c[0]['sha'][:9]} {c[0]['commit']['committer']['date']} {c[0]['commit']['message'].splitlines()[0]}" if c else None
except Exception as e:
    errors.append(f"docs commits: {e}")
try:
    now["lez_tags"] = [t["name"] for t in json.loads(get(f"{LEZ}/tags?per_page=15"))]
    now["lez_releases"] = [f"{r['tag_name']} {r['published_at']} {r['name']}" for r in json.loads(get(f"{LEZ}/releases?per_page=8"))]
except Exception as e:
    errors.append(f"LEZ tags/releases: {e}")
prev = {}
if os.path.exists(state_path):
    try:
        prev = json.load(open(state_path))
    except Exception:
        prev = {}
changed = []
for k in ["docs_row_04", "docs_version_tip", "docs_commit", "lez_tags", "lez_releases", "docs_rows"]:
    if k in now and prev.get(k) != now[k]:
        if k in ("lez_tags", "lez_releases", "docs_rows") and isinstance(prev.get(k), list):
            new = [x for x in now[k] if x not in prev[k]]
            gone = [x for x in prev[k] if x not in now[k]]
            changed.append(f"{k}: new {new}" + (f", gone {gone}" if gone else ""))
        else:
            changed.append(f"{k}: {prev.get(k)!r} -> {now[k]!r}")
row = now.get("docs_row_04") or ""
cells = [c.strip() for c in row.strip("|").split("|")] if row else []
endpoint = [c for c in cells[1:] if c and c.upper() != "TBD"]
final_tag = [t for t in now.get("lez_tags", []) if re.fullmatch(r"v0\.4\.\d+", t)]
print(f"read {now['read_utc']}")
print(f"0.4 row: {row or '(none)'}")
print(f"docs page: {now.get('docs_version_tip')}; newest commit: {now.get('docs_commit')}")
print(f"LEZ newest tags: {', '.join(now.get('lez_tags', [])[:5])}; releases: {'; '.join(now.get('lez_releases', [])[:3])}")
if not prev:
    print(f"first run: what was seen is kept in {state_path}")
for c in changed if prev else []:
    print(f"CHANGED {c}")
if not changed and prev:
    print("no change since", prev.get("read_utc"))
for e in errors:
    print(f"ERROR {e}")
merged = dict(prev); merged.update(now)
json.dump(merged, open(state_path, "w"), indent=1)
if errors:
    sys.exit(1)
if endpoint or final_tag:
    print(f"TESTNET 0.4: row cells {endpoint or '-'}; final tags {final_tag or '-'}: day one (lez/testnet04/README.md)")
    sys.exit(20)
sys.exit(10 if changed and prev else 0)
EOF
