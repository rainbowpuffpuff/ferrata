#!/usr/bin/env python3
"""Discrete-event model of the SPEC-v2 §4 checkpoint rule on LEZ v0.4 and Logos Blockchain.

Item numbers (M1.4, M2.3, ...) refer to ../MODEL.md.  One time unit is one Bedrock
slot (1 s).  A run is deterministic for a given (Params, seed).  Standard library only.

What is modelled
  Bedrock    slots, a leader lottery with rate f, propagation delay (natural forks),
             longest-chain fork choice limited to k (M2.5), LIB = k below the tip,
             a public mempool that re-admits reorged transactions (M2.3), and the
             channel rules: parent == tip_message, signer == round_robin(slot) (M3.1).
  LEZ        blocks linked by prev hash, the fold of a channel lineage (M1.1), and
             honest sequencers that build on their head, only in their own turn,
             with their own unmined publishes in the view (M1.2-M1.4), shedding them
             when they leave the branch or expire (M2.2).
  Vault      candidates, beacons and finalisation as in SPEC §4.3, with the support
             count of §4.2 item 4 computed along a single hash-linked path.
  Adversary  up to m keys (equivocate, withhold, sign blocks off chain, re-inscribe),
             private Bedrock blocks, and control of what chosen sequencer nodes see
             for a while ("capture"); scripted attacks are at the end of the file.

Checked on every finalisation: (P1) the tip is on the eventual canonical LEZ chain,
(P2) the tip was at least k Bedrock blocks deep on the public chain when finalised.
"""

from __future__ import annotations

import random
from dataclasses import dataclass, field

INF = 1 << 62
ROOT = -1  # MsgId::root() / no parent


def threshold(n: int) -> int:
    """LEZ channel_config_threshold, used by SPEC §4.2 item 3 (M5)."""
    return max(1, min(-(-2 * n // 3), n - 1))


def round_robin(chan, slot, n, timeframe, timeout):
    """ChannelState::round_robin (M3.1). chan = (tip_msg, tip_slot, tip_seq, tip_start)."""
    _, tip_slot, tip_seq, tip_start = chan
    elapsed = max(0, slot - tip_slot)
    if timeout and elapsed >= timeout:
        q = elapsed // timeout
        return (tip_seq + q) % n, tip_slot + q * timeout
    if timeframe:
        q = max(0, slot - tip_start) // timeframe
        return (tip_seq + q) % n, tip_start + q * timeframe
    return tip_seq, tip_start


class Tree:
    """Parent pointers with binary lifting: ancestor-at-depth and lowest common ancestor."""

    __slots__ = ("par", "dep", "up")

    def __init__(self):
        self.par, self.dep, self.up = [], [], []

    def add(self, parent: int) -> int:
        ups = []
        if parent >= 0:
            ups.append(parent)
            j = 0
            while j < len(self.up[ups[j]]):
                ups.append(self.up[ups[j]][j])
                j += 1
        self.par.append(parent)
        self.dep.append(0 if parent < 0 else self.dep[parent] + 1)
        self.up.append(ups)
        return len(self.par) - 1

    def anc(self, x: int, d: int) -> int:
        diff, j = self.dep[x] - d, 0
        while diff:
            if diff & 1:
                x = self.up[x][j]
            diff >>= 1
            j += 1
        return x

    def is_anc(self, a: int, b: int) -> bool:
        """a == b or a is an ancestor of b."""
        return 0 <= a and self.dep[a] <= self.dep[b] and self.anc(b, self.dep[a]) == a

    def lca(self, a: int, b: int) -> int:
        if self.dep[a] > self.dep[b]:
            a = self.anc(a, self.dep[b])
        elif self.dep[b] > self.dep[a]:
            b = self.anc(b, self.dep[a])
        if a == b:
            return a
        for j in range(len(self.up[a]) - 1, -1, -1):
            if self.up[a][j] != self.up[b][j]:
                a, b = self.up[a][j], self.up[b][j]
        return self.par[a]


@dataclass
class Params:
    n: int = 4                  # accredited keys 0..n-1
    m: int = 0                  # adversary keys: the last m indices
    k: int = 120                # Bedrock security parameter (M6)
    f: float = 1 / 30           # active slot coefficient (M6)
    net_delay: int = 2          # slots before an honest Bedrock block reaches the others
    timeframe: int = 900        # posting_timeframe (M3.1, testnet)
    timeout: int = 150          # posting_timeout (M3.1, testnet)
    block_interval: int = 10    # block_create_timeout (M1.3, testnet)
    stale_refund: int = 50      # LIB slots an own inscription may stay unmined (M2.2)
    tx_ttl: int = 3600          # mempool TTL (M2.3)
    delta: int = 7200           # the vault's Δ
    eth_slot: int = 12
    horizon: int = 24 * 3600    # the honest prover works until here
    settle: int = 4 * 3600      # then the chain runs on so every finalised tip is decided
    rule: str = "v2"            # v2 | noexcl | unverified | single | ts (the earlier rule)
    attack: str = "none"        # see make_attack()
    attack_start: int = 3 * 3600
    outage: bool = True         # forkstall: honest sequencers are down between the forks
    back_before_b: int = 0      # forkstall: they come back this long before the beacon
    honest_tip: bool = False    # forkstall: attack an honest producer's block (works with m = 0)
    eclipse: bool = False       # forkstall: keep the fooled nodes on the private fork throughout
    skew_key: int | None = None  # this honest key's clock runs Δ ahead
    slowdown: tuple | None = None  # (start, end, factor) on the honest leader rate
    accelerate: bool = False    # adversary keys sign every honest candidate at once
    prover_period: int = 30
    # SPEC-v3 D4 (docs/design/spec/bridge-spec-v3-changes.md)
    turn_wait: bool = False     # W = delta + (t - 1) * posting_timeframe instead of a flat delta
    guard: str = "off"          # off | freeze-hold | freeze-lift | cancel | cancel-tip (Monitor)
    guard_dg: int = 7200        # Δ_g: a candidate's tip must be final by submittedAt + Δ_g
    guard_stall: int = 1800     # S/2: the honest final tier must advance at least this often
    hog: bool = False           # forkstall: adversary turns held with empty inscriptions
    suppress: bool = False      # forkstall: honest turns kept off chain for their first timeout
    patience: int = 3600        # forkstall: how long the second fork phase keeps trying


@dataclass
class Seq:
    key: int
    phase: int
    skew: int = 0
    tip: int | None = None       # Bedrock block this node follows; None means the public tip
    captured: bool = False       # the adversary chooses what it sees and gets what it sends
    frozen: bool = False         # partitioned: sees nothing new and reaches nobody
    pending: list = field(default_factory=list)  # own unmined inscriptions, in order (M1.4)
    extra_echo: set = field(default_factory=set)  # beacon nonces fed to this node only


class Watch:
    """Keys counted by SPEC §4.2 item 4 along every hash-linked path below T, kept up to date.

    With nonce None it counts signers of blocks in (T, E]; otherwise of blocks in (Y, E],
    Y being the first block after T that carries the nonce.  `free` are adversary keys,
    which can always append a block they sign off chain (W4)."""

    __slots__ = ("sim", "excl", "nonce", "free", "pre", "sets", "best")

    def __init__(self, sim, T, excl, nonce, free):
        self.sim, self.excl, self.nonce = sim, excl, nonce
        self.free = free - excl
        self.pre, self.sets, self.best = set(), {}, 0
        if nonce is None:
            self.sets[T] = frozenset()
            self.best = len(self.free)
        else:
            self.pre.add(T)
        for b in sim.descendants(T):
            self.add(b)

    def add(self, b):
        sim = self.sim
        p = sim.lt.par[b]
        if p in self.sets:
            s = self.sets[p] | (sim.l_signers[b] - self.excl)
            self.sets[b] = s
            c = len(s | self.free)
        elif p in self.pre:
            if self.nonce in sim.l_nonce[b]:
                self.sets[b] = frozenset()
                c = len(self.free)
            else:
                self.pre.add(b)
                return
        else:
            return
        if c > self.best:
            self.best = c


class Vault:
    """SPEC §4.3: candidates, beacons, finalisation (plus the earlier one-step rule)."""

    def __init__(self, sim):
        self.sim = sim
        self.final = 0               # LEZ genesis
        self.cands = {}              # T -> (submittedAt, finalState tip at submission)
        self.beacons = []            # Ethereum time of each beacon
        self.blocked = False         # the last refusal was only the checkpoint freeze

    def wait(self):
        """Flat Δ (SPEC-v2), or W = Δ + (t - 1) * turn (SPEC-v3 D4)."""
        P = self.sim.P
        return P.delta + ((self.sim.t - 1) * P.timeframe if P.turn_wait else 0)

    def support(self, T, nonce, free):
        sim, rule = self.sim, self.sim.P.rule
        if rule == "v2":
            # The excluded key is the signer of a verified inscription of T; the prover
            # may present any inscription of T's bytes (M1.6).
            excls = [frozenset([e]) for e in sim.l_signers[T]]
        else:
            excls = [frozenset()]
        best = max(sim.watch(T, e, nonce, free).best for e in excls)
        if rule == "unverified":
            # SPEC §4.2 item 4 as written: the excluded key is named, not checked (W1).
            best = min(best, sim.n - 1)
        return best

    def submit_candidate(self, T, s, free=frozenset()):
        sim = self.sim
        self.blocked = False
        if T == self.final or not sim.lt.is_anc(self.final, T):
            return False
        if self.support(T, None, free) < sim.t:
            return False
        if sim.monitor.frozen:
            self.blocked = True
            return False
        self.cands[T] = (s, self.final)
        self.record_beacon(s)        # "Also records a beacon"
        return True

    def record_beacon(self, s):
        e = self.sim.P.eth_slot
        self.beacons.append(-(-s // e) * e)
        return len(self.beacons) - 1

    def finalize(self, T, i, s, free=frozenset(), who="honest"):
        self.blocked = False
        c = self.cands.get(T)
        if c is None or c[1] != self.final:
            return False
        if self.beacons[i] < c[0] + self.wait():
            return False
        if self.support(T, i, free) < self.sim.t:
            return False
        if self.sim.monitor.frozen:
            self.blocked = True
            return False
        self.final = T
        self.sim.on_finalised(T, s, who)
        return True

    def accept_ts(self, T, E, s, who):
        """The earlier rule (r2-crypto §2): t keys after T, then t keys again whose
        timestamps are at least Δ after the first group's; no producer exclusion."""
        sim, t = self.sim, self.sim.t
        if T == E or not (sim.lt.is_anc(self.final, T) and sim.lt.is_anc(T, E)):
            return False
        path, b = [], E
        while b != T:
            path.append(b)
            b = sim.lt.par[b]
        path.reverse()
        L = len(path)
        # pre[i]: the latest of the t earliest per-key timestamps in path[:i] (best first group);
        # suf[i]: the earliest of the t latest per-key timestamps in path[i:] (best second group).
        early, pre = {}, [INF] * (L + 1)
        for i, b in enumerate(path):
            for key in sim.l_signers[b]:
                early[key] = min(early.get(key, INF), sim.l_ts[b])
            if len(early) >= t:
                pre[i + 1] = sorted(early.values())[t - 1]
        late, suf = {}, [-INF] * (L + 1)
        for i in range(L - 1, -1, -1):
            for key in sim.l_signers[path[i]]:
                late[key] = max(late.get(key, -INF), sim.l_ts[path[i]])
            if len(late) >= t:
                suf[i] = sorted(late.values(), reverse=True)[t - 1]
        for split in range(1, L):
            if suf[split] - pre[split] >= sim.P.delta:
                self.final = T
                sim.on_finalised(T, s, who)
                return True
        return False


class Monitor:
    """SPEC-v3 D4 checkpoint freeze, driven by an honest node's final tier (the public chain's).

    Both readings of D4's tip trigger: a usable candidate whose tip is not final by
    submittedAt + Δ_g.  The stall trigger (final tier not advanced for guard_stall) freezes
    while the stall lasts, in every mode except 'cancel-tip'.
    'freeze-hold' the tip trigger's freeze is held until it expires (maxFreeze, 30 days):
                  for the rest of the run.  D4 read literally.
    'freeze-lift' the authority lifts it once the tip is overtaken (another block final at
                  its height).
    'cancel'      the authority rejects the candidate instead (a reject-only call that
                  SPEC-v3 does not have); no freeze for it.
    'cancel-tip'  'cancel' without the stall trigger."""

    def __init__(self, sim):
        self.sim, self.frozen, self.latched = sim, False, False
        self.height = self.advanced = 0
        self.trips = self.stall_trips = self.tip_trips = self.cancelled = self.frozen_time = 0

    def step(self, s):
        sim, P = self.sim, self.sim.P
        if P.guard == "off":
            return
        final = sim.b_lez[sim.lib(sim.pub_tip)]
        if sim.lt.dep[final] > self.height or self.height == 0:
            self.height, self.advanced = sim.lt.dep[final], s   # (no stall before the first final block)
        stall = P.guard != "cancel-tip" and s - self.advanced >= P.guard_stall
        bad = False
        V = sim.vault
        for T in list(V.cands):
            a, parent = V.cands[T]
            if parent != V.final or s < a + P.guard_dg or sim.lt.is_anc(T, final):
                continue
            if P.guard.startswith("cancel"):
                del V.cands[T]
                self.cancelled += 1
            elif P.guard == "freeze-hold":
                bad = self.latched = True
            elif sim.lt.dep[final] < sim.lt.dep[T]:
                bad = True       # 'freeze-lift': not final, not yet overtaken
        freeze = stall or bad or self.latched
        if freeze and not self.frozen:
            self.trips += 1
            self.stall_trips += stall
            self.tip_trips += bad
        if freeze:
            self.frozen_time += P.prover_period
        self.frozen = freeze


class HonestProver:
    """Picks the public LEZ head, waits for t supporters, submits, waits Δ, records a
    beacon, has the nonce echoed on LEZ, waits for t supporters after it, finalises."""

    def __init__(self, sim):
        self.sim, self.state, self.T = sim, "idle", None
        self.delay = 0               # time this target spent refused only by the freeze
        self.lost = 0                # candidates the authority cancelled under us

    def reset(self):
        if self.T is not None:
            self.sim.drop_watches(self.T)
        self.state, self.T, self.delay = "idle", None, 0

    def step(self, s):
        sim, V, P = self.sim, self.sim.vault, self.sim.P
        if sim.attack.prover_paused(s):
            return
        free = sim.bad if P.accelerate else frozenset()
        if P.rule == "ts":
            return self.step_ts(s)
        if self.state != "idle" and (V.final != self.parent or not sim.lt.is_anc(self.T, sim.b_lez[sim.pub_tip])):
            self.reset()   # orphaned, or someone else finalised first
        if self.state in ("cand", "final") and s - self.since > 6 * 3600:
            self.reset()
        if self.state in ("wait", "final") and self.T not in V.cands:
            self.lost += 1
            self.reset()             # cancelled by the freeze authority
        if self.state == "idle":
            T = sim.b_lez[sim.pub_tip]
            if T != V.final and sim.lt.is_anc(V.final, T):
                self.T, self.parent, self.since, self.state = T, V.final, s, "cand"
        elif self.state == "cand":
            if V.submit_candidate(self.T, s, free):
                self.a, self.state = s, "wait"
            elif V.blocked:
                self.delay += P.prover_period
        elif self.state == "wait":
            if -(-s // P.eth_slot) * P.eth_slot >= self.a + V.wait():
                self.i = V.record_beacon(s)
                sim.echo.add(self.i)       # a relayer submits the echo transaction on LEZ
                self.state, self.since = "final", s
        elif self.state == "final":
            if V.finalize(self.T, self.i, s, free):
                sim.finals[-1]["frozen_delay"] = self.delay
                self.reset()
            elif V.blocked:
                self.delay += P.prover_period


    def step_ts(self, s):
        """The earlier rule finalises in one step once the public head shows both groups."""
        sim, V = self.sim, self.sim.vault
        if self.state == "idle" or not sim.lt.is_anc(self.T, sim.b_lez[sim.pub_tip]) or V.final != self.parent:
            T = sim.b_lez[sim.pub_tip]
            if T != V.final and sim.lt.is_anc(V.final, T):
                self.T, self.parent, self.state = T, V.final, "cand"
            return
        if s % 300 == 0 and V.accept_ts(self.T, sim.b_lez[sim.pub_tip], s, "honest"):
            self.state, self.T = "idle", None


class Sim:
    def __init__(self, P: Params, seed: int):
        self.P, self.seed, self.rng = P, seed, random.Random(seed)
        self.n = P.n
        self.t = 1 if P.rule == "single" else threshold(P.n)
        self.bad = frozenset(range(P.n - P.m, P.n))
        # Bedrock blocks
        self.bt = Tree()
        self.b_slot, self.b_chan, self.b_lez, self.b_txs = [], [], [], []
        # LEZ blocks
        self.lt = Tree()
        self.l_prod, self.l_valid, self.l_vchain, self.l_created, self.l_ts = [], [], [], [], []
        self.l_nonce, self.l_nonces, self.l_signers, self.l_kids, self.l_honest = [], [], [], [], []
        # inscriptions (the MsgId tree)
        self.mt = Tree()
        self.m_signer, self.m_lez, self.m_libslot, self.m_admitted = [], [], [], []
        self.m_posted = []           # last time anyone (re-)posted it; the TTL runs from here
        self.mempool = {}            # parent MsgId -> [MsgId] admitted to the public mempool
        self.pending_set = set()
        self.deliveries = {}
        self.watches = {}
        self.echo = set()            # beacon nonces relayed to LEZ for honest blocks
        self.finals = []
        g = self.new_lez(ROOT, 0, True, 0, 0, frozenset())
        x = self.new_msg(ROOT, 0, g)
        self.genesis = self.add_bblock(ROOT, 0, [x], (x, 0, 0, 0), g)
        self.pub_tip = self.genesis
        self.seqs = [Seq(key=i, phase=self.rng.randrange(P.block_interval)) for i in range(P.n)]
        if P.skew_key is not None:
            self.seqs[P.skew_key].skew = P.delta
        self.by_phase = {}
        for q in self.seqs:
            self.by_phase.setdefault(q.phase, []).append(q)
        self.vault = Vault(self)
        self.monitor = Monitor(self)
        self.prover = HonestProver(self)
        self.attack = make_attack(self)

    # ---------------------------------------------------------------- objects
    def new_lez(self, prev, prod, valid, s, ts, nonces, honest=True):
        b = self.lt.add(prev)
        self.l_prod.append(prod)
        self.l_valid.append(valid)
        self.l_vchain.append(valid and (prev < 0 or self.l_vchain[prev]))
        self.l_created.append(s)
        self.l_ts.append(ts)
        self.l_nonce.append(nonces)
        base = self.l_nonces[prev] if prev >= 0 else frozenset()
        self.l_nonces.append(base | nonces if nonces else base)
        self.l_signers.append(frozenset([prod]))
        self.l_kids.append([])
        self.l_honest.append(honest)
        if prev >= 0:
            self.l_kids[prev].append(b)
        for w in self.watches.values():
            w.add(b)
        return b

    def new_msg(self, parent, signer, lez, libslot=0):
        x = self.mt.add(parent)
        self.m_signer.append(signer)
        self.m_lez.append(lez)
        self.m_libslot.append(libslot)
        self.m_admitted.append(INF)
        self.m_posted.append(-INF)
        if lez >= 0 and signer not in self.l_signers[lez]:
            # Another key inscribes the same bytes (M1.6): every count below T changes.
            self.l_signers[lez] = self.l_signers[lez] | {signer}
            self.watches = {key: Watch(self, *key) for key in self.watches}
        return x

    def add_bblock(self, parent, s, txs, chan, lez):
        b = self.bt.add(parent)
        self.b_slot.append(s)
        self.b_chan.append(chan)
        self.b_lez.append(lez)
        self.b_txs.append(txs)
        return b

    def fold(self, lez, x):
        """One fold step (M1.1): the entry's block applies iff valid and it extends the tip."""
        b = self.m_lez[x]
        if b >= 0 and self.l_valid[b] and self.lt.par[b] == lez:
            return b
        return lez   # no block, an invalid block, or one that does not extend the tip

    def admit(self, x, at):
        """(Re-)post x to the public mempool; it stays there tx_ttl after the last post (M2.3)."""
        if self.m_admitted[x] == INF:
            self.m_admitted[x] = at
            self.mempool.setdefault(self.mt.par[x], []).append(x)
        self.m_posted[x] = max(self.m_posted[x], at)

    def available(self, x, s):
        return self.m_admitted[x] <= s and s - self.m_posted[x] <= self.P.tx_ttl

    def competitor(self, parent, s, exclude):
        """An inscription in the public mempool that could still take the slot after `parent`."""
        return any(self.available(x, s) or self.m_admitted[x] > s
                   for x in self.mempool.get(parent, ()) if self.m_signer[x] not in exclude
                   and self.m_admitted[x] != INF)

    def rr(self, b, s):
        P = self.P
        return round_robin(self.b_chan[b], s, self.n, P.timeframe, P.timeout)

    def lib(self, b):
        return self.bt.anc(b, max(0, self.bt.dep[b] - self.P.k))

    def descendants(self, T):
        out, stack = [], list(reversed(self.l_kids[T]))
        while stack:
            b = stack.pop()
            out.append(b)
            stack.extend(reversed(self.l_kids[b]))
        out.sort()   # creation order: parents before children
        return out

    def watch(self, T, excl, nonce, free):
        key = (T, excl, nonce, free)
        w = self.watches.get(key)
        if w is None:
            w = self.watches[key] = Watch(self, *key)
        return w

    def drop_watches(self, T):
        self.watches = {k: w for k, w in self.watches.items() if k[0] != T}

    # ---------------------------------------------------------------- Bedrock
    def leader_rate(self, s):
        sd = self.P.slowdown
        if sd and sd[0] <= s < sd[1]:
            return self.P.f * sd[2]
        return self.P.f

    def honest_bblock(self, s):
        """An honest leader extends the public tip with every valid channel transaction
        in the mempool (M3.1): parent == tip_message, signer scheduled at this slot."""
        parent = self.pub_tip
        chan, lez, txs = self.b_chan[parent], self.b_lez[parent], []
        while True:
            idx, start = round_robin(chan, s, self.n, self.P.timeframe, self.P.timeout)
            pick = None
            for x in self.mempool.get(chan[0], ()):
                if self.m_signer[x] == idx and self.available(x, s):
                    pick = x
                    break
            if pick is None:
                break
            txs.append(pick)
            chan = (pick, s, idx, start)
            lez = self.fold(lez, pick)
        b = self.add_bblock(parent, s, txs, chan, lez)
        self.deliveries.setdefault(s + self.P.net_delay, []).append(b)

    def adv_block(self, parent, s, msgs):
        """A private block: same validity rules, never delivered unless released."""
        chan, lez = self.b_chan[parent], self.b_lez[parent]
        for x in msgs:
            idx, start = round_robin(chan, s, self.n, self.P.timeframe, self.P.timeout)
            assert self.mt.par[x] == chan[0] and self.m_signer[x] == idx, "invalid private inscription"
            chan = (x, s, idx, start)
            lez = self.fold(lez, x)
        return self.add_bblock(parent, s, list(msgs), chan, lez)

    def consider(self, b):
        """Longest chain, never switching to a fork deeper than k (M2.5)."""
        cur = self.pub_tip
        if self.bt.dep[b] > self.bt.dep[cur]:
            if self.bt.dep[cur] - self.bt.dep[self.bt.lca(cur, b)] <= self.P.k:
                self.pub_tip = b

    # ---------------------------------------------------------------- sequencers
    def node_tip(self, q):
        return self.pub_tip if q.tip is None else q.tip

    def head(self, q, tip):
        """The node's head tip and pin (M1.1, M1.2), after the SDK's shedding of own
        pending inscriptions (M2.2)."""
        chan = self.b_chan[tip]
        mt = self.mt
        # own entries now mined on this branch leave the pending set
        if q.pending and mt.is_anc(q.pending[0], chan[0]):
            keep = [x for x in q.pending if not mt.is_anc(x, chan[0])]
            for x in q.pending[: len(q.pending) - len(keep)]:
                self.pending_set.discard(x)
            q.pending = keep
        libslot = self.b_slot[self.lib(tip)]
        parent, kept = chan[0], 0
        for x in q.pending:
            if mt.par[x] != parent or libslot - self.m_libslot[x] > self.P.stale_refund:
                break
            parent, kept = x, kept + 1
        if kept < len(q.pending):
            for x in q.pending[kept:]:
                self.pending_set.discard(x)
            del q.pending[kept:]
        lez = self.b_lez[tip]
        for x in q.pending:
            lez = self.fold(lez, x)
        return lez, (q.pending[-1] if q.pending else chan[0]), chan

    def tick(self, q, s):
        """One block_create_timeout tick of a sequencer (M1.3)."""
        if self.attack.offline(q.key, s):
            return
        if q.key in self.bad and self.attack.silent(q.key, s):
            return
        tip = self.node_tip(q)
        lez, pin, chan = self.head(q, tip)
        reaches = not (q.captured or q.frozen or self.attack.mute(q, s))
        if reaches:
            for x in q.pending:          # the SDK keeps re-posting what is pending
                self.admit(x, s)
        idx, _ = round_robin(chan, s, self.n, self.P.timeframe, self.P.timeout)
        if idx != q.key:
            return
        nonces = frozenset(i for i in self.echo | q.extra_echo if i not in self.l_nonces[lez])
        b = self.new_lez(lez, q.key, True, s, s + q.skew, nonces, honest=q.key not in self.bad)
        x = self.new_msg(pin, q.key, b, self.b_slot[self.lib(tip)])
        q.pending.append(x)
        self.pending_set.add(x)
        if reaches:
            self.admit(x, s + 1)

    def capture(self, q, b):
        q.tip, q.captured = b, True

    def release(self, q):
        q.tip, q.captured = None, False

    # ---------------------------------------------------------------- properties
    def fold_depth(self, T):
        """Bedrock blocks above the first public block whose fold holds T (-1: none)."""
        tip, bt = self.pub_tip, self.bt
        if not self.lt.is_anc(T, self.b_lez[tip]):
            return -1
        lo, hi = 0, bt.dep[tip]
        while lo < hi:
            mid = (lo + hi) // 2
            if self.lt.is_anc(T, self.b_lez[bt.anc(tip, mid)]):
                hi = mid
            else:
                lo = mid + 1
        return bt.dep[tip] - lo

    def on_finalised(self, T, s, who):
        depth = self.fold_depth(T)
        self.finals.append(dict(T=T, time=s, who=who, depth=depth, kdeep=depth >= self.P.k,
                                valid=self.l_vchain[T], honest_tip=self.l_honest[T],
                                created=self.l_created[T]))

    # ---------------------------------------------------------------- run
    def run(self):
        P = self.P
        end = P.horizon + P.settle
        rng = self.rng
        for s in range(1, end):
            for b in self.deliveries.pop(s, ()):
                self.consider(b)
            if rng.random() < self.leader_rate(s):
                self.honest_bblock(s)
            for q in self.by_phase.get(s % P.block_interval, ()):
                self.tick(q, s)
            self.attack.step(s)
            if s % P.prover_period == 0:
                self.monitor.step(s)
                if s < P.horizon:
                    self.prover.step(s)
        canon = self.b_lez[self.lib(self.pub_tip)]
        head = self.b_lez[self.pub_tip]
        for r in self.finals:
            T = r["T"]
            r["canonical"] = self.lt.is_anc(T, canon)
            r["undecided"] = not r["canonical"] and self.lt.is_anc(T, head)
            r["latency"] = r["time"] - r["created"]
        M = self.monitor
        return dict(seed=self.seed, finals=self.finals, attack=self.attack.report(),
                    lez_height=self.lt.dep[canon], bedrock_height=self.bt.dep[self.pub_tip],
                    monitor=dict(trips=M.trips, stall_trips=M.stall_trips, tip_trips=M.tip_trips,
                                 cancelled=M.cancelled, frozen_time=M.frozen_time,
                                 lost=self.prover.lost))


# ==================================================================== attacks
def hog_step(attack, s):
    """Hold the adversary's turns on the public chain with empty inscriptions (M3.1): one
    landing at least every posting_timeout keeps the turn for posting_timeframe
    (inscribe.rs:249-262 resets tip_slot; channel.rs:221-258 then stays on the timeframe
    branch).  Returns whether an adversary key is scheduled now."""
    sim = attack.sim
    idx, _ = sim.rr(sim.pub_tip, s)
    if idx not in sim.bad:
        return False
    tip = sim.b_chan[sim.pub_tip][0]
    x = attack.hog_x
    if x is not None and sim.mt.par[x] == tip and sim.m_signer[x] == idx:
        return True                  # ours waits for the next Bedrock block
    attack.hog_x = sim.new_msg(tip, idx, ROOT)
    sim.admit(attack.hog_x, s)
    attack.hog_posts += 1
    return True


class NoAttack:
    def __init__(self, sim):
        self.sim = sim
        self.outcome, self.reason = None, ""

    def step(self, s):
        pass

    def silent(self, key, s):
        return False

    def offline(self, key, s):
        return False

    def mute(self, q, s):
        """Whether the network adversary keeps q's inscriptions off the public chain now."""
        return False

    def prover_paused(self, s):
        return False

    def report(self):
        return dict(outcome=self.outcome, reason=self.reason)


class Hog(NoAttack):
    """Adversary keys hold every one of their turns with empty inscriptions, all run long,
    and produce no blocks; with P.accelerate they still sign honest candidates at once.
    A liveness pressure on the stall trigger, not a safety attack."""

    def __init__(self, sim):
        super().__init__(sim)
        self.hog_x, self.hog_posts = None, 0

    def silent(self, key, s):
        return True

    def step(self, s):
        hog_step(self, s)


class Withhold(NoAttack):
    """The adversary's keys never produce and never sign (LEMMA.md §6)."""

    def silent(self, key, s):
        return True


class Scripted(NoAttack):
    """Shared machinery: a state machine that pauses the honest prover while it runs."""

    def __init__(self, sim):
        super().__init__(sim)
        self.state = "wait"
        self.fooled, self.queue, self.priv = [], [], None
        self.private_blocks, self.capture_time, self.captured_since = 0, 0, None
        self.hog_x, self.hog_posts, self.hogging = None, 0, False
        self.muted_time = 0

    @property
    def running(self):
        return self.state not in ("wait", "done", "failed")

    def prover_paused(self, s):
        return self.running

    def silent(self, key, s):
        return s >= self.sim.P.attack_start and self.state not in ("done", "failed")

    def fail(self, s, reason):
        self.end_capture(s)
        self.state, self.outcome, self.reason = "failed", "failed", reason

    def succeed(self, s, reason=""):
        self.end_capture(s)
        self.state, self.outcome, self.reason = "done", "finalised", reason

    def capture_all(self, b, s):
        for key in self.fooled:
            self.sim.capture(self.sim.seqs[key], b)
        if self.captured_since is None:
            self.captured_since = s

    def end_capture(self, s):
        for key in self.fooled:
            self.sim.release(self.sim.seqs[key])
        if self.captured_since is not None:
            self.capture_time = max(self.capture_time, s - self.captured_since)
            self.captured_since = None

    def mine_next(self, s, anchor):
        """Act as the leader of the private fork: put the next fooled node's first block
        below `anchor` on it, so the following node builds on top (one shared path)."""
        sim = self.sim
        if not self.queue:
            return
        q = sim.seqs[self.queue[0]]
        chan = sim.b_chan[self.priv]
        if sim.rr(self.priv, s)[0] != q.key:
            return
        for x in q.pending:
            if sim.mt.par[x] == chan[0] and sim.lt.is_anc(anchor, sim.m_lez[x]):
                self.priv = sim.adv_block(self.priv, s, [x])
                self.private_blocks += 1
                self.capture_all(self.priv, s)
                self.queue.pop(0)
                return

    def report(self):
        return dict(outcome=self.outcome, reason=self.reason, private_blocks=self.private_blocks,
                    capture_time=self.capture_time, hog_posts=self.hog_posts,
                    muted_time=self.muted_time)


class ForkStall(Scripted):
    """LEMMA.md §3: a tip seen on two short forks, around a stretch of time in which no
    honest LEZ block reaches the canonical chain.  With P.outage False this is the same
    attack against live honest sequencers (experiment a, d)."""

    def __init__(self, sim):
        super().__init__(sim)
        P = sim.P
        self.outage, self.back, self.honest_tip = P.outage and not P.eclipse, P.back_before_b, P.honest_tip
        self.eclipse = P.eclipse
        self.b_time = INF

    def offline(self, key, s):
        if not (self.outage and self.running) or key in self.sim.bad:
            return False
        if self.sim.seqs[key].captured:
            return False
        return not (self.back and s >= self.b_time - self.back)

    def counted_free(self, producer):
        sim = self.sim
        if sim.P.rule == "v2":
            return len(sim.bad - {producer})
        return min(len(sim.bad), sim.n - 1)

    def honest_order(self, after, exclude):
        sim = self.sim
        keys = [(after + i) % sim.n for i in range(1, sim.n + 1)]
        return [k for k in keys if k not in sim.bad and k not in exclude]

    def mute(self, q, s):
        """With P.suppress, a network adversary keeps each honest sequencer's inscriptions off
        the public chain while it is scheduled there, so its turn passes after posting_timeout."""
        sim = self.sim
        if not (sim.P.suppress and self.state in ("fool1", "stall", "fork2")) or q.key in sim.bad:
            return False
        if sim.rr(sim.pub_tip, s)[0] != q.key:
            return False
        self.muted_time += sim.P.block_interval
        return True

    def step(self, s):
        sim, P, V = self.sim, self.sim.P, self.sim.vault
        if self.running:
            if self.hogging and not hog_step(self, s):
                self.hogging = P.suppress    # under suppression we hog every adversary turn
        if self.state == "wait":
            if s < P.attack_start or sim.prover.state == "final":
                return
            idx = sim.rr(sim.pub_tip, s)[0]
            head = sim.b_lez[sim.pub_tip]
            if not sim.lt.is_anc(V.final, head):
                return
            self.free = sim.bad
            if self.honest_tip:
                if idx in sim.bad or sim.seqs[idx].pending:
                    return
                if sim.competitor(sim.b_chan[sim.pub_tip][0], s, {idx}):
                    return
                # Capture the producer as well: its next block goes only to us.
                self.M = idx
                need = sim.t - self.counted_free(idx)
                self.fooled = [idx] + self.honest_order(idx, {idx})[:need]
                self.priv = sim.pub_tip
                self.capture_all(self.priv, s)
                self.TA = None
                self.state = "produce"
                return
            if idx not in sim.bad:
                return
            if P.hog and (idx != min(sim.bad) or s - sim.rr(sim.pub_tip, s)[1] > 60):
                return      # start with the first of our consecutive turns
            p0 = sim.b_chan[sim.pub_tip][0]
            if sim.competitor(p0, s, set(range(sim.n)) - sim.bad):
                return      # wait until our own earlier blocks have landed
            self.M = idx
            need = sim.t - self.counted_free(idx)
            # An empty inscription (no block: "a missed turn", not an offence) takes the slot
            # after p0 on the public chain, so no honest unmined block can land there later.
            junk = sim.new_msg(p0, idx, ROOT)
            sim.admit(junk, s)
            self.hog_x, self.hogging = junk, P.hog
            # T_A: a valid block only the adversary has, inscribed on a private block.
            self.TA = sim.new_lez(head, idx, True, s, s, frozenset(), honest=False)
            self.eA = sim.new_msg(p0, idx, self.TA)
            self.priv = sim.adv_block(sim.pub_tip, s, [self.eA])
            self.private_blocks += 1
            self.fooled = self.honest_order(idx, set())[:need]
            if len(self.fooled) < need:
                return self.fail(s, "not enough honest keys")
            self.queue = list(self.fooled)
            self.capture_all(self.priv, s)
            self.state, self.t0 = "fool1", s
        elif self.state == "produce":
            q = sim.seqs[self.M]
            if q.pending:
                x = q.pending[0]
                self.TA, self.eA = sim.m_lez[x], x
                if sim.rr(self.priv, s)[0] != self.M:
                    return self.fail(s, "producer lost its turn")
                self.priv = sim.adv_block(self.priv, s, [x])
                self.private_blocks += 1
                self.fooled = self.fooled[1:]
                self.queue = list(self.fooled)
                self.capture_all(self.priv, s)
                sim.release(q)          # the producer goes down with the others (outage)
                self.state, self.t0 = "fool1", s
        elif self.state == "fool1":
            self.mine_next(s, self.TA)
            if V.support(self.TA, None, self.free) >= sim.t:
                if not V.submit_candidate(self.TA, s, self.free):
                    if V.blocked:
                        return          # frozen: try again on the next slot
                    return self.fail(s, "candidate refused")
                self.a = s
                self.b_time = -(-(s + V.wait()) // P.eth_slot) * P.eth_slot
                if not self.eclipse:
                    self.end_capture(s)
                self.state = "stall"
            elif s - self.t0 > 3600:
                self.fail(s, "fooled nodes did not build in time")
        elif self.state == "stall":
            if s >= self.b_time:
                self.i = V.record_beacon(s)
                self.state, self.t0 = "fork2", s
        elif self.state == "fork2":
            if s - self.t0 > P.patience:
                return self.fail(s, "no fork root: the canonical chain moved past T_A's parent")
            if self.eclipse:
                # The fooled nodes never left the private fork: extend it with Y.
                R = self.priv
                K = sim.rr(R, s)[0]
                if K not in sim.bad:
                    return
                Y = sim.new_lez(sim.b_lez[R], K, True, s, s, frozenset([self.i]), honest=False)
                self.priv = sim.adv_block(R, s, [sim.new_msg(sim.b_chan[R][0], K, Y)])
                self.private_blocks += 1
                self.anchor = Y
                self.queue = list(self.fooled)
                self.capture_all(self.priv, s)
                self.state, self.t0 = "fool2", s
                return
            R = self.find_root()
            if R is None:
                return
            if self.honest_tip:
                if sim.mt.par[self.eA] != sim.b_chan[R][0] or sim.rr(R, s)[0] != self.M:
                    return
                self.priv = sim.adv_block(R, s, [self.eA])
                for key in self.fooled:
                    sim.seqs[key].extra_echo.add(self.i)   # the echo transaction, sent to them
                self.anchor = self.TA
            else:
                K = sim.rr(R, s)[0]
                if K not in sim.bad:
                    return
                copy = sim.new_msg(sim.b_chan[R][0], K, self.TA)       # re-inscription (M1.6)
                Y = sim.new_lez(self.TA, K, True, s, s, frozenset([self.i]), honest=False)
                y = sim.new_msg(copy, K, Y)
                self.priv = sim.adv_block(R, s, [copy, y])
                self.anchor = Y
            self.private_blocks += 1
            self.queue = list(self.fooled)
            self.capture_all(self.priv, s)
            self.state, self.t0 = "fool2", s
        elif self.state == "fool2":
            self.mine_next(s, self.anchor)
            if V.finalize(self.TA, self.i, s, self.free, who="adversary"):
                return self.succeed(s)
            if V.blocked:
                self.frozen_out = True
            if self.TA not in V.cands:
                return self.fail(s, "candidate cancelled by the freeze authority")
            if s - self.t0 > P.patience:
                self.fail(s, "refused: checkpoint freeze" if getattr(self, "frozen_out", False)
                          else "fooled nodes did not build in time")

    def find_root(self):
        """The shallowest public block, at most k-1 below the tip, whose fold ends at
        T_A's parent: a fork from it re-inserts T_A (M2.3)."""
        sim = self.sim
        want, tip = sim.lt.par[self.TA], sim.pub_tip
        for d in range(sim.P.k):
            if sim.bt.dep[tip] < d:
                return None
            R = sim.bt.anc(tip, sim.bt.dep[tip] - d)
            if sim.b_lez[R] == want:
                return R
            if sim.lt.dep[sim.b_lez[R]] < sim.lt.dep[want]:
                return None
        return None


class SkewCapture(Scripted):
    """An attack proposed against the earlier timestamp rule: one honest
    clock Δ ahead and a fork that lives for two posting turns.  Rule 'ts' only."""

    def step(self, s):
        sim, P, V = self.sim, self.sim.P, self.sim.vault
        if self.state == "wait":
            if s < P.attack_start or sim.prover.state == "final":
                return
            idx = sim.rr(sim.pub_tip, s)[0]
            head = sim.b_lez[sim.pub_tip]
            if idx not in sim.bad or not sim.lt.is_anc(V.final, head):
                return
            self.M = idx
            self.TA = sim.new_lez(head, idx, True, s, s, frozenset(), honest=False)
            g1 = sim.new_lez(self.TA, idx, True, s, 0, frozenset(), honest=False)  # dated early
            eA = sim.new_msg(sim.b_chan[sim.pub_tip][0], idx, self.TA)
            e1 = sim.new_msg(eA, idx, g1)
            for K in sim.bad - {idx}:
                sim.new_msg(ROOT, K, g1)       # off-chain copies: every adversary key signs g1
            self.priv = sim.adv_block(sim.pub_tip, s, [eA, e1])
            self.private_blocks += 1
            order = [(idx + i) % sim.n for i in range(1, sim.n)]
            honest = [k for k in order if k not in sim.bad]
            # G1 = the adversary keys (on g1) + the first honest node; G2 = the skewed node + them.
            self.fooled = [honest[0], P.skew_key] if honest[0] != P.skew_key else honest[:2]
            self.queue = list(self.fooled)
            self.capture_all(self.priv, s)
            self.anchor, self.state, self.t0, self.tried = g1, "fool", s, set()
        elif self.state == "fool":
            self.mine_next(s, self.anchor)
            for b in reversed(range(len(sim.l_prod))):
                if sim.l_created[b] < self.t0:
                    break
                if (sim.l_prod[b] == P.skew_key and b not in self.tried
                        and sim.lt.is_anc(self.anchor, b)):
                    self.tried.add(b)
                    late = sim.new_lez(b, self.M, True, s, INF // 2, frozenset(), honest=False)
                    for K in sim.bad - {self.M}:
                        sim.new_msg(ROOT, K, late)
                    if V.accept_ts(self.TA, late, s, "adversary"):
                        return self.succeed(s)
            if s - self.t0 > 3600:
                self.fail(s, "fooled nodes did not build in time")


class Partition(Scripted):
    """An honest producer's node is cut off for longer than Δ (W9): it keeps building on
    its own unmined blocks; the adversary's m keys sign on top (experiment c)."""

    def step(self, s):
        sim, P, V = self.sim, self.sim.P, self.sim.vault
        if self.state == "wait":
            if s < P.attack_start or sim.prover.state == "final":
                return
            q = sim.seqs[0]
            q.frozen, q.tip = True, sim.pub_tip
            self.q, self.free, self.t0, self.TA = q, sim.bad, s, None
            self.state = "cand"
        elif self.state == "cand":
            if self.TA is None:
                for x in self.q.pending:
                    if sim.l_created[sim.m_lez[x]] > self.t0:   # built after the cut
                        self.TA = sim.m_lez[x]
                        break
                return
            if V.submit_candidate(self.TA, s, self.free):
                self.a = s
                self.b_time = -(-(s + P.delta) // P.eth_slot) * P.eth_slot
                self.state = "stall"
            elif s - self.t0 > 4 * 3600:
                self.fail(s, "candidate never reached t")
        elif self.state == "stall":
            if s >= self.b_time:
                self.i = V.record_beacon(s)
                self.q.extra_echo.add(self.i)
                self.state, self.t0 = "final", s
        elif self.state == "final":
            if V.finalize(self.TA, self.i, s, self.free, who="adversary"):
                return self.succeed(s)
            if s - self.t0 > 4 * 3600:
                self.fail(s, "finalisation never reached t")

    def silent(self, key, s):
        return False

    def end_capture(self, s):
        self.q.frozen, self.q.tip = False, None


class Fabricate(Scripted):
    """The adversary signs everything itself, off chain (experiments b and f).
    'invalid': an invalid block it made; 'orphan': an honest block that never became
    canonical (the last blocks of a turn that miss their window are such blocks)."""

    def __init__(self, sim, kind):
        super().__init__(sim)
        self.kind = kind

    def silent(self, key, s):
        return False   # the adversary's keys sequence normally meanwhile

    def step(self, s):
        sim, P, V = self.sim, self.sim.P, self.sim.vault
        if self.state == "wait":
            if s < P.attack_start or s % 60 or sim.prover.state == "final":
                return
            head = sim.b_lez[sim.pub_tip]
            if self.kind == "invalid":
                M0 = min(sim.bad)
                self.TA = sim.new_lez(head, M0, False, s, s, frozenset(), honest=False)
                sim.new_msg(ROOT, M0, self.TA)   # its inscription exists, signed by M0
            else:
                final = sim.b_lez[sim.lib(sim.pub_tip)]
                self.TA = None
                for b in range(len(sim.l_prod) - 1, 0, -1):
                    if sim.l_created[b] < s - 3 * 3600:
                        break
                    if (sim.l_honest[b] and sim.lt.dep[b] <= sim.lt.dep[final]
                            and not sim.lt.is_anc(b, final) and sim.lt.is_anc(V.final, b)):
                        self.TA = b
                        break
                if self.TA is None:
                    return
            if not sim.lt.is_anc(V.final, self.TA):
                return
            if V.submit_candidate(self.TA, s, sim.bad):
                self.a = s
                self.b_time = -(-(s + P.delta) // P.eth_slot) * P.eth_slot
                self.state = "stall"
            else:
                self.fail(s, "candidate refused: support below t")
        elif self.state == "stall":
            if s >= self.b_time:
                i = V.record_beacon(s)
                Y = sim.new_lez(self.TA, min(sim.bad), True, s, s, frozenset([i]), honest=False)
                if V.finalize(self.TA, i, s, sim.bad, who="adversary"):
                    return self.succeed(s)
                self.fail(s, "finalisation refused")


def make_attack(sim):
    kind = sim.P.attack
    if kind == "none":
        return NoAttack(sim)
    if kind == "withhold":
        return Withhold(sim)
    if kind == "hog":
        return Hog(sim)
    if kind == "forkstall":
        return ForkStall(sim)
    if kind == "skew":
        return SkewCapture(sim)
    if kind == "partition":
        return Partition(sim)
    if kind in ("invalid", "orphan"):
        return Fabricate(sim, kind)
    raise ValueError(kind)


def run(P: Params, seed: int):
    return Sim(P, seed).run()


if __name__ == "__main__":
    import sys
    import time

    P = Params(n=int(sys.argv[1]) if len(sys.argv) > 1 else 4)
    t0 = time.time()
    out = run(P, 1)
    print(f"{time.time() - t0:.1f}s", out["lez_height"], out["bedrock_height"], len(out["finals"]))
    for r in out["finals"]:
        print(r)
