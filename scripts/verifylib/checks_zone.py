"""Offline re-checks of the zone-bridge-rc3-multi run (EVIDENCE row 266).

Every expected number below is compared with a value rebuilt from the run's records, not copied
back out of its README.  The README is a second, independent comparison.  Where the README does
not contain the figures named by the evidence text, or where only the README records a figure,
the result says LIMIT instead of implying an independent recomputation.
"""
from __future__ import annotations

import csv
import hashlib
import importlib.util
import io
import json
import re
from pathlib import Path

from datetime import datetime

from . import groth16, sigs
from .core import check, fail_if

RUN = "e2e/runs/zone-bridge-rc3-multi"
README = f"{RUN}/README.md"
CHECKPOINT = f"{RUN}/checkpoint"
PROVER = f"{RUN}/prover"
FIXTURE = "vault/test/real/fixtures/zone-bridge-rc3-multi.json"
PARAMS = f"{RUN}/params.json"

COMMON = {"evidence": (266,), "mode": "offline", "tier": "quick"}
EVIDENCE_DOC = "docs/EVIDENCE.md"
STATUS_DOC = "docs/requirements-status.md"
DOCS = (EVIDENCE_DOC, STATUS_DOC)


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _json(ctx, rel: str):
    return json.loads(_text(ctx, rel))


def _hex(ctx, rel: str) -> bytes:
    return bytes.fromhex(_text(ctx, rel).strip().removeprefix("0x"))


def _tsv(ctx, rel: str) -> list[dict]:
    return list(csv.DictReader(io.StringIO(_text(ctx, rel)), delimiter="\t"))


def _jsonl(ctx, rel: str) -> list[dict]:
    return [json.loads(line) for line in _text(ctx, rel).splitlines() if line.strip()]


def _expect(problems: list, condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def _same(problems: list, source: str, name: str, actual, stated) -> None:
    _expect(problems, actual == stated,
            f"{source}: {name}: recomputed {actual!r}, stated/recorded {stated!r}")


def _readme(problems: list, readme: str, fragment: str) -> None:
    _expect(problems, fragment in readme, f"{README}: stated figure/text {fragment!r} is absent")


def _figure(problems: list, readme: str, source: str, name: str, actual, stated,
            written: str | None = None) -> None:
    _same(problems, source, name, actual, stated)
    if written is not None:
        _readme(problems, readme, written)


def _run(rel: str) -> str:
    return f"{RUN}/{rel}"


def _hex32(value: str) -> str:
    return value.lower().removeprefix("0x")


def _live_keys(ctx) -> list:
    """The channel's accredited keys at the run's end, in the channel's order (zone/channel-end.json)."""
    return [_hex32(key) for key in _json(ctx, _run("zone/channel-end.json"))["accredited_keys"]]


def _key_set_hash(keys: list, posting_timeframe: int) -> bytes:
    """attestor-core's key_set_hash: SHA-256("ferrata/keyset/v2" || n u32 || postingTimeframe u32 || keys), big-endian."""
    data = b"ferrata/keyset/v2" + len(keys).to_bytes(4, "big") + posting_timeframe.to_bytes(4, "big")
    return hashlib.sha256(data + b"".join(bytes.fromhex(key) for key in keys)).digest()


def _deployment_prefix(tag: bytes, deployment: dict) -> bytes:
    """attestor-core's Deployment::prefix: tag || evmChainId (uint256) || vault || lezConfigId || channelId."""
    chain = deployment["evm_chain_id"]
    chain = int(chain, 16) if isinstance(chain, str) else int(chain)
    return (tag + chain.to_bytes(32, "big") + bytes.fromhex(_hex32(deployment["vault"]))
            + bytes.fromhex(_hex32(deployment["lez_config_id"])) + bytes.fromhex(_hex32(deployment["channel_id"])))


def _attestation_verifies(deployment: dict, key_set_hash: bytes, entry: dict, tip_hash: str, final_msg_id: str):
    """(certificate verifies, signature verifies): attestor-core's verify_certificate and verify_attestation, with
    the pure-Python Ed25519 of verifylib/sigs.py (RFC 8032; verify_strict's refusal of small-order keys not repeated)."""
    channel_key = bytes.fromhex(_hex32(entry["channel_key"]))
    att_key = bytes.fromhex(_hex32(entry["attestation_key"]))
    certificate = _deployment_prefix(b"ferrata/attest-key/v1", deployment) + att_key
    message = (_deployment_prefix(b"ferrata/final-tier/v2", deployment) + key_set_hash
               + int(entry["tip_id"]).to_bytes(8, "big") + bytes.fromhex(_hex32(tip_hash))
               + bytes.fromhex(_hex32(final_msg_id)))
    return (sigs.ed25519_verify(channel_key, certificate, bytes.fromhex(_hex32(entry["certificate"]))),
            sigs.ed25519_verify(att_key, message, bytes.fromhex(_hex32(entry["signature"]))))


def _doc_rows(ctx) -> dict:
    """The texts that state this run's figures: EVIDENCE row 266 and the status rows that cite it."""
    out = {}
    for line in _text(ctx, EVIDENCE_DOC).splitlines():
        if line.startswith("| 266 | "):
            out["EVIDENCE row 266"] = line
    rfp = None
    for line in _text(ctx, STATUS_DOC).splitlines():
        if line.startswith("## RFP-02"):
            rfp = line[3:10]
        for rid in ("F.7", "R.6", "P.6"):
            if rfp == "RFP-022" and line.startswith(f"| {rid} | "):
                out[f"RFP-022 {rid}"] = line
    return out


def _documents(ctx, problems: list, where: str, fragments) -> None:
    """Each figure, as the documents write it, must be in the text that states it (`where`: a key of _doc_rows)."""
    text = _doc_rows(ctx).get(where)
    if text is None:
        problems.append(f"{where} not found in {EVIDENCE_DOC} / {STATUS_DOC}")
        return
    for fragment in fragments:
        _expect(problems, fragment in text, f"{where}: stated figure {fragment!r} is absent")


@check(id="zone-rc3-multi-inscriptions", family="record", rows=("RFP-022 F.7",), **COMMON,
       title="Four channel keys and all 165 final inscriptions",
       what="the accredited key set and threshold, the contiguous inscription ledger, its cross-check to blocks.tsv, "
            "and the native verifier's per-key totals (46/46/38/35)",
       against="zone/channel-end.json, checkpoint/inscriptions.jsonl, checkpoint/blocks.tsv, "
               "checkpoint/realblocks-crown.txt and the run README",
       inputs=(README, _run("zone/channel-end.json"), f"{CHECKPOINT}/inscriptions.jsonl",
               f"{CHECKPOINT}/blocks.tsv", f"{CHECKPOINT}/realblocks-crown.txt", *DOCS))
def zone_inscriptions(ctx):
    problems = []
    readme = _text(ctx, README)
    channel_rel = _run("zone/channel-end.json")
    inscriptions_rel = f"{CHECKPOINT}/inscriptions.jsonl"
    blocks_rel = f"{CHECKPOINT}/blocks.tsv"
    report_rel = f"{CHECKPOINT}/realblocks-crown.txt"
    channel = _json(ctx, channel_rel)
    keys = channel["accredited_keys"]
    _figure(problems, readme, channel_rel, "accredited keys", len(keys), 4,
            "four keys with threshold 3")
    _expect(problems, len(set(keys)) == len(keys), f"{channel_rel}: duplicate accredited keys")
    _figure(problems, readme, channel_rel, "configuration threshold", channel["configuration_threshold"], 3)
    _figure(problems, readme, channel_rel, "posting timeframe", channel["posting_timeframe"], 60,
            "posting timeframe 60 slots")

    inscriptions = _jsonl(ctx, inscriptions_rel)
    blocks = _tsv(ctx, blocks_rel)
    block_numbers = [int(row["block"]) for row in inscriptions]
    _figure(problems, readme, inscriptions_rel, "final inscriptions", len(inscriptions), 165,
            "165 final inscriptions")
    _figure(problems, readme, inscriptions_rel, "inscription block range",
            (min(block_numbers), max(block_numbers)), (2, 166), "2..=166")
    _expect(problems, block_numbers == list(range(2, 167)),
            f"{inscriptions_rel}: blocks are not exactly the unique contiguous range 2..=166")
    _same(problems, blocks_rel, "one block row per inscription", len(blocks), len(inscriptions))
    by_block = {int(row["block"]): row for row in blocks}
    fields = (("msg_id", "msg_id"), ("node_block", "node_block"), ("slot", "node_slot"),
              ("tx_hash", "tx_hash"), ("payload_bytes", "inscribed_bytes"))
    for row in inscriptions:
        block = by_block.get(row["block"])
        _expect(problems, block is not None, f"{inscriptions_rel}: block {row['block']} absent from blocks.tsv")
        if block is None:
            continue
        for inscription_key, block_key in fields:
            _expect(problems, str(row[inscription_key]) == block[block_key],
                    f"{inscriptions_rel}: block {row['block']} {inscription_key} differs from {blocks_rel}")
        try:
            signature = bytes.fromhex(row["signature"])
            payload_hash = bytes.fromhex(row["payload_sha256"])
        except ValueError as e:
            problems.append(f"{inscriptions_rel}: block {row['block']}: malformed hex: {e}")
        else:
            _expect(problems, len(signature) == 64 and len(payload_hash) == 32,
                    f"{inscriptions_rel}: block {row['block']}: signature/payload digest length changed")

    ordered_blocks = sorted(blocks, key=lambda row: int(row["block"]))
    for previous, row in zip(ordered_blocks, ordered_blocks[1:]):
        parent = row["parent_msg_id"]
        _expect(problems, parent == previous["msg_id"],
                f"{blocks_rel}: block {row['block']}'s parent {parent[:16]}… is not block {previous['block']}'s message "
                f"{previous['msg_id'][:16]}…")

    report = _text(ctx, report_rel)
    span = re.search(r"final inscriptions of LEZ blocks (\d+)\.\.=(\d+)", report)
    native = re.search(r"channel's (\d+) key\(s\) \(([^)]*)\): all (\d+) accepted; "
                       r"channel tip message = block \d+'s: true", report)
    _expect(problems, span is not None and native is not None and "verify_strict" in report,
            f"{report_rel}: native range/key/acceptance report is not parseable")
    reported_counts = {}
    accepted = key_count = 0
    if native:
        key_count, count_text, accepted_text = native.groups()
        key_count, accepted = int(key_count), int(accepted_text)
        for prefix, count in re.findall(r"([0-9a-f]{8})… (\d+)", count_text):
            matches = [key for key in keys if key.startswith(prefix)]
            _expect(problems, len(matches) == 1,
                    f"{report_rel}: signer prefix {prefix} names {len(matches)} channel keys")
            if len(matches) == 1:
                reported_counts[matches[0]] = int(count)
    _same(problems, report_rel, "native report range", tuple(map(int, span.groups())) if span else None, (2, 166))
    _figure(problems, readme, report_rel, "native accepted inscriptions", accepted, 165)
    _same(problems, report_rel, "native channel key count", key_count, 4)
    expected_counts = {prefix: count for prefix, count in
                       (("6462ecf1", 46), ("90a548e0", 46), ("99273f3e", 38), ("fce0d0fa", 35))}
    actual_by_prefix = {next((prefix for prefix in expected_counts if key.startswith(prefix)), key[:8]): count
                        for key, count in reported_counts.items()}
    _figure(problems, readme, report_rel, "per-key native counts", actual_by_prefix, expected_counts,
            "46, 46, 38 and 35")
    _same(problems, report_rel, "per-key counts cover the channel", len(reported_counts), 4)
    _same(problems, report_rel, "per-key counts sum", sum(reported_counts.values()), len(inscriptions))
    _documents(ctx, problems, "EVIDENCE row 266", ("165 final inscriptions of LEZ blocks 2..=166 by 4 keys (46/46/38/35)",
                                                   "the channel accrediting 4 keys with threshold 3"))
    fail_if(problems, "zone inscription/channel records differ")
    ordered = [reported_counts.get(key, 0) for key in keys]
    return [f"{inscriptions_rel}: {len(inscriptions)} unique inscriptions for blocks {block_numbers[0]}..="
            f"{block_numbers[-1]}, cross-checked field by field with {blocks_rel}, each block's parent the message of "
            "the block before it (block 2's parent, the channel's earlier inscription, is not in the record)",
            f"{channel_rel}: four distinct accredited keys, threshold {channel['configuration_threshold']}, "
            f"posting timeframe {channel['posting_timeframe']}; native report totals by channel order {ordered}",
            f"{report_rel}: all {accepted} accepted by its parse/re-hash/link/framing/BLAKE2b/verify_strict pass",
            "LIMIT: inscriptions.jsonl has signatures and payload hashes but neither signer ids nor signed payloads; "
            "the per-signer assignment and signature verification are the native realblocks-crown report, not "
            "independently repeated in Python"]


@check(id="zone-rc3-multi-checkpoint", family="record", rows=("RFP-022 F.7",), **COMMON,
       title="Checkpoint contents, three-of-four attestations and executor journal",
       what="136 blocks and 287 transactions rebuilt from blocks.tsv, burn/import counts from burns.json, the exact "
            "three final attestations and omitted fourth, and executor cycles/journal integrity",
       against="checkpoint records, all four zone/attestor-*/final.json files and the run README",
       inputs=(README, f"{CHECKPOINT}/blocks.tsv", f"{CHECKPOINT}/burns.json",
               f"{CHECKPOINT}/zone-28-164.json", f"{CHECKPOINT}/attestor.json", f"{CHECKPOINT}/exec.json",
               f"{CHECKPOINT}/zone-28-164.input", PARAMS, *tuple(_run(f"zone/attestor-{i}/final.json") for i in range(4)),
               _run("zone/channel-end.json"), _run("identity.json"), *DOCS))
def zone_checkpoint(ctx):
    problems = []
    readme = _text(ctx, README)
    meta_rel = f"{CHECKPOINT}/zone-28-164.json"
    blocks_rel = f"{CHECKPOINT}/blocks.tsv"
    burns_rel = f"{CHECKPOINT}/burns.json"
    attestor_rel = f"{CHECKPOINT}/attestor.json"
    exec_rel = f"{CHECKPOINT}/exec.json"
    meta = _json(ctx, meta_rel)
    rows = _tsv(ctx, blocks_rel)
    segment = [row for row in rows if meta["first_block"] <= int(row["block"]) <= meta["tip"]]
    block_count = len(segment)
    transactions = sum(int(row["transactions"]) for row in segment)
    block_bytes = sum(int(row["inscribed_bytes"]) for row in segment)
    _figure(problems, readme, blocks_rel, "checkpoint blocks", block_count, 136, "136 blocks")
    _figure(problems, readme, blocks_rel, "checkpoint transactions", transactions, 287, "287 transactions")
    _expect(problems, [int(row["block"]) for row in segment] == list(range(29, 165)),
            f"{blocks_rel}: checkpoint segment is not exactly 29..=164")
    _same(problems, meta_rel, "recorded blocks", meta["blocks"], block_count)
    _same(problems, meta_rel, "recorded transactions", meta["transactions"], transactions)
    _same(problems, meta_rel, "block bytes", meta["block_bytes"], block_bytes)

    burns = _json(ctx, burns_rel)
    burn_count = len(burns["burns"])
    import_rows = sorted(burns["imports"], key=lambda row: row["block"])
    imported_leaves = import_rows[-1]["leaf_count"] if import_rows else 0
    _figure(problems, readme, burns_rel, "burns", burn_count, 2, "two burns, two imported leaves")
    _figure(problems, readme, burns_rel, "imported leaves", imported_leaves, 2)
    _same(problems, meta_rel, "burn count", meta["burn_count"], burn_count)
    _same(problems, meta_rel, "imported leaf count", meta["imported_leaf_count"], imported_leaves)
    _same(problems, meta_rel, "burn root", meta["burn_root"], burns["burn_root"])
    _same(problems, burns_rel, "burn indices", [row["burn_index"] for row in burns["burns"]], [0, 1])

    attestor = _json(ctx, attestor_rel)
    final_rels = [_run(f"zone/attestor-{i}/final.json") for i in range(4)]
    finals = [_json(ctx, rel) for rel in final_rels]
    by_key = {"0x" + row["channel_key"].removeprefix("0x"): (rel, row) for rel, row in zip(final_rels, finals)}
    used = attestor["attestations"]
    used_keys = [row["channel_key"] for row in used]
    _figure(problems, readme, attestor_rel, "attestations used", len(used), 3,
            "three of the four attestations")
    _same(problems, attestor_rel, "distinct attestors used", len(set(used_keys)), len(used_keys))
    _figure(problems, readme, attestor_rel, "n", attestor["n"], 4, "n = 4, t = 3")
    _figure(problems, readme, attestor_rel, "t", attestor["t"], 3)
    _figure(problems, readme, attestor_rel, "attested tip", attestor["tip_id"], 164, "tip 164")
    _same(problems, meta_rel, "n/t", (meta["n"], meta["t"]), (attestor["n"], attestor["t"]))
    _same(problems, meta_rel, "attestations count", meta["attestations"], len(used))
    for entry in used:
        found = by_key.get(entry["channel_key"])
        _expect(problems, found is not None, f"{attestor_rel}: used key {entry['channel_key']} has no final.json among "
                                             "the four attestors'")
        if found:
            final_rel, final = found
            for field in ("attestation_key", "certificate", "channel_key", "signature"):
                _expect(problems, entry[field].removeprefix("0x") == final[field].removeprefix("0x"),
                        f"{attestor_rel}: {field} differs from {final_rel}")
            _expect(problems, entry["tip_id"] == final["tip_id"] == 164,
                    f"{attestor_rel}/{final_rel}: attestation is not of tip 164")
    # The key set is the channel's (zone/channel-end.json), not the four final.json files': every attestation used is by
    # an accredited channel key, its key certified for this deployment by that channel key and its tip signed under the
    # channel's key-set hash (attestor-core's messages, Ed25519).
    live = _live_keys(ctx)
    channel = _json(ctx, _run("zone/channel-end.json"))
    _same(problems, attestor_rel, "the collector's key set is the channel's accredited keys, in order",
          [_hex32(key) for key in attestor["key_set"]], live)
    key_set_hash = _key_set_hash(live, int(channel["posting_timeframe"]))
    _same(problems, attestor_rel, "the key-set hash (ferrata/keyset/v2 over the channel's keys)",
          _hex32(attestor["key_set_hash"]), key_set_hash.hex())
    identity = _json(ctx, _run("identity.json"))
    deployment = attestor["deployment"]
    _same(problems, attestor_rel, "the deployment's chain, vault and configuration",
          (int(deployment["evm_chain_id"]), _hex32(deployment["vault"]), _hex32(deployment["lez_config_id"])),
          (int(identity["chainId"]), _hex32(identity["vault"]), _hex32(_json(ctx, PARAMS)["lezConfigId"])))
    verified = 0
    for entry in used:
        key = _hex32(entry["channel_key"])
        _expect(problems, key in live, f"{attestor_rel}: an attestation by {key[:8]}…, not an accredited key of "
                                       f"{_run('zone/channel-end.json')}")
        certified, signed = _attestation_verifies(deployment, key_set_hash, entry, attestor["tip_hash"],
                                                  attestor["final_msg_id"])
        _expect(problems, certified, f"{attestor_rel}: {key[:8]}…'s certificate of its attestation key does not verify")
        _expect(problems, signed, f"{attestor_rel}: {key[:8]}…'s attestation of tip {entry['tip_id']} does not verify")
        verified += certified and signed and key in live
    for final_rel, final in zip(final_rels, finals):
        _same(problems, final_rel, "its key-set hash", _hex32(final["key_set_hash"]), key_set_hash.hex())
        _expect(problems, _hex32(final["channel_key"]) in live, f"{final_rel}: not an accredited key's attestor")
    _same(problems, attestor_rel, "attestations verified (certificate by an accredited channel key, tip signed by its "
                                  "attestation key)", verified, 3)
    omitted = sorted(set(by_key) - set(used_keys))
    _same(problems, attestor_rel, "one attestor omitted", len(omitted), 1)
    _expect(problems, len(omitted) == 1 and omitted[0].startswith("0x90a548e0"),
            f"{attestor_rel}: omitted key is {omitted}, stated key 90a548e0… ({README})")
    _readme(problems, readme, "90a548e0…")

    execution = _json(ctx, exec_rel)
    journal = bytes.fromhex(meta["journal"].removeprefix("0x"))
    _figure(problems, readme, exec_rel, "user cycles", execution["user_cycles"], 22_149_862,
            "22,149,862 user cycles")
    _expect(problems, execution["journal_equals_native"] is True,
            f"{exec_rel}: executor journal does not equal the native journal")
    _same(problems, exec_rel, "journal byte length", execution["journal_bytes"], len(journal))
    _same(problems, meta_rel, "journal SHA-256", hashlib.sha256(journal).hexdigest(), meta["journal_sha256"])
    _same(problems, exec_rel, "input byte length", execution["input_bytes"],
          ctx.path(f"{CHECKPOINT}/zone-28-164.input").stat().st_size)
    params = _json(ctx, PARAMS)
    _same(problems, exec_rel, "checkpoint image id", execution["image_id"], params["checkpointImageIdB"])
    _documents(ctx, problems, "EVIDENCE row 266", ("136 blocks, 287 transactions, 22,149,862 user cycles",
                                                   "three of four attestations"))
    _documents(ctx, problems, "RFP-022 F.7", ("three of four keys",))
    fail_if(problems, "zone checkpoint records differ")
    return [f"{blocks_rel}: {block_count} contiguous blocks and {transactions:,} transactions; {burns_rel}: "
            f"{burn_count} burns and {imported_leaves} final imported leaves",
            f"{attestor_rel} + four final.json records: {len(used)} distinct tip-164 attestations used, {verified} of "
            f"them verified (Ed25519: each attestation key certified by an accredited channel key of "
            f"{_run('zone/channel-end.json')}, tip 164 signed under the channel's key-set hash); "
            f"omitted {omitted[0][:10] if omitted else '-'}…; n={attestor['n']}, t={attestor['t']}",
            f"{exec_rel}: {execution['user_cycles']:,} user cycles; native-journal equality asserted, journal "
            f"length/SHA-256 and input length recomputed"]


REFUSAL_CASES = {
    "below-t": "Core(InsufficientAttestations { have: 2, need: 3 })",
    "outsider": "Core(NotAccredited)",
    "repeated": "Core(DuplicateAttestor)",
    "stale-tip": "Core(NotADescendant { tip_id: 29 })",
}


@check(id="zone-rc3-multi-refusals", family="record", rows=("RFP-022 F.7",), **COMMON,
       title="Four native/guest refusals with identical named errors",
       what="all four refusal cases, including their structural causes, the native error, the repeated native error "
            "in the guest transcript and the guest's own error",
       against="checkpoint/refusals/native.txt, guest.txt and each case's JSON metadata, plus the run README",
       inputs=(README, f"{CHECKPOINT}/refusals/native.txt", f"{CHECKPOINT}/refusals/guest.txt",
               *tuple(f"{CHECKPOINT}/refusals/zone-28-164-refuse-{case}.json" for case in REFUSAL_CASES),
               _run("zone/channel-end.json"), *DOCS))
def zone_refusals(ctx):
    problems = []
    readme = _text(ctx, README)
    native_rel = f"{CHECKPOINT}/refusals/native.txt"
    guest_rel = f"{CHECKPOINT}/refusals/guest.txt"
    native, guest_native, guest = {}, {}, {}
    for line in _text(ctx, native_rel).splitlines():
        match = re.fullmatch(r"(\S+): refused: (Core\(.*\))", line)
        _expect(problems, match is not None, f"{native_rel}: malformed native line {line!r}")
        if match:
            native[match.group(1)] = match.group(2)
    for line in _text(ctx, guest_rel).splitlines():
        match = re.fullmatch(r"(\S+): native refused: (Core\(.*\)); guest checkpoint refused: (Core\(.*\))", line)
        _expect(problems, match is not None, f"{guest_rel}: malformed native/guest line {line!r}")
        if match:
            guest_native[match.group(1)] = match.group(2)
            guest[match.group(1)] = match.group(3)
    expected = {f"zone-28-164-refuse-{case}": error for case, error in REFUSAL_CASES.items()}
    _same(problems, native_rel, "four native errors", native, expected)
    _same(problems, guest_rel, "native errors repeated by guest harness", guest_native, native)
    _same(problems, guest_rel, "guest errors equal native errors", guest, native)
    live = _live_keys(ctx)
    for case, error in REFUSAL_CASES.items():
        rel = f"{CHECKPOINT}/refusals/zone-28-164-refuse-{case}.json"
        metadata = _json(ctx, rel)
        _same(problems, rel, "the case's key set is the channel's accredited keys", [_hex32(k) for k in metadata["keys"]],
              live)
        _same(problems, rel, "expected refusal", metadata["expect_refusal"], error)
        _same(problems, rel, "native record", metadata["native"], f"refused: {error}")
        attestations = metadata["attestations"]
        keys = [_hex32(row["channel_key"]) for row in attestations]
        if case == "below-t":
            _same(problems, rel, "attestations below threshold", len(attestations), 2)
        elif case == "outsider":
            outside = [key for key in keys if key not in live]
            _same(problems, rel, "attestations by a key outside the channel's set", len(outside), 1)
        else:
            _same(problems, rel, "every attestation by an accredited key", [k for k in keys if k not in live], [])
        if case == "repeated":
            _same(problems, rel, "one repeated attestor", (len(attestations), len(set(keys))), (3, 2))
        if case == "stale-tip":
            _same(problems, rel, "stale/tip ids", sorted(row["tip_id"] for row in attestations), [29, 164, 164])
        _readme(problems, readme, error.removeprefix("Core(").removesuffix(")"))
    _documents(ctx, problems, "RFP-022 F.7", ("two of four attestations, a key outside the set, one attestor twice and a "
                                              "stale round refused natively and by the guest",))
    fail_if(problems, "zone native/guest refusal records differ")
    return [f"four cases rebuilt from {native_rel} and {guest_rel}: below-t, outsider, repeated, stale-tip",
            "for every case: metadata cause, native error, guest record of that native error and guest error all equal",
            "errors: InsufficientAttestations(2/3), NotAccredited, DuplicateAttestor, NotADescendant(tip 29)"]


# (receipt directory, statement, the client's wait for the answer in s, the prover's own wall time in s, 0.1 s): the
# client's `seconds` runs from its own (re)start of the job to the answer (ferrata-bridge-prover: time.time() - t0),
# so the resumed checkpoint's 249.7 s is counted from the restart, not from the spawn; `prover.wall_s` is the
# prover's run on the GPU.
PROOF_JOBS = {
    "0-checkpoint": (f"{CHECKPOINT}/receipts", f"{CHECKPOINT}/zone-28-164.json", 249.7, 92.7),
    "1-release-private-1": (f"{RUN}/releases/receipts/release-private-1",
                            f"{RUN}/releases/release-private-1.json", 569.1, 5.8),
    "1-release-public-0": (f"{RUN}/releases/receipts/release-public-0",
                           f"{RUN}/releases/release-public-0.json", 179.9, 8.1),
}


@check(id="zone-rc3-multi-proofs", family="seal", rows=("RFP-022 F.7", "RFP-022 R.6"), **COMMON,
       title="Three resumed/journaled GPU proofs, fixture identity and Groth16 verification",
       what="the three-job prover ledger, exactly-once results, SIGKILL/resume call identity and one attempt, proof "
            "durations, 260-byte seals with selector 0xe71e6620, statement journals, forge fixture equality and all "
            "three Groth16 proofs",
       against="prover/calls.tsv, events.jsonl, done answers and summary; receipt metadata and statements; the forge "
               "fixture; RISC Zero 3.1.0 in verifylib/groth16.py; the run README",
       inputs=(README, f"{PROVER}/calls.tsv", f"{PROVER}/events.jsonl", f"{PROVER}/summary.txt", PARAMS,
               FIXTURE, *tuple(path for job, (receipt, statement, _, _) in PROOF_JOBS.items()
                               for path in (f"{PROVER}/done/{job}.json", f"{PROVER}/requests/done/{job}.json",
                                            f"{receipt}/seal.hex", f"{receipt}/journal.hex", f"{receipt}/prover.json",
                                            statement)), *DOCS))
def zone_proofs(ctx):
    problems = []
    readme = _text(ctx, README)
    calls_rel = f"{PROVER}/calls.tsv"
    calls = _tsv(ctx, calls_rel)
    _figure(problems, readme, calls_rel, "proof jobs", len(calls), 3, "the checkpoint and both releases")
    _same(problems, calls_rel, "job names", [row["request"] for row in calls], list(PROOF_JOBS))
    _same(problems, calls_rel, "jobs written once", len({row["request"] for row in calls}), len(calls))
    for row in calls:
        job = row["request"]
        _expect(problems, job in PROOF_JOBS, f"{calls_rel}: unknown job {job}")
        if job not in PROOF_JOBS:
            continue
        seconds = float(row["seconds"])
        _figure(problems, readme, calls_rel, f"{job} GPU seconds", seconds, PROOF_JOBS[job][2])
        _expect(problems, row["call id spawned"] == row["call id answered"],
                f"{calls_rel}: {job}: spawned/answered call ids differ")
        _same(problems, calls_rel, f"{job} attempts", int(row["attempts"]), 1)
        _same(problems, calls_rel, f"{job} resumed", row["resumed"] == "true", job == "0-checkpoint")
        _expect(problems, row["ok"] == "true", f"{calls_rel}: {job} not ok")

    events_rel = f"{PROVER}/events.jsonl"
    events = _jsonl(ctx, events_rel)
    spawned = [row for row in events if row.get("event") == "spawned"]
    resumed = [row for row in events if row.get("event") == "resumed"]
    answers = [row for row in events if "seconds" in row]
    _same(problems, events_rel, "spawn/answer counts", (len(spawned), len(answers)), (3, 3))
    _same(problems, events_rel, "resume count", len(resumed), 1)
    call_rows = {row["request"]: row for row in calls}
    for row in spawned + resumed + answers:
        name = row.get("name")
        recorded = call_rows.get(name)
        _expect(problems, recorded is not None, f"{events_rel}: event for unknown request {name}")
        if recorded:
            _expect(problems, row["call_id"] == recorded["call id spawned"],
                    f"{events_rel}: {name}: call id differs from calls.tsv")
            _same(problems, events_rel, f"{name} event attempt(s)", row.get("attempt", row.get("attempts")), 1)
    _same(problems, events_rel, "resumed request", [row["name"] for row in resumed], ["0-checkpoint"])
    _same(problems, events_rel, "one answer for each job", sorted(row.get("name") for row in answers), sorted(PROOF_JOBS))
    for row in answers:
        name = row.get("name")
        if name not in PROOF_JOBS:
            continue
        done = _json(ctx, f"{PROVER}/done/{name}.json")
        _expect(problems, row.get("ok") is True and row.get("error") is None,
                f"{events_rel}: {name}: the answer is not a success (ok {row.get('ok')!r}, error {row.get('error')!r})")
        for field in ("kind", "seconds", "resumed", "attempts", "call_id"):
            _same(problems, events_rel, f"{name} {field} (as the done record)", row.get(field), done.get(field))
        _same(problems, events_rel, f"{name} seconds (as calls.tsv)", row.get("seconds"),
              float(call_rows[name]["seconds"]) if name in call_rows else None)
        _expect(problems, str(row.get("job", "")).endswith(f"/inbox/{name}.json"),
                f"{events_rel}: {name}: its job is not the inbox's {name}.json")

    summary_rel = f"{PROVER}/summary.txt"
    summary = _text(ctx, summary_rel)
    checkpoint_call = call_rows.get("0-checkpoint", {}).get("call id spawned", "")
    summary_fragments = (
        f"client 1 killed (SIGKILL) while it waited for call {checkpoint_call} (0-checkpoint)",
        f"0-checkpoint: answered by the call client 1 spawned ({checkpoint_call}), resumed by client 2, one attempt",
        "PROVER KILL-RESUME OK",
    )
    for fragment in summary_fragments:
        _expect(problems, fragment in summary, f"{summary_rel}: missing {fragment!r}")
    _expect(problems, summary.count("killed (SIGKILL)") == 1,
            f"{summary_rel}: SIGKILL count is not one")
    # The documents say the client was killed 8 s after it started: the two lines' clock times.
    started = re.search(r"^\[(\d\d):(\d\d):(\d\d)\] client 1 started", summary, re.M)
    killed = re.search(r"^\[(\d\d):(\d\d):(\d\d)\] client 1 killed \(SIGKILL\)", summary, re.M)
    secs = (lambda m: int(m[1]) * 3600 + int(m[2]) * 60 + int(m[3]))
    _same(problems, summary_rel, "seconds from the client's start to its kill",
          secs(killed) - secs(started) if started and killed else None, 8)
    for row in calls:
        _expect(problems, f"{row['request']}" in summary and f"{float(row['seconds']):.1f}" in summary,
                f"{summary_rel}: job/duration row for {row['request']} absent")

    params = _json(ctx, PARAMS)
    fixture = _json(ctx, FIXTURE)
    fixture_releases = {row["name"]: row for row in fixture["releases"]}
    verified = []
    for job, (receipt_dir, statement_rel, seconds, wall) in PROOF_JOBS.items():
        done_rel = f"{PROVER}/done/{job}.json"
        mirror_rel = f"{PROVER}/requests/done/{job}.json"
        done = _json(ctx, done_rel)
        _same(problems, mirror_rel, "one identical completed answer", _text(ctx, mirror_rel), _text(ctx, done_rel))
        statement = _json(ctx, statement_rel)
        journal = bytes.fromhex(statement["journal"].removeprefix("0x"))
        seal = _hex(ctx, f"{receipt_dir}/seal.hex")
        receipt_journal = _hex(ctx, f"{receipt_dir}/journal.hex")
        prover = _json(ctx, f"{receipt_dir}/prover.json")
        _same(problems, done_rel, "journal equals statement", done["journal"].removeprefix("0x"), journal.hex())
        _same(problems, f"{receipt_dir}/journal.hex", "journal equals statement", receipt_journal, journal)
        _same(problems, done_rel, "seal equals receipt", bytes.fromhex(done["seal"].removeprefix("0x")), seal)
        _same(problems, f"{receipt_dir}/seal.hex", "seal length", len(seal), 260)
        _same(problems, f"{receipt_dir}/seal.hex", "RISC Zero selector", seal[:4].hex(), "e71e6620")
        expected_image = params["checkpointImageIdB"] if job == "0-checkpoint" else params["releaseImageId"]
        image = bytes.fromhex(done["image_id"].removeprefix("0x"))
        _same(problems, done_rel, "image id", done["image_id"], expected_image)
        for field in ("name", "kind", "image_id", "ok", "seconds", "call_id", "attempts", "resumed", "prover"):
            _same(problems, f"{receipt_dir}/prover.json", f"receipt/done {field}", prover[field], done[field])
        _same(problems, done_rel, "the client's wait (s)", done["seconds"], seconds)
        _same(problems, done_rel, "the prover's own wall time on the GPU (s, to 0.1)",
              round(float(done["prover"]["wall_s"]), 1), wall)
        ok, release, why = groth16.verify_seal(seal, image, hashlib.sha256(journal).digest())
        _expect(problems, ok and release == "3.1.0",
                f"{receipt_dir}/seal.hex: Groth16 verification for its image id and statement journal failed: "
                f"{why or release}")
        verified.append((job, len(seal)))

        if job == "0-checkpoint":
            _same(problems, FIXTURE, "checkpoint image id", fixture["imageId"], expected_image)
            _same(problems, FIXTURE, "checkpoint journal", fixture["journal"].removeprefix("0x"), journal.hex())
            _same(problems, FIXTURE, "checkpoint seal", fixture["seal"].removeprefix("0x"), seal.hex())
            _same(problems, FIXTURE, "checkpoint journal SHA-256", fixture["journalSha256"].removeprefix("0x"),
                  hashlib.sha256(journal).hexdigest())
        else:
            name = job.removeprefix("1-release-")
            release_fixture = fixture_releases.get(name)
            _expect(problems, release_fixture is not None, f"{FIXTURE}: release {name} absent")
            if release_fixture:
                _same(problems, FIXTURE, f"{name} image id", fixture["releaseImageId"], expected_image)
                _same(problems, FIXTURE, f"{name} seal", release_fixture["seal"].removeprefix("0x"), seal.hex())
                # Forge constructs the release journal from these request fields, so they identify its journal too.
                for fixture_field, statement_field in (("burnRoot", "burn_root"), ("nullifier", "nullifier"),
                                                       ("recipient", "recipient"), ("relayer", "relayer"),
                                                       ("relayFee", "relay_fee"), ("token", "token")):
                    _same(problems, FIXTURE, f"{name} {fixture_field}", release_fixture[fixture_field].lower(),
                          str(statement[statement_field]).lower())
                _same(problems, FIXTURE, f"{name} denomination", release_fixture["denomIndex"], 0)
    _same(problems, FIXTURE, "two release statements", len(fixture_releases), 2)
    _readme(problems, readme, "260-byte Groth16 seal")
    _readme(problems, readme, "0xe71e6620")
    _documents(ctx, problems, "EVIDENCE row 266", ("the prover's own wall time on the GPU 92.7 s for the checkpoint, 5.8 s "
                                                   "and 8.1 s for the releases; the prover client's wait for each answer "
                                                   "249.7 s for the checkpoint (from its restart: killed 8 s after it "
                                                   "started, it resumed the same call, one attempt), 569.1 s and 179.9 s "
                                                   "for the releases (from the spawn), three jobs each written once; "
                                                   "seals 260 B, selector `0xe71e6620`",))
    _documents(ctx, problems, "RFP-022 R.6", ("killed (SIGKILL) 8 s after it started", "(the same call id, one attempt)",
                                              "received the proof 249.7 s after it resumed",
                                              "(three jobs, each written once; row 266)"))
    fail_if(problems, "zone prover/seal/forge-fixture records differ")
    return [f"{calls_rel}, events.jsonl and summary.txt: three jobs, three answers, one attempt each, one SIGKILL, "
            "one resume of the checkpoint's unchanged call id; the client's waits 249.7 (from the restart)/569.1/"
            "179.9 s, the prover's own wall time 92.7/5.8/8.1 s",
            "three receipt journals equal their statements; every (seal, image id, journal) verifies with Groth16 "
            f"3.1.0; seals 260 B: {', '.join(job for job, _ in verified)}",
            f"{FIXTURE}: checkpoint journal/image/seal and both release request/seal/image tuples equal this run, "
            "so forge replays these statements rather than another run",
            "LIMIT: this README names the three proofs but does not state 249.7, 569.1 and 179.9 s (the client's "
            "waits) or 92.7, 5.8 and 8.1 s (the prover's); they were compared with calls.tsv/events/done records and "
            "with EVIDENCE row 266 and RFP-022 R.6, not with README prose"]


@check(id="zone-rc3-multi-anvil", family="record", rows=("RFP-022 F.7", "RFP-022 Sec.6"), **COMMON,
       title="Two anvil releases, named recipients and exact replay refusals",
       what="the corrected vault attempt's two net payments and recipients/nullifiers against release statements, "
            "final state and burn root, transaction hashes and the two NullifierAlreadyUsed plus ParentNotFinal replays",
       against="vault-checks.json, final logs/zone-bridge.log, crown/release records and manifest, plus the run README",
       inputs=(README, _run("vault-checks.json"), _run("logs/zone-bridge.log"), _run("manifest.json"),
               _run("crown.json"), f"{CHECKPOINT}/zone-28-164.json", f"{CHECKPOINT}/burns.json",
               f"{RUN}/releases/release-public-0.json", f"{RUN}/releases/release-private-1.json",
               f"{RUN}/releases/receipts/release-public-0/seal.hex", f"{RUN}/releases/receipts/release-private-1/seal.hex",
               *DOCS))
def zone_anvil(ctx):
    problems = []
    readme = _text(ctx, README)
    checks_rel = _run("vault-checks.json")
    checks = _json(ctx, checks_rel)
    crown = _json(ctx, _run("crown.json"))
    statement_rels = [f"{RUN}/releases/release-{name}.json" for name in ("public-0", "private-1")]
    statements = [_json(ctx, rel) for rel in statement_rels]
    expected_recipients = [row["recipient"].lower() for row in statements]
    expected_nullifiers = [row["nullifier"].lower() for row in statements]
    paid_rows = checks["releases"]
    recipients = [row["recipient"].lower() for row in paid_rows]
    nullifiers = [row["nullifier"].lower() for row in paid_rows]
    paid = [int(row["paidWei"]) for row in paid_rows]
    _same(problems, checks_rel, "recipients are those named by burns", recipients, expected_recipients)
    _same(problems, checks_rel, "release nullifiers", nullifiers, expected_nullifiers)
    _figure(problems, readme, checks_rel, "payments in wei", paid, [999_000_000_000_000, 999_000_000_000_000],
            "999,000,000,000,000 wei each")
    _same(problems, checks_rel, "two distinct, once-only releases", len(set(nullifiers)), 2)
    amounts = [int(row["amount"], 16) for row in statements]
    fees = [amount - payment for amount, payment in zip(amounts, paid)]
    _same(problems, checks_rel, "burn amounts less 0.1% checkpoint fees", fees, [1_000_000_000_000] * 2)
    for fixture_index, crown_release in enumerate(crown["releases"]):
        statement = statements[fixture_index]
        for crown_field, statement_field in (("nullifier", "nullifier"), ("recipient", "recipient"),
                                             ("relayer", "relayer"), ("burnRoot", "burn_root"),
                                             ("relayFee", "relay_fee"), ("token", "token")):
            _same(problems, _run("crown.json"), f"release {fixture_index} {crown_field}",
                  str(crown_release[crown_field]).lower(), str(statement[statement_field]).lower())
        _same(problems, _run("crown.json"), f"release {fixture_index} denomination", crown_release["denomIndex"], 0)
        # the request the vault was sent carries the seal zone-rc3-multi-proofs verifies for that statement
        receipt_seal = _hex(ctx, f"{RUN}/releases/receipts/release-{crown_release['name']}/seal.hex")
        _same(problems, _run("crown.json"), f"release {crown_release['name']} seal (as its verified receipt)",
              _hex32(crown_release["seal"]), receipt_seal.hex())

    meta = _json(ctx, f"{CHECKPOINT}/zone-28-164.json")
    burns = _json(ctx, f"{CHECKPOINT}/burns.json")
    _same(problems, checks_rel, "final state", checks["finalState"].lower(), meta["new_state"].lower())
    _same(problems, checks_rel, "accepted burn root", checks["burnRoot"].lower(), burns["burn_root"].lower())
    _expect(problems, checks["burnRootAccepted"] is True, f"{checks_rel}: burn root is not accepted")

    log_rel = _run("logs/zone-bridge.log")
    log = _text(ctx, log_rel)
    final_attempt = log.rfind("an earlier vault attempt's transactions")
    _expect(problems, final_attempt >= 0, f"{log_rel}: corrected final-attempt marker absent")
    final_log = log[final_attempt:] if final_attempt >= 0 else log
    logged_payments = re.findall(r"release ([01]): recipient (\S+) \+(\d+) wei; nullifierUsed true", final_log)
    _same(problems, log_rel, "corrected attempt payment count", len(logged_payments), 2)
    _same(problems, log_rel, "corrected attempt payments", [int(row[2]) for row in logged_payments], paid)
    _same(problems, log_rel, "corrected attempt recipients", [row[1].lower() for row in logged_payments], recipients)
    expected_replays = [("release-0", "NullifierAlreadyUsed"), ("release-1", "NullifierAlreadyUsed"),
                        ("submit", "ParentNotFinal")]
    replay_rows = checks["replaysRefused"]
    _same(problems, checks_rel, "replay stages/errors", [(row["what"], row["refused"]) for row in replay_rows],
          expected_replays)
    for replay in replay_rows:
        _expect(problems, f"{replay['what']} (transaction {replay['transaction']}) called again: refused "
                          f"({replay['refused']})" in final_log,
                f"{log_rel}: final-attempt replay record for {replay['what']} absent")
    manifest = _json(ctx, _run("manifest.json"))
    for replay in replay_rows:
        stage = manifest["stages"].get(replay["what"], [])
        _expect(problems, len(stage) == 1 and stage[0]["hash"] == replay["transaction"] and stage[0]["status"] == "0x1",
                f"{_run('manifest.json')}: mined transaction mismatch for replay {replay['what']}")
    _documents(ctx, problems, "EVIDENCE row 266", ("each release paid 999,000,000,000,000 wei once",
                                                   "replays refused (`NullifierAlreadyUsed` twice, `ParentNotFinal`)"))
    fail_if(problems, "zone anvil vault records differ")
    return [f"{checks_rel} + corrected {log_rel}: two recipients/nullifiers match the release statements; payments "
            f"{paid[0]:,} and {paid[1]:,} wei once each (burn minus 0.1% fee)",
            "final state equals checkpoint new_state, burn root accepted; replays: two NullifierAlreadyUsed and one "
            "ParentNotFinal, with transaction hashes equal to manifest.json",
            "LIMIT: forge's submitCandidate 343,581, finalizeAttested 181,337 and releases 377,860/273,760 gas occur "
            "only in this README and EVIDENCE row 266; no committed record holds them and no test asserts them (`cd "
            "vault && forge test --match-test test_gas_rc3Multi -vv` logs them), so they are not passed as "
            "recomputed gas evidence"]


@check(id="zone-rc3-multi-fork", family="record", rows=("RFP-022 F.7",), **COMMON,
       title="Sepolia fork rehearsal 31: binding, gas, payments and refusals",
       what="fork block and same-run deployment binding, all four stated operation gas figures, two run-release "
            "payments and gas, frozen/replay refusal ordering and the final OK",
       against="fork-rehearsal-31/transcript.txt and its params/deployment records, run params/identity and README",
       inputs=(README, _run("fork-rehearsal-31/transcript.txt"), _run("fork-rehearsal-31/params.json"),
               _run("fork-rehearsal-31/deployment.json"), PARAMS, _run("identity.json"), *DOCS))
def zone_fork(ctx):
    problems = []
    readme = _text(ctx, README)
    transcript_rel = _run("fork-rehearsal-31/transcript.txt")
    transcript = _text(ctx, transcript_rel)
    fork_match = re.search(r"fork of Sepolia at block (\d+)", transcript)
    fork_block = int(fork_match.group(1)) if fork_match else None
    _figure(problems, readme, transcript_rel, "fork block", fork_block, 12_216_620, "12,216,620")
    patterns = {
        "submit": r"candidate submitted with its RISC Zero 3\.1 seal .*gas (\d+)",
        "final": r"final; burn root accepted .*gas (\d+)",
    }
    stated_gas = {"submit": 375_245, "final": 218_361}
    written = {"submit": "375,245 gas", "final": "final after W (218,361)"}
    actual_gas = {}
    for name, pattern in patterns.items():
        match = re.search(pattern, transcript)
        actual_gas[name] = int(match.group(1)) if match else None
        _figure(problems, readme, transcript_rel, f"{name} gas", actual_gas[name], stated_gas[name], written[name])
    releases = re.findall(r"FERRATA_OK release ([01]) paid \(wei\) (\d+) \([^\n]*gas (\d+)\)", transcript)
    _same(problems, transcript_rel, "two indexed release payments", [int(row[0]) for row in releases], [0, 1])
    release_paid = [int(row[1]) for row in releases]
    release_gas = [int(row[2]) for row in releases]
    _figure(problems, readme, transcript_rel, "fork release payments", release_paid,
            [999_000_000_000_000, 999_000_000_000_000], "both releases paid")
    _figure(problems, readme, transcript_rel, "release 0 gas", release_gas[0] if release_gas else None, 421_342,
            "421,342")
    _figure(problems, readme, transcript_rel, "release 1 gas", release_gas[1] if len(release_gas) > 1 else None,
            384_654, "384,654")

    refusal_positions = [match.start() for match in re.finditer(r"FERRATA_OK release 0 refused as expected", transcript)]
    _same(problems, transcript_rel, "frozen and replay refusals", len(refusal_positions), 2)
    if len(refusal_positions) == 2 and releases:
        paid_positions = [transcript.index(f"FERRATA_OK release {i} paid") for i in (0, 1)]
        _expect(problems, refusal_positions[0] < paid_positions[0] < paid_positions[1] < refusal_positions[1],
                f"{transcript_rel}: frozen refusal/paid releases/replay are out of order")
        _expect(problems, transcript.rfind("FERRATA_OK releases frozen", 0, refusal_positions[0]) >= 0,
                f"{transcript_rel}: first refusal has no preceding releases-frozen step")
    _readme(problems, readme, "a release\nrefused while releases are frozen")
    _readme(problems, readme, "a replay refused")
    total_match = re.search(r"received (\d+) wei for the two releases", transcript)
    _same(problems, transcript_rel, "recipient two-release total", int(total_match.group(1)) if total_match else None,
          sum(release_paid))
    _expect(problems, transcript.count("FORK REHEARSAL 31 OK") == 1,
            f"{transcript_rel}: final FORK REHEARSAL 31 OK does not occur exactly once")
    _readme(problems, readme, "FORK REHEARSAL\n31 OK")
    _expect(problems, "FERRATA_OK binding: chain, vault, configuration and genesis are the journal's" in transcript,
            f"{transcript_rel}: run binding check absent")
    _expect(problems, "VERSION 3.1.0 SELECTOR 0xe71e6620" in transcript and "owner renounced" in transcript,
            f"{transcript_rel}: verifier 3.1.0/selector/renounced-stop record absent")

    params = _json(ctx, PARAMS)
    _same(problems, PARAMS, "the vault's floors (nMin, tMin)", (params["nMin"], params["tMin"]), (4, 3))
    _documents(ctx, problems, "EVIDENCE row 266", ("floors nMin 4, tMin 3",))
    fork_params = _json(ctx, _run("fork-rehearsal-31/params.json"))
    for field in ("checkpointImageIdB", "releaseImageId", "lezConfigId", "genesisState", "nMin", "tMin"):
        _same(problems, _run("fork-rehearsal-31/params.json"), field, fork_params[field], params[field])
    deployment = _json(ctx, _run("fork-rehearsal-31/deployment.json"))
    identity = _json(ctx, _run("identity.json"))
    _same(problems, _run("fork-rehearsal-31/deployment.json"), "chain/vault/deployer",
          (deployment["chainId"], deployment["vault"].lower(), deployment["deployer"].lower()),
          (identity["chainId"], identity["vault"].lower(), identity["deployer"].lower()))
    _expect(problems, deployment["complete"] is True and deployment["mode"] == "B",
            f"{_run('fork-rehearsal-31/deployment.json')}: incomplete or wrong vault mode")
    _documents(ctx, problems, "EVIDENCE row 266", ("fork of Sepolia at block 12,216,620: checkpoint 375,245 gas, final "
                                                   "after W (218,361), releases 421,342 and 384,654, a release refused "
                                                   "while frozen",))
    fail_if(problems, "zone fork rehearsal records differ")
    return [f"{transcript_rel}: Sepolia fork block {fork_block:,}; submit/final gas {actual_gas['submit']:,}/"
            f"{actual_gas['final']:,}; release gas {release_gas}",
            "two indexed releases paid 999,000,000,000,000 wei each; frozen refusal precedes them and replay follows; "
            "recipient total rebuilt from payment rows",
            "same run's programs/configuration/genesis and vault/deployer, 3.1 verifier binding and renounced stop; "
            "FORK REHEARSAL 31 OK exactly once"]


def _resources_module(root: Path):
    path = root / "checkpoint" / "attestor" / "resources.py"
    spec = importlib.util.spec_from_file_location("zone_bridge_resources", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@check(id="zone-rc3-multi-resources", family="record", rows=("RFP-022 P.6",), **COMMON,
       title="Four attestors' per-round CPU, wall, RSS, I/O and daily cadence",
       what="all 100 resource rows through the run's resources.py, per-attestor rounds/signatures, aggregate medians "
            "and maxima, first-round bytes, and derived core share and MiB/day at a 60-second cadence",
       against="zone/attestor-{0..3}/resources.tsv via checkpoint/attestor/resources.py, the cadence log and README",
       inputs=(README, "checkpoint/attestor/resources.py", _run("logs/zone-bridge.log"),
               *tuple(_run(f"zone/attestor-{i}/resources.tsv") for i in range(4)), *DOCS))
def zone_resources(ctx):
    problems = []
    readme = _text(ctx, README)
    resources = _resources_module(ctx.root)
    rows_by_attestor = {}
    for i in range(4):
        rel = _run(f"zone/attestor-{i}/resources.tsv")
        rows_by_attestor[rel] = resources.parse(str(ctx.path(rel)))
    _figure(problems, readme, "zone/attestor-*/resources.tsv", "attestors", len(rows_by_attestor), 4,
            "zone/attestor-{0..3}/")
    per_rounds = [len(rows) for rows in rows_by_attestor.values()]
    _figure(problems, readme, "zone/attestor-*/resources.tsv", "rounds per attestor", per_rounds, [25] * 4)
    signed = [sum(row["ok"] for row in rows_by_attestor[rel]) for rel in rows_by_attestor]
    _same(problems, "zone/attestor-*/resources.tsv", "signed rounds per attestor", signed, [25] * 4)
    for rel in rows_by_attestor:
        times = [datetime.strptime(line.split("\t")[0], "%Y-%m-%dT%H:%M:%SZ")
                 for line in _text(ctx, rel).splitlines() if line.strip()]
        # the attestor's loop: a round, then 60 s of sleep (zone-bridge.sh attest_loop), so 60-61 s apart; its last row
        # is the final round (final_rounds), started once the zone reached T1, a few seconds after the loop's last
        gaps = [(b - a).total_seconds() for a, b in zip(times, times[1:])]
        _expect(problems, len(gaps) == 24 and all(gap in (60.0, 61.0) for gap in gaps[:-1]) and 0 < gaps[-1] < 60,
                f"{rel}: the rounds' timestamps are not 24 a minute apart (60-61 s) and then the final round: gaps "
                f"{sorted(set(gaps))}")
    every = [row for rows in rows_by_attestor.values() for row in rows]
    _same(problems, "zone/attestor-*/resources.tsv", "total rounds", len(every), 100)
    summary = resources.summary(every, 60)
    _figure(problems, readme, "all resources.tsv rows", "CPU median (s)", summary["cpu_s_median"], 0.17)
    _figure(problems, readme, "all resources.tsv rows", "wall median (s)", summary["wall_s_median"], 0.19)
    _figure(problems, readme, "all resources.tsv rows", "largest CPU (s)", summary["cpu_s_max"], 0.36)
    _figure(problems, readme, "all resources.tsv rows", "largest wall (s)", summary["wall_s_max"], 0.94)
    _figure(problems, readme, "all resources.tsv rows", "median peak RSS MiB", summary["rss_mib_median"], 18.0)
    _figure(problems, readme, "all resources.tsv rows", "largest peak RSS MiB", summary["rss_mib_max"], 18.0)
    _figure(problems, readme, "all resources.tsv rows", "median bytes read a round", summary["bytes_median"], 35_714)
    first_bytes = [rows[0]["bytes"] for rows in rows_by_attestor.values()]
    _same(problems, "zone/attestor-*/resources.tsv", "identical first-round bytes", first_bytes, [2_639_089] * 4)
    _figure(problems, readme, "all resources.tsv rows", "first round rounded MB",
            round(summary["bytes_max"] / 1_000_000, 1), 2.6)
    _figure(problems, readme, "all resources.tsv rows", "core share rounded to three decimals",
            round(summary["core_share_at_cadence"], 3), 0.003)
    _figure(problems, readme, "all resources.tsv rows", "MiB read/day rounded to whole MiB",
            round(summary["mib_per_day_at_cadence"]), 314)
    log_rel = _run("logs/zone-bridge.log")
    log = _text(ctx, log_rel)
    _expect(problems, "four attestors started (a round every 60 s" in log,
            f"{log_rel}: four-attestor/60-second cadence record absent")
    _documents(ctx, problems, "RFP-022 P.6", ("four attestors, 25 rounds each: 24 a minute apart (60–61 s), then the final "
                                              "round, whose attestations the checkpoint took",
                                              "a round 0.17 CPU s and 0.19 s wall (medians; at most 0.36 and 0.94), "
                                              "peak RSS 18.0 MiB, 35,714 bytes read (the first round 2.6 MB)",
                                              "0.003 of a core and 314 MiB read a day (row 266)"))
    _documents(ctx, problems, "EVIDENCE row 266", ("100 rounds (each attestor's 24 a minute apart, then the final round, "
                                                   "whose attestations the checkpoint took), 0.17 CPU s and 0.19 s wall a "
                                                   "round (medians), peak RSS "
                                                   "18.0 MiB, 35,714 B read a round (the first 2.6 MB), 0.003 of a "
                                                   "core and 314 MiB a day at one round a minute",))
    fail_if(problems, "zone attestor resource records differ")
    return [f"four resources.tsv files: {per_rounds} rounds, all signed; aggregate {len(every)} rows; medians "
            f"CPU {summary['cpu_s_median']} s, wall {summary['wall_s_median']} s, RSS {summary['rss_mib_median']} MiB, "
            f"bytes {summary['bytes_median']:,}",
            f"maxima CPU {summary['cpu_s_max']} s, wall {summary['wall_s_max']} s, RSS {summary['rss_mib_max']} MiB; "
            f"first round {first_bytes[0]:,} B = {first_bytes[0] / 1_000_000:.4f} MB",
            f"at 60 s cadence: exact summary core share {summary['core_share_at_cadence']} (0.003 to three decimals), "
            f"{summary['mib_per_day_at_cadence']} MiB/day (314 MiB rounded)",
            "LIMIT: this README names the attestor directories and resource command but does not state 25 rounds, "
            "60 s cadence or the CPU/wall/RSS/I/O/core/daily figures; those were compared with the raw TSV summaries "
            "and with RFP-022 P.6 and EVIDENCE row 266, not with README prose"]
