#!/usr/bin/env bash
# Operate the lab (README.md): a Modal app `ferrata-lab` in the workspace of LAB_PROFILE.
#
#   lab.sh deploy        upload the bundle built by build.sh (if it changed) and deploy the app:
#                        the schedule starts (a session every two hours)
#   lab.sh run [MIN]     start one session now (MIN: the minutes it may submit jobs; default 20)
#   lab.sh status        both instances' finalized slots and periods against the chains' own,
#                        the sessions so far and their failures
#   lab.sh fetch         the record (manifest, block log, jobs, kept updates) and the sessions'
#                        logs into results/
#   lab.sh verify        offline: results/ replays (blocks, charges, both instances against the
#                        native light client); the driver runs on the build box (mrun) unless
#                        LAB_VERIFY_BIN names a local lab-driver
#   lab.sh stop          stop the app: no schedule, nothing running or billing (the Volume stays)
set -euo pipefail
LAB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$LAB_DIR/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
LAB_TMP="${LAB_TMP:-$BESIDE/tmp/lab}"
# LAB_PROFILE: the Modal profile of the lab's workspace; every step needs it but a local verify (LAB_VERIFY_BIN).
need_profile() { [ -n "${LAB_PROFILE:-}" ] || { echo "lab.sh: set LAB_PROFILE to the Modal profile of the lab's workspace" >&2; exit 2; }; }
case "${1:-}" in deploy|run|status|fetch|stop) need_profile ;; verify) [ -n "${LAB_VERIFY_BIN:-}" ] || need_profile ;; esac
export MODAL_PROFILE="${LAB_PROFILE:-}"
VOLUME=ferrata-lab
RESULTS="$LAB_DIR/results"
mkdir -p "$LAB_TMP/pull"

vol_get() { # remote local: replace local with the Volume's copy (a directory lands in local's
  # parent under the remote's name, which the callers keep equal to local's)
  rm -rf "$2"; mkdir -p "$(dirname "$2")"
  modal volume get --force "$VOLUME" "$1" "$(dirname "$2")/" > /dev/null
}

case "${1:-}" in
  deploy)
    [ -f "$LAB_TMP/bundle.tar.zst" ] || { echo "no bundle: build.sh all && build.sh pack on the build box first" >&2; exit 1; }
    modal volume create "$VOLUME" > /dev/null 2>&1 || true
    local_sum="$(sha256sum "$LAB_TMP/bundle.tar.zst" | cut -d' ' -f1)"
    remote_sum="$(vol_get /bundle.sha256 "$LAB_TMP/pull/bundle.sha256" 2>/dev/null && cut -d' ' -f1 "$LAB_TMP/pull/bundle.sha256" || true)"
    if [ "$local_sum" != "$remote_sum" ]; then
      modal volume put --force "$VOLUME" "$LAB_TMP/bundle.tar.zst" /bundle.tar.zst > /dev/null
      echo "$local_sum  bundle.tar.zst" > "$LAB_TMP/bundle.sha256"
      modal volume put --force "$VOLUME" "$LAB_TMP/bundle.sha256" /bundle.sha256 > /dev/null
      echo "bundle uploaded ($local_sum)"
    else
      echo "bundle unchanged ($local_sum)"
    fi
    ( cd "$LAB_DIR" && modal deploy app.py )
    ;;
  run)
    python3 -c "import modal, sys; c = modal.Function.from_name('ferrata-lab', 'session').spawn(deadline_min=int(sys.argv[1])); print('spawned', c.object_id)" "${2:-20}"
    ;;
  status)
    vol_get /records/sessions.tsv "$LAB_TMP/pull/sessions.tsv" || { echo "no sessions recorded yet"; exit 0; }
    vol_get /records/latest/instances.tsv "$LAB_TMP/pull/instances.tsv" || true
    vol_get /records/latest/jobs.tsv "$LAB_TMP/pull/jobs.tsv" || true
    python3 -I "$LAB_DIR/status.py" "$LAB_TMP/pull"
    modal app list 2>/dev/null | grep -E 'ferrata-lab|State' || true
    ;;
  fetch)
    mkdir -p "$RESULTS"
    vol_get /records/latest "$LAB_TMP/pull/latest"
    vol_get /records/sessions "$LAB_TMP/pull/sessions"
    vol_get /records/sessions.tsv "$LAB_TMP/pull/sessions.tsv"
    rm -rf "$RESULTS/record" "$RESULTS/sessions"
    cp -r "$LAB_TMP/pull/latest" "$RESULTS/record"
    cp -r "$LAB_TMP/pull/sessions" "$RESULTS/sessions"
    cp "$LAB_TMP/pull/sessions.tsv" "$RESULTS/sessions.tsv"
    # Sessions before 36f1ee3+1 kept raw copies of some logs: the same scrub as session.sh.
    for f in "$RESULTS"/sessions/*/sequencer.log; do [ -f "$f" ] && { sed -e 's#/root/lab/#lab/#g' "$f" | gzip -c > "$f.gz"; rm -f "$f"; }; done
    find "$RESULTS/sessions" -name '*.log' -exec sed -i -e 's#/root/lab/#lab/#g' {} +
    if grep -rl -E '/home/|/root/' "$RESULTS" > /dev/null 2>&1; then echo "warning: machine paths in results/:"; grep -rl -E '/home/|/root/' "$RESULTS"; fi
    echo "fetched into ${RESULTS#$REPO/}: $(wc -l < "$RESULTS/record/run.tsv") manifest lines, $(du -sh "$RESULTS" | cut -f1)"
    ;;
  verify)
    [ -d "$RESULTS/record" ] || { echo "fetch first" >&2; exit 1; }
    # The lab's program is this branch's of 11 Nov; A1 (1a452aea) later changed lez/wire's instance
    # layout, so the driver that reads the lab's instances is built from LAB_DRIVER_REV (the commit
    # whose verify of 11 Nov passed), in a worktree beside the repository, with this record.
    rev="${LAB_DRIVER_REV:-66f6bbbd}"
    if [ -z "${LAB_VERIFY_BIN:-}" ] && [ -z "${LAB_IN_DRIVER_SRC:-}" ] \
        && ! git -C "$REPO" diff --quiet "$rev" HEAD -- lez/wire lez/lab/driver; then
      src="$BESIDE/ferrata-lab-driver-src"
      [ -d "$src" ] || git -C "$REPO" worktree add -q --detach "$src" "$rev"
      rm -rf "$src/lez/lab/results"; cp -r "$RESULTS" "$src/lez/lab/results"; cp "$LAB_DIR/lab.sh" "$src/lez/lab/lab.sh"
      exec env LAB_IN_DRIVER_SRC=1 "$src/lez/lab/lab.sh" verify
    fi
    if [ -n "${LAB_VERIFY_BIN:-}" ]; then
      "$LAB_VERIFY_BIN" verify --dir "$RESULTS/record" --binary "${LAB_PROGRAM_BIN:-$LAB_TMP/program/ferrata_lez.bin}"
    else
      ( cd "$REPO" && "$BESIDE/modal-build/mrun" --profile "$MODAL_PROFILE" --put "$LAB_TMP/program/ferrata_lez.bin" -- \
          "(apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq libpcsclite-dev pkg-config) > /dev/null 2>&1; mkdir -p ../tmp/lab/logs && lez/lab/build.sh driver > ../tmp/lab/logs/verify-build.log 2>&1 || { tail -n 20 ../tmp/lab/logs/verify-build.log; exit 1; }; ../tmp/lab/live/bin/lab-driver verify --dir lez/lab/results/record --binary ../tmp/lab/program/ferrata_lez.bin" )
    fi
    ;;
  stop)
    modal app stop ferrata-lab --yes 2>/dev/null || modal app stop ferrata-lab
    modal app list 2>/dev/null | grep -E 'ferrata-lab|State' || true
    ;;
  *) sed -n '2,17p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
