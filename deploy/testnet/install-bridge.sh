#!/usr/bin/env bash
# The Sepolia bridge demo on this host, installed and left stopped (README: "The Sepolia bridge demo"). As root:
#
#   install-bridge.sh BIN_DIR PROGRAMS_31_DIR VAULT
#
# On LEZ v0.4.0 (FERRATA_LEZ_TAG=v0.4.0) none of the rc3 pins below is used: FERRATA_BRIDGE_PINS names a sha256sum file
# listing each of the five binaries, release.bin and the rule (B) program FERRATA_BRIDGE_CHECKPOINT_B exactly once;
# FERRATA_CHECKPOINT_IMAGE_B is that program's image id and FERRATA_RELEASE_IMAGE_ID the release program's (it may be
# left out when the pinned release.bin is rc3's own, whose id is known); the bridge's prover client is given those
# programs (/etc/ferrata/bridge-prover-programs.json and a drop-in of ferrata-bridge-prover.service).
# FERRATA_CHECKPOINT_IMAGE_A is the v0.4.0 rule (A) program's image id, which ferrata-bridge-configure (run at the end)
# writes into the vault's parameters with the other two. All are checked before anything is installed.
#     BIN_DIR           ferrata-relayer (cli/), monitor (monitor/), crown-driver (lez/crown/driver/), realblocks-crown
#                       and ferrata_attestor (checkpoint/), built as README "Artifacts" says; each is checked against
#                       the sha256 pinned below, and refused otherwise
#     PROGRAMS_31_DIR   release.bin and checkpoint_b_0_3_5.bin of docs/image-ids.md's RISC Zero 3.1 rows
#     VAULT             the vault address the deploy tool's --dry predicts
#
# Then the owner funds the two addresses, deploys the vault with /var/lib/ferrata/bridge/params.json, and one command
# starts everything: ferrata-bridge-start --vault VAULT. The prover client also needs the Modal token of a workspace
# without secrets in /etc/ferrata/bridge-prover.env (MODAL_TOKEN_ID=..., MODAL_TOKEN_SECRET=...; root, mode 600).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lez-tag.sh"
BIN="${1:?bin dir}"; P31="${2:?programs dir}"; VAULT="${3:?vault address}"
OPT=/opt/ferrata; B=/var/lib/ferrata/bridge

# On v0.4.0 the operator supplies both artifact sets' sha256sums: none of rc3's pins is a fallback. Validate the whole
# file before checking anything, including the v0.4.0 rule (B) filename, so a partial or ambiguous pins file is refused.
BRIDGE_CHECKPOINT_B="${FERRATA_BRIDGE_CHECKPOINT_B-}"
bridge_check_v040_pins() {
  local pins="${FERRATA_BRIDGE_PINS-}"
  if [ -z "$pins" ] || [ ! -f "$pins" ] || [ ! -r "$pins" ]; then
    echo "install-bridge.sh: FERRATA_BRIDGE_PINS must name a readable sha256sum pins file on v0.4.0" >&2
    return 2
  fi
  if [ -z "$BRIDGE_CHECKPOINT_B" ] || [[ "$BRIDGE_CHECKPOINT_B" == */* ]] ||
     [ "$BRIDGE_CHECKPOINT_B" = . ] || [ "$BRIDGE_CHECKPOINT_B" = .. ] ||
     [ "$BRIDGE_CHECKPOINT_B" = release.bin ]; then
    echo "install-bridge.sh: FERRATA_BRIDGE_CHECKPOINT_B must name the v0.4.0 rule (B) program file" >&2
    return 2
  fi
  local names=(ferrata-relayer monitor crown-driver realblocks-crown ferrata_attestor release.bin "$BRIDGE_CHECKPOINT_B")
  local line name
  local -A allowed=() counts=() sums=()
  for name in "${names[@]}"; do allowed["$name"]=1; counts["$name"]=0; done
  while IFS= read -r line || [ -n "$line" ]; do
    if ! [[ "$line" =~ ^[0-9a-fA-F]{64}\ [\ \*]([^/]+)$ ]]; then
      echo "install-bridge.sh: $pins is not in sha256sum format: $line" >&2
      return 2
    fi
    name="${BASH_REMATCH[1]}"
    if [ "${allowed[$name]+present}" != present ]; then
      echo "install-bridge.sh: $pins names unexpected artifact '$name'" >&2
      return 2
    fi
    sums["$name"]="$line"
    counts["$name"]=$((counts["$name"] + 1))
  done < "$pins"
  for name in "${names[@]}"; do
    if [ "${counts[$name]}" -ne 1 ]; then
      echo "install-bridge.sh: $pins must name '$name' exactly once" >&2
      return 2
    fi
  done
  # the prover's programs (written after the install below): the rule (B) program's image id, and the release's
  CKB_ID="${FERRATA_CHECKPOINT_IMAGE_B-}"; REL_ID="${FERRATA_RELEASE_IMAGE_ID-}"
  CKB_SHA="${sums[$BRIDGE_CHECKPOINT_B]%% *}"; REL_SHA="${sums[release.bin]%% *}"
  [[ "$CKB_ID" =~ ^0x[0-9a-f]{64}$ ]] \
    || { echo "install-bridge.sh: FERRATA_CHECKPOINT_IMAGE_B (0x + 64 hex) must name the v0.4.0 rule (B) program's image id" >&2; return 2; }
  # ferrata-bridge-configure, at the end, needs rule (A)'s id too: refused here, before anything is installed
  [[ "${FERRATA_CHECKPOINT_IMAGE_A-}" =~ ^0x[0-9a-f]{64}$ ]] \
    || { echo "install-bridge.sh: FERRATA_CHECKPOINT_IMAGE_A (0x + 64 hex) must name the v0.4.0 rule (A) program's image id" >&2; return 2; }
  if [ -z "$REL_ID" ]; then
    [ "${REL_SHA,,}" = d0fdcac9fa71ee87d22014ccfbb90f4ac543ae2559838d85a9dd0315c7cae2ac ] \
      || { echo "install-bridge.sh: release.bin is not rc3's: FERRATA_RELEASE_IMAGE_ID must name its image id" >&2; return 2; }
    REL_ID=0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd
  fi
  [[ "$REL_ID" =~ ^0x[0-9a-f]{64}$ ]] || { echo "install-bridge.sh: FERRATA_RELEASE_IMAGE_ID must be 0x + 64 hex" >&2; return 2; }
  ( cd "$BIN" && printf '%s\n' "${sums[ferrata-relayer]}" "${sums[monitor]}" "${sums[crown-driver]}" \
      "${sums[realblocks-crown]}" "${sums[ferrata_attestor]}" | sha256sum -c - )
  ( cd "$P31" && printf '%s\n' "${sums[release.bin]}" "${sums[$BRIDGE_CHECKPOINT_B]}" | sha256sum -c - )
}

# bridge_prover_programs FILE: the prover client's program table for v0.4.0 (after bridge_check_v040_pins)
bridge_prover_programs() {
  python3 -I - "$1" "$REL_ID" "$REL_SHA" "$BRIDGE_CHECKPOINT_B" "$CKB_ID" "$CKB_SHA" <<'PY'
import json, sys
out, rel_id, rel_sha, ckb, ckb_id, ckb_sha = sys.argv[1:7]
table = {"release": ["release.bin", rel_id, rel_sha.lower(), "release"], "checkpoint-b": [ckb, ckb_id, ckb_sha.lower(), "checkpoint"]}
with open(out, "w") as f:
    json.dump(table, f, indent=1)
    f.write("\n")
PY
  chmod 644 "$1"
}

# The binaries, each the build README "Artifacts" names (its sha256 here, not a file shipped beside it).
if [ "$LEZ_TAG" = v0.4.0 ]; then
  bridge_check_v040_pins
elif [ "$LEZ_TAG" = v0.4.0-rc3 ]; then
( cd "$BIN" && sha256sum -c - <<'SUMS'
09de92f46313ba94d8fc3f84757073aece7aae51087f4dcd9e6282e5dc8b6247  ferrata-relayer
b32d783e776a819cc618e561523e55eeb2fe79c6323ee339d1d52039413e5e37  monitor
ee4d23de648234a900ba03ad29e001f7b097c42c2e8591a46d5c1a306bd28aee  crown-driver
9bfa1a4f1cf7d114867009f8ec03b12b7b3d9940b8fbeb98a09b7bcb5011f505  realblocks-crown
d3fde012c4643dae7fa59623ee15b363d9f47f841785645677ae102cf80717aa  ferrata_attestor
SUMS
)
fi
install -m 755 "$BIN/ferrata-relayer" "$BIN/monitor" "$BIN/crown-driver" "$BIN/realblocks-crown" "$BIN/ferrata_attestor" "$OPT/bin/"
# Foundry's cast (ferrata-bridge-keys makes the keystores with it): release v1.3.6, the tarball's sha256 pinned.
if [ "$(sha256sum "$OPT/bin/cast" 2>/dev/null | cut -d' ' -f1)" != 61e4f27e1ede14ce99763e1a63d1b4071af7c0c09c789e3cbf5d930707ac3335 ]; then
  t="$(mktemp -d "$OPT/foundry.XXXX")"
  curl -fsSL -o "$t/f.tgz" https://github.com/foundry-rs/foundry/releases/download/v1.3.6/foundry_v1.3.6_linux_amd64.tar.gz
  echo "01248d7ce267f5b7958bc9b331d068c6c2d8eeb182e58f0b556d7295f398e2ce  $t/f.tgz" | sha256sum -c -
  tar -xzf "$t/f.tgz" -C "$t" cast && install -m 755 "$t/cast" "$OPT/bin/cast"; rm -rf "$t"
fi
install -d -m 755 "$OPT/programs/31"
if [ "$LEZ_TAG" = v0.4.0 ]; then
  install -m 644 "$P31/release.bin" "$P31/$BRIDGE_CHECKPOINT_B" "$OPT/programs/31/"
  # The prover client proves with the programs just installed, not its built-in rc3 table (ferrata-bridge-prover reads
  # FERRATA_PROVER_PROGRAMS_JSON: kind -> [file, image id, sha256, journal kind])
  bridge_prover_programs /etc/ferrata/bridge-prover-programs.json
  install -d -m 755 /etc/systemd/system/ferrata-bridge-prover.service.d
  printf '[Service]\nEnvironment=FERRATA_PROVER_PROGRAMS_JSON=/etc/ferrata/bridge-prover-programs.json\n' \
    > /etc/systemd/system/ferrata-bridge-prover.service.d/lez-v040.conf
else
for f in release.bin:d0fdcac9fa71ee87d22014ccfbb90f4ac543ae2559838d85a9dd0315c7cae2ac \
         checkpoint_b_0_3_5.bin:86eaa5502bffbb85d8661e18d24ae83a797cab5e2cdc4db70c01b6a7ae20bbdc; do
  echo "${f#*:}  $P31/${f%%:*}" | sha256sum -c - > /dev/null
  install -m 644 "$P31/${f%%:*}" "$OPT/programs/31/"
done
fi
install -m 755 "$HERE"/host/bin/ferrata-bridge-* "$OPT/bin/"
# Users and directories: each service sees only its own directory in its mount namespace; the prover client also
# the relayer's inbox, which lives inside the relayer's directory (one mount with its state: the relayer moves jobs
# from the inbox into its state with rename(2)).
bash "$OPT/bin/ferrata-bridge-keys" > /dev/null
getent group ferrata-bridge >/dev/null || groupadd --system ferrata-bridge
usermod -a -G ferrata-bridge ferrata-relayer; usermod -a -G ferrata-bridge ferrata-prover
install -d -o ferrata-relayer -g ferrata-relayer -m 700 "$B/relayer/state"
install -d -o ferrata-relayer -g ferrata-bridge -m 2770 "$B/relayer/inbox"
install -d -o ferrata-prover -g ferrata-prover -m 700 "$B/prover" "$B/prover/requests" "$B/prover/inputs"
install -d -o ferrata -g ferrata -m 700 "$B/lez-import"   # ferrata-bridge-import.service runs as ferrata
# The prover client's Modal SDK, in its own environment.
if [ ! -x "$OPT/venv-modal/bin/python3" ]; then python3 -m venv "$OPT/venv-modal"; fi
# modal 1.5.1 and its dependencies, each wheel's sha256 pinned (host/modal-requirements.txt).
"$OPT/venv-modal/bin/pip" install --quiet --disable-pip-version-check --require-hashes --only-binary=:all: \
  -r "$HERE/host/modal-requirements.txt"
install -m 644 "$HERE"/host/systemd/ferrata-bridge-*.service /etc/systemd/system/
systemctl daemon-reload
# The watcher's user exists now: let it read the node's API (its node view), with the other allowed users.
"$OPT/bin/ferrata-node-api-guard" > /dev/null
# The configuration for VAULT and the deploy's params (reads only).
bash "$OPT/bin/ferrata-bridge-configure" --vault "$VAULT" > /dev/null
echo "== installed, stopped: $(systemctl is-enabled ferrata-bridge-relayer ferrata-bridge-monitor ferrata-bridge-prover | tr '\n' ' ')"
cat "$B/addresses.json" "$B/params.json"
