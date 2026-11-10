#!/usr/bin/env python3
"""Independent check of vectors/*.json: recomputes every output from its inputs with the Python
standard library (hashlib, hmac) and the layouts written in the README, sharing no code with the
Rust crate. Run: python3 -I scripts/check_vectors_v1.py [vectors-dir]  (v1 encodings, superseded; kept to check vectors/v1)"""

import hashlib
import hmac
import json
import pathlib
import sys

checks = 0


def b(x):
    return bytes.fromhex(x[2:] if x.startswith("0x") else x)


def sha(data):
    return hashlib.sha256(data).digest()


def prf(k, msg):
    return hmac.new(k, msg, hashlib.sha256).digest()


def u64(n):
    return n.to_bytes(8, "big")


def eq(label, got, want):
    global checks
    checks += 1
    if got != want:
        sys.exit(f"MISMATCH {label}: computed {got.hex()} file {want.hex()}")


LEAF_TAG = sha(b"ferrata/vault/leaf/v1")
BURN_LEAF_TAG = sha(b"ferrata/burn/leaf/v1")
RELEASE_DOMAIN = sha(b"ferrata/vault/release/v1")


def leaf(tag, index, token, amount, commitment):
    return sha(b"\x00" + tag + u64(index) + token + amount + commitment)


def node(left, right):
    return sha(b"\x01" + left + right)


def zero_hashes():
    z = [bytes(32)]
    for _ in range(32):
        z.append(node(z[-1], z[-1]))
    return z


def root_of(leaves):
    z = zero_hashes()
    level = list(leaves)
    for h in range(32):
        level = [node(level[i], level[i + 1] if i + 1 < len(level) else z[h]) for i in range(0, len(level), 2)]
    return level[0] if level else z[32]


def root_from_path(lf, index, path):
    assert 0 <= index < 2**32
    for h, sib in enumerate(path):
        lf = node(lf, sib) if (index >> h) & 1 == 0 else node(sib, lf)
    return lf


def word(n):
    return n.to_bytes(32, "big")


def addr_word(a):
    return bytes(12) + a


def check_tags(v):
    for t in v["hashed"]:
        eq("tag " + t["tag"], sha(t["tag"].encode()), b(t["sha256"]))
    for t in v["prefix"]:
        eq("prefix " + t["tag"], t["tag"].encode(), b(t["bytes"]))


def check_tree(v, tag):
    z = zero_hashes()
    for h, zh in enumerate(v["zero_hashes"]):
        eq(f"zero[{h}]", z[h], b(zh))
    eq("empty root", z[32], b(v["empty_root"]))
    leaves = []
    for e in v["leaves"]:
        pre = b"\x00" + tag + u64(e["leaf_index"]) + b(e["token"]) + b(e["amount"]) + b(e["commitment"])
        eq("leaf preimage", pre, b(e["preimage"]))
        eq("leaf", sha(pre), b(e["leaf"]))
        leaves.append(sha(pre))
        eq("root after", root_of(leaves), b(e["root_after"]))
    eq("final root", root_of(leaves), b(v["final_root"]))
    for p in v["paths"]:
        eq("path", root_from_path(leaves[p["leaf_index"]], p["leaf_index"], [b(s) for s in p["siblings"]]), b(v["final_root"]))


def check_inbound(v):
    for c in v["cases"]:
        k, vault, chain = b(c["k"]), b(c["vault"]), c["chain_id"]
        msg = b"ferrata/s/v1" + u64(chain) + vault + b(c["depositor"]) + u64(c["j"])
        eq("s message", msg, b(c["s_message"]))
        s = prf(k, msg)
        eq("s", s, b(c["s"]))
        cpre = b"ferrata/C/v1" + s + b(c["dest"])
        eq("C preimage", cpre, b(c["C_preimage"]))
        eq("C", sha(cpre), b(c["C"]))
        lf = leaf(LEAF_TAG, c["leaf_index"], b(c["token"]), b(c["amount"]), sha(cpre))
        eq("deposit leaf", lf, b(c["leaf"]))
        fpre = b"ferrata/deposit/v1" + u64(chain) + vault + u64(c["leaf_index"]) + lf
        eq("fid preimage", fpre, b(c["fid_preimage"]))
        eq("fid", sha(fpre), b(c["fid"]))
        npre = b"ferrata/nf/v1" + s + sha(fpre)
        eq("N preimage", npre, b(c["N_preimage"]))
        eq("N", sha(npre), b(c["N"]))
    c = v["claim"]
    k, vault, chain = b(c["k"]), b(c["vault"]), c["chain_id"]
    s = prf(k, b"ferrata/s/v1" + u64(chain) + vault + b(c["depositor"]) + u64(c["j"]))
    eq("claim s", s, b(c["s"]))
    lf = leaf(LEAF_TAG, c["leaf_index"], b(c["token"]), b(c["amount"]), sha(b"ferrata/C/v1" + s + b(c["dest"])))
    leaves = [b(x) for x in c["leaves"]]
    eq("claim leaf in tree", leaves[c["leaf_index"]], lf)
    eq("claim root", root_of(leaves), b(c["root"]))
    eq("claim path", root_from_path(lf, c["leaf_index"], [b(p) for p in c["path"]]), b(c["root"]))
    fid = sha(b"ferrata/deposit/v1" + u64(chain) + vault + u64(c["leaf_index"]) + lf)
    eq("claim N", sha(b"ferrata/nf/v1" + s + fid), b(c["N"]))


def check_outbound(v):
    for c in v["cases"]:
        k, config, vault, chain = b(c["k"]), b(c["lez_config_id"]), b(c["vault"]), c["chain_id"]
        rmsg = b"ferrata/burn-tag/v1" + config + u64(c["i"])
        eq("r message", rmsg, b(c["r_message"]))
        r = prf(k, rmsg)
        eq("r", r, b(c["r"]))
        eq("s_out message", b"ferrata/s-out/v1" + r, b(c["s_out_message"]))
        s_out = prf(k, b"ferrata/s-out/v1" + r)
        eq("s_out", s_out, b(c["s_out"]))
        eq("memo message", b"ferrata/memo/v1" + r, b(c["memo_message"]))
        pad = prf(k, b"ferrata/memo/v1" + r)[:20]
        eq("memo pad", pad, b(c["memo_pad"]))
        eq("memo", bytes(x ^ y for x, y in zip(b(c["recipient"]), pad)), b(c["memo"]))
        cpre = b"ferrata/Cout/v1" + s_out + u64(chain) + vault + b(c["recipient"])
        eq("C_out preimage", cpre, b(c["C_out_preimage"]))
        eq("C_out", sha(cpre), b(c["C_out"]))
        lf = leaf(BURN_LEAF_TAG, c["leaf_index"], b(c["token"]), b(c["amount"]), sha(cpre))
        eq("burn leaf", lf, b(c["burn_leaf"]))
        bpre = b"ferrata/burn/v1" + config + u64(c["leaf_index"]) + lf
        eq("burn id preimage", bpre, b(c["burn_id_preimage"]))
        eq("burn id", sha(bpre), b(c["burn_id"]))
        npre = b"ferrata/nf-out/v1" + s_out + sha(bpre)
        eq("N_out preimage", npre, b(c["N_out_preimage"]))
        eq("N_out", sha(npre), b(c["N_out"]))
    c = v["release"]
    k, config, vault, chain = b(c["k"]), b(c["lez_config_id"]), b(c["vault"]), c["chain_id"]
    r = prf(k, b"ferrata/burn-tag/v1" + config + u64(c["i"]))
    eq("release r", r, b(c["r"]))
    s_out = prf(k, b"ferrata/s-out/v1" + r)
    eq("release s_out", s_out, b(c["s_out"]))
    recipient = bytes(x ^ y for x, y in zip(b(c["memo"]), prf(k, b"ferrata/memo/v1" + r)[:20]))
    eq("release recipient from memo", recipient, b(c["recipient"]))
    c_out = sha(b"ferrata/Cout/v1" + s_out + u64(chain) + vault + recipient)
    eq("release C_out", c_out, b(c["C_out"]))
    lf = leaf(BURN_LEAF_TAG, c["leaf_index"], b(c["token"]), b(c["amount"]), c_out)
    leaves = [b(x) for x in c["leaves"]]
    eq("release leaf in tree", leaves[c["leaf_index"]], lf)
    eq("burn root", root_of(leaves), b(c["burn_root"]))
    eq("release path", root_from_path(lf, c["leaf_index"], [b(p) for p in c["path"]]), b(c["burn_root"]))
    n_out = sha(b"ferrata/nf-out/v1" + s_out + sha(b"ferrata/burn/v1" + config + u64(c["leaf_index"]) + lf))
    eq("release N_out", n_out, b(c["N_out"]))
    journal = (RELEASE_DOMAIN + word(chain) + addr_word(vault) + config + b(c["burn_root"]) + n_out
               + addr_word(b(c["token"])) + b(c["amount"]) + addr_word(recipient)
               + addr_word(b(c["fee_recipient"])) + b(c["relay_fee"]))
    eq("release journal", journal, b(c["journal"]))
    eq("release journal sha256", sha(journal), b(c["journal_sha256"]))
    gi = (b(c["burn_root"]) + u64(c["leaf_index"]) + b"".join(b(p) for p in c["path"]) + s_out + u64(chain) + vault
          + config + b(c["token"]) + b(c["amount"]) + recipient + b(c["fee_recipient"]) + b(c["relay_fee"]))
    eq("guest input", gi, b(c["guest_input"]))


def check_journals(v):
    assert v["count"] == len(v["cases"])
    for c in v["cases"]:
        j = (RELEASE_DOMAIN + word(c["chainId"]) + addr_word(b(c["vault"])) + b(c["lezConfigId"]) + b(c["burnRoot"])
             + b(c["nullifier"]) + addr_word(b(c["token"])) + b(c["amount"]) + addr_word(b(c["recipient"]))
             + addr_word(b(c["feeRecipient"])) + b(c["relayFee"]))
        eq("journal", j, b(c["journal"]))
        eq("journal digest", sha(j), b(c["digest"]))


def main():
    d = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "vectors" / "v1")
    load = lambda n: json.loads((d / n).read_text())
    check_tags(load("tags.json"))
    check_tree(load("deposit-tree.json"), LEAF_TAG)
    check_tree(load("burn-tree.json"), BURN_LEAF_TAG)
    check_inbound(load("inbound.json"))
    check_outbound(load("outbound.json"))
    check_journals(load("release-journal.json"))
    print(f"vectors OK: {checks} values recomputed independently in {d}")


if __name__ == "__main__":
    main()
