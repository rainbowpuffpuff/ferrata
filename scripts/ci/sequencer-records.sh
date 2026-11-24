# Helpers of scripts/ci/sequencer-attest.sh, sourced by it and tested by scripts/ci/test-sequencer-records.sh.
# bash, awk and python3 only.

# scrub_paths DIR BESIDE HOME [REPO]: in every text file under DIR, the machine's own paths replaced with
# relative ones (BESIDE/ -> ../, HOME/ -> ~/), literally; with REPO, the repository's own path and the relative
# path a tool writes to it from beside it (../NAME/, NAME the repository's directory name) both -> <repo>/,
# so no record names the checkout's directory. Binary files (a NUL in their first 4 KiB) left alone. The
# script runs it on every exit, so a failed run's records are scrubbed too before CI uploads them.
scrub_paths() {
  python3 -I - "$1" "$2" "$3" "${4:-}" <<'PY'
import os, sys
top, beside, home, repo = sys.argv[1:5]
pairs = []
if repo and repo != "/":
    pairs.append((repo.rstrip("/").encode() + b"/", b"<repo>/"))
for old, rel in ((beside, b"../"), (home, b"~/")):
    if old and old != "/":
        pairs.append((old.rstrip("/").encode() + b"/", rel))
if repo and repo != "/":
    pairs.append((b"../" + os.path.basename(repo.rstrip("/")).encode() + b"/", b"<repo>/"))
for d, _, files in os.walk(top):
    for f in files:
        p = os.path.join(d, f)
        if os.path.islink(p):
            continue
        try:
            with open(p, "rb") as h:
                data = h.read()
        except OSError:
            continue
        if b"\0" in data[:4096]:
            continue
        new = data
        for old, rel in pairs:
            new = new.replace(old, rel)
        if new != data:
            with open(p, "wb") as h:
                h.write(new)
PY
}

# r0vm_of HOME VERSION: the r0vm of VERSION that rzup installed in the RISC Zero home HOME, which it keeps inside
# cargo-risczero's extension (HOME/extensions/v<VERSION>-cargo-risczero-<target>/r0vm, no bin/ link), else an r0vm on
# PATH that reports VERSION; status 1 if there is neither. A GitHub runner has r0vm on PATH in neither version:
# scripts/ci/install-risc0.sh installs through rzup only, and risc0-zkvm finds its own version through RISC0_HOME.
r0vm_of() {
  local d
  for d in "$1"/extensions/v"$2"-cargo-risczero-*/r0vm; do
    if [ -f "$d" ] && [ -x "$d" ]; then echo "$d"; return 0; fi
  done
  d="$(command -v r0vm 2>/dev/null || true)"
  if [ -n "$d" ] && "$d" --version 2>/dev/null | grep -q -E "(^| )$2\$"; then echo "$d"; return 0; fi
  return 1
}

# consumer_deployed RECORDS: the reference consumer was deployed through the loader in this run: its
# CreateHeader and each of its segments (programs.tsv's count) applied, per run.tsv. programs.tsv alone
# proves nothing: the driver lists the consumer there whenever it is given --consumer-bin.
consumer_deployed() {
  local segments
  segments=$(awk -F'\t' '$1 == "consumer" { print $7 }' "$1/programs.tsv" 2> /dev/null)
  case "$segments" in ''|*[!0-9]*) return 1 ;; esac
  awk -F'\t' -v n="$segments" '
    $2 ~ /^deploy consumer: CreateHeader/ && $6 == "applied" { header = 1 }
    $2 ~ /^deploy consumer: WriteSegment [0-9]+ of / && $6 == "applied" { split($2, w, " "); seen[w[4]] = 1 }
    END { c = 0; for (k in seen) c++; exit !(header && c == n) }' "$1/run.tsv"
}
