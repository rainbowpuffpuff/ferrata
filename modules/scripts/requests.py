#!/usr/bin/env python3
"""Write the requests that check.sh sends to ferrata_attest_module, from the repository's fixtures.

    python3 -I modules/scripts/requests.py OUT_DIR

Each file is one method's argument (JSON). `expect.json` says what each reply must hold; check.sh
tests the module's replies (through Logos Core) and the native replies against it.

Fixtures: crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10 (one finalized mainnet block,
26384851, its eth_getProof responses and the checkpoint's bootstrap; SOURCE.md lists every request)
and mainnet-p1899 (the bootstrap of the lez/ harness's mainnet instance).
"""

import copy
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
FIXTURES = REPO / "crates/ferrata-spectests/fixtures"
ATTEST = FIXTURES / "mainnet-attest-2026-11-10"
P1899 = FIXTURES / "mainnet-p1899"


def load(path):
    data = json.loads(path.read_text())
    return data["result"] if isinstance(data, dict) and "jsonrpc" in data else data


def mainnet_config(parts):
    """`mainnet_config(parts)` and `mainnet_spec()` of lez/harness/src/lib.rs."""
    far = 2**64 - 1
    return {
        "chain_id": 1,
        "sync_committee_size": 512,
        "slots_per_epoch": 32,
        "epochs_per_sync_committee_period": 256,
        "min_sync_committee_participants": 1,
        "fork_versions": ["0x%02x000000" % i for i in range(9)],
        "fork_epochs": [0, 74240, 144896, 194048, 269568, 364032, 411392, far, far],
        "genesis_validators_root": "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95",
        "genesis_time": 1606824023,
        "seconds_per_slot": 12,
        "store_fork": 3,
        "adoption_parts": parts,
        "ws_period_epochs": 3532,  # the harness's test_ws_period_epochs() (part of the instance id since lc-a1)
    }


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    requests, expect = {}, {}

    fx = load(ATTEST / "fixture.json")
    block = load(ATTEST / "block_26384851.json")
    # The fixture's block is the finalized header's execution payload (SOURCE.md's cross-checks).
    assert int(block["number"], 16) == fx["block"]
    assert block["hash"] == fx["block_hash"]
    assert block["stateRoot"] == fx["state_root"]
    assert block["receiptsRoot"] == fx["receipts_root"]

    # The record the program writes when its instance verifies the finality update of the fixture
    # (lez/harness/src/attest.rs, Stage 4): anchored at the finalized slot, holding the block's fields.
    record = {
        "chain_id": fx["chain_id"],
        "anchor_slot": fx["finalized_slot"],
        "block_number": fx["block"],
        "timestamp": block["timestamp"],
        "block_hash": fx["block_hash"],
        "parent_hash": block["parentHash"],
        "state_root": fx["state_root"],
        "receipts_root": fx["receipts_root"],
    }
    weth = load(ATTEST / "proof_weth_26384851.json")
    weth_value = next(e["value"] for e in weth["storageProof"] if e["key"] == fx["weth_balance_slot"])

    def storage(slot, get_proof, chain_id=1):
        return {"chain_id": chain_id, "record": record, "storage": {"slot": slot, "get_proof": get_proof}}

    requests["attest_weth_balance"] = ("dry_run_attest", storage(fx["weth_balance_slot"], weth))
    expect["attest_weth_balance"] = {"verdict": "attested", "code": 0, "provider_value_matches": True,
                                     "fact.value": "0x%064x" % int(weth_value, 16)}
    requests["attest_empty_slot"] = ("dry_run_attest", storage(fx["empty_slot"], weth))
    expect["attest_empty_slot"] = {"verdict": "attested", "code": 0, "fact.value": "0x" + "00" * 32}
    requests["attest_absent_account"] = ("dry_run_attest", storage("0x" + "00" * 32, load(ATTEST / "proof_absent_26384851.json")))
    expect["attest_absent_account"] = {"verdict": "attested", "code": 0, "fact.value": "0x" + "00" * 32}

    # Refusals. One nibble changed in the account's leaf node.
    tampered = copy.deepcopy(weth)
    leaf = tampered["accountProof"][-1]
    k = len(leaf) // 2
    tampered["accountProof"][-1] = leaf[:k] + ("1" if leaf[k] == "0" else "0") + leaf[k + 1:]
    requests["refuse_tampered_proof"] = ("dry_run_attest", storage(fx["weth_balance_slot"], tampered))
    expect["refuse_tampered_proof"] = {"verdict": "refused", "stage": "proof"}
    # The same slot proven at another block (26384835) against this block's record.
    other = load(ATTEST / "proof_weth_26384835.json")
    requests["refuse_other_block"] = ("dry_run_attest", storage(fx["weth_balance_slot"], other))
    expect["refuse_other_block"] = {"verdict": "refused", "stage": "proof"}
    # A consumer configured for another chain (Sepolia's id) than the record's instance.
    requests["refuse_wrong_chain"] = ("dry_run_attest", storage(fx["weth_balance_slot"], weth, chain_id=11155111))
    expect["refuse_wrong_chain"] = {"verdict": "refused", "code": 140, "error": "ChainMismatch"}

    # Checkpoints. The lez/ harness's mainnet instance (World::new(2)): its id is in the program's own
    # transactions (lez/results/stage3-read-tx.hex; prefix in lez/results/prove-private-dry-run.log),
    # and a second provider's root for its slot is EXPECTED_FINALIZED[0] in lez/harness/src/lib.rs.
    p1899_root = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"
    requests["checkpoint_p1899"] = ("verify_checkpoint", {
        "config": mainnet_config(2),
        "checkpoint": p1899_root,
        # The id with lc-a1's ws_period_epochs in the configuration (e3ba34e updated the configuration and the
        # calls crate's test, instance_id_equals_the_programs, but not this one); 0xce624027…afdfb was
        # integration6's.
        "expected_instance_id": "0x39860f39c97574b5eed493bdfce3429de1b8ff1b1f2a2452e72292460953b79c",
        "bootstrap": "0x" + (P1899 / "bootstrap.ssz").read_bytes().hex(),
        "observed": [{"source": "second beacon provider, slot 15556608 (lez/harness EXPECTED_FINALIZED)",
                      "root": p1899_root}],
    })
    expect["checkpoint_p1899"] = {"verdict": "ok", "instance_id_matches": True, "bootstrap.code": 0,
                                  "chain_pair.status": "known"}

    # The attestation fixture's checkpoint, with the root a second beacon provider reports for its slot.
    second = load(ATTEST / "beacon_header_15622144_second.json")["data"]
    assert int(second["header"]["message"]["slot"]) == fx["bootstrap_slot"]
    observed = [{"source": "ethereum-beacon-api.publicnode.com, /eth/v1/beacon/headers/15622144 "
                           "(beacon_header_15622144_second.json)", "root": second["root"]}]
    bootstrap = "0x" + (ATTEST / "bootstrap.ssz").read_bytes().hex()
    requests["checkpoint_attest_fixture"] = ("verify_checkpoint", {
        "config": mainnet_config(2), "checkpoint": fx["bootstrap_root"], "bootstrap": bootstrap,
        "observed": observed,
    })
    expect["checkpoint_attest_fixture"] = {"verdict": "ok", "bootstrap.code": 0, "observed_agreeing": 1}
    # The wrong checkpoint for that bootstrap (the finalized root of the update instead).
    requests["checkpoint_wrong_root"] = ("verify_checkpoint", {
        "config": mainnet_config(2), "checkpoint": fx["finalized_root"], "bootstrap": bootstrap,
        "observed": observed,
    })
    expect["checkpoint_wrong_root"] = {"verdict": "failed", "bootstrap.code": 10,
                                       "bootstrap.error": "CheckpointMismatch", "observed_agreeing": 0}
    # Sepolia's chain id with mainnet's genesis_validators_root.
    mixed = mainnet_config(2)
    mixed["chain_id"] = 11155111
    requests["checkpoint_chain_mismatch"] = ("verify_checkpoint", {"config": mixed, "checkpoint": p1899_root})
    expect["checkpoint_chain_mismatch"] = {"verdict": "failed", "chain_pair.status": "mismatch"}

    calls = []
    for name, (method, request) in requests.items():
        (out / f"{name}.json").write_text(json.dumps(request) + "\n")
        calls.append({"name": name, "method": method, "request": f"{name}.json"})
    (out / "calls.json").write_text(json.dumps(calls, indent=1) + "\n")
    (out / "expect.json").write_text(json.dumps(expect, indent=1) + "\n")
    print(f"requests.py: {len(calls)} requests")


if __name__ == "__main__":
    main()
