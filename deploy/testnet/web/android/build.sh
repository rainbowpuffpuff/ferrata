#!/usr/bin/env bash
# The wallet's Android app: a Trusted Web Activity, that is /wallet/ run by Chrome's engine full screen, with Android
# checking through Digital Asset Links that the app and the site belong together. Built on a Linux build machine (the
# Modal build box through mrun); nothing heavy runs on a laptop.
#
#   HOST=<the network's host> COMMIT=<commit> bash deploy/testnet/web/android/build.sh
#
# 1. Android SDK: Google's command-line tools (the archive's SHA-1 as Google's repository2-3.xml lists it), platform 36,
#    build tools 35.0.0.
# 2. Bubblewrap's generator (@bubblewrap/core, npm ci of package-lock.json); this repository's icons served on 127.0.0.1.
# 3. The project generated twice, in two directories, and built twice (./gradlew assembleRelease): the two unsigned APKs
#    must be equal byte for byte.
# 4. The release key from KEYDIR (made offline and brought in for the build; MAKE_KEY=1 makes one here, RSA 4096,
#    PKCS#12, a random password; never in the repository). zipalign, apksigner sign (v1-v3), apksigner verify.
# 5. OUT: ferrata-wallet-<version>.apk, apk.json (size, SHA-256, signing certificate, permissions, SDK levels),
#    assetlinks.json (for https://HOST/.well-known/), SHA256SUMS, badging.txt, the generated manifests for review.
#
# Scratch (ANDROID_WORK, default ferrata-scratch/android beside the repository): sdk/, gradle-home/, key/, out/.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICONS_DIR="$HERE/../wallet/icons"
BESIDE="$(cd "$HERE/../../../../.." && pwd)"
: "${HOST:?set HOST to the host name of the network}"
COMMIT="${COMMIT:-unknown}"
SCR="${ANDROID_WORK:-$BESIDE/ferrata-scratch/android}"
export ANDROID_HOME="${ANDROID_HOME:-$SCR/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$SCR/gradle-home}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-17-openjdk-amd64}"
KEYDIR="${KEYDIR:-$SCR/key}"
OUT="${OUT:-$SCR/out}"
CLT=commandlinetools-linux-16111833_latest.zip
CLT_SHA1=e025545c62a8e64c7559119566a569fb1dec5f60
PLATFORM="platforms;android-36"
BUILD_TOOLS=35.0.0
BT="$ANDROID_HOME/build-tools/$BUILD_TOOLS"
PORT="${ICON_PORT:-18765}"
mkdir -p "$SCR" "$OUT"
log() { echo "[android] $*"; }

sdk() {
  if [ ! -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ]; then
    mkdir -p "$ANDROID_HOME/cmdline-tools" "$SCR/dl"
    [ -f "$SCR/dl/$CLT" ] || curl -fsSL -o "$SCR/dl/$CLT" "https://dl.google.com/android/repository/$CLT"
    echo "$CLT_SHA1  $SCR/dl/$CLT" | sha1sum -c - > /dev/null
    log "command-line tools: sha1 ok, sha256 $(sha256sum "$SCR/dl/$CLT" | cut -c1-64)"
    rm -rf "$SCR/dl/x" && unzip -q "$SCR/dl/$CLT" -d "$SCR/dl/x" && mv "$SCR/dl/x/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
  fi
  local sm="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
  (yes || true) | "$sm" --licenses > "$SCR/sdk-licenses.log" 2>&1 || true
  "$sm" "$PLATFORM" "build-tools;$BUILD_TOOLS" > "$SCR/sdk-install.log" 2>&1
  log "sdk: $PLATFORM, build-tools $BUILD_TOOLS"
}

generator() {
  rm -rf "$SCR/gen" && mkdir -p "$SCR/gen"
  cp "$HERE/package.json" "$HERE/gen.mjs" "$HERE/twa.json" "$SCR/gen/"
  if [ -f "$HERE/package-lock.json" ]; then
    cp "$HERE/package-lock.json" "$SCR/gen/"
    (cd "$SCR/gen" && npm ci --ignore-scripts --no-audit --no-fund) > "$SCR/npm.log" 2>&1
  else
    (cd "$SCR/gen" && npm install --ignore-scripts --no-audit --no-fund) > "$SCR/npm.log" 2>&1
    cp "$SCR/gen/package-lock.json" "$HERE/package-lock.json"
    log "package-lock.json written (commit it)"
  fi
  log "generator: @bubblewrap/core $(node -p "require('$SCR/gen/node_modules/@bubblewrap/core/package.json').version")"
}

build_one() { # N
  local P="$SCR/project-$1"
  rm -rf "$P"
  (cd "$SCR/gen" && HOST="$HOST" ICONS="http://127.0.0.1:$PORT" node gen.mjs "$P") > "$SCR/gen-$1.log" 2>&1 \
    || { tail -30 "$SCR/gen-$1.log"; exit 3; }
  chmod +x "$P/gradlew"
  (cd "$P" && ./gradlew --no-daemon --console=plain assembleRelease) > "$SCR/gradle-$1.log" 2>&1 \
    || { grep -E 'error|FAILED|What went wrong' -A5 "$SCR/gradle-$1.log" | head -40; exit 4; }
  cp "$P/app/build/outputs/apk/release/app-release-unsigned.apk" "$SCR/unsigned-$1.apk"
  log "build $1: unsigned $(sha256sum "$SCR/unsigned-$1.apk" | cut -c1-64) $(stat -c %s "$SCR/unsigned-$1.apk") bytes"
}

key() {
  if [ ! -f "$KEYDIR/release.p12" ]; then
    # Whoever can read KEYDIR can sign an update every installed copy accepts. Bring a key made offline into KEYDIR for
    # the build (DROP_KEY=1 removes that copy at the end), or set MAKE_KEY=1 to make one here knowingly (README: Signing).
    [ "${MAKE_KEY:-0}" = 1 ] || { echo "error: no release key in KEYDIR ($KEYDIR); see README, Signing (or MAKE_KEY=1)" >&2; exit 5; }
    mkdir -p "$KEYDIR" && chmod 700 "$KEYDIR"
    head -c 30 /dev/urandom | base64 > "$KEYDIR/password" && chmod 600 "$KEYDIR/password"
    "$JAVA_HOME/bin/keytool" -genkeypair -keystore "$KEYDIR/release.p12" -storetype PKCS12 -alias ferrata-wallet \
      -keyalg RSA -keysize 4096 -sigalg SHA256withRSA -validity 10000 \
      -dname "CN=Ferrata wallet (LEZ test network), O=Ferrata" \
      -storepass:file "$KEYDIR/password" -keypass:file "$KEYDIR/password" > "$SCR/keytool.log" 2>&1
    chmod 600 "$KEYDIR/release.p12"
    log "release key made in $KEYDIR"
  fi
}

sdk
generator
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$ICONS_DIR" > "$SCR/icons-server.log" 2>&1 &
ICONS_PID=$!
trap 'kill $ICONS_PID 2>/dev/null || true' EXIT
sleep 1
build_one 1
build_one 2
U1=$(sha256sum "$SCR/unsigned-1.apk" | cut -c1-64); U2=$(sha256sum "$SCR/unsigned-2.apk" | cut -c1-64)
[ "$U1" = "$U2" ] && REPRO=true || REPRO=false
log "two builds equal: $REPRO"
key
VERSION=$(node -p "require('$HERE/twa.json').appVersion")
APK="ferrata-wallet-$VERSION.apk"
"$BT/zipalign" -f -p 4 "$SCR/unsigned-1.apk" "$SCR/aligned.apk"
"$BT/apksigner" sign --ks "$KEYDIR/release.p12" --ks-type PKCS12 --ks-key-alias ferrata-wallet \
  --ks-pass "file:$KEYDIR/password" --v4-signing-enabled false \
  --out "$OUT/$APK" "$SCR/aligned.apk"
"$BT/apksigner" verify --verbose --print-certs "$OUT/$APK" > "$OUT/apksigner-verify.txt"
"$BT/aapt2" dump badging "$OUT/$APK" > "$OUT/badging.txt"
(cd "$OUT" && sha256sum "$APK" > SHA256SUMS)
mkdir -p "$OUT/review"
cp "$SCR/project-1/twa-manifest.json" "$SCR/project-1/app/src/main/AndroidManifest.xml" "$SCR/project-1/app/build.gradle" "$OUT/review/"
python3 -I - "$OUT" "$APK" "$REPRO" "$U1" "$COMMIT" "$HERE/twa.json" <<'EOF'
import datetime, hashlib, json, os, re, sys
out, apk, repro, unsigned, commit, twa_path = sys.argv[1:7]
twa = json.load(open(twa_path))
v = open(os.path.join(out, "apksigner-verify.txt")).read()
b = open(os.path.join(out, "badging.txt")).read()
fp = re.search(r"Signer #1 certificate SHA-256 digest: ([0-9a-f]{64})", v).group(1).upper()
fp = ":".join(fp[i:i + 2] for i in range(0, 64, 2))
data = open(os.path.join(out, apk), "rb").read()
schemes = {k: (re.search(rf"Verified using {k} scheme \([^)]*\): (true|false)", v) or [None, None])[1] for k in ("v1", "v2", "v3")}
info = {
    "file": apk, "size": len(data), "sha256": hashlib.sha256(data).hexdigest(), "certSha256": fp,
    "package": re.search(r"package: name='([^']+)'", b).group(1),
    "versionName": re.search(r"versionName='([^']+)'", b).group(1), "versionCode": int(re.search(r"versionCode='(\d+)'", b).group(1)),
    "minSdk": int(re.search(r"(?:minSdkVersion|sdkVersion):'(\d+)'", b).group(1)), "targetSdk": int(re.search(r"targetSdkVersion:'(\d+)'", b).group(1)),
    "permissions": sorted(set(re.findall(r"uses-permission: name='([^']+)'", b))),
    "signatureSchemes": schemes, "unsignedSha256": unsigned, "reproducible": repro == "true",
    "startUrl": twa["startUrl"], "fallback": twa["fallbackType"], "generator": twa["generatorApp"], "commit": commit,
    "built": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
json.dump(info, open(os.path.join(out, "apk.json"), "w"), indent=2)
links = [{"relation": ["delegate_permission/common.handle_all_urls"],
          "target": {"namespace": "android_app", "package_name": info["package"], "sha256_cert_fingerprints": [fp]}}]
json.dump(links, open(os.path.join(out, "assetlinks.json"), "w"), indent=2)
print(json.dumps(info, indent=1))
EOF
if [ "${DROP_KEY:-0}" = 1 ]; then rm -f "$KEYDIR/release.p12" "$KEYDIR/password"; log "release key copy removed from $KEYDIR"; fi
