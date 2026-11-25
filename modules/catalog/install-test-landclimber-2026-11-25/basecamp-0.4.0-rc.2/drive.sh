#!/bin/bash
# bc-drive.sh NAME ACTIONS_FILE: one Logos Basecamp 0.4.0-rc.2 session (the release AppImage, as a tester runs it) on a
# virtual screen, driven by ACTIONS_FILE, as the earlier Basecamp desktop recording's session scripts (not in the repository) did: Xvfb
# 1920x1080, openbox, a D-Bus session, the extracted AppImage's AppRun (no FUSE in the box), a fresh home.
# Actions, one per line: click X Y | dclick X Y | sleep S | shot NAME | type TEXT | key KEYS | mark TEXT
# Output: OUT=<scratch>/bc/NAME (the script as run from <scratch>/box/) (shots/, actions.tsv, session.log, basecamp.out, logs/, datadir.txt).
set -u
NAME=$1; ACTIONS=$2; DN=${DN:-97}
here=$(cd "$(dirname "$0")" && pwd); S=$(cd "$here/.." && pwd)
OUT=$S/bc/$NAME; rm -rf "$OUT"; mkdir -p "$OUT/shots"
exec > >(tee "$OUT/session.log") 2>&1
B=/var/tmp/bc; mkdir -p "$B"
URL=https://github.com/logos-co/logos-basecamp/releases/download/pre-release-4cea4bb-1433/logos-basecamp-x86_64.AppImage
SUM=e59e14cddd211bed763c49e6a9bdfea462b91123b368386bdb8f700527b747a0
if [ ! -x "$B/rc/squashfs-root/AppRun" ]; then
  t0=$(date +%s)
  curl -fsSL -o "$B/rc.AppImage" "$URL" && echo "$SUM  $B/rc.AppImage" | sha256sum -c || { echo "AppImage download/check failed"; exit 1; }
  chmod +x "$B/rc.AppImage" && rm -rf "$B/rc" && mkdir -p "$B/rc" && (cd "$B/rc" && "$B/rc.AppImage" --appimage-extract > /dev/null)
  echo "AppImage fetched, checked and extracted in $(( $(date +%s) - t0 )) s"
else
  echo "AppImage already extracted (this container)"
fi
echo "AppImage: $(stat -c %s "$B/rc.AppImage" 2>/dev/null) bytes, sha256 $(sha256sum "$B/rc.AppImage" | cut -d' ' -f1)"
H=$B/home-$NAME; rm -rf "$H" "$B/t-$NAME"; mkdir -p "$H"
export HOME=$H XDG_RUNTIME_DIR=$H/run LANG=C.UTF-8 DISPLAY=:$DN TMPDIR=$B/t-$NAME
mkdir -p "$TMPDIR" "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
echo "start $(date -u +%FT%TZ) display=$DISPLAY"
Xvfb :$DN -screen 0 1920x1080x24 -nolisten tcp > "$OUT/xvfb.log" 2>&1 &
XV=$!
for i in $(seq 50); do xdpyinfo > /dev/null 2>&1 && break; sleep 0.2; done
eval "$(dbus-launch --sh-syntax)"
openbox > "$OUT/openbox.log" 2>&1 &
sleep 1; xsetroot -solid '#3b4252' 2> /dev/null || true
cd "$HOME"
T0=$(date +%s.%N)
"$B/rc/squashfs-root/AppRun" > "$OUT/basecamp.out" 2>&1 &
BC=$!
for i in $(seq 240); do
  xdotool search --onlyvisible --name 'Basecamp|Logos' > /dev/null 2>&1 && { echo "window after $(echo "$(date +%s.%N) - $T0" | bc) s"; break; }
  kill -0 $BC 2> /dev/null || { echo "Basecamp exited before a window"; break; }
  sleep 0.5
done
for w in $(xdotool search --onlyvisible --name 'Basecamp|Logos' 2> /dev/null); do echo "win $w: $(xdotool getwindowname $w) $(xdotool getwindowgeometry $w | tr '\n' ' ')"; done
sleep 4
last=$(date +%s.%N)
: > "$OUT/actions.tsv"
mark() { local now; now=$(date +%s.%N); printf '%s\t%s\t%.1f\n' "$(date -u +%TZ)" "$1" "$(echo "$now - $T0" | bc)" >> "$OUT/actions.tsv"; }
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in ''|'#'*) continue ;; esac
  set -- $line; a=$1; shift
  case "$a" in
    click) xdotool mousemove "$1" "$2" click 1 ;;
    dclick) xdotool mousemove "$1" "$2" click --repeat 2 --delay 120 1 ;;
    sleep) sleep "$1" ;;
    shot) import -window root "$OUT/shots/$1.png" ;;
    type) xdotool type --delay 25 "$*" ;;
    key) xdotool key "$@" ;;
    mark) : ;;
    *) echo "unknown action: $line" ;;
  esac
  mark "$line"
  kill -0 $BC 2> /dev/null || { echo "Basecamp exited at $(date -u +%TZ) during: $line"; break; }
done < "$ACTIONS"
echo "actions done $(date -u +%FT%TZ)"
D="$H/.local/share/Logos/LogosBasecamp"
{ echo "== $D"; ls -la "$D" 2>&1; for s in modules plugins keyring module_data logs; do echo "== $s"; ls -la "$D/$s" 2>&1 | head -40; done
  echo "== config files"; find "$H/.config" "$D" -maxdepth 2 -type f \( -name '*.json' -o -name '*.conf' -o -name '*.ini' -o -name '*.yaml' \) 2> /dev/null | head -20; } > "$OUT/datadir.txt"
find "$D/keyring" -maxdepth 2 -type f -exec sh -c 'echo "--- $1"; head -c 2000 "$1"; echo' _ {} \; >> "$OUT/datadir.txt" 2> /dev/null
for f in $(find "$H/.config" "$D" -maxdepth 2 -type f \( -name '*.json' -o -name '*.conf' -o -name '*.ini' \) 2> /dev/null | head -10); do echo "--- $f"; head -c 3000 "$f"; echo; done >> "$OUT/datadir.txt"
{ echo "name\tversion\ttype\thashes.root\tdir"
  for f in "$D"/modules/*/manifest.json "$D"/plugins/*/manifest.json; do
    [ -f "$f" ] && jq -r --arg d "$(basename "$(dirname "$(dirname "$f")")")/$(basename "$(dirname "$f")")" '[.name, .version, .type, (.hashes.root // "-"), $d] | @tsv' "$f"
  done; } > "$OUT/installed.tsv" 2>&1
mkdir -p "$OUT/logs" && cp -L "$D"/logs/*.log "$OUT/logs/" 2> /dev/null
kill $BC 2> /dev/null; sleep 3; kill -9 $BC 2> /dev/null; kill $XV 2> /dev/null
echo "done $(date -u +%FT%TZ)"
