#!/usr/bin/env bash
# Provision the Ferrata testnet host (Ubuntu 24.04, run as root over SSM: deploy.sh). Idempotent; run in stages:
#
#   provision.sh base VOLUME_ID   packages, chrony on Amazon Time Sync, the data volume on /var/lib/ferrata,
#                                 the ferrata user, AWS CLI v2, CloudWatch agent, Caddy
#   provision.sh toolchain        rustup (1.98.1, 1.93.0, wasm32), rzup 0.5.2 + RISC Zero 3.0.5 (scripts/ci/install-risc0.sh),
#                                 cargo-leptos
#   provision.sh fetch BUCKET     LEZ v0.4.0-rc3 (bafd07be) from GitHub; this repository and the program binaries
#                                 from the artifact bucket (bundle/ferrata-src.tar.gz, bundle/programs.tar.gz)
#   provision.sh build-zone       LEZ's sequencer_service and wallet; the Logos Blockchain node LEZ pins (its resolver)
#   provision.sh build-explorer   LEZ's indexer_service (feature prove, as LEZ's Dockerfile) and explorer_service
#   provision.sh build-ferrata    ferrata-day1, ferrata-updater (against rc3: lez-select.sh), attest-monitor
#   provision.sh install          binaries, configuration, systemd units, Caddy, the site; keys made on the host
#
# Logs: /var/log/ferrata/provision-<stage>.log. Nothing here prints a key.
set -euo pipefail
STAGE="${1:?stage}"; shift || true
LEZ_TAG=v0.4.0-rc3
LEZ_COMMIT=bafd07be0701398f9bc435f73ab05e9a7936c301
OPT=/opt/ferrata
SRC=$OPT/src
BUILD=$OPT/build
DATA=/var/lib/ferrata
LOGDIR=/var/log/ferrata
export HOME=/root CARGO_HOME=/root/.cargo RUSTUP_HOME=/root/.rustup DEBIAN_FRONTEND=noninteractive
export PATH="/root/.cargo/bin:/root/.risc0/bin:/usr/local/bin:$PATH"
# The Logos circuit crates fetch their prover artifacts into the cache at build time and read them at run time.
export XDG_CACHE_HOME=$OPT/cache
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$(nproc)}"
mkdir -p "$LOGDIR" "$OPT/bin" "$OPT/programs" "$SRC" "$BUILD" "$OPT/cache"
exec > >(tee -a "$LOGDIR/provision-$STAGE.log") 2>&1
echo "== $(date -u +%FT%TZ) provision $STAGE $*"

base() {
  local vol="${1:?data volume id}"
  apt-get update -q
  apt-get -y -q -o Dpkg::Options::=--force-confold upgrade
  apt-get install -y -q build-essential pkg-config libssl-dev libclang-dev clang cmake ninja-build curl unzip git jq \
    python3 python3-venv libpcsclite-dev protobuf-compiler chrony logrotate nvme-cli gnupg xz-utils zstd \
    debian-keyring debian-archive-keyring apt-transport-https unattended-upgrades
  # Amazon Time Sync first: Bedrock's slots are wall-clock seconds.
  grep -q '169.254.169.123' /etc/chrony/chrony.conf \
    || sed -i '1i server 169.254.169.123 prefer iburst minpoll 4 maxpoll 4' /etc/chrony/chrony.conf
  systemctl restart chrony
  id ferrata >/dev/null 2>&1 || useradd --system --home-dir "$DATA" --no-create-home --shell /usr/sbin/nologin ferrata
  # The public gateway's own user: it owns only the faucet's key and ledger.
  id ferrata-gw >/dev/null 2>&1 || useradd --system --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin ferrata-gw
  # The data volume (its NVMe serial is the volume id without the dash).
  local dev="/dev/disk/by-id/nvme-Amazon_Elastic_Block_Store_${vol//-/}"
  for _ in $(seq 1 60); do [ -e "$dev" ] && break; sleep 2; done
  [ -e "$dev" ] || { echo "data volume $vol not attached"; exit 1; }
  if ! blkid "$dev" >/dev/null 2>&1; then mkfs.ext4 -q -L ferrata-data "$dev"; fi
  local uuid; uuid="$(blkid -s UUID -o value "$dev")"
  mkdir -p "$DATA"
  grep -q "$uuid" /etc/fstab || echo "UUID=$uuid $DATA ext4 defaults,nofail,noatime 0 2" >> /etc/fstab
  mountpoint -q "$DATA" || mount "$DATA"
  chown ferrata:ferrata "$DATA"; chmod 750 "$DATA"
  install -d -o ferrata -g ferrata -m 750 "$LOGDIR"
  # AWS CLI v2 (artifacts, metrics) and the CloudWatch agent, pinned versions checked by sha256 (24 Nov 2026).
  if ! command -v aws >/dev/null; then
    curl -sSfL https://awscli.amazonaws.com/awscli-exe-linux-x86_64-2.42.2.zip -o "$BUILD/awscliv2.zip"
    echo "eb2181c8f391ee63acbadf53b13a80efe8702cbfef6a7da99a57e60631b47665  $BUILD/awscliv2.zip" | sha256sum -c -
    rm -rf "$BUILD/aws"; unzip -q "$BUILD/awscliv2.zip" -d "$BUILD"; "$BUILD/aws/install" --update
  fi
  if [ ! -x /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl ]; then
    curl -sSfL https://amazoncloudwatch-agent.s3.amazonaws.com/ubuntu/amd64/1.300075.0b2061/amazon-cloudwatch-agent.deb -o "$BUILD/cwagent.deb"
    echo "7b22430a255ebdeae68398f83b1e36d10194c5dc8824526c0d4f26ce3b6ea696  $BUILD/cwagent.deb" | sha256sum -c -
    dpkg -i -E "$BUILD/cwagent.deb"
  fi
  # Caddy from its own apt repository: the repository's key checked by its fingerprint, the version pinned.
  if ! command -v caddy >/dev/null; then
    curl -1sLf https://dl.cloudsmith.io/public/caddy/stable/gpg.key -o "$BUILD/caddy.key"
    gpg --show-keys --with-colons "$BUILD/caddy.key" | grep -q '^fpr:::::::::65760C51EDEA2017CEA2CA15155B6D79CA56EA34:' \
      || { echo "Caddy's repository key has another fingerprint"; exit 1; }
    gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg < "$BUILD/caddy.key"
    curl -1sLf https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt > /etc/apt/sources.list.d/caddy-stable.list
    apt-get update -q; apt-get install -y -q caddy=2.11.9
  fi
  systemctl disable --now ssh.service ssh.socket 2>/dev/null || true
  echo "base done: $(df -h "$DATA" | tail -1)"
}

toolchain() {
  if ! command -v rustup >/dev/null; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.98.1
  fi
  rustup toolchain install 1.98.1 1.93.0 --profile minimal
  rustup target add wasm32-unknown-unknown --toolchain 1.98.1
  command -v rzup >/dev/null || cargo +1.98.1 install --locked rzup --version 0.5.2
  RISC0_HOME_LEZ=/root/risc0-home-lez bash "$SRC/ferrata/scripts/ci/install-risc0.sh"
  command -v cargo-leptos >/dev/null || cargo +1.98.1 install --locked cargo-leptos --version 0.3.12
  local r0; r0="$(ls -d /root/.risc0/extensions/v3.0.5-cargo-risczero-*/r0vm | head -1)"
  install -m 755 "$r0" "$OPT/bin/r0vm"
  "$OPT/bin/r0vm" --version
}

fetch() {
  local bucket="${1:?artifact bucket}"
  local lez="$SRC/upstream/lez-$LEZ_TAG"
  if [ ! -d "$lez/.git" ]; then
    git clone -q --depth 1 --branch "$LEZ_TAG" https://github.com/logos-blockchain/logos-execution-zone "$lez"
  fi
  [ "$(git -C "$lez" rev-parse HEAD)" = "$LEZ_COMMIT" ] || { echo "LEZ $LEZ_TAG is not $LEZ_COMMIT"; exit 1; }
  aws s3 cp --only-show-errors "s3://$bucket/bundle/ferrata-src.tar.gz" "$BUILD/ferrata-src.tar.gz"
  aws s3 cp --only-show-errors "s3://$bucket/bundle/programs.tar.gz" "$BUILD/programs.tar.gz"
  rm -rf "$SRC/ferrata.new"; mkdir -p "$SRC/ferrata.new"
  tar -xzf "$BUILD/ferrata-src.tar.gz" -C "$SRC/ferrata.new"
  rm -rf "$SRC/ferrata"; mv "$SRC/ferrata.new" "$SRC/ferrata"
  tar -xzf "$BUILD/programs.tar.gz" -C "$OPT/programs"
  # The site bundle, when the operator uploaded one (deploy.sh bundle with WEB_TGZ): install.sh installs it.
  aws s3 cp --only-show-errors "s3://$bucket/bundle/web.tar.gz" "$OPT/web.tar.gz" 2>/dev/null || true
  ( cd "$OPT/programs" && sha256sum -c SHA256SUMS )
  # Ferrata's LEZ crates against rc3 (manifests, features, lockfiles), as the rehearsal built them.
  "$SRC/ferrata/lez/scripts/lez-select.sh" "$LEZ_TAG"
}

build_zone() {
  local lez="$SRC/upstream/lez-$LEZ_TAG"
  ( cd "$lez" && CARGO_TARGET_DIR="$BUILD/target-lez" cargo build --release --locked -p sequencer_service -p wallet )
  install -m 755 "$BUILD/target-lez/release/sequencer_service" "$BUILD/target-lez/release/wallet" "$OPT/bin/"
  # The node LEZ pins, by LEZ's resolver (a release whose tag, asset checksum and commit match, or a build).
  ( cd "$lez" && RUSTFLAGS="-A single-use-lifetimes" RISC0_DEV_MODE=1 python3 -I bedrock/tools/resolve_bedrock_node.py \
      --target-platform host --output-directory "$OPT/bin/bedrock" --target-directory "$BUILD/target-bedrock-node" )
  cat "$OPT/bin/bedrock/metadata.json"
  sha256sum "$OPT/bin/sequencer_service" "$OPT/bin/wallet" "$OPT/bin/bedrock/logos-blockchain-node" | tee "$OPT/bin/zone.sha256"
}

build_explorer() {
  local lez="$SRC/upstream/lez-$LEZ_TAG"
  # Feature `prove` builds risc0-circuit-recursion, whose build script downloads recursion_zkr.zip from RISC Zero's
  # bucket; that answered 403 from this host (23 Nov 2026). The same file (risc0 v3.0.5's src/recursion_zkr.zip) is
  # given through RECURSION_SRC_PATH from the artifact bucket when it is there; the build script checks its sha256.
  local zkr="$OPT/cache/recursion_zkr.zip"
  if [ ! -f "$zkr" ] && [ -n "${FERRATA_BUCKET:-}" ]; then
    aws s3 cp --only-show-errors "s3://$FERRATA_BUCKET/bundle/recursion_zkr.zip" "$zkr" || rm -f "$zkr"
  fi
  if [ -f "$zkr" ]; then
    echo "744b999f0a35b3c86753311c7efb2a0054be21727095cf105af6ee7d3f4d8849  $zkr" | sha256sum -c - && export RECURSION_SRC_PATH="$zkr"
  fi
  ( cd "$lez" && CARGO_TARGET_DIR="$BUILD/target-indexer" cargo build --release --locked -p indexer_service --features prove --bin indexer_service )
  install -m 755 "$BUILD/target-indexer/release/indexer_service" "$OPT/bin/"
  ( cd "$lez/lez/explorer_service" && cargo leptos build --release )
  install -m 755 "$lez/target/release/explorer_service" "$OPT/bin/"
  rm -rf "$OPT/explorer-site"; cp -r "$lez/target/site" "$OPT/explorer-site"
  sha256sum "$OPT/bin/indexer_service" "$OPT/bin/explorer_service" | tee "$OPT/bin/explorer.sha256"
}

build_ferrata() {
  local repo="$SRC/ferrata"
  ( cd "$repo/lez/updater/sequencer" && RISC0_HOME=/root/risc0-home-lez CARGO_TARGET_DIR="$BUILD/target-ferrata" \
      cargo build --release --locked --bin ferrata-day1 --bin ferrata-updater )
  install -m 755 "$BUILD/target-ferrata/release/ferrata-day1" "$BUILD/target-ferrata/release/ferrata-updater" "$OPT/bin/"
  # attest-monitor links lee_core by path and lez-select.sh does not rewrite its manifest: point it at the rc3 tree
  # (lee/state_machine/core is the same in rc1 and rc3: account.rs and Cargo.toml compared, no difference).
  sed -i 's#upstream/lez-v0.4.0-rc1/#upstream/lez-v0.4.0-rc3/#' "$repo/monitor/attest/Cargo.toml"
  ( cd "$repo/monitor/attest" && CARGO_TARGET_DIR="$BUILD/target-monitor" cargo build --release --locked )
  install -m 755 "$BUILD/target-monitor/release/attest-monitor" "$OPT/bin/"
  sha256sum "$OPT/bin/ferrata-day1" "$OPT/bin/ferrata-updater" "$OPT/bin/attest-monitor" | tee "$OPT/bin/ferrata.sha256"
}

install_host() {
  bash "$SRC/ferrata/deploy/testnet/install.sh"
}

case "$STAGE" in
  base) base "$@" ;;
  toolchain) toolchain ;;
  fetch) fetch "$@" ;;
  build-zone) build_zone ;;
  build-explorer) build_explorer ;;
  build-ferrata) build_ferrata ;;
  install) install_host ;;
  *) echo "unknown stage $STAGE"; exit 2 ;;
esac
echo "== $(date -u +%FT%TZ) provision $STAGE done"
