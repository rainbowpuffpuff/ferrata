#!/usr/bin/env bash
# The audit rows (RFP-021 S.8, RFP-022 Sec.1) can have no functional test until the audit reports are
# published: the reviews are contracted by Logos, not ours to produce. What the bid promises until then
# is that the code is marked unaudited / not for production, everywhere a user or a catalog reader meets
# it: README.md, SECURITY.md, the catalog's description (modules/catalog/logos-repo.json) and every
# module's own description (modules/*/metadata.json).
#
# Fails with one line per file whose marking is missing. Exits 0 when every file carries it.
#
#   bash scripts/ci/check-unaudited.sh      # from the repository root
#
# Standard tools only (bash, grep, python3). Run by CI's `links` job.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
cd "$repo"

# A marking says the code is unaudited or not for production: "unaudited", "not audited",
# "has not been audited", "not for production", "unfit for production". "prototype" alone is a maturity
# label, not this promise, and does not pass. (One pattern per engine: POSIX for grep, \s for Python.)
marking='unaudited|not[[:space:]]+(been[[:space:]]+)?audited|not[[:space:]]+for[[:space:]]+production|unfit[[:space:]]+for[[:space:]]+production'
marking_py='unaudited|not\s+(been\s+)?audited|not\s+for\s+production|unfit\s+for\s+production'

missing=0
check_file() { # PATH
  if grep -qiE "$marking" "$1"; then
    return 0
  fi
  echo "check-unaudited.sh: no unaudited / not-for-production marking in $1" >&2
  missing=1
}

check_description() { # PATH (a JSON file whose "description" field is the marking's home)
  if python3 -I - "$1" "$marking_py" <<'EOF'
import json, re, sys
path, marking = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    description = json.load(handle).get("description", "")
sys.exit(0 if re.search(marking, description, re.IGNORECASE) else 1)
EOF
  then
    return 0
  fi
  echo "check-unaudited.sh: no unaudited / not-for-production marking in $1's description" >&2
  missing=1
}

check_file README.md
check_file SECURITY.md
check_description modules/catalog/logos-repo.json
shopt -s nullglob
for metadata in modules/*/metadata.json; do
  check_description "$metadata"
done

if [ "$missing" -ne 0 ]; then
  echo "check-unaudited.sh: mark each file above unaudited / not for production, or publish the audit reports" >&2
  exit 1
fi
echo "check-unaudited.sh: every file is marked unaudited / not for production"
