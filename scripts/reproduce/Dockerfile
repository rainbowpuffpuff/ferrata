# The pinned toolchains, to rebuild every guest of this tree to its recorded image ids (scripts/reproduce-image-ids.sh
# docker). Paths do not matter: scripts/guest-rustc remaps the tree, the upstream sources, the cargo home and the
# RISC Zero toolchains in guest builds, so any layout gives the same ids; this one is just a neutral choice. The tree
# is copied to /work/ferrata at run time, beside the upstream sources the guests name by relative path (../upstream).
FROM ubuntu:24.04
ENV HOME=/home/builder RUSTUP_HOME=/home/builder/.rustup CARGO_HOME=/home/builder/.cargo CARGO_TERM_COLOR=never \
    PATH=/home/builder/.cargo/bin:/home/builder/.risc0/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    RISC0_HOME_LEZ=/work/risc0-home-lez DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates build-essential pkg-config \
      libssl-dev git protobuf-compiler libprotobuf-dev clang libclang-dev llvm lld cmake ninja-build zstd rsync jq \
      unzip xz-utils file libudev-dev perl m4 nasm libgmp-dev python3 \
    && rm -rf /var/lib/apt/lists/* && mkdir -p $HOME /work/upstream
# Host Rust: 1.93.0 (default; checkpoint/ and bridge/), 1.98.1 (lez/rust-toolchain.toml), 1.88.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default --default-toolchain 1.93.0 \
    && rustup toolchain install 1.98.1 --profile default && rustup toolchain install 1.88 --profile default
# RISC Zero 3.0.5: guest Rust 1.88.0 in ~/.risc0 (checkpoint/, bridge/); guest Rust 1.91.1 in its own home (lez/).
RUN curl -L https://risczero.com/install | bash \
    && rzup install cargo-risczero 3.0.5 && (rzup install r0vm 3.0.5 || true) && rzup install rust 1.88.0 && rzup install cpp 2024.1.5
RUN export RISC0_HOME=$RISC0_HOME_LEZ && rzup install rust 1.91.1 && rzup install cpp 2024.1.5 \
    && rzup install cargo-risczero 3.0.5 && (rzup install r0vm 3.0.5 || true) && rzup show && r0vm --version
# The upstream trees the guests and the workspaces name by path (LEZ v0.4.0-rc1; the Logos Blockchain revision it pins).
RUN git clone --depth 1 --branch v0.4.0-rc1 https://github.com/logos-blockchain/logos-execution-zone.git \
      /work/upstream/lez-v0.4.0-rc1 \
    && test "$(git -C /work/upstream/lez-v0.4.0-rc1 rev-parse HEAD)" = 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 \
    && git init -q /work/upstream/logos-blockchain-d480efc6 && cd /work/upstream/logos-blockchain-d480efc6 \
    && git remote add origin https://github.com/logos-blockchain/logos-blockchain.git \
    && git fetch -q --depth 1 origin d480efc6bbb48fb493c10fe9afefd84c18a04f02 && git checkout -q FETCH_HEAD \
    && git config --global --add safe.directory '*'
WORKDIR /work
