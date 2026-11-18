# Measurement environment

- date: 2026-11-10T21:18:09+00:00
- where: a Modal build box (profile ws-9; 16 physical cores, 64 GiB at most; under gVisor), by spike/tmp/rc2/box1.sh
- ferrata worktree HEAD: b76130052e45e1b94c232a98f556425f9d41510a (uncommitted: 0 paths besides results/ and RESULTS.md)
- LEZ: v0.4.0-rc2 = e3edf47402a82557103ca719b30d0bd2684338d1, upstream tree clean: yes
- risc0-zkvm (host and guests): version="3.0.5"
- r0vm: risc0-r0vm 3.0.5
- host toolchain: rustc 1.98.1 (48a229cea 2026-09-01)
- guest toolchain: RISC Zero Rust 1.91.1 (in RISC0_HOME), C++ 2024.1.5
- CPU: unknown; 32 processors in /proc/cpuinfo, 2 usable by this process (nproc)
- RAM: 64 GB; GPU: none
- OS: Linux 4.19.0-gvisor
