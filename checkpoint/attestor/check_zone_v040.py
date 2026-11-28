#!/usr/bin/env python3
"""Check the records of checkpoint/attestor/zone-v040.sh (CI's sequencer-zone-v040 job, and the recorded runs).

    python3 -I checkpoint/attestor/check_zone_v040.py DIR

DIR holds a run's records (facts.json) or subdirectories that do (standalone/, vote/). Each facts.json is checked by
what its run did: a standalone run (a sequencer that made blocks, the sealed wallet, a transfer whose receipt says it
applied, a private transfer's when one ran, and with a program deployed, its record), a vote run (four keys accredited,
the two first votes not enough, the third replacing the params, the Bedrock channel taking the new posting timeframe,
and the unstake, run by default (`unstake_enabled` false says it was off): its stake paid no sooner than LEZ's release
rule allows, 2 x finality_depth - 1 node blocks after the key's removal (release_finality.rs, producible_releases; the
removing configuration final, chain.rs), measured from the removal as seen, less POLL_SLACK_BLOCKS since the heights are
read by polls ten seconds apart). Every failed check is printed; exit 1 if any,
2 if DIR holds no facts.json.
"""
import json
import os
import re
import sys

V040 = "v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115"
# zone-v040.sh reads the node's height every 10 s; with 4 s slots a height read can be up to 3 blocks late
POLL_SLACK_BLOCKS = 3
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def find(o, key):
    """The first value of `key` anywhere in a JSON value (None if absent)."""
    if isinstance(o, dict):
        if key in o:
            return o[key]
        for v in o.values():
            r = find(v, key)
            if r is not None:
                return r
    elif isinstance(o, list):
        for v in o:
            r = find(v, key)
            if r is not None:
                return r
    return None


def check_standalone(f):
    errs = []
    h0, h1 = f.get("standalone_first_height"), f.get("standalone_last_height")
    if not isinstance(h0, int) or h0 < 2:
        errs.append(f"standalone_first_height {h0!r}: no block 2")
    if not isinstance(h1, int) or not isinstance(h0, int) or h1 <= h0:
        errs.append(f"standalone_last_height {h1!r} is not past the first {h0!r}")
    if f.get("check_health") is not True:
        errs.append("wallet check-health did not pass")
    if f.get("wallet_storage_sealed") is not True:
        errs.append("the wallet's storage was not sealed")
    for k in ("test_account", "new_account"):
        if not str(f.get(k, "")).startswith("Public/"):
            errs.append(f"{k} {f.get(k)!r} is not a public account")
    if not HEX64.match(str(f.get("transfer_tx", ""))):
        errs.append(f"transfer_tx {f.get('transfer_tx')!r} is not a transaction hash")
    r = f.get("transfer_receipt")
    if find(r, "success") is not True:
        errs.append(f"the transfer's receipt does not say it applied: {json.dumps(r)[:200]}")
    tb = f.get("transfer_block")
    if not (isinstance(tb, int) or (isinstance(tb, str) and tb.isdigit())) or int(tb) < 2:
        errs.append(f"transfer_block {tb!r}: the transfer's block was not recorded")
    if f.get("private_enabled") is not False and not all(k in f for k in ("private_tx", "private_account", "private_receipt")):
        errs.append("the private transfer (run by default) has no complete record")
    if "private_tx" in f or "private_receipt" in f:
        if not HEX64.match(str(f.get("private_tx", ""))):
            errs.append(f"private_tx {f.get('private_tx')!r} is not a transaction hash")
        if not str(f.get("private_account", "")).startswith("Private/"):
            errs.append(f"private_account {f.get('private_account')!r} is not a private account")
        if find(f.get("private_receipt"), "success") is not True:
            errs.append(f"the private transfer's receipt does not say it applied: {json.dumps(f.get('private_receipt'))[:200]}")
    if "program_deploy" in f:
        d = f["program_deploy"]
        if not isinstance(d, dict):
            errs.append(f"program_deploy is not a record: {d!r}")
            return errs
        if not str(d.get("header", "")).startswith("Public/") or not d.get("segments"):
            errs.append(f"program_deploy is incomplete: {d}")
        if not re.match(r"^(0x)?[0-9a-f]{64}$", str(d.get("image_id", ""))):
            errs.append(f"program_deploy's image id {d.get('image_id')!r}")
    return errs


def check_vote(f):
    errs = []
    keys = f.get("channel_keys")
    if not (isinstance(keys, list) and len(keys) == 4 and all(HEX64.match(k) for k in keys) and len(set(keys)) == 4):
        errs.append(f"channel_keys {keys!r}: not four distinct keys")
    if not isinstance(f.get("four_keys_after_s"), int):
        errs.append("the channel never accredited four keys")
    before, voted = f.get("posting_timeframe_before"), f.get("posting_timeframe_voted")
    if not isinstance(before, int) or voted != before + 30:
        errs.append(f"posting_timeframe_voted {voted!r} is not the one in force ({before!r}) + 30")
    for i in (1, 2):
        v = str(f.get(f"vote_{i}", ""))
        if "now" in v or "of the 3 accredited keys" not in v:
            errs.append(f"vote {i} should not have been enough (t(4) = 3): {v[:160]!r}")
    v3 = str(f.get("vote_3", ""))
    if "The channel params are now" not in v3:
        errs.append(f"vote 3 did not replace the params: {v3[:160]!r}")
    # the Bedrock channel as read after each vote: the timeframe in force until the third
    for i in (1, 2):
        seen = f.get(f"channel_posting_timeframe_after_vote_{i}")
        if seen is not None and str(seen) != str(before):
            errs.append(f"the Bedrock channel had posting timeframe {seen!r} after vote {i}, not the one in force ({before!r})")
    if f.get("bedrock_took_params") is not True:
        errs.append("the Bedrock channel did not take the new posting timeframe")
    if f.get("unstake_enabled") is not False and "finality_depth" not in f:
        errs.append("the unstake under L1 finality (run by default) has no record: it did not complete")
    if "finality_depth" in f:  # LEZ PR 1026: a whole stake is paid once the key's removal is 2 x finality_depth - 1 deep
        fd = f["finality_depth"]
        try:
            h0, h1, h2 = (int(f.get(k) or -1) for k in ("unstake_node_height", "key_removed_node_height", "release_node_height"))
        except (TypeError, ValueError):
            h0 = h1 = h2 = -1
        if not isinstance(f.get("release_paid"), int) or f["release_paid"] <= 0:
            errs.append("the unstaked key's stake was not paid")
        elif min(h0, h1, h2) < 0:
            errs.append(f"node heights missing: unstake {h0}, removal {h1}, release {h2}")
        elif not isinstance(fd, int) or not (h0 <= h1 <= h2) or h2 - h1 < 2 * fd - 1 - POLL_SLACK_BLOCKS:
            errs.append(f"the release at node height {h2} is not 2 x finality_depth ({fd}) - 1 blocks (less "
                        f"{POLL_SLACK_BLOCKS} for the polls) after the key's removal seen at {h1} (the unstake at {h0})")
    return errs


def check(path):
    with open(path) as fh:
        f = json.load(fh)
    errs = []
    if f.get("lez") != V040:
        errs.append(f"lez {f.get('lez')!r} is not {V040}")
    if "standalone_first_height" in f:
        errs += check_standalone(f)
    elif "channel_keys" in f or "posting_timeframe_before" in f:
        errs += check_vote(f)
    else:
        errs.append("neither a standalone nor a vote run")
    return errs


def main(argv):
    if len(argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    root = argv[1]
    found = sorted(os.path.join(d, "facts.json") for d, _, files in os.walk(root) if "facts.json" in files)
    if not found:
        print(f"check_zone_v040: no facts.json under {root}", file=sys.stderr)
        return 2
    bad = 0
    for p in found:
        errs = check(p)
        for e in errs:
            print(f"{os.path.relpath(p, root)}: {e}")
        if not errs:
            print(f"{os.path.relpath(p, root)}: ok")
        bad += len(errs)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
