#!/usr/bin/env bash
# Rebuild every program deployed on the Ferrata LEZ test network from its source commit and compare the image ids and
# binary sha256s with scripts/reproduce/deployed-programs.json (the list the /apps/ page shows).
#
#   scripts/reproduce/deployed.sh build   --out DIR [--work DIR]
#       for each source of deployed-programs.json: `git archive` of its commit into WORK/src-<commit> (skipped when that
#       directory exists, or unpacked from WORK/src-<commit>.tar.gz where git is not at hand), its "overlay" files copied in place,
#       the LEZ tag it was built against selected,
#       its build command run, scripts/reproduce/guest_ids.py's table of every guest written to DIR/<source>.tsv.
#   scripts/reproduce/deployed.sh compare --out DIR [--network URL]
#       one line per deployed program: recorded id, rebuilt id, binary sha256; exit 1 on any difference. With --network,
#       also scripts/reproduce/check_network.py: the network's /programs.json and each program's loader header on chain.
#   scripts/reproduce/deployed.sh docker  --out DIR [--network URL]
#       the pinned toolchains of scripts/reproduce/Dockerfile (the image of scripts/reproduce/run.sh), `build` inside it on
#       archives made here, then `compare`. Needs Docker and ~30 GB of disk.
#
# Needs: the toolchains of scripts/reproduce/Dockerfile (RISC Zero 3.0.5; guest Rust 1.91.1 in RISC0_HOME_LEZ, default
# ../risc0-home-lez beside the tree) and the LEZ sources beside the tree: ../upstream/lez-v0.4.0-rc1 (1806e87) and
# ../upstream/lez-v0.4.0-rc3 (bafd07be), as scripts/fetch-upstream.sh lays them out. Guest builds go through
# scripts/guest-rustc, so the work directory's path does not change any id (docs/image-ids.md, "two paths").
set -euo pipefail  # each step below that may fail is tested explicitly (if / ||), so -e stops only on the unexpected
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$SRC/.." && pwd)"
LIST="$HERE/deployed-programs.json"
OUT=""; WORK=""; NETWORK=""
mode="${1:-}"; shift || true
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --network) NETWORK="$2"; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT" ] || { sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
src_field() { python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); s=[x for x in d["sources"] if x["id"]==sys.argv[2]][0]; v=s.get(sys.argv[3], ""); print(" ".join(v) if isinstance(v, list) else ("1" if v is True else ("" if v is False else v)))' "$LIST" "$1" "$2"; }
# The tree at the commit, without what no guest build reads (the vault, documents, records, results, other workspaces).
SKIP=(':(exclude)vault' ':(exclude)docs' ':(exclude)spike' ':(exclude)releases' ':(exclude)e2e' ':(exclude)lez/results'
      ':(exclude)modules' ':(exclude)bench/perf' ':(exclude)fuzz' ':(exclude)gpu-prover' ':(exclude)checkpoint/realblocks')
sources() { python3 -I -c 'import json,sys; print(" ".join(s["id"] for s in json.load(open(sys.argv[1]))["sources"]))' "$LIST"; }

case "$mode" in
  build)
    WORK="${WORK:-$OUT/work}"; mkdir -p "$WORK" "$OUT/logs"; WORK="$(cd "$WORK" && pwd)"
    log() { echo "$(date -u +%T) $*" | tee -a "$OUT/logs/steps.txt"; }
    R0LEZ="${RISC0_HOME_LEZ:-$BESIDE/risc0-home-lez}"
    [ -d "$R0LEZ" ] || { log "missing $R0LEZ (RISC0_HOME_LEZ)"; exit 1; }
    # The copies resolve ../upstream and ../risc0-home-lez beside themselves, as the tree does.
    [ -e "$WORK/upstream" ] || ln -s "$BESIDE/upstream" "$WORK/upstream"
    [ -e "$WORK/risc0-home-lez" ] || ln -s "$R0LEZ" "$WORK/risc0-home-lez"
    log "toolchains: $(cargo --version), $(r0vm --version 2>&1 | head -1); nproc $(nproc)"
    fail=0
    for s in $(sources); do
      commit="$(src_field "$s" commit)"; tag="$(src_field "$s" lez_tag)"; pkgs="$(src_field "$s" packages)"
      rel="$(src_field "$s" release)"; locked="$(src_field "$s" locked)"
      # A deployed program whose source is not in public history has no commit to rebuild from: said, and counted.
      if [ -z "$commit" ]; then log "$s: its source is not in public history: not rebuilt"; fail=1; continue; fi
      d="$WORK/src-${commit:0:12}"
      if [ ! -d "$d" ] && [ -f "$d.tar.gz" ]; then  # an archive made where git is (`git archive`, gzip)
        mkdir -p "$d"
        if ! tar -xzf "$d.tar.gz" -C "$d"; then rm -rf "$d"; log "$s: cannot unpack $d.tar.gz"; fail=1; continue; fi
      elif [ ! -d "$d" ]; then
        mkdir -p "$d"
        # A failed archive must not leave an empty directory that the next run would take for an unpacked tree.
        if ! git -C "$SRC" archive "$commit" -- . "${SKIP[@]}" | tar -x -C "$d"; then
          rm -rf "$d"; log "$s: cannot archive $commit (fetch it, or copy its tree to $d)"; fail=1; continue
        fi
      fi
      # deployed-programs.json "overlay" (FROM:TO, copied within the copy before the build): a deployed program's source
      # kept beside its successor is built at the path it was built at, since paths enter a guest's build.
      for pair in $(src_field "$s" overlay); do
        cp "$d/${pair%%:*}" "$d/${pair#*:}" || { log "$s: overlay $pair failed"; fail=1; continue 2; }
      done
      [ -d "$BESIDE/upstream/lez-$tag" ] || { log "$s: missing $BESIDE/upstream/lez-$tag"; fail=1; continue; }
      if [ "$tag" != "v0.4.0-rc1" ] && grep -rlq --include=Cargo.toml 'upstream/lez-v0.4.0-rc1' "$d/lez"; then
        # names/e2e/box.sh copy_tree: select the tag, then the manifests lez-select.sh does not list follow it.
        ( cd "$d/lez" && scripts/lez-select.sh "$tag" ) >> "$OUT/logs/$s-select.log" 2>&1 || { log "$s: lez-select failed"; fail=1; continue; }
        { grep -rl --include=Cargo.toml 'upstream/lez-v0.4.0-rc1' "$d/lez" || true; } | xargs -r sed -i "s#upstream/lez-v0.4.0-rc1#upstream/lez-$tag#g"
      fi
      args=(build); [ -n "$rel" ] && args+=(--release); [ -n "$locked" ] && args+=(--locked)
      for p in $pkgs; do args+=(-p "$p"); done
      log "$s: ${commit:0:12} (LEZ $tag): cargo ${args[*]}"
      if ( cd "$d/lez" && env -u CARGO_TARGET_DIR -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u FERRATA_GUEST_REMAP_EXTRA \
             RUSTC_WRAPPER="$d/scripts/guest-rustc" RISC0_HOME="$R0LEZ" CARGO_TERM_COLOR=never \
             CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-16}" cargo "${args[@]}" ) > "$OUT/logs/$s-build.log" 2>&1; then
        python3 -I "$HERE/guest_ids.py" "$d" "$OUT/$s.tsv" > "$OUT/logs/$s-ids.log" 2>&1 || { log "$s: guest_ids failed"; fail=1; }
        log "$s: built, $(( $(wc -l < "$OUT/$s.tsv") - 1 )) guests"  # the TSV's first line is its header
      else
        log "$s: build FAILED"; tail -n 15 "$OUT/logs/$s-build.log"; fail=1
      fi
    done
    exit $fail
    ;;
  compare)
    python3 -I "$HERE/check_network.py" compare --list "$LIST" --out "$OUT" ${NETWORK:+--network "$NETWORK"}
    ;;
  docker)
    command -v docker >/dev/null || { echo "docker not found" >&2; exit 2; }
    WORK="$OUT/work"; mkdir -p "$WORK"
    for s in $(sources); do c="$(src_field "$s" commit)"; mkdir -p "$WORK/src-${c:0:12}"
      git -C "$SRC" archive "$c" -- . "${SKIP[@]}" | tar -x -C "$WORK/src-${c:0:12}" \
        || { rm -rf "$WORK/src-${c:0:12}"; echo "cannot archive $c" >&2; exit 1; }; done
    docker build -t ferrata-reproduce -f "$HERE/Dockerfile" "$HERE" || { echo "docker build failed" >&2; exit 1; }
    docker run --rm -v "$SRC:/src:ro" -v "$OUT:/out" ferrata-reproduce bash -c \
      'rsync -a --exclude .git --exclude target/ --exclude /vault/ /src/ /work/ferrata/ && cd /work/ferrata/lez \
       && git -C /work/upstream/lez-v0.4.0-rc3 rev-parse HEAD >/dev/null \
       && bash /work/ferrata/scripts/reproduce/deployed.sh build --out /out --work /out/work'
    "$0" compare --out "$OUT" ${NETWORK:+--network "$NETWORK"}
    ;;
  *) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
