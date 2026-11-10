#!/usr/bin/env python3
"""When the sequencer published its blocks, against the seconds at which the Bedrock node's epochs
began, as the two logs show it. One line for the run's environment record.

usage: phase.py SLOT0_UNIX_SECOND BLOCK_INTERVAL_SECONDS EPOCH_SECONDS sequencer.log bedrock.log
"""
import calendar
import collections
import re
import sys
import time

STAMP = re.compile(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(\.\d+)?")


def seconds(path, needle):
    """Whole seconds of the lines that hold `needle`. A stamp with a fraction (the node's) is
    rounded to the nearest second: the node begins a slot within some 15 ms of a whole second,
    before it or after it. The sequencer's stamps have no fraction."""
    out = []
    with open(path, errors="replace") as log:
        for line in log:
            if needle in line:
                found = STAMP.search(line)
                if found:
                    whole = calendar.timegm(time.strptime(found.group(1), "%Y-%m-%dT%H:%M:%S"))
                    out.append(whole + (1 if found.group(2) and float(found.group(2)) >= 0.5 else 0))
    return out


def main():
    origin, interval, epoch = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])
    sequencer_log, bedrock_log = sys.argv[4], sys.argv[5]
    published = seconds(sequencer_log, "Publishing block")
    after = collections.Counter((t - origin) % interval for t in published)
    epochs = sorted(set(seconds(bedrock_log, "epoch transition")))
    began = collections.Counter((t - origin) % epoch for t in epochs)
    # The least time from a published block to the next epoch that did begin.
    gaps = [min((e - t for e in epochs if e >= t), default=None) for t in published]
    gaps = [g for g in gaps if g is not None]
    removed = 0
    with open(bedrock_log, errors="replace") as log:
        for line in log:
            if "proposed block" in line and "removed)" in line and "(0 removed)" not in line:
                removed += 1
    print(
        "- as the logs show it: {} blocks published, at seconds after an epoch second (mod {}): {}; "
        "{} epochs began during the run, at seconds after slot 0 (mod {}): {}; the least time from a "
        "published block to the next epoch's first second: {}; blocks the node proposed with a "
        "transaction removed: {}".format(
            len(published),
            interval,
            ", ".join("{} s (x{})".format(k, v) for k, v in sorted(after.items())) or "none",
            len(epochs),
            epoch,
            ", ".join("{} (x{})".format(k, v) for k, v in sorted(began.items())) or "none",
            "{} s".format(min(gaps)) if gaps else "no epoch began after a block",
            removed,
        )
    )


if __name__ == "__main__":
    main()
