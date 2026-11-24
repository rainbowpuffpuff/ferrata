#!/usr/bin/env bash
# The Sepolia bridge demo on this host, installed and left stopped (README: "The Sepolia bridge demo"). As root:
#
#   install-bridge.sh BIN_DIR PROGRAMS_31_DIR VAULT
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
BIN="${1:?bin dir}"; P31="${2:?programs dir}"; VAULT="${3:?vault address}"
OPT=/opt/ferrata; B=/var/lib/ferrata/bridge
# The binaries, each the build README "Artifacts" names (its sha256 here, not a file shipped beside it).
( cd "$BIN" && sha256sum -c - <<'SUMS'
09de92f46313ba94d8fc3f84757073aece7aae51087f4dcd9e6282e5dc8b6247  ferrata-relayer
b32d783e776a819cc618e561523e55eeb2fe79c6323ee339d1d52039413e5e37  monitor
ee4d23de648234a900ba03ad29e001f7b097c42c2e8591a46d5c1a306bd28aee  crown-driver
9bfa1a4f1cf7d114867009f8ec03b12b7b3d9940b8fbeb98a09b7bcb5011f505  realblocks-crown
d3fde012c4643dae7fa59623ee15b363d9f47f841785645677ae102cf80717aa  ferrata_attestor
SUMS
)
install -m 755 "$BIN/ferrata-relayer" "$BIN/monitor" "$BIN/crown-driver" "$BIN/realblocks-crown" "$BIN/ferrata_attestor" "$OPT/bin/"
# Foundry's cast (ferrata-bridge-keys makes the keystores with it): release v1.3.6, the tarball's sha256 pinned.
if [ "$(sha256sum "$OPT/bin/cast" 2>/dev/null | cut -d' ' -f1)" != 61e4f27e1ede14ce99763e1a63d1b4071af7c0c09c789e3cbf5d930707ac3335 ]; then
  t="$(mktemp -d "$OPT/foundry.XXXX")"
  curl -fsSL -o "$t/f.tgz" https://github.com/foundry-rs/foundry/releases/download/v1.3.6/foundry_v1.3.6_linux_amd64.tar.gz
  echo "01248d7ce267f5b7958bc9b331d068c6c2d8eeb182e58f0b556d7295f398e2ce  $t/f.tgz" | sha256sum -c -
  tar -xzf "$t/f.tgz" -C "$t" cast && install -m 755 "$t/cast" "$OPT/bin/cast"; rm -rf "$t"
fi
install -d -m 755 "$OPT/programs/31"
for f in release.bin:d0fdcac9fa71ee87d22014ccfbb90f4ac543ae2559838d85a9dd0315c7cae2ac \
         checkpoint_b_0_3_5.bin:86eaa5502bffbb85d8661e18d24ae83a797cab5e2cdc4db70c01b6a7ae20bbdc; do
  echo "${f#*:}  $P31/${f%%:*}" | sha256sum -c - > /dev/null
  install -m 644 "$P31/${f%%:*}" "$OPT/programs/31/"
done
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
