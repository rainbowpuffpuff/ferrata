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
import re
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
    """UTC milliseconds of `YYYY-MM-DDTHH:MM:SS.mmmZ`, whatever the machine's time zone."""
    import calendar
    base, frac = s.rstrip("Z").split(".")
    return calendar.timegm(time.strptime(base, "%Y-%m-%dT%H:%M:%S")) * 1000 + int(frac)


def _actor_ids(tx) -> list:
    return [a["account_id"] for a in tx["public_context"]["actors"]]


def scan(blocks, program: bytes, idl: lc.Idl, last: int | None = None, first: int = 1, instances: dict | None = None):
    """Walk the blocks in order: hashes recomputed and linked, every transaction to `program` decoded.
    Returns (lines, chain): the record's lines and what the walk saw of the chain."""
    lines = []
    prev = None
    n = 0
    gaps = []
    instances = dict(instances or {})  # instance account -> the chain configuration of its Init
    jobs = {}  # job account -> the open line's fields
    first_ts = None
    problems = []
    for raw in blocks:
        b = lez.decode_block(raw)
        h = b["header"]
        if h["block_id"] < first:
            continue
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
                # Init writes instance(id) and the job of its first committee's adoption (job(id, 0…)). Only an
                # applied Init counts: a refused one (a re-Init of a live instance, say) changes nothing on chain,
                # so it changes nothing here either.
                inst = lez.b58encode(root["to"]["account_id"])
                cfg = args["config"]
                job = next((a for a in ids if a != inst), "-")
                if line["outcome"] == "applied":
                    instances[inst] = cfg
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
        as_of_ms = parse_iso(chain["as_of"]["time_utc"]) if chain.get("as_of") else None
        # A job with no finish opened before the newest finish was left: the updater went on to later updates. One
        # opened after it is open at the as-of block: running, or left; the chain up to that block cannot tell.
        unfinished = [{"block": int(r["block"]), "time_utc": r["time_utc"], "job": r["job"], "sender": r["sender"],
                       "finalized_slot": _int(r["finalized_slot"]),
                       "open_at_as_of": int(r["block"]) > last_finish,
                       "opened_s_before_as_of": round((as_of_ms - parse_iso(r["time_utc"])) / 1000, 3)
                       if as_of_ms is not None else None}
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
        opened_at = {r["job"]: parse_iso(r["time_utc"]) for r in opens if r["outcome"] == "applied"}
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


def reconcile_state(rpc: lez.Rpc, idl: lc.Idl, program: bytes, lines: list, cache: Path, as_of: int,
                    expected: set | None = None) -> list:
    """Each instance's on-chain counter against the chain: `applied` (Init's activation and every update the program
    applied) must equal 1 + the applied finishes in the chain's blocks up to the head read with it, and the store's
    finalized slot the newest applied update's. The instances are those the chain's applied Inits set (not the
    record's lines); `expected`, the record's, must be the same set. Returns (problems, states)."""
    problems = []
    for attempt in range(3):
        head = rpc.call("getLastBlockId")
        later, now, probs = scan(_cached_or_fetch(rpc, 1, head, cache), program, idl, last=head)
        states = {inst: instance_state(rpc, idl, program, inst) for inst in sorted(now["instances"])}
        if rpc.call("getLastBlockId") == head:
            break
    else:
        problems.append("the network's head moved while the counters were read, three times")
    problems += probs
    if not states:
        problems.append(f"the chain's blocks 1-{head} hold no applied Init of the attestation program: no counter to read")
    if expected is not None and set(states) != set(expected):
        problems.append(f"the chain's instances {sorted(states)} are not the record's {sorted(expected)}")
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
    chain["first_block"] = 1
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
        left = [j for j in v["jobs_opened_not_finished"] if not j["open_at_as_of"]]
        still = [j for j in v["jobs_opened_not_finished"] if j["open_at_as_of"]]
        out.append(f"- Refused (in the block's reverted list): {len(v['refused'])}. Jobs opened and left unfinished "
                   f"(an update finished after them, theirs never): {len(left)}"
                   + (f"; open at the as-of block, opened {', '.join(_dur(j['opened_s_before_as_of']) for j in still)} "
                      "before it (running or left: the chain up to that block cannot tell)" if still else "") + ".")
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


HEX64 = re.compile(r"[0-9a-f]{64}")
ISO_MS = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z")


def chain_consistency(rows: list, chain: dict) -> list:
    """What the record's chain facts (chain.json) must satisfy with its own transaction list: the shape of each fact,
    times and hashes agreeing with the lines, the pauses the lines prove, the instances their Inits set, the counters
    their finishes give. Returns problems. None of this reads the chain: run-record-online does."""
    problems = []
    a = chain.get("as_of") or {}
    if not (isinstance(a.get("block"), int) and HEX64.fullmatch(str(a.get("hash", ""))) and ISO_MS.fullmatch(str(a.get("time_utc", "")))):
        return [f"chain.json: as_of is not a block number, a 32-byte hash and a time: {a}"]
    first = chain.get("first_block")
    if first != 1:
        problems.append(f"chain.json: first_block is {first}; the record reads the chain from block 1")
        first = 1
    if chain.get("blocks") != a["block"] - first + 1:
        problems.append(f"chain.json: {chain.get('blocks')} blocks read, but blocks {first}-{a['block']} are "
                        f"{a['block'] - first + 1}")
    as_of_ms = parse_iso(a["time_utc"])
    fbt = chain.get("first_block_with_time") or {}
    if not (isinstance(fbt.get("block"), int) and ISO_MS.fullmatch(str(fbt.get("time_utc", "")))) or \
            not first <= fbt["block"] <= a["block"]:
        problems.append(f"chain.json: first_block_with_time is not a block of the record with a time: {fbt}")
        fbt = None
    read_at = str(chain.get("read_at_utc", ""))
    if not re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ", read_at) or \
            parse_iso(read_at.replace("Z", ".999Z")) < as_of_ms:
        problems.append(f"chain.json: read at {read_at!r}, before its as-of block's time {a['time_utc']}")
    # the lines against the as-of block, and against each other
    seen, prev, by_block = set(), None, {}
    for i, r in enumerate(rows, 2):
        if not (HEX64.fullmatch(r["tx"]) and HEX64.fullmatch(r["block_hash"]) and ISO_MS.fullmatch(r["time_utc"])):
            problems.append(f"line {i}: its hashes or time are not well formed")
            continue
        b, t = int(r["block"]), parse_iso(r["time_utc"])
        if r["tx"] in seen:
            problems.append(f"line {i}: transaction {r['tx'][:16]}… listed twice")
        seen.add(r["tx"])
        if b in by_block and by_block[b] != (r["block_hash"], r["time_utc"]):
            problems.append(f"line {i}: block {b}'s hash or time differs from an earlier line's")
        by_block[b] = (r["block_hash"], r["time_utc"])
        if b == a["block"] and r["block_hash"] != a["hash"]:
            problems.append(f"line {i}: the as-of block's hash is not chain.json's")
        if prev is not None:
            pb, pi, pt = prev
            if (b, int(r["index"])) <= (pb, pi):
                problems.append(f"line {i}: not after the line before it (block, index)")
            elif t < pt:
                problems.append(f"line {i}: block {b}'s time is before block {pb}'s")
        prev = (b, int(r["index"]), t)
    if by_block:
        lo, hi = min(by_block), max(by_block)
        if lo < first or hi > a["block"]:
            problems.append(f"chain.json: the lines' blocks {lo}-{hi} are not within the record's blocks "
                            f"{first}-{a['block']}")
        newest = max(parse_iso(v[1]) for v in by_block.values())
        if newest > as_of_ms:
            problems.append(f"chain.json: the as-of block's time {a['time_utc']} is before the newest line's {iso(newest)}")
        if fbt and min(parse_iso(v[1]) for v in by_block.values()) < parse_iso(fbt["time_utc"]):
            problems.append(f"chain.json: the first block with a time ({fbt['time_utc']}) is after a line's time")
    # the pauses: listed ones well formed; and between two lines with no pause listed in between, each block came
    # within 60 s of the one before, so the lines' times differ by at most 60 s a block
    gaps = chain.get("block_gaps_over_60s")
    if not isinstance(gaps, list):
        problems.append("chain.json: block_gaps_over_60s is not a list")
        gaps = []
    after = []
    for g in gaps:
        try:
            ok = (first <= g["after_block"] < a["block"] and ISO_MS.fullmatch(g["from_utc"]) and ISO_MS.fullmatch(g["to_utc"])
                  and round((parse_iso(g["to_utc"]) - parse_iso(g["from_utc"])) / 1000, 3) == g["seconds"] and g["seconds"] > 60)
        except (KeyError, TypeError, ValueError):
            ok = False
        if not ok:
            problems.append(f"chain.json: a pause is not well formed: {g}")
        else:
            after.append(g["after_block"])
    if after != sorted(set(after)):
        problems.append("chain.json: the pauses are not in block order, or one is listed twice")
    blocks = sorted(by_block.items())
    for (b1, (_, t1)), (b2, (_, t2)) in zip(blocks, blocks[1:]):
        if not any(b1 <= x < b2 for x in after) and parse_iso(t2) - parse_iso(t1) > 60_000 * (b2 - b1):
            problems.append(f"blocks {b1} to {b2}: {(parse_iso(t2) - parse_iso(t1)) / 1000:.0f} s for {b2 - b1} "
                            "blocks, but chain.json lists no pause of more than 60 s between them")
    # the instances: those the applied Inits of the list set, with their configuration
    inits = {r["instance"]: r for r in rows if r["step"] == "init" and r["outcome"] == "applied"}
    insts = chain.get("instances") or {}
    if set(insts) != set(inits):
        problems.append(f"chain.json: instances {sorted(insts)} are not the list's applied Inits {sorted(inits)}")
    for inst, r in inits.items():
        cfg = insts.get(inst) or {}
        want = (f"chain_id={cfg.get('chain_id')} committee={cfg.get('sync_committee_size')} "
                f"adoption_parts={cfg.get('adoption_parts')}")
        if inst in insts and r["detail"] != want:
            problems.append(f"instance {inst[:10]}…: its Init line says {r['detail']!r}, chain.json {want!r}")
    named = {r["instance"] for r in rows if r["instance"] != "-"} - set(inits)
    if named:
        problems.append(f"lines name instances no applied Init of the list set: {sorted(named)}")
    # the counters read with the record: 1 (Init) + the applied finishes, and the newest one's finalized slot
    counters = chain.get("instance_counters_read") or {}
    if set(counters) != set(inits):
        problems.append(f"chain.json: counters read for {sorted(counters)}, the instances are {sorted(inits)}")
    for inst, c in counters.items():
        fin = [r for r in rows if r["instance"] == inst and r["step"] == "finish" and r["outcome"] == "applied"]
        try:
            if c["applied_by_as_of"] != 1 + len(fin):
                problems.append(f"instance {inst[:10]}…: counter by the as-of block {c['applied_by_as_of']}, the list's "
                                f"finishes give 1 + {len(fin)}")
            if c["head"] < a["block"] or c["applied"] < c["applied_by_as_of"]:
                problems.append(f"instance {inst[:10]}…: counter read at block {c['head']}: {c['applied']} applied")
            if c["head"] == a["block"]:
                if c["applied"] != c["applied_by_as_of"]:
                    problems.append(f"instance {inst[:10]}…: read at the as-of block, {c['applied']} applied, not "
                                    f"{c['applied_by_as_of']}")
                if fin and c["finalized_slot"] != int(fin[-1]["finalized_slot"]):
                    problems.append(f"instance {inst[:10]}…: store's finalized slot {c['finalized_slot']}, the newest "
                                    f"applied update's is {fin[-1]['finalized_slot']}")
        except (KeyError, TypeError) as e:
            problems.append(f"instance {inst[:10]}…: counter entry incomplete ({e})")
    return problems


@check(id="run-record", title="The light client's run on our public LEZ test network: every figure recomputed",
       family="run-record", mode="offline", evidence=(242, 248), rows=("RFP-021 S.1",),
       what=f"{RECORD_DIR}/summary.json and RECORD.md: updates applied, committee adoptions, period crossings, every "
            "interval between updates (the longest, those over 1 h and 3 h), refusals, unfinished jobs, senders, lag",
       against=f"the transaction list {RECORD_DIR}/transactions.tsv (one line per transaction to the attestation "
               "program, decoded from the chain) and chain.json; their consistency with each other (blocks, times and "
               "hashes of the lines against the as-of block, the pauses the lines prove, the instances their Inits "
               "set, the counters their finishes give, each step after its job's open). The chain itself is read "
               "again by run-record-online, run-record-counters and run-record-beacon, not here",
       inputs=(RECORD_DIR,))
def check_record(ctx):
    d = ctx.root / RECORD_DIR
    if not (d / "transactions.tsv").exists():
        raise CheckFailed(f"{RECORD_DIR}/transactions.tsv is missing")
    rows = read_tsv(d / "transactions.tsv")
    chain = json.loads((d / "chain.json").read_text())
    summary = json.loads((d / "summary.json").read_text())
    if not rows:
        raise CheckFailed(f"{RECORD_DIR}/transactions.tsv lists no transaction")
    problems = chain_consistency(rows, chain)
    opened = {}
    for i, r in enumerate(rows, 2):
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
    lines = [f"as of LEZ block {chain['as_of']['block']} ({chain['as_of']['time_utc']}), as chain.json records it: "
             f"{len(rows)} transactions to the attestation program; the list and chain.json agree (times, hashes, "
             f"{len(chain['block_gaps_over_60s'])} pauses, instances, counters); not read from the chain here "
             "(run-record-online does)"]
    for inst, v in summary["instances"].items():
        iv = v["intervals"]
        lg = iv["longest"]
        lines.append(f"instance {inst[:10]}…: {v['updates_applied']} updates applied, "
                     f"{len(v['committee_adoptions'])} committee adoptions, {len(v['period_crossings'])} period "
                     f"crossings; longest interval {lg['seconds'] / 60:.1f} min" if lg else
                     f"instance {inst[:10]}…: {v['updates_applied']} updates applied")
        if lg:
            left = [j for j in v["jobs_opened_not_finished"] if not j["open_at_as_of"]]
            still = len(v["jobs_opened_not_finished"]) - len(left)
            lines.append(f"  intervals over 1 h: {len(iv['over_1h'])}, over 3 h: {len(iv['over_3h'])}; "
                         f"refused: {len(v['refused'])}; jobs opened and left unfinished: {len(left)}"
                         + (f"; {still} job open at the as-of block (running or left: the chain cannot tell yet)"
                            if still else ""))
    return lines


def _rows_of(lines: list) -> list:
    """The scan's lines as the TSV reads them back (strings)."""
    buf = io.StringIO()
    w = csv.DictWriter(buf, fieldnames=COLUMNS, delimiter="\t", lineterminator="\n")
    w.writeheader()
    for ln in lines:
        w.writerow({k: ln[k] for k in COLUMNS})
    return list(csv.DictReader(io.StringIO(buf.getvalue()), delimiter="\t"))


def check_record_online(ctx):
    d = ctx.root / RECORD_DIR
    rows = read_tsv(d / "transactions.tsv")
    chain = json.loads((d / "chain.json").read_text())
    rpc = lez.Rpc(ctx.lez_url + "/rpc", budget_s=120)
    try:
        tip = rpc.call("getLastBlockId")
    except lez.RpcError as e:
        raise Skipped(f"the network's RPC did not answer: {e}") from e
    problems = []
    if not rows:
        problems.append(f"{RECORD_DIR}/transactions.tsv lists no transaction: nothing to read again")
    program = program_account(ctx.lez_url, rpc)  # the network's /programs.json and the loader header on chain
    if lez.b58encode(program) != chain.get("program_account"):
        problems.append(f"the record names program {chain.get('program_account')}; the network's attestation program "
                        f"is {lez.b58encode(program)}")
    as_of = chain["as_of"]["block"]
    if tip < as_of:
        raise CheckFailed(f"the network's head {tip} is below the record's as-of block {as_of}: another chain?")
    # From the chain's first block, with no instance taken from the record; tests alone start later (Ctx)
    first, seed = (1, None) if ctx.record_from_block is None else (ctx.record_from_block, chain.get("instances", {}))
    cache = (ctx.cache or ctx.root / ".scratch/verify-cache") / "lez-blocks"
    lines, chain2, probs = scan(_cached_or_fetch(rpc, first, as_of, cache), program,
                                lc.Idl.load(ctx.root / "idl/ferrata_attestation.idl.json"), last=as_of, first=first,
                                instances=seed)
    problems += probs
    if chain2["as_of"] is None or chain2["as_of"]["block"] != as_of:
        problems.append(f"the chain's blocks {first}-{as_of} could not all be read")
    else:
        for k in ("hash", "time_utc"):
            if chain2["as_of"][k] != chain["as_of"][k]:
                problems.append(f"block {as_of}'s {k} on the chain is {chain2['as_of'][k]}, the record says "
                                f"{chain['as_of'][k]}")
    if chain2["blocks"] != chain["blocks"]:
        problems.append(f"{chain2['blocks']} blocks read from {first} to {as_of}; the record says {chain['blocks']}")
    for k in ("first_block_with_time", "block_gaps_over_60s", "instances"):
        if json.loads(json.dumps(chain2[k])) != chain.get(k):
            problems.append(f"chain.json's {k} is not what the chain's blocks {first}-{as_of} give")
    got = _rows_of(lines)
    if len(got) != len(rows):
        problems.append(f"the chain holds {len(got)} transactions to the attestation program in blocks {first}-{as_of}; "
                        f"the record lists {len(rows)}")
    by_tx = {r["tx"]: r for r in got}
    listed = {r["tx"] for r in rows}
    for i, r in enumerate(rows, 2):
        g = by_tx.get(r["tx"])
        if g is None:
            problems.append(f"line {i}: transaction {r['tx'][:16]}… is not in the chain's blocks {first}-{as_of}")
            continue
        diff = [k for k in COLUMNS if g[k] != r[k]]
        if diff:
            problems.append(f"line {i}: {r['tx'][:16]}… differs in {', '.join(diff)}")
    for g in got:
        if g["tx"] not in listed:
            problems.append(f"block {g['block']}: transaction {g['tx'][:16]}… ({g['step']}) is missing from the record")
    fail_if(problems, "the run record against the chain")
    return [f"blocks {first}-{as_of} read from {ctx.lez_url}/rpc ({rpc.requests} requests, {rpc.bytes / 1e6:.1f} MB; "
            f"blocks already read are kept in {cache.relative_to(ctx.root) if cache.is_relative_to(ctx.root) else cache}"
            "; delete it to read them again): every hash recomputed and linked to the one before",
            f"the program: the network's attestation program ({lez.b58encode(program)[:10]}…, its immutable loader "
            f"header naming image {ATTESTATION_IMAGE[:12]}…); the as-of block's hash and time, the pauses, the "
            f"instances' configurations: as the record says",
            f"{len(rows)} of {len(rows)} transactions equal their line, field by field; none missing; head now {tip}"]


check(id="run-record-online", title="The run record against the public chain: every block and transaction read again",
      family="run-record", mode="online", evidence=(242, 248), rows=("RFP-021 S.1",),
      what=f"{RECORD_DIR}/transactions.tsv and chain.json: every line, and that no transaction to the attestation "
           "program is missing from the list",
      against="the network's public RPC: the record's blocks read again (getBlockRange), each hash recomputed from "
              "its bytes and linked to the one before, each transaction to the program decoded again",
      needs=("python3", "network"), inputs=(RECORD_DIR,))(check_record_online)


@check(id="run-record-counters", title="Each instance's own counter of applied jobs against the chain's finishes",
       family="lez-network", mode="online", evidence=(242, 248), rows=("RFP-021 S.1",),
       what="each instance's InstanceState as the network serves it now: `applied` and its store's finalized slot",
       against="1 (Init's activation) + the applied finish steps in the chain's blocks up to the head read with it "
               f"(the record's {RECORD_DIR}/transactions.tsv up to its as-of block, the chain after it), and the "
               "newest one's finalized slot", needs=("python3", "network"), inputs=(RECORD_DIR,))
def check_record_counters(ctx):
    d = ctx.root / RECORD_DIR
    rows = read_tsv(d / "transactions.tsv")
    chain = json.loads((d / "chain.json").read_text())
    rpc = lez.Rpc(ctx.lez_url + "/rpc", budget_s=120)
    try:
        rpc.call("getLastBlockId")
    except lez.RpcError as e:
        raise Skipped(f"the network's RPC did not answer: {e}") from e
    program = program_account(ctx.lez_url, rpc)
    problems = []
    if lez.b58encode(program) != chain.get("program_account"):
        problems.append(f"the record names program {chain.get('program_account')}; the network's attestation program "
                        f"is {lez.b58encode(program)}")
    cache = (ctx.cache or ctx.root / ".scratch/verify-cache") / "lez-blocks"
    probs, states = reconcile_state(rpc, lc.Idl.load(ctx.root / "idl/ferrata_attestation.idl.json"), program, rows,
                                    cache, chain["as_of"]["block"], expected=set(chain.get("instances") or {}))
    fail_if(problems + probs, "the instances' counters")
    return [f"{k[:10]}…: {v['applied']} applied jobs at block {v['head']} = 1 + the finishes the chain holds "
            f"({v['applied_by_as_of']} by the record's as-of block); finalized slot {v['finalized_slot']:,}"
            for k, v in states.items()]


@check(id="run-record-beacon", title="Every header the light client applied is Sepolia's canonical block at its slot",
       family="beacon", mode="online", evidence=(242, 248), rows=("RFP-021 S.1",),
       what=f"the finalized header (slot and hash_tree_root) of every update the record lists as applied, "
            f"decoded from the update the chain holds ({RECORD_DIR}/transactions.tsv)",
       against="public Sepolia beacon APIs (scripts/pubnet-endpoints.json, or --beacon): /eth/v1/beacon/headers/SLOT's "
               "root, canonical", needs=("python3", "network"), inputs=(RECORD_DIR,))
def check_record_beacon(ctx):
    rows = read_tsv(ctx.root / RECORD_DIR / "transactions.tsv")
    applied = [r for r in rows if r["step"] == "finish" and r["outcome"] == "applied"]
    bad = [r["tx"][:16] for r in applied if not (r["finalized_slot"].isdigit() and HEX64.fullmatch(r["finalized_root"]))]
    if bad:
        raise CheckFailed(f"applied finishes with no decoded update (slot and root): {', '.join(bad[:5])}…")
    want = sorted({(int(r["finalized_slot"]), r["finalized_root"]) for r in applied})
    if not want:
        raise CheckFailed(f"{RECORD_DIR}/transactions.tsv lists no applied update: nothing to hold to Sepolia")
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
