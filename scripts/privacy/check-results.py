#!/usr/bin/env python3
"""The privacy suite's CI gate (RFP-022 Privacy #1, #4, #6). Standard library only.

    python3 -I scripts/privacy/check-results.py OUT_DIR [--committed DIR]
    python3 -I scripts/privacy/check-results.py --self-test

`scripts/privacy/run.sh` runs the 20 users' round trips and `scripts/privacy/adversary.py` writes
`results.json`, `summary.md`, `b6.md` and `fields.md` into OUT_DIR — but exits 0 whatever it finds.
This reads them and exits 0 only when privacy holds, so CI has a check that fails. DIR (default
`docs/privacy/generated`) is the committed copy: the published tables.

What is checked, and the requirement each check answers (RFP-022, "Privacy Preservation"):

1. The differential gate passes for every private default group (the private deposit, the private
   claim, the private burn, the release's NOut), and the groups `adversary.py` builds as
   deliberately linkable — the naive nullifier, the naive NOut, the plaintext claim — are found
   linkable (the gate fails on the group, or the adversary identifies its pairs), which shows the
   check has power (Privacy #1; #4).
2. The adversary on the private defaults, inbound and outbound, does no better than chance and
   identifies no pair exactly; on the controls it succeeds (Privacy #1: "no correlation derivable
   from public state identifies the true deposit-to-mint pairing better than chance").
3. Every value that must stay hidden (the `hidden` section: the seed, kAuth, kView, the
   destinations, s, C, the burn note's parts, ...) is found 0 times in the records' public bytes
   (Privacy #4: "Information that would connect the two legs must never leave the user's control
   ... must be absent from all submitted transaction data").
4. The recovery from the seed alone holds: the seed, which never leaves the user's control, is the
   only thing that connects the two legs (Privacy #4), and every user's wallet finds its unclaimed
   deposits and unreleased burns, and nothing else (Privacy #4).
5. The four generated files equal the committed ones byte for byte, so the published tables —
   including `fields.md`, what each third party sees (Privacy #6: "Where a third party submits on
   the user's behalf, document precisely what that party learns") — are what the code produces.
   Normalised on both sides, and only this: the proving time, a wall-clock time the collectors
   record in every run, dev mode included (`prove_s` in results.json, the "min / median / max; sum"
   cell of summary.md's proofs table). The image ids are compared as they are: the guests build
   independent of the path (docs/image-ids.md), so another id is another program, and fails.

After `scripts/privacy/run.sh [OUT_DIR]` (its OUT_DIR defaults to docs/privacy/generated), run:

    python3 -I scripts/privacy/check-results.py OUT_DIR

`--self-test` loads the committed `results.json` instead, flips one verdict at a time in memory and
checks that each altered case is refused.
"""

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
COMMITTED_DEFAULT = REPO / "docs" / "privacy" / "generated"
FILES = ("fields.md", "b6.md", "summary.md", "results.json")

# The suite is 20 users per direction (lez/harness/tests/privacy_suite.rs, "Twenty fresh users" in
# docs/privacy/RESULTS.md), and adversary.py's chance is 1/20 (attack(): "chance": 1 / n).
N_USERS = 20
CHANCE = 1 / N_USERS
# success and chance are means of at most 20 terms of size 1/20; the committed record's private
# games read success 0.05000000000000001 against chance 0.05 (accumulated 1/20 sums), well inside.
CHANCE_TOL = 1e-12
# A control game "succeeds" beyond any doubt of chance. The committed record (committed
# docs/privacy/generated/results.json) shows success 1.0 with 20 of 20 pairs identified exactly on
# all three controls — structural: fid and burnId are public-derivable, the plaintext witness opens
# the deposit — so the bar only absorbing noise; chance is 0.05.
CONTROL_SUCCESS = 0.5

# Key names as adversary.py's main() writes them (its G and `games`).
GATE_PRIVATE = ("deposit (A: private destinations)", "claim, private (default)", "burn, private holding",
                "release, NOut of the release statement")
CONTROLS = (  # (gate group, adversary game): the deliberately linkable variants
    ("claim, naive nullifier N = fid", "inbound, naive nullifier N = fid"),
    ("claim, plaintext", "inbound, plaintext claim"),
    ("release, naive NOut = burnId", "outbound, naive NOut = burnId"),
)
PRIVATE_GAMES = ("inbound, private (default)", "outbound, private (default)")
HIDDEN_STEPS = ("deposit hides the LEZ destination", "claim hides which deposit and a private destination",
                "burn hides the holder and the recipient", "release hides which burn")

# The only environment-dependent value adversary.py writes, normalised in check 5 (see the
# docstring): the proving times, wall-clock, in every run.
IMAGE_PATTERNS = (re.compile(r"program image `([0-9a-f]{64})`"), re.compile(r"circuit `([0-9a-f]{64})`"))
PROVE_TIME_JSON = re.compile(r'"prove_s": \{[^{}]*\}')  # spread() of the collectors' proving times
PROVE_TIME_MD = re.compile(r"[0-9]+\.[0-9] / [0-9]+\.[0-9] / [0-9]+\.[0-9]; [0-9]+")  # summary.md's cell


# ------------------------------------------------------------------------------------------------
# Helpers
# ------------------------------------------------------------------------------------------------

def dig(obj, *keys):
    for k in keys:
        if not isinstance(obj, dict) or k not in obj:
            return None
        obj = obj[k]
    return obj


def fmt(x):
    return f"{x:.4f}" if isinstance(x, (int, float)) else str(x)


def short(s, n=110):
    s = str(s)
    return s if len(s) <= n else s[:n] + "…"


def user_count(res):
    pu = dig(res, "recovery", "per_user")
    if isinstance(pu, list) and pu:
        return len(pu)
    ch = dig(res, "adversary", "inbound, private (default)", "chance")
    return round(1 / ch) if isinstance(ch, (int, float)) and ch > 0 else 0


def at_chance(av):
    return (isinstance(av, dict) and isinstance(av.get("success"), (int, float))
            and isinstance(av.get("chance"), (int, float)) and abs(av["success"] - av["chance"]) <= CHANCE_TOL)


def adversary_succeeds(av, n):
    """A control's game identifies the pairing: far above chance, and most pairs pinned exactly."""
    return (isinstance(av, dict) and isinstance(av.get("success"), (int, float))
            and av["success"] >= CONTROL_SUCCESS and av.get("exactly_identified", -1) >= max(1, n // 2))


def load_files(directory):
    out = {}
    for name in FILES:
        p = Path(directory) / name
        out[name] = p.read_bytes() if p.is_file() else None
    return out


def parse_results(raw):
    try:
        return json.loads(raw) if raw is not None else None
    except (ValueError, TypeError):
        return None


# ------------------------------------------------------------------------------------------------
# The checks. Each returns a list of (name, ok, what was compared).
# ------------------------------------------------------------------------------------------------

def record_checks(res, n):
    ch = dig(res, "adversary", "inbound, private (default)", "chance")
    ok = n == N_USERS and isinstance(ch, (int, float)) and abs(ch - CHANCE) <= CHANCE_TOL
    return [("0 suite: the record is the 20-user suite's (adversary.py's chance is 1/20)", ok,
             f"{n} users, chance {fmt(ch)}")]


def gate_checks(res, n):
    out = []
    gate = dig(res, "gate") or {}
    for g in GATE_PRIVATE:
        v = gate.get(g)
        ok = isinstance(v, dict) and v.get("pass") is True
        detail = f"pass={v.get('pass')}" if isinstance(v, dict) else "group missing from results.json"
        if not ok and isinstance(v, dict) and "pass" in v:
            detail = f"pass={v['pass']} ({short(v.get('detail'))})"
        out.append((f"1 gate: {g}: the private default group passes the differential gate "
                    "(Privacy #4: the connecting information is absent from the submitted data)", ok, detail))
    for g, game in CONTROLS:
        gv = gate.get(g)
        gate_fail = isinstance(gv, dict) and gv.get("pass") is False
        av = dig(res, "adversary", game) or {}
        adv = adversary_succeeds(av, n)
        if gate_fail and adv:
            how = f"the gate fails on the group, and the adversary identifies it ({fmt(av.get('success'))})"
        elif gate_fail:
            how = "the gate fails on the group"
        elif adv:
            how = (f"the adversary identifies it (success {fmt(av.get('success'))}, "
                   f"{av.get('exactly_identified')} of {n})")
        else:
            how = "neither: the gate passes and the adversary is at chance — the control shows no power"
        out.append((f"1 control: {g} is found linkable (the gate fails on it, or the adversary identifies it)",
                    gate_fail or adv, how))
    return out


def adversary_checks(res, n):
    out = []
    for game in PRIVATE_GAMES:
        av = dig(res, "adversary", game) or {}
        out.append((f"2 adversary: {game} is at chance (Privacy #1: 'no correlation derivable from public "
                    "state identifies the true deposit-to-mint pairing better than chance')", at_chance(av),
                    f"success {fmt(av.get('success'))} vs chance {fmt(av.get('chance'))} (tolerance {CHANCE_TOL:g})"))
        out.append((f"2 adversary: {game} identifies no pair exactly", av.get("exactly_identified") == 0,
                    f"{av.get('exactly_identified')} of {n}"))
    for _, game in CONTROLS:
        av = dig(res, "adversary", game) or {}
        out.append((f"2 adversary: control {game} succeeds on the deliberately linkable variant",
                    adversary_succeeds(av, n),
                    f"success {fmt(av.get('success'))} (bar {CONTROL_SUCCESS}); {av.get('exactly_identified')} "
                    f"of {n} identified exactly (bar {max(1, n // 2)})"))
    return out


def hidden_checks(res, n):
    out = []
    hid = dig(res, "hidden") or {}
    for step in HIDDEN_STEPS:
        v = hid.get(step)
        if not isinstance(v, dict):
            ok, detail = False, "step missing from the hidden section of results.json"
        else:
            found, searched = v.get("found"), v.get("values searched", 0)
            # The committed record searched at least two values per user per step (40 and 120; the
            # current adversary.py searches 680 in total, docs/privacy/RESULTS.md): fewer means the
            # search did not run, and "0 found" would be vacuous.
            ok = found == 0 and isinstance(searched, int) and searched >= 2 * n
            detail = f"{found} of {searched} values found in the records' public bytes (bar: at least {2 * n})"
        out.append((f"3 hidden: {step} (Privacy #4: what must stay hidden is absent from all submitted "
                    "transaction data)", ok, detail))
    found_keys = sorted(k for k in hid if isinstance(k, str) and k.endswith(": FOUND"))
    out.append(("3 hidden: no value recorded as FOUND", not found_keys, ", ".join(found_keys) or "none"))
    return out


def recovery_checks(res, n):
    rec = dig(res, "recovery") or {}

    def line(name, ok, detail):
        return (f"4 recovery: {name} (Privacy #4: the seed alone, which never leaves the user's control, "
                "is the only thing that connects the legs)", ok, detail)

    ue = rec.get("unclaimed_expected")
    ue_ok = isinstance(ue, int) and ue >= 1
    out = [line("every unclaimed deposit recovered", ue_ok and rec.get("unclaimed_found") == ue,
                f"{rec.get('unclaimed_found')} of {ue}")]
    exp = n - ue if ue_ok else None
    seen = rec.get("claimed_seen_as_claimed")
    out.append(line("every claimed deposit seen as claimed", exp is not None and seen == exp,
                    f"{seen} of {exp}"))
    re_ = rec.get("unreleased_expected")
    re_ok = isinstance(re_, int) and re_ >= 1
    out.append(line("every unreleased burn recovered", re_ok and rec.get("unreleased_found") == re_,
                    f"{rec.get('unreleased_found')} of {re_}"))
    exp = n - re_ if re_ok else None
    seen = rec.get("released_seen_as_released")
    out.append(line("every released burn seen as released", exp is not None and seen == exp,
                    f"{seen} of {exp}"))
    out.append(line("no other user's item taken as own", rec.get("foreign_items") == 0,
                    f"{rec.get('foreign_items')} foreign items"))
    out.append(line("every recipient decrypted and authenticated", rec.get("recipients_decrypted_correctly") == n,
                    f"{rec.get('recipients_decrypted_correctly')} of {n}"))
    out.append(line("every recovered NOut equals the vault's", rec.get("nout_matches_vault") == n,
                    f"{rec.get('nout_matches_vault')} of {n}"))
    out.append(line("the bridge core's recover_burns finds every burn", rec.get("rust_core") == n,
                    f"{rec.get('rust_core')} of {n}"))
    # One deposit and one burn per user is the scenario (privacy_suite.rs; the committed record shows
    # deposits_found 1, burns_found 1, recipient_ok true for every user).
    pu = rec.get("per_user")
    ok = (isinstance(pu, list) and len(pu) == n
          and all(isinstance(p, dict) and p.get("user") == i and p.get("deposits_found") == 1
                  and p.get("burns_found") == 1 and p.get("recipient_ok") is True for i, p in enumerate(pu)))
    out.append(line("each user's wallet finds exactly its one deposit and one burn, recipient ok", ok,
                    f"{len(pu) if isinstance(pu, list) else 0} of {n} wallets"))
    return out


def normalise(text):
    text = PROVE_TIME_JSON.sub('"prove_s": "<proving time>"', text)
    text = PROVE_TIME_MD.sub("<proving time>", text)
    return text


def file_checks(out_files, committed_files, res, res_committed, committed_dir):
    out = []
    for name in FILES:
        a, b = out_files.get(name), committed_files.get(name)
        where = "OUT_DIR" if a is None else str(committed_dir)
        if a is None or b is None:
            out.append((f"5 published: {name} equals the committed file byte for byte (Privacy #6: the "
                        "published tables, fields.md's third-party view included, are what the code produces)",
                        False, f"missing in {where}"))
            continue
        na = normalise(a.decode("utf-8", "replace"))
        nb = normalise(b.decode("utf-8", "replace"))
        if na == nb:
            ok, detail = True, "identical (the proving times normalised; nothing else)"
        else:
            la, lb = na.split("\n"), nb.split("\n")
            i = next((k for k in range(min(len(la), len(lb))) if la[k] != lb[k]), min(len(la), len(lb)))
            sa = la[i] if i < len(la) else "<end of file>"
            sb = lb[i] if i < len(lb) else "<end of file>"
            ok, detail = False, f"first difference at line {i + 1}: {short(sa, 60)!r} vs {short(sb, 60)!r}"
        out.append((f"5 published: {name} equals the committed file byte for byte (Privacy #6: the published "
                    "tables, fields.md's third-party view included, are what the code produces)", ok, detail))
    return out


def all_checks(res, out_files, committed_files, res_committed, committed_dir):
    n = user_count(res)
    checks = []
    checks += record_checks(res, n)
    checks += gate_checks(res, n)
    checks += adversary_checks(res, n)
    checks += hidden_checks(res, n)
    checks += recovery_checks(res, n)
    checks += file_checks(out_files, committed_files, res, res_committed, committed_dir)
    return checks


def report(checks):
    fails = 0
    for name, ok, detail in checks:
        print(f"{'ok' if ok else 'FAIL'}  {name}: {detail}")
        fails += not ok
    if fails:
        print(f"summary: {fails} of {len(checks)} checks FAILED")
    else:
        print(f"summary: {len(checks)} of {len(checks)} checks hold")
    return fails


# ------------------------------------------------------------------------------------------------
# --self-test: the committed results.json, one verdict altered at a time, each refused
# ------------------------------------------------------------------------------------------------

def neuter(av):
    """Make an adversary game look at-chance (a control the adversary can no longer see)."""
    av.update({"success": av.get("chance", CHANCE), "exactly_identified": 0,
               "candidate_set_sizes": [[N_USERS, N_USERS]], "max_score": 0.0})


def self_test():
    files = load_files(COMMITTED_DEFAULT)
    res = parse_results(files["results.json"])
    if res is None or any(v is None for v in files.values()):
        print(f"self-test: cannot read the committed files in {COMMITTED_DEFAULT}")
        return 1
    base = all_checks(res, files, files, res, COMMITTED_DEFAULT)
    if any(not ok for _, ok, _ in base):
        report(base)
        print("self-test: the committed record itself is refused; the checks are wrong, not the record")
        return 1
    print(f"self-test: the committed record passes all {len(base)} checks; now one verdict at a time")
    a = "adversary"
    g = "gate"
    h = "hidden"
    r = "recovery"
    cases = [
        ("gate pass flipped on 'deposit (A: private destinations)'", "1 gate: deposit (A: private",
         lambda d: d[g]["deposit (A: private destinations)"].__setitem__("pass", False), None),
        ("gate pass flipped on 'claim, private (default)'", "1 gate: claim, private",
         lambda d: d[g]["claim, private (default)"].__setitem__("pass", False), None),
        ("gate pass flipped on 'burn, private holding'", "1 gate: burn, private",
         lambda d: d[g]["burn, private holding"].__setitem__("pass", False), None),
        ("gate pass flipped on 'release, NOut of the release statement'", "1 gate: release, NOut",
         lambda d: d[g]["release, NOut of the release statement"].__setitem__("pass", False), None),
        ("the naive-nullifier control is no longer identified (its game at chance)", "1 control: claim, naive",
         lambda d: neuter(d[a]["inbound, naive nullifier N = fid"]), None),
        ("the naive-NOut control is no longer identified (its game at chance)", "1 control: release, naive",
         lambda d: neuter(d[a]["outbound, naive NOut = burnId"]), None),
        ("the plaintext-claim game at chance (its group still fails the gate)", "control inbound, plaintext claim",
         lambda d: neuter(d[a]["inbound, plaintext claim"]), None),
        ("the inbound private adversary is above chance", "2 adversary: inbound, private (default) is at chance",
         lambda d: d[a]["inbound, private (default)"].__setitem__("success", 0.25), None),
        ("the inbound private adversary identifies a pair exactly", "inbound, private (default) identifies",
         lambda d: (d[a]["inbound, private (default)"].__setitem__("exactly_identified", 1),
                    d[a]["inbound, private (default)"].__setitem__("success", 0.1)), None),
        ("the outbound private adversary is above chance", "2 adversary: outbound, private (default) is at chance",
         lambda d: d[a]["outbound, private (default)"].__setitem__("success", 0.25), None),
        ("the outbound private adversary identifies a pair exactly", "outbound, private (default) identifies",
         lambda d: (d[a]["outbound, private (default)"].__setitem__("exactly_identified", 1),
                    d[a]["outbound, private (default)"].__setitem__("success", 0.1)), None),
        ("a hidden value found in a claim's public bytes", "3 hidden: claim hides",
         lambda d: (d[h]["claim hides which deposit and a private destination"].__setitem__("found", 1),
                    d[h].__setitem__("claim hides which deposit and a private destination: FOUND", ["planted"])),
         None),
        ("a hidden value found in a release's public bytes", "3 hidden: release hides",
         lambda d: d[h]["release hides which burn"].__setitem__("found", 1), None),
        ("recovery misses an unclaimed deposit", "4 recovery: every unclaimed deposit",
         lambda d: d[r].__setitem__("unclaimed_found", 9), None),
        ("recovery misses a claimed deposit's spent nullifier", "4 recovery: every claimed deposit",
         lambda d: d[r].__setitem__("claimed_seen_as_claimed", 9), None),
        ("recovery misses an unreleased burn", "4 recovery: every unreleased burn",
         lambda d: d[r].__setitem__("unreleased_found", 9), None),
        ("recovery misses a released burn's NOut", "4 recovery: every released burn",
         lambda d: d[r].__setitem__("released_seen_as_released", 9), None),
        ("recovery takes another user's item as own", "4 recovery: no other user's",
         lambda d: d[r].__setitem__("foreign_items", 1), None),
        ("recovery fails a recipient's mac", "4 recovery: every recipient",
         lambda d: d[r].__setitem__("recipients_decrypted_correctly", 19), None),
        ("recovery derives a wrong NOut", "4 recovery: every recovered NOut",
         lambda d: d[r].__setitem__("nout_matches_vault", 19), None),
        ("the core's recover_burns misses a burn", "4 recovery: the bridge core's",
         lambda d: d[r].__setitem__("rust_core", 19), None),
        ("a user's wallet finds two deposits", "4 recovery: each user's wallet",
         lambda d: d[r]["per_user"][3].__setitem__("deposits_found", 2), None),
        # File cases: the run's copy differs from the published one; the verdicts stay whole.
        ("fields.md differs from the published copy by a byte", "5 published: fields.md", None,
         lambda f: f.__setitem__("fields.md", f["fields.md"] + b"\n")),
        ("b6.md differs from the published copy by a byte", "5 published: b6.md", None,
         lambda f: f.__setitem__("b6.md", f["b6.md"] + b"\n")),
        ("summary.md differs from the published copy by a byte", "5 published: summary.md", None,
         lambda f: f.__setitem__("summary.md", f["summary.md"] + b"\n")),
        ("results.json differs by a byte that keeps the same data", "5 published: results.json", None,
         lambda f: f.__setitem__("results.json", f["results.json"].replace(b'"gate": {', b'"gate":  {', 1))),
        # Another bridge program: its image id is compared as it is (the guests build independent of the path).
        ("summary.md names another bridge program image", "5 published: summary.md", None,
         lambda f: f.__setitem__("summary.md", f["summary.md"].replace(
             IMAGE_PATTERNS[0].search(f["summary.md"].decode()).group(1).encode(), b"0" * 64))),
        # What the normalisation is for: the proving time is wall-clock, recorded in every run.
        ("summary.md's proving times differ (wall clock)", None, None,
         lambda f: f.__setitem__("summary.md", PROVE_TIME_MD.sub(
             "9.9 / 9.9 / 9.9; 99", f["summary.md"].decode(), count=1).encode())),
    ]
    refused = accepted = problems = 0
    for label, expect, mutate, files_mutate in cases:
        altered = json.loads(json.dumps(res))
        out_files, committed_files = dict(files), dict(files)
        if mutate:
            mutate(altered)
            # The altered case's committed copy says the same, so only the verdict can refuse it.
            text = json.dumps(altered, indent=1).encode()
            out_files["results.json"] = committed_files["results.json"] = text
        if files_mutate:
            files_mutate(out_files)
        checks = all_checks(altered, out_files, committed_files,
                            parse_results(committed_files["results.json"]), COMMITTED_DEFAULT)
        failures = [name for name, ok, _ in checks if not ok]
        if expect is None:  # accepted by design: shows exactly what the normalisation swallows
            if failures:
                print(f"NOT accepted  {label}: refused by {failures}")
                problems += 1
            else:
                print(f"accepted      {label} (the proving time is normalised, as designed)")
                accepted += 1
        elif any(expect in f for f in failures):
            print(f"refused       {label} (by: {'; '.join(short(f, 80) for f in failures)})")
            refused += 1
        else:
            print(f"NOT refused   {label} (failing: {failures or 'nothing'})")
            problems += 1
    print(f"self-test: {refused} altered cases refused, {accepted} accepted as the normalisation's one "
          f"purpose, {problems} problems")
    return 1 if problems else 0


# ------------------------------------------------------------------------------------------------

def main(argv):
    committed = COMMITTED_DEFAULT
    rest = []
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--self-test":
            return self_test()
        if arg == "--committed":
            i += 1
            if i >= len(argv):
                print("check-results.py: --committed needs a directory", file=sys.stderr)
                return 2
            committed = Path(argv[i])
        else:
            rest.append(arg)
        i += 1
    if len(rest) != 1:
        print("usage: python3 -I scripts/privacy/check-results.py OUT_DIR [--committed DIR]   "
              "(or: --self-test)", file=sys.stderr)
        return 2
    out_dir = Path(rest[0])
    out_files, committed_files = load_files(out_dir), load_files(committed)
    res, res_committed = parse_results(out_files["results.json"]), parse_results(committed_files["results.json"])
    if out_dir.exists() and out_dir.resolve() == Path(committed).resolve():
        print("note: OUT_DIR is the committed directory itself, so the byte-for-byte check compares each")
        print("      file with itself; run the suite into a fresh directory to compare a run with the")
        print("      published tables.")
    checks = all_checks(res, out_files, committed_files, res_committed, committed)
    return 1 if report(checks) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
