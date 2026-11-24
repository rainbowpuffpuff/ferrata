#!/usr/bin/env python3
"""Fetch the mainnet fixture of the attestation tests (`lez/harness/tests/attest_*.rs`).

The program records a finalized header only after it has verified it, so the fixture is one
finalized mainnet block together with what proves it and what is proven under it:

1. the latest light-client finality update (SSZ) from a beacon node; it finalizes execution block
   `n`;
2. while the public nodes still hold the state of `n` (most keep about 128 blocks, and a finalized
   block is already 64 to 95 blocks old): `eth_getProof` at `n` for WETH (the balance slot of one
   holder and an empty slot), for an address without an account, and for the EIP-2935 history
   contract (the slot of `m = n - 16`); `eth_getProof` at `m` for the same WETH slot;
   `eth_call balanceOf` at both blocks as a cross-check;
3. `eth_getBlockReceipts` for `n` (the tests rebuild the receipts trie from it), and from it the
   logs the tests attest: an ERC-20 `Transfer` whose recipient is a contract (`eth_getCode` at `n`),
   another `Transfer` of the same token to another recipient, a `Transfer` of another token, a
   `Transfer` in the receipt with the most logs, and a failed transaction;
4. the block headers of `n` and `m`, rebuilt as RLP from `eth_getBlockByNumber` and checked against
   their hashes, and the same blocks from a second execution provider;
5. a light-client bootstrap for a finalized checkpoint earlier in the same sync-committee period
   (what `Init` verifies), and the canonical roots of both beacon blocks from a second beacon
   provider.

`fixture.json` records every choice (blocks, slots, addresses, log positions); SOURCE.md lists every
request with its endpoint and JSON-RPC method; SHA256SUMS pins every file.

Python standard library only; the HTTP, Keccak, RLP and SSZ helpers are those of
`scripts/fetch-mainnet-e2e.py`. Every request goes to a public endpoint without an API key, with
the generic User-Agent of scripts/pubnet.py (`curl/8`) and nothing else that identifies the caller.

Usage: lez/scripts/fetch-attest-fixture.py OUTDIR      (OUTDIR must not exist yet)

If the finalized block has no failed transaction or no ERC-20 `Transfer` to a contract, the script
stops without writing a fixture: run it again after the next finality update (about 6 minutes).

**A Gloas network (`--network sepolia`).** From the Gloas fork on, a light-client header binds only
the hash of an execution block (the bid's parent block, EIP-7732), and an attestation carries that
block's RLP header. The script then fetches, in this order:

1. the latest finality update (SSZ, Gloas layout) from a beacon node that serves it;
2. the execution block its finalized header binds, by hash, from two execution providers; its RLP
   header, rebuilt and checked against the hash (the state and receipts roots come from it);
3. at once, while the public nodes still hold that block's state: `eth_getProof` for every
   `--storage` contract and its slots, with an `eth_call` cross-check for a slot given as
   `SLOT=getter()`; `eth_getProof` for the `--same-address` account (default Multicall3), and the
   same account on mainnet (latest block), to record that one contract has one address and one code
   hash on both chains (the chain-binding test asks a Sepolia fact as a mainnet statement);
4. `eth_getBlockReceipts` of the block, and the first ERC-20 `Transfer` in it (of `--log-token`
   if given);
5. with `--deposits VAULT:FROM_BLOCK`: the vault's `Deposit` logs from FROM_BLOCK to the block
   (`eth_getLogs`), the leaves of the deposit tree whose root is the vault's slot 0;
6. with `--updates-from P`: every period's update from P to the period before the finality
   update's (the replay fixture ends at period 1409; an instance needs these to reach the record);
7. the canonical root of the finalized beacon block from a second beacon provider.

With `--hops` (needs `--deposits`), at once with step 3: the deposits' logs, `eth_getProof` of the
EIP-2935 history contract at the block for the slot of each deposit's block (`m mod 8191`), and each
such block's RLP header (rebuilt, checked against its hash) and receipts, so that each deposit's
event can be attested one hop back from the record. With `--absent`: `eth_getProof` at the block for
an address without an account (`keccak256("ferrata/absent-account")[12..]`).

  lez/scripts/fetch-attest-fixture.py --network sepolia [--storage ADDR:SLOT[=getter()][,SLOT...]]...
      [--log-token ADDR] [--same-address ADDR] [--deposits VAULT:FROM_BLOCK] [--updates-from P]
      [--hops] [--absent] OUTDIR

SLOT is a decimal or 0x-hex slot number. Without `--storage`, Sepolia USDC's `totalSupply_`
(slot 11, checked against `totalSupply()`). `--same-address none` reads no account on both chains
(no request to mainnet). Ferrata's vault v5 (`e2e/runs/crown-live-1`):
`--storage 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54:0=depositRoot(),1=nextLeafIndex()
 --deposits 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54:12111048 --updates-from 1410 --hops --absent`
into `crates/ferrata-spectests/fixtures/sepolia-vault-v5-2026-11-11` (read by
`lez/harness/tests/vault.rs` and `vault_consumer.rs`).
"""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import sys
import urllib.error

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True  # no __pycache__ beside the imported script
_spec = importlib.util.spec_from_file_location(
    "fetch_mainnet_e2e", os.path.join(HERE, "..", "..", "scripts", "fetch-mainnet-e2e.py"))
e2e = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(e2e)
_spec = importlib.util.spec_from_file_location(
    "fetch_sepolia_gloas", os.path.join(HERE, "..", "..", "scripts", "fetch-sepolia-gloas.py"))
gloas = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gloas)

CHAIN_ID = 1
WETH = e2e.WETH
HOLDER = e2e.HOLDER  # Uniswap v2 USDC/WETH pair
WETH_BALANCE_SLOT = e2e.WETH_BALANCE_SLOT
EMPTY_SLOT = e2e.EMPTY_SLOT
ABSENT = e2e.ABSENT
HISTORY = e2e.HISTORY
HISTORY_SERVE_WINDOW = e2e.HISTORY_SERVE_WINDOW
HOP = 16
SLOTS_PER_PERIOD = e2e.SLOTS_PER_PERIOD
# keccak256("Transfer(address,address,uint256)")
TRANSFER = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
# How many distinct recipients to ask `eth_getCode` about before giving up.
MAX_CODE_PROBES = 24


def erc20_transfers(receipts):
    """(tx index, position in the receipt, log) of every ERC-20 `Transfer` of a successful
    transaction: three topics and one 32-byte data word (an ERC-721 transfer has four topics)."""
    out = []
    for r in receipts:
        if r["status"] != "0x1":
            continue
        for position, log in enumerate(r["logs"]):
            if len(log["topics"]) == 3 and log["topics"][0] == TRANSFER and len(log["data"]) == 2 + 64:
                out.append((int(r["transactionIndex"], 16), position, log))
    return out


def describe(tx_index, position, log, receipts):
    return {
        "tx_index": tx_index,
        "log_index": position,
        "block_log_index": int(log["logIndex"], 16),
        "logs_in_receipt": len(receipts[tx_index]["logs"]),
        "token": log["address"],
        "from": "0x" + log["topics"][1][26:],
        "to": "0x" + log["topics"][2][26:],
        "value": log["data"],
    }


NETWORKS = {"sepolia": 11155111}
SEPOLIA_USDC = "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"  # Circle's FiatToken proxy on Sepolia
MULTICALL3 = "0xca11bde05977b3631167028862be2a173976ca11"
# keccak256("Deposit(address,uint64,uint64,address,uint256,bytes32)"): FerrataVault's event.
DEPOSIT_EVENT = "Deposit(address,uint64,uint64,address,uint256,bytes32)"
LOG_RANGE = 10_000  # blocks per eth_getLogs request
EMPTY_CODE_HASH = "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"


def main():
    p = argparse.ArgumentParser(usage=__doc__.split("Usage: ")[1].split("\n")[0])
    p.add_argument("--network", choices=["mainnet", *NETWORKS], default="mainnet")
    p.add_argument("--storage", action="append", default=[])
    p.add_argument("--log-token")
    p.add_argument("--same-address", default=MULTICALL3)
    p.add_argument("--deposits")
    p.add_argument("--updates-from", type=int)
    p.add_argument("--hops", action="store_true")
    p.add_argument("--absent", action="store_true")
    p.add_argument("out")
    a = p.parse_args()
    if a.network == "mainnet" and (a.storage or a.log_token or a.deposits or a.updates_from or a.hops or a.absent
                                   or a.same_address != MULTICALL3):
        p.error("mainnet fetches the fixed Stage 4 fixture; the other options are for a Gloas network")
    if a.hops and not a.deposits:
        p.error("--hops hops to the blocks of the --deposits logs")
    os.makedirs(a.out)
    try:
        if a.network == "mainnet":
            fetch(a.out)
        else:
            fetch_gloas(a.out, a)
    except BaseException:
        shutil.rmtree(a.out, ignore_errors=True)  # never leave half a fixture behind
        raise


def slot_word(text):
    return e2e.word(int(text, 16) if text.lower().startswith("0x") else int(text))


def parse_storage(spec):
    """`ADDR:SLOT[=getter()][,SLOT...]` -> (address, [(slot word, getter or None)])."""
    address, _, slots = spec.partition(":")
    if len(address) != 42 or not slots:
        raise SystemExit(f"--storage {spec}: expected ADDR:SLOT[=getter()][,SLOT...]")
    out = []
    for item in slots.split(","):
        slot, _, getter = item.partition("=")
        out.append((slot_word(slot), getter or None))
    return address.lower(), out


def selector(signature):
    return "0x" + e2e.keccak256(signature.encode())[:4].hex()


def fetch_gloas(out, a):
    """The fixture of an attestation under a record in the Gloas layout (see the docstring)."""
    f = e2e.Fetcher(out)
    started = e2e.now()
    chain_id = NETWORKS[a.network]
    beacon, execution = gloas.BEACON, gloas.EXECUTION
    storage = [parse_storage(x) for x in (a.storage or [f"{SEPOLIA_USDC}:11=totalSupply()"])]

    # 1. The finality update, and 2. the bound block, by hash.
    fu_raw = f.beacon(beacon[0], "/eth/v1/beacon/light_client/finality_update", "finality_update.ssz", ssz=True)
    fu = gloas.parse_finality_update(fu_raw)
    fin = fu["finalized"]
    period = fu["signature_slot"] // SLOTS_PER_PERIOD
    if fin["slot"] // SLOTS_PER_PERIOD != period:
        raise SystemExit("finalized slot and signature slot are in different periods; retry later")
    block_hash = "0x" + fin["block_hash"]
    block, served = f.rpc_any("eth_getBlockByHash", [block_hash, False], "block.json", execution)
    n = int(block["number"], 16)
    rlp = e2e.header_rlp(block)
    if "0x" + e2e.keccak256(rlp).hex() != block_hash:
        raise SystemExit(f"rebuilt header of {block_hash} does not hash to it")
    f.save("header.rlp", rlp)
    print(f"finalized slot {fin['slot']} binds block {n} {block_hash} (period {period})", flush=True)

    # 3. State-dependent requests, at once.
    state_n = [served] + [e for e in execution if e != served]
    facts = []
    for address, slots in storage:
        proof, _ = f.rpc_any("eth_getProof", [address, [w for w, _ in slots], hex(n)], f"proof_{address}_{n}.json",
                             state_n)
        if proof["codeHash"] == EMPTY_CODE_HASH:
            raise SystemExit(f"{address} has no code at {n}")
        entry = {"address": address, "slots": []}
        for (word_, getter), sp in zip(slots, proof["storageProof"]):
            value = "0x" + int(sp["value"], 16).to_bytes(32, "big").hex()
            row = {"slot": word_, "value": value}
            if getter:
                called, _ = f.rpc_any("eth_call", [{"to": address, "data": selector(getter)}, hex(n)],
                                      f"call_{address}_{getter.split('(')[0]}_{n}.json", state_n)
                if int(called, 16) != int(value, 16):
                    raise SystemExit(f"{address} slot {word_} is {value}, {getter} returns {called}")
                row["getter"] = getter
            entry["slots"].append(row)
        facts.append(entry)
    # `--same-address none`: no account read on both chains (no mainnet request at all; CI's sequencer job).
    same = None
    if a.same_address != "none":
        same, _ = f.rpc_any("eth_getProof", [a.same_address, [], hex(n)], f"proof_{a.same_address}_{n}.json", state_n)
    absent = None
    if a.absent:
        f.rpc_any("eth_getProof", [ABSENT, [e2e.word(0)], hex(n)], f"proof_absent_{n}.json", state_n)
        absent = {"address": ABSENT, "file": f"proof_absent_{n}.json"}
    deposit_logs = None
    hops = None
    if a.hops:
        # The deposits' blocks, then EIP-2935's slots of them in the state of n, while it is served.
        deposit_logs = vault_deposits(f, a.deposits, n, state_n)
        blocks = sorted({int(x["blockNumber"], 16) for x in deposit_logs})
        if not blocks or blocks[0] < n - HISTORY_SERVE_WINDOW:
            raise SystemExit(f"deposit blocks {blocks} are not all within EIP-2935's window of {n}")
        f.rpc_any("eth_getProof", [HISTORY, [e2e.word(m % HISTORY_SERVE_WINDOW) for m in blocks], hex(n)],
                  f"proof_history_{n}.json", state_n)
        hops = {"history": HISTORY, "file": f"proof_history_{n}.json", "blocks": []}
        for m in blocks:
            older, _ = f.rpc_any("eth_getBlockByNumber", [hex(m), False], f"block_{m}.json", state_n)
            older_rlp = e2e.header_rlp(older)
            if "0x" + e2e.keccak256(older_rlp).hex() != older["hash"]:
                raise SystemExit(f"rebuilt header of block {m} does not hash to {older['hash']}")
            f.save(f"header_{m}.rlp", older_rlp)
            older_receipts, _ = f.rpc_any("eth_getBlockReceipts", [hex(m)], f"receipts_{m}.json", state_n)
            if [int(r["transactionIndex"], 16) for r in older_receipts] != list(range(len(older_receipts))):
                raise SystemExit(f"receipts of {m} are not in transaction order")
            hops["blocks"].append({"block": m, "hash": older["hash"], "slot": e2e.word(m % HISTORY_SERVE_WINDOW),
                                   "receipts_root": older["receiptsRoot"], "receipts": len(older_receipts),
                                   "header": f"header_{m}.rlp", "receipts_file": f"receipts_{m}.json"})
    head, _ = f.rpc_any("eth_blockNumber", [], None, state_n)
    print(f"proofs at {n} served; head {int(head, 16)}", flush=True)
    if same is not None:
        mainnet_block, _ = f.rpc_any("eth_blockNumber", [], None, e2e.EXECUTION)
        on_mainnet, _ = f.rpc_any("eth_getProof", [a.same_address, [], mainnet_block],
                                  f"proof_{a.same_address}_mainnet_{int(mainnet_block, 16)}.json", e2e.EXECUTION)
        if same["codeHash"] == EMPTY_CODE_HASH or same["codeHash"] != on_mainnet["codeHash"]:
            raise SystemExit(f"{a.same_address}: code hash {same['codeHash']} here, {on_mainnet['codeHash']} on mainnet")

    # 4. Receipts, and the ERC-20 Transfer the tests attest.
    receipts, _ = f.rpc_any("eth_getBlockReceipts", [hex(n)], f"receipts_{n}.json", state_n)
    if [int(r["transactionIndex"], 16) for r in receipts] != list(range(len(receipts))):
        raise SystemExit("receipts are not in transaction order")
    transfers = [t for t in erc20_transfers(receipts)
                 if a.log_token is None or t[2]["address"].lower() == a.log_token.lower()]
    if not transfers:
        raise SystemExit(f"block {n} has no ERC-20 Transfer{' of ' + a.log_token if a.log_token else ''}; "
                         "run again after the next finality update")
    transfer = describe(*transfers[0], receipts)

    # 5. The vault's deposits.
    deposits = None
    if a.deposits:
        vault, _, start = a.deposits.partition(":")
        topic = "0x" + e2e.keccak256(DEPOSIT_EVENT.encode()).hex()
        logs = deposit_logs if deposit_logs is not None else vault_deposits(f, a.deposits, n, state_n)
        f.save(f"deposits_{n}.json", (json.dumps(logs, indent=1) + "\n").encode())
        deposits = {"vault": vault.lower(), "from_block": int(start), "event": DEPOSIT_EVENT, "topic": topic,
                    "count": len(logs), "file": f"deposits_{n}.json"}

    # 6. The updates an instance needs to reach the finality update's period.
    updates = []
    if a.updates_from is not None and a.updates_from < period:
        count = period - a.updates_from
        stream = f.beacon(beacon[0], f"/eth/v1/beacon/light_client/updates?start_period={a.updates_from}&count={count}",
                          None, ssz=True)
        for i, (digest, payload) in enumerate(gloas.split_chunks(stream)):
            u = gloas.parse_update(payload)
            if u["layout"] != "gloas" or u["attested"] // SLOTS_PER_PERIOD != a.updates_from + i:
                raise SystemExit(f"update {i} of the stream is not period {a.updates_from + i} in the Gloas layout")
            f.save(f"update_{a.updates_from + i}.ssz", payload)
            updates.append(a.updates_from + i)
        if len(updates) != count:
            raise SystemExit(f"{len(updates)} updates served, {count} asked")

    # Second providers: the block, and the finalized beacon block's root.
    second, _ = f.rpc_any("eth_getBlockByHash", [block_hash, False], "block_second.json",
                          [e for e in execution if e != served])
    for field in ["hash", "number", "stateRoot", "receiptsRoot"]:
        if second[field] != block[field]:
            raise SystemExit(f"providers disagree on block {n} {field}")
    finalized_root = "0x" + e2e.beacon_header_root(fin["beacon"])
    raw = f.beacon(beacon[1], f"/eth/v1/beacon/headers/{fin['slot']}", f"beacon_header_{fin['slot']}_second.json",
                   ssz=False)
    if json.loads(raw)["data"]["root"] != finalized_root:
        raise SystemExit(f"second beacon provider reports another root for slot {fin['slot']}")

    manifest = {
        "network": a.network,
        "chain_id": chain_id,
        "period": period,
        "attested_slot": fu["attested"]["slot"],
        "signature_slot": fu["signature_slot"],
        "participants": fu["participants"],
        "finalized_slot": fin["slot"],
        "finalized_root": finalized_root,
        "block": n,
        "block_hash": block_hash,
        "state_root": block["stateRoot"],
        "receipts_root": block["receiptsRoot"],
        "receipts": len(receipts),
        "head_when_fetched": int(head, 16),
        "storage": facts,
        "same_address": None if same is None else {"address": a.same_address, "code_hash": same["codeHash"],
                                                    "mainnet_block": int(mainnet_block, 16),
                                                    "mainnet_code_hash": on_mainnet["codeHash"]},
        "transfer": transfer,
        "deposits": deposits,
        "updates": updates,
        "hops": hops,
        "absent": absent,
    }
    f.save("fixture.json", (json.dumps(manifest, indent=2) + "\n").encode())
    files = sorted(x for x in os.listdir(out) if x not in ("SOURCE.md", "SHA256SUMS"))
    sums = {x: hashlib.sha256(open(os.path.join(out, x), "rb").read()).hexdigest() for x in files}
    with open(os.path.join(out, "SHA256SUMS"), "w") as s:
        s.writelines(f"{sums[x]}  {x}\n" for x in files)
    t = transfer
    lines = [
        f"# {a.network.capitalize()} fixture of the attestation tests under a Gloas record",
        "",
        f"Fetched by `lez/scripts/fetch-attest-fixture.py --network {a.network}` between {started} and {e2e.now()}",
        f"(UTC). Every request went to a public endpoint without an API key, with User-Agent `{e2e.UA}`.",
        "Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which",
        "checked that it hashes to the bound block hash; `fixture.json` records the script's choices.",
        "`SHA256SUMS` pins every file.",
        "",
        "| | |",
        "|---|---|",
        f"| chain | {a.network}, chain id {chain_id} |",
        f"| finality update (Gloas) | attested slot {fu['attested']['slot']}, signature slot {fu['signature_slot']}, "
        f"{fu['participants']}/512 signers (period {period}) |",
        f"| finalized beacon block | slot {fin['slot']}, root {finalized_root} |",
        f"| bound execution block (the bid's parent) | {n}, hash {block_hash} |",
        f"| its state root / receipts root | {block['stateRoot']} / {block['receiptsRoot']} |",
        f"| execution head when the proofs were fetched | {int(head, 16)} |",
    ]
    for entry in facts:
        for row in entry["slots"]:
            check = f", equal to `{row['getter']}` at the block" if "getter" in row else ""
            lines.append(f"| storage | {entry['address']} slot {row['slot']} = {row['value']}{check} |")
    if same is not None:
        lines.append(f"| same address on mainnet | {a.same_address}: code hash {same['codeHash']} at {n} and at mainnet "
                     f"block {int(mainnet_block, 16)} |")
    lines += [
        f"| ERC-20 `Transfer` | tx {t['tx_index']}, log {t['log_index']} of {t['logs_in_receipt']} (`logIndex` "
        f"{t['block_log_index']}): {t['token']} from {t['from']} to {t['to']}, value {int(t['value'], 16)} |",
    ]
    if deposits:
        lines.append(f"| vault deposits | {deposits['count']} `Deposit` logs of {deposits['vault']} from block "
                     f"{deposits['from_block']} to {n} |")
    for h in (hops or {}).get("blocks", []):
        lines.append(f"| EIP-2935 hop | block {h['block']} ({h['hash']}), slot {h['slot']} of {HISTORY} at {n}; "
                     f"{h['receipts']} receipts, receipts root {h['receipts_root']} |")
    if absent:
        lines.append(f"| address without an account | {ABSENT} at {n} |")
    lines += ["", "## Requests", "", "| UTC | endpoint | request | file | note |", "|---|---|---|---|---|"]
    lines += [f"| {r['at']} | {r['endpoint']} | `{r['request']}` | {r['file'] or ''} | {r['note']} |" for r in e2e.LOG]
    with open(os.path.join(out, "SOURCE.md"), "w") as s:
        s.write("\n".join(lines) + "\n")
    print("done:", out)


def vault_deposits(f, spec, n, providers):
    """The vault's `Deposit` logs from FROM_BLOCK to `n` (`--deposits VAULT:FROM_BLOCK`)."""
    vault, _, start = spec.partition(":")
    topic = "0x" + e2e.keccak256(DEPOSIT_EVENT.encode()).hex()
    logs = []
    for lo in range(int(start), n + 1, LOG_RANGE):
        hi = min(lo + LOG_RANGE - 1, n)
        got, _ = f.rpc_any("eth_getLogs", [{"address": vault, "topics": [topic], "fromBlock": hex(lo),
                                            "toBlock": hex(hi)}], None, providers)
        logs += got
    return logs


def fetch(out):
    f = e2e.Fetcher(out)
    started = e2e.now()
    beacon, execution = e2e.BEACON, e2e.EXECUTION

    # 1. The finality update.
    fu_raw = f.beacon(beacon[0], "/eth/v1/beacon/light_client/finality_update", "finality_update.ssz", ssz=True)
    fu = e2e.parse_finality_update(fu_raw)
    fin = fu["finalized"]
    n = fin["block_number"]
    m = n - HOP
    period = fu["signature_slot"] // SLOTS_PER_PERIOD
    if fin["slot"] // SLOTS_PER_PERIOD != period:
        raise SystemExit("finalized slot and signature slot are in different periods; retry later")
    print(f"finalized slot {fin['slot']} block {n} 0x{fin['block_hash']} (period {period})", flush=True)

    # 2. State-dependent requests, the older block first.
    holder_slot = e2e.mapping_slot(HOLDER, WETH_BALANCE_SLOT)
    call = "0x70a08231" + HOLDER[2:].rjust(64, "0")
    _, at_m = f.rpc_any("eth_getProof", [WETH, [holder_slot], hex(m)], f"proof_weth_{m}.json")
    f.rpc_any("eth_call", [{"to": WETH, "data": call}, hex(m)], f"call_weth_balance_{m}.json", [at_m] + execution)
    _, at_n = f.rpc_any("eth_getProof", [WETH, [holder_slot, EMPTY_SLOT], hex(n)], f"proof_weth_{n}.json")
    state_n = [at_n] + execution
    f.rpc_any("eth_getProof", [ABSENT, [e2e.word(0)], hex(n)], f"proof_absent_{n}.json", state_n)
    f.rpc_any("eth_getProof", [HISTORY, [e2e.word(m % HISTORY_SERVE_WINDOW)], hex(n)], f"proof_history_{n}.json",
              state_n)
    f.rpc_any("eth_call", [{"to": WETH, "data": call}, hex(n)], f"call_weth_balance_{n}.json", state_n)

    # 3. Receipts of n, and the logs the tests attest.
    receipts, _ = f.rpc_any("eth_getBlockReceipts", [hex(n)], f"receipts_{n}.json")
    if [int(r["transactionIndex"], 16) for r in receipts] != list(range(len(receipts))):
        raise SystemExit("receipts are not in transaction order")
    failed = [int(r["transactionIndex"], 16) for r in receipts if r["status"] == "0x0"]
    transfers = erc20_transfers(receipts)
    if not failed:
        raise SystemExit(f"block {n} has no failed transaction; run again after the next finality update")
    is_contract = {}
    chosen = None
    for tx_index, position, log in transfers:
        to = "0x" + log["topics"][2][26:]
        if to not in is_contract:
            if len(is_contract) >= MAX_CODE_PROBES:
                break
            code, _ = f.rpc_any("eth_getCode", [to, hex(n)], None, state_n)
            is_contract[to] = code != "0x"
        if is_contract[to]:
            chosen = (tx_index, position, log)
            break
    if chosen is None:
        raise SystemExit(f"block {n} has no ERC-20 Transfer to a contract among the first recipients; run again later")
    transfer = describe(*chosen, receipts)
    f.rpc_any("eth_getCode", [transfer["to"], hex(n)], f"code_recipient_{n}.json", state_n)
    head, _ = f.rpc_any("eth_blockNumber", [], None, state_n)
    print(f"proofs at {m} and {n} served; head {int(head, 16)}", flush=True)

    def first(predicate):
        return next((describe(t, p, l, receipts) for t, p, l in transfers if predicate(t, p, l)), None)

    other_recipient = first(lambda t, p, l: l["address"] == transfer["token"]
                            and "0x" + l["topics"][2][26:] != transfer["to"])
    other_token = first(lambda t, p, l: l["address"] != transfer["token"])
    largest = max(receipts, key=lambda r: len(r["logs"]))
    largest_tx = int(largest["transactionIndex"], 16)
    in_largest = first(lambda t, p, l: t == largest_tx)
    if other_recipient is None or other_token is None or in_largest is None:
        raise SystemExit(f"block {n} lacks one of the logs the tests need; run again after the next finality update")

    # 4. Headers (rebuilt as RLP and checked), and a second execution provider.
    rebuilt, served = {}, {}
    for number in (n, m):
        block, served[number] = f.rpc_any("eth_getBlockByNumber", [hex(number), False], f"block_{number}.json")
        rlp = e2e.header_rlp(block)
        if "0x" + e2e.keccak256(rlp).hex() != block["hash"] or int(block["number"], 16) != number:
            raise SystemExit(f"rebuilt header {number} does not hash to {block['hash']}")
        f.save(f"header_{number}.rlp", rlp)
        rebuilt[number] = block
    for field, value in [("hash", fin["block_hash"]), ("stateRoot", fin["state_root"]),
                         ("receiptsRoot", fin["receipts_root"])]:
        if rebuilt[n][field] != "0x" + value:
            raise SystemExit(f"execution block {n} does not match the finalized light-client header ({field})")
    second = [e for e in execution if e != served[n]]
    for number in (n, m):
        block, _ = f.rpc_any("eth_getBlockByNumber", [hex(number), False], f"block_{number}_second.json", second)
        for field in ["hash", "stateRoot", "receiptsRoot"]:
            if block[field] != rebuilt[number][field]:
                raise SystemExit(f"providers disagree on block {number} {field}")

    # 5. Bootstrap for a checkpoint earlier in the same period, and second-provider beacon roots.
    first_epoch = period * SLOTS_PER_PERIOD // 32
    boot_slot = boot_root = None
    for epoch in range(first_epoch, fin["slot"] // 32):
        slot = epoch * 32
        try:
            hdr = json.loads(f.beacon(beacon[1], f"/eth/v1/beacon/headers/{slot}", None, ssz=False))["data"]
        except urllib.error.HTTPError:
            continue  # empty slot: no checkpoint block at the epoch start
        try:
            f.beacon(beacon[0], f"/eth/v1/beacon/light_client/bootstrap/{hdr['root']}", "bootstrap.ssz", ssz=True)
        except urllib.error.HTTPError as e:
            print(f"bootstrap for slot {slot} refused ({e.code}); trying the next epoch", flush=True)
            continue
        boot_slot, boot_root = slot, hdr["root"]
        break
    if boot_slot is None:
        raise SystemExit("no bootstrap served for this period")
    finalized_root = "0x" + e2e.beacon_header_root(fin["beacon"])
    for slot, expected in [(boot_slot, boot_root), (fin["slot"], finalized_root)]:
        raw = f.beacon(beacon[1], f"/eth/v1/beacon/headers/{slot}", f"beacon_header_{slot}_second.json", ssz=False)
        if json.loads(raw)["data"]["root"] != expected:
            raise SystemExit(f"second beacon provider reports another root for slot {slot}")

    # fixture.json: every choice, for the tests (which check each against the responses).
    manifest = {
        "chain_id": CHAIN_ID,
        "period": period,
        "bootstrap_slot": boot_slot,
        "bootstrap_root": boot_root,
        "attested_slot": fu["attested"]["slot"],
        "signature_slot": fu["signature_slot"],
        "participants": fu["participants"],
        "finalized_slot": fin["slot"],
        "finalized_root": finalized_root,
        "block": n,
        "block_hash": "0x" + fin["block_hash"],
        "state_root": "0x" + fin["state_root"],
        "receipts_root": "0x" + fin["receipts_root"],
        "receipts": len(receipts),
        "hop_block": m,
        "hop_block_hash": rebuilt[m]["hash"],
        "head_when_fetched": int(head, 16),
        "weth": WETH,
        "holder": HOLDER,
        "weth_balance_slot": holder_slot,
        "empty_slot": EMPTY_SLOT,
        "absent_account": ABSENT,
        "history_slot": e2e.word(m % HISTORY_SERVE_WINDOW),
        "transfer": transfer,
        "transfer_other_recipient": other_recipient,
        "transfer_other_token": other_token,
        "transfer_in_largest_receipt": in_largest,
        "failed_tx": failed[0],
    }
    f.save("fixture.json", (json.dumps(manifest, indent=2) + "\n").encode())

    # SOURCE.md and SHA256SUMS.
    files = sorted(x for x in os.listdir(out) if x not in ("SOURCE.md", "SHA256SUMS"))
    sums = {x: hashlib.sha256(open(os.path.join(out, x), "rb").read()).hexdigest() for x in files}
    with open(os.path.join(out, "SHA256SUMS"), "w") as s:
        s.writelines(f"{sums[x]}  {x}\n" for x in files)

    def log_row(name, t):
        return (f"| {name} | tx {t['tx_index']}, log {t['log_index']} of {t['logs_in_receipt']} in its receipt "
                f"(`logIndex` {t['block_log_index']}) | {t['token']} | {t['from']} | {t['to']} | {int(t['value'], 16)} |")

    lines = [
        "# Mainnet fixture of the attestation tests: one finalized block and facts under it",
        "",
        f"Fetched by `lez/scripts/fetch-attest-fixture.py` between {started} and {e2e.now()} (UTC). Every",
        f"request went to a public endpoint without an API key, with User-Agent `{e2e.UA}`. Responses are",
        "stored byte for byte; `header_*.rlp` are rebuilt from `block_*.json` by the script, which checked",
        "that each hashes to the block's hash; `fixture.json` records the script's choices. `SHA256SUMS`",
        "pins every file.",
        "",
        "| | |",
        "|---|---|",
        f"| chain | Ethereum mainnet, chain id {CHAIN_ID} |",
        f"| sync-committee period | {period} |",
        f"| bootstrap (the checkpoint `Init` trusts) | slot {boot_slot}, block root {boot_root} |",
        f"| finality update | attested slot {fu['attested']['slot']}, signature slot {fu['signature_slot']}, "
        f"{fu['participants']}/512 signers |",
        f"| finalized beacon block | slot {fin['slot']}, root {finalized_root} |",
        f"| finalized execution block `n` | {n}, hash 0x{fin['block_hash']} |",
        f"| `n` state root / receipts root | 0x{fin['state_root']} / 0x{fin['receipts_root']} |",
        f"| `n` receipts | {len(receipts)} ({len(failed)} failed; the first failed transaction is {failed[0]}) |",
        f"| `m` = n - {HOP} (one EIP-2935 hop from n) | {m}, hash {rebuilt[m]['hash']} |",
        f"| execution head when the proofs at n were fetched | {int(head, 16)} |",
        f"| WETH | {WETH} |",
        f"| holder (Uniswap v2 USDC/WETH pair) | {HOLDER} |",
        f"| WETH balance slot, empty slot | {holder_slot}, {EMPTY_SLOT} |",
        f"| address without an account | {ABSENT} (`keccak256(\"ferrata/absent-account\")[12..]`) |",
        f"| history contract, slot of m | {HISTORY}, {e2e.word(m % HISTORY_SERVE_WINDOW)} |",
        "",
        "Logs chosen in block `n` (ERC-20 `Transfer`: three topics, one data word; positions count from zero):",
        "",
        "| log | where | token | from | to | value |",
        "|---|---|---|---|---|---:|",
        log_row("the first `Transfer` whose recipient has code at n (`code_recipient_*.json`)", transfer),
        log_row("the first `Transfer` of that token to another recipient", other_recipient),
        log_row("the first `Transfer` of another token", other_token),
        log_row("the first `Transfer` in the receipt with the most logs", in_largest),
        "",
        "Cross-checks made by the script: the finalized light-client header's execution block hash, state",
        "root and receipts root equal those of `block_n`; a second execution provider (`block_*_second.json`)",
        "reports the same hash, state root and receipts root for n and m; a second beacon provider",
        "(`beacon_header_*_second.json`) reports the finalized block root computed from the update, and the",
        "bootstrap's block root.",
        "",
        "## Requests",
        "",
        "| UTC | endpoint | request | file | note |",
        "|---|---|---|---|---|",
    ]
    lines += [f"| {r['at']} | {r['endpoint']} | `{r['request']}` | {r['file'] or ''} | {r['note']} |" for r in e2e.LOG]
    with open(os.path.join(out, "SOURCE.md"), "w") as s:
        s.write("\n".join(lines) + "\n")
    print("done:", out)


if __name__ == "__main__":
    main()
