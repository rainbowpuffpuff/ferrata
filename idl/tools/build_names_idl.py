#!/usr/bin/env python3
"""Writes idl/ferrata_names.idl.json (the names program v2, lez/names-wire) in SPEL's IDL format, as the other IDLs here.
The error list is read from the Rust source. Standard library only: python3 -I idl/tools/build_names_idl.py"""
import json, re, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
SRC = (ROOT / "lez/names-wire/src/lib.rs").read_text()

ACC = {"array": ["u8", 32]}
B32 = {"array": ["u8", 32]}
def opt(t): return {"option": t}
def f(name, t): return {"name": name, "type": t}
def acct(name, writable, signer, docs, pda=None, init=False):
    a = {"name": name, "writable": writable, "signer": signer, "init": init, "visibility": ["public"], "docs": [docs]}
    if pda: a["pda"] = {"seeds": pda}
    return a

USER = acct("user", True, True, "user(signer) = Actor(signer, program): the signer's own actor of this program; the root message, signed")
NAME_SEEDS = [{"kind": "const", "value": "ferrata-names/v2/name/"}, {"kind": "arg", "path": "skeleton(name)"}]
def NAME(doc, n="name"):
    return acct(n, True, False, "name(n): the public PDA of the program with seed sha256(\"ferrata-names/v2/name/\" || skeleton(n)); " + doc, pda=NAME_SEEDS)
SEQUENCE = acct("sequence", True, False, "sequence: the public PDA of the program with seed sha256(\"ferrata-names/v2/sequence\"); PDA evidence while it holds no state",
                pda=[{"kind": "const", "value": "ferrata-names/v2/sequence"}])
def other(n, doc): return acct(n, True, False, doc)
RECEIVER = opt({"defined": "PrivateReceiver"})

ins = [
    ("register", "Register", [f("name", "string"), f("now_ms", "u64"), f("target", opt(ACC)), f("receiver", RECEIVER), f("salt", B32)],
     [USER, NAME("PDA evidence while it holds no record")],
     "The reveal, signed. The signer's pending commitment must equal sha256(\"ferrata-names/v2/commit/\" || signer || salt || name), now_ms >= its not_before_ms and now_ms + 120,000 <= its expires_ms; the commitment is spent and `user` sends Claim to name(n). Valid only in blocks within 120 s of now_ms. Emits Registered, or Displaced when it takes the name from a younger commitment."),
    ("renew", "Renew", [f("name", "string"), f("now_ms", "u64")], [USER, NAME("holds the record")],
     "Owner: extend to now_ms + 180 days; refused (LAPSED) unless now_ms + 600,000 <= expires_ms + 30 days. A renewal that may land after expiry drops the offer. Emits Renewed."),
    ("transfer", "Transfer", [f("name", "string"), f("to", ACC), f("now_ms", "u64")], [USER, NAME("holds the record")],
     "Owner, now_ms + 600,000 <= expires_ms: offer the name to `to`, who takes it with accept; `to` = the owner withdraws the offer. Emits Offered."),
    ("set_target", "SetTarget", [f("name", "string"), f("target", opt(ACC)), f("receiver", RECEIVER), f("now_ms", "u64")],
     [USER, NAME("holds the record")], "Owner, now_ms + 600,000 <= expires_ms: change what the name resolves to. Emits Updated."),
    ("release", "Release", [f("name", "string")], [USER, NAME("holds the record; emptied")], "Owner, any time: free the name; it leaves the owner's list. Emits Released."),
    ("set_primary", "SetPrimary", [f("name", opt("string")), f("now_ms", "u64")], [USER, NAME("holds the record (absent when clearing)")],
     "Set the signer's primary name (reverse record): the name must point at the signer, now_ms + 600,000 <= expires_ms. None clears it."),
    ("accept", "Accept", [f("name", "string"), f("now_ms", "u64")], [USER, NAME("holds the record")],
     "Take a name offered to the signer, now_ms + 600,000 <= expires_ms: it then points at the signer, with no private receiver; it joins the signer's list (at most 5). The previous owner's actor is not needed: the name stays in its list until it forgets it. Emits Transferred."),
    ("commit", "Commit", [f("commitment", B32), f("now_ms", "u64")], [USER, SEQUENCE],
     "Step 1 of registering: store the commitment at user(signer), replacing a pending one, numbered by `sequence` (Tick, Sequenced); revealable from now_ms + 240,000 ms (COMMIT_SLACK 120 s + MIN_REVEAL_DELAY 120 s) with the reveal's window ending by now_ms + 3,600,000 ms. Valid only in blocks within 120 s of now_ms."),
    ("forget", "Forget", [f("names", {"vec": "string"})], [USER, NAME("one per name (Disown); PDA evidence for one that holds nothing", n="names")],
     "Drop 1 to 5 distinct names from the signer's list that are no longer the signer's (accepted by another, registered afresh after they lapsed, or taken by an older commitment)."),
    ("credit", "Credit", [f("name", "string")], [other("user", "user(a), from name(name) only")], "Internal (from name(name) only): a now holds the name (added to its list once)."),
    ("debit", "Debit", [f("name", "string")], [other("user", "user(a), from name(name) only")], "Internal (from name(name) only): the name leaves a's list; clears it as primary."),
    ("primary", "Primary", [f("name", "string")], [other("user", "user(a), from name(name) only")], "Internal (from name(name) only): make the name a's primary name."),
    ("claim", "Claim", [f("name", "string"), f("now_ms", "u64"), f("target", opt(ACC)), f("receiver", RECEIVER), f("seq", "u64")],
     [NAME("from user(a) only")],
     "Internal (from user(a) only, after it checked the reveal): register the name for a if the address is free, lapsed, or held by a registration with a larger commit_seq whose final_at (registered_ms + 3,600,000) is at least now_ms + 120,000; else TAKEN (or CONFUSABLE for a look-alike)."),
    ("disown", "Disown", [f("name", "string")], [NAME("from user(a) only")], "Internal (from user(a) only): refused while a owns the name (STILL_HELD), else Debit back to a."),
    ("tick", "Tick", [], [SEQUENCE], "Internal (from user(a) only): add 1 to the counter and send Sequenced back."),
    ("sequenced", "Sequenced", [f("seq", "u64")], [other("user", "user(a), from sequence only")], "Internal (from sequence only): number a's pending commitment."),
]
instructions = [{"name": n, "variant": v, "discriminator": [i], "accounts": a, "args": args,
                 "execution": {"public": True, "private_owned": False}, "docs": [d]} for i, (n, v, args, a, d) in enumerate(ins)]

types = [
    {"name": "PrivateReceiver", "kind": "struct", "fields": [f("npk", B32), f("vpk", {"vec": "u8"})],
     "docs": ["What a sender needs to pay the name's holder privately: LEZ's nullifier public key and ML-KEM-768 viewing public key (1,184 bytes), as LEZ's wallet takes them (--to-npk, --to-vpk)."]},
    {"name": "NameRecord", "kind": "struct", "fields": [f("name", "string"), f("owner", ACC), f("target", opt(ACC)), f("receiver", RECEIVER),
                                                         f("registered_ms", "u64"), f("expires_ms", "u64"), f("offer", opt(ACC)), f("commit_seq", "u64")],
     "docs": ["The state of name(n); empty when free. The name it holds may be a look-alike of n (same skeleton): n resolves only if name == n and block time < expires_ms. Provisional until registered_ms + 3,600,000 (a reveal of a commitment with a smaller number can take it)."]},
    {"name": "PendingCommit", "kind": "struct", "fields": [f("hash", B32), f("seq", "u64"), f("not_before_ms", "u64"), f("expires_ms", "u64")],
     "docs": ["A commitment waiting for its reveal: seq numbers it among all commitments; the reveal needs now_ms >= not_before_ms and now_ms + 120,000 <= expires_ms."]},
    {"name": "UserState", "kind": "struct", "fields": [f("names", {"vec": "string"}), f("primary", opt("string")), f("commit", opt({"defined": "PendingCommit"}))],
     "docs": ["The state of user(a): the names in a's list (at most 5; those that left a without a signing stay until a forgets them), the primary name (counts only while that name is live and points at a), the pending commitment."]},
    {"name": "NameEvent", "kind": "struct", "fields": [f("name", "string"), f("owner", opt(ACC)), f("target", opt(ACC)), f("expires_ms", "u64")],
     "docs": ["ProgramEvent data of every change; selector = sha256(\"ferrata_names::<kind>\")[..8], kind one of Registered, Displaced, Renewed, Offered, Transferred, Updated, Released."]},
]

errors, doc = [], []
for line in SRC[SRC.index("pub mod code {"):].splitlines():
    line = line.strip()
    if line.startswith("///"):
        doc.append(line[3:].strip())
    elif m := re.fullmatch(r"([A-Z_]+) = (\d+);", line):
        errors.append({"code": int(m.group(2)), "name": m.group(1), "msg": " ".join(doc)})
        doc = []
    elif line.startswith("}") and errors:
        break
assert len(errors) == 22, (len(errors), errors)

idl = {
    "version": "0.2.0", "name": "ferrata_names",
    "spec": "logos-co/spel c6f73ce (spel-framework-core 0.7.0, docs/reference/idl.md)",
    "metadata": {"name": "ferrata_names", "version": "0.2.0"},
    "instruction_type": "ferrata_lez_names_wire::NamesMessage",
    "docs": [
        "Human-readable names for LEZ accounts, for the LEZ v0.4 test network (EXPERIMENTAL, UNAUDITED); program v2. Hand-written IDL in SPEL's format, as ferrata_bridge.idl.json.",
        "Each instruction is a variant of the borsh enum named by instruction_type; discriminator is its one-byte variant index, followed by args in order. Instructions 0-8 are signed by an account (root at user(signer)); 9-15 are the program's own messages between its actors.",
        "skeleton(n): replace every '0' by 'o' and every '1' by 'l', then every 'rn' by 'm', then every 'vv' by 'w' (left to right, no overlaps). Names with one skeleton share name(n): one of them at a time.",
        "Names: 3-32 bytes of a-z, 0-9 and -, no leading or trailing -, no --; not reserved: no '-'-separated part whose skeleton, as it is or without its leading and trailing digits, is a reserved name's skeleton. At most 5 per account; 180-day term, 30-day grace; no fee on the test network.",
        "Registration is commit and reveal: commit (numbered by sequence) stores sha256(\"ferrata-names/v2/commit/\" || signer || salt || name); register reveals it from 4 minutes after the commit's now_ms, its whole window within the hour after it. The oldest commitment wins: a reveal of an older commitment takes a registration made in the hour before (Displaced).",
        "Time: now_ms in a message sets the transaction's timestamp validity window to [now_ms, now_ms + 600000) ([now_ms, now_ms + 120000) for commit and register); a deadline must hold for the whole window.",
        "Refusals exit with one byte, 230-236 and 240-254 (below); the bridge program also uses 240-255, so a code is read with the program that returned it.",
    ],
    "instructions": instructions,
    "accounts": [{"name": "NameRecord", "type": {"defined": "NameRecord"}}, {"name": "UserState", "type": {"defined": "UserState"}},
                 {"name": "Sequence", "type": "u64"}],
    "types": types,
    "errors": errors,
}
out = ROOT / "idl/ferrata_names.idl.json"
out.write_text(json.dumps(idl, indent=2, ensure_ascii=False) + "\n")
print(f"wrote {out.relative_to(ROOT)}: {len(instructions)} instructions, {len(types)} types, {len(errors)} errors")
