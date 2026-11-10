#!/usr/bin/env python3
"""Self-checks of the simulator's building blocks against the upstream behaviour it models.

    python3 test_sim.py
"""

from sim import ROOT, Params, Sim, Tree, round_robin, run, threshold


def test_threshold():
    # committee_discovery.rs:27-35
    assert [threshold(n) for n in range(1, 11)] == [1, 1, 2, 3, 4, 4, 5, 6, 6, 7]


def test_round_robin_worked_example():
    # DECENTRALIZED_SEQUENCING.md:78-90: three keys, timeframe 60, timeout 180, a busy channel.
    chan = (0, 0, 0, 0)          # tip at slot 0 by index 0, its turn started at slot 0
    for slot, want in ((0, 0), (59, 0), (60, 1), (119, 1), (120, 2), (179, 2)):
        # the channel stays busy: the tip moves with every slot, so no timeout fires
        busy = (0, slot, 0, 0)
        assert round_robin(busy, slot, 3, 60, 180)[0] == want, slot
    # silent channel: after 180 slots without a post the timeout grid takes over
    assert round_robin(chan, 180, 3, 60, 180) == (1, 180)
    assert round_robin(chan, 360, 3, 60, 180) == (2, 360)
    # posting_timeframe = posting_timeout = 0: accredited_keys[0] only
    assert round_robin((0, 5, 0, 5), 10_000, 3, 0, 0)[0] == 0


def test_tree():
    t = Tree()
    a = t.add(ROOT)
    b = t.add(a)
    c = t.add(b)
    d = t.add(b)
    e = t.add(d)
    assert t.anc(e, 1) == b and t.is_anc(a, e) and not t.is_anc(c, e)
    assert t.lca(c, e) == b and t.lca(e, e) == e


def test_fold_like_chain_rs():
    s = Sim(Params(n=2), 1)
    g = 0
    b1 = s.new_lez(g, 0, True, 1, 1, frozenset())
    bad = s.new_lez(g, 0, False, 1, 1, frozenset())         # an invalid block
    dup = s.new_lez(g, 1, True, 1, 1, frozenset())          # a second block at height 1
    b2 = s.new_lez(b1, 0, True, 2, 2, frozenset())
    m = [s.new_msg(ROOT, 0, x) for x in (bad, b1, dup, b2)]
    tip = g
    for x in m:
        tip = s.fold(tip, x)
    # chain.rs:721-737 (invalid skipped) and :739-755 (a duplicate height never rewinds the head)
    assert tip == b2
    # an entry without a block moves nothing (chain.rs:706-719)
    assert s.fold(tip, s.new_msg(ROOT, 0, ROOT)) == b2


def test_deterministic():
    p = Params(n=3, m=1, attack="forkstall", horizon=6 * 3600, settle=3600)
    assert run(p, 7) == run(p, 7)


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
            print("ok", name)
