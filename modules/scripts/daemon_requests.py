#!/usr/bin/env python3
"""Requests for the one-shot methods of ferrata_attestor_module, ferrata_monitor_module and
ferrata_updater_module, from records committed in the repository; check.sh calls each natively and
through Logos Core and compares the replies (summary.py).

    python3 -I modules/scripts/daemon_requests.py OUT_DIR

Appends to OUT_DIR/calls.json and OUT_DIR/expect.json (requests.py and bridge_requests.py write them
first). The attestor's requests are the zone run's (checkpoint/attestor/runs/zone: the node's saved
answers, the certificates, the attestations, the collected input the checkpoint program proved); the
monitor's are offline verdicts on a small node view and the lab record's node view; the updater's are
its configuration files. The daemons' own loops (start, status, stop) are exercised by attestor-e2e.sh,
updater-e2e.sh and monitor/tests/anvil-e2e.sh, not here.
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
ZONE = REPO / "checkpoint/attestor/runs/zone"
DEVNET = REPO.parent / "upstream/lez-v0.4.0-rc1/lez/configs/docker-all-in-one/devnet"


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    calls = json.loads((out / "calls.json").read_text()) if (out / "calls.json").exists() else []
    expect = json.loads((out / "expect.json").read_text()) if (out / "expect.json").exists() else {}

    def add(module, name, method, request, want):
        file = ""
        if request is not None:
            file = f"{name}.json"
            (out / file).write_text(json.dumps(request, indent=1))
        calls.append({"name": name, "method": method, "request": file, "module": module})
        expect[name] = want

    # ---- ferrata_attestor_module ----------------------------------------------------------------
    A = "ferrata_attestor_module"
    att = [json.loads((ZONE / f"att-{i}.json").read_text()) for i in range(4)]
    cert0 = json.loads((ZONE / "cert-0.json").read_text())
    tier = json.loads((ZONE / "tier.json").read_text())
    proven = hashlib.sha256((ZONE / "rbz-zone-n4-t3.input").read_bytes()).hexdigest()
    # The zone attestor 0's key, from its public devnet label (attestor/zone-attest.sh's keygen).
    key0 = out / "zone-attestation-0.key"
    key0.write_bytes(hashlib.sha256(b"ferrata/attestor/devnet-key/zone-attestation-0").digest())
    add(A, "attestor_version", "version", None, {"module": A})
    add(A, "attestor_tier", "tier", {"answers_dir": str(ZONE / "answers")},
        {"verdict": "ok", "tip_id": tier["tip_id"], "tip_hash": tier["tip_hash"], "blocks_checked": tier["blocks_checked"]})
    add(A, "attestor_deployment", "deployment", {"input_file": str(ZONE / "rbz-zone-n4-t3.input")},
        {"verdict": "ok", "deployment.vault": att[0]["deployment"]["vault"], "deployment.lez_config_id": att[0]["deployment"]["lez_config_id"]})
    add(A, "attestor_pubkey", "pubkey", {"key_file": str(DEVNET / "seq-0/channel_signing_key")},
        {"verdict": "ok", "public_key": cert0["channel_key"]})
    add(A, "attestor_certify", "certify", {"channel_key_file": str(DEVNET / "seq-0/channel_signing_key"),
        "att_pub": cert0["attestation_key"], "deployment_file": str(ZONE / "deployment.json")},
        {"verdict": "certified", "cert.certificate": cert0["certificate"]})
    add(A, "attestor_attest", "attest", {"deployment_file": str(ZONE / "deployment.json"), "cert_file": str(ZONE / "cert-0.json"),
        "att_key_file": str(key0), "answers_dir": str(ZONE / "answers")},
        {"verdict": "attested", "tip_id": 30, "attestation.encoded": att[0]["encoded"]})
    add(A, "attestor_verify", "verify", {"deployment_file": str(ZONE / "deployment.json"),
        "att_files": [str(ZONE / f"att-{i}.json") for i in range(4)]}, {"verdict": "verified", "refused": 0})
    # The proven input as the base: the collector replaces its attestations with the valid ones given.
    add(A, "attestor_collect_3", "collect", {"base_file": str(ZONE / "rbz-zone-n4-t3.input"),
        "att_files": [str(ZONE / f"att-{i}.json") for i in range(3)]},
        {"verdict": "collected", "n": 4, "t": 3, "input_sha256": proven})
    add(A, "attestor_collect_2", "collect", {"base_file": str(ZONE / "rbz-zone-n4-t3.input"),
        "att_files": [str(ZONE / f"att-{i}.json") for i in range(2)]}, {"verdict": "refused"})
    add(A, "attestor_assemble_2", "assemble", {"base_file": str(ZONE / "rbz-zone-n4-t3.input"),
        "att_files": [str(ZONE / f"att-{i}.json") for i in range(2)], "name": "two-of-four", "expect": "InsufficientAttestations"},
        {"verdict": "assembled", "native": "refused: InsufficientAttestations { have: 2, need: 3 }"})
    add(A, "attestor_refuse_cert", "attest", {"deployment_file": str(ZONE / "deployment.json"), "cert_file": str(ZONE / "cert-1.json"),
        "att_key_file": str(key0), "answers_dir": str(ZONE / "answers")}, {"verdict": "refused"})
    add(A, "attestor_invalid", "collect", {"att_files": []}, {"verdict": "invalid_request"})

    # ---- ferrata_monitor_module -----------------------------------------------------------------
    Mo = "ferrata_monitor_module"
    h = lambda b: "0x" + b * 32
    keys = [h(b) for b in ("a1", "a2", "a3", "a4")]
    view = {"node": "synthetic", "finalHeight": 3, "finalBlocks": {"1": h("11"), "2": h("22"), "3": h("33")},
            "keySets": [{"fromBlock": 0, "keys": keys, "postingTimeframe": 60}]}
    cand = lambda tip, tip_hash, n=4, t=3: {"newState": h("c1"), "parentState": h("c0"), "burnRoot": h("00"), "burnCount": 0,
        "importedLeafCount": 0, "sunset": False, "deltaAtSubmit": 3600, "tipId": tip, "tipHash": tip_hash, "n": n, "t": t,
        "submittedAt": 1000}
    add(Mo, "monitor_version", "version", None, {"module": Mo})
    add(Mo, "monitor_check_pass", "check", {"rule": "B", "candidate": cand(3, h("33")), "nodes": [view], "now": 1010}, {"verdict": "pass"})
    add(Mo, "monitor_check_conflict", "check", {"rule": "B", "candidate": cand(3, h("44")), "nodes": [view], "now": 1010},
        {"verdict": "cancel", "reason.kind": "tipConflict"})
    add(Mo, "monitor_check_stale_set", "check", {"rule": "B", "candidate": cand(3, h("33"), 3, 2), "nodes": [view], "now": 1010},
        {"verdict": "cancel", "reason.kind": "keySetNotLive"})
    add(Mo, "monitor_check_wait", "check", {"rule": "B", "candidate": cand(5, h("55")), "nodes": [view], "now": 1010}, {"verdict": "wait"})
    add(Mo, "monitor_node_view_lab", "node_view", {"chain_tsv_file": str(REPO / "lez/lab/results/record/chain.tsv"), "label": "lab-devnet"},
        {"verdict": "ok", "view.finalHeight": 258})
    add(Mo, "monitor_run_unbounded", "run", {"args": ["--rpc", "http://127.0.0.1:9", "--vault", "0x" + "00" * 19 + "01",
        "--node", "file:/nonexistent", "--state", str(out / "monitor-state.json")]}, {"verdict": "invalid_request"})
    add(Mo, "monitor_invalid", "check", {"rule": "C"}, {"verdict": "invalid_request"})

    # ---- ferrata_updater_module -----------------------------------------------------------------
    U = "ferrata_updater_module"
    add(U, "updater_version", "version", None, {"module": U})
    add(U, "updater_config", "check_config", {"config_file": str(REPO / "modules/ferrata_updater_module/dev/recorded.toml")},
        {"verdict": "ok", "ledger": "harness", "chains.0.name": "mainnet", "chains.1.name": "sepolia"})
    add(U, "updater_config_binary", "check_config", {"config_file": str(REPO / "lez/updater/ferrata-updater.example.toml")},
        {"verdict": "ok"})
    bad = out / "updater-bad.toml"
    bad.write_text((REPO / "modules/ferrata_updater_module/dev/recorded.toml").read_text().replace("[daemon]", "[daemon]\nunknown_field = 1", 1))
    add(U, "updater_config_unknown_field", "check_config", {"config_file": str(bad)}, {"verdict": "invalid_request"})

    # ---- ferrata_relayer_module -----------------------------------------------------------------
    # The binary's own parser and its refusals before any chain is asked; its rounds run in relayer-e2e.sh.
    Re = "ferrata_relayer_module"
    add(Re, "relayer_version", "version", None, {"module": Re, "lez_wallet": False})
    add(Re, "relayer_run_unknown_option", "run", {"args": ["--frobnicate", "status"]}, {"verdict": "invalid_request"})
    add(Re, "relayer_run_unbounded", "run", {"args": ["--eth-rpc", "http://127.0.0.1:9", "--vault", "0x" + "00" * 19 + "01",
        "--unlocked", "0x" + "00" * 19 + "02", "run", "--inbox", str(out / "relayer-inbox")]}, {"verdict": "invalid_request"})
    add(Re, "relayer_run_no_endpoint", "run", {"args": ["status"]}, {"verdict": "error", "error": "--eth-rpc is required"})
    add(Re, "relayer_run_no_vault", "run", {"args": ["--eth-rpc", "http://127.0.0.1:9", "status"]},
        {"verdict": "error", "error": "--vault is required"})

    (out / "calls.json").write_text(json.dumps(calls, indent=1))
    (out / "expect.json").write_text(json.dumps(expect, indent=1))
    print(f"daemon_requests.py: {sum(1 for c in calls if c.get('module') in (A, Mo, U, Re))} calls for {A}, {Mo}, {U}, {Re}")


if __name__ == "__main__":
    main()
