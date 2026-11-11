#!/usr/bin/env python3
"""Add the requests that check.sh sends to ferrata_bridge_module to OUT_DIR, from the bridge
specification's vectors (bridge/vectors/*-v2.json, the vectors the bridge core, the program, the
guests and the vault are tested against).

    python3 -I modules/scripts/bridge_requests.py OUT_DIR

Appends to OUT_DIR/calls.json and OUT_DIR/expect.json (written first by requests.py); every call
here carries `"module": "ferrata_bridge_module"` and a name starting with `bridge_`.
"""

import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
VECTORS = REPO / "bridge/vectors"
MODULE = "ferrata_bridge_module"


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    calls = json.loads((out / "calls.json").read_text()) if (out / "calls.json").exists() else []
    expect = json.loads((out / "expect.json").read_text()) if (out / "expect.json").exists() else {}
    inbound = json.loads((VECTORS / "inbound-v2.json").read_text())
    outbound = json.loads((VECTORS / "outbound-v2.json").read_text())

    def add(name, method, request, want):
        file = f"{name}.json" if request is not None else ""
        if request is not None:
            (out / file).write_text(json.dumps(request, indent=1))
        calls.append({"name": name, "method": method, "request": file, "module": MODULE})
        expect[name] = want

    add("bridge_version", "version", None, {"module": MODULE, "spec": "v2"})

    # Deposits: every inbound case (both destination kinds), commitment to nullifier.
    for k, c in enumerate(inbound["cases"]):
        req = {key: c[key] for key in ("bridge_seed", "evm_chain_id", "vault", "depositor", "j", "kind", "dest", "token", "leaf_index", "amount")}
        add(f"bridge_deposit_case{k}", "deposit", req,
            {"verdict": "ok", "commitment": c["C"], "leaf": c["leaf"], "deposit_id": c["fid"], "nullifier": c["N"]})

    # The claim vector (a deposit tree of five leaves), and the same claim with another amount.
    cl = inbound["claim"]
    req = {key: cl[key] for key in ("bridge_seed", "evm_chain_id", "vault", "depositor", "j", "leaf_index", "token", "amount", "kind", "dest", "leaves", "root")}
    add("bridge_claim_vector", "claim", req, {"verdict": "claimable", "nullifier": cl["N"], "root": cl["root"],
                                              "path.directions": cl["path"]["directions"]})
    bad = dict(req, amount="0x" + "00" * 31 + "01")
    add("bridge_claim_wrong_amount", "claim", bad, {"verdict": "refused", "error": "RootMismatch"})

    # Burns: every outbound case.
    for k, c in enumerate(outbound["cases"]):
        req = {key: c[key] for key in ("bridge_seed", "config_id", "i", "evm_chain_id", "vault", "recipient", "token", "amount", "burn_index")}
        add(f"bridge_burn_case{k}", "burn", req,
            {"note.r": c["r"], "note.memo": c["memo"], "note.mac": c["mac"], "note.c_out": c["C_out"],
             "burn_leaf": c["burn_leaf"], "burn_id": c["burn_id"], "nullifier": c["N_out"]})

    # The release vector (a burn tree of six leaves): nullifier, root and the vault's journal.
    rl = outbound["release"]
    req = {key: rl[key] for key in ("bridge_seed", "config_id", "i", "burn_index", "evm_chain_id", "vault", "recipient", "token", "amount", "relayer", "relay_fee", "leaves", "burn_root")}
    add("bridge_release_vector", "release", req, {"verdict": "releasable", "nullifier": rl["N_out"], "burn_root": rl["burn_root"],
                                                  "journal": rl["journal"], "journal_sha256": rl["journal_sha256"],
                                                  "path.directions": rl["path"]["directions"]})
    other = dict(req, recipient="0x" + "11" * 20)
    add("bridge_release_wrong_recipient", "release", other, {"verdict": "refused", "error": "RootMismatch"})

    # Recovery: the wallet of inbound case 0 among its own deposit and another wallet's.
    c0, c1 = inbound["cases"][0], inbound["cases"][1]
    deposit = lambda c: {"depositor": c["depositor"], "j": c["j"], "leaf_index": c["leaf_index"], "token": c["token"], "amount": c["amount"], "commitment": c["C"]}
    req = {"bridge_seed": c0["bridge_seed"], "evm_chain_id": c0["evm_chain_id"], "vault": c0["vault"], "gap": 4,
           "destinations": [{"kind": c0["kind"], "index": 0, "dest": c0["dest"]}],
           "deposits": [deposit(c1), deposit(c0)]}
    add("bridge_recover", "recover", req, {"verdict": "ok", "deposits_scanned": 2, "deposits.0.nullifier": c0["N"],
                                           "deposits.0.leaf_index": c0["leaf_index"]})

    # Status: one deposit claimed, one claimable, one awaiting a root; one burn released, one
    # releasable, one awaiting a checkpoint.
    n = lambda b: "0x" + b * 32
    req = {"imported_leaf_count": 2, "spent_nullifiers": [n("a1")], "checkpointed_burn_count": 2, "released_nullifiers": [n("b1")],
           "deposits": [{"leaf_index": 0, "nullifier": n("a1")}, {"leaf_index": 1, "nullifier": n("a2")}, {"leaf_index": 2, "nullifier": n("a3")}],
           "burns": [{"burn_index": 0, "nullifier": n("b1")}, {"burn_index": 1, "nullifier": n("b2")}, {"burn_index": 5, "nullifier": n("b3")}]}
    add("bridge_status", "status", req, {"deposits.0.status": "claimed", "deposits.1.status": "claimable", "deposits.2.status": "awaiting_root",
                                         "burns.0.status": "released", "burns.1.status": "releasable", "burns.2.status": "awaiting_checkpoint"})

    (out / "calls.json").write_text(json.dumps(calls, indent=1))
    (out / "expect.json").write_text(json.dumps(expect, indent=1))
    print(f"bridge_requests.py: {sum(1 for c in calls if c.get('module') == MODULE)} calls for {MODULE}")


if __name__ == "__main__":
    main()
