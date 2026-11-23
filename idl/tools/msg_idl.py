#!/usr/bin/env python3
"""Writes the IDLs of the messages programs in SPEL's format: `idl/ferrata_msg.idl.json` (v1, live on the test network,
`lez/msg-wire`) and `idl/ferrata_msg_v2.idl.json` (`lez/msg-v2-wire`). Standard library only.

    python3 -I idl/tools/msg_idl.py          # rewrite both
    python3 -I idl/tools/msg_idl.py --check  # exit 1 if a committed file differs from what this writes

`python3 -I idl/check.py` then decodes every committed vector of both programs with these files alone
(`idl/vectors/ferrata_msg*.vectors.json`, written by the wire crates' `tests/idl_vectors.rs`)."""
import json
import sys
from pathlib import Path

IDL = Path(__file__).resolve().parent.parent
ID = {"array": ["u8", 32]}
SPEC = "logos-co/spel c6f73ce (spel-framework-core 0.7.0, docs/reference/idl.md)"


def f(name, t):
    return {"name": name, "type": t}


def account(name, signer, docs, writable=True, pda=None):
    a = {"name": name, "writable": writable, "signer": signer, "init": False, "visibility": ["public"], "docs": [docs]}
    if pda:
        a["pda"] = {"seeds": [{"kind": "const", "value": pda[0]}] + [{"kind": "arg", "path": p} for p in pda[1:]]}
    return a


def instructions(table):
    return [
        {"name": n, "variant": v, "discriminator": [i], "accounts": accs, "args": args,
         "execution": {"public": True, "private_owned": False}, "docs": [d]}
        for i, (n, v, args, accs, d) in enumerate(table)
    ]


def errors(codes):
    return [{"code": c, "name": n, "msg": m} for c, n, m in codes]


KEY = {"name": "MsgKey", "kind": "struct", "fields": [f("epoch", "u32"), f("x25519", ID), f("mlkem768", {"option": {"vec": "u8"}})],
       "docs": ["A published messaging key: X25519, and ML-KEM-768 (1184 bytes) for hybrid envelopes."]}
POLICY = {"name": "Policy", "kind": "enum", "variants": [{"name": "Open"}, {"name": "ContactsOnly"}],
          "docs": ["Open: anyone not blocked, within the stranger limits; ContactsOnly: contacts only."]}

COMMON = [
    (0xA1, "BAD_MESSAGE", "The message does not decode (or, v2, Delete's numbers are empty, too many or not ascending)."),
    (0xA2, "BAD_STATE", "An actor state that does not decode, or a write-once actor written twice: defensive, no input reaches it."),
    (0xA3, "NOT_AUTHORIZED", "An owner's action not signed by the owner or not sent by the transaction itself."),
    (0xA4, "WRONG_SENDER", "A message that did not come from the one actor allowed to send it to this receiver."),
    (0xA5, "NO_KEY", "The recipient has published no key (v2: also Delete or Clear before any key)."),
    (0xA6, "BAD_KEY", "A malformed key, or an epoch that does not move forward."),
    (0xA7, "BLOCKED", "The recipient blocked the sender."),
    (0xA8, "NOT_A_CONTACT", "The recipient accepts contacts only."),
    (0xA9, "BAD_SIZE", "The envelope's length is not a bucket (v2: or a hybrid envelope below 2,048 bytes)."),
    (0xAA, "BAD_VERSION", "An unknown envelope version, or a hybrid envelope to a key without ML-KEM."),
    (0xAB, "STALE_KEY", "The envelope is for another key epoch than the recipient's."),
    (0xAC, "DOWNGRADE", "An X25519-only envelope to a recipient that published an ML-KEM key."),
    (0xAD, "INBOX_FULL", "The inbox holds 32 messages or 64 KiB."),
    (0xAE, "STRANGER_LIMIT", "This non-contact already holds its unread messages here (v1: 2, v2: 1)."),
    (0xAF, "STRANGERS_FULL", "Non-contacts already hold 8 messages here."),
    (0xB0, "RATE_LIMITED", "v1: 30 sends in the current fixed hour window; v2: 30 sends declared within the last hour."),
    (0xB1, "CLOCK_BACKWARDS", "now_ms before the sender's last send."),
    (0xB2, "SELF_SEND", "Sending to oneself."),
    (0xB3, "LIST_FULL", "More than 64 contacts or blocked."),
    (0xB4, "NOT_FOUND", "Delete of a message the inbox does not hold."),
]


def v1():
    own = account("own", True, "the signer's own actor (account, ferrata_msg); created on first use")
    rec = account("recipient", False, "the recipient's actor (to, ferrata_msg): receives Deliver as a synchronous message")
    table = [
        ("publish", "Publish", [f("key", {"defined": "MsgKey"})], [own], "Owner: publish or rotate the messaging key. Event KeyPublished."),
        ("set_policy", "SetPolicy", [f("policy", {"defined": "Policy"})], [own], "Owner: who may deliver."),
        ("contacts", "Contacts", [f("add", {"vec": ID}), f("remove", {"vec": ID})], [own], "Owner: edit the contacts list (removals first; at most 64)."),
        ("block", "Block", [f("add", {"vec": ID}), f("remove", {"vec": ID})], [own], "Owner: edit the block list (removals first; at most 64)."),
        ("send", "Send", [f("to", ID), f("now_ms", "u64"), f("body", {"vec": "u8"})], [own, rec],
         "Owner, as a sender: body is an envelope (512/1024/2048/4096 bytes; version 1 or 2; the recipient's epoch). At most 30 in a fixed window of an hour from the first send; valid only in blocks stamped [now_ms, now_ms + 600000)."),
        ("deliver", "Deliver", [f("now_ms", "u64"), f("body", {"vec": "u8"})], [account("recipient", False, "the recipient's actor")],
         "Internal: only from another actor of ferrata_msg (the sender's, through Send). Event Delivered."),
        ("delete", "Delete", [f("seqs", {"vec": "u64"})], [own], "Owner: delete these messages (each must be held)."),
        ("clear", "Clear", [f("upto", "u64")], [own], "Owner: delete every message with seq <= upto."),
    ]
    types = [
        KEY, POLICY,
        {"name": "Stored", "kind": "struct", "fields": [f("seq", "u64"), f("from", ID), f("at_ms", "u64"), f("body", {"vec": "u8"})], "docs": ["One delivered envelope."]},
        {"name": "Delivered", "kind": "struct", "fields": [f("from", ID), f("to", ID), f("seq", "u64"), f("len", "u32")], "docs": ["Event data, selector sha256(\"ferrata_msg::Delivered\")[..8]."]},
        {"name": "KeyPublished", "kind": "struct", "fields": [f("account", ID), f("epoch", "u32"), f("hybrid", "bool")], "docs": ["Event data, selector sha256(\"ferrata_msg::KeyPublished\")[..8]."]},
        {"name": "MsgMessage", "kind": "enum", "variants": [{"name": v, "fields": args} for (_, v, args, _, _) in table], "docs": ["The instruction enum (borsh; one-byte variant index)."]},
    ]
    accounts = [{"name": "Mailbox", "type": {"name": "Mailbox", "kind": "struct", "fields": [
        f("key", {"option": {"defined": "MsgKey"}}), f("policy", {"defined": "Policy"}), f("contacts", {"vec": ID}), f("blocked", {"vec": ID}),
        f("next_seq", "u64"), f("inbox", {"vec": {"defined": "Stored"}}), f("window_start_ms", "u64"), f("sent_in_window", "u32"),
        f("last_send_ms", "u64"), f("sent_total", "u64")]},
        "docs": ["The state of the actor (account, ferrata_msg): key, policy, lists, inbox (32 messages, 64 KiB), the sender's counters, all in one."]}]
    return {
        "version": "0.1.0", "name": "ferrata_msg", "spec": SPEC, "metadata": {"name": "ferrata_msg", "version": "0.1.0"},
        "instruction_type": "ferrata_lez_msg_wire::MsgMessage",
        "docs": [
            "ferrata_msg (v1): encrypted wallet-to-wallet messages, live and immutable on the Ferrata LEZ v0.4 test network (prototype, unaudited; docs/design/messages.md). Hand-written IDL in SPEL's format for the borsh interface of lez/msg-wire. Its successor is ferrata_msg_v2.",
            "One actor per account: (account, ferrata_msg); no PDAs. Owner instructions are sent by a transaction the account signs, straight to its own actor.",
            "The program stores envelopes it cannot read (versions 1 and 2: docs/design/messages.md, deploy/testnet/web/msg/ferrata-msg.js).",
            "Refusals exit with one byte, 0xA1-0xB4 (errors); a refusal reverts the whole transaction and writes nothing.",
        ],
        "instructions": instructions(table), "accounts": accounts, "types": types, "errors": errors(COMMON),
    }


def v2():
    prof = account("profile", True, "the signer's profile (account, ferrata_msg_v2): key, policy, lists, sending record")
    inbox = account("inbox", False, "the owner's inbox: index of its messages", pda=("inbox", "owner"))
    env = account("envelope", False, "one message's envelope, written once, emptied on delete", pda=("envelope", "recipient", "sender", "seq"))
    rprof = account("recipient_profile", False, "the recipient's profile: read, not written", writable=False)
    table = [
        ("publish", "Publish", [f("key", {"defined": "MsgKey"})], [prof, inbox], "Owner: publish or rotate the messaging key; the first one opens the inbox (Open). Event KeyPublished."),
        ("set_policy", "SetPolicy", [f("policy", {"defined": "Policy"})], [prof], "Owner: who may deliver."),
        ("contacts", "Contacts", [f("add", {"vec": ID}), f("remove", {"vec": ID})], [prof], "Owner: edit the contacts list (removals first; at most 64)."),
        ("block", "Block", [f("add", {"vec": ID}), f("remove", {"vec": ID})], [prof], "Owner: edit the block list (removals first; at most 64)."),
        ("send", "Send", [f("to", ID), f("now_ms", "u64"), f("seq", "u64"), f("body", {"vec": "u8"})], [prof, rprof, inbox, env],
         "Owner, as a sender: seq is the sender's send counter (STALE_SEQ otherwise); body an envelope (512/1024/2048/4096 bytes, a hybrid one at least 2048; version 3 or 4; the recipient's epoch). At most 30 sends declared within any hour; valid only in blocks stamped [now_ms, now_ms + 600000). Declares the recipient's profile, the recipient's inbox and envelope(to, signer, seq)."),
        ("delete", "Delete", [f("ns", {"vec": "u64"})], [prof, inbox, env], "Owner: delete these messages (1 to 32 numbers, strictly ascending, each held); their envelopes are emptied. Declares the inbox and each envelope."),
        ("clear", "Clear", [f("upto", "u64")], [prof, inbox, env], "Owner: delete every message numbered up to upto; their envelopes are emptied."),
        ("deliver", "Deliver", [f("now_ms", "u64"), f("seq", "u64"), f("body", {"vec": "u8"})], [rprof], "Internal: sender's profile to recipient's profile, only from an actor of ferrata_msg_v2."),
        ("open", "Open", [], [inbox], "Internal: owner's profile to its new inbox."),
        ("admit", "Admit", [f("from", ID), f("seq", "u64"), f("at_ms", "u64"), f("len", "u32"), f("contact", "bool")], [inbox], "Internal: recipient's profile to its inbox: capacity and stranger limits, then the entry. Event Delivered."),
        ("store", "Store", [f("from", ID), f("seq", "u64"), f("at_ms", "u64"), f("body", {"vec": "u8"})], [env], "Internal: recipient's profile to the new envelope."),
        ("remove", "Remove", [f("ns", {"vec": "u64"})], [inbox], "Internal: owner's profile to its inbox; Erase to each removed envelope."),
        ("remove_up_to", "RemoveUpTo", [f("upto", "u64")], [inbox], "Internal: owner's profile to its inbox; Erase to each removed envelope."),
        ("erase", "Erase", [], [env], "Internal: inbox to one of its envelopes: empty it."),
    ]
    types = [
        KEY, POLICY,
        {"name": "Entry", "kind": "struct", "fields": [f("n", "u64"), f("from", ID), f("seq", "u64"), f("at_ms", "u64"), f("len", "u32"), f("contact", "bool")],
         "docs": ["One message of an inbox: its number, sender, the sender's seq (with the recipient and the sender, the envelope's address), time, length, contact or not when delivered."]},
        {"name": "Delivered", "kind": "struct", "fields": [f("from", ID), f("to", ID), f("n", "u64"), f("len", "u32")], "docs": ["Event data, selector sha256(\"ferrata_msg_v2::Delivered\")[..8]."]},
        {"name": "KeyPublished", "kind": "struct", "fields": [f("account", ID), f("epoch", "u32"), f("hybrid", "bool")], "docs": ["Event data, selector sha256(\"ferrata_msg_v2::KeyPublished\")[..8]."]},
        {"name": "MsgMessage", "kind": "enum", "variants": [{"name": v, "fields": args} for (_, v, args, _, _) in table], "docs": ["The instruction enum (borsh; one-byte variant index): 0-6 the owner's, 7-13 internal."]},
    ]
    accounts = [
        {"name": "Profile", "type": {"name": "Profile", "kind": "struct", "fields": [
            f("key", {"option": {"defined": "MsgKey"}}), f("policy", {"defined": "Policy"}), f("contacts", {"vec": ID}), f("blocked", {"vec": ID}),
            f("sent_at", {"vec": "u64"}), f("last_send_ms", "u64"), f("sent_total", "u64")]},
         "docs": ["profile (account, ferrata_msg_v2): written only by the owner's own transactions; sent_at holds the declared times of its sends within the last hour (at most 30)."]},
        {"name": "Inbox", "type": {"name": "Inbox", "kind": "struct", "fields": [f("owner", ID), f("next_n", "u64"), f("entries", {"vec": {"defined": "Entry"}})]},
         "docs": ["inbox(owner): at most 32 entries and 64 KiB; non-contacts at most 8 entries, 1 each."]},
        {"name": "Envelope", "type": {"name": "Envelope", "kind": "struct", "fields": [f("recipient", ID), f("from", ID), f("seq", "u64"), f("at_ms", "u64"), f("body", {"vec": "u8"})]},
         "docs": ["envelope(recipient, sender, seq): one ciphertext; empty once deleted."]},
    ]
    bad_state = (0xA2, "BAD_STATE", "An actor state that does not decode: a send addressed to one of the program's own accounts (an inbox or an envelope, whose state is not a profile); or a write-once actor written twice (defensive). Nothing is written.")
    codes = [bad_state if c == 0xA2 else (c, n, m) for c, n, m in COMMON]
    codes += [(0xB5, "STALE_SEQ", "Send whose seq is not the sender's send counter (a concurrent or replayed send).")]
    return {
        "version": "0.2.0", "name": "ferrata_msg_v2", "spec": SPEC, "metadata": {"name": "ferrata_msg_v2", "version": "0.2.0"},
        "instruction_type": "ferrata_lez_msg_v2_wire::MsgMessage",
        "docs": [
            "ferrata_msg_v2: encrypted wallet-to-wallet messages on the Ferrata LEZ v0.4 test network, one account per message (prototype, unaudited; docs/design/messages.md). Hand-written IDL in SPEL's format for the borsh interface of lez/msg-v2-wire.",
            "Actors: profile (account, ferrata_msg_v2); inbox and envelope are PDAs: an actor's seed is SHA-256(\"ferrata-msg/v2/\" || tag || parts), with u64 parts little-endian; its account is LEZ v0.4's AccountId::for_public_pda(program, seed). The program creates both PDAs itself (it names their seeds); a transaction declares every actor it reaches and carries no evidence.",
            "Owner instructions (0-6) are sent by a transaction the account signs, straight to its own profile; 7-13 pass only between the program's own actors, each checked to come from the one actor allowed to send it.",
            "The program stores envelopes it cannot read (versions 3 and 4: docs/design/messages.md, deploy/testnet/web/msg/ferrata-msg.js).",
            "Refusals exit with one byte, 0xA1-0xB5 (errors); a refusal reverts the whole transaction and writes nothing.",
        ],
        "instructions": instructions(table), "accounts": accounts, "types": types, "errors": errors(codes),
    }


def main():
    check = "--check" in sys.argv
    bad = 0
    for name, idl in (("ferrata_msg", v1()), ("ferrata_msg_v2", v2())):
        text = json.dumps(idl, indent=2) + "\n"
        path = IDL / f"{name}.idl.json"
        if check:
            if path.read_text() != text:
                print(f"{path.name}: differs from idl/tools/msg_idl.py")
                bad += 1
        else:
            path.write_text(text)
            print(f"wrote {path.name}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
