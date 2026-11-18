#!/usr/bin/env python3
"""drive.py's automatic resume (e2e/NETWORK.md), without a chain: fake stages, sleep recorded instead of slept.

  A  a stage stopping twice for a transient reason (once a read, once sepolia.sh's exit 75) is resumed twice, with
     pauses of 120 s and 240 s +-25 %, each in crown.log and run.json "auto_resumes"; the run then ends DONE (0);
  B  a permanent stop (any other exit status) stops at once (1), nothing resumed;
  C  a stage that stops transiently every time is resumed CROWN_AUTO_RESUMES (4) times, then stops (1);
  D  drive.py's own proxy (start_proxy) in front of a local anvil: its cast reads go through it (chain id, the pinned
     block), the environment names it for sepolia.sh, and its journal directory is <run>/net.
"""
import importlib.util
import json
import os
import shutil
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
NAME = "netfault-autoresume"
RUN = f"{REPO}/e2e/runs/{NAME}"
SLEPT = []


def load():
    shutil.rmtree(RUN, ignore_errors=True)
    os.makedirs(RUN)
    with open(f"{RUN}/run.json", "w") as f:
        json.dump({"mode": "fork", "deployer": "0x" + "11" * 20}, f)
    sys.argv = ["drive.py", NAME, "fork"]
    spec = importlib.util.spec_from_file_location("drive", f"{REPO}/e2e/crown/cloud/drive.py")
    d = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(d)
    for k in ("prepare_inputs", "start_anvil", "stop_anvil", "ensure_proxy", "stop_proxy"):
        setattr(d, k, lambda: None)
    d.lez_chain = lambda: "lez"
    class T:  # the time module, with sleep recorded instead of slept
        sleep = staticmethod(SLEPT.append)

        def __getattr__(self, k):
            return getattr(time, k)

    d.time = T()
    return d


def scenario(steps):
    SLEPT.clear()
    d = load()
    d.STAGES = list(steps)
    d.STEPS = steps
    rc = d.main()
    log = open(f"{RUN}/crown.log").read()
    return rc, json.load(open(f"{RUN}/run.json")).get("auto_resumes", []), log, list(SLEPT), d


def main():
    # A: two transient stops, then done
    d = load()
    n = {"a": 0}

    def a():
        n["a"] += 1
        if n["a"] == 1:
            d.die("cast call failed: header not found", transient=True)  # a read, as out() raises it
        if n["a"] == 2:
            d.run(["bash", "-c", "exit 75"], log="fake.log")  # sepolia.sh: transient, nothing in flight
        return True

    SLEPT.clear()
    d.STAGES, d.STEPS = ["a", "b"], {"a": a, "b": lambda: True}
    rc = d.main()
    rj = json.load(open(f"{RUN}/run.json"))
    log = open(f"{RUN}/crown.log").read()
    assert rc == 0, rc
    assert len(rj["auto_resumes"]) == 2 and "auto-resume 1 of 4" in log and "auto-resume 2 of 4" in log, log
    assert 90 <= SLEPT[0] <= 150 and 180 <= SLEPT[1] <= 300, SLEPT
    print(f"A: two transient stops resumed (pauses {[round(x) for x in SLEPT]} s), DONE: ok")

    # B: a permanent stop
    d = load()
    SLEPT.clear()
    d.STAGES, d.STEPS = ["a"], {"a": lambda: d.run(["bash", "-c", "exit 1"], log="fake.log")}
    rc = d.main()
    rj = json.load(open(f"{RUN}/run.json"))
    assert rc == 1 and not rj.get("auto_resumes") and not SLEPT, (rc, rj)
    print("B: a permanent stop (exit 1) stops at once, nothing resumed: ok")

    # C: transient every time
    d = load()
    SLEPT.clear()
    d.STAGES, d.STEPS = ["a"], {"a": lambda: d.run(["bash", "-c", "exit 75"], log="fake.log")}
    rc = d.main()
    rj = json.load(open(f"{RUN}/run.json"))
    log = open(f"{RUN}/crown.log").read()
    assert rc == 1 and len(rj["auto_resumes"]) == 4 and "automatic resumes used up" in log, (rc, rj)
    print(f"C: transient every time: resumed 4 times (pauses {[round(x) for x in SLEPT]} s), then stopped: ok")

    # D: the run's proxy, for real, in front of a local anvil
    import socket
    import subprocess
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    anvil = subprocess.Popen(["anvil", "--port", str(port), "--chain-id", "11155111"], stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            if subprocess.run(["cast", "chain-id", "--rpc-url", f"http://127.0.0.1:{port}"],
                              capture_output=True).returncode == 0:
                break
            time.sleep(0.1)
        os.environ.update(SEPOLIA_RPC_URL=f"http://127.0.0.1:{port}",
                          ETH_FROM="0x70997970C51812dc3A010C7d01b50e0d17dc79C8")
        for k in ("PUBNET_URL", "PUBNET_DIR"):
            os.environ.pop(k, None)
        d = load()
        d.time = time  # the real clock here: start_proxy waits for the port file
        d.start_proxy()
        try:
            cid, head = d.cast("chain-id"), d.cast("block-number")
            env_ok = os.environ["PUBNET_URL"].startswith("http://127.0.0.1:") and os.environ["PUBNET_DIR"] == f"{RUN}/net"
            assert cid == "11155111" and head == "0" and env_ok, (cid, head, env_ok)
            assert d.rpc_env()["ETH_RPC_URL"] == os.environ["PUBNET_URL"] + "/drive"
            print(f"D: drive.py's proxy {os.environ['PUBNET_URL']}: chain {cid}, pinned block {head}, journal net/: ok")
        finally:
            d.PROXY.terminate()
            d.PROXY.wait(timeout=10)
    finally:
        anvil.terminate()
        anvil.wait(timeout=10)
    shutil.rmtree(RUN, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
