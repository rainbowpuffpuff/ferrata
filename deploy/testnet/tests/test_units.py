#!/usr/bin/env python3
"""The host's systemd units, read as files: the relayer's inbox and state on one mount, the prover client writing into
that inbox, and every bridge unit sandboxed to its own directories. Where the system allows a rootless mount namespace
(`unshare -rm`), also the reason: rename(2) between two bind mounts of one filesystem fails with EXDEV, within one it
works (the relayer moves each finished job from its inbox into its state with rename; across two mounts the round
fails before its finalisations, every round).
Run: python3 -I deploy/testnet/tests/test_units.py"""
import pathlib
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
UNITS = HERE.parent / "host" / "systemd"
FAILED = []


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f": {detail}" if detail and not ok else ""))
    if not ok:
        FAILED.append(name)


def unit(name):
    """{key: [values]} of the [Service] section (repeated keys kept in order)."""
    out, section = {}, None
    for line in (UNITS / name).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("["):
            section = line
            continue
        if section == "[Service]" and "=" in line:
            k, v = line.split("=", 1)
            out.setdefault(k, []).append(v)
    return out


def arg(cmd, flag):
    words = shlex.split(cmd)
    return words[words.index(flag) + 1] if flag in words else None


def mount_of(path, binds):
    """The BindPaths entry that holds `path` (the longest prefix), or None."""
    hits = [b for b in binds if path == b or path.startswith(b.rstrip("/") + "/")]
    return max(hits, key=len) if hits else None


relayer = unit("ferrata-bridge-relayer.service")
prover = unit("ferrata-bridge-prover.service")
r_cmd = relayer["ExecStart"][0]
r_binds = " ".join(relayer.get("BindPaths", [])).split()
inbox, state = arg(r_cmd, "--inbox"), arg(r_cmd, "--state")
check("the relayer names an inbox and a state", bool(inbox and state), r_cmd)
check("the relayer's inbox is under a BindPaths entry", mount_of(inbox, r_binds) is not None, f"{inbox} {r_binds}")
check("the relayer's inbox and state are in one BindPaths entry (one mount: rename works)",
      mount_of(inbox, r_binds) is not None and mount_of(inbox, r_binds) == mount_of(state, r_binds),
      f"inbox {mount_of(inbox, r_binds)}, state {mount_of(state, r_binds)}")
check("the relayer refuses to start with its inbox on another mount than its state",
      any("stat -c %%m" in v and inbox in v and state in v for v in relayer.get("ExecStartPre", [])))
p_cmd = prover["ExecStart"][0]
p_binds = " ".join(prover.get("BindPaths", [])).split()
check("the prover client writes into the relayer's inbox", arg(p_cmd, "--inbox") == inbox, p_cmd)
check("the prover client's namespace holds that inbox", mount_of(inbox, p_binds) == inbox, f"{p_binds}")
check("the prover client sees no other part of the relayer's directory",
      not any(b != inbox and mount_of(b, [str(pathlib.PurePosixPath(inbox).parent)]) for b in p_binds), f"{p_binds}")
for name in sorted(p.name for p in UNITS.glob("ferrata-bridge-*.service")):
    u = unit(name)
    if "User" not in u:
        continue  # a root unit (none of the bridge's long-running services)
    hidden = "/var/lib/ferrata:ro" in u.get("TemporaryFileSystem", [])
    write_limited = u.get("ProtectSystem") == ["strict"] and bool(u.get("ReadWritePaths"))
    check(f"{name}: sandboxed (hides /var/lib/ferrata but its own paths, or writes only its ReadWritePaths)",
          hidden or write_limited, f"{u.get('TemporaryFileSystem')} {u.get('ProtectSystem')} {u.get('ReadWritePaths')}")

# Every log a unit appends to is created by install.sh owned by the user logrotate runs as.
import re  # noqa: E402
install = (HERE.parent / "install.sh").read_text()
m = re.search(r'LOGS="([^"]*)"', install)
listed = set(m.group(1).split()) if m else set()
appended = set()
for f in UNITS.glob("*.service"):
    appended |= set(re.findall(r"append:/var/log/ferrata/([a-z0-9-]+)\.log", f.read_text()))
check("install.sh creates every unit's log owned by ferrata (logrotate su ferrata)", appended <= listed,
      f"missing: {sorted(appended - listed)}")
check("logrotate runs as ferrata with copytruncate", "su ferrata ferrata" in (HERE.parent / "host" / "logrotate").read_text())

# The node's API: refused to every local user but the ones that use it (L: the node's keys are LEZ's published ones).
import subprocess as _sp  # noqa: E402
guard = _sp.run(["bash", str(HERE.parent / "host" / "bin" / "ferrata-node-api-guard"), "--print"], capture_output=True, text=True).stdout
check("the node API guard rejects port 18080 for other uids (root always allowed)",
      "tcp dport 18080 meta skuid != { 0" in guard and "reject with tcp reset" in guard and "ip6 daddr ::1" in guard, guard)
check("install.sh starts the guard before the node", install.find("ferrata-node-api-guard.service") != -1
      and install.find("ferrata-node-api-guard.service") < install.find("enable --now ferrata-node.service"))

# The mechanism, where a rootless mount namespace can be made (a laptop or VM; not every container allows it).
if shutil.which("unshare") and subprocess.run(["unshare", "-rm", "true"], capture_output=True).returncode == 0:
    with tempfile.TemporaryDirectory(dir=HERE) as d:
        d = pathlib.Path(d)
        for sub in ("real-relayer/state", "real-relayer/inbox", "real-inbox", "m-relayer", "m-inbox"):
            (d / sub).mkdir(parents=True)
        probe = ("import os, sys\n"
                 "open(sys.argv[1], 'w').close()\n"
                 "try:\n    os.rename(sys.argv[1], sys.argv[2]); print('renamed')\n"
                 "except OSError as e:\n    print('errno', e.errno)\n")

        def run(binds, src, dst):
            mounts = " && ".join(f"mount --bind {shlex.quote(str(a))} {shlex.quote(str(b))}" for a, b in binds)
            cmd = f"{mounts} && {shlex.quote(sys.executable)} -I -c {shlex.quote(probe)} {shlex.quote(str(src))} {shlex.quote(str(dst))}"
            return subprocess.run(["unshare", "-rm", "sh", "-c", cmd], capture_output=True, text=True).stdout.strip()

        two = run([(d / "real-relayer", d / "m-relayer"), (d / "real-inbox", d / "m-inbox")],
                  d / "m-inbox" / "job.json", d / "m-relayer" / "state" / "job.json")
        check("rename between two bind mounts of one filesystem fails with EXDEV (18)", two == "errno 18", two)
        one = run([(d / "real-relayer", d / "m-relayer")],
                  d / "m-relayer" / "inbox" / "job.json", d / "m-relayer" / "state" / "job.json")
        check("rename within one bind mount works", one == "renamed", one)
else:
    print("skip the mount-namespace checks: `unshare -rm` is not allowed here")

print(f"{'FAILED ' + str(len(FAILED)) if FAILED else 'all passed'}")
sys.exit(1 if FAILED else 0)
