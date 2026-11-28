#!/usr/bin/env python3
"""Who inscribed what in a zone run: the channel's inscriptions in the node's blocks, by signer and by turn.

    python3 -I checkpoint/attestor/zone_inscriptions.py ZONE_DIR [--out FILE]

ZONE_DIR is attestor/zone.sh's output: answers/blocks/*.json (the node's /cryptarchia/blocks), answers/channel.json and
zone.log, whose `seq-<i> set up: <channel key>` lines name each sequencer's key. Every inscription of the channel (op
0x11, `channel_id` the zone's) in the blocks is counted for its signer; consecutive inscriptions by one signer form a
turn (its first and last slot). Written to FILE (default ZONE_DIR/inscriptions.json) in the layout of
e2e/runs/zone-rc3-four-keys/inscriptions.json; exit 1 if the blocks hold no inscription of the channel.
"""
import argparse
import json
import os
import re
import sys

OP_CHANNEL_INSCRIBE = 0x11  # checkpoint/crates/checkpoint-core/src/mantle.rs


def sequencer_keys(zone_log):
    """{channel key hex: 'seq-i'} from zone.sh's log lines `seq-<i> set up: <key>`."""
    keys = {}
    with open(zone_log) as f:
        for line in f:
            m = re.search(r"seq-(\d+) set up: ([0-9a-f]{64})\b", line)
            if m:
                keys[m.group(2)] = f"seq-{m.group(1)}"
    return keys


def blocks(answers):
    """The node's blocks of every answer window, in slot order; a block in two overlapping windows counts once (by its
    header id)."""
    d = os.path.join(answers, "blocks")
    seen = {}
    for name in sorted(os.listdir(d)):
        if name.endswith(".json"):
            with open(os.path.join(d, name)) as f:
                for b in json.load(f):
                    seen.setdefault(b["header"].get("id") or json.dumps(b["header"], sort_keys=True), b)
    return sorted(seen.values(), key=lambda b: b["header"]["slot"])


def inscriptions(node_blocks, channel_id):
    """[(slot, signer)] of the channel's inscriptions, in slot order."""
    out = []
    for b in node_blocks:
        slot = b["header"]["slot"]
        for tx in b.get("transactions") or []:
            for op in (tx.get("mantle_tx") or {}).get("ops") or []:
                if op.get("opcode") != OP_CHANNEL_INSCRIBE:
                    continue
                p = op.get("payload") or {}
                if str(p.get("channel_id", "")).lower().removeprefix("0x") != channel_id:
                    continue
                out.append((slot, str(p.get("signer", "")).lower().removeprefix("0x")))
    return out


def summary(zone_dir, channel_id):
    names = sequencer_keys(os.path.join(zone_dir, "zone.log"))
    nb = blocks(os.path.join(zone_dir, "answers"))
    ins = inscriptions(nb, channel_id)
    per, turns = {}, []
    for slot, signer in ins:
        who = names.get(signer, signer[:16])
        per[who] = per.get(who, 0) + 1
        if turns and turns[-1]["sequencer"] == who:
            turns[-1]["inscriptions"] += 1
            turns[-1]["toSlot"] = slot
        else:
            turns.append({"sequencer": who, "inscriptions": 1, "fromSlot": slot, "toSlot": slot})
    return {"_source": "the node's final blocks (cryptarchia/blocks) of zone.sh's run, channel inscriptions with their signer "
                       "(checkpoint/attestor/zone_inscriptions.py)",
            "nodeBlocks": len(nb), "inscriptions": len(ins), "perSequencer": per, "turns": turns,
            "signersNamed": sorted(set(names.values()))}


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("zone_dir")
    ap.add_argument("--out")
    ap.add_argument("--channel", default="01" * 32)
    a = ap.parse_args(argv[1:])
    s = summary(a.zone_dir, a.channel.lower().removeprefix("0x"))
    out = a.out or os.path.join(a.zone_dir, "inscriptions.json")
    with open(out, "w") as f:
        json.dump(s, f, indent=1)
        f.write("\n")
    print(f"{s['inscriptions']} inscriptions in {s['nodeBlocks']} node blocks: {s['perSequencer']}; {len(s['turns'])} turns")
    return 0 if s["inscriptions"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
