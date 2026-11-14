# Measurement environment

- date: 2026-11-14T09:42:12+00:00
- where: a Modal build box (our Modal profile; 16 physical cores, 64 GiB at most; under gVisor), by spike/tmp/lez-rc3/boxC.sh
- ferrata worktree HEAD: 97246f4 (uncommitted: 0 paths besides results/ and RESULTS.md)
- LEZ: v0.4.0-rc3 = bafd07be0701398f9bc435f73ab05e9a7936c301, upstream tree clean: yes
- risc0-zkvm (host and guests): version="3.0.5"
- r0vm: risc0-r0vm 3.0.5
- host toolchain: rustc 1.98.1 (48a229cea 2026-09-01)
- guest toolchain: RISC Zero Rust 1.88.0 (in RISC0_HOME), C++ 2024.1.5
- CPU: unknown; 32 processors in /proc/cpuinfo, 2 usable by this process (nproc)
- RAM: 64 GB; GPU: none
- OS: Linux 4.19.0-gvisor
