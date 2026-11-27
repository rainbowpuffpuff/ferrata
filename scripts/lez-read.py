#!/usr/bin/env python3
"""Read a LEZ v0.4.0-rc3 network from a terminal, as the test network's explorer reads it in a browser: blocks,
transactions, accounts and Ferrata's programs, each decoded and each hash recomputed. Read-only; Python 3.11+
standard library only (scripts/verifylib/lez.py and lc.py, the explorer's decoder ported).

    python3 -I scripts/lez-read.py [--lez URL] head                  the head block: number, hash (recomputed), time
    python3 -I scripts/lez-read.py [--lez URL] block N               block N: its transactions, decoded, and reverted ones
    python3 -I scripts/lez-read.py [--lez URL] tx HASH               a transaction (getTransaction): decoded, its hash
                                                                     recomputed
    python3 -I scripts/lez-read.py [--lez URL] programs              each program of the network's /programs.json and
                                                                     its loader header on chain (image id, immutable)
    python3 -I scripts/lez-read.py [--lez URL] instance ACCOUNT      a light-client instance: configuration, active,
                                                                     applied jobs, finalized header (slot, root);
                                                                     with --alarms, the status page's alarms for it
    python3 -I scripts/lez-read.py [--lez URL] account ACCOUNT PROG  an account's state under a program, decoded with
                                                                     the program's IDL where it has one
    --json prints one JSON object instead of text.

URL: the network's site (default: scripts/reproduce/deployed-programs.json's network); its RPC is URL/rpc. Messages to
Ferrata's programs are decoded with the IDLs under idl/ (attestation, bridge, names, messages); others are shown raw.
Exit 0, 1 when something read does not hold (a hash that does not match its bytes, a message that does not decode),
2 when the network cannot be read. `instance ACCOUNT --alarms` also exits 1 when an alarm is "bad" (a warning does
not: the instance is behind, not wrong).
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import lc, lez  # noqa: E402

LOADER = bytes([0xFE]) * 32


def _builtin(name: str) -> bytes:
    return lez.sha256(b"/LEE-BuiltinProgram/v1/AccountId", name.encode())


# LEZ's built-in programs (lez.js BUILTINS), and the two whose messages every block carries, decoded.
BUILTINS = {lez.b58encode(bytes(32)): "native token", lez.b58encode(LOADER): "program loader",
            **{lez.b58encode(_builtin(n)): n.replace("_", " ") for n in (
                "clock", "fee", "bridge", "sequencer_stake", "bridge_lock", "cross_zone_inbox", "cross_zone_outbox",
                "wrapped_token", "ping_sender", "ping_receiver")}}
BUILTIN_IDLS = {
    "clock": {"name": "clock", "instructions": [
        {"name": "tick", "discriminator": [0], "args": [{"name": "timestamp", "type": "u64"}, {"name": "block_id", "type": "u64"}]}]},
    "fee": {"name": "fee", "instructions": [
        {"name": "distribute", "discriminator": [0], "args": [{"name": "summary", "type": {"defined": "BlockFeeSummary"}},
                                                              {"name": "payout", "type": "u128"}, {"name": "producer", "type": "account_id"}]},
        {"name": "refund", "discriminator": [1], "args": [{"name": "amount", "type": "u128"}, {"name": "payer", "type": "account_id"}]}],
        "types": [{"name": "BlockFeeSummary", "type": {"kind": "struct", "fields": [
            {"name": "gas_used_exec", "type": "u64"}, {"name": "gas_used_stor", "type": "u64"},
            {"name": "revenue_base", "type": "u128"}, {"name": "revenue_tip", "type": "u128"}]}}]},
    "native token": {"name": "native_token", "instructions": [
        {"name": "transfer", "discriminator": [0], "args": [{"name": "to", "type": "account_id"}, {"name": "amount", "type": "u128"}]}]},
}
IDLS = {"ferrata_attestation": "idl/ferrata_attestation.idl.json", "ferrata_bridge_b1": "idl/ferrata_bridge.idl.json",
        "ferrata_names": "idl/ferrata_names.idl.json", "ferrata_msg": "idl/ferrata_msg.idl.json",
        "ferrata_msg_v2": "idl/ferrata_msg_v2.idl.json"}


def plain(v):
    """A decoded value as JSON: bytes as hex (32-byte values also in base58 when they are account ids)."""
    if isinstance(v, bytes):
        return v.hex()
    if isinstance(v, dict):
        return {k: plain(x) for k, x in v.items()}
    if isinstance(v, list):
        return [plain(x) for x in v]
    return v


# The status page's alarm rules, ported from the explorer's `alarms`
# (deploy/testnet/web/explorer/assets/attest.js; its tests are deploy/testnet/web/explorer/test/alarms.test.mjs,
# run in CI `verify`): the same thresholds, the same verdicts on the same inputs. The thresholds are the page's
# defaults; the page's config.json may override them. What this port does not compute are the rules that need the
# public APIs a browser calls per provider: each beacon API's block root at the instance's slot and each execution
# RPC's block for the record (scripts/verify.py's online check `lez-network-head` holds the instances to a beacon
# API), and whether the finalized header's record account exists (a second account read).
ALARM_THRESHOLDS = {"lag_warn_epochs": 4, "lag_bad_epochs": 64, "period_warn_hours": 12, "ws_warn_hours": 24}


def _div(a: int, b: int) -> int:
    """BigInt division: truncated toward zero, as the page's `Number(a / b)` computes it."""
    return -(-a // b) if a < 0 else a // b


def alarms(cfg: dict, slot: int, *, active: bool, next_committee: bool, now_ms: int,
           network_finalized: int | None = None, thresholds: dict | None = None) -> list[dict]:
    """The status page's alarms for one instance, from its configuration and finalized header's slot.

    `cfg` is the instance's own configuration (the IDL's `Config`, as `instance ACCOUNT` prints it); `slot` its
    finalized header's slot; `active` and `next_committee` as the instance's state holds them; `now_ms` the time to
    judge against (the wall clock); `network_finalized` Ethereum's finalized checkpoint slot when the caller has one
    (the page reads it from the public beacon APIs; without it the lag is estimated from the wall clock, two epochs
    behind it, as the page does when no provider answered). Returns [{level: ok|warn|bad, text, ...}], the page's
    levels and messages, with the numbers each verdict came from."""
    th = {**ALARM_THRESHOLDS, **(thresholds or {})}
    spe, period = cfg["slots_per_epoch"], cfg["slots_per_epoch"] * cfg["epochs_per_sync_committee_period"]

    def time_of_slot_ms(s: int) -> int:
        return (cfg["genesis_time"] + s * cfg["seconds_per_slot"]) * 1000

    out = []
    if not active:
        out.append({"level": "warn", "text": "Not yet active: the initial committee's adoption has not finished."})
    # the lag behind Ethereum's finality: by the checkpoint given, by the wall clock if none was
    wall_slot = _div(now_ms // 1000 - cfg["genesis_time"], cfg["seconds_per_slot"])
    ref = network_finalized
    lag_slots = (ref - slot) if ref is not None else (wall_slot - slot - 2 * spe)
    lag_epochs = _div(lag_slots, spe)
    by = ("Ethereum's finalized checkpoint (--finalized-slot)" if ref is not None
          else "the wall clock (no checkpoint given: --finalized-slot judges by Ethereum's finality)")
    if ref is not None and _div(wall_slot - ref, spe) >= th["lag_bad_epochs"]:
        providers = _div(wall_slot - ref, spe)
        out.append({"level": "warn",
                    "text": f"Ethereum's finalized checkpoint, as given, is {providers} epochs behind the wall clock: "
                            "Ethereum's finality is stalled, or the checkpoint is stale."})
    if lag_epochs >= th["lag_bad_epochs"]:
        out.append({"level": "bad",
                    "text": f"Finalized header {lag_epochs} epochs behind {by}: the updater is not keeping up or "
                            "has stopped."})
    elif lag_epochs >= th["lag_warn_epochs"]:
        out.append({"level": "warn", "text": f"Finalized header {lag_epochs} epochs behind {by}."})
    # the committee period, when the next committee is not known yet
    period_end_ms = time_of_slot_ms((slot // period + 1) * period)
    if not next_committee:
        hours = (period_end_ms - now_ms) / 3600000
        if hours < 0:
            out.append({"level": "bad", "text": "The next sync committee is unknown and the finalized header's "
                                                "period has ended: the instance cannot follow without a committee "
                                                "update."})
        elif hours < th["period_warn_hours"]:
            out.append({"level": "warn",
                        "text": f"The next sync committee is unknown and the period ends in {hours:.1f} h."})
    # the weak-subjectivity period of the finalized header
    ws_hours = (time_of_slot_ms(slot + cfg["ws_period_epochs"] * spe) - now_ms) / 3600000
    if ws_hours < 0:
        out.append({"level": "bad", "text": "The finalized header is older than the weak-subjectivity period: the "
                                            "program refuses further updates; a successor instance is needed."})
    elif ws_hours < th["ws_warn_hours"]:
        out.append({"level": "warn", "text": f"The weak-subjectivity period of the finalized header ends in "
                                             f"{ws_hours:.1f} h."})
    if not out:
        out.append({"level": "ok", "text": "Following Ethereum's finality."})
    return [a | {"lag_epochs": lag_epochs, "lag_slots": lag_slots,
                 "period_end_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(period_end_ms / 1000)),
                 "ws_end_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(
                     time_of_slot_ms(slot + cfg["ws_period_epochs"] * spe) / 1000))} for a in out]


class Reader:
    def __init__(self, site: str, committee: int = 512):
        self.site = site.rstrip("/")
        self.committee = committee
        self.rpc = lez.Rpc(self.site + "/rpc", budget_s=120)
        self._programs = None
        self._idls = {}

    def programs(self) -> dict:
        """program account (base58) -> (name, image id) from the network's /programs.json."""
        if self._programs is None:
            try:
                dep = self.rpc.get_json(self.site + "/programs.json")
                self._programs = {p["program_account"]: (name, p.get("image_id", ""))
                                  for name, p in (dep.get("programs") or {}).items() if p.get("program_account")}
            except Exception:  # noqa: BLE001 - a network without the file: messages are shown raw
                self._programs = {}
        return self._programs

    def idl(self, name):
        if name not in self._idls:
            path = IDLS.get(name)
            if name in BUILTIN_IDLS:
                self._idls[name] = lc.Idl(BUILTIN_IDLS[name])
            else:
                self._idls[name] = lc.Idl.load(ROOT / path) if path and (ROOT / path).exists() else None
        return self._idls[name]

    def describe_tx(self, tx: dict, reverted: bool | None = None) -> dict:
        root = tx["root"]
        out = {"hash": tx["hash"].hex(), "kind": tx["kind"], "size": tx["size"],
               "signers": [lez.b58encode(s) for s in tx["signers"]],
               "nonces": [{"account": lez.b58encode(n["account"]), "nonce": n["nonce"]} for n in tx["nonces"]]}
        if reverted is not None:
            out["outcome"] = "reverted" if reverted else "applied"
        if tx["kind"] == "PrivacyPreserving":
            e = tx["execution"]
            out["private"] = {"nullifiers": len(e["private_actions"]), "proof_bytes": len(tx["proof"]),
                              "boundary_steps": len(e["boundary"])}
        if root is None:
            out["call"] = None
            return out
        prog = lez.b58encode(root["to"]["program_account_id"])
        name, image = self.programs().get(prog, (BUILTINS.get(prog), None))
        call = {"program": prog, "program_name": name, "account": lez.b58encode(root["to"]["account_id"]),
                "message_bytes": len(root["message"])}
        idl = self.idl(name) if name else None
        if idl is not None:
            try:
                ins, args = idl.instruction(root["message"])
                call["instruction"] = ins
                call["args"] = {k: (f"<{len(v)} bytes>" if isinstance(v, bytes) and len(v) > 64 else plain(v))
                                for k, v in args.items()}
                if name == "ferrata_attestation" and ins == "open":
                    u = lc.compact_update(args["update"], self.committee)
                    call["update"] = plain(u)
            except lez.DecodeError as e:
                call["decode_error"] = str(e)
        else:
            call["message_hex"] = root["message"][:64].hex() + ("…" if len(root["message"]) > 64 else "")
        out["call"] = call
        return out

    def block(self, n: int) -> dict:
        import base64
        raw = self.rpc.call("getBlock", [n])
        if raw is None:
            raise LookupError(f"block {n}: not served")
        b = lez.decode_block(base64.b64decode(raw))
        h = b["header"]
        return {"block": h["block_id"], "hash": h["hash"].hex(), "hash_recomputed": b["computed_hash"].hex(),
                "hash_ok": b["hash_ok"], "prev": h["prev_block_hash"].hex(),
                "time_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(h["timestamp"] / 1000)),
                "bedrock_status": b["bedrock_status"], "bytes": b["size"],
                "transactions": [self.describe_tx(tx, i in set(b["reverted"])) for i, tx in enumerate(b["transactions"])]}

    def tx(self, h: str) -> dict:
        import base64
        raw = self.rpc.call("getTransaction", [h.removeprefix("0x")])
        if raw is None:
            raise LookupError(f"transaction {h}: not served")
        tx = lez.decode_transaction(base64.b64decode(raw))
        d = self.describe_tx(tx)
        d["hash_ok"] = tx["hash"].hex() == h.removeprefix("0x").lower()
        return d

    def account(self, acct: str, prog: str) -> dict:
        view = self.rpc.call("getAccountView", [{"account_id": acct, "program_account_id": prog}])
        data = bytes((view or {}).get("data", {}).get("actor_states", {}).get(prog, []))
        out = {"account": acct, "program": prog, "bytes": len(data), "nonce": (view or {}).get("nonce")}
        if prog == lez.b58encode(LOADER) and len(data) == 66 and data[0] == 0:
            out["loader_header"] = {"image_id": "0x" + data[1:33].hex(), "first_segment": lez.b58encode(data[33:65]),
                                    "immutable": data[65] == 1}
            return out
        name, _ = self.programs().get(prog, (None, None))
        idl = self.idl(name) if name else None
        if idl is not None:
            for t in [a["name"] for a in idl.doc.get("accounts", [])]:
                try:
                    out[t] = plain(idl.decode_type(data, t))
                    break
                except (lez.DecodeError, KeyError, ValueError):
                    continue
        if len(out) == 4:
            out["hex"] = data.hex()
        return out

    def instance(self, acct: str) -> dict:
        prog = next((p for p, (n, _) in self.programs().items() if n == "ferrata_attestation"), None)
        if prog is None:
            raise LookupError("the network's /programs.json names no attestation program")
        a = self.account(acct, prog)
        st = a.get("InstanceState")
        if st is None:
            raise LookupError(f"{acct} holds no InstanceState of the attestation program")
        store = bytes.fromhex(st["store"])
        hdr = lc._beacon(store[1:113])
        cfg = st["config"]
        # CompactStore (ferrata.js `decodeCompactStore`): fork byte, the finalized header (112 B), the current
        # committee's summary (80 B), a flag, the next committee's summary when the flag is set.
        if len(store) not in (194, 274):
            raise ValueError(f"{acct}: the instance's store is {len(store)} bytes, a CompactStore is 194 or 274")
        return {"instance": acct, "program": prog, "id": st["id"], "chain_id": cfg["chain_id"], "active": st["active"],
                "sync_committee_size": cfg["sync_committee_size"],
                "applied_jobs": st["applied"], "fork": lc.FORKS[store[0]] if store[0] < len(lc.FORKS) else store[0],
                "finalized_slot": hdr["slot"], "finalized_root": "0x" + lc.beacon_root(hdr).hex(),
                "finalized_slot_time_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(
                    cfg["genesis_time"] + hdr["slot"] * cfg["seconds_per_slot"])),
                "period": hdr["slot"] // (cfg["slots_per_epoch"] * cfg["epochs_per_sync_committee_period"]),
                "next_committee": len(store) == 274, "slots_per_epoch": cfg["slots_per_epoch"],
                "epochs_per_sync_committee_period": cfg["epochs_per_sync_committee_period"],
                "genesis_time": cfg["genesis_time"], "seconds_per_slot": cfg["seconds_per_slot"],
                "ws_period_epochs": cfg["ws_period_epochs"]}


def show(obj, as_json: bool, indent=""):
    if as_json:
        print(json.dumps(obj, indent=1))
        return
    for k, v in obj.items():
        if isinstance(v, list) and v and isinstance(v[0], dict):
            print(f"{indent}{k}:")
            for i, x in enumerate(v):
                print(f"{indent}  [{i}]")
                show(x, False, indent + "    ")
        elif isinstance(v, dict):
            print(f"{indent}{k}:")
            show(v, False, indent + "  ")
        else:
            print(f"{indent}{k}: {v}")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--lez", help="the network's site (default: scripts/reproduce/deployed-programs.json's network)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--committee", type=int, default=512, metavar="N",
                    help="the sync committee size an Open's update is decoded for (default 512, Sepolia's and mainnet's; "
                         "an instance's Init sets it, `lez-read instance ACCOUNT` shows it)")
    ap.add_argument("--alarms", action="store_true",
                    help="with `instance ACCOUNT`: the status page's alarms for it (lag, the committee period's end, "
                         "the weak-subjectivity end; the page's rules and thresholds)")
    ap.add_argument("--finalized-slot", type=int, metavar="N",
                    help="with --alarms: Ethereum's finalized checkpoint slot, as a beacon API serves it (the lag is "
                         "judged by it, as the page judges it; without it, by the wall clock)")
    ap.add_argument("what", choices=["head", "block", "tx", "programs", "instance", "account"])
    ap.add_argument("args", nargs="*")
    a = ap.parse_args(argv)
    site = a.lez or json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text())["network"]
    r = Reader(site, a.committee)
    try:
        if a.what == "head":
            out = r.block(r.rpc.call("getLastBlockId"))
            out = {k: v for k, v in out.items() if k != "transactions"} | {"transactions": len(out["transactions"])}
        elif a.what == "block":
            out = r.block(int(a.args[0]))
        elif a.what == "tx":
            out = r.tx(a.args[0])
        elif a.what == "programs":
            out = {"programs": []}
            for acct, (name, image) in sorted(r.programs().items(), key=lambda kv: kv[1][0]):
                hdr = r.account(acct, lez.b58encode(LOADER)).get("loader_header", {})
                out["programs"].append({"name": name, "account": acct, "published_image_id": image,
                                        "on_chain_image_id": hdr.get("image_id"), "immutable": hdr.get("immutable"),
                                        "equal": hdr.get("image_id", "").lower() == str(image).lower()})
        elif a.what == "instance":
            out = r.instance(a.args[0])
            if a.alarms:
                out["alarms"] = alarms(
                    {k: out[k] for k in ("slots_per_epoch", "epochs_per_sync_committee_period", "genesis_time",
                                         "seconds_per_slot", "ws_period_epochs")},
                    out["finalized_slot"], active=out["active"], next_committee=out["next_committee"],
                    now_ms=int(time.time() * 1000), network_finalized=a.finalized_slot)
        else:
            out = r.account(a.args[0], a.args[1])
    except (lez.RpcError, OSError) as e:
        print(f"lez-read: the network could not be read: {e}", file=sys.stderr)
        return 2
    except (LookupError, IndexError, ValueError) as e:
        print(f"lez-read: {e}", file=sys.stderr)
        return 1
    show(out, a.json)
    txs = out.get("transactions") if isinstance(out.get("transactions"), list) else []
    bad = (out.get("hash_ok") is False or any(not p["equal"] or not p["immutable"] for p in out.get("programs", []))
           or any((t.get("call") or {}).get("decode_error") for t in txs)
           or any(x["level"] == "bad" for x in out.get("alarms", [])))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
