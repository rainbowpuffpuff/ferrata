#!/usr/bin/env python3
"""Samples processes every --every seconds until SIGTERM: CPU seconds (utime+stime), resident and peak memory
(VmRSS, VmHWM), bytes read and written through syscalls (rchar, wchar), into a CSV (bench/perf: a daemon's
resources). --pid NAME=PID, repeatable.
  python3 -I proc_sample.py --pid honest-1=1234 --pid honest-2=1240 --out samples.csv --every 5"""
import argparse, os, signal, sys, time

def read(pid):
    with open(f"/proc/{pid}/stat") as f:
        st = f.read().rsplit(")", 1)[1].split()
    tick = os.sysconf("SC_CLK_TCK")
    cpu = (int(st[11]) + int(st[12])) / tick
    rss = hwm = 0
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith("VmRSS:"):
                rss = int(line.split()[1])
            elif line.startswith("VmHWM:"):
                hwm = int(line.split()[1])
    io = {}
    try:
        with open(f"/proc/{pid}/io") as f:
            for line in f:
                k, v = line.split(":")
                io[k] = int(v)
    except OSError:
        pass
    return cpu, rss, hwm, io.get("rchar", 0), io.get("wchar", 0)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pid", action="append", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--every", type=float, default=5)
    a = ap.parse_args()
    pids = [p.split("=", 1) for p in a.pid]
    out = open(a.out, "w")
    out.write("unix\tname\tpid\tcpu_s\trss_kb\thwm_kb\trchar\twchar\n")
    signal.signal(signal.SIGTERM, lambda *_: (out.flush(), os._exit(0)))
    while True:
        t = time.time()
        for name, pid in pids:
            try:
                cpu, rss, hwm, rc, wc = read(pid)
                out.write(f"{t:.1f}\t{name}\t{pid}\t{cpu:.2f}\t{rss}\t{hwm}\t{rc}\t{wc}\n")
            except OSError:
                pass
        out.flush()
        time.sleep(a.every)

if __name__ == "__main__":
    sys.exit(main())
