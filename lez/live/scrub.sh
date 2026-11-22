# Sourced by the scripts that write records (run-live.sh, hooks/crown.sh, hooks/gasdrop.sh).
#
#   scrub < log > record
#
# No path of the machine a run used, and no name of the folders it used, reaches a record: the
# run's scratch (FERRATA_LIVE_TMP, whatever it is called) becomes <scratch>, the repository the run
# is from (REPO, whatever its copy is called) <repo>, any other folder in tmp/ beside the repository
# (another scratch) <tmp>, the rest of the folder beside the repository (BESIDE: LEZ's tree in
# upstream/, the guest toolchain of ../RESULTS.md §8) ../, and the home folder ~. Each path is
# matched as given and as resolved, the longest first, so a scratch inside the repository or in tmp/
# is still <scratch>.

_scrub_forms() {   # length <tab> placeholder <tab> path
  local name path form
  for name in '<scratch>' '<repo>' '<tmp>' '..' '~'; do
    case "$name" in
      '<scratch>') path="${FERRATA_LIVE_TMP:-}" ;;
      '<repo>') path="${REPO:-}" ;;
      '<tmp>') path="${BESIDE:+$BESIDE/tmp}" ;;
      ..) path="${BESIDE:-}" ;;
      *) path="${HOME:-}" ;;
    esac
    [ -n "$path" ] || continue
    for form in "$path" "$(cd "$path" 2>/dev/null && pwd)" "$(cd "$path" 2>/dev/null && pwd -P)"; do
      form="${form%/}"   # "/" itself is never replaced
      if [ -n "$form" ]; then printf '%s\t%s\t%s\n' "${#form}" "$name" "$form"; fi
    done
  done
}

scrub() {
  local -a rules=()
  local _len name form q
  while IFS=$'\t' read -r _len name form; do
    q="$(printf '%s' "$form" | sed -e 's/[][\.*^$#]/\\&/g')"   # the path as a literal pattern
    if [ "$name" = '<tmp>' ]; then   # tmp/<any folder>
      rules+=(-e "s#$q/[^/[:space:]\"',;:=)]\{1,\}#<tmp>#g")
    else
      rules+=(-e "s#$q/#$name/#g" -e "s#$q\$#$name#" -e "s#$q\([^/A-Za-z0-9._-]\)#$name\1#g")
    fi
  done < <(_scrub_forms | sort -t "$(printf '\t')" -k1,1nr | awk -F '\t' '!seen[$3]++')
  if [ "${#rules[@]}" = 0 ]; then cat; else sed "${rules[@]}"; fi
}
