#!/usr/bin/env python3
"""Samples named processes and their descendants every --every seconds until SIGTERM (bench/perf: the resources of
a daemon that runs helpers, such as a Logos module's host process with the executor it starts).

  python3 -I tree_sample.py --find updater='logos_host --name ferrata_updater_module ' --pid monitor=1234 \\
      --out samples.csv --every 5

--pid NAME=PID names a process; --find NAME=TEXT names the process whose command line (arguments joined by spaces)
contains TEXT and whose program's name contains TEXT's first word (a Nix wrapper's `.NAME-wrapped` too), looked up
again at every sample until found (and again if it exits). One CSV row per name and sample:
  t            UNIX seconds
  name, pid    (pid 0 while not found)
  cpu_s        the process's own CPU seconds (utime + stime)
  cpu_reaped_s CPU seconds of its children that exited and were waited for (cutime + cstime)
  cpu_tree_s   cpu_s + cpu_reaped_s + the same two of every live descendant
  rss_kb       the process's resident memory (VmRSS); hwm_kb its peak (VmHWM; 0 where the kernel does not report it,
               as the build box's gVisor: the peak is then the largest sample)
  rss_tree_kb  resident memory of the process and its live descendants together
  procs        processes in the tree"""
import argparse, os, signal, sys, time

TICK = os.sysconf("SC_CLK_TCK")


def stat(pid):
    with open(f"/proc/{pid}/stat") as f:
        st = f.read().rsplit(")", 1)[1].split()
    # st[0] is the state (field 3): ppid is field 4, utime..cstime fields 14..17.
    return int(st[1]), (int(st[11]) + int(st[12])) / TICK, (int(st[13]) + int(st[14])) / TICK


def mem(pid):
    rss = hwm = 0
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith("VmRSS:"):
                rss = int(line.split()[1])
            elif line.startswith("VmHWM:"):
                hwm = int(line.split()[1])
    return rss, hwm


def cmdline(pid):
    with open(f"/proc/{pid}/cmdline", "rb") as f:
        return f.read().replace(b"\0", b" ").decode(errors="replace")


def snapshot():
    procs = {}
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                ppid, cpu, reaped = stat(int(d))
                procs[int(d)] = (ppid, cpu, reaped)
            except (OSError, IndexError, ValueError):
                pass
    return procs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pid", action="append", default=[], help="NAME=PID")
    ap.add_argument("--find", action="append", default=[], help="NAME=TEXT in the command line")
    ap.add_argument("--out", required=True)
    ap.add_argument("--every", type=float, default=5.0)
    a = ap.parse_args()
    fixed = {k: int(v) for k, v in (p.split("=", 1) for p in a.pid)}
    finds = dict(p.split("=", 1) for p in a.find)
    found = {}
    me = os.getpid()
    stop = []
    signal.signal(signal.SIGTERM, lambda *_: stop.append(1))
    signal.signal(signal.SIGINT, lambda *_: stop.append(1))
    new = not os.path.exists(a.out)
    with open(a.out, "a", buffering=1) as out:
        if new:
            out.write("t,name,pid,cpu_s,cpu_reaped_s,cpu_tree_s,rss_kb,hwm_kb,rss_tree_kb,procs\n")
        while not stop:
            t = time.time()
            procs = snapshot()
            children = {}
            for p, (pp, _, _) in procs.items():
                children.setdefault(pp, []).append(p)
            for name, text in finds.items():
                if found.get(name) not in procs:
                    found.pop(name, None)
                    for p in sorted(procs):
                        if p == me:
                            continue
                        try:
                            c = cmdline(p)
                        except OSError:
                            continue
                        # The program itself, not a shell or tool whose arguments mention it: its own name holds the first word.
                        if text in c + " " and text.split(" ", 1)[0] in os.path.basename(c.split(" ", 1)[0]):
                            found[name] = p
                            break
            for name in list(fixed) + list(finds):
                pid = fixed.get(name, found.get(name, 0))
                if pid not in procs:
                    out.write(f"{t:.1f},{name},0,,,,,,,0\n")
                    continue
                tree, todo = [], [pid]
                while todo:
                    p = todo.pop()
                    tree.append(p)
                    todo.extend(children.get(p, []))
                try:
                    rss, hwm = mem(pid)
                except OSError:
                    continue
                rss_tree = 0
                for p in tree:
                    try:
                        rss_tree += mem(p)[0]
                    except OSError:
                        pass
                _, cpu, reaped = procs[pid]
                cpu_tree = sum(procs[p][1] + procs[p][2] for p in tree if p in procs)
                out.write(f"{t:.1f},{name},{pid},{cpu:.2f},{reaped:.2f},{cpu_tree:.2f},{rss},{hwm},{rss_tree},{len(tree)}\n")
            slept = 0.0
            while slept < a.every and not stop:
                time.sleep(min(0.5, a.every - slept))
                slept += 0.5
    return 0


if __name__ == "__main__":
    sys.exit(main())
