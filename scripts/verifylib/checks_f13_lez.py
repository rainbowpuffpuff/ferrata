"""RFP-022 F.13 (ADR 0048): re-derive the three LEZ-fee records' facts offline.

The protocol fees are escrowed by the vault (`lezFees`) and minted on LEZ, in the wrapped token, by
`CollectFees`; claims and burns carry no fee value.  Three records show it: the both-halves Markdown
summary (a per-case summary, not a transaction replay: that limit is reported), the rc3 sequencer round
trip (checked from facts.json through the CLI's own JSON recoveries and printed transactions, the
collections' records, the LEZ transaction ledger, steps.tsv and docs/image-ids.md), and the real-proof
record of the bridge program.  No network or toolchain is used and every path is relative to ctx.root.
"""
from __future__ import annotations

import csv
import io
import json
import re

from .core import check, fail_if

NEW_FEES = "e2e/both-halves/results/f13-lez/fees.md"
RT = "e2e/runs/f13-lezfee-1"
PROOFS = "lez/results/f13/prove-bridge-lezfee.md"
IMAGES = "docs/image-ids.md"
BRIDGE_ROW = "ferrata_bridge_lezfee"
STEPS = (
    "fork",
    "zone: programs and the stand-in instance",
    "vault deployed on the fork",
    "deposit under schedule 0",
    "fee change",
    "deposit under schedule 1",
    "operator import",
    "fees",
    "claim of leaf 0 (schedule 0)",
    "claim of leaf 1 (schedule 1)",
    "mint fees collected",
    "nothing more to collect",
    "claim again refused",
    "burn",
    "checkpoint journal",
    "checkpoint accepted",
    "release journal",
    "release paid",
    "record after the release",
    "burn fee collected",
    "older record refused",
    "escrow backs LEZ",
    "release again refused",
)


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _expect(problems: list, condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def _one(pattern: str, text: str, label: str, problems: list, flags: int = 0):
    matches = list(re.finditer(pattern, text, flags))
    _expect(problems, len(matches) == 1, f"{label}: expected one match for {pattern!r}, found {len(matches)}")
    return matches[0] if len(matches) == 1 else None


def _pipe_table(text: str, first_header: str) -> list[list[str]]:
    lines = text.splitlines()
    for i, line in enumerate(lines):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells and cells[0] == first_header and i + 2 < len(lines):
            rows = []
            for following in lines[i + 2:]:
                if not following.startswith("|"):
                    break
                row = [c.strip() for c in following.strip().strip("|").split("|")]
                if len(row) == len(cells):
                    rows.append(row)
            return rows
    return []


def _json_stdout(ctx, rel: str) -> dict:
    """The last complete CLI JSON answer, with the wallet's sync lines ignored."""
    answers = [ln for ln in _text(ctx, rel).splitlines() if ln.startswith('{"command"')]
    if len(answers) != 1:
        raise ValueError(f"{rel}: expected one CLI JSON answer, found {len(answers)}")
    return json.loads(answers[0])


def _holding(answer: dict, account: int) -> int:
    holdings = [h for h in answer["result"]["holdings"]
                if h["kind"] == "public" and h["token"] == "0x" + "0" * 40 and h["account"] == account]
    if len(holdings) != 1:
        raise ValueError(f"recover: expected one ETH holding of public /{account}, found {len(holdings)}")
    return int(holdings[0]["amount"])


def _last_line(ctx, rel: str) -> dict:
    """The last JSON line of a relayer transcript (its sync lines ignored)."""
    lines = [ln for ln in _text(ctx, rel).splitlines() if ln.startswith("{")]
    if not lines:
        raise ValueError(f"{rel}: no JSON line")
    return json.loads(lines[-1])


def _num(value):
    """A facts/JSON figure as an int, or None when it is not one."""
    try:
        return int(str(value))
    except (TypeError, ValueError):
        return None


@check(id="f13-lez-both-halves", family="record", mode="offline", tier="quick", rows=("RFP-022 F.13",),
       evidence=(),
       title="ADR 0048 both halves: the fees escrowed on Ethereum, minted on LEZ in totals",
       what="the schedule's notice and per-denomination fees, six claim rows over an unsplit set, the first collection "
            "(the mint fees), three releases, the second collection (the burn fees), the refused older record, "
            "`fees(cid, ETH)` and the final supply against the escrow",
       against=f"{NEW_FEES}'s own rows (the harness's Markdown summary of cli/tests/both_halves.rs)",
       inputs=(NEW_FEES,))
def f13_lez_both_halves(ctx):
    problems = []
    text = _text(ctx, NEW_FEES)

    headline = _one(
        r"Schedule 1 announced at (\d+), in force at (\d+) \(seven days and (\d+) s later\), from leaf (\d+): "
        r"mint \[([\d, ]+)\], burn \[([\d, ]+)\], checkpoint \[([\d, ]+)\] wei for the denominations "
        r"0\.1, 1 and 10 ETH\.", text, "both-halves schedule", problems)
    rows = _pipe_table(text, "leaf")
    if headline is None or len(rows) != 6:
        _expect(problems, len(rows) == 6, f"both-halves: expected six claim rows, found {len(rows)}")
        fail_if(problems, "F.13-lez both-halves record")
        return []
    announced, effective, extra, split = map(int, headline.groups()[:4])
    mint, burn, checkpoint = ([int(x) for x in headline.group(i).split(",")] for i in (5, 6, 7))
    denominations = [10**17, 10**18, 10**19]
    _expect(problems, effective - announced == 7 * 86400 + extra and extra == 1,
            f"both-halves: notice is {effective - announced}s, headline says seven days and {extra}s")
    _expect(problems, all(len(v) == 3 for v in (mint, burn, checkpoint)), "both-halves: fee arrays are not triplets")
    for i, denomination in enumerate(denominations):
        _expect(problems, mint[i] * 200 == denomination, f"both-halves: mint[{i}] is not 0.5% of its denomination")
        _expect(problems, burn[i] * 500 == denomination, f"both-halves: burn[{i}] is not 0.2% of its denomination")
        _expect(problems, checkpoint[i] * 1000 == denomination,
                f"both-halves: checkpoint[{i}] is not 0.1% of its denomination")

    relay_values = set()
    for position, row in enumerate(rows):
        _expect(problems, len(row) == 5, f"both-halves claim {position}: five columns expected, got {len(row)}")
        if len(row) != 5:
            continue
        leaf, schedule, relay, before, at_claim = map(int, row)
        relay_values.add(relay)
        _expect(problems, leaf == position, f"both-halves claim {position}: leaf {leaf}")
        _expect(problems, schedule == (0 if leaf < split else 1),
                f"both-halves leaf {leaf}: schedule {schedule} disagrees with change at leaf {split}")
        _expect(problems, before == len(rows),
                f"both-halves leaf {leaf}: pre-claim set {before}, not the whole {len(rows)}-deposit set")
        _expect(problems, at_claim == len(rows) - position,
                f"both-halves leaf {leaf}: plaintext set {at_claim}, not {len(rows) - position}")
    _expect(problems, relay_values == {10**15}, f"both-halves: registered relay fees are {sorted(relay_values)}")
    _expect(problems, "| leaf | schedule at deposit | relay fee (wei) | set before any claim | set at this claim |" in text,
            "both-halves: the claim table's headings changed")
    _expect(problems, "Every claim of 1 ETH mints the same fixed relay fee and no mint fee." in text
            and f"{len(rows)} and {len(rows)} on both sides of the change" in text
            and f"does not split the set at leaf {split}" in text,
            "both-halves: no-mint-fee / unsplit-set statement changed")

    collected = _one(r"After the claims, one `CollectFees` under the record of the claims' root minted (\d+) wei, "
                     r"the vault's `lezFees` then \((\d+): the three schedule-1 deposits' mint fees\)", text,
                     "both-halves collection 1", problems)
    if collected:
        minted1, mint_total = map(int, collected.groups())
        _expect(problems, minted1 == mint_total == 3 * mint[1],
                f"both-halves: the first collection minted {minted1} of the vault's {mint_total}, "
                f"not 3 x the 1 ETH mint fee {3 * mint[1]}")
    _expect(problems, "the same collection again minted nothing (244)" in text,
            "both-halves: the second collection under the same record is not refused (244)")
    _expect(problems, "Nothing was credited on Ethereum at the deposits." in text,
            "both-halves: the record no longer states that nothing was credited on Ethereum at the deposits")

    release = _one(r"Each recipient received (\d+) wei: 1 ETH less the burn fee (\d+) and the checkpoint fee (\d+); "
                   r".*?the vault's `totalOwed` rose by (\d+) wei over (\d+) releases\), the burn fee kept in escrow "
                   r"and counted in `lezFees` \(\+(\d+) wei\)", text, "both-halves release", problems)
    if release:
        received, release_burn, release_checkpoint, owed, releases, burn_total = map(int, release.groups())
        _expect(problems, release_burn == burn[1] and release_checkpoint == checkpoint[1],
                "both-halves: release does not use schedule 1's 1 ETH burn/checkpoint fees")
        _expect(problems, received == denominations[1] - release_burn - release_checkpoint,
                "both-halves: recipient release arithmetic does not hold")
        _expect(problems, owed == releases * release_checkpoint,
                f"both-halves: totalOwed rose by {owed}, not {releases} x the checkpoint fee alone "
                f"({releases * release_checkpoint})")
        _expect(problems, burn_total == releases * burn[1],
                f"both-halves: lezFees grew by {burn_total}, not the releases' burn fees ({releases * burn[1]})")
        second = _one(r"A second `CollectFees`, under a record after the releases, minted (\d+) wei \(the burn fees\)",
                      text, "both-halves collection 2", problems)
        if second:
            _expect(problems, int(second.group(1)) == burn_total,
                    "both-halves: the second collection did not mint the burn fees lezFees grew by")
    _expect(problems, "the earlier record's smaller total was then refused (247)" in text,
            "both-halves: the older record's smaller total is not refused (247)")
    counted = _one(r"`fees\(cid, ETH\)` counts (\d+) wei collected", text, "both-halves fees(cid, ETH)", problems)
    if counted and collected and release:
        _expect(problems, int(counted.group(1)) == int(collected.group(1)) + int(release.group(6)),
                "both-halves: fees(cid, ETH) does not count both collections' sum")
    final = _one(r"the wrapped supply on LEZ, (\d+) wei, is the vault's escrow, (\d+) wei", text,
                 "both-halves final supply", problems)
    if final:
        _expect(problems, int(final.group(1)) == int(final.group(2)) > 0,
                "both-halves: the final wrapped supply is not the vault's final escrow")
    fail_if(problems, "F.13-lez both-halves record")
    return [
        f"schedule 1: {effective - announced}s notice; three denominations' mint/burn/checkpoint fees are "
        "0.5%/0.2%/0.1%",
        "6 claims: leaves 0-5, schedule change at leaf 3, pre-claim set 6 throughout, plaintext sets 6,5,4,3,2,1; "
        "one fixed relay fee and no fee value in any claim",
        f"collection 1 minted {3 * mint[1]} wei (the vault's lezFees then: the three schedule-1 deposits' mint fees); "
        f"releases paid {denominations[1] - burn[1] - checkpoint[1]} wei each, totalOwed +{3 * checkpoint[1]} "
        f"(the checkpoint fees alone), collection 2 minted {3 * burn[1]} wei (the burn fees)",
        "the older record refused (247), the same collection twice refused (244); fees(cid, ETH) counts both "
        "collections; the final supply equals the final escrow",
        "LIMIT: this is the harness's Markdown summary; it has no raw transactions or balances to replay",
    ]


@check(id="f13-lez-roundtrip", family="record", mode="offline", tier="quick", rows=("RFP-022 F.13",),
       evidence=(),
       title="ADR 0048 rc3 round trip: the fees minted on LEZ end to end, against the CLI and ledger records",
       what="23 ordered ok steps; the bridge program's image; two deposits' prices and escrow/lezFees deltas; two "
            "claims from the recoveries' JSON; two CollectFees that minted (the mint fee, then the burn fee) and two "
            "the program refused (the same record again: 244; an older record: 247), from the collections' records "
            "and the LEZ ledger; the release's fees and the final conservation",
       against=f"{RT}/run (steps.tsv, facts.json, the user transcripts and recoveries, run/lez/1*-collect-*.json, the "
               "LEZ transaction ledger and the loader session); docs/image-ids.md's named row",
       inputs=(f"{RT}/run", IMAGES))
def f13_lez_roundtrip(ctx):
    problems = []
    facts = json.loads(_text(ctx, f"{RT}/run/facts.json"))
    fees = facts["fees"]
    steps = [line.split("\t") for line in _text(ctx, f"{RT}/run/steps.tsv").splitlines() if line]
    by_name = {row[0]: row for row in steps if len(row) == 4}
    _expect(problems, [r[0] for r in steps] == list(STEPS),
            f"roundtrip steps: order/names are {[r[0] for r in steps]!r}")
    _expect(problems, len(steps) == len(STEPS) and all(len(r) == 4 and r[1] == "ok" and r[2].isdigit() for r in steps),
            f"roundtrip steps: not {len(STEPS)} well-formed ok rows")
    _expect(problems, fees.get("adr") == "0048", "roundtrip: facts' fees section is not ADR 0048's")

    image = fees["bridgeProgram"]
    _expect(problems, image.get("row") == BRIDGE_ROW,
            f"roundtrip: facts' bridge row is {image.get('row')!r}, not {BRIDGE_ROW}")
    image_pattern = rf"\| [^|\n]*`{re.escape(image['row'])}` \| `({image['imageId']})` \|"
    _expect(problems, re.search(image_pattern, _text(ctx, IMAGES)) is not None,
            f"image id: docs/image-ids.md has no {image.get('row')} row with {image.get('imageId')}")
    session = json.loads(_text(ctx, f"{RT}/run/lez/session-a.json"))
    _expect(problems, session["bridge"]["imageId"] == image["imageId"],
            "image id: the loader session differs from facts.fees.bridgeProgram")
    _expect(problems, f"bridge program {image['imageId'][:12]}…" in by_name[STEPS[1]][3],
            "image id: the zone step's abbreviated id differs")

    denomination = _num(facts["deposit"]["denominationWei"])
    relay = _num(fees["relayFeeWei"])
    lez_ledger = list(csv.DictReader(io.StringIO(_text(ctx, f"{RT}/run/lez/transactions.tsv")), delimiter="\t"))
    lez_by_hash = {r["tx_hash"]: r for r in lez_ledger}
    deposit_texts = {
        "schedule0": _text(ctx, f"{RT}/run/user/04-deposit.txt"),
        "schedule1": _text(ctx, f"{RT}/run/user/06-deposit-2.txt"),
    }
    claim_texts = {
        "schedule0": _text(ctx, f"{RT}/run/user/08-claim.txt"),
        "schedule1": _text(ctx, f"{RT}/run/user/08c-claim-2.txt"),
    }
    recovery_names = {
        "schedule0": (f"{RT}/run/user/09-recover.txt", f"{RT}/run/user/09b-recover.txt"),
        "schedule1": (f"{RT}/run/user/09c-recover.txt", f"{RT}/run/user/09d-recover.txt"),
    }
    recoveries = {key: (_json_stdout(ctx, names[0]), _json_stdout(ctx, names[1]))
                  for key, names in recovery_names.items()}
    first_destination = _holding(recoveries["schedule0"][0], 0)
    first_relayer = _holding(recoveries["schedule0"][1], 1)
    second_destination = _holding(recoveries["schedule1"][0], 0) - first_destination
    second_relayer = _holding(recoveries["schedule1"][1], 1) - first_relayer
    observed_claim_amounts = {"schedule0": (first_destination, first_relayer),
                              "schedule1": (second_destination, second_relayer)}

    for index, key in enumerate(("schedule0", "schedule1")):
        schedule = fees[key]
        step = by_name[f"deposit under schedule {index}"][3]
        deposit_text = deposit_texts[key]
        deposit_file = "04-deposit.txt" if index == 0 else "06-deposit-2.txt"
        fields = {}
        for field, label in (("denomination", "denomination"), ("mint fee", "mintFeeWei"),
                             ("relay fee", "relayFeeWei"), ("total", "total")):
            m = _one(rf"^  {field}\s+[^(\n]*\((\d+) base units,", deposit_text, f"{deposit_file} {field}", problems, re.M)
            if m:
                fields[label] = int(m.group(1))
        mint = _num(schedule["mintFeeWei"])
        _expect(problems, fields.get("denomination") == denomination,
                f"{key}: CLI denomination {fields.get('denomination')} != facts {denomination}")
        _expect(problems, fields.get("mintFeeWei") == mint,
                f"{key}: CLI mint fee {fields.get('mintFeeWei')} != facts {mint}")
        _expect(problems, fields.get("relayFeeWei") == relay,
                f"{key}: CLI relay fee {fields.get('relayFeeWei')} != facts {relay}")
        _expect(problems, None not in (denomination, mint, relay)
                and fields.get("total") == denomination + mint + relay,
                f"{key}: deposit total is not denomination + mint + relay")
        escrow = _num(schedule["escrowDeltaWei"])
        lezfees = _num(schedule["lezFeesDeltaWei"])
        if index == 0:
            _expect(problems, escrow == denomination + relay,
                    f"{key}: escrow delta {escrow} != denomination + relay {denomination + relay}")
            _expect(problems, lezfees == 0, f"{key}: lezFees grew by {lezfees} at a deposit with no mint fee")
        else:
            _expect(problems, escrow == denomination + relay + mint,
                    f"{key}: escrow delta {escrow} != denomination + relay + mint {denomination + relay + mint}")
            _expect(problems, lezfees == _num(schedule["lezFeesAccruedEventsWei"]) == mint,
                    f"{key}: lezFees delta {lezfees} / LezFeesAccrued {_num(schedule['lezFeesAccruedEventsWei'])} "
                    f"is not the mint fee {mint}")
        _expect(problems, _num(schedule["feeRecipientCreditDeltaWei"]) == 0,
                f"{key}: the Ethereum fee recipient was credited at the deposit (ADR 0048: nothing)")
        if index == 1:
            _expect(problems, _num(schedule["owedCreditedEventsWei"]) == 0,
                    "schedule1: the deposit's OwedCredited events credit something (ADR 0048: none)")
        sent = _one(r"^sent: (0x[0-9a-f]{64}) in block (\d+)$", deposit_text, f"{deposit_file} sent", problems, re.M)
        _expect(problems, sent is not None and sent.group(1) == schedule["depositTx"],
                f"{key}: CLI deposit transaction differs from facts")
        _expect(problems, schedule["depositTx"] in step, f"{key}: step deposit transaction differs from facts")
        _expect(problems, f"escrow +{escrow} wei" in step and f"lezFees +{lezfees}" in step,
                f"{key}: the deposit step's escrow/lezFees figures differ from facts")

        claim_text = claim_texts[key]
        claim_step = by_name[f"claim of leaf {index} (schedule {index})"][3]
        claim_sent = _one(r"^sent: ([0-9a-f]{64})$", claim_text, f"{key} claim sent", problems, re.M)
        claim_hash = schedule["claim"]["tx"]
        _expect(problems, claim_sent is not None and claim_sent.group(1) == claim_hash,
                f"{key}: CLI claim transaction differs from facts")
        _expect(problems, claim_hash in claim_step, f"{key}: claim step transaction differs from facts")
        ledger = lez_by_hash.get(claim_hash)
        _expect(problems, ledger is not None and ledger["program"] == "bridge"
                and ledger["bridge_variant"] == "Claim" and ledger["reverted"] == "false"
                and ledger["kind"] == "public",
                f"{key}: LEZ ledger does not show the public bridge Claim un-reverted")
        _expect(problems, f"--leaf {index} " in claim_text.splitlines()[0],
                f"{key}: CLI claim command does not name leaf {index}")
        _expect(problems, not re.search(r"schedule|range", claim_text, re.I),
                f"{key}: plaintext claim output names a schedule or range")
        observed_destination, observed_relayer = observed_claim_amounts[key]
        _expect(problems, _num(schedule["claim"]["mintedToDestinationWei"]) == observed_destination == denomination,
                f"{key}: destination delta/facts/denomination differ ({observed_destination}, {denomination})")
        _expect(problems, _num(schedule["claim"]["paidToRelayerWei"]) == observed_relayer == relay,
                f"{key}: relayer delta/facts/registered fee differ ({observed_relayer}, {relay})")
        _expect(problems, str(observed_destination) in claim_step and str(observed_relayer) in claim_step,
                f"{key}: claim step amounts differ from the recoveries")

    _expect(problems, fees["ethereumFeeRecipient"] in by_name["deposit under schedule 1"][3],
            "roundtrip: step fee recipient differs from facts")

    mint_fee = _num(fees["schedule1"]["mintFeeWei"])
    burn_fee = _num(fees["schedule1"]["burnFeeWei"])
    lezfees_after_deposits = _num(fees["schedule0"]["lezFeesDeltaWei"]) + _num(fees["schedule1"]["lezFeesDeltaWei"])
    _expect(problems, lezfees_after_deposits == mint_fee,
            "roundtrip: lezFees after both deposits is not the schedule-1 mint fee")

    ledger_end = max(int(r["block"]) for r in lez_ledger)
    fee_recipient_lez = fees["lezFeeRecipient"]

    def collection_record(file: str, label: str) -> dict:
        record = json.loads(_text(ctx, f"{RT}/run/lez/{file}"))
        step = record.get("step") or {}
        tx = str(record.get("tx", ""))
        _expect(problems, re.fullmatch(r"[0-9a-f]{64}", tx) is not None and step.get("tx") == tx,
                f"{label}: the record's transaction and its sequencer step differ")
        _expect(problems, record.get("token") == "0x" + "0" * 40, f"{label}: the token is not ETH")
        _expect(problems, record.get("feeRecipient") == fee_recipient_lez,
                f"{label}: the fee recipient {record.get('feeRecipient')} is not the configuration's")
        _expect(problems, f"header(id, {record.get('recordSlot')})" in str(step.get("step", ""))
                and f"block {record.get('block')}" in str(step.get("step", "")),
                f"{label}: the sequencer step names another record")
        row = lez_by_hash.get(tx)
        if _num(step.get("block")) is not None and _num(step.get("block")) <= ledger_end:
            _expect(problems, row is not None and row["block"] == str(step.get("block"))
                    and row["program"] == "bridge" and row["bridge_variant"] == "CollectFees"
                    and row["kind"] == "public"
                    and row["reverted"] == ("false" if record.get("applied") is True else "true"),
                    f"{label}: the LEZ ledger does not show this CollectFees "
                    f"{'applied' if record.get('applied') is True else 'reverted'} in block {step.get('block')}")
        else:
            _expect(problems, row is None and _num(step.get("block")) is not None,
                    f"{label}: a block the ledger covers, or none")
        return record

    def collection(which: str, file: str, index: int, amount: int, collected_before: int, lezfees_at: int):
        label = f"collection {index + 1}"
        record = collection_record(file, label)
        entry = fees["collections"][index]
        step = by_name[which][3]
        _expect(problems, record.get("event") == "lez-fees-collected" and record.get("applied") is True
                and record.get("exit") is None and (record.get("step") or {}).get("status") == "applied",
                f"{label}: not applied on the zone ({record.get('event')!r}, exit {record.get('exit')!r})")
        _expect(problems, _num(record.get("vaultTotal")) == lezfees_at,
                f"{label}: vaultTotal {_num(record.get('vaultTotal'))} is not lezFees then ({lezfees_at})")
        _expect(problems, _num(record.get("amount")) == amount,
                f"{label}: minted {_num(record.get('amount'))}, not {amount}")
        _expect(problems, _num(record.get("collectedBefore")) == collected_before
                and _num(record.get("collectedAfter")) == collected_before + amount == lezfees_at,
                f"{label}: fees(cid, ETH) did not go {collected_before} -> {lezfees_at}")
        _expect(problems, _num(record.get("feeRecipientHoldingBefore")) == collected_before
                and _num(record.get("feeRecipientHoldingAfter")) == collected_before + amount,
                f"{label}: the fee recipient's holding did not go {collected_before} -> "
                f"{collected_before + amount}")
        tx = str(record.get("tx", ""))
        _expect(problems, tx == entry["tx"], f"{label}: the record's tx differs from facts")
        _expect(problems, _num(entry["vaultTotalWei"]) == lezfees_at and _num(entry["mintedWei"]) == amount
                and _num(entry["feeRecipientHoldingAfterWei"]) == collected_before + amount,
                f"{label}: facts' summary figures differ from the record")
        _expect(problems, tx in step and f"lezFees {lezfees_at} attested" in step
                and f"holding {collected_before} -> {collected_before + amount}" in step
                and f"header(id, {record.get('recordSlot')}) of block {record.get('block')}" in step,
                f"{label}: the step's figures differ from the record")
        return record

    def refused(which: str, file: str, label: str, code: int, collected: int, vault_total: int, slot):
        record = collection_record(file, label)
        _expect(problems, record.get("event") == "lez-fee-collection-refused" and record.get("applied") is False
                and record.get("exit") == code
                and (record.get("step") or {}).get("status") == "refused"
                and str((record.get("step") or {}).get("exit", "")).startswith(f"{code} "),
                f"{label}: not refused by the program with {code} ({record.get('event')!r}, exit {record.get('exit')!r})")
        _expect(problems, _num(record.get("amount")) == 0
                and _num(record.get("collectedBefore")) == _num(record.get("collectedAfter")) == collected
                and _num(record.get("feeRecipientHoldingBefore")) == _num(record.get("feeRecipientHoldingAfter"))
                == collected,
                f"{label}: something was minted or counted ({record.get('amount')}; fees(cid, ETH) "
                f"{record.get('collectedBefore')} -> {record.get('collectedAfter')})")
        _expect(problems, _num(record.get("vaultTotal")) == vault_total and record.get("recordSlot") == slot,
                f"{label}: the record read another total ({record.get('vaultTotal')}) or slot ({record.get('recordSlot')})")
        step = by_name[which][3]
        _expect(problems, f"refused by the program with {code}" in step and str(record.get("tx")) in step,
                f"{label}: the step does not state the refusal")
        return record

    first = collection("mint fees collected", "17-collect-mint.json", 0, mint_fee, 0, lezfees_after_deposits)
    refused("nothing more to collect", "18-collect-again.json", "the same collection again", 244, mint_fee,
            lezfees_after_deposits, first.get("recordSlot"))

    release = facts["release"]
    fr = fees["release"]
    amount = _num(release["amountWei"])
    recipient = _num(release["recipientBalanceWei"])
    release_relay = _num(release["relayFeeWei"])
    credited = _num(release["creditedFeesWei"])
    _expect(problems, None not in (amount, recipient, release_relay, credited, burn_fee)
            and amount == recipient + release_relay + credited + burn_fee,
            "release: amount != recipient + relay fee + checkpoint credit + burn fee")
    _expect(problems, credited == _num(fees["schedule1"]["checkpointFeeWei"]),
            "release: the credited fee is not schedule 1's checkpoint fee")
    _expect(problems, _num(fr["burnFeeAccruedEventsWei"]) == burn_fee,
            "release: the LezFeesAccrued events are not the burn fee")
    _expect(problems, _num(fr["escrowBeforeWei"]) is not None
            and _num(fr["escrowBeforeWei"]) - _num(fr["escrowAfterWei"]) == denomination - burn_fee,
            "release: escrow before - after != denomination - burn fee")
    lezfees_after_release = _num(fr["lezFeesAfterWei"])
    _expect(problems, lezfees_after_release == mint_fee + burn_fee,
            "release: lezFees after the release is not the mint fee + the burn fee")
    release_step = by_name["release paid"][3]
    _expect(problems, release["tx"] in release_step
            and all(str(x) in release_step for x in (amount, recipient, release_relay, credited, burn_fee))
            and f"lezFees {lezfees_after_deposits} -> {lezfees_after_release}" in release_step,
            "release: the step's figures differ from facts")

    second = collection("burn fee collected", "19-collect-burn.json", 1, burn_fee, mint_fee, lezfees_after_release)
    _expect(problems, isinstance(second.get("recordSlot"), int) and isinstance(first.get("recordSlot"), int)
            and second["recordSlot"] > first["recordSlot"]
            and f"header(id, {second['recordSlot']}) of block {second.get('block')}" in by_name["record after the release"][3],
            "the burn fee's collection does not read the record made after the release")
    refused("older record refused", "19b-collect-older.json", "the older record", 247, mint_fee + burn_fee,
            lezfees_after_deposits, first.get("recordSlot"))

    cons = fees["conservation"]
    user0, user1 = _num(cons["user0Wei"]), _num(cons["user1Wei"])
    fee_recipient = _num(cons["feeRecipientWei"])
    supply, escrow_end = _num(cons["lezSupplyWei"]), _num(cons["escrowWei"])
    _expect(problems, None not in (user0, user1, fee_recipient, supply, escrow_end)
            and escrow_end == supply == user0 + user1 + fee_recipient,
            "conservation: escrow != the wrapped supply != /0 + /1 + the fee recipient's holding")
    _expect(problems, _holding(_json_stdout(ctx, f"{RT}/run/user/20-recover.txt"), 0) == user0,
            "conservation: the user's /0 differs from the end recovery")
    _expect(problems, _holding(_json_stdout(ctx, f"{RT}/run/user/20b-recover.txt"), 1) == user1,
            "conservation: the relayer /1 differs from the end recovery")
    _expect(problems, fee_recipient == mint_fee + burn_fee,
            "conservation: the fee recipient's holding is not both collections")
    escrow_step = by_name["escrow backs LEZ"][3]
    _expect(problems, f"escrow {escrow_end} wei = the wrapped ETH on LEZ {supply}" in escrow_step
            and all(str(x) in escrow_step for x in (user0, user1, fee_recipient)),
            "conservation: the step's figures differ from facts")

    fee_change = by_name["fee change"][3]
    _expect(problems, f"mint fee {mint_fee}" in fee_change and f"burn fee {burn_fee}" in fee_change
            and "schedule 1 in force" in fee_change,
            "fee change: the step's fees differ from facts")

    fail_if(problems, "F.13-lez round-trip record")
    return [
        f"{len(STEPS)}/{len(STEPS)} named steps in order and ok; both public Claims un-reverted in the LEZ ledger",
        f"deposits: escrow deltas {fees['schedule0']['escrowDeltaWei']} (denomination + relay) and "
        f"{fees['schedule1']['escrowDeltaWei']} (+ the mint fee); lezFees deltas 0 and {mint_fee} "
        "(LezFeesAccrued), nothing credited on Ethereum",
        f"claims from the recoveries' JSON: /0 +{first_destination}/+{second_destination}, /1 "
        f"+{first_relayer}/+{second_relayer}; neither claim names a schedule or range",
        f"collections: {mint_fee} then {burn_fee} wei minted to the configuration's fee recipient on LEZ "
        f"(holding 0 -> {mint_fee} -> {mint_fee + burn_fee}); the same record again refused (244) and the older "
        "record refused (247), nothing minted",
        f"release: {amount} = {recipient} + {release_relay} + {credited} + {burn_fee}; escrow -"
        f"{denomination - burn_fee}; conservation escrow = supply = {escrow_end}",
        f"image {BRIDGE_ROW} = {image['imageId']} agrees with docs/image-ids.md and the loader session",
        "LIMIT: the run's Ethereum state deltas (escrow, lezFees, owed) come from `cast` reads the hook recorded "
        "in steps.tsv and facts.json, not from receipts retained in the record; the LEZ ledger's decoding of the "
        f"bridge calls is the sequencer's, and the ledger was exported at the checkpoint (through block {ledger_end}): "
        "the later collections are checked from crown-driver's records of the sequencer's answers",
    ]


@check(id="f13-lez-prove", family="record", mode="offline", tier="quick", rows=("RFP-022 F.13",),
       evidence=(),
       title="ADR 0048's bridge program: a private claim and a private burn proven for real",
       what="the record's bridge image id against docs/image-ids.md's ferrata_bridge_lezfee row (and the ELF size), "
            "RISC0_DEV_MODE off, and the claim's and the burn's REAL PROOF, written and settled lines with their "
            "byte counts and gas figures",
       against=f"{PROOFS} against {IMAGES}'s named row",
       inputs=(PROOFS, IMAGES))
def f13_lez_prove(ctx):
    problems = []
    text = _text(ctx, PROOFS)
    docs = _text(ctx, IMAGES)
    row = _one(r"\| [^|\n]*`ferrata_bridge_lezfee` \| `(0x[0-9a-f]{64})` \| `[0-9a-f]{64}` \| ([\d,]+) \|",
               docs, "docs image row", problems)

    head = _one(r"^bridge image (?:0x)?([0-9a-f]{64}) \((\d+) ELF bytes\); RISC0_DEV_MODE (on|off); shape \w+$",
               text, "prove bridge image", problems, re.M)
    if head:
        _expect(problems, row is not None and head.group(1) == row.group(1).removeprefix("0x"),
                "prove: the record's bridge image differs from docs/image-ids.md's ferrata_bridge_lezfee row")
        _expect(problems, row is not None and int(head.group(2)) == int(row.group(2).replace(",", "")),
                "prove: the record's ELF bytes differ from docs/image-ids.md's row")
        _expect(problems, head.group(3) == "off", "prove: RISC0_DEV_MODE is not off")
    pinned = _one(r"FERRATA_EXPECT_BRIDGE_IMAGE_ID=(?:0x)?([0-9a-f]{64})", text, "prove pinned image", problems)
    _expect(problems, pinned is not None and head is not None
            and pinned.group(1) == head.group(1),
            "prove: the command does not pin the record's bridge image")

    def proven(what: str):
        proof = _one(rf"^{what}: REAL PROOF: execute_and_prove (\d+(?:\.\d+)?) s \(prove \+ settle (\d+(?:\.\d+)?) s\); "
                     rf"receipt \(succinct InnerReceipt, borsh\) (\d+) bytes; transaction (\d+) bytes$",
                     text, f"prove {what}", problems, re.M)
        wrote = _one(rf"^{what}: wrote the proven transaction to \S+ \((\d+) bytes\)$", text,
                     f"prove {what} written", problems, re.M)
        settled = _one(rf"^{what}: settled: execution gas ([\d,]+) \(PRIVATE_VERIFY_GAS ([\d,]+) \+ public part "
                       rf"([\d,]+) cycles\), storage gas ([\d,]+), base fee ([\d,]+); receipt at bytes "
                       rf"([\d,]+)..([\d,]+) of ([\d,]+)$", text, f"prove {what} settled", problems, re.M)
        if proof:
            prove_s, total_s = float(proof.group(1)), float(proof.group(2))
            receipt, tx_bytes = int(proof.group(3)), int(proof.group(4))
            _expect(problems, total_s >= prove_s, f"prove: {what}'s prove + settle time is below its prove time")
            _expect(problems, 0 < receipt < tx_bytes, f"prove: {what}'s receipt does not fit its transaction")
            _expect(problems, wrote is not None and int(wrote.group(1)) == tx_bytes,
                    f"prove: {what}'s written transaction size differs from its proof's")
        if settled:
            g = [int(x.replace(",", "")) for x in settled.groups()]
            _expect(problems, g[0] == g[1] + g[2],
                    f"prove: {what}'s execution gas is not PRIVATE_VERIFY_GAS + its public part")
            if proof:
                _expect(problems, g[7] == int(proof.group(4)),
                        f"prove: {what}'s settled byte count differs from its transaction")
                _expect(problems, g[6] - g[5] == int(proof.group(3)) and 0 < g[5] < g[6],
                        f"prove: {what}'s receipt is not at the bytes its size gives")
        return proof

    claim = proven("private claim")
    burn = proven("private burn")
    _expect(problems, "private burn: one private-to-public delivery of the 185-byte Burn to the token's account, "
            "from an alias" in text, "prove: the burn's delivery statement changed")
    none_holds = _one(r"^private claim: of its (\d+) transaction bytes, none holds the destination, s, C, the leaf, "
                     r"the depositor, the witness or any of (\d+) path siblings; N, configId and the root read are "
                     r"present$", text, "prove claim privacy", problems, re.M)
    if none_holds and claim:
        _expect(problems, int(none_holds.group(1)) == int(claim.group(4)),
                "prove: the claim's no-private-inputs statement names another byte count than its transaction")
        _expect(problems, int(none_holds.group(2)) >= 1, "prove: the claim names no path siblings")
    _expect(problems, "N, configId and the root read are present" in text,
            "prove: the claim's public-fields statement changed")

    fail_if(problems, "F.13-lez real-proof record")
    minted = [int(m.group(4)) for m in (claim, burn) if m]
    return [
        f"the bridge image 0x{head.group(1) if head else '?'} (ELF {head.group(2) if head else '?'} bytes) is "
        "docs/image-ids.md's ferrata_bridge_lezfee row, pinned by FERRATA_EXPECT_BRIDGE_IMAGE_ID; RISC0_DEV_MODE off",
        f"a private claim and a private burn, each REAL PROOF and settled"
        + (f" ({', '.join(str(b) for b in minted)} transaction bytes)" if minted else ""),
        "the claim's transaction bytes hold none of the destination, s, C, the leaf, the depositor, the witness or "
        "any path sibling; the settled gas figures add up (execution = PRIVATE_VERIFY_GAS + public part, the "
        "receipt at the bytes its size gives)",
        "LIMIT: the receipts and the proven transactions are not committed (the record quotes the command's "
        "output); nothing is re-proven here",
    ]
