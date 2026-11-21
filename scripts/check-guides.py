#!/usr/bin/env python3
"""Keep the guides in step with the code they describe.

    python3 -I scripts/check-guides.py           check (exit 1 on any finding)
    python3 -I scripts/check-guides.py --write   regenerate the generated blocks, then check

Checks:
  1. docs/guides/consumer-guide.md, between `<!-- codes:begin -->` and `<!-- codes:end -->`: one row per error code
     of the attestation program's IDL (idl/ferrata_attestation.idl.json), with what a client does about it
     (RFP-021 S.4, "handling each typed error code").
  2. docs/guides/user-guide.md, between `<!-- ui-privacy:begin -->` and `<!-- ui-privacy:end -->`: what the `ferrata`
     tool prints before each step (cli/src/privacy.rs, the mini-app's privacy.js table), word for word (RFP-022 U.7).
  3. docs/guides/user-guide.md §11 names every code of the bridge program's IDL (idl/ferrata_bridge.idl.json).
  4. No guide, README or trust document of the documentation set still calls itself a first draft, and neither
     README.md, docs/README.md nor the attestation design note calls a guide one (they may say so of
     docs/performance.md, which is still the first draft of 11 November).
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FINDINGS = []


def finding(msg):
    FINDINGS.append(msg)


# ---- 4. no first drafts left -----------------------------------------------------------------------------------------

# The documentation set (besides docs/guides/*.md): none may call itself a first draft, except to say what it replaces.
DOCS = ("docs/trust.md", "lez/ATTEST-CLI.md", "cli/README.md", "monitor/README.md", "lez/updater/README.md")
# Indexes and notes that point at the guides: they may say "first draft" only of the performance document, named.
POINTERS = ("README.md", "docs/README.md", "docs/design/attestation.md")


def first_draft_findings(texts):
    """`texts`: path (relative to the repository) -> content. One finding per "first draft" left."""
    out = []
    for path, raw in texts.items():
        text = re.sub(r"\s+", " ", raw)
        for m in re.finditer(r"first draft", text, re.I):
            before = text[max(0, m.start() - 30) : m.start()]
            if path in POINTERS:
                # Of the performance document by name, just before: "(`docs/performance.md` is still the first draft".
                if "performance.md" in text[max(0, m.start() - 50) : m.start()]:
                    continue
            elif re.search(r"(replaces|since|of) the $", before, re.I):
                continue
            out.append(f"{path}: still says '...{before}{m.group(0)}'")
    return out


# ---- 1. every error code of the attestation program, with what a client does --------------------------------------

def advice(code, name):
    """(who meets the code, what the client should do)."""
    upd = "an updater (a record exists only once these pass)"
    if code in (10, 11, 12):
        return "the deployer, at `Init`", "use the bootstrap of the checkpoint you checked; the instance is not created"
    if code == 52:
        return upd, "nothing: the update is applied already (duplicate or stale)"
    if code == 53:
        return upd, "the instance is past its weak-subjectivity bound: move to a replacement (§9)"
    if code == 60:
        return upd, "send the period update that supplies the next committee first"
    if code < 80:
        return upd, "refused update: take it from another source (the updater demotes the one that served it)"
    if 80 <= code < 100:
        if code in (81, 82):
            return "an updater (job step)", "another updater moved the job or the store: re-read the job and send its due step"
        return "an updater (job step)", "send the job's due step with the right witness, or open a new job"
    if code in (100, 101, 131, 200, 214):
        return "a consumer's client", "fix the request: it does not decode or asks for something the program cannot serve"
    if 110 <= code <= 113:
        return "a consumer's client", "refetch the proof at the record's block number; another block's proof never verifies"
    if code in (120, 121, 215):
        return "a consumer's client", "send the RLP header of the record's own block (a Gloas record binds the hash only)"
    if 122 <= code <= 125:
        return "a consumer's client", "this fact cannot be attested: a missing, failed or pre-Byzantium receipt, or no such log"
    if code == 130:
        return "a consumer", "refuse the action (rule 4): the condition does not hold"
    if code in (140, 211):
        return "a consumer", "configuration error: the request names another chain or instance; do not retry"
    if code == 141:
        return "a consumer's client", "use a record whose finalized header carries the execution binding"
    if 150 <= code <= 152:
        return "a consumer's client", "hop from a nearer record, or chain hops (§4.4)"
    if 160 <= code <= 163:
        return "a consumer's client", "rebuild the accumulator path against the attested root"
    if code == 201:
        return "any sender", "send the message to the actor it is for (derive it: §2)"
    if code == 202:
        return "a forged internal message", "nothing: only the program's own actors may send it"
    if code == 203:
        return "the deployer", "the instance exists: use it, or initialise a new one from another checkpoint"
    if code == 204:
        return "any sender", "no instance there: check the instance id you derived"
    if code in (205, 207):
        return "an updater (job step)", "the job is finished, or open already: re-read it and send its due step"
    if code == 206:
        return "an updater", "wait until the initial committee is adopted"
    if code == 208:
        return "nobody (a write-once record)", "report it: a record never holds two headers"
    if code == 209:
        return "a consumer's client", "send the request to a record `header(id, slot)`, not another actor"
    if code == 210:
        return "a consumer's client", "a read needs a sender to reply to: send it from your program"
    if code == 212:
        return "the deployer", "use a store layout the program knows"
    if code == 213:
        return "a consumer's client", "no record there yet: wait for the updater, or use the newest record"
    if code in (216, 217):
        return "whoever sends `Succeed`", "a successor copies the parent's immutable fields and starts from its latest record"
    return "?", "?"


def first_sentence(msg):
    msg = msg.replace("|", "\\|").strip()
    cut = re.search(r"(?<=[a-z0-9`)\]])\. ", msg)
    return msg[: cut.start() + 1] if cut else msg


def codes_block():
    idl = json.loads((ROOT / "idl/ferrata_attestation.idl.json").read_text())
    rows = ["| Code | Name | Meaning (the IDL's) | Met by | The client should |", "|---:|---|---|---|---|"]
    for e in sorted(idl["errors"], key=lambda e: e["code"]):
        who, todo = advice(e["code"], e["name"])
        if who == "?":
            finding(f"consumer guide: no advice for code {e['code']} {e['name']}: add it to scripts/check-guides.py")
        rows.append(f"| {e['code']} | `{e['name']}` | {first_sentence(e['msg'])} | {who} | {todo} |")
    return "\n".join(rows)


# ---- 2. what the tool prints before each step ----------------------------------------------------------------------

def ui_block():
    src = (ROOT / "cli/src/privacy.rs").read_text()
    out = ["| Step, as the tool titles it | Public on Ethereum | Public on LEZ | Kept private |", "|---|---|---|---|"]
    for m in re.finditer(r"pub const [A-Z_]+: Step = Step \{(.*?)\n\};", src, re.S):
        body = m.group(1)
        title = re.search(r'title: "([^"]*)"', body).group(1)

        def field(name):
            f = re.search(name + r": &\[(.*?)\]", body, re.S)
            items = re.findall(r'"((?:[^"\\]|\\.)*)"', f.group(1)) if f else []
            return "; ".join(items) if items else "nothing"

        out.append(f"| {title} | {field('public_eth')} | {field('public_lez')} | {field('hidden')} |")
    if len(out) < 7:
        finding("user guide: fewer steps than expected in cli/src/privacy.rs")
    return "\n".join(out)


# ---- generated blocks ----------------------------------------------------------------------------------------------

def sync_block(path, tag, block, write):
    p = ROOT / path
    text = p.read_text()
    begin, end = f"<!-- {tag}:begin -->", f"<!-- {tag}:end -->"
    if begin not in text or end not in text:
        finding(f"{path}: no {begin} ... {end} block")
        return
    a, b = text.index(begin) + len(begin), text.index(end)
    want = "\n" + block + "\n"
    if text[a:b] != want:
        if write:
            p.write_text(text[:a] + want + text[b:])
            print(f"{path}: {tag} block written")
        else:
            finding(f"{path}: the {tag} block differs from the code: run scripts/check-guides.py --write")


def main():
    write = "--write" in sys.argv[1:]
    sync_block("docs/guides/consumer-guide.md", "codes", codes_block(), write)
    sync_block("docs/guides/user-guide.md", "ui-privacy", ui_block(), write)

    # 3. the bridge's codes in the user guide's §11
    bridge = json.loads((ROOT / "idl/ferrata_bridge.idl.json").read_text())
    guide = (ROOT / "docs/guides/user-guide.md").read_text()
    m = re.search(r"\n## 11\..*?(?=\n## )", guide, re.S)
    sec = m.group(0) if m else ""
    for e in bridge["errors"]:
        if not re.search(rf"\b{e['code']}\b", sec) or e["name"] not in sec:
            finding(f"user guide §11: bridge code {e['code']} {e['name']} is not named")

    # 4. no first drafts left
    docs = sorted((ROOT / "docs/guides").glob("*.md")) + [ROOT / p for p in DOCS]
    texts = {str(d.relative_to(ROOT)): d.read_text() for d in docs + [ROOT / p for p in POINTERS]}
    for f in first_draft_findings(texts):
        finding(f)

    for f in FINDINGS:
        print("FINDING", f)
    print(f"check-guides: {len(FINDINGS)} finding(s)")
    return 1 if FINDINGS else 0


if __name__ == "__main__":
    sys.exit(main())
