#!/usr/bin/env python3
"""The U.08 cases' mutants (e2e/runs/cli-u8/README.md): each case must fail when its refusal, its message, its
no-naming or its nothing-sent check is broken.

    python3 -I e2e/runs/cli-u8/mutants.py tool OUTDIR      # cargo test --test e2e, in cli/, per mutant
    python3 -I e2e/runs/cli-u8/mutants.py miniapp OUTDIR   # modules/ferrata_bridge_ui/scripts/box-test.sh, per mutant
    python3 -I e2e/runs/cli-u8/mutants.py check            # each mutant applied and restored, nothing built

Needs what cli/tests/e2e.sh and box-test.sh need (anvil and forge, vault/lib, RISC0_HOME for LEZ's guests; for the
mini-app Qt 6's qmltestrunner). Each mutant edits the checkout and restores every file afterwards, also on error; the
mini-app's last run (F0) is the unmutated one, so results/ holds it. A summary is written to OUTDIR/summary-<mode>.txt.
"""
import os
import pathlib
import subprocess
import sys
import time

W = pathlib.Path(__file__).resolve().parents[3]
BASE = "b5147257"  # the branch before review-u8's fixes: its format.js is mutant Q5

CLAIM = "cli/wallet/src/claim.rs"
CODES = "cli/src/codes.rs"
LEZ = "cli/src/lez.rs"
FLOW = "cli/src/flow.rs"
ETH = "cli/src/eth.rs"
E2E = "cli/tests/e2e.rs"
FMT = "modules/ferrata_bridge_ui/qml/js/format.js"
VIEW = "modules/ferrata_bridge_ui/qml/Main.qml"

ROUND1_NAMES_NOTHING = '''    let t = text.to_lowercase().replace("leaf index", "leaf").replace("leaf_index", "leaf");
    let leaf_named = t
        .match_indices("leaf")
        .any(|(k, _)| t[k + 4..].trim_start_matches([' ', ':', '=', '#']).starts_with(|c: char| c.is_ascii_digit()));
    !leaf_named && !text.contains("tag counter") && !text.contains("j = ")
'''

# An edit is (file, old, new): `old` once in the file, replaced by `new`; or (file, ("SPAN", start, end), new): the text
# from `start` through `end` (both included) replaced; or (file, None, "git:<rev>"): the file as it was at <rev>.
T1 = (CLAIM, "        None => Err(Refusal::program(\n            if imported.is_empty()",
      "        None if true => Ok(leaf_index + 1),\n        None => Err(Refusal::program(\n            if imported.is_empty()")
T4 = (CLAIM, "    if let Err(code) = bw::verify_claim(inputs.root, &witness, &range) {",
      "    if let Err(code) = Ok::<(), u8>(()) {")
T5 = (CLAIM, "    if !private && owned.kind != DestKind::Public {", "    if false && !private && owned.kind != DestKind::Public {")
N1 = [(CODES, "pub const VAULT_ERRORS: [(&str, &str); 41] = [", "pub const VAULT_ERRORS: [(&str, &str); 40] = ["),
      (CODES, ("SPAN", "    // The RISC Zero verifier's error (IRiscZeroVerifier)", "or ask the relayer for a new proof\"),\n"), "")]

TOOL = [
    ("T1 refusal removed: pick_import takes no import as covering", "the 245 case", [T1]),
    ("T2 message removed: what to do (wait for the next root import)", "the 245 case",
     [(CLAIM, "no imported root covers this leaf yet; wait for the next root import", "no imported root covers this leaf yet")]),
    ("T3 the refusal names the leaf (covers leaf 0)", "the 245 case, by names_none_of",
     [(CLAIM, "            \"checked before sending: no imported root covers this leaf yet; wait for the next root import\",",
       "            format!(\"checked before sending: no imported root covers leaf {leaf_index} yet; wait for the next root import\"),")]),
    ("T4 refusal removed: verify_claim not run before sending", "the 249 case: sent (its state check)", [T4]),
    ("T5 refusal removed: the plaintext path of a private-kind deposit not checked before sending",
     "the 252 case: sent (its state check)", [T5]),
    ("T6 message removed: 252's what to do", "the 252 case",
     [(CODES, "; the deposit stays claimable: claim a private-kind deposit privately (the default, without --plaintext), as its destination account", "")]),
    ("T7 message removed: the inbound depth not named", "the depth case", [(LEZ, "    Ok(if slots == 0 {", "    Ok(if slots == 0 || true {")]),
    ("T8 message removed: BurnRootNotAccepted's what to do", "the release before its checkpoint finalises",
     [(FLOW, "no finalized checkpoint covers this burn yet; wait for the next checkpoint", "no finalized checkpoint covers this burn yet")]),
    ("N1 VerificationFailed not mapped (the tool before review-u8 M1)", "the release whose proof the verifier rejects", N1),
    ("N2 the round-1 names_nothing (no leaf by number only)", "names_nothing_catches_every_form",
     [(E2E, ("SPAN", "    let mut rest = text.to_string();\n", "    !numbered && !hash_number && !text.contains(\"tag counter\") && !text.contains(\"j = \")\n"),
       ROUND1_NAMES_NOTHING)]),
    ("N3 a freeze refusal names a leaf by number (review-u8 R4)", "the frozen claim's refusal",
     [(LEZ, "                    \"checked before sending: {what} {scope} are frozen until",
       "                    \"checked before sending (leaf number 3): {what} {scope} are frozen until")]),
    ("N4 message removed: TokenNotRegistered's what to do", "the unsupported token case",
     [(CODES, ": only registered tokens are bridged (`ferrata status --token <address>` shows whether a token is, and its denominations)", "")]),
]

MINIAPP = [
    ("Q1 messages removed: the U.08 entries of format.js", "test_00, 06b, 09, 09b-09f, 10b, 11c, 12", [
        (FMT, "    [/ALREADY_CLAIMED|code 250 /,", "    [/^never-250$/,"),
        (FMT, "    [/FROZEN|frozen/i,", "    [/^never-frozen$/,"),
        (FMT, "    [/\\bCAPPED\\b|code 255 |CapExceeded|cap exceeded/i,", "    [/^never-cap$/,"),
        (FMT, "    [/BAD_MESSAGE|code 240 /,", "    [/^never-240$/,"),
        (FMT, "    [/InvalidDenominationIndex|not a denomination|the amount is not one of the token's denominations/,", "    [/^never-amount$/,"),
        (FMT, "    [/TokenNotRegistered|no token\\(cid, token\\) account|the token is not registered|unsupported token/i,", "    [/^never-token$/,"),
        (FMT, "    [/NOT_REGISTERED|code 251 /,", "    [/^never-251$/,"),
        (FMT, "    [/LEAF_NOT_IMPORTED|code 245 |code 248 |no imported root/,", "    [/^never-final$/,"),
        (FMT, "    [/BurnRootNotAccepted|no accepted burn root|wait for the next checkpoint/,", "    [/^never-checkpoint$/,"),
        (FMT, "    [/CLAIM_INVALID|code 249 /,", "    [/^never-249$/,"),
        (FMT, "    [/WRONG_DESTINATION|code 252 /,", "    [/^never-252$/,"),
        (FMT, "    [/VerificationFailed|0x439cc0cd/,", "    [/^never-verification$/,"),
    ]),
    ("Q2 refusals removed before sending: pick_import, verify_claim, the 252 check, the registry check",
     "test_06b, 09b, 09e, 09f", [
        T1, T4, T5,
        (ETH, "    ensure!(\n        cfg.registered,\n        crate::codes::vault_refusal(&crate::codes::selector(\"TokenNotRegistered\")",
         "    ensure!(\n        cfg.registered || true,\n        crate::codes::vault_refusal(&crate::codes::selector(\"TokenNotRegistered\")"),
    ]),
    ("Q3 the view refreshes the registry after a failed admin step (ffcf0da3, review-u8 L3)", "test_12 (the banner)",
     [(VIEW, "                                if (ok) refreshRegistry();", "                                refreshRegistry();")]),
    ("Q4 VerificationFailed not mapped in the devnet's tool library (N1)", "test_11c (the code shown)", N1),
    ("Q5 format.js as before review-u8 (b5147257)", "test_00, 11c, 12", [(FMT, None, "git:" + BASE)]),
    ("F0 no mutant: the recorded run", "nothing: all pass", []),
]


def apply(edits):
    saved = {}
    for f, old, new in edits:
        p = W / f
        saved.setdefault(f, p.read_text())
        cur = p.read_text()
        if old is None:
            # The file at another commit: from git, or (a checkout without its history, such as a build box's copy)
            # from U8_BASE_DIR/<file>.
            rev = new.split(":", 1)[1]
            shown = subprocess.run(["git", "show", f"{rev}:{f}"], cwd=W, capture_output=True, text=True)
            text = shown.stdout if shown.returncode == 0 else (pathlib.Path(os.environ["U8_BASE_DIR"]) / f).read_text()
            p.write_text(text)
            continue
        if isinstance(old, tuple):
            _, start, end = old
            i = cur.index(start)
            j = cur.index(end, i) + len(end)
            p.write_text(cur[:i] + new + cur[j:])
            continue
        assert cur.count(old) == 1, f"{f}: {old!r} found {cur.count(old)} times"
        p.write_text(cur.replace(old, new))
    return saved


def restore(saved):
    for f, text in saved.items():
        (W / f).write_text(text)


def environment():
    env = dict(os.environ)
    out = subprocess.run(["python3", "-I", str(W / "scripts/fetch_native.py"), "env"], capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        line = line.strip().removeprefix("export ")
        if "=" in line:
            k, v = line.split("=", 1)
            env[k] = v.strip().strip("'\"")
    env.update(RISC0_DEV_MODE="1", FERRATA_E2E="1", PATH=os.path.expanduser("~/.foundry/bin") + ":" + env["PATH"])
    return env


def main():
    if sys.argv[1] == "check":
        for name, _, edits in TOOL + MINIAPP:
            restore(apply(edits))
            print("applies and restores:", name)
        return
    mode, outdir = sys.argv[1], pathlib.Path(sys.argv[2])
    outdir.mkdir(parents=True, exist_ok=True)
    env = environment()
    lines = []
    for name, must_break, edits in (TOOL if mode == "tool" else MINIAPP):
        tag = name.split()[0]
        t0 = time.time()
        saved = apply(edits)
        try:
            if mode == "tool":
                r = subprocess.run(["cargo", "test", "--locked", "--test", "e2e", "--", "--nocapture"], cwd=W / "cli",
                                   env=env, capture_output=True, text=True, timeout=3000)
                log = r.stdout + r.stderr
            else:
                r = subprocess.run(["bash", "modules/ferrata_bridge_ui/scripts/box-test.sh"], cwd=W, env=env,
                                   capture_output=True, text=True, timeout=3000)
                q = W / "modules/ferrata_bridge_ui/results/qmltest.txt"
                log = r.stdout + r.stderr + "\n---- qmltest.txt\n" + (q.read_text() if q.exists() else "(none)")
        finally:
            restore(saved)
        (outdir / f"{tag}.log").write_text(log)
        if mode == "tool":
            failed = [l.split()[1] for l in log.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
            errors = [l for l in log.splitlines() if l.startswith("Error:") or "panicked at" in l]
            if "error[E" in log or "could not compile" in log:
                verdict = "DID NOT COMPILE"
            else:
                verdict = f"exit {r.returncode}; failed: {', '.join(failed) or 'none'}; {errors[0][:260] if errors else ''}"
        else:
            fails = sorted({l.split("::")[2].split("(")[0] for l in log.splitlines() if l.startswith("FAIL!")})
            totals = [l for l in log.splitlines() if l.startswith("Totals")]
            built = "error[E" not in log and "could not compile" not in log
            verdict = f"exit {r.returncode}; {totals[-1] if totals else 'no totals'}; failed: {', '.join(fails) or 'none'}" + ("" if built else "; DID NOT COMPILE")
        line = f"{name} (must break: {must_break}) -> {verdict} ({time.time() - t0:.0f} s)"
        print(line, flush=True)
        lines.append(line)
    (outdir / f"summary-{mode}.txt").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
