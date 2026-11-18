#!/usr/bin/env python3
"""What forging a light-client header puts at stake on Ethereum mainnet, measured from a beacon node's standard API.

  python3 -I measure.py OUT.json --beacon URL [--state head]

Read-only, User-Agent "ferrata-updater", nothing that identifies the caller:
  GET  /eth/v1/beacon/headers/{state}                      the slot (so the sync-committee period)
  GET  /eth/v1/beacon/states/{state}/sync_committees       the 512 seats of the current committee (and ?epoch= the next)
  GET  /eth/v1/beacon/states/{state}/validators?id=...     each member's effective balance, status, slashed flag
  POST /eth/v1/beacon/rewards/sync_committee/{block}  []   a query (it changes nothing): each member's sync reward in
                                                           that block, from which the total active balance follows
  GET  /lighthouse/validator_inclusion/{epoch}/global      (a Lighthouse node only) the active balance, directly

Seats: 342 of 512 sign a valid light-client update (sum(bits) * 3 >= 512 * 2). The cheapest 342 seats: the members
sorted by effective balance per seat they hold (a validator can hold several seats), taken until 342 seats.
Total active balance B (Gwei) from the per-seat participant reward r (consensus-specs, Altair, process_sync_aggregate):
  r = (((64 * 10**9 // isqrt(B)) * (B // 10**9)) * 2 // 64 // 32) // 512;  every interval of B with that r is reported.
"""
import argparse, json, math, sys, time, urllib.error, urllib.request

UA = "ferrata-updater"
GWEI = 10**9


def call(url, body=None, timeout=60):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method="GET" if body is None else "POST",
                                 headers={"User-Agent": UA, "Accept": "application/json", "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, (e.read()[:300].decode("utf-8", "replace") if e.fp else "")
    except Exception as e:  # noqa: BLE001 - recorded
        return None, repr(e)


def reward(b):
    """The Altair participant reward per seat and slot for a total active balance of b Gwei."""
    base_per_increment = GWEI * 64 // math.isqrt(b)
    total_base = base_per_increment * (b // GWEI)
    return total_base * 2 // 64 // 32 // 512


def balances_for(r):
    """Every interval of total active balances, in whole ETH (B // 10**9), whose participant reward is r. The reward is
    not monotone in B (the base reward per increment is floored), so there can be several."""
    f = lambda inc: GWEI * 64 // math.isqrt(inc * GWEI)  # noqa: E731 - base reward per increment, non-increasing

    def first(pred, lo=1, hi=2**40):
        while lo < hi:
            mid = (lo + hi) // 2
            if pred(mid):
                hi = mid
            else:
                lo = mid + 1
        return lo

    out = []
    for bpi in range(100, 5000):
        # bpi * inc // 524288 == r, with 524288 = 64 * 32 * 512 / 2.
        lo_inc = -(-(r * 524288) // bpi)
        hi_inc = -(-((r + 1) * 524288) // bpi) - 1
        a = first(lambda inc: f(inc) <= bpi)
        b = first(lambda inc: f(inc) < bpi) - 1
        lo, hi = max(lo_inc, a), min(hi_inc, b)
        if lo <= hi:
            out.append([lo, hi])
    return sorted(out)


def committee(base, state, epoch=None):
    q = f"?epoch={epoch}" if epoch is not None else ""
    st, v = call(f"{base}/eth/v1/beacon/states/{state}/sync_committees{q}")
    if st != 200:
        return {"status": st, "error": str(v)[:300]}
    seats = [int(x) for x in v["data"]["validators"]]
    distinct = sorted(set(seats))
    eff = {}
    for i in range(0, len(distinct), 64):
        ids = "&".join(f"id={x}" for x in distinct[i:i + 64])
        st2, w = call(f"{base}/eth/v1/beacon/states/{state}/validators?{ids}")
        if st2 != 200:
            return {"status": st2, "error": str(w)[:300]}
        for d in w["data"]:
            eff[int(d["index"])] = (int(d["validator"]["effective_balance"]), d["status"], d["validator"]["slashed"])
    per_seat = {x: seats.count(x) for x in distinct}
    total_seats_balance = sum(eff[x][0] for x in seats)
    members_balance = sum(eff[x][0] for x in distinct)
    # Cheapest 342 seats: members by effective balance per seat held.
    order = sorted(distinct, key=lambda x: eff[x][0] / per_seat[x])
    taken, stake, members = 0, 0, 0
    for x in order:
        if taken >= 342:
            break
        taken += per_seat[x]
        stake += eff[x][0]
        members += 1
    buckets = {}
    for x in distinct:
        e = eff[x][0] // GWEI
        k = "32" if e <= 32 else "33-1023" if e < 1024 else "1024-2047" if e < 2048 else "2048"
        buckets[k] = buckets.get(k, 0) + 1
    return {
        "seats": len(seats),
        "distinct_members": len(distinct),
        "members_with_several_seats": sum(1 for x in distinct if per_seat[x] > 1),
        "max_seats_of_one_member": max(per_seat.values()),
        "members_by_effective_balance_eth": buckets,
        "effective_balance_of_members_eth": members_balance / GWEI,
        "effective_balance_over_seats_eth": total_seats_balance / GWEI,
        "cheapest_342_seats": {"members": members, "seats": taken, "effective_balance_eth": stake / GWEI},
        "slashed_members": sum(1 for x in distinct if eff[x][2]),
        "members_by_status": {st: sum(1 for x in distinct if eff[x][1] == st) for st in sorted({eff[x][1] for x in distinct})},
        "effective_balance_by_status_eth": {
            st: sum(eff[x][0] for x in distinct if eff[x][1] == st) / GWEI for st in sorted({eff[x][1] for x in distinct})
        },
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--beacon", required=True)
    ap.add_argument("--state", default="head")
    a = ap.parse_args()
    base = a.beacon.rstrip("/")
    out = {"beacon": base, "started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    st, v = call(f"{base}/eth/v1/node/version")
    out["node_version"] = v["data"]["version"] if st == 200 else f"HTTP {st}"
    st, h = call(f"{base}/eth/v1/beacon/headers/{a.state}")
    slot = int(h["data"]["header"]["message"]["slot"])
    block_root = h["data"]["root"]
    period, epoch = slot // 8192, slot // 32
    out.update(slot=slot, block_root=block_root, epoch=epoch, period=period)
    out["current_committee"] = committee(base, a.state)
    out["next_committee"] = committee(base, a.state, epoch=(period + 1) * 256)
    st, rw = call(f"{base}/eth/v1/beacon/rewards/sync_committee/{block_root}", body=[])
    if st == 200:
        rewards = [int(d["reward"]) for d in rw["data"]]
        positive = sorted({x for x in rewards if x > 0})
        r = math.gcd(*positive) if positive else None
        out["sync_rewards"] = {"block_root": block_root, "members_rewarded": len(rewards), "participant_reward_gwei": r,
                               "total_active_balance_eth_intervals": balances_for(r) if r else None}
    else:
        out["sync_rewards"] = {"status": st, "error": str(rw)[:300]}
    st, g = call(f"{base}/lighthouse/validator_inclusion/{epoch - 1}/global")
    out["lighthouse_validator_inclusion"] = (
        {"epoch": epoch - 1, "current_epoch_active_gwei": int(g["data"]["current_epoch_active_gwei"]),
         "current_epoch_active_eth": int(g["data"]["current_epoch_active_gwei"]) / GWEI}
        if st == 200 else {"status": st, "error": str(g)[:200]})
    out["ended"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(a.out, "w") as f:
        json.dump(out, f, indent=1, sort_keys=True)
    print(json.dumps(out, indent=1, sort_keys=True))


if __name__ == "__main__":
    sys.exit(main())
