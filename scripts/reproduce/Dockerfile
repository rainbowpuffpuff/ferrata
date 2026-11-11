# The crown build box's toolchains at the crown build box's paths, to rebuild the deployed guests with the same
# image ids (a guest built without RISC Zero's Docker build embeds absolute source paths: $HOME/.cargo/registry,
# $HOME/.cargo/git, the tree at the crown build path and spike/upstream/lez-v0.4.0-rc1).
# Mirrors spike/modal-build/box.py's image (ubuntu 24.04, HOME=/home/q). Used by scripts/reproduce-image-ids.sh docker.
FROM ubuntu:24.04
ARG SPIKE=/home/q/Downloads/software_i_built/logos-rfp/spike
ENV HOME=/home/q RUSTUP_HOME=/home/q/.rustup CARGO_HOME=/home/q/.cargo CARGO_TERM_COLOR=never \
    PATH=/home/q/.cargo/bin:/home/q/.risc0/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    FERRATA_REPRO_CONTAINER=1 DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates build-essential pkg-config \
      libssl-dev git protobuf-compiler libprotobuf-dev clang libclang-dev llvm lld cmake ninja-build zstd rsync jq \
      unzip xz-utils file libudev-dev perl m4 nasm libgmp-dev python3 \
    && rm -rf /var/lib/apt/lists/* && mkdir -p $HOME $SPIKE/upstream $SPIKE/tmp
# Host Rust: 1.93.0 (default; checkpoint/ and bridge/), 1.98.1 (lez/rust-toolchain.toml), 1.88.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default --default-toolchain 1.93.0 \
    && rustup toolchain install 1.98.1 --profile default && rustup toolchain install 1.88 --profile default
# RISC Zero 3.0.5: guest Rust 1.88.0 in ~/.risc0 (checkpoint/, bridge/); guest Rust 1.91.1 in spike/risc0-home-lez (lez/).
RUN curl -L https://risczero.com/install | bash \
    && rzup install cargo-risczero 3.0.5 && (rzup install r0vm 3.0.5 || true) && rzup install rust 1.88.0 && rzup install cpp 2024.1.5
RUN export RISC0_HOME=$SPIKE/risc0-home-lez && rzup install rust 1.91.1 && rzup install cpp 2024.1.5 \
    && rzup install cargo-risczero 3.0.5 && (rzup install r0vm 3.0.5 || true) && rzup show && r0vm --version
# The upstream trees the guests and the workspaces name by path (LEZ v0.4.0-rc1; the Logos Blockchain revision it pins).
RUN git clone --depth 1 --branch v0.4.0-rc1 https://github.com/logos-blockchain/logos-execution-zone.git \
      $SPIKE/upstream/lez-v0.4.0-rc1 \
    && test "$(git -C $SPIKE/upstream/lez-v0.4.0-rc1 rev-parse HEAD)" = 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 \
    && git init -q $SPIKE/upstream/logos-blockchain-d480efc6 && cd $SPIKE/upstream/logos-blockchain-d480efc6 \
    && git remote add origin https://github.com/logos-blockchain/logos-blockchain.git \
    && git fetch -q --depth 1 origin d480efc6bbb48fb493c10fe9afefd84c18a04f02 && git checkout -q FETCH_HEAD \
    && git config --global --add safe.directory '*'
WORKDIR /home/q
