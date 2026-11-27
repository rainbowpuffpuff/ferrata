"""RFP-022 F.13 (ADR 0046): re-derive the two records' fee facts offline.

The both-halves Markdown is a per-case summary, not a transaction replay: its tables,
schedule and release arithmetic are checked here, and that limit is reported.  The
round-trip check goes further, from facts.json to the CLI's own JSON recoveries and
printed transactions, the LEZ transaction ledger, steps.tsv and docs/image-ids.md.
No network or toolchain is used and every path is relative to ctx.root.
"""
from __future__ import annotations

import csv
import io
import json
import re

from .core import check, fail_if

NEW_FEES = "e2e/both-halves/results/f13/fees.md"
OLD_FEES = "e2e/both-halves/results/fees.md"
RT = "e2e/runs/f13-roundtrip-1"
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
    "claim again refused",
    "burn",
    "checkpoint journal",
    "checkpoint accepted",
    "release journal",
    "release paid",
    "release again refused",
)


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _expect(problems: list, condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def _integer(text: str) -> int:
    return int(text.replace(",", "").replace("_", ""))


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


@check(id="f13-both-halves-fees", family="record", mode="offline", tier="quick", rows=(), evidence=(),
       title="ADR 0046 both halves: schedule arithmetic, six claims and one anonymity set",
       what="the schedule's notice and per-denomination fees, six claim rows and their decreasing sets, three releases, "
            "and the absence of a claim mint-fee column",
       against=f"{NEW_FEES}'s own rows, with {OLD_FEES} as the pre-ADR contrast; no status row cites these records yet",
       inputs=(NEW_FEES, OLD_FEES))
def f13_both_halves(ctx):
    problems = []
    text = _text(ctx, NEW_FEES)
    old = _text(ctx, OLD_FEES)

    headline = _one(
        r"Schedule 1 announced at (\d+), in force at (\d+) \(seven days and (\d+) s later\), from leaf (\d+): "
        r"mint \[([\d, ]+)\], burn \[([\d, ]+)\], checkpoint \[([\d, ]+)\] wei for the denominations "
        r"0\.1, 1 and 10 ETH", text, "both-halves schedule", problems)
    rows = _pipe_table(text, "leaf")
    old_rows = _pipe_table(old, "leaf")
    if headline is None or len(rows) != 6:
        _expect(problems, len(rows) == 6, f"both-halves: expected six claim rows, found {len(rows)}")
        fail_if(problems, "F.13 both-halves record")
        return []
    announced, effective, extra, split = map(int, headline.groups()[:4])
    mint, burn, checkpoint = ([_integer(x) for x in headline.group(i).split(",")] for i in (5, 6, 7))
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
        _expect(problems, len(row) == 5, f"both-halves claim {position}: five ADR-0046 columns expected, got {len(row)}")
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
            "both-halves: ADR-0046 claim headings changed")
    _expect(problems, "Every claim of 1 ETH mints the same fixed relay fee and no mint fee." in text
            and "6 and 6 on both sides of the change" in text and "does not split the set at leaf 3" in text,
            "both-halves: no-mint-fee / unsplit-set statement changed")
    _expect(problems, "mint fee (wei, to the fee recipient on LEZ)" not in text,
            "both-halves: the new claim table still exposes a mint fee on LEZ")

    _expect(problems, len(old_rows) == 6 and all(len(r) == 6 for r in old_rows),
            "pre-ADR record: expected six rows including a mint-fee column")
    if len(old_rows) == 6 and all(len(r) == 6 for r in old_rows):
        for position, row in enumerate(old_rows):
            leaf, schedule, minted, relay, before, at_claim = map(int, row)
            within = position if position < 3 else position - 3
            _expect(problems, (leaf, schedule, before, at_claim) ==
                    (position, 0 if position < 3 else 1, 3, 3 - within),
                    f"pre-ADR leaf {position}: expected its 3-and-3 split and decreasing within-schedule set")
            _expect(problems, minted == (0 if schedule == 0 else mint[1]) and relay == next(iter(relay_values)),
                    f"pre-ADR leaf {position}: old mint/relay fee differs from the schedules")
    _expect(problems, "3 and 3 with the change" in old and "fee change splits the set at leaf 3" in old,
            "pre-ADR record: the 3-and-3 contrast statement changed")

    release = _one(r"Each recipient received (\d+) wei: 1 ETH less the burn fee (\d+) and the checkpoint fee (\d+), "
                   r".*?`totalOwed` rose by (\d+) wei over (\d+) releases", text, "both-halves release", problems)
    if release:
        received, release_burn, release_checkpoint, owed, releases = map(int, release.groups())
        _expect(problems, release_burn == burn[1] and release_checkpoint == checkpoint[1],
                "both-halves: release does not use schedule 1's 1 ETH burn/checkpoint fees")
        _expect(problems, received == denominations[1] - release_burn - release_checkpoint,
                "both-halves: recipient release arithmetic does not hold")
        _expect(problems, owed == releases * (release_burn + release_checkpoint),
                "both-halves: totalOwed delta is not releases * (burn + checkpoint fee)")
    fail_if(problems, "F.13 both-halves record")
    return [
        f"schedule 1: {effective - announced}s notice; three denominations' mint/burn/checkpoint fees are 0.5%/0.2%/0.1%",
        f"6 claims: leaves 0–5, schedule change at {split}, pre-claim set 6 throughout, plaintext sets 6,5,4,3,2,1; "
        "one fixed relay fee and no LEZ mint-fee field",
        "the pre-ADR table independently retains its contrasting 3-and-3 sets and nonzero schedule-1 mint fees",
        f"release: 1 ETH - {burn[1]} - {checkpoint[1]} = {denominations[1] - burn[1] - checkpoint[1]} wei; "
        f"3 releases credit {3 * (burn[1] + checkpoint[1])} wei",
        "LIMIT: this is the harness's Markdown summary; it has no raw transactions or balances to replay",
    ]


@check(id="f13-roundtrip-record", family="record", mode="offline", tier="quick", rows=(), evidence=(),
       title="ADR 0046 rc3 round trip: all F.13 fee facts against the CLI and ledger records",
       what="17 ordered ok steps; two deposits/claims, escrow and owed deltas, LEZ application, image id, release and "
            "every numeric F.13 statement",
       against=f"{RT}/run/user, steps.tsv, facts.json and LEZ transaction ledger; docs/image-ids.md's named row",
       inputs=(f"{RT}/F13.md", f"{RT}/run", "docs/image-ids.md"))
def f13_roundtrip(ctx):
    problems = []
    facts = json.loads(_text(ctx, f"{RT}/run/facts.json"))
    fees = facts["fees"]
    steps_text = _text(ctx, f"{RT}/run/steps.tsv")
    steps = [line.split("\t") for line in steps_text.splitlines() if line]
    by_name = {row[0]: row for row in steps if len(row) == 4}
    _expect(problems, [r[0] for r in steps] == list(STEPS),
            f"roundtrip steps: order/names are {[r[0] for r in steps]!r}")
    _expect(problems, len(steps) == 17 and all(len(r) == 4 and r[1] == "ok" and r[2].isdigit() for r in steps),
            "roundtrip steps: not 17 well-formed ok rows")

    image = fees["bridgeProgram"]
    image_pattern = rf"\| [^|\n]*`{re.escape(image['row'])}` \| `({image['imageId']})` \|"
    _expect(problems, re.search(image_pattern, _text(ctx, "docs/image-ids.md")) is not None,
            f"image id: docs/image-ids.md has no {image['row']} row with {image['imageId']}")
    session = json.loads(_text(ctx, f"{RT}/run/lez/session-a.json"))
    _expect(problems, session["bridge"]["imageId"] == image["imageId"],
            "image id: the loader session differs from facts.fees.bridgeProgram")
    _expect(problems, f"bridge program {image['imageId'][:12]}…" in by_name[STEPS[1]][3],
            "image id: the zone step's abbreviated id differs")

    denomination = int(facts["deposit"]["denominationWei"])
    relay = int(fees["relayFeeWei"])
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
        _expect(problems, fields.get("denomination") == denomination,
                f"{key}: CLI denomination {fields.get('denomination')} != facts {denomination}")
        _expect(problems, fields.get("relayFeeWei") == relay,
                f"{key}: CLI relay fee {fields.get('relayFeeWei')} != facts {relay}")
        mint = int(schedule["mintFeeWei"])
        _expect(problems, fields.get("mintFeeWei") == mint,
                f"{key}: CLI mint fee {fields.get('mintFeeWei')} != facts {mint}")
        _expect(problems, fields.get("total") == denomination + mint + relay,
                f"{key}: deposit total is not denomination + mint + relay")
        escrow = int(schedule["escrowDeltaWei"])
        owed = int(schedule["feeRecipientCreditDeltaWei"])
        _expect(problems, escrow == denomination + relay,
                f"{key}: escrow delta {escrow} != denomination + relay {denomination + relay}")
        _expect(problems, owed == mint, f"{key}: owed delta {owed} != deposit mint fee {mint}")
        _expect(problems, f"escrow +{escrow} wei" in step, f"{key}: step escrow delta differs from facts")
        credit_fragment = "credit +0" if index == 0 else f"credited {owed} wei at the deposit (OwedCredited)"
        _expect(problems, credit_fragment in step, f"{key}: step fee-recipient credit differs from facts")
        sent = _one(r"^sent: (0x[0-9a-f]{64}) in block (\d+)$", deposit_text, f"{deposit_file} sent", problems, re.M)
        _expect(problems, sent is not None and sent.group(1) == schedule["depositTx"],
                f"{key}: CLI deposit transaction differs from facts")
        _expect(problems, schedule["depositTx"] in step, f"{key}: step deposit transaction differs from facts")

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
        claim_relay = _one(r"^  relay fee\s+[^(\n]*\((\d+) base units,", claim_text, f"{key} claim relay", problems, re.M)
        _expect(problems, claim_relay is not None and int(claim_relay.group(1)) == relay,
                f"{key}: claim relay fee differs from facts")
        observed_destination, observed_relayer = observed_claim_amounts[key]
        _expect(problems, int(schedule["claim"]["mintedToDestinationWei"]) == observed_destination == denomination,
                f"{key}: destination delta/facts/denomination differ ({observed_destination}, {denomination})")
        _expect(problems, int(schedule["claim"]["paidToRelayerWei"]) == observed_relayer == relay,
                f"{key}: relayer delta/facts/registered fee differ ({observed_relayer}, {relay})")
        _expect(problems, str(observed_destination) in claim_step and str(observed_relayer) in claim_step,
                f"{key}: claim step amounts differ from the recoveries")

    _expect(problems, fees["feeRecipient"] in by_name["deposit under schedule 1"][3],
            "roundtrip: step fee recipient differs from facts")
    if "owedCreditedEventsWei" in fees["schedule1"]:
        event_total = int(fees["schedule1"]["owedCreditedEventsWei"])
        _expect(problems, event_total == int(fees["schedule1"]["mintFeeWei"])
                == int(fees["schedule1"]["feeRecipientCreditDeltaWei"]),
                "roundtrip: OwedCredited summary is not the schedule-1 mint fee and owed delta")
        _expect(problems, "OwedCredited" in by_name["deposit under schedule 1"][3],
                "roundtrip: OwedCredited fact has no corresponding step observation")

    release = facts["release"]
    _expect(problems, int(release["amountWei"]) == int(release["recipientBalanceWei"])
            + int(release["relayFeeWei"]) + int(release["creditedFeesWei"]),
            "roundtrip: release amount != recipient + relay + credited fees")
    for key in ("amountWei", "recipientBalanceWei", "relayFeeWei", "creditedFeesWei"):
        _expect(problems, release[key] in by_name["release paid"][3], f"roundtrip: release step lacks facts.{key}")

    fee_change = by_name["fee change"][3]
    announced = _one(r"mint fees (\d+) (\d+) (\d+) wei", fee_change, "roundtrip announced mint fees", problems)
    announced_fees = tuple(map(int, announced.groups())) if announced else ()
    _expect(problems, announced_fees == tuple(d // 200 for d in (10**15, 10**16, 10**17)),
            f"roundtrip: announced fees are not 0.5% of its denominations: {announced_fees}")
    _expect(problems, bool(announced_fees) and announced_fees[0] == int(fees["schedule1"]["mintFeeWei"]),
            "roundtrip: schedule-1 denomination-0 mint fee differs from announceFees")
    _expect(problems, "seven days passed" in fee_change and "schedule 1 in force" in fee_change,
            "roundtrip: fee-change notice/activation statement changed")

    doc = _text(ctx, f"{RT}/F13.md")
    c = lambda n: f"{n:,}"
    short_image = image["imageId"][:10] + "…" + image["imageId"][-4:]
    doc_fragments = [
        "What ran, every step ok (17 of 17)",
        f"escrow +{c(int(fees['schedule0']['escrowDeltaWei']))} wei",
        f"relay fee {c(relay)}",
        "the fee recipient's credit (`owed`) +0",
        "mint fees " + " / ".join(c(x) for x in announced_fees) + " wei",
        f"the fee recipient credited {c(int(fees['schedule1']['feeRecipientCreditDeltaWei']))} wei at the deposit "
        "(`owed` and one `OwedCredited` event)",
        f"escrow +{c(int(fees['schedule1']['escrowDeltaWei']))} wei, the same as under schedule 0",
        f"/0 +{c(int(fees['schedule1']['claim']['mintedToDestinationWei']))} and /1 "
        f"+{c(int(fees['schedule1']['claim']['paidToRelayerWei']))}, the same amounts as under schedule 0",
        f"the release paid {c(int(release['recipientBalanceWei']))} wei",
        short_image,
        "250 `ALREADY_CLAIMED`",
        "ImportRoot` n = 1 and n = 2",
        "claim of leaf 0 (schedule 0)",
        "claim of leaf 1 (schedule 1)",
    ]
    for fragment in doc_fragments:
        _expect(problems, fragment in doc, f"F13.md no longer states the derived figure/text {fragment!r}")
    fail_if(problems, "F.13 round-trip record")
    return [
        "17/17 named steps in order and ok; both public Claim hashes are un-reverted in the LEZ ledger",
        f"deposits: escrow deltas = denomination + relay = {denomination + relay} wei; owed deltas = mint fees "
        f"0 / {fees['schedule1']['mintFeeWei']} wei, from CLI prices and step deltas",
        f"claims from recover JSON: destination +{first_destination}/+{second_destination}, relayer "
        f"+{first_relayer}/+{second_relayer}; neither claim transcript names a schedule/range",
        f"image {image['row']} = {image['imageId']} agrees with docs and the loader session; F13.md's fee/release figures "
        "agree with facts.json",
        "LIMIT: no deposit receipt is retained and run/eth/transactions.tsv is header-only, so OwedCredited and the "
        "escrow/owed state deltas are summary reconciliations, not independently decoded Ethereum events/state reads",
    ]
