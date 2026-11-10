"""Derived figures for RESULTS.md from results/<run>/<fixture>/run.json (run with `python3 -I`).

Prints the CPU-versus-GPU table, the per-segment figures and the cost estimate. Each number is
taken from the JSON or from a constant below whose source is given."""

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Modal prices, https://modal.com/pricing (read 2026-11-10): per second.
L4_PER_S = 0.000222
CPU_CORE_PER_S = 0.0000131
MEM_GIB_PER_S = 0.00000222

# CPU baselines, checkpoint/RESULTS.md (Ryzen 9 7940HS, 8 cores/16 threads, r0vm 3.0.5,
# CPU only). Only cp-empty-1 was proven (twice, image 0xbbf58fed…34c0, 3,704,942 user cycles, same 4
# segments); the others are that file's extrapolation at 289 s per segment (1,157.5 s / 4).
CPU = {
    "cp-empty-1": ("1,157.5 s quiet start / 1,758.9 s loaded (measured)", 1157.5),
    "cp-empty-360": ("≈ 2,023 s (7 × 289 s, extrapolated)", 7 * 289.4),
    "cp-pp5-360": ("≈ 3,473 s (12 × 289 s, extrapolated)", 12 * 289.4),
    "cp-pp50-360": ("≈ 17,363 s = 4.8 h (60 × 289 s, extrapolated)", 60 * 289.4),
}
ORDER = ["cp-empty-1", "cp-empty-360", "cp-pp5-360", "cp-pp50-360"]


def runs():
    found = {}
    for p in sorted(ROOT.glob("results/*/*/run.json")):
        r = json.loads(p.read_text())
        if r.get("ok") and r.get("prove"):
            found[r["name"]] = r  # the latest successful run of each fixture
    return found


def lez_runs():
    """The LEZ jobs: `lez-update` (the light-client update) and `lez-attest` (one private attestation)."""
    return [json.loads(p.read_text()) for p in sorted(ROOT.glob("results/*/lez-*/run.json"))]


def main():
    rs = runs()
    if not rs:
        sys.exit("no successful runs under results/")
    print("| fixture | user cycles | segments | CPU, laptop | GPU (L4) total | execute | segment proofs (mean each) | lift + join + check | speed-up | receipt | GPU memory peak | host RSS peak | GPU util. while proving |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    total_container_s = 0.0
    for name in [n for n in ORDER if n in rs] + sorted(set(rs) - set(ORDER)):
        r = rs[name]
        p, g, t = r["prove"], r["gpu_stats"], r["gnu_time"]
        cpu_text, cpu_s = CPU.get(name, ("n/a", None))
        speed = f"{cpu_s / p['total_s']:.0f}×" if cpu_s else "n/a"
        rss_gib = int(t["max_rss_kib"]) / 1024 / 1024 if t.get("max_rss_kib") else float("nan")
        print(
            f"| `{name}` | {p['user_cycles']:,} | {p['segments']} | {cpu_text} | **{p['total_s']:.1f} s** | {p['execute_s']:.1f} s "
            f"| {p['segment_proofs_s']:.1f} s ({p['segment_proofs_s'] / p['segments']:.2f} s) | {p['recursion_s']:.1f} s | {speed} "
            f"| {p['receipt_bytes']:,} B | {g['gpu_memory_used_mib_peak']:,.0f} MiB | {rss_gib:.2f} GiB "
            f"| {g.get('gpu_util_pct_mean_in_window', float('nan')):.0f}% |"
        )
        total_container_s += r["container_s_until_done"]
    print()
    for name, r in rs.items():
        segs = r["prove"]["segment_times"]
        ts = [s["prove_s"] for s in segs]
        print(f"{name}: per-segment proof {min(ts):.2f}–{max(ts):.2f} s (first {ts[0]:.2f} s), po2 {sorted({s['po2'] for s in segs})}; "
              f"GPU {r['gpu']}; image ids equal: {r['image_ids_equal']}; prover wall {r['prover_wall_s']:.1f} s; "
              f"container until done {r['container_s_until_done']:.1f} s; task {r.get('modal_task_id')}")
    for r in lez_runs():
        t, g = r["gnu_time"], r["gpu_stats"]
        rss = int(t["max_rss_kib"]) / 1024 / 1024 if t.get("max_rss_kib") else float("nan")
        print(f"\n{r['name']} ({r['run']}): ok={r['ok']} exit={r['exit_code']} wall {r['wall_s']:.1f} s (GNU time {t.get('elapsed')}, "
              f"user {t.get('user_s')} s, {t.get('cpu_percent')}), host max RSS {rss:.2f} GiB, GPU {r['gpu']}, "
              f"GPU memory peak {g.get('gpu_memory_used_mib_peak')} MiB (before {g.get('gpu_memory_used_mib_before')}), "
              f"GPU busy {g.get('gpu_busy_seconds'):.1f} s, mean util {g.get('gpu_util_pct_mean_in_window', float('nan')):.0f}%, "
              f"tx {r['tx_bytes']} B; container until done {r['container_s_until_done']:.1f} s")
        for line in r["log_lines"]:
            print("   ", line)
        total_container_s += r["container_s_until_done"]
    cores, mem_gib = next(iter(rs.values()))["container_cpu_cores"], next(iter(rs.values()))["container_memory_mib"] / 1024
    per_s = L4_PER_S + cores * CPU_CORE_PER_S + mem_gib * MEM_GIB_PER_S
    print(f"\nGPU containers: {total_container_s:.0f} s from function start to done (startup before the function and teardown are not in this)")
    print(f"  × (L4 {L4_PER_S} + {cores:g} cores × {CPU_CORE_PER_S} + {mem_gib:g} GiB × {MEM_GIB_PER_S}) = {per_s:.7f} $/s"
          f" → ${total_container_s * per_s:.2f} (GPU part alone ${total_container_s * L4_PER_S:.2f})")


if __name__ == "__main__":
    main()
