# Ferrata's reproduction image: every toolchain pinned and checked, the upstream sources in /work/upstream, and no
# copy of the tree. scripts/reproduce/run.sh (Docker) and scripts/reproduce/modal_app.py (Modal) put a checkout at
# /work/ferrata at run time and run scripts/reproduce/check.sh there; `scripts/reproduce-image-ids.sh docker` uses
# the image for the guests alone. The paths do not matter to the image ids (scripts/guest-rustc remaps them); /work
# is a neutral choice. x86-64 only (RISC Zero's toolchains). What is pinned, each checked while the image builds:
#   Ubuntu 24.04 by digest (its packages are the archive's of the day the image is built: check.sh writes their
#     versions to packages.txt; the image ids do not depend on them, the guests are built by the toolchains below);
#   rustup 1.29.3 (sha256); Rust 1.93.0 (the default: the workspace, checkpoint/, bridge/), 1.98.1 (lez/ and cli/ pin
#     it), 1.88 (checkpoint/'s `cargo +1.88`);
#   rzup 0.5.2 (crates.io, --locked: the rzup risc0-build 3.0.5 links; the installer at risczero.com fetches whatever
#     rzup is newest); RISC Zero 3.0.5 (cargo-risczero, r0vm) with guest Rust 1.88.0 and C++ 2024.1.5 in ~/.risc0,
#     and the same with guest Rust 1.91.1, LEZ's own, in /work/risc0-home-lez (as scripts/ci/install-risc0.sh);
#   Foundry v1.3.6 (the release tarball's sha256); solc 0.8.26 (the sha256 of binaries.soliditylang.org's list);
#   LEZ v0.4.0-rc1 (1806e87), v0.4.0-rc2 (e3edf47) and v0.4.0-rc3 (bafd07b; the test network's, for scripts/reproduce/deployed.sh), Logos Blockchain d480efc6 (scripts/fetch-upstream.sh's).
FROM ubuntu:24.04@sha256:2198e4c65076e9cb0b3d695219df667c0e3b0072da64ca3c2648aca17dcecaf5
ENV HOME=/home/builder RUSTUP_HOME=/home/builder/.rustup CARGO_HOME=/home/builder/.cargo \
    FOUNDRY_DIR=/home/builder/.foundry RISC0_HOME_LEZ=/work/risc0-home-lez CARGO_TERM_COLOR=never LANG=C.UTF-8 \
    DEBIAN_FRONTEND=noninteractive \
    PATH=/home/builder/.cargo/bin:/home/builder/.risc0/bin:/home/builder/.foundry/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# System packages. libpcsclite-dev is for cli/ (LEZ's wallet crates), the rest for the builds of every workspace and
# the scripts. (Ubuntu's snapshot archive would pin them by date, but served about 60 KB/s on 12 November 2026.)
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl build-essential pkg-config \
       libssl-dev git protobuf-compiler libprotobuf-dev clang libclang-dev llvm lld cmake ninja-build zstd rsync jq \
       unzip xz-utils file libudev-dev libpcsclite-dev libsqlite3-dev perl m4 nasm libgmp-dev python3 time procps \
    && rm -rf /var/lib/apt/lists/* && mkdir -p $HOME /work/upstream

# Rust: minimal profiles (no local docs); rustfmt and clippy on the default toolchain.
RUN curl --proto '=https' --tlsv1.2 -sSfL -o /root/rustup-init \
       https://static.rust-lang.org/rustup/archive/1.29.3/x86_64-unknown-linux-gnu/rustup-init \
    && echo 'b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55  /root/rustup-init' | sha256sum -c - \
    && chmod +x /root/rustup-init \
    && /root/rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.93.0 --component rustfmt,clippy \
    && rm /root/rustup-init && rustup toolchain install 1.98.1 1.88 --profile minimal && rustup toolchain list

# RISC Zero's toolchains, in two homes: 3.1.0 for bridge/ and checkpoint/ (risc0-zkvm 3.1.0), 3.0.5 for lez/ (LEZ's).
RUN cargo install rzup --version 0.5.2 --locked && rm -rf $CARGO_HOME/registry $CARGO_HOME/git \
    && for hv in $HOME/.risc0:3.1.0 $RISC0_HOME_LEZ:3.0.5; do h=${hv%:*}; v=${hv##*:}; \
         RISC0_HOME=$h rzup install cargo-risczero $v && (RISC0_HOME=$h rzup install r0vm $v || true) \
         && RISC0_HOME=$h rzup install cpp 2024.1.5 || exit 1; done \
    && rzup install rust 1.88.0 && RISC0_HOME=$RISC0_HOME_LEZ rzup install rust 1.91.1 \
    && rzup show && RISC0_HOME=$RISC0_HOME_LEZ rzup show \
    && { r0vm --version | grep 3.1.0 && cargo risczero --version || { find $HOME/.risc0 -maxdepth 3; exit 1; }; }

# Foundry, then solc fetched once by forge (vault/foundry.toml keeps forge offline) and checked.
RUN mkdir -p $FOUNDRY_DIR/bin && cd $FOUNDRY_DIR && curl -sSfL -o foundry.tar.gz \
       https://github.com/foundry-rs/foundry/releases/download/v1.3.6/foundry_v1.3.6_linux_amd64.tar.gz \
    && echo '01248d7ce267f5b7958bc9b331d068c6c2d8eeb182e58f0b556d7295f398e2ce  foundry.tar.gz' | sha256sum -c - \
    && tar -xzf foundry.tar.gz -C bin && rm foundry.tar.gz && forge --version \
    && mkdir -p /opt/solc-warm/src && cd /opt/solc-warm \
    && printf '[profile.default]\nsolc_version = "0.8.26"\nauto_detect_solc = false\n' > foundry.toml \
    && printf '// SPDX-License-Identifier: MIT\npragma solidity 0.8.26;\ncontract W {}\n' > src/W.sol \
    && forge build && cd / && rm -rf /opt/solc-warm \
    && s="$(ls $HOME/.svm/0.8.26/solc-0.8.26 $HOME/.local/share/svm/0.8.26/solc-0.8.26 2>/dev/null | head -n 1)" \
    && { echo "d5f23436f443edb85d8e76906d12f0a86ce0490e7663a9e608efeb7a93f149ef  $s" | sha256sum -c - \
         || { find $HOME -name 'solc*'; exit 1; }; }

# The upstream trees beside the checkout, where scripts/fetch-upstream.sh puts them (run again, it only checks them).
RUN for spec in v0.4.0-rc1:1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 v0.4.0-rc2:e3edf47402a82557103ca719b30d0bd2684338d1 v0.4.0-rc3:bafd07be0701398f9bc435f73ab05e9a7936c301; do \
      tag=${spec%%:*}; commit=${spec#*:}; \
      git clone -q --depth 1 --branch $tag https://github.com/logos-blockchain/logos-execution-zone.git \
        /work/upstream/lez-$tag && test "$(git -C /work/upstream/lez-$tag rev-parse HEAD)" = $commit || exit 1; done \
    && git init -q /work/upstream/logos-blockchain-d480efc6 && cd /work/upstream/logos-blockchain-d480efc6 \
    && git remote add origin https://github.com/logos-blockchain/logos-blockchain.git \
    && git fetch -q --depth 1 origin d480efc6bbb48fb493c10fe9afefd84c18a04f02 && git checkout -q FETCH_HEAD \
    && test "$(git rev-parse HEAD)" = d480efc6bbb48fb493c10fe9afefd84c18a04f02 \
    && git config --global --add safe.directory '*' && git config --global advice.detachedHead false
WORKDIR /work
