#!/usr/bin/env python3
"""Writes idl/ferrata_attestation.idl.json and idl/ferrata_bridge.idl.json.

The two JSON files are the IDL (SPEL's format, `spel-framework-core` 0.7.0's `SpelIdl`). This
script is how they were written by hand: the instructions, accounts and types below are typed
from `lez/wire/src/lib.rs` and `lez/bridge-wire/src/lib.rs`; the error lists are read from the
Rust sources so that each code keeps its name and its first doc line. `idl/check.py` is the
arbiter: it decodes the committed borsh vectors with the JSON alone.

    python3 idl/tools/build_idl.py      # rewrites both files

Standard library only.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEL = "logos-co/spel c6f73ce (spel-framework-core 0.7.0, docs/reference/idl.md)"

# --- type shorthands (SPEL's IdlType grammar) ---------------------------------------------
U8, U32, U64, U128, BOOL = "u8", "u32", "u64", "u128", "bool"
ACCOUNT_ID = "account_id"  # 32 bytes; lee_core's AccountId (rendered in base58)
BYTES = {"vec": "u8"}


def arr(t, n):
    return {"array": [t, n]}


def vec(t):
    return {"vec": t}


def opt(t):
    return {"option": t}


def d(name):
    return {"defined": name}


ROOT32 = arr(U8, 32)  # a 32-byte hash, root, id or big-endian word
ADDRESS = arr(U8, 20)  # an Ethereum address
PROOF = vec(BYTES)  # Merkle-Patricia nodes, as eth_getProof returns them


def f(name, ty):
    return {"name": name, "type": ty}


def struct(name, *fields, docs=None, **extra):
    t = {"name": name, "kind": "struct", "fields": list(fields)}
    if docs:
        t["docs"] = docs if isinstance(docs, list) else [docs]
    t.update(extra)
    return t


def enum(name, *variants, docs=None):
    t = {"name": name, "kind": "enum", "variants": list(variants)}
    if docs:
        t["docs"] = docs if isinstance(docs, list) else [docs]
    return t


def v(name, *fields):
    out = {"name": name}
    if fields:
        out["fields"] = list(fields)
    return out


def tv(name, *types):
    """A tuple variant: fields named "0", "1", ... (borsh: the values in order)."""
    return v(name, *[f(str(i), t) for i, t in enumerate(types)])


# --- errors, read from the sources ----------------------------------------------------------


def first_doc(lines, i):
    """The doc comment above line i, joined (up to the first sentence's end of paragraph)."""
    docs = []
    j = i - 1
    while j >= 0 and lines[j].strip().startswith("///"):
        docs.insert(0, lines[j].strip()[3:].strip())
        j -= 1
    return " ".join(docs).strip() or None


def codes_from_macro(path, module_marker):
    lines = Path(path).read_text().splitlines()
    start = next(i for i, l in enumerate(lines) if module_marker in l)
    out = []
    for i in range(start, len(lines)):
        m = re.match(r"\s+([A-Z_][A-Z0-9_]*) = (\d+);", lines[i])
        if m:
            out.append({"code": int(m.group(2)), "name": m.group(1), "msg": first_doc(lines, i)})
        if lines[i].startswith("}") and out:
            break
    return out


def core_errors():
    lines = (ROOT / "crates/ferrata-core/src/error.rs").read_text().splitlines()
    out = []
    in_enum = False
    for i, l in enumerate(lines):
        if l.startswith("pub enum Error"):
            in_enum = True
            continue
        if in_enum and l.startswith("}"):
            break
        m = re.match(r"\s+([A-Z][A-Za-z0-9]*) = (\d+),", l) if in_enum else None
        if m:
            out.append({"code": int(m.group(2)), "name": m.group(1), "msg": first_doc(lines, i)})
    return out


def step_errors():
    lines = (ROOT / "crates/ferrata-core/src/steps.rs").read_text().splitlines()
    docs = {}
    for i, l in enumerate(lines):
        m = re.match(r"\s+([A-Z][A-Za-z]*),$", l)
        if m:
            docs.setdefault(m.group(1), first_doc(lines, i))
    out = []
    for l in lines:
        m = re.match(r"\s+StepError::([A-Z][A-Za-z]*) => (\d+),", l)
        if m:
            out.append({"code": int(m.group(2)), "name": m.group(1), "msg": docs.get(m.group(1))})
    return out


# --- the attestation program ----------------------------------------------------------------

A_TYPES = [
    struct(
        "Config",
        f("chain_id", U64),
        f("sync_committee_size", U32),
        f("slots_per_epoch", U64),
        f("epochs_per_sync_committee_period", U64),
        f("min_sync_committee_participants", U64),
        f("fork_versions", arr(arr(U8, 4), 9)),
        f("fork_epochs", arr(U64, 9)),
        f("genesis_validators_root", ROOT32),
        f("genesis_time", U64),
        f("seconds_per_slot", U64),
        f("store_fork", U8),
        f("adoption_parts", U8),
        f("ws_period_epochs", U64),
        docs="The chain an instance follows. Fixed at Init; part of the instance id. store_fork: 0 Altair, 1 Capella, 2 Deneb, 3 Electra, 4 Gloas. ws_period_epochs: the weak-subjectivity period of the finalized header (nonzero); Init, Open, VerifyAll and Succeed are valid only before it ends.",
    ),
    enum(
        "Message",
        v("Init", f("config", d("Config")), f("checkpoint", ROOT32), f("bootstrap", BYTES)),
        v("Open", f("update", BYTES), f("signers", BYTES)),
        v("Miller"),
        v("Adopt", f("part", U8), f("keys", vec(arr(U8, 96)))),
        v("Finish"),
        v("Read"),
        v("ReadPadded", f("padding", BYTES)),
        v("Attest", f("request", d("Request"))),
        v(
            "VerifyAll",
            f("config", d("Config")),
            f("checkpoint", ROOT32),
            f("store", BYTES),
            f("update", BYTES),
            f("signers", BYTES),
            f("adoption", vec(arr(U8, 96))),
        ),
        v("JobOpen", f("instance_id", ROOT32), f("job_id", ROOT32), f("committee_size", U32), f("job", BYTES)),
        v("Apply", f("instance_id", ROOT32), f("job_id", ROOT32), f("job", BYTES)),
        v("ApplyVerified", f("instance_id", ROOT32), f("job", BYTES)),
        v("Record", f("instance_id", ROOT32), f("slot", U64), f("chain_id", U64), f("record", BYTES), f("root_id", ROOT32)),
        v("Succeed", f("config", d("Config")), f("checkpoint", ROOT32)),
        v("Spawn", f("root_id", ROOT32), f("parent_id", ROOT32), f("checkpoint", ROOT32), f("config", d("Config")), f("store", BYTES)),
        docs="Every message the program handles (ferrata_lez_wire::Message): a borsh enum, one byte of variant index then the fields.",
    ),
    enum("Cmp", v("Eq"), v("Ne"), v("Lt"), v("Le"), v("Gt"), v("Ge"), docs="A comparison of two 256-bit unsigned values."),
    enum(
        "Field",
        v("Nonce"),
        v("Balance"),
        v("StorageRoot"),
        v("CodeHash"),
        v("Value"),
        v("Emitter"),
        tv("Topic", U8),
        tv("DataWord", U32),
        docs="A field of a fact, as a 256-bit word. Topic(i): log topic i; DataWord(i): the i-th 32-byte word of a log's data.",
    ),
    struct("Condition", f("field", d("Field")), f("cmp", d("Cmp")), f("rhs", ROOT32), docs="field <cmp> rhs (rhs big-endian)."),
    struct("Operand", f("fact", U32), f("field", d("Field")), docs="A field of one of the request's facts, by the fact's position in the reply."),
    struct("Relation", f("left", d("Operand")), f("cmp", d("Cmp")), f("right", d("Operand"))),
    struct("SlotQuery", f("slot", ROOT32), f("proof", PROOF), f("conditions", vec(d("Condition"))), docs="storageProof[i].proof of eth_getProof."),
    struct(
        "AccountQuery",
        f("address", ADDRESS),
        f("proof", PROOF),
        f("conditions", vec(d("Condition"))),
        f("slots", vec(d("SlotQuery"))),
        docs="accountProof of eth_getProof, and slots of the account's storage.",
    ),
    struct(
        "LogQuery",
        f("tx_index", U64),
        f("log_index", U64),
        f("proof", PROOF),
        f("conditions", vec(d("Condition"))),
        docs="The log at log_index (from zero within the receipt) of the receipt of transaction tx_index, with the receipt's proof in the receipts trie.",
    ),
    struct(
        "LeafQuery",
        f("root", U32),
        f("leaf_index", U64),
        f("leaf", ROOT32),
        f("path", vec(ROOT32)),
        docs="A leaf of the accumulator whose root is the value of the storage fact at position root of the reply, with its siblings.",
    ),
    struct(
        "Hop",
        f("account_proof", PROOF),
        f("storage_proof", PROOF),
        f("header", BYTES),
        docs="One EIP-2935 hop to an older block: proofs in the state of the block hopped from, and the older block's RLP header.",
    ),
    struct(
        "Request",
        f("instance_id", ROOT32),
        f("chain_id", U64),
        f("header", opt(BYTES)),
        f("hops", vec(d("Hop"))),
        f("accounts", vec(d("AccountQuery"))),
        f("logs", vec(d("LogQuery"))),
        f("leaves", vec(d("LeafQuery"))),
        f("relations", vec(d("Relation"))),
        docs="What a consumer asks of header(id, slot). header: the RLP header of the record's block when the record binds a block hash only (Gloas layout).",
    ),
    enum(
        "Fact",
        v("Account", f("address", ADDRESS), f("nonce", U64), f("balance", ROOT32), f("storage_root", ROOT32), f("code_hash", ROOT32)),
        v("Storage", f("address", ADDRESS), f("slot", ROOT32), f("value", ROOT32)),
        v("Log", f("tx_index", U64), f("log_index", U64), f("address", ADDRESS), f("topics", vec(ROOT32)), f("data", BYTES)),
        v("AccumulatorLeaf", f("address", ADDRESS), f("slot", ROOT32), f("leaf_index", U64), f("leaf", ROOT32), f("root", ROOT32)),
        docs="A proven fact.",
    ),
    struct("Statement", f("id", ROOT32), f("fact", d("Fact")), docs="A fact with its statement identifier."),
    struct(
        "Attestation",
        f("instance_id", ROOT32),
        f("chain_id", U64),
        f("slot", U64),
        f("block_number", U64),
        f("block_hash", ROOT32),
        f("hops", U32),
        f("statements", vec(d("Statement"))),
        docs="The reply to Attest: facts in the order each account, then its slots, then the logs, then the leaves.",
    ),
]

A_ACCOUNTS = [
    {
        "name": "InstanceState",
        "type": struct(
            "InstanceState",
            f("id", ROOT32),
            f("config", d("Config")),
            f("active", BOOL),
            f("store", BYTES),
            f("applied", U64),
            f("root_id", ROOT32),
            f("parent_id", ROOT32),
            docs="instance(id): store is the CompactStore encoding; active once the initial committee is adopted. root_id: the id of the first instance of its line (its own id for an Init); parent_id: the instance a successor was spawned from (zero for an Init).",
        ),
    },
    {
        "name": "JobState",
        "type": struct(
            "JobState", f("instance_id", ROOT32), f("job_id", ROOT32), f("committee_size", U32), f("job", BYTES), docs="job(id, job_id)."
        ),
    },
    {
        "name": "RecordState",
        "type": struct(
            "RecordState",
            f("instance_id", ROOT32),
            f("chain_id", U64),
            f("record", BYTES),
            f("root_id", ROOT32),
            docs="header(id, slot), written once: record is ferrata_core::witness::HeaderRecord's encoding; root_id: the instance line's root id, which an Attest request may name instead of the instance.",
        ),
    },
]


def pda_a(tag, *args):
    return {"seeds": [{"kind": "const", "value": tag}] + [{"kind": "arg", "path": a} for a in args]}


def acct(name, pda=None, writable=True, signer=False, init=False, docs=None, rest=False):
    out = {"name": name, "writable": writable, "signer": signer, "init": init, "visibility": ["public"]}
    if pda:
        out["pda"] = pda
    if rest:
        out["rest"] = True
    if docs:
        out["docs"] = [docs]
    return out


INSTANCE = lambda init=False, docs=None: acct("instance", pda_a("instance", "instance_id"), init=init, docs=docs)
JOB = lambda init=False, docs=None: acct("job", pda_a("job", "instance_id", "job_id"), init=init, docs=docs)
HEADER = lambda init=False, docs=None, writable=True: acct("header", pda_a("header", "instance_id", "slot"), init=init, docs=docs, writable=writable)
ANY = lambda docs: acct("receiver", None, writable=False, docs=docs)


def instr(variants, index, name, accounts, docs, public=True, private_owned=False, internal=False):
    var = variants[index]
    out = {
        "name": name,
        "variant": var["name"],
        "discriminator": [index],
        "accounts": accounts,
        "args": var.get("fields", []),
        "execution": {"public": public, "private_owned": private_owned},
        "docs": [docs],
    }
    if internal:
        out["internal"] = True
    return out


def attestation_idl():
    variants = A_TYPES[1]["variants"]
    I = lambda *a, **k: instr(variants, *a, **k)
    instructions = [
        I(0, "init", [INSTANCE(init=True, docs="instance(instance_id(config, checkpoint)), created: the transaction carries LEZ's PDA evidence for its seed"), JOB(init=True, docs="the job that adopts the bootstrap's committee")], "Create an instance from a bootstrap matching checkpoint, and open the job that adopts its committee. Anyone may call."),
        I(1, "open", [INSTANCE(), JOB(init=True, docs="job(id, job_id): job_id hashes the instance, its store digest and the update")], "Step 1 of an update (CompactUpdate, SignerWitness encodings): opens job(id, job_id). Anyone may call; no step checks a sender."),
        I(2, "miller", [JOB()], "Step 2, at the job: the Miller loops."),
        I(3, "adopt", [JOB(), INSTANCE(docs="the last part of Init's adoption only: activates the instance"), HEADER(init=True, docs="the last part of Init's adoption only: the bootstrap header's record")], "One part of the job's committee adoption: n / parts uncompressed keys."),
        I(4, "finish", [JOB(), INSTANCE(docs="receives Apply"), HEADER(init=True, docs="receives Record when a new header is finalized")], "Step 3, at the job: the final exponentiation, then Apply to the instance."),
        I(5, "read", [ANY("any actor of this program; replies StateReply with its state")], "Reply with this actor's state (LEZ v0.4's read idiom)."),
        I(6, "read_padded", [ANY("any actor of this program")], "As Read, carrying bytes the program ignores (cost measurement)."),
        I(7, "attest", [HEADER(writable=False, docs="header(id, slot): the record the request is proven under")], "Prove the facts of request under this record; reply with an Attestation, or, as a transaction's root, emit the FRT-ATT1 event. Changes no state."),
        I(8, "verify_all", [ANY("any actor of this program, meant to run privately; sends ApplyVerified to instance(id)")], "The private placement: the whole verification of one update in one call.", public=False, private_owned=True),
        I(9, "job_open", [JOB(init=True, docs="from instance(id) only")], "Internal: instance(id) -> job(id, job_id), the job step 1 produced.", internal=True),
        I(10, "apply", [INSTANCE(docs="from job(id, job_id) only")], "Internal: the verified job, applied to the instance (compare-and-swap on the store digest).", internal=True),
        I(11, "apply_verified", [INSTANCE(docs="from any actor of this program (authenticated by program)")], "Internal: a job verified in one call by VerifyAll.", internal=True),
        I(12, "record", [HEADER(init=True, docs="from instance(id) only; written once")], "Internal: the record to write once (HeaderRecord encoding), with the instance's chain id and root id.", internal=True),
        I(13, "succeed", [INSTANCE(docs="instance(parent): checks the successor's config against its own and that checkpoint is its finalized header; sends Spawn"), acct("successor", pda_a("instance", "successor_id"), init=True, docs="instance(successor_id), successor_id = SHA-256(\"successor-id\" || root_id || parent || config || checkpoint), created by Spawn")], "Spawn a successor of this instance with a new fork schedule, from its finalized header, inside its weak-subjectivity period; emits FRT-SUC1. Anyone may call."),
        I(14, "spawn", [INSTANCE(init=True, docs="instance(successor_id): from the parent instance only; created active with the parent's store")], "Internal: the parent instance -> the successor it spawns.", internal=True),
    ]
    errors = core_errors() + step_errors() + codes_from_macro(ROOT / "lez/wire/src/lib.rs", "pub mod code {")
    return {
        "version": "0.1.0",
        "name": "ferrata_attestation",
        "spec": SPEL,
        "metadata": {"name": "ferrata_attestation", "version": "0.1.0"},
        "instruction_type": "ferrata_lez_wire::Message",
        "docs": [
            "The attestation program of Ferrata on LEZ v0.4 (RFP-021). Hand-written IDL in SPEL's format: SPEL's stable release targets LEZ v0.2.4, so the program is written against LEZ v0.4 directly, and this IDL describes its borsh interface.",
            "Each instruction is a variant of the borsh enum named by instruction_type; discriminator is its one-byte variant index, followed by args in order.",
            "PDA seeds: an actor's seed is SHA-256(\"ferrata-lez/v1/\" || tag || parts), with u64 parts little-endian; its account is LEZ v0.4's AccountId::for_public_pda(program, seed). instance_id = SHA-256(\"ferrata-lez/v1/instance-id\" || borsh(Config) || checkpoint).",
            "Refusals exit with one byte: 1-61 and 100-199 the core's, 80-99 the step API's, 200-217 this program's (lez/wire CODE_BLOCKS). A refusal reverts the whole transaction on v0.4.",
            "Events: FRT-FIN1 (finalized header advanced), FRT-UPG1 (store moved to a later fork's layout), FRT-ATT1 (Attest as a transaction's root: instance id, chain id, block number LE, then each statement id).",
        ],
        "instructions": instructions,
        "accounts": A_ACCOUNTS,
        "types": A_TYPES,
        "errors": errors,
    }


# --- the bridge program ---------------------------------------------------------------------

B_TYPES = [
    struct(
        "TokenEntry",
        f("token", ADDRESS),
        f("lez_definition", ROOT32),
        f("decimals", U8),
        f("lez_unit", U128),
        f("unit_weight", U128),
        f("denominations", vec(U128)),
        f("relay_fees", vec(U128)),
        docs="A token's registry entry, as the vault holds it (attested by Register).",
    ),
    struct(
        "FeeSchedule",
        f("index", U64),
        f("from_leaf", U64),
        f("announced_at", U64),
        f("effective_at", U64),
        f("mint_fees", vec(U128)),
        f("burn_fees", vec(U128)),
        f("checkpoint_fees", vec(U128)),
    ),
    struct("ScheduleRange", f("index", U64), f("from_leaf", U64), f("until_leaf", U64)),
    struct("CapParams", f("capacity", U128), f("refill_per_second", U128), docs="A token bucket's parameters (u128::MAX: unlimited)."),
    struct("Bucket", f("capacity", U128), f("refill_per_second", U128), f("level", U128), f("last_ms", U64)),
    struct(
        "Gate",
        f("now_ms", U64),
        f("mints_until", U64),
        f("burns_until", U64),
        f("mint", d("Bucket")),
        f("burn", d("Bucket")),
        docs="The attested clock, the freezes (until, in ms) and the mint and burn buckets.",
    ),
    struct("TimeRecord", f("record_slot", U64), f("header", opt(BYTES))),
    enum("Timed", v("Tick"), v("Freeze", f("mints", BOOL), f("burns", BOOL), f("until_ms", U64)), v("Pause", f("mints", BOOL), f("burns", BOOL)), v("Propose", f("freeze_authority", d("FreezeAuthority")), f("watchers", vec(ROOT32))), v("Apply")),
    struct("PendingTime", f("record", d("TimeRecord")), f("then", d("Timed"))),
    struct("FreezeAuthority", f("members", vec(ROOT32)), f("threshold", U8), docs="The freeze authority: one account (threshold 1) or an M-of-N multisig of at most 8 distinct members (Cosigned)."),
    struct("RoleProposal", f("freeze_authority", d("FreezeAuthority")), f("watchers", vec(ROOT32)), f("effective_ms", U64), docs="A role change the admin proposed; ApplyRoles once a record 14 days of attested time later is read."),
    struct("Roles", f("freeze_authority", d("FreezeAuthority")), f("watchers", vec(ROOT32)), f("proposal", opt(d("RoleProposal"))), f("vault_retired", BOOL), f("sunset_approved", BOOL), f("inbound_depth_slots", U64), f("tip_slot", U64), docs="The live roles in config(cid): the freeze authority, up to three watchers, a pending proposal; vault_retired once AttestRetired read the vault's retired flag; sunset_approved by a multisig's Cosigned Sunset; inbound_depth_slots, the admin's inbound finality depth beyond Ethereum finality (SetDepth, at most 216,000), measured against tip_slot, the newest record slot the configuration's clock read."),
    enum("CosignedAction", v("Sunset"), v("Freeze", f("token", opt(ADDRESS)), f("mints", BOOL), f("burns", BOOL), f("until_ms", U64), f("record", d("TimeRecord"))), docs="What a multisig freeze authority signs (Cosigned)."),
    struct(
        "Controls",
        f("admin", ROOT32),
        f("freeze_authority", ROOT32),
        f("fee_recipient", ROOT32),
        f("floor", d("CapParams")),
        f("token_cap", d("CapParams")),
        f("global_cap", d("CapParams")),
    ),
    struct(
        "BridgeConfig",
        f("bedrock_genesis_id", ROOT32),
        f("channel_id", ROOT32),
        f("bridge_image_id", ROOT32),
        f("attestation", ACCOUNT_ID),
        f("instance_id", ROOT32),
        f("evm_chain_id", U64),
        f("vault", ADDRESS),
        f("controls", d("Controls")),
        docs="A configuration; its id (config_id) is what the vault's slot 2 holds (reciprocal pairing).",
    ),
    struct("Note", f("r", ROOT32), f("memo", ADDRESS), f("mac", arr(U8, 16)), f("token", ADDRESS), f("amount", ROOT32), f("c_out", ROOT32)),
    struct("ImportProof", f("account", PROOF), f("root_slot", PROOF), f("count_slot", PROOF)),
    struct("PairingProof", f("account", PROOF), f("config_slot", PROOF)),
    struct("VaultProof", f("account", PROOF), f("slots", vec(PROOF))),
    struct(
        "ClaimWitness",
        f("depositor", ADDRESS),
        f("token", ADDRESS),
        f("amount", ROOT32),
        f("leaf_index", U64),
        f("path", vec(ROOT32)),
        f("s", ROOT32),
        f("kind", U8),
        f("dest", ROOT32),
        docs="The opening of a deposit leaf. kind: 1 private, 2 public destination.",
    ),
    struct("FeeShare", f("0", ROOT32), f("1", U128), docs="A Rust tuple (account, amount): borsh writes the two values in order.", **{"x-rust": "tuple"}),
    enum(
        "BridgeMessage",
        v("Burn", f("config_id", ROOT32), f("r", ROOT32), f("memo", ADDRESS), f("mac", arr(U8, 16)), f("token", ADDRESS), f("amount", ROOT32), f("c_out", ROOT32)),
        v("ImportRoot", f("config_id", ROOT32), f("record_slot", U64), f("root", ROOT32), f("leaf_count", U64), f("proof", d("ImportProof"))),
        v("Sunset", f("config_id", ROOT32)),
        v("Echo", f("nonce", ROOT32)),
        v("Init", f("config", d("BridgeConfig")), f("record_slot", U64), f("proof", d("PairingProof"))),
        v("Claim", f("config_id", ROOT32), f("leaf_count", U64), f("witness", d("ClaimWitness")), f("range", d("ScheduleRange")), f("relayer", opt(ROOT32))),
        v("ClaimPrivately", f("config_id", ROOT32), f("leaf_count", U64), f("witness", d("ClaimWitness")), f("range", d("ScheduleRange")), f("relayer", opt(ROOT32))),
        v("Read"),
        v("WriteRoot", f("root", d("RootState"))),
        v(
            "Spend",
            f("nullifier", ROOT32),
            f("config_id", ROOT32),
            f("evm_chain_id", U64),
            f("vault", ADDRESS),
            f("token", ADDRESS),
            f("leaf_count", U64),
            f("amount", U128),
            f("mint_to", opt(ROOT32)),
            f("fees", vec(d("FeeShare"))),
        ),
        v("Mint", f("nullifier", ROOT32), f("config_id", ROOT32), f("evm_chain_id", U64), f("vault", ADDRESS), f("token", ADDRESS), f("holder", ROOT32), f("amount", U128)),
        v("Register", f("config_id", ROOT32), f("entry", d("TokenEntry")), f("schedule", d("FeeSchedule")), f("record_slot", U64), f("proof", d("VaultProof"))),
        v(
            "ImportFees",
            f("config_id", ROOT32),
            f("token", ADDRESS),
            f("index", U64),
            f("schedule", opt(d("FeeSchedule"))),
            f("record_slot", U64),
            f("proof", d("VaultProof")),
        ),
        v("WriteToken", f("state", d("TokenState"))),
        v("ExtendFees", f("config_id", ROOT32), f("token", ADDRESS), f("index", U64), f("schedule", opt(d("FeeSchedule"))), f("count", U64)),
        v(
            "Issue",
            f("config_id", ROOT32),
            f("token", ADDRESS),
            f("nullifier", ROOT32),
            f("amount", U128),
            f("range", d("ScheduleRange")),
            f("leaf_count", U64),
            f("mint_to", opt(ROOT32)),
            f("relayer", opt(ROOT32)),
        ),
        v("Admit", f("config_id", ROOT32), f("token", ADDRESS), f("mints", BOOL), f("value", U128)),
        v("Freeze", f("config_id", ROOT32), f("token", opt(ADDRESS)), f("mints", BOOL), f("burns", BOOL), f("until_ms", U64), f("record", d("TimeRecord"))),
        v("SetCap", f("config_id", ROOT32), f("token", opt(ADDRESS)), f("mints", BOOL), f("cap", d("CapParams"))),
        v("Tick", f("config_id", ROOT32), f("token", opt(ADDRESS)), f("record", d("TimeRecord"))),
        v("AttestRetired", f("config_id", ROOT32), f("record_slot", U64), f("proof", d("VaultProof"))),
        v("ProposeRoles", f("config_id", ROOT32), f("freeze_authority", d("FreezeAuthority")), f("watchers", vec(ROOT32)), f("record", d("TimeRecord"))),
        v("ApplyRoles", f("config_id", ROOT32), f("record", d("TimeRecord"))),
        v("CancelRoles", f("config_id", ROOT32)),
        v("Pause", f("config_id", ROOT32), f("token", opt(ADDRESS)), f("mints", BOOL), f("burns", BOOL), f("record", d("TimeRecord"))),
        v("Cosigned", f("config_id", ROOT32), f("signers", vec(ROOT32)), f("action", d("CosignedAction"))),
        v("Transfer", f("config_id", ROOT32), f("token", ADDRESS), f("amount", U128), f("to", ROOT32)),
        v("Credit", f("config_id", ROOT32), f("token", ADDRESS), f("amount", U128), f("to", ROOT32)),
        v("SetDepth", f("config_id", ROOT32), f("slots", U64)),
        v("AttestListing", f("config_id", ROOT32), f("token", ADDRESS), f("record_slot", U64), f("proof", d("VaultProof"))),
        v("SetListing", f("config_id", ROOT32), f("token", ADDRESS), f("delisted", BOOL), f("record_slot", U64)),
        docs="Every message the bridge program handles (ferrata_lez_bridge_wire::BridgeMessage).",
    ),
    enum(
        "ConfigState",
        v("Pairing", f("config", d("BridgeConfig")), f("record_slot", U64)),
        v("Ready", f("config", d("BridgeConfig")), f("pending", opt(d("Pending"))), f("gate", d("Gate")), f("sunset", BOOL), f("roles", d("Roles"))),
    ),
    enum(
        "Pending",
        tv("Import", d("PendingImport")),
        v("Register", f("entry", d("TokenEntry")), f("schedule", d("FeeSchedule")), f("record_slot", U64)),
        v("Fees", f("token", ADDRESS), f("index", U64), f("schedule", opt(d("FeeSchedule"))), f("record_slot", U64)),
        tv("Time", d("PendingTime")),
        v("Retired", f("record_slot", U64)),
        v("Listing", f("token", ADDRESS), f("record_slot", U64)),
        docs="A transaction-internal step: an attestation asked and not yet answered.",
    ),
    struct("PendingImport", f("record_slot", U64), f("root", ROOT32), f("leaf_count", U64)),
    struct(
        "RootState",
        f("config_id", ROOT32),
        f("evm_chain_id", U64),
        f("vault", ADDRESS),
        f("leaf_count", U64),
        f("root", ROOT32),
        f("record_slot", U64),
        f("block_number", U64),
    ),
    struct(
        "TokenState",
        f("config_id", ROOT32),
        f("evm_chain_id", U64),
        f("vault", ADDRESS),
        f("attestation", ACCOUNT_ID),
        f("instance_id", ROOT32),
        f("entry", d("TokenEntry")),
        f("schedules", vec(d("FeeSchedule"))),
        f("known_until", U64),
        f("controls", d("Controls")),
        f("gate", d("Gate")),
        f("pending", opt(d("PendingTime"))),
        f("delisted", BOOL),
        f("listing_slot", U64),
    ),
    struct("NullifierState", f("config_id", ROOT32), f("token", ADDRESS), f("leaf_count", U64), f("amount", U128)),
    struct("Holding", f("config_id", ROOT32), f("token", ADDRESS), f("amount", U128)),
    struct(
        "PendingClaim",
        f("config_id", ROOT32),
        f("leaf_count", U64),
        f("witness", d("ClaimWitness")),
        f("range", d("ScheduleRange")),
        f("relayer", opt(ROOT32)),
    ),
    struct("HolderState", f("holdings", vec(d("Holding"))), f("pending", opt(d("PendingClaim")))),
    struct("BurnRecord", f("config_id", ROOT32), f("note", d("Note"))),
]

B_ACCOUNTS = [
    {
        "name": "BridgeState",
        "type": enum(
            "BridgeState",
            tv("Config", d("ConfigState")),
            tv("Root", d("RootState")),
            tv("Nullifier", d("NullifierState")),
            tv("Holder", d("HolderState")),
            tv("Burn", d("BurnRecord")),
            tv("Token", d("TokenState")),
            docs="The state of every actor of the bridge program: config(cid), root(cid, n), nullifier(cid, chain, vault, N), holder(cid, account), burn(cid, H(note)), token(cid, token).",
        ),
    }
]


def bridge_idl():
    variants = B_TYPES[[t["name"] for t in B_TYPES].index("BridgeMessage")]["variants"]
    I = lambda *a, **k: instr(variants, *a, **k)
    pda = lambda tag, *args: {"seeds": [{"kind": "const", "value": tag}] + [{"kind": "arg", "path": a} for a in args]}
    CONFIG = lambda init=False, docs=None: acct("config", pda("config", "config_id"), init=init, docs=docs)
    TOKEN = lambda init=False, docs=None: acct("token_actor", pda("token", "config_id", "token"), init=init, docs=docs)
    ROOTA = lambda init=False, docs=None, writable=True: acct("root", pda("root", "config_id", "leaf_count"), init=init, docs=docs, writable=writable)
    NULL = lambda: acct("nullifier", pda("nullifier", "config_id", "evm_chain_id (u256 big-endian)", "vault", "nullifier"), init=True, docs="written once: a second Spend refuses with 250 ALREADY_CLAIMED")
    HOLDER = lambda docs=None: acct("holder", pda("holder", "config_id", "holder"), docs=docs)
    BURN = lambda: acct("burn", pda("burn", "config_id", "note_hash"), init=True, docs="burn(cid, H(note)), written once")
    OWN = lambda who: acct("own", None, signer=True, docs=f"{who}'s own account (the program's actor at it); signed")
    ANY = lambda docs: acct("receiver", None, writable=False, docs=docs)

    def RECORD(docs="the record the message reads"):
        """The attestation program's header(instance, slot): another program's actor, read only."""
        a = acct("attestation_record", pda("header", "instance_id", "record_slot"), writable=False, docs=docs + "; the attestation program's account (its PDA domain is ferrata-lez/v1/, see ferrata_attestation.idl.json)")
        a["owner"] = "ferrata_attestation"
        return a

    HOLDERS = lambda: acct("holder", pda("holder", "config_id", "holder"), docs="holder(cid, account) of each account credited (the destination, the fee recipient, the relayer), each listed once")
    instructions = [
        I(0, "burn", [OWN("the holder"), HOLDER("the root instead of the own actor when the denomination is debited from holder(cid, account)"), TOKEN(docs="admits the note: denomination, burn freezes and caps"), CONFIG(docs="Admit: the global burn freeze and bucket"), BURN()], "Debit one denomination and record the note once at burn(cid, H(note)); a top-level call or the root of a private execution."),
        I(1, "import_root", [CONFIG(), RECORD(), ROOTA(init=True, docs="written once at the attested leaf count")], "Attest the vault's slots 0 and 1 under the record at record_slot and write root(cid, leaf_count). Anyone may call; refused after Sunset."),
        I(2, "sunset", [OWN("the freeze authority"), CONFIG()], "The configuration's one-way Sunset: stops ImportRoot for good."),
        I(3, "echo", [ANY("any actor")], "Rule (A)'s echo nonce: accepted anywhere, changes nothing."),
        I(4, "init", [CONFIG(init=True, docs="cid computed from config; created: the transaction carries LEZ's PDA evidence for its seed"), RECORD()], "Attest the vault's slot 2 and write the configuration if it holds cid (the reciprocal pairing). Anyone may call."),
        I(5, "claim", [ROOTA(writable=False, docs="the imported root, the transaction's root actor"), TOKEN(), CONFIG(docs="Admit"), NULL(), HOLDERS()], "The plaintext claim, for a public destination; relayer receives the relay fee (None: the fee recipient)."),
        I(6, "claim_privately", [OWN("the claimant (meant to run privately)"), ROOTA(writable=False), TOKEN(), CONFIG(docs="Admit"), NULL(), HOLDERS()], "Read root(cid, n), verify the claim inside this execution, then Issue; a private destination must be this account.", public=False, private_owned=True),
        I(7, "read", [ANY("any actor of this program; replies StateReply")], "Reply with this actor's state."),
        I(8, "write_root", [ROOTA(init=True, docs="from config(cid) only")], "Internal: config(cid) -> root(cid, n), the verified root, written once.", internal=True),
        I(9, "spend", [NULL()], "Internal: token(cid, token) -> nullifier(cid, chain, vault, N): spend N once; mint to mint_to and each fee to its account.", internal=True),
        I(10, "mint", [HOLDER("from nullifier(cid, chain, vault, N) only")], "Internal: credit amount of token to holder.", internal=True),
        I(11, "register", [CONFIG(), RECORD(), TOKEN(init=True)], "Attest the vault's registry entry of entry.token, its fee schedules 0 and 1 and its leaf count under one record, and write token(cid, token)."),
        I(12, "import_fees", [CONFIG(), RECORD(), TOKEN()], "Attest the vault's fee schedule index of token (or its absence) and its leaf count, and extend token(cid, token)."),
        I(13, "write_token", [TOKEN(init=True, docs="from config(cid) only")], "Internal: the attested entry, written once.", internal=True),
        I(14, "extend_fees", [TOKEN(docs="from config(cid) only")], "Internal: an attested fee schedule (appended), or the attested absence of schedule index at a leaf count.", internal=True),
        I(15, "issue", [TOKEN(), CONFIG(docs="Admit"), NULL()], "A claimant -> token(cid, token) after verifying the deposit: the denomination, the fee schedule of the range, the mint freezes and caps; then Admit and Spend."),
        I(16, "admit", [CONFIG(docs="from token(cid, token) only")], "Internal: the global freeze and bucket of a mint or a burn of value units, at the configuration's attested time.", internal=True),
        I(17, "freeze", [OWN("the freeze authority"), CONFIG(docs="target when token is None"), TOKEN(docs="target when token is Some"), RECORD("the record whose execution time the target reads first")], "Freeze mints and/or burns of token (or the whole configuration) until until_ms (0 unfreezes), at most 30 days after the attested time."),
        I(18, "set_cap", [OWN("the admin"), CONFIG(docs="target when token is None"), TOKEN(docs="target when token is Some")], "Set the mint (or burn) bucket of token (or the global one), never below the floor."),
        I(19, "tick", [CONFIG(docs="target when token is None"), TOKEN(docs="target when token is Some"), RECORD("the record whose execution time the target advances to")], "Anyone: read the attestation program's record and advance the target's clock to its execution timestamp."),
        I(20, "attest_retired", [CONFIG(), RECORD("the record the vault's storage slot 37 (byte 11, retired) is proven under")], "Anyone: attest that the vault has retired (FerrataVault v5 slot 37, byte 11 = 1); afterwards anyone may send the 33-byte Sunset at config(cid)."),
        I(21, "propose_roles", [OWN("the admin"), CONFIG(), RECORD("a record no older than a day before the block")], "The admin proposes a freeze authority (one account or M of N) and up to three watchers, effective 14 days of attested time later."),
        I(22, "apply_roles", [CONFIG(), RECORD("a record at least 14 days after the proposal's")], "Anyone: apply the proposed roles once their 14-day notice has passed."),
        I(23, "cancel_roles", [OWN("the admin"), CONFIG()], "The admin cancels a pending role proposal."),
        I(24, "pause", [OWN("a watcher"), CONFIG(docs="target when token is None"), TOKEN(docs="target when token is Some"), RECORD("the record the 12 h pause runs from")], "A watcher freezes mints and/or burns until 12 h after the record, never shortening a longer freeze."),
        I(25, "cosigned", [OWN("each signer, in order (the call is forwarded along the signers' own actors)"), CONFIG(), TOKEN(docs="target of a token's Freeze")], "A multisig freeze authority's action (Freeze, or approving Sunset), signed by at least its threshold of distinct members in one transaction."),
        I(26, "transfer", [OWN("the holder"), HOLDER("the holder's holder(cid, account), debited when it is not a private holder"), TOKEN(docs="requires a multiple of the LEZ unit and credits holder(cid, to)"), HOLDER("holder(cid, to), credited")], "Move a balance to another holder; from a private holder the transaction shows only the Transfer to the token from an alias."),
        I(27, "credit", [HOLDER("holder(cid, to): from token(cid, token) only")], "Internal: token(cid, token) -> holder(cid, to), the transferred amount.", internal=True),
        I(28, "set_depth", [OWN("the admin"), CONFIG()], "The admin sets the inbound finality depth beyond Ethereum finality, in beacon slots (at most 216,000): ImportRoot under a record at slot s waits (242 FROZEN) until the configuration's clock has read a record at s + depth or later. Applies to imports from then on; imported roots stay valid."),
        I(29, "attest_listing", [CONFIG(), RECORD("the record the vault's listing slot of token (sha256(\"ferrata/vault/listing/v1\" || token): 1 delisted, 0 listed) is proven under"), TOKEN(docs="SetListing")], "Anyone: attest the vault's listing of a token (F.10); token(cid, token) then holds whether deposits of it are delisted. Claims, burns and transfers go on."),
        I(30, "set_listing", [TOKEN(docs="from config(cid) only")], "Internal: the attested listing, kept only if its record is newer than the token's last.", internal=True),
    ]
    return {
        "version": "0.1.0",
        "name": "ferrata_bridge",
        "spec": SPEL,
        "metadata": {"name": "ferrata_bridge", "version": "0.1.0"},
        "instruction_type": "ferrata_lez_bridge_wire::BridgeMessage",
        "docs": [
            "The bridge program of Ferrata on LEZ v0.4 (RFP-022). Hand-written IDL in SPEL's format (see ferrata_attestation.idl.json for why).",
            "Each instruction is a variant of the borsh enum named by instruction_type; discriminator is its one-byte variant index, followed by args in order.",
            "PDA seeds: SHA-256(\"ferrata-lez/v1/bridge/\" || tag || parts), u64 parts little-endian, the EVM chain id as a 32-byte big-endian word; the account is AccountId::for_public_pda(program, seed). holder(cid, account) hashes the configuration id, then the account's 32 bytes; nullifier(cid, chain, vault, N) the configuration id first (milestone B1: every account a configuration writes is keyed by it).",
            "Refusals exit with one byte, 240-255 (below). The attestation program's codes (ferrata_attestation.idl.json) can surface through an attested import.",
        ],
        "instructions": instructions,
        "accounts": B_ACCOUNTS,
        "types": B_TYPES,
        "errors": codes_from_macro(ROOT / "lez/bridge-wire/src/lib.rs", "pub mod code {"),
    }


def main():
    out = ROOT / "idl"
    for name, idl in (("ferrata_attestation", attestation_idl()), ("ferrata_bridge", bridge_idl())):
        path = out / f"{name}.idl.json"
        path.write_text(json.dumps(idl, indent=2) + "\n")
        print(f"wrote {path.relative_to(ROOT)}: {len(idl['instructions'])} instructions, {len(idl['types'])} types, "
              f"{len(idl['accounts'])} account types, {len(idl['errors'])} errors")


if __name__ == "__main__":
    sys.exit(main())
