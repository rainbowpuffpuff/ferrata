#!/usr/bin/env python3
"""Fetch a fresh Sepolia checkpoint for the crown run's instance (`lez/crown/driver`, `sepolia_base`).

The attestation program bounds every transaction that creates or extends an instance by the weak-subjectivity
period of the instance's finalized header, by the sequencer's clock (`crates/ferrata-core/src/ws.rs`): an `Init`
from a checkpoint older than the period reverts ("Execution outside of the validity window"). The replay fixture
`sepolia-gloas-2026-11-10` starts at the period-1378 bootstrap (5 Oct 2026), which left the 3,532-epoch period on
21 Oct; this script fetches a checkpoint inside the period, in the Gloas layout:

1. the latest light-client finality update (SSZ, Gloas layout) from Lodestar: the current sync-committee period and
   Lodestar's finalized checkpoint;
2. a bootstrap (Gloas layout): for the period's first checkpoint (the first epoch-start slot holding a block, its
   root from the second beacon provider) if Lodestar serves it, else for Lodestar's finalized checkpoint (on 12 Nov
   2026 Lodestar served a Gloas bootstrap for that checkpoint alone; refusals are recorded in SOURCE.md); its header
   must hash to the root the second provider reports at its slot;
3. every period's light-client update from the bootstrap's period to the current one (SSZ), each attested, signed
   and finalized in its period, carrying the next sync committee, and relevant to a store fresh from the bootstrap
   (newer than it, or the next committee of the store's own period); the second provider's root of every
   finalized header named.

Python standard library only; it reuses the helpers of `fetch-mainnet-e2e.py` and the parsers of
`fetch-sepolia-gloas.py`. Every request goes to a public endpoint without an API key, with the generic User-Agent
`ferrata-fixture-fetch` and nothing else that identifies the caller.

Usage: scripts/fetch-sepolia-checkpoint.py OUTDIR      (OUTDIR must not exist yet)
"""
import datetime
import hashlib
import importlib.util
import json
import os
import sys
import time
import urllib.error

_spec = importlib.util.spec_from_file_location("gloas", os.path.join(os.path.dirname(os.path.abspath(__file__)), "fetch-sepolia-gloas.py"))
g = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(g)
e2e = g.e2e

LODESTAR, SECOND = g.BEACON
SLOTS_PER_PERIOD = g.SLOTS_PER_PERIOD
GENESIS_TIME = 1_655_733_600
WS_PERIOD_EPOCHS = 3_532  # lez/live/driver/src/plan.rs sepolia_config
BOOTSTRAP_LEN = g.HEADER + g.COMMITTEE + 11 * 32  # Gloas LightClientBootstrap: header, committee, branch (gindex 2945)


def utc(slot):
    return datetime.datetime.fromtimestamp(GENESIS_TIME + 12 * slot, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def ws_end_slot(slot):
    """`weak_subjectivity_end_slot`: the first slot outside the period of a finalized header at `slot`."""
    return (slot // 32 + WS_PERIOD_EPOCHS + 1) * 32


def failed(endpoint, path, err):
    body = err.read()[:120].decode(errors="replace") if hasattr(err, "read") else ""
    e2e.LOG.append({"at": e2e.now(), "endpoint": endpoint, "request": f"GET {path}", "file": None,
                    "note": f"HTTP {getattr(err, 'code', '?')}: {body}".replace("|", "/").replace("\n", " ")})


def header_at(f, slot, name=None):
    """The second provider's canonical header at `slot` (its JSON answer), or None for an empty slot."""
    path = f"/eth/v1/beacon/headers/{slot}"
    try:
        raw = f.beacon(SECOND, path, name, ssz=False)
    except urllib.error.HTTPError as err:
        failed(SECOND, path, err)
        if err.code == 404:
            return None
        raise
    d = json.loads(raw)["data"]
    if int(d["header"]["message"]["slot"]) != slot or d.get("canonical") is False:
        raise SystemExit(f"the second provider's header at slot {slot} is not canonical at that slot")
    return raw, d["root"]


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out)
    f = e2e.Fetcher(out)
    started = e2e.now()

    # The period's first checkpoint, then the finalized one: Lodestar serves a Gloas bootstrap only for the
    # checkpoint it holds the committee witness of (12 Nov 2026: its current finalized checkpoint alone).
    boot, tried_first = None, False
    for attempt in range(3):
        fu = g.parse_finality_update(f.beacon(LODESTAR, "/eth/v1/beacon/light_client/finality_update", "finality_update.ssz", ssz=True))
        fin = fu["finalized"]
        period = fu["signature_slot"] // SLOTS_PER_PERIOD
        if fin["slot"] // SLOTS_PER_PERIOD != period:
            raise SystemExit("finalized slot and signature slot are in different periods; retry later")
        fin_root = "0x" + e2e.beacon_header_root(fin["beacon"])
        print(f"finality update: finalized slot {fin['slot']} (period {period}), signature slot {fu['signature_slot']}", flush=True)
        candidates = []
        if not tried_first:
            tried_first = True
            for epoch in range(period * SLOTS_PER_PERIOD // 32, fin["slot"] // 32):
                got = header_at(f, epoch * 32)
                if got is not None:
                    candidates.append((epoch * 32, got[1], got[0]))
                    break
        candidates.append((fin["slot"], fin_root, None))
        for slot, root, raw_header in candidates:
            path = f"/eth/v1/beacon/light_client/bootstrap/{root}"
            try:
                b = f.beacon(LODESTAR, path, None, ssz=True)
            except urllib.error.HTTPError as err:
                failed(LODESTAR, path, err)
                continue
            if len(b) != BOOTSTRAP_LEN:
                raise SystemExit(f"the bootstrap of slot {slot} is {len(b)} bytes, not the Gloas layout's {BOOTSTRAP_LEN}")
            if raw_header is None:
                got = header_at(f, slot)
                if got is None or got[1] != root:
                    raise SystemExit(f"the second provider reports another root at slot {slot}")
                raw_header = got[0]
            if e2e.u64(b, 0) != slot or "0x" + e2e.beacon_header_root(b[:112]) != root:
                raise SystemExit(f"the bootstrap of slot {slot} does not hash to the second provider's root {root}")
            f.save("bootstrap.ssz", b)
            f.save(f"beacon_header_{slot}_second.json", raw_header)
            boot = (slot, root)
            break
        if boot is not None:
            break
        time.sleep(20)  # the finalized checkpoint moved between the two requests
    if boot is None:
        raise SystemExit(f"Lodestar served no bootstrap of period {period}")
    boot_slot, boot_root = boot
    boot_period = boot_slot // SLOTS_PER_PERIOD
    print(f"bootstrap: slot {boot_slot} (period {boot_period}), root {boot_root}", flush=True)

    # Every period's update from the bootstrap's period to the current one.
    count = period - boot_period + 1
    stream = f.beacon(LODESTAR, f"/eth/v1/beacon/light_client/updates?start_period={boot_period}&count={count}", None, ssz=True)
    chunks = g.split_chunks(stream)
    if len(chunks) != count:
        raise SystemExit(f"{len(chunks)} updates for {count} periods")
    rows, check = [], [(fin["slot"], fin_root)]
    for i, (digest, payload) in enumerate(chunks):
        p = boot_period + i
        u = g.parse_update(payload)
        if u["layout"] != "gloas":
            raise SystemExit(f"update {p}: {u['layout']} layout, not Gloas")
        if u["signature_slot"] // SLOTS_PER_PERIOD != p or u["attested"] // SLOTS_PER_PERIOD != p:
            raise SystemExit(f"update {p} is not attested and signed in period {p}")
        branch_at = g.HEADER + g.COMMITTEE
        if payload[branch_at:branch_at + 11 * 32] == bytes(11 * 32):
            raise SystemExit(f"update {p} carries no next sync committee")
        fslot = e2e.u64(u["finalized_beacon"], 0)
        if fslot // SLOTS_PER_PERIOD != p:
            raise SystemExit(f"update {p} finalizes slot {fslot}, outside its period")
        # Relevant to a store fresh from the bootstrap: newer than it, or (validate_light_client_update's
        # update_has_next_sync_committee) the next committee of the store's own period, which such a store lacks.
        relevance = "newer than the bootstrap" if u["attested"] > boot_slot else (
            "next committee of the store's period (finalized header older than the bootstrap)" if p == boot_period else None)
        if relevance is None:
            raise SystemExit(f"update {p} is irrelevant to a store at slot {boot_slot}")
        f.save(f"update_{p}.ssz", payload)
        froot = "0x" + e2e.beacon_header_root(u["finalized_beacon"])
        signers = sum(bin(x).count("1") for x in u["bits"])
        rows.append(f"| {p} | {u['layout']} | 0x{digest.hex()} | {u['attested']} | {u['signature_slot']} | {fslot} | {signers}/512 | {relevance} |")
        check.append((fslot, froot))
    for slot, want in sorted(set(check)):
        got = header_at(f, slot, f"beacon_header_{slot}_second.json")
        if got is None or got[1] != want:
            raise SystemExit(f"second beacon provider reports another root at slot {slot}")

    end = ws_end_slot(boot_slot)
    files = sorted(x for x in os.listdir(out) if x not in ("SOURCE.md", "SHA256SUMS"))
    sums = {x: hashlib.sha256(open(os.path.join(out, x), "rb").read()).hexdigest() for x in files}
    with open(os.path.join(out, "SHA256SUMS"), "w") as s:
        s.writelines(f"{sums[x]}  {x}\n" for x in files)
    lines = [
        f"# Sepolia: a Gloas bootstrap of period {boot_period} (slot {boot_slot:,}) and the period updates from it",
        "",
        f"Fetched by `scripts/fetch-sepolia-checkpoint.py` between {started} and {e2e.now()} (UTC). Every request went",
        f"to a public endpoint without an API key, with User-Agent `{e2e.UA}`. Responses are stored byte for byte,",
        "except the `updates` stream, which is split into one file per period with its 8-byte length and 4-byte fork",
        "digest removed (the digests are listed below). `SHA256SUMS` pins every file.",
        "",
        "**Why a new checkpoint.** The attestation program bounds every transaction that creates or extends an",
        "instance by the weak-subjectivity period of the instance's finalized header, by the sequencer's clock",
        "(`crates/ferrata-core/src/ws.rs`); the crown run's instance is configured with `ws_period_epochs` 3,532",
        "(`sepolia_config`). The period-1378 bootstrap of `sepolia-gloas-2026-11-10` (slot 11,288,576, 5 Oct 2026)",
        f"left that period at slot {ws_end_slot(11288576):,} ({utc(ws_end_slot(11288576))}), so the crown run's `Init`",
        "from it reverted on LEZ (`crown-live-2`, 12 Nov 2026). On 10 Nov Lodestar served no Gloas-era bootstrap",
        "(that fixture's SOURCE.md). Now it serves one for its current finalized checkpoint and answers HTTP 500",
        "\"syncCommitteeWitness not available\" for earlier ones (recorded below for the period's first checkpoint;",
        "on 12 Nov 2026 around 10:40Z also for the first 24 epoch-start checkpoints of period 1411, by a first version",
        "of this script, and for the 7 checkpoints before the then finalized one, by hand), so the bootstrap is the",
        "checkpoint Lodestar had finalized at the fetch.",
        "",
        "**The period's update is older than the bootstrap, and still taken.** Lodestar's best update of the",
        "bootstrap's period finalizes that period's first checkpoint, before the bootstrap. A store fresh from the",
        "bootstrap does not know the next sync committee, so the update is relevant all the same: it carries the next",
        "committee of the store's own period (`validate_light_client_update`'s `update_has_next_sync_committee`;",
        "`crates/ferrata-core/src/validate.rs`), and `apply_light_client_update` adopts that committee without moving",
        "the finalized header. The store then follows the chain into the next period, as the old fixture's did.",
        "",
        f"**When this checkpoint leaves the period.** Slot {boot_slot:,} is in epoch {boot_slot // 32:,}; its period ends",
        f"at the start of epoch {boot_slot // 32 + WS_PERIOD_EPOCHS + 1:,}, slot {end:,}, **{utc(end)}**. An `Init` from",
        "this checkpoint is valid only before then (each later transaction is bounded by the period of the store's",
        "finalized header at that time, which the updates move forward).",
        "",
        "| | |",
        "|---|---|",
        "| chain | Sepolia: chain id 11155111, `genesis_validators_root` 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078; Gloas since epoch 353,024 (version 0x90000076) |",
        f"| bootstrap (Gloas layout, {BOOTSTRAP_LEN:,} bytes) | slot {boot_slot:,} ({utc(boot_slot)}), period {boot_period}, block root {boot_root} (the second provider's, `beacon_header_{boot_slot}_second.json`) |",
        f"| its weak-subjectivity end (3,532 epochs) | slot {end:,}, {utc(end)} |",
        f"| finality update (Gloas) | attested slot {fu['attested']['slot']:,}, signature slot {fu['signature_slot']:,}, {fu['participants']}/512 signers; finalized slot {fin['slot']:,}, root {fin_root} |",
        "",
        "| period | layout | fork digest | attested slot | signature slot | finalized slot | signers | relevant to the bootstrapped store as |",
        "|---|---|---|---|---|---|---|---|",
    ] + rows + [
        "",
        "Cross-checks made by the script: the bootstrap is in the Gloas layout and its header hashes to the root the",
        "second beacon provider (ethereum-sepolia-beacon-api.publicnode.com) reports as canonical at its slot; every",
        "update is attested, signed and finalized in its own period, carries a next sync committee, and is relevant",
        "to the bootstrapped store (last column); the second provider reports the root of every finalized header",
        "named (`beacon_header_*_second.json`), the finality update's included. The bootstrap's committee branch and",
        "the updates' signatures are verified by the",
        "light client itself (`crown-driver check --chain sepolia`, the driver's tests, and the program's `Init`).",
        "",
        "## Requests",
        "",
        "| UTC | endpoint | request | file | note |",
        "|---|---|---|---|---|",
    ]
    lines += [f"| {r['at']} | {r['endpoint']} | `{r['request']}` | {r['file'] or ''} | {r['note']} |" for r in e2e.LOG]
    with open(os.path.join(out, "SOURCE.md"), "w") as s:
        s.write("\n".join(lines) + "\n")
    print(f"done: {out}; the checkpoint leaves the weak-subjectivity period at slot {end} ({utc(end)})")


if __name__ == "__main__":
    main()
