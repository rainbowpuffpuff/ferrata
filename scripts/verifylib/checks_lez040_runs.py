"""Offline re-checks of the recorded LEZ v0.4.0 zone runs.

The standalone block, transaction and receipt figures are decoded here, independently of the
run's checker.  The four-sequencer inscription totals are likewise counted directly from the
node answers before the run's own summary program is asked to regenerate its JSON.  The vote
runs' keys, stakes, votes and channel parameters come from their command logs and channel JSON.
In every case the README is a second statement of the records' figures, never their source.
"""
from __future__ import annotations

import base64
import csv
import hashlib
import io
import json
import re
import tempfile
from pathlib import Path

from . import lez040
from .core import CheckFailed, check, command, fail_if, run_script

STANDALONE = "e2e/runs/zone-v040-standalone-1"
FOUR_KEYS = "e2e/runs/zone-v040-four-keys-1"
VOTE_1 = "e2e/runs/zone-v040-vote-1"
VOTE_2 = "e2e/runs/zone-v040-vote-2"
STANDALONE_README = f"{STANDALONE}/README.md"
FOUR_KEYS_README = f"{FOUR_KEYS}/README.md"
FACTS = f"{STANDALONE}/facts.json"
STANDALONE_LOG = f"{STANDALONE}/run.log"
PRIVATE_LOG = f"{STANDALONE}/logs/private.log"
SEQUENCER_LOG = f"{STANDALONE}/logs/sequencer.log"
INSCRIPTIONS = f"{FOUR_KEYS}/inscriptions.json"
CHANNEL = f"{FOUR_KEYS}/answers/channel.json"
DEPLOYMENT = f"{FOUR_KEYS}/deployment-settings.yaml"
DEPLOYMENT_SHA256 = f"{FOUR_KEYS}/deployment-settings.sha256"
ATTEST_RUN = f"{FOUR_KEYS}/attest-run.txt"
ZONE_LOG = f"{FOUR_KEYS}/zone.log"
CKPT_DATA = "checkpoint/crates/checkpoint-core/tests/data/zone-v040-four-keys-1/blocks.hex"  # the v0.4.0 parser's test data
ANSWER_BLOCKS = f"{FOUR_KEYS}/answers/blocks"
STANDALONE_CHECKER = "checkpoint/attestor/check_zone_v040.py"
VOTE_CHECKER = STANDALONE_CHECKER
INSCRIPTION_TOOL = "checkpoint/attestor/zone_inscriptions.py"
IMAGE_RECORDS = "docs/port-v0.4.0/image-ids"
LEZ_TAGS = "docs/port-v0.4.0/lez-tags.json"
CHANNEL_ID = "01" * 32


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _json(ctx, rel: str):
    return json.loads(_text(ctx, rel))


def _same(problems: list, source: str, name: str, actual, stated) -> None:
    if actual != stated:
        problems.append(f"{source}: {name}: re-derived {actual!r}, recorded/stated {stated!r}")


def _fragment(problems: list, source: str, text: str, name: str, fragment: str) -> None:
    if fragment not in text:
        problems.append(f"{source}: {name} {fragment!r} is absent or differs")


def _pattern(problems: list, source: str, text: str, name: str, pattern: str) -> None:
    if re.search(pattern, text) is None:
        problems.append(f"{source}: {name} does not match the records (expected /{pattern}/)")


def _receipt(record: dict) -> dict:
    """The decoder's flat receipt, compared field by field with the run's nested JSON form."""
    if not isinstance(record, dict) or set(record) != {"success", "fee"}:
        return {"malformed": record}
    fee = record["fee"]
    if not isinstance(fee, dict):
        return {"malformed": record}
    return {"success": record["success"], **{key: fee.get(key) for key in
            ("gas_exec", "gas_stor", "fee_base", "tip", "refund")}}


def _short_account(account: str) -> str:
    prefix, _, address = account.partition("/")
    return f"{prefix}/{address[:8]}…{address[-4:]}"


def _standalone_run_figures(text: str) -> dict:
    """Figures printed by the standalone driver (the receipt JSON is parsed, not sought as text)."""
    patterns = {
        "first": r"standalone sequencer up: block (\d+) after (\d+) s",
        "last": r"standalone sequencer at block (\d+) after (\d+) s",
        "transfer": r"public transfer (\S+) -> (\S+) \((\d+)\): ([0-9a-f]{64})",
        "private": r"private transfer (\S+) -> (\S+) \((\d+)\), proven and sent in (\d+) s: ([0-9a-f]{64})",
        "deploy": (r"deployed (\S+) \(image id ([0-9a-f]{64}), (\d+) bytes\) at header (\S+) with "
                   r"(\d+) segments in (\d+) s"),
    }
    out = {}
    for name, pattern in patterns.items():
        match = re.search(pattern, text)
        if match:
            out[name] = match.groups()
    for name, prefix in (("transfer_receipt", "receipt: "),
                         ("private_receipt", "private transfer's receipt: ")):
        line = next((line for line in text.splitlines() if re.search(r"\] " + re.escape(prefix), line)), None)
        if line is not None:
            try:
                out[name] = json.loads(line.split(prefix, 1)[1])
            except json.JSONDecodeError:
                pass
    return out


@check(id="lez040-run-standalone", family="record", mode="offline", rows=(), evidence=(303,),
       title="The standalone v0.4.0 public/private transfers, decoded receipts and deployed rebate program",
       what="every saved Borsh block and hash; the public transaction, receipt and storage-size fee; the shared gas "
            "price and private fee; wallet/sequencer log lines; image id, binary digest, LEZ commit and README table",
       against="blocks/block-*.json, facts.json, run.log, logs/private.log and logs/sequencer.log, the v0.4.0 image-id "
               "and tag records, the run README, and checkpoint/attestor/check_zone_v040.py",
       inputs=(STANDALONE_README, FACTS, STANDALONE_LOG, PRIVATE_LOG, SEQUENCER_LOG,
               f"{STANDALONE}/blocks", IMAGE_RECORDS, LEZ_TAGS, STANDALONE_CHECKER))
def standalone(ctx):
    problems = []
    facts = _json(ctx, FACTS)
    readme = _text(ctx, STANDALONE_README)
    run_text = _text(ctx, STANDALONE_LOG)
    run = _standalone_run_figures(run_text)
    blocks = {}
    block_paths = sorted(ctx.path(f"{STANDALONE}/blocks").glob("block-*.json"))
    if not block_paths:
        problems.append(f"{STANDALONE}/blocks: no block-N.json records")
    for path in block_paths:
        match = re.fullmatch(r"block-(\d+)\.json", path.name)
        if match is None:
            continue
        number = int(match.group(1))
        rel = f"{STANDALONE}/blocks/{path.name}"
        try:
            raw = base64.b64decode(json.loads(path.read_text(encoding="utf-8")), validate=True)
            block = lez040.decode_block(raw)  # decode_block's Reader.finish requires every byte consumed
        except (ValueError, TypeError, json.JSONDecodeError) as error:
            problems.append(f"{rel}: does not decode as base64/Borsh: {error}")
            continue
        blocks[number] = block
        _same(problems, rel, "block_id", block["header"]["block_id"], number)
        if not block["hash_ok"]:
            problems.append(f"{rel}: recomputed block hash {block['computed_hash'].hex()} differs from "
                            f"{block['header']['hash'].hex()}")

    transfer_block_number = int(facts["transfer_block"])
    transfer_block = blocks.get(transfer_block_number)
    transfer = receipt = None
    transfer_index = None
    if transfer_block is None:
        problems.append(f"{FACTS}: transfer_block {transfer_block_number} has no decoded block record")
    else:
        matches = [(i, tx) for i, tx in enumerate(transfer_block["transactions"])
                   if tx["hash"].hex() == facts["transfer_tx"]]
        _same(problems, FACTS, "transfer_tx occurrences in its block", len(matches), 1)
        if len(matches) == 1:
            transfer_index, transfer = matches[0]
            if transfer_index < len(transfer_block["receipts"]):
                receipt = transfer_block["receipts"][transfer_index]
                _same(problems, FACTS, "transfer_receipt field by field", _receipt(facts["transfer_receipt"]), receipt)
            else:
                problems.append(f"{FACTS}: transfer transaction {transfer_index} has no receipt in block "
                                f"{transfer_block_number}")
    gas_used = fee_base = None
    if transfer is not None and receipt is not None:
        _same(problems, FACTS, "public storage gas is serialized transaction length including enum tag",
              len(transfer["raw"]), receipt["gas_stor"])
        gas_used = receipt["gas_exec"] + receipt["gas_stor"]
        fee_base = receipt["fee_base"]
        quotient, remainder = divmod(fee_base, gas_used) if gas_used else (None, fee_base)
        if not gas_used or remainder:
            problems.append(f"{FACTS}: public fee_base {fee_base} is not a whole-number price times gas {gas_used}")
        else:
            private = _receipt(facts["private_receipt"])
            expected_private_fee = quotient * (private["gas_exec"] + private["gas_stor"])
            _same(problems, FACTS, "private fee_base at the public transaction's whole gas price",
                  expected_private_fee, private["fee_base"])

    # The wallet's private.log is truncated during its enormous transaction dump after naming the transaction.  The
    # driver wrote the sequencer receipt to run.log; binding that separate record is the most this run can support.
    private_log = _text(ctx, PRIVATE_LOG)
    _fragment(problems, PRIVATE_LOG, private_log, "wallet transaction hash",
              f"Transaction hash is {facts['private_tx']}")
    _same(problems, STANDALONE_LOG, "private transaction hash", run.get("private", (None,) * 5)[4],
          facts["private_tx"])
    _same(problems, STANDALONE_LOG, "private receipt as printed by the driver", run.get("private_receipt"),
          facts["private_receipt"])
    _same(problems, STANDALONE_LOG, "public receipt as printed by the driver", run.get("transfer_receipt"),
          facts["transfer_receipt"])

    # All build records must agree on the named program, so a second build cannot silently supply a different binary.
    image_rows = []
    for path in sorted(ctx.path(IMAGE_RECORDS).glob("*/ids.tsv")):
        for row in csv.DictReader(io.StringIO(path.read_text(encoding="utf-8")), delimiter="\t"):
            if row.get("name") == "ferrata_rebate":
                image_rows.append((path, row))
    if not image_rows:
        problems.append(f"{IMAGE_RECORDS}/*/ids.tsv: no ferrata_rebate row")
    deployed = facts["program_deploy"]
    for path, row in image_rows:
        rel = str(path.relative_to(ctx.root))
        _same(problems, rel, "ferrata_rebate image_id", deployed["image_id"], row["image_id"].removeprefix("0x"))
        _same(problems, rel, "ferrata_rebate sha256_bin", deployed["sha256"], row["sha256_bin"])
    tags = _json(ctx, LEZ_TAGS)
    _same(problems, FACTS, "LEZ v0.4.0 tag and commit", facts["lez"],
          f"v0.4.0 {tags['tags']['v0.4.0']['commit']}")

    # The sequencer's own publication lines independently bound the first and last heights named by facts/run.log;
    # its startup JSON also carries the finality depth and lack of exit delay stated in the README's first table row.
    sequencer_text = _text(ctx, SEQUENCER_LOG)
    published = [int(n) for n in re.findall(r"Publishing block block_id=(\d+)\b", sequencer_text)]
    channel_params = {}
    config_match = re.search(r"Starting with config config=(\{.*\}) channel_signing_key_file=", sequencer_text)
    if config_match is None:
        problems.append(f"{SEQUENCER_LOG}: startup config JSON absent")
    else:
        try:
            channel_params = json.loads(config_match.group(1))["genesis"]["channel_params"]
        except (ValueError, KeyError, TypeError) as error:
            problems.append(f"{SEQUENCER_LOG}: startup channel params do not parse: {error}")
    if channel_params:
        _fragment(problems, STANDALONE_README, readme, "finality depth and no exit delay",
                  f"finality_depth {channel_params.get('finality_depth')}, no exit_delay")
        if "exit_delay" in channel_params:
            problems.append(f"{SEQUENCER_LOG}: exit_delay is present although {STANDALONE_README} says there is none")
    if published:
        _same(problems, SEQUENCER_LOG, "contiguous published heights", published,
              list(range(min(published), max(published) + 1)))
        _same(problems, FACTS, "first/last standalone heights",
              (facts["standalone_first_height"], facts["standalone_last_height"]),
              (min(published), max(published)))
    else:
        problems.append(f"{SEQUENCER_LOG}: no published block heights")
    _same(problems, STANDALONE_LOG, "first height", run.get("first", (None, None))[0],
          str(facts["standalone_first_height"]))
    _same(problems, STANDALONE_LOG, "last height", run.get("last", (None, None))[0],
          str(facts["standalone_last_height"]))
    if run.get("transfer"):
        _same(problems, STANDALONE_LOG, "public transfer accounts/hash",
              (run["transfer"][0], run["transfer"][1], run["transfer"][3]),
              (facts["test_account"], facts["new_account"], facts["transfer_tx"]))
    if run.get("private"):
        _same(problems, STANDALONE_LOG, "private transfer accounts",
              (run["private"][0], run["private"][1]), (facts["test_account"], facts["private_account"]))
        _same(problems, FACTS, "private_seconds", int(run["private"][3]), facts["private_seconds"])
    if run.get("deploy"):
        _same(problems, STANDALONE_LOG, "deployed program/header/image/segments/seconds",
              (run["deploy"][0], run["deploy"][1], run["deploy"][3], int(run["deploy"][4]), int(run["deploy"][5])),
              (deployed["elf"], deployed["image_id"], deployed["header"], deployed["segments"], deployed["seconds"]))

    # Hold the README to figures rebuilt above, including its thousands separators and abbreviated account ids.
    public = facts["transfer_receipt"]["fee"]
    private = facts["private_receipt"]["fee"]
    first_seconds = run.get("first", (None, "?"))[1]
    last_seconds = run.get("last", (None, "?"))[1]
    _fragment(problems, STANDALONE_README, readme, "standalone heights and seconds",
              f"block {facts['standalone_first_height']} {first_seconds} s after start, block "
              f"{facts['standalone_last_height']} after {last_seconds} s")
    _fragment(problems, STANDALONE_README, readme, "test account", _short_account(facts["test_account"]))
    transfer_amount = run.get("transfer", (None, None, "?"))[2]
    _fragment(problems, STANDALONE_README, readme, "public transfer amount", f"A public transfer of {int(transfer_amount):,}")
    _same(problems, FACTS, "new account balance", facts["new_account_view"],
          f"Balance {transfer_amount}, nonce 0 ")
    _fragment(problems, STANDALONE_README, readme, "public receipt",
              f"`success: true`; storage gas {public['gas_stor']:,} bytes, base fee {public['fee_base']:,}; "
              f"the new account holds {int(transfer_amount):,}")
    private_amount = run.get("private", (None, None, "?"))[2]
    _fragment(problems, STANDALONE_README, readme, "private transfer amount and seconds",
              f"A transfer of {int(private_amount):,} into a new private account")
    _fragment(problems, STANDALONE_README, readme, "private receipt and seconds",
              f"proven and sent in {facts['private_seconds']} s; receipt `success: true`, execution gas "
              f"{private['gas_exec']:,}, storage gas {private['gas_stor']:,} bytes")
    _fragment(problems, STANDALONE_README, readme, "private base fee", f"base fee {private['fee_base']:,}")
    _fragment(problems, STANDALONE_README, readme, "deployment header/segments/seconds",
              f"header `{_short_account(deployed['header'])}`, {deployed['segments']} segments, "
              f"{deployed['seconds']} s")
    if image_rows:
        binary_bytes = {int(row["bytes_bin"]) for _, row in image_rows}
        if len(binary_bytes) == 1:
            size = binary_bytes.pop()
            _fragment(problems, STANDALONE_README, readme, "program binary size", f"{size:,} bytes")
            if run.get("deploy"):
                _same(problems, IMAGE_RECORDS, "program binary size as printed by the driver",
                      int(run["deploy"][2]), size)

    rc, output = run_script(ctx, ["python3", "-I", STANDALONE_CHECKER, STANDALONE])
    if rc != 0:
        problems.append(f"{STANDALONE_CHECKER}: exit status {rc}: {' | '.join(output[-8:])}")
    fail_if(problems, "standalone LEZ v0.4.0 run records differ")
    price = fee_base // gas_used
    return [f"{len(blocks)} saved blocks decode completely, ids equal filenames and recomputed hashes match; "
            f"transfer {facts['transfer_tx'][:12]}… in block {transfer_block_number}: receipt equals facts.json, "
            f"storage gas {receipt['gas_stor']} = {len(transfer['raw'])} serialized bytes including enum tag",
            f"gas price p = {price}: public {gas_used} gas -> {fee_base} base fee; private "
            f"{private['gas_exec']} + {private['gas_stor']} gas -> {private['fee_base']} base fee",
            f"{PRIVATE_LOG} names the private hash; its receipt is not in that truncated log but equals the JSON in "
            f"{STANDALONE_LOG}; ferrata_rebate image id and binary SHA-256 equal every build record; "
            f"{STANDALONE_CHECKER} exits 0"]


def _read_node_blocks(ctx) -> list:
    blocks = []
    for path in sorted(ctx.path(ANSWER_BLOCKS).glob("*.json")):
        value = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(value, list):
            raise CheckFailed(f"{path.relative_to(ctx.root)}: node block answer is not a JSON list")
        blocks.extend(value)
    return sorted(blocks, key=lambda block: block["header"]["slot"])


def _channel_inscriptions(node_blocks: list) -> list:
    """Independent walk of Mantle operations: [(slot, signer)], without importing the run's summarizer."""
    found = []
    for block in node_blocks:
        for transaction in block.get("transactions") or []:
            for operation in (transaction.get("mantle_tx") or {}).get("ops") or []:
                if operation.get("opcode") != 0x11:
                    continue
                payload = operation.get("payload") or {}
                if str(payload.get("channel_id", "")).lower().removeprefix("0x") != CHANNEL_ID:
                    continue
                found.append((block["header"]["slot"],
                              str(payload.get("signer", "")).lower().removeprefix("0x")))
    return found


def _sequencers(zone_log: str) -> dict:
    return {key: f"seq-{number}" for number, key in
            re.findall(r"seq-(\d+) set up: ([0-9a-f]{64})\b", zone_log)}


def _summarize_inscriptions(found: list, names: dict) -> tuple:
    per, turns = {}, []
    for slot, signer in found:
        who = names.get(signer, signer[:16])
        per[who] = per.get(who, 0) + 1
        if turns and turns[-1]["sequencer"] == who:
            turns[-1]["inscriptions"] += 1
            turns[-1]["toSlot"] = slot
        else:
            turns.append({"sequencer": who, "inscriptions": 1, "fromSlot": slot, "toSlot": slot})
    return per, turns


@check(id="lez040-run-four-keys", family="record", mode="offline", rows=(), evidence=(303,),
       title="The four-key v0.4.0 zone's 107 inscriptions, V2 node settings and ordered-actors refusal",
       what="node blocks and channel inscription totals counted independently; per-sequencer counts and turns; the "
            "summary regenerated by its own tool; channel parameters, deployment digest/schema, refusal and README",
       against="answers/blocks/*.json, answers/channel.json, zone.log, inscriptions.json, deployment-settings.yaml "
               "and .sha256, attest-run.txt, the run README, and checkpoint/attestor/zone_inscriptions.py",
       inputs=(FOUR_KEYS_README, INSCRIPTIONS, CHANNEL, DEPLOYMENT, DEPLOYMENT_SHA256, ATTEST_RUN, ZONE_LOG,
               ANSWER_BLOCKS, INSCRIPTION_TOOL, CKPT_DATA))
def four_keys(ctx):
    problems = []
    readme = _text(ctx, FOUR_KEYS_README)
    committed = _json(ctx, INSCRIPTIONS)
    channel = _json(ctx, CHANNEL)
    node_blocks = _read_node_blocks(ctx)
    found = _channel_inscriptions(node_blocks)
    names = _sequencers(_text(ctx, ZONE_LOG))
    per, turns = _summarize_inscriptions(found, names)
    _same(problems, INSCRIPTIONS, "nodeBlocks counted from answers", len(node_blocks), committed["nodeBlocks"])
    _same(problems, INSCRIPTIONS, "channel inscriptions counted from answers", len(found), committed["inscriptions"])
    _same(problems, INSCRIPTIONS, "perSequencer counted from answers", per, committed["perSequencer"])
    _same(problems, INSCRIPTIONS, "turns counted from answers", turns, committed["turns"])

    # The LEZ blocks inside the inscriptions, read with LEZ v0.4.0's layout (scripts/verifylib/lez040.py): each decodes
    # whole, its hash recomputes, the ids run 1, 2, ... and each names the previous block's hash. This is the layout
    # the checkpoint's own LEZ parser (rc1's, checkpoint-core) does not read yet (attest-run.txt).
    inscribed = []
    for block in node_blocks:
        for transaction in block.get("transactions") or []:
            for operation in (transaction.get("mantle_tx") or {}).get("ops") or []:
                payload = operation.get("payload") or {}
                if operation.get("opcode") == 0x11 and \
                        str(payload.get("channel_id", "")).lower().removeprefix("0x") == CHANNEL_ID:
                    try:
                        inscribed.append(lez040.decode_block(bytes.fromhex(str(payload.get("inscription", "")))))
                    except ValueError as error:
                        problems.append(f"{ANSWER_BLOCKS}: an inscribed LEZ block (slot {block['header']['slot']}) "
                                        f"does not decode with LEZ v0.4.0's layout: {error}")
    inscription_hex = []
    for block in node_blocks:
        for transaction in block.get("transactions") or []:
            for operation in (transaction.get("mantle_tx") or {}).get("ops") or []:
                payload = operation.get("payload") or {}
                if operation.get("opcode") == 0x11 and \
                        str(payload.get("channel_id", "")).lower().removeprefix("0x") == CHANNEL_ID:
                    inscription_hex.append(str(payload.get("inscription", "")).lower())
    if ctx.path(CKPT_DATA).is_file():
        test_data = [ln.strip().lower() for ln in _text(ctx, CKPT_DATA).splitlines() if ln.strip()]
        if test_data != inscription_hex:
            problems.append(f"{CKPT_DATA}: checkpoint-core's v0.4.0 test data is not the run's {len(inscription_hex)} "
                            f"inscriptions in order ({len(test_data)} lines)")
    for index, lez_block in enumerate(inscribed):
        header = lez_block["header"]
        if not lez_block["hash_ok"]:
            problems.append(f"{ANSWER_BLOCKS}: inscribed LEZ block {header['block_id']}: its hash does not recompute")
        if header["block_id"] != index + 1:
            problems.append(f"{ANSWER_BLOCKS}: inscribed LEZ block {index + 1} of the channel has id {header['block_id']}")
        previous = inscribed[index - 1]["header"]["hash"] if index else bytes(32)
        if header["prev_block_hash"] != previous:
            problems.append(f"{ANSWER_BLOCKS}: inscribed LEZ block {header['block_id']} does not name the previous "
                            "block's hash")

    scratch = ctx.root / ".scratch" / "verify-lez040-runs"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="four-keys-", dir=scratch) as temporary:
        regenerated_path = Path(temporary) / "inscriptions.json"
        rc, output = run_script(ctx, ["python3", "-I", INSCRIPTION_TOOL, FOUR_KEYS,
                                     "--out", str(regenerated_path)])
        if rc != 0 or not regenerated_path.is_file():
            problems.append(f"{INSCRIPTION_TOOL}: exit status {rc}: {' | '.join(output[-8:])}")
        else:
            regenerated = json.loads(regenerated_path.read_text(encoding="utf-8"))
            if regenerated != committed:
                problems.append(f"{INSCRIPTIONS}: differs from JSON regenerated by {INSCRIPTION_TOOL}; "
                                f"regenerated summary {regenerated.get('nodeBlocks')} blocks/"
                                f"{regenerated.get('inscriptions')} inscriptions/{regenerated.get('perSequencer')}")

    _pattern(problems, FOUR_KEYS_README, readme, "inscription/node/per-sequencer counts and turns",
             rf"{len(found)} channel inscriptions in the node's {len(node_blocks)} final blocks, "
             rf"seq-0 {per.get('seq-0')}\s+\(two turns\), seq-3 {per.get('seq-3')}, "
             rf"seq-2 {per.get('seq-2')}, seq-1 {per.get('seq-1')}, in five consecutive turns")
    _same(problems, FOUR_KEYS_README, "number of turns (the README's five)", len(turns), 5)
    _pattern(problems, FOUR_KEYS_README, readme, "channel threshold/timeframe/timeout",
             rf"configuration threshold\s+{channel['configuration_threshold']}, posting timeframe "
             rf"{channel['posting_timeframe']} slots, posting timeout {channel['posting_timeout']}")

    yaml_bytes = ctx.path(DEPLOYMENT).read_bytes()
    digest = hashlib.sha256(yaml_bytes).hexdigest()
    stated_digest = _text(ctx, DEPLOYMENT_SHA256).strip()
    _same(problems, DEPLOYMENT_SHA256, "deployment-settings.yaml SHA-256", digest, stated_digest)
    yaml_text = yaml_bytes.decode("utf-8")
    _pattern(problems, DEPLOYMENT, yaml_text, "era zero's V2 schema tag", r"(?m)^eras:\n\s+0:\s+!V2\s*$")
    _fragment(problems, FOUR_KEYS_README, readme, "deployment schema statement", "V2 era schema")

    refusal = 'block 2: parser refused it: NotStrictlyOrdered { what: "actors" }'
    _fragment(problems, ATTEST_RUN, _text(ctx, ATTEST_RUN), "rc3 parser refusal", refusal)
    _fragment(problems, FOUR_KEYS_README, readme, "quoted rc3 parser refusal", refusal)
    fail_if(problems, "four-key LEZ v0.4.0 run records differ")
    return [f"answers: {len(node_blocks)} node blocks and {len(found)} channel inscriptions counted directly; "
            f"per sequencer {per}; {len(turns)} turns; {INSCRIPTION_TOOL} regenerates inscriptions.json exactly",
            f"channel: threshold {channel['configuration_threshold']}, posting timeframe "
            f"{channel['posting_timeframe']}, timeout {channel['posting_timeout']}; deployment SHA-256 {digest[:12]}… "
            "and era 0 !V2; all figures stated by the README",
            f"{len(inscribed)} inscribed LEZ blocks read with LEZ v0.4.0's layout: ids 1-{len(inscribed)}, each hash "
            f"recomputed and chained, {sum(len(b['transactions']) for b in inscribed)} transactions, "
            f"{sum(len(b['receipts']) for b in inscribed)} receipts; checkpoint-core's v0.4.0 parser test data "
            f"({CKPT_DATA}) is these inscriptions byte for byte" if ctx.path(CKPT_DATA).is_file() else
            f"{sum(len(b['receipts']) for b in inscribed)} receipts",
            f"{ATTEST_RUN}: {refusal}"]


def _vote_inputs(run: str) -> tuple:
    return (f"{run}/README.md", f"{run}/facts.json", f"{run}/channel-four-keys.json",
            f"{run}/channel-after-vote.json", f"{run}/run.log", f"{run}/logs/stake.log",
            f"{run}/logs/vote.log", f"{run}/logs/unstake.log", f"{run}/logs/wallet.log",
            f"{run}/logs/seq-0.log", *(f"{run}/logs/setup-{i}.log" for i in range(4)), VOTE_CHECKER)


def _channel_params(text: str) -> dict:
    """The integer fields of one Rust-displayed ChannelParams value."""
    match = re.search(r"ChannelParams \{([^{}]*)\}", text)
    if match is None:
        return {}
    return {key: int(value) for key, value in re.findall(r"(\w+): (\d+)", match.group(1))}


def _vote_records(text: str) -> list:
    """The three command records as (requested parameters, vote block, outcome), in log order."""
    pattern = re.compile(r"(?m)^Voting for (ChannelParams \{[^\n]+\}): ([0-9a-f]{64})\n"
                         r"Included in block (\d+)\n([^\n]+)$")
    return [(_channel_params(match.group(1)), match.group(2), int(match.group(3)), match.group(4))
            for match in pattern.finditer(text)]


def _stake_records(text: str) -> list:
    pattern = re.compile(r"(?m)^Staking (\d+) from (\S+) into (\S+): ([0-9a-f]{64})\n"
                         r"Included in block (\d+)$")
    return [(int(match.group(1)), match.group(2), match.group(3), match.group(4), int(match.group(5)))
            for match in pattern.finditer(text)]


def _setup_keys(ctx, run: str, problems: list) -> list:
    """Keys read from setup-N.log, cross-checked against the driver's surviving setup lines."""
    keys = []
    for i in range(4):
        rel = f"{run}/logs/setup-{i}.log"
        found = re.findall(r"(?m)^\s*channel signing public key\s+([0-9a-f]{64})\s*$", _text(ctx, rel))
        _same(problems, rel, "channel signing key occurrence count", len(found), 1)
        keys.append(found[0] if len(found) == 1 else None)
    run_rel = f"{run}/run.log"
    printed = {int(i): key for i, key in re.findall(r"seq-(\d+) set up: channel key ([0-9a-f]{64})\b",
                                                    _text(ctx, run_rel))}
    for i, key in enumerate(keys):
        if key is not None:
            _same(problems, run_rel, f"seq-{i} setup key", printed.get(i), key)
    return keys


def _genesis_params(ctx, run: str, problems: list) -> dict:
    rel = f"{run}/logs/seq-0.log"
    text = _text(ctx, rel)
    match = re.search(r"Starting with config config=(\{.*\}) channel_signing_key_file=", text)
    if match is None:
        problems.append(f"{rel}: startup config JSON absent")
        return {}
    try:
        genesis = json.loads(match.group(1))["genesis"]
        params = genesis["channel_params"]
    except (ValueError, KeyError, TypeError) as error:
        problems.append(f"{rel}: genesis channel params do not parse: {error}")
        return {}
    if "exit_delay" in params or "exit_delay" in genesis:
        problems.append(f"{rel}: exit_delay is present in the zone genesis; LEZ v0.4.0 uses L1 finality instead")
    return params


def _vote_common(ctx, run: str, checker_gap: str = "") -> tuple:
    """Re-derive the common vote records; the final result lines are made by each run's wrapper."""
    problems = []
    facts = _json(ctx, f"{run}/facts.json")
    readme = _text(ctx, f"{run}/README.md")
    flat_readme = " ".join(readme.split())
    before = _json(ctx, f"{run}/channel-four-keys.json")
    after = _json(ctx, f"{run}/channel-after-vote.json")
    keys = _setup_keys(ctx, run, problems)
    key_set = set(keys)
    _same(problems, f"{run}/run.log and setup logs", "four distinct setup keys", len(key_set), 4)
    _same(problems, f"{run}/facts.json", "channel_keys in setup order", facts.get("channel_keys"), keys)

    expected_keys = sorted(key for key in keys if key is not None)
    _same(problems, f"{run}/channel-four-keys.json", "four accredited setup keys",
          sorted(before.get("accredited_keys") or []), expected_keys)
    _same(problems, f"{run}/channel-after-vote.json", "same four accredited setup keys",
          sorted(after.get("accredited_keys") or []), expected_keys)
    _same(problems, f"{run}/channel-four-keys.json", "configuration_threshold before vote",
          before.get("configuration_threshold"), 3)
    _same(problems, f"{run}/channel-after-vote.json", "configuration_threshold after vote",
          after.get("configuration_threshold"), before.get("configuration_threshold"))
    _same(problems, f"{run}/channel-four-keys.json", "posting_timeframe before vote",
          before.get("posting_timeframe"), 60)
    _same(problems, f"{run}/channel-four-keys.json", "posting_timeframe in facts",
          facts.get("posting_timeframe_before"), before.get("posting_timeframe"))
    _same(problems, f"{run}/channel-after-vote.json", "posting_timeframe after third vote",
          after.get("posting_timeframe"), 90)
    _same(problems, f"{run}/facts.json", "posting_timeframe_voted",
          facts.get("posting_timeframe_voted"), after.get("posting_timeframe"))
    _same(problems, f"{run}/facts.json", "bedrock_took_params", facts.get("bedrock_took_params"), True)
    for i, expected in enumerate((60, 60, 90), 1):
        _same(problems, f"{run}/facts.json", f"channel posting timeframe after vote {i}",
              facts.get(f"channel_posting_timeframe_after_vote_{i}"), str(expected))

    vote_rel = f"{run}/logs/vote.log"
    vote_text = _text(ctx, vote_rel)
    votes = _vote_records(vote_text)
    _same(problems, vote_rel, "ordered vote command count", len(votes), 3)
    expected_outcomes = ("1 of the 3 accredited keys these params need have voted for them",
                         "2 of the 3 accredited keys these params need have voted for them", None)
    genesis = _genesis_params(ctx, run, problems)
    depth = genesis.get("finality_depth")
    expected_params = {"minimum_sequencer_stake": 149, "posting_timeframe": 90, "posting_timeout": 25,
                       "max_block_size": 1048576, "finality_depth": depth}
    for i, vote in enumerate(votes[:3], 1):
        requested, _, block, outcome = vote
        _same(problems, vote_rel, f"vote {i} requested ChannelParams", requested, expected_params)
        expected = expected_outcomes[i - 1]
        if expected is not None:
            _same(problems, vote_rel, f"vote {i} outcome", outcome, expected)
        else:
            outcome_params = _channel_params(outcome)
            _same(problems, vote_rel, "third vote's resulting ChannelParams", outcome_params, expected_params)
            _fragment(problems, vote_rel, outcome, "third vote outcome prefix", "The channel params are now ")
        fact_vote = f"Included in block {block} {outcome} "
        _same(problems, f"{run}/facts.json", f"vote_{i} exact log record", facts.get(f"vote_{i}"), fact_vote)
    _same(problems, f"{run}/logs/seq-0.log", "genesis finality_depth", depth, 35)
    _same(problems, vote_rel, "voted finality_depth equals genesis",
          votes[2][0].get("finality_depth") if len(votes) == 3 else None, depth)
    if "finality_depth" in facts:
        _same(problems, f"{run}/facts.json", "finality_depth from genesis", facts.get("finality_depth"), depth)

    stake_rel = f"{run}/logs/stake.log"
    stakes = _stake_records(_text(ctx, stake_rel))
    _same(problems, stake_rel, "stake command count", len(stakes), 3)
    _same(problems, stake_rel, "three stakes of 149 in log order", [row[0] for row in stakes], [149, 149, 149])
    wallet_rel = f"{run}/logs/wallet.log"
    imported = re.findall(r"(?m)^Imported public account Public/(\S+)$", _text(ctx, wallet_rel))
    if len(imported) >= 2:
        _same(problems, stake_rel, "stake payers are the imported published accounts in order",
              [row[1] for row in stakes], [imported[(i - 1) % 2] for i in range(1, len(stakes) + 1)])
    else:
        problems.append(f"{wallet_rel}: fewer than two imported public accounts")

    _fragment(problems, f"{run}/README.md", flat_readme, "before-vote channel",
              "configuration threshold 3, posting timeframe 60")
    _fragment(problems, f"{run}/README.md", flat_readme, "first/second vote outcomes",
              '"1 of the 3 accredited keys')
    _fragment(problems, f"{run}/README.md", flat_readme, "second vote outcome", '"2 of the 3')
    if len(votes) == 3:
        result_text = "The channel params are now ChannelParams { minimum_sequencer_stake: 149, " \
                      "posting_timeframe: 90, posting_timeout: 25, max_block_size: 1048576, finality_depth: 35 }"
        _fragment(problems, f"{run}/README.md", flat_readme, "third vote's ChannelParams", result_text)
        _fragment(problems, f"{run}/README.md", flat_readme, "third vote block", f"block {votes[2][2]}")
        if run == VOTE_1:
            _same(problems, f"{run}/README.md", "first two vote blocks named in order",
                  [int(n) for n in re.findall(r"\| block (\d+):", readme)], [vote[2] for vote in votes])
    _fragment(problems, f"{run}/README.md", flat_readme, "Bedrock read after vote 3",
              "90 at the first read after the third vote")

    rc, output = run_script(ctx, ["python3", "-I", VOTE_CHECKER, run])
    if not checker_gap and rc != 0:
        problems.append(f"{VOTE_CHECKER}: exit status {rc}: {' | '.join(output[-8:])}")
    elif checker_gap:
        # A run that stopped short: the run's own checker must report exactly that gap, and nothing else.
        reported = [ln for ln in output if ln.strip()]
        if rc != 1 or len(reported) != 1 or checker_gap not in reported[0]:
            problems.append(f"{VOTE_CHECKER}: expected exit 1 reporting only '{checker_gap}', got {rc}: "
                            f"{' | '.join(output[-8:])}")
    return problems, facts, readme, keys, votes, genesis, stakes, after


@check(id="lez040-run-vote-1", family="record", mode="offline", rows=(), evidence=(303,),
       title="The four-sequencer v0.4.0 committee vote and the refused unstake",
       what="setup keys, three stakes, ordered vote blocks and ChannelParams, before/after Bedrock channels, genesis "
            "finality depth without exit_delay, and LEZ's unstake refusal",
       against="raw setup/stake/vote/unstake/sequencer logs and channel JSON, then facts.json, the run README, and "
               "checkpoint/attestor/check_zone_v040.py",
       inputs=_vote_inputs(VOTE_1))
def vote_one(ctx):
    run = VOTE_1
    problems, _, readme, _, votes, _, stakes, after = _vote_common(
        ctx, run, checker_gap="the unstake under L1 finality (run by default) has no record: it did not complete")
    refusal = "it is neither a public account nor a bound private address"
    _pattern(problems, f"{run}/logs/unstake.log", _text(ctx, f"{run}/logs/unstake.log"), "unstake refusal",
             r"FinalizeUnstake cannot pay \S+: " + re.escape(refusal))
    _fragment(problems, f"{run}/README.md", " ".join(readme.split()), "README says unstake was refused",
              "was refused by LEZ's own transaction builder")
    _fragment(problems, f"{run}/README.md", " ".join(readme.split()), "README quotes unstake refusal", refusal)
    fail_if(problems, "LEZ v0.4.0 vote-1 run records differ")
    blocks = [vote[2] for vote in votes]
    return [f"setup logs: four keys; three stakes of 149; votes at blocks {blocks} change posting_timeframe "
            f"60 -> {after['posting_timeframe']}, with threshold 3 and the same keys",
            f"genesis finality_depth {votes[2][0]['finality_depth']} and no exit_delay; unstake refused: {refusal}; "
            f"{VOTE_CHECKER} reports that gap and only it"]


@check(id="lez040-run-vote-2", family="record", mode="offline", rows=(), evidence=(303,),
       title="The four-sequencer v0.4.0 committee vote and the L1-final stake release",
       what="setup keys, stakes and ordered votes; channel configuration; genesis finality; the unstaked key and raw "
            "release; recorded poll heights, seconds and destination balances; 69 = 2 x finality_depth - 1",
       against="raw setup/stake/vote/unstake/sequencer logs and channel JSON, then facts.json, the run README, and "
               "checkpoint/attestor/check_zone_v040.py",
       inputs=_vote_inputs(VOTE_2) + (f"{VOTE_2}/logs/seq-3.log",))
def vote_two(ctx):
    run = VOTE_2
    problems, facts, readme, keys, votes, genesis, _, after = _vote_common(ctx, run)
    flat_readme = " ".join(readme.split())
    unstake_rel = f"{run}/logs/unstake.log"
    unstake_text = _text(ctx, unstake_rel)
    release = re.search(r"(?m)^Releasing (\d+) to (\S+): ([0-9a-f]{64})$", unstake_text)
    included = re.search(r"(?m)^Included in block (\d+)$", unstake_text[release.end():] if release else "")
    _same(problems, unstake_rel, "release command count",
          len(re.findall(r"(?m)^Releasing \d+ to \S+: [0-9a-f]{64}$", unstake_text)), 1)
    paid = int(release.group(1)) if release else -1
    destination = release.group(2) if release else ""
    release_tx = release.group(3) if release else ""
    landing_block = int(included.group(1)) if included else -1
    if release is None:
        problems.append(f"{unstake_rel}: raw Releasing amount/destination/transaction line absent")
    _same(problems, f"{run}/facts.json", "release_paid equals raw Releasing amount", facts.get("release_paid"), paid)
    _same(problems, f"{run}/facts.json", "unstake destination equals raw log", facts.get("unstake_destination"),
          f"Public/{destination}")
    _same(problems, f"{run}/facts.json", "unstake_out exact raw release and inclusion",
          facts.get("unstake_out"), f"Releasing {paid} to {destination}: {release_tx} Included in block {landing_block} ")

    # The run always unstakes sequencer 3; its setup and startup key are raw records, unlike the poll values below.
    key = keys[3] if len(keys) == 4 else None
    seq3_rel = f"{run}/logs/seq-3.log"
    loaded = re.findall(r"Channel signing key loaded public_key=([0-9a-f]{64})\b", _text(ctx, seq3_rel))
    _same(problems, seq3_rel, "seq-3 loaded key is setup's unstaked key", sorted(set(loaded)), [key])
    if key is not None:
        _fragment(problems, f"{run}/README.md", flat_readme, "unstaked key", f"(`{key[:8]}…`) unstaked")

    # These poll values exist only in facts/README: run.log was overwritten before the lines that printed them.
    # Deriving the final balance still uses the raw log's release amount, not facts.json's release_paid.
    def integer(name):
        try:
            return int(facts[name])
        except (KeyError, TypeError, ValueError):
            problems.append(f"{run}/facts.json: {name} is not an integer: {facts.get(name)!r}")
            return -1

    landing_height = integer("unstake_node_height")
    removed_height = integer("key_removed_node_height")
    paid_height = integer("release_node_height")
    landing_seconds = integer("key_removed_after_s")
    paid_seconds = integer("release_after_s")
    before_balance = integer("release_destination_before")
    after_balance = before_balance + paid
    depth = genesis.get("finality_depth")
    distance = paid_height - removed_height
    _same(problems, f"{run}/facts.json", "unstake and removal heights", (landing_height, removed_height), (185, 185))
    _same(problems, f"{run}/facts.json", "paid height", paid_height, 254)
    _same(problems, f"{run}/facts.json", "landing/removal and paid seconds", (landing_seconds, paid_seconds), (30, 306))
    _same(problems, f"{run}/facts.json", "destination balance before funding", before_balance, 1000)
    _same(problems, unstake_rel, "destination balance after raw release", after_balance, 1149)
    _same(problems, f"{run}/facts.json", "node_info_end height at paid observation",
          (facts.get("node_info_end") or {}).get("cryptarchia_info", {}).get("height"), paid_height)
    _same(problems, f"{run}/facts.json and genesis", "release distance 254 - 185 = 69",
          (paid_height, removed_height, distance), (254, 185, 69))
    if isinstance(depth, int):
        _same(problems, f"{run}/facts.json and genesis", "69 = 2 x finality_depth - 1", distance, 2 * depth - 1)
    else:
        problems.append(f"{run}/logs/seq-0.log: no integer genesis finality_depth for the release identity")

    _fragment(problems, f"{run}/README.md", flat_readme, "landing height/seconds and raw release",
              f'| the unstake request lands ("Releasing {paid} to …") | {landing_height} | {landing_seconds} '
              "(the command returns once it is included) |")
    _fragment(problems, f"{run}/README.md", flat_readme, "removal height and seconds",
              f"| the channel drops the key (three accredited keys) | {removed_height} (seen at the first poll) | "
              f"{landing_seconds} |")
    _fragment(problems, f"{run}/README.md", flat_readme, "destination balance after, paid height and seconds",
              f"| the stake is paid (the destination's balance {before_balance:,} → {after_balance:,}) | "
              f"{paid_height} | {paid_seconds} |")
    if isinstance(depth, int):
        for name, fragment in (("README's distance", f"The release came {distance} node blocks after the key's removal"),
                               ("README's 2 x finality_depth - 1", f"2 × `finality_depth` − 1 = {2 * depth - 1}"),
                               ("README's finality_depth", f"`finality_depth` {depth}")):
            _fragment(problems, f"{run}/README.md", flat_readme, name, fragment)
    fail_if(problems, "LEZ v0.4.0 vote-2 run records differ")
    return [f"setup/vote/channel: four keys and threshold 3, three stakes of 149, votes at "
            f"{[vote[2] for vote in votes]}, posting_timeframe 60 -> {after['posting_timeframe']}",
            f"key {key[:8]}… released {paid} to {destination[:8]}…; destination {before_balance:,} -> "
            f"{after_balance:,}; landing/removal {landing_height}/{removed_height} at {landing_seconds} s; paid at "
            f"{paid_height} after {paid_seconds} s; {paid_height} - {removed_height} = {distance} = 2 x {depth} - 1; "
            f"{VOTE_CHECKER} exits 0",
            "the poll heights, seconds and the balance before are the run's facts.json (its run.log was cut short): "
            "held to the README and the node's end height, not re-derived from a raw log; the release itself (149, "
            "its transaction, block) is the raw unstake.log's"]


CLIENTS_RUN = "cli/results/v040-standalone-1"
command(id="lez040-run-clients", family="record", mode="offline", rows=(), evidence=(304,),
        argv=("python3", "-I", "scripts/test_lez040_decode.py", "LiveRun"),
        title="The clients against LEZ v0.4.0's standalone sequencer: two wallet transfers, the second on a rotated key",
        what=f"{CLIENTS_RUN}: the recorded blocks hold the three transfers: two the web wallet's v0.4.0 codec built and "
             "signed (nonces 0 and 1), a third through the page's network layer (nonce 2), each applied, each receipt's fee as recorded, the witness carrying the initial key at "
             "nonce 0 only, and lez-read's v0.4.0 reading of the same blocks",
        against="the blocks as the sequencer served them (decoded by scripts/verifylib/lez040.py, hashes recomputed), "
                "facts.json and lez-read's recorded output (scripts/test_lez040_decode.py, class LiveRun)",
        inputs=(CLIENTS_RUN, "scripts/test_lez040_decode.py", "scripts/verifylib/lez040.py", "scripts/verifylib/lez.py",
                "scripts/verifylib/__init__.py", "deploy/testnet/web/wallet/test/fixtures/lez-v040-vectors.json",
                "deploy/testnet/web/wallet/test/fixtures/lez-v040-standalone-blocks.json"))


CLI_E2E = "cli/results/v040-e2e-1"


@check(id="lez040-run-cli-e2e", family="record", mode="offline", rows=(), evidence=(305,),
       title="The ferrata CLI's end-to-end round trip on LEZ v0.4.0 (dev mode): the README held to the test logs",
       what=f"{CLI_E2E}/README.md's results: deposit_scan_claim_burn_release_and_recovery passed in 87 s, attest_faults "
            "3 passed in 52.8 s, and run 3 (after the deposit's version check) 2 passed in 50.6 s",
       against=f"{CLI_E2E}/e2e.txt, attest-faults.txt and e2e-3.txt, cargo's own test output (a log record: the run "
               "itself is not repeated offline; dev mode, as CI runs these tests)",
       inputs=(f"{CLI_E2E}/README.md", f"{CLI_E2E}/e2e.txt", f"{CLI_E2E}/attest-faults.txt", f"{CLI_E2E}/e2e-3.txt"))
def cli_e2e(ctx):
    problems = []
    readme = " ".join(_text(ctx, f"{CLI_E2E}/README.md").split())
    e2e = _text(ctx, f"{CLI_E2E}/e2e.txt")
    faults = _text(ctx, f"{CLI_E2E}/attest-faults.txt")
    if "test deposit_scan_claim_burn_release_and_recovery ... ok" not in e2e:
        problems.append(f"{CLI_E2E}/e2e.txt: the round trip's test is not reported ok")
    summary = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in "
                        r"([0-9.]+)s", e2e)
    if not summary or re.search(r"(?m)^test .* \.\.\. FAILED$", e2e):
        problems.append(f"{CLI_E2E}/e2e.txt: no passing test summary, or a failed test")
    elif f"passed in {round(float(summary.group(2)))} s" not in readme:
        problems.append(f"{CLI_E2E}/README.md: does not state the round trip's {round(float(summary.group(2)))} s")
    first = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in "
                      r"([0-9.]+)s", faults)
    if not first or re.search(r"(?m)^test .* \.\.\. FAILED$", faults):
        problems.append(f"{CLI_E2E}/attest-faults.txt: no passing attest_faults summary, or a failed test")
    elif f"`attest_faults` {first.group(1)} passed in {float(first.group(2)):.1f} s" not in readme:
        problems.append(f"{CLI_E2E}/README.md: does not state attest_faults' {first.group(1)} passed in "
                        f"{float(first.group(2)):.1f} s")
    e2e3 = _text(ctx, f"{CLI_E2E}/e2e-3.txt")
    third = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in "
                      r"([0-9.]+)s", e2e3)
    if ("test deposit_scan_claim_burn_release_and_recovery ... ok" not in e2e3 or not third
            or re.search(r"(?m)^test .* \.\.\. FAILED$", e2e3)):
        problems.append(f"{CLI_E2E}/e2e-3.txt: run 3's round trip is not reported ok with a passing summary")
    elif f"passed ({third.group(1)} in {float(third.group(2)):.1f} s)" not in readme:
        problems.append(f"{CLI_E2E}/README.md: does not state run 3's {third.group(1)} passed in "
                        f"{float(third.group(2)):.1f} s")
    # Runs 2 and 3 name the tests that passed: each row's names (the backticked test names before its result) must be
    # exactly the tests its log reports ok, so a row cannot name a test the log does not hold.
    rows = {ln.split("|")[1].strip(): ln.split("|")[3] for ln in _text(ctx, f"{CLI_E2E}/README.md").splitlines()
            if re.match(r"^\| [0-9]+ \|", ln) and ln.count("|") >= 4}
    for n, log_name, log, cut in (("2", "e2e.txt", e2e, ":"), ("3", "e2e-3.txt", e2e3, " passed (")):
        cell = rows.get(n)
        if cell is None or cut not in cell:
            problems.append(f"{CLI_E2E}/README.md: no run {n} row naming its tests")
            continue
        named = set(re.findall(r"`([a-z][a-z0-9_]*)`", cell.split(cut, 1)[0]))
        logged = set(re.findall(r"(?m)^test ([a-z][a-z0-9_]*) \.\.\. ok$", log))
        if not named or named != logged:
            problems.append(f"{CLI_E2E}/README.md: run {n} names {sorted(named)}, {log_name} reports ok "
                            f"{sorted(logged)}")
    fail_if(problems, "the CLI's v0.4.0 end-to-end record")
    return [f"e2e.txt: deposit_scan_claim_burn_release_and_recovery ok, {summary.group(1)} passed, 0 failed in "
            f"{summary.group(2)} s; attest-faults.txt: {first.group(1)} passed in {first.group(2)} s; e2e-3.txt (after "
            f"the deposit's version check): {third.group(1)} passed in {third.group(2)} s; the README states all three "
            "(a log record: dev mode, not repeated offline; the version reads are the test's code, tests/e2e.rs, not "
            "lines of the log)"]


HARNESS_RUNS = ("docs/port-v0.4.0/harness-a7a45cb6", "docs/port-v0.4.0/harness-cec9d834")
HARNESS_RUN = HARNESS_RUNS[0]
SHARDS = ("harness-1", "harness-2", "harness-3", "harness-4")
_OK = re.compile(r"(?m)^test (\S+) \.\.\. ok$")
_FAILED = re.compile(r"(?m)^test (\S+) \.\.\. FAILED$")
_IGNORED = re.compile(r"(?m)^test (\S+) \.\.\. ignored")


def _harness_record(ctx, run: str, problems: list) -> str:
    """One record of CI's v0.4.0 shards: its README held to its cargo lines. Returns the summary line."""
    readme = _text(ctx, f"{run}/README.md")
    flat = " ".join(readme.split())
    counts, failed, ignored, shard = {}, [], [], None
    for ln in _text(ctx, f"{run}/tests.txt").splitlines():
        m = re.match(r"^== (harness-\d)$", ln)
        if m:
            shard = m.group(1)
            counts.setdefault(shard, [0, 0])
            continue
        if shard is None:
            continue
        if _OK.match(ln):
            counts[shard][0] += 1
        m = _FAILED.match(ln)
        if m:
            counts[shard][1] += 1
            failed.append(m.group(1))
        m = _IGNORED.match(ln)
        if m:
            ignored.append(m.group(1))
    if tuple(sorted(counts)) != SHARDS:
        problems.append(f"{run}/tests.txt: shards {sorted(counts)}, not {list(SHARDS)}")
    for name, (ok, bad) in sorted(counts.items()):
        if f"| {name} | {ok} | {bad} |" not in readme:
            problems.append(f"{run}/README.md: no row | {name} | {ok} | {bad} | (tests.txt's counts)")
    ok = sum(c[0] for c in counts.values())
    bad = sum(c[1] for c in counts.values())
    if f"| all four | {ok} | {bad} |" not in readme:
        problems.append(f"{run}/README.md: no row | all four | {ok} | {bad} |")
    if f"{ok} of {ok + bad} tests pass, {bad} fail" not in readme.splitlines()[0]:
        problems.append(f"{run}/README.md: its title does not say {ok} of {ok + bad} tests pass, {bad} fail")
    for name in failed + ignored:
        if f"`{name}`" not in flat:
            problems.append(f"{run}/README.md: does not name the {'failed' if name in failed else 'ignored'} test {name}")
    if ignored and f"{len(ignored)} ignored" not in flat:
        problems.append(f"{run}/README.md: does not say {len(ignored)} ignored")
    gate = _text(ctx, f"{run}/updater-gate.txt")
    gate_ok = len(_OK.findall(gate))
    if _FAILED.search(gate) or gate.count("-- exit 0") != 2 or re.search(r"(?m)^-- exit [1-9]", gate):
        problems.append(f"{run}/updater-gate.txt: a failure, or not both commands exiting 0")
    elif f"passed ({gate_ok} of {gate_ok}," not in flat:
        problems.append(f"{run}/README.md: does not state updater-gate's {gate_ok} of {gate_ok}")
    privacy = _text(ctx, f"{run}/updater-privacy.txt")
    before, _, after = privacy.partition("-- bash scripts/privacy/run.sh")
    blocks = re.split(r"(?m)^-- exit \d+$", before)
    updater_ok = [len(_OK.findall(b)) for b in blocks if _OK.search(b)]
    if _FAILED.search(before) or len(updater_ok) != 2:
        problems.append(f"{run}/updater-privacy.txt: the updater's tests are not two passing runs")
    elif f"({updater_ok[0]}, then the remaining {updater_ok[1]} recorded-data tests; none failed)" not in flat:
        problems.append(f"{run}/README.md: does not state the updater's {updater_ok[0]} and {updater_ok[1]}")
    lost = _FAILED.findall(after)
    for name in lost:
        if f"`{name}`" not in flat:
            problems.append(f"{run}: updater-privacy's failed test {name} is not named in its README")
    if not after.strip():
        problems.append(f"{run}/updater-privacy.txt: no privacy suite step")
    return (f"{run.rsplit('/', 1)[-1]}: {', '.join(f'{k} {v[0]}/{v[1]}' for k, v in sorted(counts.items()))} "
            f"(passed/failed), {ok} passed, {bad} failed, {len(ignored)} ignored, each failed or ignored test named; "
            f"updater-gate {gate_ok} passed and the demo; the updater's "
            f"{' + '.join(str(n) for n in updater_ok)} passed; the privacy suite's failures {lost or 'none'}")


@check(id="lez040-run-harness", family="record", mode="offline", rows=(), evidence=(),
       title="CI's v0.4.0 harness shards run on a laptop: each record's README held to its test lines",
       what="each record under docs/port-v0.4.0/harness-*/: passed and failed per shard and in all, every failed or "
            "ignored test named, the updater shards' results",
       against="each record's tests.txt, updater-gate.txt and updater-privacy.txt, cargo's own lines from the run "
               "(a log record: the run is not repeated offline; dev mode)",
       inputs=tuple(f"{run}/{name}" for run in HARNESS_RUNS
                    for name in ("README.md", "tests.txt", "updater-gate.txt", "updater-privacy.txt")))
def harness_run(ctx):
    problems = []
    lines = [_harness_record(ctx, run, problems) for run in HARNESS_RUNS]
    fail_if(problems, "the v0.4.0 harness records")
    return lines + ["log records: dev mode, not repeated offline"]
