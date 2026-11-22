# Helpers of scripts/ci/sequencer-attest.sh, sourced by it and tested by scripts/ci/test-sequencer-records.sh.
# bash, awk and python3 only.

# scrub_paths DIR BESIDE HOME: in every text file under DIR, the machine's own paths replaced with relative
# ones (BESIDE/ -> ../, HOME/ -> ~/), literally; binary files (a NUL in their first 4 KiB) left alone. The
# script runs it on every exit, so a failed run's records are scrubbed too before CI uploads them.
scrub_paths() {
  python3 -I - "$1" "$2" "$3" <<'PY'
import os, sys
top, beside, home = sys.argv[1:4]
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
        for old, rel in ((beside, b"../"), (home, b"~/")):
            if old and old != "/":
                new = new.replace(old.rstrip("/").encode() + b"/", rel)
        if new != data:
            with open(p, "wb") as h:
                h.write(new)
PY
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
