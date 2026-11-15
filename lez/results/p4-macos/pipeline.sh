#!/bin/bash
# p4-macos: the whole measurement on one Mac, as ec2-user, from ~/w: the machine, the toolchains, the sources,
# the build, then real proofs (RISC0_DEV_MODE=0) of the padded private attestation, claim and burn, two runs
# each, every transaction settled by LEZ's settlement (receipt verified), and after run 1 one flipped bit in
# each transaction refused.
# usage (after copying ferrata.bundle and this file to ~/w):
#   nohup bash ~/w/pipeline.sh > ~/w/out/pipeline.log 2>&1 &
# Each phase appends "BEGIN/END <phase> exit <rc>" to ~/w/out/progress.txt; a failed phase stops the pipeline.
set -u
W=$HOME/w; O=$W/out; P=$O/progress.txt
mkdir -p "$O"
FERRATA_HEAD=17030d462c0a7c8c8b48eb14a68acde98a52624c   # integration6 = main on GitHub
LEZ_COMMIT=1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21     # logos-execution-zone v0.4.0-rc1
LB_COMMIT=d480efc6bbb48fb493c10fe9afefd84c18a04f02       # logos-blockchain, the revision LEZ pins
export CARGO_TERM_COLOR=never
mark() { echo "$(date -u +%FT%TZ) $*" >> "$P"; }
step() {
  local n=$1; shift; mark "BEGIN $n"; "$@"; local rc=$?; mark "END $n exit $rc"
  [ "$rc" = 0 ] || { mark "STOP at $n"; exit "$rc"; }
}
envs() { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; export PATH=$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH; }

machine() {
  {
    date -u +%FT%TZ
    echo "== sysctl -n machdep.cpu.brand_string hw.ncpu hw.perflevel0.physicalcpu hw.memsize"
    sysctl -n machdep.cpu.brand_string hw.ncpu hw.perflevel0.physicalcpu hw.memsize
    echo "== sysctl (more)"
    sysctl hw.model hw.nperflevels hw.physicalcpu hw.logicalcpu hw.perflevel0.name hw.perflevel1.name \
      hw.perflevel1.physicalcpu hw.pagesize 2>&1
    echo "== sw_vers"; sw_vers
    echo "== uname -a"; uname -a
    echo "== xcode-select -p"; xcode-select -p 2>&1
    pkgutil --pkg-info=com.apple.pkg.CLTools_Executables 2>&1 | head -2
    echo "== clang --version"; clang --version 2>&1 | head -1
    echo "== git --version"; git --version 2>&1
    echo "== perl -v"; perl -e 'print "$^V\n"'
    echo "== df -h \$HOME"; df -h "$HOME" | tail -1
    echo "== GPU (system_profiler SPDisplaysDataType)"
    system_profiler SPDisplaysDataType 2>/dev/null | grep -E 'Chipset Model|Total Number of Cores|Metal'
    echo "== sudo -n true"; sudo -n true && echo "sudo ok"
    echo "== uptime"; uptime
  } > "$O/machine.txt" 2>&1
}

toolchains() {
  {
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o "$W/rustup-init.sh" || return 1
    sh "$W/rustup-init.sh" -y --profile minimal --default-toolchain none --no-modify-path || return 1
    envs
    rustup toolchain install 1.98.1 --profile minimal || return 1
    curl --proto '=https' --tlsv1.2 -sSfL https://risczero.com/install -o "$W/rzup-install.sh" || return 1
    bash "$W/rzup-install.sh" || return 1
    envs
    rzup --version
    rzup install rust 1.91.1 || return 1
    rzup install cpp 2024.1.5 || return 1
    rzup install r0vm 3.0.5 || return 1
    rzup install cargo-risczero 3.0.5 || return 1
    rzup show
    ls -la "$HOME/.risc0/extensions" "$HOME/.cargo/bin"
  } > "$O/toolchains.log" 2>&1
}

r0vm_path() { ls -d "$HOME"/.risc0/extensions/v3.0.5-cargo-risczero-*/r0vm 2>/dev/null | head -1; }

r0vm_info() {
  envs; R0VM=$(r0vm_path); [ -x "$R0VM" ] || return 1
  {
    echo "r0vm $R0VM"; "$R0VM" --version; shasum -a 256 "$R0VM"
    echo "== otool -L"; otool -L "$R0VM"
    echo "== rustc +risc0 --version"; rustc +risc0 --version
    echo "== cargo risczero --version"; cargo risczero --version
  } > "$O/r0vm.txt" 2>&1
}

sources() {
  {
    cd "$W" || return 1
    git clone -q -b integration6 "$W/ferrata.bundle" ferrata || return 1
    test "$(git -C ferrata rev-parse HEAD)" = "$FERRATA_HEAD" || { echo "unexpected ferrata HEAD"; return 1; }
    mkdir -p upstream && cd upstream || return 1
    git clone -q --depth 1 --branch v0.4.0-rc1 https://github.com/logos-blockchain/logos-execution-zone.git \
      lez-v0.4.0-rc1 || return 1
    test "$(git -C lez-v0.4.0-rc1 rev-parse HEAD)" = "$LEZ_COMMIT" || { echo "unexpected LEZ commit"; return 1; }
    git init -q logos-blockchain-d480efc6 || return 1
    git -C logos-blockchain-d480efc6 remote add origin https://github.com/logos-blockchain/logos-blockchain.git
    git -C logos-blockchain-d480efc6 fetch -q --depth 1 origin "$LB_COMMIT" || return 1
    git -C logos-blockchain-d480efc6 checkout -q FETCH_HEAD || return 1
    test "$(git -C logos-blockchain-d480efc6 rev-parse HEAD)" = "$LB_COMMIT" || { echo "unexpected LB commit"; return 1; }
    for d in "$W/ferrata" "$W/upstream/lez-v0.4.0-rc1" "$W/upstream/logos-blockchain-d480efc6"; do
      echo "$d $(git -C "$d" rev-parse HEAD) $(git -C "$d" status --porcelain | wc -l | tr -d ' ') changes"
    done
  } > "$O/sources.log" 2>&1
}

build() {
  envs; export RISC0_BUILD_LOCKED=1
  cd "$W/ferrata/lez" || return 1
  { date -u +%FT%TZ; rustc --version; cargo --version; rustup show active-toolchain; } > "$O/build-env.txt" 2>&1
  /usr/bin/time -l cargo build -p harness --locked --bin prove_attest --bin prove_bridge > "$O/build.log" 2>&1 || return 1
  for b in prove_attest prove_bridge; do
    target/debug/$b --image-ids > "$O/$b-ids.txt" 2>&1
    echo "$b $(stat -f %z target/debug/$b) bytes $(shasum -a 256 target/debug/$b | cut -c1-64)" >> "$O/build-env.txt"
  done
}

sampler() { # $1 csv: every 2 s r0vm's RSS, the host binary's RSS and vm_stat's page counts (16 KiB pages)
  local f=$1 t0 r h v; t0=$(date +%s)
  echo "t_s,r0vm_rss_kib,r0vm_procs,host_rss_kib,pages_free,pages_active,pages_inactive,pages_wired,pages_compressor,swapouts" > "$f"
  while :; do
    r=$(ps -axo rss=,comm= | awk '$2 ~ /(^|\/)r0vm$/ {s+=$1; n++} END {printf "%d,%d", s, n}')
    h=$(ps -axo rss=,comm= | awk '$2 ~ /\/prove_(attest|bridge)$/ {s+=$1} END {printf "%d", s}')
    v=$(vm_stat | awk -F: '/^Pages free/ {f=$2} /^Pages active/ {a=$2} /^Pages inactive/ {i=$2}
        /^Pages wired down/ {w=$2} /occupied by compressor/ {c=$2} /^Swapouts/ {s=$2}
        END {gsub(/[ .]/,"",f); gsub(/[ .]/,"",a); gsub(/[ .]/,"",i); gsub(/[ .]/,"",w); gsub(/[ .]/,"",c);
             gsub(/[ .]/,"",s); printf "%s,%s,%s,%s,%s,%s", f, a, i, w, c, s}')
    echo "$(( $(date +%s) - t0 )),$r,$h,$v" >> "$f"
    sleep 2
  done
}

flip() { # bit 0 of the middle byte, as minimum.py's _flip
  perl -e 'open(my $f, "<:raw", $ARGV[0]) or die; local $/; my $d = <$f>; my $at = int(length($d) / 2);
           substr($d, $at, 1) = chr(ord(substr($d, $at, 1)) ^ 1); open(my $g, ">:raw", $ARGV[1]) or die;
           print $g $d; print "$at\n"' "$1" "$2"
}

prove_run() { # $1 run, $2 job (attest | bridge)
  local i=$1 job=$2 rd=$O/runs/run-$1/$2 bin txs rc t0 t1 sp pm="" t u files at
  envs; R0VM=$(r0vm_path)
  mkdir -p "$rd"; cd "$W/ferrata/lez" || return 1
  case $job in
    attest) bin=prove_attest; txs="tx-attest.borsh"; set -- --shape wallet --tx-out "$rd/tx-attest.borsh";;
    bridge) bin=prove_bridge; txs="tx-claim.borsh tx-burn.borsh"
            set -- --shape wallet --claim-out "$rd/tx-claim.borsh" --burn-out "$rd/tx-burn.borsh";;
  esac
  sampler "$rd/mem.csv" & sp=$!
  if [ "$i" = 1 ]; then # is the GPU used? its residency, 60 s from 120 s into the proof (LEZ's circuit by then)
    ( sleep 120; sudo -n powermetrics --samplers gpu_power,cpu_power -i 10000 -n 6 > "$rd/powermetrics.txt" 2>&1 ) &
    pm=$!
  fi
  echo "RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info RISC0_SERVER_PATH=$R0VM /usr/bin/time -l target/debug/$bin $*" \
    > "$rd/command.txt"
  t0=$(date +%s)
  RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info RISC0_SERVER_PATH=$R0VM /usr/bin/time -l "target/debug/$bin" "$@" \
    > "$rd/prove.log" 2>&1
  rc=$?; t1=$(date +%s)
  kill "$sp" 2>/dev/null; [ -n "$pm" ] && wait "$pm"
  echo "prove exit $rc wall_s $((t1 - t0)) start $(date -u -r "$t0" +%FT%TZ)" > "$rd/result.txt"
  [ "$rc" = 0 ] || return "$rc"
  (cd "$rd" && RISC0_DEV_MODE=0 "$W/ferrata/lez/target/debug/$bin" --settle $txs > settle.log 2>&1)
  echo "settle exit $?" >> "$rd/result.txt"
  if [ "$i" = 1 ]; then
    for t in $txs; do
      at=$(flip "$rd/$t" "$rd/tampered-$t")
      files=""; for u in $txs; do if [ "$u" = "$t" ]; then files="$files tampered-$t"; else files="$files $u"; fi; done
      (cd "$rd" && RISC0_DEV_MODE=0 "$W/ferrata/lez/target/debug/$bin" --settle $files > "tampered-$t.log" 2>&1)
      echo "tampered $t (bit 0 of byte $at) settle exit $?" >> "$rd/result.txt"
      rm -f "$rd/tampered-$t"
    done
  fi
}

mark "pipeline start"
step machine machine
step toolchains toolchains
step r0vm r0vm_info
step sources sources
step build build
for i in 1 2; do
  for job in attest bridge; do step "run-$i-$job" prove_run "$i" "$job"; done
done
mark "DONE"
