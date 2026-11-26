"""The light client's run on our public LEZ v0.4.0-rc3 test network, reconstructed from the public chain alone.

    python3 -I scripts/verify.py run-record            offline: every figure of the record recomputed from its
                                                        transaction list, and the list's own consistency
    python3 -I scripts/verify.py run-record-online     read-only, from the network's public RPC: every block from 1
                                                        to the record's as-of block read again, each hash recomputed and
                                                        linked to the one before; every transaction to the attestation
                                                        program in them decoded again and compared with the record's
                                                        line, field by field, and none missing from it
    python3 -I scripts/verifylib/runrecord.py build OUT [--lez URL]
                                                        the maintainer's step: read the chain, write OUT/

What is read is only what anyone can read: the blocks the network's sequencer serves (`getBlockRange`), each one's
hash recomputed from its bytes (lez.py), and the attestation program's messages decoded with its IDL
(idl/ferrata_attestation.idl.json) and ferrata-core's update encoding (lc.py). No host access, no logs, no keys.
"""
from __future__ import annotations

import argparse
import csv
import io
import json
import statistics
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from verifylib import lc, lez  # type: ignore
    from verifylib.core import CheckFailed, Skipped, check, fail_if  # type: ignore
else:
    from . import lc, lez
    from .core import CheckFailed, Skipped, check, fail_if

RECORD_DIR = "lez/updater/results/public-network-run"
ATTESTATION_IMAGE = "816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df"
COLUMNS = ("block", "time_utc", "block_hash", "index", "tx", "step", "outcome", "instance", "job", "sender", "nonce",
           "fork", "attested_slot", "finalized_slot", "finalized_root", "signature_slot", "participants",
           "next_committee", "part", "detail")
UPDATE_FIELDS = ("fork", "attested_slot", "finalized_slot", "finalized_root", "signature_slot", "participants",
                 "next_committee")
GAP_ALARM_S = 3 * 3600


def iso(ms: int) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(ms / 1000)) + f".{ms % 1000:03d}Z"


def parse_iso(s: str) -> int:
    base, frac = s.rstrip("Z").split(".")
    return int(time.mktime(time.strptime(base, "%Y-%m-%dT%H:%M:%S")) - time.timezone) * 1000 + int(frac)


def _actor_ids(tx) -> list:
    return [a["account_id"] for a in tx["public_context"]["actors"]]


def scan(blocks, program: bytes, idl: lc.Idl, last: int | None = None):
    """Walk the blocks in order: hashes recomputed and linked, every transaction to `program` decoded.
    Returns (lines, chain): the record's lines and what the walk saw of the chain."""
    lines = []
    prev = None
    n = 0
    gaps = []
    instances = {}  # instance account -> (chain config of its Init)
    jobs = {}  # job account -> the open line's fields
    first_ts = None
    problems = []
    for raw in blocks:
        b = lez.decode_block(raw)
        h = b["header"]
        if last is not None and h["block_id"] > last:
            break
        if not b["hash_ok"]:
            problems.append(f"block {h['block_id']}: its hash does not match its bytes")
        if prev is not None:
            if h["block_id"] != prev["block_id"] + 1:
                problems.append(f"block {h['block_id']} follows block {prev['block_id']}")
            if h["prev_block_hash"] != prev["hash"]:
                problems.append(f"block {h['block_id']}: its previous hash is not block {prev['block_id']}'s hash")
            if h["timestamp"] and prev["timestamp"] and h["timestamp"] - prev["timestamp"] > 60_000:
                gaps.append({"after_block": prev["block_id"], "seconds": (h["timestamp"] - prev["timestamp"]) / 1000,
                             "from_utc": iso(prev["timestamp"]), "to_utc": iso(h["timestamp"])})
        if first_ts is None and h["timestamp"]:
            first_ts = (h["block_id"], h["timestamp"])
        reverted = set(b["reverted"])
        for i, tx in enumerate(b["transactions"]):
            root = tx["root"]
            if root is None or root["to"]["program_account_id"] != program:
                continue
            try:
                name, args = idl.instruction(root["message"])
            except lez.DecodeError as e:
                name, args = "undecodable", {"error": str(e)}
            line = {c: "-" for c in COLUMNS}
            line.update(block=h["block_id"], time_utc=iso(h["timestamp"]), block_hash=h["hash"].hex(), index=i,
                        tx=tx["hash"].hex(), step=name, outcome="reverted" if i in reverted else "applied",
                        sender=lez.b58encode(tx["signers"][0]) if tx["signers"] else "-",
                        nonce=next((nn["nonce"] for nn in tx["nonces"] if tx["signers"] and nn["account"] == tx["signers"][0]), "-"))
            ids = [lez.b58encode(a) for a in _actor_ids(tx)]
            if name == "init":
                # Init writes instance(id) and the job of its first committee's adoption (job(id, 0…)).
                inst = lez.b58encode(root["to"]["account_id"])
                cfg = args["config"]
                instances[inst] = cfg
                job = next((a for a in ids if a != inst), "-")
                jobs[job] = line
                line.update(instance=inst, job=job, detail=f"chain_id={cfg['chain_id']} committee="
                            f"{cfg['sync_committee_size']} adoption_parts={cfg['adoption_parts']}")
            elif name == "open":
                # Open writes instance(id) and opens job(id, job_id).
                inst = next((a for a in ids if a in instances), "-")
                job = next((a for a in ids if a != inst), "-")
                line.update(instance=inst, job=job)
                try:
                    u = lc.compact_update(args["update"], instances.get(inst, {}).get("sync_committee_size", 512))
                    line.update(fork=u["fork"], attested_slot=u["attested_slot"], finalized_slot=u["finalized_slot"],
                                finalized_root=u["finalized_root"].hex(), signature_slot=u["signature_slot"],
                                participants=u["participants"], next_committee=int(u["next_committee"]))
                except lez.DecodeError as e:
                    line["detail"] = f"update not decoded: {e}"
                if job != "-" and line["outcome"] == "applied":
                    jobs[job] = line
            elif name in ("miller", "adopt", "finish"):
                # Each step names its job; the job's open (or init) names the instance and the update.
                job = next((a for a in ids if a in jobs), "-")
                o = jobs.get(job)
                line.update(job=job, instance=o["instance"] if o else "-")
                if o is not None and o["step"] == "open":
                    for k in UPDATE_FIELDS:
                        line[k] = o[k]
                elif o is not None:
                    line["detail"] = "the initial committee's adoption (Init's job)"
                if name == "adopt":
                    line["part"] = args.get("part", "-")
            elif name == "attest":
                req = args["request"]
                # The request names the instance by its id; its account is the PDA instance(id) (lez/wire).
                acct = lez.b58encode(lez.ferrata_pda(program, "instance", req["instance_id"]))
                line["instance"] = acct if acct in instances else "-"
                line["detail"] = (f"accounts={len(req['accounts'])} logs={len(req['logs'])} leaves={len(req['leaves'])} "
                                  f"relations={len(req['relations'])} hops={len(req['hops'])}")
            lines.append(line)
        prev = h
        n += 1
    chain = {"blocks": n, "first_block_with_time": {"block": first_ts[0], "time_utc": iso(first_ts[1])} if first_ts else None,
             "as_of": {"block": prev["block_id"], "hash": prev["hash"].hex(), "time_utc": iso(prev["timestamp"])} if prev else None,
             "block_gaps_over_60s": gaps, "instances": {k: _cfg_json(v) for k, v in instances.items()}}
    return lines, chain, problems


def _cfg_json(cfg: dict) -> dict:
    return {k: (v.hex() if isinstance(v, bytes) else v) for k, v in cfg.items() if k not in ("fork_versions", "fork_epochs")}


def write_tsv(lines, path: Path) -> None:
    buf = io.StringIO()
    w = csv.DictWriter(buf, fieldnames=COLUMNS, delimiter="\t", lineterminator="\n")
    w.writeheader()
    for ln in lines:
        w.writerow({k: ln[k] for k in COLUMNS})
    path.write_text(buf.getvalue())


def read_tsv(path: Path) -> list:
    with open(path, newline="") as fh:
        return list(csv.DictReader(fh, delimiter="\t"))


def _int(x):
    return int(x) if x not in ("-", "", None) else None


def summarize(rows: list, chain: dict) -> dict:
    """Every figure of the record, from its transaction list (strings, as read back from the TSV) and the chain facts
    the walk recorded (the as-of block, the instances' Init configuration, the block gaps)."""
    out = {"as_of": chain["as_of"], "chain": {"blocks": chain["blocks"],
                                              "first_block_with_time": chain["first_block_with_time"],
                                              "block_gaps_over_60s": chain["block_gaps_over_60s"]},
           "transactions": len(rows), "instances": {}}
    by_inst = {}
    for r in rows:
        by_inst.setdefault(r["instance"], []).append(r)
    for inst, rs in sorted(by_inst.items(), key=lambda kv: min(int(r["block"]) for r in kv[1])):
        cfg = chain["instances"].get(inst, {})
        per = (cfg.get("slots_per_epoch", 32) * cfg.get("epochs_per_sync_committee_period", 256))
        genesis = cfg.get("genesis_time")
        sps = cfg.get("seconds_per_slot", 12)
        steps = {}
        for r in rs:
            key = f"{r['step']} {r['outcome']}"
            steps[key] = steps.get(key, 0) + 1
        finishes = [r for r in rs if r["step"] == "finish" and r["outcome"] == "applied"]
        opens = [r for r in rs if r["step"] == "open"]
        finished_jobs = {r["job"] for r in finishes}
        last_finish = max((int(r["block"]) for r in finishes), default=-1)
        unfinished = [{"block": int(r["block"]), "time_utc": r["time_utc"], "job": r["job"], "sender": r["sender"],
                       "finalized_slot": _int(r["finalized_slot"]),
                       # a job opened after the newest finish is still running at the as-of block, not left
                       "in_progress_at_as_of": int(r["block"]) > last_finish}
                      for r in opens if r["outcome"] == "applied" and r["job"] not in finished_jobs]
        updates = []
        for r in finishes:
            t = parse_iso(r["time_utc"])
            fs = _int(r["finalized_slot"])
            u = {"block": int(r["block"]), "time_utc": r["time_utc"], "finalized_slot": fs,
                 "finalized_root": r["finalized_root"],
                 "attested_slot": _int(r["attested_slot"]), "signature_slot": _int(r["signature_slot"]),
                 "period": fs // per if fs is not None else None, "participants": _int(r["participants"]),
                 "next_committee": r["next_committee"] == "1", "sender": r["sender"]}
            if genesis is not None and fs is not None:
                u["lag_s"] = round(t / 1000 - (genesis + fs * sps), 1)
            updates.append(u)
        intervals = []
        for a, b in zip(updates, updates[1:]):
            intervals.append({"seconds": round((parse_iso(b["time_utc"]) - parse_iso(a["time_utc"])) / 1000, 3),
                              "from_block": a["block"], "to_block": b["block"],
                              "from_utc": a["time_utc"], "to_utc": b["time_utc"]})
        longest = max(intervals, key=lambda x: x["seconds"]) if intervals else None
        periods = sorted({u["period"] for u in updates if u["period"] is not None})
        crossings = []
        for a, b in zip(updates, updates[1:]):
            if a["period"] is not None and b["period"] is not None and b["period"] > a["period"]:
                crossings.append({"from_period": a["period"], "to_period": b["period"], "block": b["block"],
                                  "time_utc": b["time_utc"], "finalized_slot": b["finalized_slot"]})
        adoptions = [{"block": u["block"], "time_utc": u["time_utc"], "finalized_slot": u["finalized_slot"],
                      "period": u["period"]} for u in updates if u["next_committee"]]
        senders = {}
        for r in rs:
            s = senders.setdefault(r["sender"], {"transactions": 0, "first_block": int(r["block"]),
                                                 "last_block": int(r["block"]), "steps": {}})
            s["transactions"] += 1
            s["last_block"] = max(s["last_block"], int(r["block"]))
            s["steps"][r["step"]] = s["steps"].get(r["step"], 0) + 1
        days = {}
        for u in updates:
            days[u["time_utc"][:10]] = days.get(u["time_utc"][:10], 0) + 1
        epoch = cfg.get("slots_per_epoch", 32)
        esteps = {}
        for a, b in zip(updates, updates[1:]):
            k = str((b["finalized_slot"] - a["finalized_slot"]) // epoch)
            esteps[k] = esteps.get(k, 0) + 1
        opened_at = {r["job"]: parse_iso(r["time_utc"]) for r in opens}
        o2f = [(parse_iso(r["time_utc"]) - opened_at[r["job"]]) / 1000 for r in finishes if r["job"] in opened_at]
        lags = [u["lag_s"] for u in updates if "lag_s" in u]
        parts = [u["participants"] for u in updates if u["participants"] is not None]
        refused = [{"block": int(r["block"]), "time_utc": r["time_utc"], "tx": r["tx"], "step": r["step"],
                    "sender": r["sender"]} for r in rs if r["outcome"] == "reverted"]
        init = next((r for r in rs if r["step"] == "init"), None)
        out["instances"][inst] = {
            "init": {"block": int(init["block"]), "time_utc": init["time_utc"], "sender": init["sender"],
                     "detail": init["detail"]} if init else None,
            "chain_id": cfg.get("chain_id"),
            "steps": dict(sorted(steps.items())),
            "updates_applied": len(updates),
            "first_update": updates[0] if updates else None,
            "last_update": updates[-1] if updates else None,
            "finalized_slots": [updates[0]["finalized_slot"], updates[-1]["finalized_slot"]] if updates else None,
            "periods": periods,
            "period_crossings": crossings,
            "committee_adoptions": adoptions,
            "intervals": {
                "count": len(intervals),
                "median_s": round(statistics.median([i["seconds"] for i in intervals]), 3) if intervals else None,
                "longest": longest,
                "over_1h": [i for i in intervals if i["seconds"] > 3600],
                "over_3h": [i for i in intervals if i["seconds"] > GAP_ALARM_S],
            },
            "updates_per_utc_day": days,
            "finalized_epochs_advanced_per_update": dict(sorted(esteps.items(), key=lambda kv: int(kv[0]))),
            "open_to_finish_s": {"median": round(statistics.median(o2f), 3), "max": round(max(o2f), 3)} if o2f else None,
            "longest_intervals": sorted(intervals, key=lambda x: -x["seconds"])[:10],
            "lag_behind_finality_s": {"min": min(lags), "median": round(statistics.median(lags), 1), "max": max(lags)}
            if lags else None,
            "participants": {"min": min(parts), "max": max(parts)} if parts else None,
            "jobs_opened_not_finished": unfinished,
            "refused": refused,
            "senders": senders,
        }
    return out


def _cached_or_fetch(rpc, first, last, cache: Path):
    lez.fetch_blocks(rpc, first, last, cache)
    return lez.cached_blocks(cache)


LOADER = bytes([0xFE]) * 32  # LEZ's program loader (lez.js PROGRAM_LOADER_ID)


def program_account(site: str, rpc: lez.Rpc) -> bytes:
    """The attestation program's account: the network's /programs.json names it, and its loader header on chain must
    be `LoaderEntry::Header` naming the published image id, immutable (checked here, so the record does not rest on
    the file)."""
    dep = rpc.get_json(site.rstrip("/") + "/programs.json")
    progs = dep.get("programs") or {}
    acct = next((p.get("program_account") for p in (progs.values() if isinstance(progs, dict) else progs)
                 if str(p.get("image_id", "")).lower().removeprefix("0x") == ATTESTATION_IMAGE), None)
    if acct is None:
        raise CheckFailed("the network's /programs.json names no program with the attestation image id")
    a = lez.b58decode(acct)
    view = rpc.call("getAccountView", [{"account_id": acct, "program_account_id": lez.b58encode(LOADER)}])
    data = bytes((view or {}).get("data", {}).get("actor_states", {}).get(lez.b58encode(LOADER), []))
    if len(data) != 66 or data[0] != 0 or data[1:33].hex() != ATTESTATION_IMAGE or data[65] != 1:
        raise CheckFailed(f"the loader header of {acct} is not an immutable header naming image {ATTESTATION_IMAGE[:16]}…")
    return a


def instance_state(rpc: lez.Rpc, idl: lc.Idl, program: bytes, account: str) -> dict:
    """The instance's own state on chain: its count of applied jobs and its store's finalized header slot."""
    view = rpc.call("getAccountView", [{"account_id": account, "program_account_id": lez.b58encode(program)}])
    data = bytes((view or {}).get("data", {}).get("actor_states", {}).get(lez.b58encode(program), []))
    st = idl.decode_type(data, "InstanceState")
    store = st["store"]  # CompactStore: fork byte, the finalized beacon header's SSZ (slot first), ...
    return {"applied": st["applied"], "active": st["active"], "finalized_slot": int.from_bytes(store[1:9], "little")}


def reconcile_state(rpc: lez.Rpc, idl: lc.Idl, program: bytes, lines: list, cache: Path, as_of: int) -> list:
    """Each instance's on-chain counter against the record: `applied` (Init's activation and every update the program
    applied) must equal 1 + the record's applied finishes + those in the blocks after the as-of block up to the head
    read with it, and the store's finalized slot the newest applied update's. Returns problems (empty: it holds)."""
    problems = []
    for attempt in range(3):
        head = rpc.call("getLastBlockId")
        states = {inst: instance_state(rpc, idl, program, inst)
                  for inst in sorted({ln["instance"] for ln in lines if ln["step"] == "init"})}
        if rpc.call("getLastBlockId") == head:
            break
    later, _, probs = scan(_cached_or_fetch(rpc, 1, head, cache), program, idl, last=head)
    problems += probs
    for inst, st in states.items():
        fin = [ln for ln in later if ln["instance"] == inst and ln["step"] == "finish" and ln["outcome"] == "applied"]
        if st["applied"] != 1 + len(fin):
            problems.append(f"instance {inst[:10]}…: its counter says {st['applied']} applied jobs at block {head}; "
                            f"the chain's finishes give 1 + {len(fin)}")
        if fin and st["finalized_slot"] != int(fin[-1]["finalized_slot"]):
            problems.append(f"instance {inst[:10]}…: its store's finalized slot {st['finalized_slot']} is not the "
                            f"newest applied update's {fin[-1]['finalized_slot']}")
        mine = sum(1 for ln in lines if ln["instance"] == inst and ln["step"] == "finish" and ln["outcome"] == "applied")
        st.update(head=head, applied_by_as_of=1 + mine)
    return problems, states


def beacon_header(urls: list, slot: int, timeout: float = 30.0):
    """(root, canonical) of the block at `slot` from the first beacon API that answers; None if none does."""
    import urllib.error
    import urllib.request
    for u in urls:
        try:
            req = urllib.request.Request(f"{u.rstrip('/')}/eth/v1/beacon/headers/{slot}",
                                         headers={"Accept": "application/json", "User-Agent": "ferrata-verify/1"})
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                d = json.loads(resp.read())["data"]
            return d["root"].removeprefix("0x"), bool(d.get("canonical", True)), u
        except (urllib.error.URLError, TimeoutError, KeyError, ValueError):
            continue
    return None


def build(out: Path, site: str, cache: Path, last: int | None = None) -> dict:
    rpc = lez.Rpc(site.rstrip("/") + "/rpc")
    program = program_account(site, rpc)
    tip = rpc.call("getLastBlockId") if last is None else last
    fetched = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    idl = lc.Idl.load(Path(__file__).resolve().parents[2] / "idl/ferrata_attestation.idl.json")
    lines, chain, problems = scan(_cached_or_fetch(rpc, 1, tip, cache), program, idl, last=tip)
    if problems:
        raise CheckFailed("\n".join(problems[:20]))
    probs, states = reconcile_state(rpc, idl, program, lines, cache, tip)
    if probs:
        raise CheckFailed("\n".join(probs))
    out.mkdir(parents=True, exist_ok=True)
    write_tsv(lines, out / "transactions.tsv")
    rows = read_tsv(out / "transactions.tsv")
    chain["program_account"] = lez.b58encode(program)
    chain["read_at_utc"] = fetched
    chain["instance_counters_read"] = states
    (out / "chain.json").write_text(json.dumps(chain, indent=1, sort_keys=True) + "\n")
    summary = summarize(rows, chain)
    (out / "summary.json").write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n")
    (out / "RECORD.md").write_text(render(json.loads((out / "summary.json").read_text())))
    return summary


def _dur(sec: float) -> str:
    sec = int(round(sec))
    d, r = divmod(sec, 86400)
    h, r = divmod(r, 3600)
    m, s_ = divmod(r, 60)
    return (f"{d} d " if d else "") + (f"{h} h " if h or d else "") + f"{m} min" + ("" if d or h else f" {s_} s")


def render(summary: dict) -> str:
    """RECORD.md: the summary in words and tables, generated (the offline check regenerates it)."""
    a = summary["as_of"]
    c = summary["chain"]
    out = ["# The light client's run on our public LEZ test network, from chain data", "",
           "Generated by `scripts/verifylib/runrecord.py` from `transactions.tsv` and `chain.json` beside it; "
           "`python3 -I scripts/verify.py run-record` regenerates it and fails on any difference. Read the README "
           "first.", "",
           f"As of LEZ block {a['block']} ({a['time_utc']}), hash `{a['hash']}`. The chain's first block with a time "
           f"is block {c['first_block_with_time']['block']} ({c['first_block_with_time']['time_utc']}); "
           f"{c['blocks']} blocks were read, each hash recomputed from its bytes and linked to the one before. "
           f"{summary['transactions']} transactions to the attestation program.", "",
           "## Block production", "",
           "Blocks come every 10 s; the pauses of more than 60 s between two blocks:", "",
           "| after block | from (UTC) | to (UTC) | pause |", "|---:|---|---|---:|"]
    for g in c["block_gaps_over_60s"]:
        if g["from_utc"].startswith("1970"):
            continue  # the genesis block carries no time
        out.append(f"| {g['after_block']} | {g['from_utc']} | {g['to_utc']} | {_dur(g['seconds'])} |")
    for inst, v in summary["instances"].items():
        out += ["", f"## Instance `{inst}`", ""]
        ini = v["init"]
        if ini:
            out.append(f"`Init` in block {ini['block']} ({ini['time_utc']}) by `{ini['sender']}`: {ini['detail']}.")
        fu, lu = v["first_update"], v["last_update"]
        if not fu:
            out.append("No update applied.")
            continue
        span = (parse_iso(lu["time_utc"]) - parse_iso(fu["time_utc"])) / 1000
        out += ["", f"- **{v['updates_applied']} updates applied**, from block {fu['block']} ({fu['time_utc']}) to "
                f"block {lu['block']} ({lu['time_utc']}): {_dur(span)}; Sepolia finalized slots "
                f"{v['finalized_slots'][0]:,} to {v['finalized_slots'][1]:,}, sync-committee periods "
                f"{', '.join(str(x) for x in v['periods'])}."]
        iv = v["intervals"]
        lg = iv["longest"]
        out.append(f"- Between consecutive updates: median {_dur(iv['median_s'])}, longest {_dur(lg['seconds'])} "
                   f"(blocks {lg['from_block']} to {lg['to_block']}, {lg['from_utc']} to {lg['to_utc']}); "
                   f"{len(iv['over_1h'])} over 1 h, {len(iv['over_3h'])} over 3 h.")
        out.append(f"- Committee adoptions (an update carrying the next sync committee, adopted): "
                   f"{len(v['committee_adoptions'])}; sync-committee periods crossed: {len(v['period_crossings'])}.")
        out.append(f"- Refused (in the block's reverted list): {len(v['refused'])}. Jobs opened and left "
                   f"unfinished: {sum(1 for j in v['jobs_opened_not_finished'] if not j['in_progress_at_as_of'])}.")
        if v["lag_behind_finality_s"]:
            lag = v["lag_behind_finality_s"]
            out.append(f"- Each update's LEZ block time minus its finalized slot's time: median {_dur(lag['median'])}, "
                       f"min {_dur(lag['min'])}, max {_dur(lag['max'])}.")
        if v["participants"]:
            out.append(f"- Sync-committee participation of the applied updates: {v['participants']['min']} to "
                       f"{v['participants']['max']} of 512.")
        if v["open_to_finish_s"]:
            out.append(f"- From a job's `Open` to its `Finish`: median {_dur(v['open_to_finish_s']['median'])}, "
                       f"max {_dur(v['open_to_finish_s']['max'])}.")
        out.append("- Finalized epochs advanced from one update to the next: " +
                   ", ".join(f"{k}: {n}" for k, n in sorted(v["finalized_epochs_advanced_per_update"].items(),
                                                            key=lambda kv: int(kv[0]))) + ".")
        out += ["", "Updates per UTC day:", "", "| day | updates |", "|---|---:|"]
        out += [f"| {d} | {n} |" for d, n in sorted(v["updates_per_utc_day"].items())]
        out += ["", "Committee adoptions and period crossings:", "", "| event | block | time (UTC) | finalized slot |",
                "|---|---:|---|---:|"]
        ev = [("adoption, period " + str(x["period"]), x) for x in v["committee_adoptions"]] + \
             [(f"crossing {x['from_period']} → {x['to_period']}", x) for x in v["period_crossings"]]
        for name, x in sorted(ev, key=lambda e: e[1]["block"]):
            out.append(f"| {name} | {x['block']} | {x['time_utc']} | {x['finalized_slot']:,} |")
        out += ["", "The ten longest intervals between consecutive updates:", "",
                "| from block | to block | from (UTC) | to (UTC) | interval |", "|---:|---:|---|---|---:|"]
        for x in v["longest_intervals"]:
            out.append(f"| {x['from_block']} | {x['to_block']} | {x['from_utc']} | {x['to_utc']} | {_dur(x['seconds'])} |")
        if v["refused"]:
            out += ["", "Refused transactions:", "", "| block | time (UTC) | step | sender | transaction |",
                    "|---:|---|---|---|---|"]
            for x in v["refused"]:
                out.append(f"| {x['block']} | {x['time_utc']} | {x['step']} | `{x['sender']}` | `{x['tx']}` |")
        out += ["", "Senders:", "", "| account | transactions | first block | last block | steps |", "|---|---:|---:|---:|---|"]
        for acct, x in sorted(v["senders"].items(), key=lambda kv: -kv[1]["transactions"]):
            steps = ", ".join(f"{k} {n}" for k, n in sorted(x["steps"].items()))
            out.append(f"| `{acct}` | {x['transactions']} | {x['first_block']} | {x['last_block']} | {steps} |")
    return "\n".join(out) + "\n"


# ---- the checks -------------------------------------------------------------------------------------------------

ROWS = ("RFP-021 U.3", "RFP-021 S.1", "RFP-021 S.7", "RFP-021 R.4")


@check(id="run-record", title="The light client's run on our public LEZ test network: every figure recomputed",
       family="run-record", mode="offline", evidence=(242, 248), rows=("RFP-021 S.1",),
       what=f"{RECORD_DIR}/summary.json: updates applied, committee adoptions, period crossings, every interval "
            "between updates (the longest, those over 1 h and 3 h), refusals, unfinished jobs, senders, lag",
       against=f"the transaction list {RECORD_DIR}/transactions.tsv (one line per transaction to the attestation "
               "program, decoded from the chain) and chain.json; the list's own consistency (blocks in order, each "
               "finish after its job's open, slots and periods as their decoding gives)",
       inputs=(RECORD_DIR,))
def check_record(ctx):
    d = ctx.root / RECORD_DIR
    if not (d / "transactions.tsv").exists():
        raise CheckFailed(f"{RECORD_DIR}/transactions.tsv is missing")
    rows = read_tsv(d / "transactions.tsv")
    chain = json.loads((d / "chain.json").read_text())
    summary = json.loads((d / "summary.json").read_text())
    problems = []
    last_block = -1
    opened = {}
    for i, r in enumerate(rows, 2):
        b = int(r["block"])
        if b < last_block:
            problems.append(f"line {i}: block {b} before block {last_block}")
        last_block = b
        if len(r["tx"]) != 64 or len(r["block_hash"]) != 64:
            problems.append(f"line {i}: a hash is not 32 bytes of hex")
        if r["step"] in ("open", "init") and r["outcome"] == "applied":
            opened[r["job"]] = r
        if r["step"] in ("miller", "adopt", "finish") and r["job"] != "-":
            o = opened.get(r["job"])
            if o is None:
                problems.append(f"line {i}: {r['step']} of job {r['job'][:8]}… with no open or init before it")
            elif o["instance"] != r["instance"]:
                problems.append(f"line {i}: {r['step']}'s instance is not its job's")
            elif o["step"] == "open" and any(r[k] != o[k] for k in UPDATE_FIELDS):
                problems.append(f"line {i}: {r['step']}'s update fields are not its open's")
        if chain["as_of"] and b > chain["as_of"]["block"]:
            problems.append(f"line {i}: block {b} after the as-of block {chain['as_of']['block']}")
    if (d / "RECORD.md").read_text() != render(summary):
        problems.append("RECORD.md is not what summary.json renders to")
    again = summarize(rows, chain)
    if json.dumps(again, sort_keys=True) != json.dumps(summary, sort_keys=True):
        a = json.loads(json.dumps(again))
        for inst, v in a["instances"].items():
            for k, vv in v.items():
                if summary["instances"].get(inst, {}).get(k) != vv:
                    problems.append(f"summary.json: instance {inst[:8]}…'s {k} is not what the list gives")
        for k in ("transactions", "as_of", "chain"):
            if a.get(k) != summary.get(k):
                problems.append(f"summary.json: {k} is not what the list gives")
        if not problems:
            problems.append("summary.json differs from the figures recomputed from the list")
    fail_if(problems, "the run record")
    lines = [f"as of LEZ block {chain['as_of']['block']} ({chain['as_of']['time_utc']}); {len(rows)} transactions "
             f"to the attestation program; {chain['blocks']} blocks walked, each hash recomputed and linked"]
    for inst, v in summary["instances"].items():
        iv = v["intervals"]
        lg = iv["longest"]
        lines.append(f"instance {inst[:10]}…: {v['updates_applied']} updates applied, "
                     f"{len(v['committee_adoptions'])} committee adoptions, {len(v['period_crossings'])} period "
                     f"crossings; longest interval {lg['seconds'] / 60:.1f} min" if lg else
                     f"instance {inst[:10]}…: {v['updates_applied']} updates applied")
        if lg:
            left = [j for j in v["jobs_opened_not_finished"] if not j["in_progress_at_as_of"]]
            running = len(v["jobs_opened_not_finished"]) - len(left)
            lines.append(f"  intervals over 1 h: {len(iv['over_1h'])}, over 3 h: {len(iv['over_3h'])}; "
                         f"refused: {len(v['refused'])}; jobs opened and left unfinished: {len(left)}"
                         + (f"; {running} job running at the as-of block" if running else ""))
    return lines


@check(id="run-record-online", title="The run record against the public chain: every block and transaction read again",
       family="run-record", mode="online", evidence=(242, 248), rows=("RFP-021 S.1",),
       what=f"{RECORD_DIR}/transactions.tsv and chain.json: every line, and that no transaction to the attestation "
            "program is missing from the list",
       against="the network's public RPC: blocks 1 to the as-of block read again (getBlockRange), each hash "
               "recomputed from its bytes and linked to the one before, each transaction decoded again; the program's "
               "loader header naming the published image id",
       needs=("python3", "network"), inputs=(RECORD_DIR,))
def check_record_online(ctx):
    d = ctx.root / RECORD_DIR
    rows = read_tsv(d / "transactions.tsv")
    chain = json.loads((d / "chain.json").read_text())
    rpc = lez.Rpc(ctx.lez_url + "/rpc")
    try:
        tip = rpc.call("getLastBlockId")
    except lez.RpcError as e:
        raise Skipped(f"the network's RPC did not answer: {e}") from e
    as_of = chain["as_of"]["block"]
    if tip < as_of:
        raise CheckFailed(f"the network's head {tip} is below the record's as-of block {as_of}: another chain?")
    program = lez.b58decode(chain["program_account"])
    cache = (ctx.cache or ctx.root / ".scratch/verify-cache") / "lez-blocks"
    lines, chain2, problems = scan(_cached_or_fetch(rpc, 1, as_of, cache), program,
                                   lc.Idl.load(ctx.root / "idl/ferrata_attestation.idl.json"), last=as_of)
    if chain2["as_of"]["hash"] != chain["as_of"]["hash"]:
        problems.append(f"block {as_of}'s hash on the chain is {chain2['as_of']['hash'][:16]}…, the record says "
                        f"{chain['as_of']['hash'][:16]}…")
    buf = io.StringIO()
    w = csv.DictWriter(buf, fieldnames=COLUMNS, delimiter="\t", lineterminator="\n")
    for ln in lines:
        w.writerow({k: ln[k] for k in COLUMNS})
    got = list(csv.DictReader(io.StringIO("\t".join(COLUMNS) + "\n" + buf.getvalue()), delimiter="\t"))
    if len(got) != len(rows):
        problems.append(f"the chain holds {len(got)} transactions to the attestation program up to block {as_of}; "
                        f"the record lists {len(rows)}")
    by_tx = {r["tx"]: r for r in got}
    for i, r in enumerate(rows, 2):
        g = by_tx.get(r["tx"])
        if g is None:
            problems.append(f"line {i}: transaction {r['tx'][:16]}… is not in the chain's blocks 1-{as_of}")
            continue
        diff = [k for k in COLUMNS if g[k] != r[k]]
        if diff:
            problems.append(f"line {i}: {r['tx'][:16]}… differs in {', '.join(diff)}")
    probs, states = reconcile_state(rpc, lc.Idl.load(ctx.root / "idl/ferrata_attestation.idl.json"), program, got,
                                    cache, as_of)
    problems += probs
    fail_if(problems, "the run record against the chain")
    counters = "; ".join(f"{k[:10]}… {v['applied']} applied jobs at block {v['head']} "
                         f"({v['applied_by_as_of']} by the as-of block)" for k, v in states.items())
    return [f"blocks 1-{as_of} read from {ctx.lez_url}/rpc ({rpc.requests} requests, {rpc.bytes / 1e6:.1f} MB; "
            f"a cache in {cache} is reused): every hash recomputed and linked",
            f"{len(rows)} of {len(rows)} transactions equal, field by field; none missing; head now {tip}",
            f"each instance's own counter and finalized slot equal what the chain's finishes give: {counters}"]


@check(id="run-record-beacon", title="Every header the light client applied is Sepolia's canonical block at its slot",
       family="beacon", mode="online", evidence=(242, 248), rows=("RFP-021 S.1",),
       what=f"the finalized header (slot and hash_tree_root) of every update the record lists as applied, "
            f"decoded from the update the chain holds ({RECORD_DIR}/transactions.tsv)",
       against="public Sepolia beacon APIs (scripts/pubnet-endpoints.json, or --beacon): /eth/v1/beacon/headers/SLOT's "
               "root, canonical", needs=("python3", "network"), inputs=(RECORD_DIR,))
def check_record_beacon(ctx):
    rows = read_tsv(ctx.root / RECORD_DIR / "transactions.tsv")
    want = sorted({(int(r["finalized_slot"]), r["finalized_root"]) for r in rows
                   if r["step"] == "finish" and r["outcome"] == "applied"})
    problems = []
    used = {}
    for slot, root in want:
        got = beacon_header(ctx.beacon, slot)
        if got is None:
            raise Skipped(f"no beacon API answered for slot {slot} ({', '.join(ctx.beacon)})")
        broot, canonical, url = got
        used[url] = used.get(url, 0) + 1
        if broot != root or not canonical:
            problems.append(f"slot {slot}: the record's header root {root[:16]}… is not the canonical block's "
                            f"{broot[:16]}… ({url})")
    fail_if(problems, "the applied headers against Sepolia")
    return [f"{len(want)} of {len(want)} applied finalized headers are Sepolia's canonical blocks at their slots "
            f"({', '.join(f'{u}: {n}' for u, n in used.items())})"]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("out")
    b.add_argument("--lez", help="the test network's site (default: scripts/reproduce/deployed-programs.json's)")
    b.add_argument("--last", type=int, help="the as-of block (default: the head when the run starts)")
    b.add_argument("--cache", default=str(Path(__file__).resolve().parents[2] / ".scratch/verify-cache/lez-blocks"))
    a = ap.parse_args(argv)
    root = Path(__file__).resolve().parents[2]
    site = a.lez or json.loads((root / "scripts/reproduce/deployed-programs.json").read_text())["network"]
    s = build(Path(a.out), site, Path(a.cache), a.last)
    print(json.dumps({k: v for k, v in s.items() if k != "instances"}, indent=1))
    for inst, v in s["instances"].items():
        print(inst, v["updates_applied"], "updates;", len(v["committee_adoptions"]), "adoptions;",
              "longest", v["intervals"]["longest"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
