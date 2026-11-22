# Sourced by the scripts that write records (run-live.sh, hooks/crown.sh, hooks/gasdrop.sh).
#
#   scrub < log > record
#
# No path of the machine a run used, and no name of the folders it used, reaches a record. The
# folders the run is given become placeholders: its scratch (FERRATA_LIVE_TMP) <scratch>, the
# repository it runs from (REPO, whatever its copy is called) <repo>, LEZ's tree (LEZ_SRC) <lez>,
# the guest toolchain (RISC0_HOME) <risc0-home>, the home folder ~. Of the folder beside the
# repository (BESIDE), any other folder in its tmp/ (another scratch) becomes <tmp>, any other
# folder in it <beside>, and itself ... Each path is matched as given and as resolved, the longest
# first, so a scratch inside the repository or in tmp/ is still <scratch>.

_scrub_forms() {   # length <tab> order <tab> placeholder <tab> path
  local name path form order=0
  for name in '<scratch>' '<repo>' '<lez>' '<risc0-home>' '<tmp>' '<beside>' '..' '~'; do
    order=$((order + 1))
    case "$name" in
      '<scratch>') path="${FERRATA_LIVE_TMP:-}" ;;
      '<repo>') path="${REPO:-}" ;;
      '<lez>') path="${LEZ_SRC:-}" ;;
      '<risc0-home>') path="${RISC0_HOME:-}" ;;
      '<tmp>') path="${BESIDE:+$BESIDE/tmp}" ;;
      '<beside>' | ..) path="${BESIDE:-}" ;;
      *) path="${HOME:-}" ;;
    esac
    [ -n "$path" ] || continue
    for form in "$path" "$(cd "$path" 2>/dev/null && pwd)" "$(cd "$path" 2>/dev/null && pwd -P)"; do
      form="${form%/}"   # "/" itself is never replaced
      if [ -n "$form" ]; then printf '%s\t%s\t%s\t%s\n' "${#form}" "$order" "$name" "$form"; fi
    done
  done
}

scrub() {
  local -a rules=()
  local _len _order name form q
  local folder='[^/[:space:]"'\'',;:=)]\{1,\}'   # one folder name
  while IFS=$'\t' read -r _len _order name form; do
    q="$(printf '%s' "$form" | sed -e 's/[][\.*^$#]/\\&/g')"   # the path as a literal pattern
    case "$name" in
      '<tmp>' | '<beside>') rules+=(-e "s#$q/$folder#$name#g") ;;   # the folder in it, whatever its name
      *) rules+=(-e "s#$q/#$name/#g" -e "s#$q\$#$name#" -e "s#$q\([^/A-Za-z0-9._-]\)#$name\1#g") ;;
    esac
  done < <(_scrub_forms | sort -t "$(printf '\t')" -k1,1nr -k2,2n | awk -F '\t' '!seen[$3 FS $4]++')
  if [ "${#rules[@]}" = 0 ]; then cat; else sed "${rules[@]}"; fi
}
