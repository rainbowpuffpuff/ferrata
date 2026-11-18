#!/usr/bin/env python3
"""Negative controls for the privacy suite's gate (docs/privacy/CONTROLS.md). Standard library only.

    python3 -I scripts/privacy/controls.py DATA_DIR OUT_DIR

DATA_DIR is what the collectors of scripts/privacy/run.sh wrote (target/privacy/). The script first runs the
suite's own analysis (adversary.py's main) on the unmodified data into OUT_DIR/baseline/, recording the exact
groups, contexts and secrets the suite used; then:

1. Planted leaks. For each operation and each value it must hide (adversary.py's hidden_steps: s, C, the leaf,
   fid, the destination, the depositor, the recipient, sOut, NOut, the memo pad, r, COut, memo, mac, the burn
   leaf, burnId, the seed and its two keys), a test-only copy of the 20 public records with that value written
   into a public field of every record: a constant field (fee.refund_to, the release's burnRoot, an extra vault
   event), a fresh-randomness field (the encrypted note, C, NOut) and the state (an account's state, a storage
   write); raw, and in the fresh field also byte-reversed, as hex text, and as its first or last 16 bytes. Also
   low-entropy values (a leaf or burn index), values only a linker derives (H(C), H(leaf), N on a deposit ...) and
   three leaks the gate is not expected to see. Each copy goes through the suite's three detectors: the
   differential gate, the hidden-column search and the adversary (at chance: every candidate set is all 20).
2. Weakened detectors. Each detector is mutated (a field skipped, only a prefix compared, an encoding ignored,
   a class or the shared-state rule widened, the adversary's windows lengthened or its derived identifiers
   dropped), run on the unmodified data (it must still pass) and on every planted copy; a mutant is caught when
   some planted leak the real detector finds goes unseen by it.
Writes OUT_DIR/controls.json and OUT_DIR/controls.md.
"""

import copy
import importlib.util
import json
import re
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("adversary", HERE / "adversary.py")
A = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(A)

STEP_GROUP = {"deposit": "deposit (A: private destinations)", "claim": "claim, private (default)",
              "burn": "burn, private holding", "release": "release, NOut of the release statement"}
STEP_TRUTH = {"deposit": "deposit", "claim": "claim", "burn": "burn", "release": "release"}
# The adversary's game each step's records take part in: (left group, right group, left truth, right truth).
GAME = {"deposit": ("deposit", "claim"), "claim": ("deposit", "claim"), "burn": ("burn", "release"),
        "release": ("burn", "release")}
ENCODINGS = ("raw", "reversed", "hex", "prefix16", "suffix16")


# ------------------------------------------------------------------------------------------------
# The baseline: the suite's own run, with its inputs captured
# ------------------------------------------------------------------------------------------------

def baseline(data_dir, out_dir):
    cap = {"gate": [], "attack": [], "hidden": None}
    real = (A.gate, A.attack, A.hidden_check)

    def spy_gate(recs, ctx):
        cap["gate"].append((recs, ctx))
        return real[0](recs, ctx)

    def spy_attack(left, right, tl, tr, meta, cl, cr, use_derived=True, keep_order_state=False):
        if use_derived and not keep_order_state:
            cap["attack"].append((left, right, tl, tr, meta, cl, cr))
        return real[1](left, right, tl, tr, meta, cl, cr, use_derived=use_derived, keep_order_state=keep_order_state)

    def spy_hidden(steps, G, T, **kw):
        cap["hidden"] = (steps, G, T)
        return real[2](steps, G, T, **kw)

    A.gate, A.attack, A.hidden_check = spy_gate, spy_attack, spy_hidden
    argv = sys.argv
    sys.argv = ["adversary.py", str(data_dir), str(out_dir / "baseline")]
    try:
        A.main()
    finally:
        sys.argv = argv
        A.gate, A.attack, A.hidden_check = real
    steps, G, T = cap["hidden"]
    by_id = {id(v): k for k, v in G.items()}
    gate_ctx = {by_id[id(recs)]: ctx for recs, ctx in cap["gate"]}
    games = {}
    for left, right, tl, tr, meta, cl, cr in cap["attack"]:
        games[(by_id[id(left)], by_id[id(right)])] = (meta, cl, cr)
    results = json.loads((out_dir / "baseline" / "results.json").read_text())
    return steps, G, T, gate_ctx, games, results


# ------------------------------------------------------------------------------------------------
# Detectors (the suite's functions) and their verdicts on one step's records
# ------------------------------------------------------------------------------------------------

class Suite:
    def __init__(self, steps, G, T, gate_ctx, games):
        self.steps = {s[1]: s for s in steps}  # by group
        self.G, self.T, self.gate_ctx, self.games = G, T, gate_ctx, games

    def gate(self, step, recs, ctx=None):
        ok, _ = A.gate(recs, ctx or self.gate_ctx[STEP_GROUP[step]])
        return not ok  # caught

    def hidden(self, step, recs, find=None, data_of=None):
        g = STEP_GROUP[step]
        G = dict(self.G)
        G[g] = recs
        kw = {}
        if find:
            kw["find"] = find
        if data_of:
            kw["data_of"] = data_of
        h = A.hidden_check([self.steps[g]], G, self.T, **kw)
        return h[self.steps[g][0]]["found"] > 0

    def adversary(self, step, recs, use_derived=True, strip=None):
        lk, rk = (STEP_GROUP[s] for s in GAME[step])
        left = recs if STEP_GROUP[step] == lk else self.G[lk]
        right = recs if STEP_GROUP[step] == rk else self.G[rk]
        if strip:
            left, right = [strip(x) for x in left], [strip(x) for x in right]
        meta, cl, cr = self.games[(lk, rk)]
        lt, rt = (STEP_TRUTH[s] for s in GAME[step])
        res = A.attack(left, right, self.T[lt], self.T[rt], meta, cl, cr, use_derived=use_derived)
        return res["candidate_set_sizes"] != [(len(right), len(right))]  # caught: some record narrowed down


# ------------------------------------------------------------------------------------------------
# Planting
# ------------------------------------------------------------------------------------------------

def clone(x):
    y = copy.copy(x)
    y.blobs, y.fields, y.time = list(x.blobs), list(x.fields), list(x.time)
    for a in ("diff", "writes", "balances"):
        if hasattr(x, a):
            setattr(y, a, list(getattr(x, a)))
    return y


def replace_bytes(x, old, new):
    """Every occurrence of `old` in the record's public data becomes `new` (the same length)."""
    assert len(old) == len(new) and len(old) >= 16
    x.blobs = [(n, v.replace(old, new)) for n, v in x.blobs]
    x.fields = [(n, v.replace(old, new)) for n, v in x.fields]
    for a in ("diff", "writes", "balances"):
        if hasattr(x, a):
            setattr(x, a, [tuple(e.replace(old, new) for e in t) for t in getattr(x, a)])


def overwrite(old, payload, offset=0):
    payload = payload[:max(0, len(old) - offset)]
    return old[:offset] + payload + old[offset + len(payload):]


def encode_payload(v, enc):
    return {"raw": v, "reversed": v[::-1], "hex": v.hex().encode(), "prefix16": v[:16], "suffix16": v[-16:]}[enc]


def field(x, name):
    return next(v for n, v in x.fields if n == name)


def plant(step, x, carrier, payload, offset=0, ctx=None, meta=None):
    """A copy of record `x` with `payload` written into the carrier (a public field of that step's records)."""
    y = clone(x)
    if step in ("claim", "burn"):
        if carrier in ("fresh", "const", "message"):
            name = {"fresh": "private_actions[0].note.ciphertext", "const": "fee.refund_to",
                    "message": "boundary[2].out.message.Issue.relayer" if step == "claim"
                    else "boundary[0].out.message.Burn.config_id"}[carrier]
            old = field(y, name)
            off = offset if offset >= 0 else len(old) + offset
            replace_bytes(y, old, overwrite(old, payload, off))
        else:  # state: the largest account state that differs per record (not a shared account)
            acc, before, after = max((d for d in y.diff if d[0] not in ctx["shared"]), key=lambda d: len(d[2]))
            replace_bytes(y, after, overwrite(after, payload, offset if offset >= 0 else len(after) + offset))
    elif step == "deposit" and carrier == "const":  # a vault event the deposit does not have: Leak(bytes)
        li = sum(1 for n, _ in y.blobs if n == "log.emitter")
        topic = A.keccak256(b"Leak(bytes)")
        data = payload + bytes(-len(payload) % 32)
        y.blobs += [("log.emitter", meta["vault"]), ("log.topic", topic), ("log.data", data)]
        y.fields += [(f"event.log[{li}].topic[0]", topic)]
        y.fields += [(f"event.log[{li}].data[{i}]", data[32 * i:32 * (i + 1)]) for i in range(len(data) // 32)]
    else:
        if carrier == "fresh":
            old = field(y, "calldata.commitment" if step == "deposit" else "calldata.request.nullifier")
        elif carrier == "const":
            old = field(y, "calldata.request.burnRoot")
        else:  # state: deposit: the deposit root's new value; release: the token's escrow total
            if step == "deposit":
                old = next(w[3] for w in y.writes if w[1] == A.word(0))
            else:
                old = next(w[3] for w in y.writes if "per-token" in ctx["slot_names"].get(w[1], ""))
        off = offset if offset >= 0 else len(old) + offset
        replace_bytes(y, old, overwrite(old, payload, off))
    return y


# ------------------------------------------------------------------------------------------------
# The planted set
# ------------------------------------------------------------------------------------------------

def planted_set(suite, meta):
    """(step, value label, carrier, encoding, expected, values_of_user) for every control."""
    out = []
    for step in ("deposit", "claim", "burn", "release"):
        g = STEP_GROUP[step]
        values_of = suite.steps[g][3]
        labels = [label for label, _ in values_of(0)]
        for label in labels:
            pick = (lambda lab, vo: (lambda u: next(v for l2, v in vo(u) if l2 == lab)))(label, values_of)
            for carrier in ("const", "message", "fresh", "state") if step in ("claim", "burn") else ("const", "fresh", "state"):
                out.append((step, label, carrier, "raw", "caught", pick, 0))
            for enc in ENCODINGS[1:]:
                out.append((step, label, "fresh", enc, "caught", pick, 0))
    G, T = suite.G, suite.T
    # Values only a linker derives from the other side's public data (adversary.py's `derived`).
    dep = {T["deposit"][x.seq]: dict((A.norm(n), v) for n, v in x.fields) for x in G[STEP_GROUP["deposit"]]}
    claim_n = {T["claim"][x.seq]: next(v for n, v in x.fields if n.endswith("Issue.nullifier")) for x in G[STEP_GROUP["claim"]]}
    burns = {T["burn"][x.seq]: x for x in G[STEP_GROUP["burn"]]}

    def leaf_of(u):
        d = dep[u]
        return A.deposit_leaf(u, d["event.Deposit.depositor"][12:], d["event.Deposit.token"][12:], d["event.Deposit.amount"],
                              d["event.Deposit.commitment"])

    def burn_parts(u):
        x = burns[u]
        g = lambda k: next(v for n, v in x.fields if n.endswith(k))
        leaf = A.burn_leaf(meta["cid"], x.seq, g("Burn.token"), g("Burn.amount"), g("Burn.c_out"), g("Burn.r"))
        note = A.sha(g("Burn.r") + g("Burn.memo") + g("Burn.mac") + g("Burn.token") + g("Burn.amount") + g("Burn.c_out"))
        return leaf, A.burn_id(meta["cid"], x.seq, leaf), note

    linkers = [
        ("deposit", "N (the claim's nullifier)", lambda u: claim_n[u]),
        ("claim", "H(C)", lambda u: A.sha(dep[u]["event.Deposit.commitment"])),
        ("claim", "H(leaf)", lambda u: A.sha(leaf_of(u))),
        ("claim", "H(fid)", lambda u: A.sha(A.fid_of(meta["chain"], meta["vault"], u, leaf_of(u)))),
        ("release", "H(burn leaf)", lambda u: A.sha(burn_parts(u)[0])),
        ("release", "H(burnId)", lambda u: A.sha(burn_parts(u)[1])),
        ("release", "H(note), the burn record's key", lambda u: burn_parts(u)[2]),
    ]
    for step, label, f in linkers:
        for carrier in ("const", "fresh"):
            out.append((step, label, carrier, "raw", "caught", f, 0))
    # Low-entropy values: only the differential gate can see them, and only in a field that is otherwise constant.
    li_le = lambda u: u.to_bytes(8, "little")  # the deposit's leaf index (= the user here), LEZ's encoding
    bi_be = lambda u: burns[u].seq.to_bytes(8, "big")  # the burn's index in the burn tree, Ethereum's encoding
    out.append(("claim", "leaf index (u64)", "const", "raw", "caught", li_le, -8))
    out.append(("claim", "leaf index (u64)", "message", "raw", "caught", li_le, -8))
    out.append(("release", "burn index (u64)", "const", "raw", "caught", bi_be, -8))
    # What the gate is not expected to see (CONTROLS.md, limits).
    out.append(("claim", "leaf index (u64)", "fresh", "raw", "limit: low entropy in a fresh field", li_le, 0))
    mask = A.sha(b"a public mask")
    s_of = lambda u: next(v for lab, v in suite.steps[STEP_GROUP["claim"]][3](u) if lab == "s")
    out.append(("claim", "s XOR a public constant", "fresh", "raw", "limit: an encoding outside the four",
                lambda u: bytes(a ^ m for a, m in zip(s_of(u), mask)), 0))
    out.append(("claim", "s in two 8-byte halves, 40 bytes apart", "fresh", "raw", "limit: no 16-byte window",
                lambda u: s_of(u)[:8] + bytes(40) + s_of(u)[8:16], 0))
    return out


def variant(suite, step, carrier, enc, value_of, offset, meta):
    g = STEP_GROUP[step]
    ctx = suite.gate_ctx[g]
    recs = []
    for x in suite.G[g]:
        u = suite.T[STEP_TRUTH[step]][x.seq]
        recs.append(plant(step, x, carrier, encode_payload(value_of(u), enc), offset, ctx, meta))
    return recs


# ------------------------------------------------------------------------------------------------
# Weakened detectors
# ------------------------------------------------------------------------------------------------

def strip_fields(pattern):
    """A record without the fields whose name matches `pattern`: their bytes are cut from every blob too."""
    rx = re.compile(pattern)

    def f(x):
        y = clone(x)
        gone = [v for n, v in y.fields if rx.search(A.norm(n)) and len(v) >= 16]
        y.fields = [(n, v) for n, v in y.fields if not rx.search(A.norm(n))]
        for v in gone:  # wherever the bytes occur (a dev-mode receipt's claim holds the whole message)
            y.blobs = [(n, b.replace(v, b"")) for n, b in y.blobs]
            y.fields = [(n, b.replace(v, b"")) for n, b in y.fields]
        return y
    return f


def strip_blobs(prefixes, field_prefixes=()):
    def f(x):
        y = clone(x)
        y.blobs = [(n, v) for n, v in y.blobs if not n.startswith(prefixes)]
        if field_prefixes:
            y.fields = [(n, v) for n, v in y.fields if not n.startswith(field_prefixes)]
        if hasattr(y, "diff") and "diff." in prefixes:
            y.diff = []
        if hasattr(y, "writes") and "write." in prefixes:
            y.writes = []
        return y
    return f


def mutants():
    """(detector, name, what it skips, how to run it on (suite, step, recs))."""
    H = [
        ("no byte-reversed form", dict(find=lambda d, v: A.find_leak(d, v, forms=("raw", "hex", "HEX")))),
        ("no hex form", dict(find=lambda d, v: A.find_leak(d, v, forms=("raw", "reversed")))),
        ("whole values only, no 16-byte windows", dict(find=lambda d, v: A.find_leak(d, v, windows=False))),
        ("only a prefix of each value (its first 16 bytes)", dict(find=lambda d, v: A.find_leak(d, v[:16], windows=False))),
        ("only a prefix of each record (its first 2,048 bytes)", dict(data_of=lambda x: A.everything(x)[:2048])),
        ("encrypted notes skipped", dict(data_of=lambda x: A.everything(strip_fields(r"\.note\.ciphertext$")(x)))),
        ("state skipped (LEZ account states, Ethereum storage writes)", dict(data_of=lambda x: A.everything(strip_blobs(("diff.", "write."))(x)))),
        ("events skipped", dict(data_of=lambda x: A.everything(strip_blobs(("log.", "event"), ("event.",))(x)))),
    ]
    out = [("hidden", n, (lambda kw: lambda s, step, recs: s.hidden(step, recs, **kw))(kw)) for n, kw in H]

    def gate_with(classify=None, cut=None, transform=None):
        def run(s, step, recs):
            saved = (A.classify, A.cut)
            if classify:
                A.classify = classify
            if cut:
                A.cut = cut
            try:
                return s.gate(step, [transform(x) for x in recs] if transform else recs)
            finally:
                A.classify, A.cut = saved
        return run
    real_classify, real_cut = A.classify, A.cut
    broad = re.compile(r"(Issue|Burn)\.|^fee\.|^calldata\.request\.")

    def no_state_values(x):
        y = clone(x)
        if hasattr(y, "diff"):
            y.diff = [(acc, b"", b"") for acc, _, _ in y.diff]
        if hasattr(y, "writes"):
            y.writes = [(acc, slot, b"", b"") for acc, slot, _, _ in y.writes]
            y.balances = [(acc, b"", b"") for acc, _, _ in y.balances]
        return y
    out += [
        ("gate", "every varying field cut, classified or not", gate_with(classify=lambda n: (A.FRESH, "mutant"))),
        ("gate", "classes too broad (Issue.*, Burn.*, fee.*, request.* as fresh randomness)",
         gate_with(classify=lambda n: (A.FRESH, "mutant") if broad.search(A.norm(n)) else real_classify(n))),
        ("gate", "only the first 256 bytes of each part compared",
         gate_with(cut=lambda blob, values: real_cut(blob, values)[:256])),
        ("gate", "only each part's length compared",
         gate_with(cut=lambda blob, values: str(len(real_cut(blob, values))).encode())),
        ("gate", "account states and storage values not compared", gate_with(transform=no_state_values)),
    ]
    old_window = A.N_WINDOW

    def windows32(s, step, recs):
        A.N_WINDOW = 32
        try:
            return s.adversary(step, recs)
        finally:
            A.N_WINDOW = old_window
    out += [
        ("adversary", "no derived identifiers", lambda s, step, recs: s.adversary(step, recs, use_derived=False)),
        ("adversary", "32-byte windows instead of 16", windows32),
        ("adversary", "state and events not read", lambda s, step, recs: s.adversary(step, recs, strip=strip_blobs(("diff.", "write.", "log.", "event")))),
    ]
    return out


# ------------------------------------------------------------------------------------------------

def main():
    data_dir, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)
    t0 = time.time()
    steps, G, T, gate_ctx, games, base = baseline(data_dir, out_dir)
    suite = Suite(steps, G, T, gate_ctx, games)
    meta = next(iter(games.values()))[0]
    real = {"gate": suite.gate, "hidden": suite.hidden, "adversary": suite.adversary}
    detectors = ("gate", "hidden", "adversary")

    base_verdict = {d: any(real[d](step, G[STEP_GROUP[step]]) for step in STEP_GROUP) for d in detectors}
    rows = []
    for step, label, carrier, enc, expected, value_of, offset in planted_set(suite, meta):
        recs = variant(suite, step, carrier, enc, value_of, offset, meta)
        r = {"step": step, "value": label, "carrier": carrier, "encoding": enc, "expected": expected}
        for d in detectors:
            r[d] = real[d](step, recs)
        r["caught"] = r["gate"] or r["hidden"] or r["adversary"]
        r["_recs"] = recs
        rows.append(r)
    t_planted = time.time() - t0

    mres = []
    for det, name, run in mutants():
        on_base = any(run(suite, step, G[STEP_GROUP[step]]) for step in STEP_GROUP)
        kills, kills_composite, checked = [], [], 0
        for r in rows:
            if not r[det]:
                continue  # the real detector does not see this one: it cannot tell the mutant apart
            checked += 1
            if not run(suite, r["step"], r["_recs"]):
                kills.append(f'{r["step"]}: {r["value"]} ({r["carrier"]}, {r["encoding"]})')
                if not any(r[d] for d in detectors if d != det):
                    kills_composite.append(kills[-1])
        mres.append({"detector": det, "mutant": name, "false alarm on the unmodified data": on_base,
                     "planted leaks the real detector finds": checked, "of them missed by the mutant": len(kills),
                     "of them missed by the whole gate with this mutant": len(kills_composite),
                     "caught": bool(kills) or on_base, "examples": kills[:4], "examples (whole gate)": kills_composite[:4]})
    for r in rows:
        del r["_recs"]
    expected_caught = [r for r in rows if r["expected"] == "caught"]
    limits = [r for r in rows if r["expected"] != "caught"]
    summary = {
        "baseline": {"gate passes": {k: v["pass"] for k, v in base["gate"].items()},
                     "hidden": base["hidden"],
                     "adversary success": {k: v["success"] for k, v in base["adversary"].items()},
                     "any detector fires on the unmodified data (default groups)": base_verdict},
        "planted": {"total": len(rows), "expected caught": len(expected_caught),
                    "caught": sum(1 for r in expected_caught if r["caught"]),
                    "by gate": sum(1 for r in expected_caught if r["gate"]),
                    "by hidden search": sum(1 for r in expected_caught if r["hidden"]),
                    "by adversary": sum(1 for r in expected_caught if r["adversary"]),
                    "missed": [r for r in expected_caught if not r["caught"]],
                    "limits": len(limits), "limits seen anyway": [r for r in limits if r["caught"]]},
        "mutants": {"total": len(mres), "caught": sum(1 for m in mres if m["caught"]),
                    "caught by a missed planted leak": sum(1 for m in mres if m["of them missed by the mutant"]),
                    "change the whole gate's verdict": sum(1 for m in mres if m["of them missed by the whole gate with this mutant"])},
        "seconds": {"planted": round(t_planted, 1), "total": round(time.time() - t0, 1)},
    }
    (out_dir / "controls.json").write_text(json.dumps({"summary": summary, "planted": rows, "mutants": mres}, indent=1))
    write_md(out_dir / "controls.md", summary, rows, mres)
    print(json.dumps(summary, indent=1))


def write_md(path, summary, rows, mres):
    p = summary["planted"]
    c = ["# Privacy gate: negative controls (generated)", "",
         "Written by `scripts/privacy/controls.py`; see [CONTROLS.md](../CONTROLS.md) for what this is.", "",
         f"Planted leaks: {p['total']} copies of the public data; {p['expected caught']} must be caught, "
         f"{p['caught']} are (gate {p['by gate']}, hidden-column search {p['by hidden search']}, adversary {p['by adversary']}); "
         f"{p['limits']} are leaks the gate is not expected to see, of which {len(p['limits seen anyway'])} were seen.", "",
         f"Weakened detectors: {summary['mutants']['total']}, caught {summary['mutants']['caught']}.", "",
         "## Planted leaks", "",
         "| step | value | carrier | encoding | gate | hidden search | adversary | caught | expected |",
         "|---|---|---|---|---|---|---|---|---|"]
    yn = lambda v: "yes" if v else "no"
    for r in rows:
        c.append(f'| {r["step"]} | {r["value"]} | {r["carrier"]} | {r["encoding"]} | {yn(r["gate"])} | {yn(r["hidden"])} | '
                 f'{yn(r["adversary"])} | **{yn(r["caught"])}** | {r["expected"]} |')
    c += ["", "## Weakened detectors", "",
          "| detector | mutant | false alarm on the unmodified data | planted leaks the real detector finds | missed by the mutant | missed by the whole gate | caught | example |",
          "|---|---|---|---:|---:|---:|---|---|"]
    for m in mres:
        ex = (m["examples"] or ["-"])[0]
        c.append(f'| {m["detector"]} | {m["mutant"]} | {yn(m["false alarm on the unmodified data"])} | '
                 f'{m["planted leaks the real detector finds"]} | {m["of them missed by the mutant"]} | '
                 f'{m["of them missed by the whole gate with this mutant"]} | **{yn(m["caught"])}** | {ex} |')
    path.write_text("\n".join(c) + "\n")


if __name__ == "__main__":
    main()
