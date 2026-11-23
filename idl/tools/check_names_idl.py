#!/usr/bin/env python3
"""Checks idl/ferrata_names.idl.json (program v2) against the program's own encodings, with the IDL alone:
lez/results/names-vectors.json (written by lez/harness/tests/names.rs from the Rust types) has every message, a record,
a user state, the commitment, the counter's and the names' addresses. Each message decodes through the instruction
table to its last byte (and one byte short is refused); the record and the user state through their types; every
name address and the counter's are recomputed from what the IDL's docs define (the skeleton, the seeds, LEZ's PDA
rule); the IDL's errors equal the program's codes. Standard library only: python3 -I idl/tools/check_names_idl.py
(exit 0 when every check passes)."""
import hashlib, json, pathlib, re, struct, sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
IDL = json.loads((ROOT / "idl/ferrata_names.idl.json").read_text())
V = json.loads((ROOT / "lez/results/names-vectors.json").read_text())
TYPES = {t["name"]: t for t in IDL["types"]}

def dec(t, b, o):
    if t in ("u8", "u64"):
        n = 1 if t == "u8" else 8
        if o + n > len(b): raise ValueError("truncated")
        return int.from_bytes(b[o:o + n], "little"), o + n
    if t == "string":
        n = struct.unpack_from("<I", b, o)[0]; o += 4
        if o + n > len(b): raise ValueError("truncated")
        return b[o:o + n].decode("utf-8"), o + n
    if "array" in t:
        inner, n = t["array"]; assert inner == "u8"
        if o + n > len(b): raise ValueError("truncated")
        return b[o:o + n].hex(), o + n
    if "vec" in t:
        n = struct.unpack_from("<I", b, o)[0]; o += 4
        if t["vec"] == "u8":
            if o + n > len(b): raise ValueError("truncated")
            return b[o:o + n].hex(), o + n
        out = []
        for _ in range(n):
            v, o = dec(t["vec"], b, o)
            out.append(v)
        return out, o
    if "option" in t:
        if o >= len(b): raise ValueError("truncated")
        tag = b[o]; o += 1
        if tag == 0: return None, o
        if tag != 1: raise ValueError("bad option tag")
        return dec(t["option"], b, o)
    if "defined" in t:
        out = {}
        for fld in TYPES[t["defined"]]["fields"]:
            out[fld["name"]], o = dec(fld["type"], b, o)
        return out, o
    raise ValueError(t)

def whole(t, b):
    v, o = dec(t, b, 0)
    if o != len(b): raise ValueError(f"{len(b) - o} trailing bytes")
    return v

fails = 0
def check(ok, what):
    global fails
    print(("ok  " if ok else "FAIL") + " " + what)
    fails += 0 if ok else 1

by_disc = {i["discriminator"][0]: i for i in IDL["instructions"]}
def decode_message(b):
    ins = by_disc[b[0]]
    args, o = {}, 1
    for a in ins["args"]:
        args[a["name"]], o = dec(a["type"], b, o)
    return ins, args, o

kinds = ["Register", "Register", "Renew", "Transfer", "SetTarget", "Release", "SetPrimary", "SetPrimary", "Accept", "Commit",
         "Forget", "Credit", "Debit", "Primary", "Claim", "Disown", "Tick", "Sequenced"]
check(len(V["messages"]) == len(kinds), f"{len(kinds)} message vectors")
for hexmsg, kind in zip(V["messages"], kinds):
    b = bytes.fromhex(hexmsg)
    ins, args, o = decode_message(b)
    check(ins["variant"] == kind and o == len(b), f"message {kind}: {len(b)} bytes decode through `{ins['name']}` with no byte left")
    check(args.get("now_ms", V["t0"]) == V["t0"], f"message {kind}: now_ms = t0")
    try:
        o2 = 1
        for a in ins["args"]:
            _, o2 = dec(a["type"], b[:-1], o2)
        short_refused = o2 != len(b) - 1
    except (ValueError, struct.error, IndexError):
        short_refused = True
    check(short_refused, f"message {kind}: one byte short is refused")
check(decode_message(bytes.fromhex(V["messages"][10]))[1]["names"] == ["alice", "bob"], "Forget's names decode")

rec = whole({"defined": "NameRecord"}, bytes.fromhex(V["record"]))
check(rec["name"] == "alice" and rec["owner"] == V["account"] and rec["target"] == V["account"] and rec["offer"] == V["account"]
      and len(rec["receiver"]["vpk"]) == 2 * 1184 and rec["commit_seq"] == 7, "NameRecord decodes (owner, target, 1,184-byte vpk, offer, commit_seq)")
user = whole({"defined": "UserState"}, bytes.fromhex(V["user"]))
check(user["names"] == ["alice", "bob"] and user["primary"] == "alice" and user["commit"]["hash"] == V["commitment"]
      and user["commit"]["seq"] == 7 and user["commit"]["not_before_ms"] == V["t0"] + 240000, "UserState decodes (names, primary, pending commitment)")

# Addresses from what the IDL's docs define.
docs = " ".join(IDL["docs"])
check("'0' by 'o'" in docs and "'1' by 'l'" in docs and "'rn' by 'm'" in docs and "'vv' by 'w'" in docs, "the IDL defines the skeleton")
def skeleton(n):
    return n.replace("0", "o").replace("1", "l").replace("rn", "m").replace("vv", "w")
PDA_PREFIX = b"/LEE/v0.2/AccountId/PDA/" + bytes(8)  # LEZ v0.4's AccountId::for_public_pda
def pda(program, seed):
    return hashlib.sha256(PDA_PREFIX + bytes.fromhex(program) + seed).hexdigest()
name_const = next(s["value"] for s in by_disc[0]["accounts"][1]["pda"]["seeds"] if s["kind"] == "const")
for v in V["names"]:
    seed = hashlib.sha256(name_const.encode() + skeleton(v["name"]).encode()).digest()
    check(seed.hex() == v["seed"] and pda(v["program"], seed) == v["account"] and skeleton(v["name"]) == v["skeleton"],
          f"name({v['name']}) ({v['programLabel']} program) from the IDL's seeds")
seq_const = next(a for a in by_disc[7]["accounts"] if a["name"] == "sequence")["pda"]["seeds"][0]["value"]
seq_seed = hashlib.sha256(seq_const.encode()).digest()
check(seq_seed.hex() == V["sequence"]["seed"] and pda(V["sequence"]["program"], seq_seed) == V["sequence"]["account"], "sequence from the IDL's seed")
check(hashlib.sha256(b"ferrata-names/v2/commit/" + bytes.fromhex(V["account"]) + bytes.fromhex(V["salt"]) + b"alice").hexdigest()
      == V["commitment"] and "ferrata-names/v2/commit/" in json.dumps(IDL), "the commitment as the IDL defines it")

src = (ROOT / "lez/names-wire/src/lib.rs").read_text()
rust = {n: int(v) for n, v in re.findall(r"\n\s+([A-Z_]+) = (\d+);", src[src.index("pub mod code {"):])}
check(rust == {e["name"]: e["code"] for e in IDL["errors"]}, f"errors = the program's {len(rust)} codes")
check(len(IDL["instructions"]) == 16 and sorted(by_disc) == list(range(16)), "16 instructions, discriminators 0-15")
print("all passed" if not fails else f"{fails} failed")
sys.exit(1 if fails else 0)
