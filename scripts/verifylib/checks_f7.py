"""Offline, per-record re-checks of the two new RFP-022 F.7 return-leg runs.

No proof native execution, signature verification, EVM replay or network request is done here.  The check instead
recomputes every count, amount and README figure it reports from the run's retained per-case records, and says where
that stops.  All repository access is relative to ctx.root so the tamper tests exercise precisely these comparisons.
"""
from __future__ import annotations

import csv
import hashlib
import io
import json
import re
from decimal import Decimal

from .core import check, fail_if

ERC20 = "e2e/runs/zone-bridge-rc3-erc20"
ROTATION = "e2e/runs/zone-bridge-rc3-rotation"
FIXTURE_ROTATION = "vault/test/real/fixtures/zone-bridge-rc3-rotation.json"
DEPLOY_SCRIPT = "vault/script/DeployVault.s.sol"
TEST_ERC20 = "vault/test/real/ZoneBridgeRc3Erc20.t.sol"
TEST_ROTATION = "vault/test/real/ZoneBridgeRc3Rotation.t.sol"
TEST_BASE = "vault/test/real/ZoneBridgeRc3Multi.t.sol"
ZERO = "0x" + "0" * 40
WORDS = {1: "one", 2: "two", 3: "three", 4: "four", 5: "five"}


def _text(ctx, rel):
    return ctx.path(rel).read_text(encoding="utf-8")


def _json(ctx, rel):
    return json.loads(_text(ctx, rel))


def _expect(problems, condition, message):
    if not condition:
        problems.append(message)


def _one(pattern, text, label, problems, flags=0):
    found = list(re.finditer(pattern, text, flags))
    _expect(problems, len(found) == 1, f"{label}: expected one match, found {len(found)}")
    return found[0] if len(found) == 1 else None


def _hex_int(value):
    return int(value, 16) if isinstance(value, str) and value.startswith("0x") else int(value)


def _hex_bytes(value):
    return bytes.fromhex(value.removeprefix("0x"))


def _same(a, b):
    return str(a).lower() == str(b).lower()


def _percent(part, whole):
    # The schedules used here have exact finite decimal percentages.
    value = Decimal(part) * 100 / Decimal(whole)
    return format(value.normalize(), "f")


def _functions(text):
    return re.findall(r"\bfunction\s+(test_[A-Za-z0-9_]+)\s*\(", text)


def _release_schedule(run, params, identity, deploy_source):
    """(denominations, registered relay fees, checkpoint fees, decimals) for denomination zero's asset.

    The ERC-20 run carries its complete selected-token schedule in identity.json.  The ETH run's identity has no
    selected-token object: its denominations and fees are the fixed deployment script schedule, checked against that
    source here rather than inferred backwards from paidWei.  The selected burn is still checked against the schedule.
    """
    selected = params.get("selectedToken")
    if selected is None:
        fragments = (
            "_three(0.001 ether, 0.01 ether, 0.1 ether)",
            "_three(2e12, 2e13, 2e14)",
            "_three(1e12, 1e13, 1e14)",
            "18,",
        )
        if not all(fragment in deploy_source for fragment in fragments):
            raise ValueError("ETH deployment schedule changed or is not the fixed schedule this record check reads")
        return [10**15, 10**16, 10**17], [2 * 10**12, 2 * 10**13, 2 * 10**14], [10**12, 10**13, 10**14], 18
    if selected.get("name") != "test-token":
        raise ValueError(f"{run}: unknown selected token {selected.get('name')!r}")
    token = identity.get("selectedToken", {})
    required = ("denominations", "relayFees", "checkpointFees", "decimals")
    if not all(key in token for key in required):
        raise ValueError(f"{run}/identity.json: the selected token's denomination/fee schedule is missing")
    return token["denominations"], token["relayFees"], token["checkpointFees"], token["decimals"]


def _rotation_facts(ctx, run, main, attestor, before, after, realblocks, fixture):
    """The one configuration change: old committee -> five keys, threshold four, three old signers."""
    problems = []
    changes = main["config_changes"]
    _expect(problems, len(changes) == 1, f"rotation: expected one configuration change, found {len(changes)}")
    if len(changes) != 1:
        return problems
    change = changes[0]
    old, new = before["accredited_keys"], after["accredited_keys"]
    _expect(problems, len(old) == 4 and len(set(old)) == 4, "rotation: the channel before it is not four distinct keys")
    _expect(problems, len(new) == 5 and len(set(new)) == 5, "rotation: the channel after it is not five distinct keys")
    _expect(problems, set(old) < set(new) and len(set(new) - set(old)) == 1,
            "rotation: the new channel does not retain the four old keys and add exactly one")
    _expect(problems, before["configuration_threshold"] == 3 and after["configuration_threshold"] == 4,
            "rotation: channel thresholds are not 3 before and 4 after")
    old_proof = ["0x" + k for k in old]
    new_proof = ["0x" + k for k in new]
    _expect(problems, main["parent_keys"] == old_proof and main["keys_after"] == new_proof
            == attestor["key_set"] == fixture["rotatedKeys"],
            "rotation: checkpoint/channel/attestor/fixture key sets differ")
    _expect(problems, fixture["keys"] == old_proof and set(fixture["keys"]) < set(fixture["rotatedKeys"]),
            "rotation fixture: keys are not the four earlier keys all retained in rotatedKeys")
    _expect(problems, change["signer_indices"] == [0, 1, 2],
            f"rotation: signer indices are {change['signer_indices']}, not three of the four earlier keys")
    _expect(problems, len(change["signer_indices"]) == 3 and all(0 <= i < len(old) for i in change["signer_indices"]),
            "rotation: configuration is not signed by three indices of the earlier set")
    _expect(problems, main["n"] == main["t"] + 1 == 5 and main["t"] == after["configuration_threshold"],
            "rotation: checkpoint n/t are not five/four after the change")
    pattern = (f"configuration change in node block {change['node_block']} (slot {change['node_slot']}): "
               f"{len(new)} keys, threshold {after['configuration_threshold']}, posting timeframe "
               f"{after['posting_timeframe']}, signed by key indices [0, 1, 2] of the set before it; "
               f"new configuration tip 0x{after['config_tip_hash']}")
    _expect(problems, pattern in realblocks,
            "rotation: realblocks-crown.txt does not reproduce the configuration change")
    return problems


def _record(ctx, run, rotation=False):
    problems = []
    load = lambda rel: _json(ctx, f"{run}/{rel}")
    read = lambda rel: _text(ctx, f"{run}/{rel}")
    params, deploy, identity = load("params.json"), load("deploy.json"), load("identity.json")
    crown, vault, manifest = load("crown.json"), load("vault-checks.json"), load("manifest.json")
    attestor, burns, execution = load("checkpoint/attestor.json"), load("checkpoint/burns.json"), load("checkpoint/exec.json")
    exec_all = load("checkpoint/exec-all.json")
    collect_all, collect_below = read("checkpoint/collect-all.txt"), read("checkpoint/collect-below.txt")
    exec_below = read("checkpoint/exec-below.txt")
    native_refusals, guest_refusals = read("checkpoint/refusals/native.txt"), read("checkpoint/refusals/guest.txt")
    realblocks, log, readme = read("checkpoint/realblocks-crown.txt"), read("logs/zone-bridge.log"), read("README.md")
    checkpoint_receipt = read("checkpoint/receipts/journal.hex").strip().removeprefix("0x")
    deploy_source = _text(ctx, DEPLOY_SCRIPT)

    tip, parent_tip = burns["tip"], burns["parent_tip"]
    stem = f"zone-{parent_tip}-{tip}"
    main = load(f"checkpoint/{stem}.json")
    all_case = load(f"checkpoint/{stem}-all.json")
    n, t = attestor["n"], attestor["t"]
    key_set, attestations = attestor["key_set"], attestor["attestations"]
    signers = [a["channel_key"] for a in attestations]
    _expect(problems, len(key_set) == len(set(key_set)) == n, f"attestor: n = {n} but key_set has {len(key_set)} entries")
    _expect(problems, len(attestations) == main["attestations"] == t, "attestor: accepted attestation count differs from checkpoint/t")
    _expect(problems, len(set(signers)) == len(signers) and set(signers) <= set(key_set),
            "attestor: accepted channel keys are duplicated or not accredited")
    _expect(problems, all(a["tip_id"] == tip == attestor["tip_id"] for a in attestations),
            "attestor: an accepted attestation is for another tip")
    _expect(problems, (main["n"], main["t"]) == (n, t) and n >= t > 0,
            f"checkpoint: n/t {(main['n'], main['t'])} != attestor {(n, t)}")
    _expect(problems, (main["n_min"], main["t_min"]) == (params["nMin"], params["tMin"]),
            "checkpoint: deployment floors differ from params.json")
    _expect(problems, main["tip"] == tip and main["first_block"] == parent_tip + 1 and main["blocks"] == tip - parent_tip,
            "checkpoint: name/range/block count are inconsistent")
    _expect(problems, len(all_case["attestations"]) == n, f"collector: all-attestation case has {len(all_case['attestations'])}, not {n}")
    _expect(problems, f"kept {n} of {n} (n = {n}, t = {t})" in collect_all
            and f"{n} attestations counted" in collect_all,
            "collector: collect-all.txt does not count every accredited attestation")
    below = load(f"checkpoint/{stem}-below.json")
    _expect(problems, len(below["attestations"]) == t - 1, "collector: below-t case is not t - 1 attestations")
    shortage = f"Core(InsufficientAttestations {{ have: {t - 1}, need: {t} }})"
    _expect(problems, below["expect_refusal"] == shortage and below["native"] == "refused: " + shortage,
            "collector: below-t case names a different native refusal")
    _expect(problems, f"{t - 1} of {n}: refused by the collector" in collect_below and shortage in collect_below,
            "collector: collect-below.txt does not name the shortage")
    _expect(problems, exec_below.count("checkpoint refused: " + shortage) >= 2,
            "collector: guest below-t execution does not name InsufficientAttestations")
    if rotation:
        _expect(problems, len(attestations) == 4 and n == 5 and t == 4, "rotation: selected checkpoint is not four of five, t = 4")
        _expect(problems, exec_all["journal_equals_native"] is True and all_case["journal"] == main["journal"],
                "rotation: all-five execution does not accept and reproduce the native journal")
    else:
        _expect(problems, main["config_changes"] == [], "ERC-20 run unexpectedly carries a configuration change")

    refusal_names = ("below-t", "outsider", "repeated", "stale-tip")
    for name in refusal_names:
        case = load(f"checkpoint/refusals/{stem}-refuse-{name}.json")
        expected = case["expect_refusal"]
        case_label = f"refusal {name}"
        _expect(problems, case["native"] == "refused: " + expected, f"{case_label}: native verdict differs from expect_refusal")
        _expect(problems, f"{stem}-refuse-{name}: refused: {expected}" in native_refusals.splitlines(),
                f"{case_label}: native.txt does not carry exactly its named error")
        _expect(problems, f"{stem}-refuse-{name}: native refused: {expected}; guest checkpoint refused: {expected}"
                in guest_refusals.splitlines(), f"{case_label}: guest/native output does not carry exactly its named error")
        if name == "below-t":
            _expect(problems, expected == f"Core(InsufficientAttestations {{ have: {len(case['attestations'])}, need: {t} }})"
                    and len(case["attestations"]) < t,
                    "refusal below-t: per-case data/error does not count its attestations against current t")
        elif name == "outsider":
            _expect(problems, expected == "Core(NotAccredited)"
                    and any(a["channel_key"] not in key_set for a in case["attestations"]),
                    "refusal outsider: no unaccredited signer in its per-case data")
        elif name == "repeated":
            keys = [a["channel_key"] for a in case["attestations"]]
            _expect(problems, expected == "Core(DuplicateAttestor)" and len(set(keys)) < len(keys),
                    "refusal repeated: no repeated attestor in its per-case data")
        else:
            stale = [a["tip_id"] for a in case["attestations"] if a["tip_id"] != tip]
            _expect(problems, len(stale) == 1 and expected == f"Core(NotADescendant {{ tip_id: {stale[0]} }})"
                    and stale[0] < tip, "refusal stale-tip: case/error does not name its earlier tip")

    journal = main["journal"]
    _expect(problems, execution["journal_equals_native"] is True, "checkpoint exec: journal_equals_native verdict is not true")
    _expect(problems, execution["image_id"] == params["checkpointImageIdB"], "checkpoint exec: image id differs from params")
    _expect(problems, execution["journal_bytes"] == len(_hex_bytes(journal)),
            "checkpoint exec: journal byte length differs from native journal")
    _expect(problems, execution["input_bytes"] == main["input_bytes"] == ctx.path(f"{run}/checkpoint/{stem}.input").stat().st_size,
            "checkpoint exec: input bytes differ from native record/input file")
    _expect(problems, hashlib.sha256(_hex_bytes(journal)).hexdigest() == main["journal_sha256"],
            "checkpoint: journal_sha256 is not the native journal's digest")
    _expect(problems, checkpoint_receipt == journal and crown["candidate"]["journal"].removeprefix("0x") == journal,
            "checkpoint: GPU/crown journal differs from native")
    _expect(problems, burns["burn_count"] == len(burns["burns"]) == main["burn_count"] == main["imported_leaf_count"] == 2,
            "burns: expected two burns and two imported leaves")
    _expect(problems, [b["burn_index"] for b in burns["burns"]] == [0, 1], "burns: indices are not 0 and 1")
    _expect(problems, _same(burns["burn_root"], main["burn_root"]) and _same(vault["burnRoot"], main["burn_root"]),
            "burns: burn roots differ between checkpoint and vault")
    _expect(problems, vault["burnRootAccepted"] is True and _same(vault["finalState"], main["new_state"]),
            "vault: final state/burn-root acceptance differs from checkpoint")
    _expect(problems, f"{stem}: {main['blocks']} blocks, {main['transactions']} transactions" in realblocks,
            "realblocks-crown: checkpoint block/transaction figures differ")
    denominations, relay_fees, checkpoint_fees, decimals = _release_schedule(run, params, identity, deploy_source)
    _expect(problems, all(checkpoint_fees[i] * 1000 == denominations[i] for i in range(3)),
            "schedule: checkpoint fees are not 0.1% of all three denominations")
    if params.get("selectedToken"):
        _expect(problems, all(relay_fees[i] * 400 == denominations[i] for i in range(3)),
                "ERC-20 schedule: relay fees are not 0.25%")
        selected = params["selectedToken"]
        _expect(problems, _same(selected["address"], identity["selectedToken"]["address"])
                and _same(selected["address"], deploy["token"]), "ERC-20: params/identity/deploy token addresses differ")
    expected_paid = []
    crown_by_nullifier = {}
    for index, burn in enumerate(burns["burns"]):
        release = crown["releases"][index]
        denomination_index = release["denomIndex"]
        denomination = denominations[denomination_index]
        checkpoint_fee = checkpoint_fees[denomination_index]
        relay_fee = _hex_int(release["relayFee"])
        native_release = load(f"releases/release-{release['name']}.json")
        _expect(problems, _hex_int(burn["amount"]) == denomination == _hex_int(native_release["amount"]),
                f"release {index}: burn/release amount is not its selected denomination")
        _expect(problems, _same(release["burnRoot"], burns["burn_root"]) and _same(native_release["burn_root"], burns["burn_root"]),
                f"release {index}: burn root differs")
        _expect(problems, _same(release["nullifier"], native_release["nullifier"])
                and _same(release["recipient"], native_release["recipient"]), f"release {index}: statement identity differs")
        _expect(problems, _same(release["relayer"], native_release["relayer"])
                and _hex_int(native_release["relay_fee"]) == relay_fee, f"release {index}: relayer/relay fee differs")
        same_account = _same(release["relayer"], release["recipient"])
        if same_account:
            _expect(problems, relay_fee == 0, f"release {index}: relayer is recipient but relay fee is not zero")
        paid = denomination - checkpoint_fee - relay_fee + (relay_fee if same_account else 0)
        expected_paid.append(paid)
        _expect(problems, _same(release["token"], burn["token"]) and _same(native_release["token"], burn["token"]),
                f"release {index}: token differs from burn")
        crown_by_nullifier[release["nullifier"]] = release
    paid_rows = vault["releases"]
    _expect(problems, len(paid_rows) == len(crown_by_nullifier) == 2, "vault: expected exactly two paid releases")
    seen = set()
    for index, paid in enumerate(expected_paid):
        release = crown["releases"][index]
        matches = [row for row in paid_rows if _same(row["nullifier"], release["nullifier"])]
        _expect(problems, len(matches) == 1, f"vault: release {index} is not present exactly once")
        if len(matches) != 1:
            continue
        row = matches[0]
        seen.add(row["nullifier"])
        _expect(problems, _same(row["recipient"], release["recipient"]) and int(row["paidWei"]) == paid,
                f"vault release {index}: paid amount {row['paidWei']} != derived amount {paid}")
        if "relayFeeUnits" in row:
            _expect(problems, int(row["relayFeeUnits"]) == _hex_int(release["relayFee"])
                    and _same(row["relayer"], release["relayer"]), f"vault release {index}: relayer/fee differs")
        if "token" in row:
            _expect(problems, _same(row["token"], release["token"]), f"vault release {index}: token differs")
    _expect(problems, len(seen) == 2, "vault: paid nullifiers are not two distinct releases")
    replays = {r["what"]: r for r in vault["replaysRefused"]}
    _expect(problems, len(replays) == len(vault["replaysRefused"]) == 3, "vault: expected two release replays and one checkpoint replay")
    for stage, error in (("release-0", "NullifierAlreadyUsed"), ("release-1", "NullifierAlreadyUsed"), ("submit", "ParentNotFinal")):
        mined = manifest["stages"][stage]
        _expect(problems, len(mined) == 1 and mined[0]["status"] == "0x1", f"manifest: {stage} is not one successful mined transaction")
        replay = replays.get(stage)
        _expect(problems, replay is not None and replay["refused"] == error, f"vault replay {stage}: not refused with {error}")
        if replay:
            _expect(problems, _same(replay["transaction"], mined[-1]["hash"]), f"vault replay {stage}: replay transaction differs")
            _expect(problems, f"{stage} (transaction {mined[-1]['hash']}) called again: refused ({error})" in log,
                    f"vault log: {stage} replay/named error is absent")
    for index, paid in enumerate(expected_paid):
        suffix = "base units" if params.get("selectedToken") else "wei"
        _expect(problems, re.search(rf"release {index}: .*recipient {release['recipient']} .*\+{paid} {suffix};", log)
                is not None and len(re.findall(rf"release {index}:", log)) == 1,
                f"vault log: release {index} was not recorded paid once with its derived amount")
    seals = [crown["candidate"]["seal"]] + [r["seal"] for r in crown["releases"]]
    _expect(problems, all(len(_hex_bytes(seal)) == 260 and seal.startswith("0xe71e6620") for seal in seals),
            "seals: checkpoint/releases are not all 260 bytes with selector 0xe71e6620")

    calls = list(csv.DictReader(io.StringIO(read("prover/calls.tsv")), delimiter="\t"))
    summary = read("prover/summary.txt")
    request_names = ("0-checkpoint", "1-release-private-1", "1-release-public-0")
    _expect(problems, len(calls) == 3 and [r["request"] for r in calls] == list(request_names),
            "prover: calls.tsv is not the checkpoint and two releases in order")
    for row in calls:
        _expect(problems, row["attempts"] == "1" and row["ok"] == "true" and row["resumed"] == ("true" if row["request"] == request_names[0] else "false"),
                f"prover {row['request']}: attempts/ok/resumed state differs")
        _expect(problems, row["call id spawned"] == row["call id answered"], f"prover {row['request']}: spawn/answer call ids differ")
        _expect(problems, row["request"] in summary and row["call id spawned"] in summary and f"{row['seconds']}\n" in summary,
                f"prover {row['request']}: summary does not carry its call and seconds")
    _expect(problems, "client 1 killed (SIGKILL)" in summary and "resumed by client 2" in summary
            and "PROVER KILL-RESUME OK" in summary, "prover: kill/resume verdict is absent")
    killed = next((line for line in summary.splitlines() if "client 1 killed (SIGKILL)" in line), "")
    _expect(problems, calls and calls[0]["call id spawned"] in killed, "prover: kill line does not name the checkpoint call")

    if rotation:
        before, after = load("zone/channel-before-rotation.json"), load("zone/channel-after-rotation.json")
        fixture = _json(ctx, FIXTURE_ROTATION)
        problems.extend(_rotation_facts(ctx, run, main, attestor, before, after, realblocks, fixture))
        _expect(problems, "the channel accredits five keys 32 s after the stake was sent" in log,
                "rotation: log does not support the README's 32-second accreditation interval")
        base_count = len(_functions(_text(ctx, TEST_BASE)))
        rotation_count = len(_functions(_text(ctx, TEST_ROTATION)))
        _expect(problems, base_count + rotation_count == 9 and "9 of 9" in readme,
                "rotation README: 9 of 9 is not the seven fixture tests plus two rotation tests")
    else:
        test_count = len(_functions(_text(ctx, TEST_ERC20)))
        _expect(problems, test_count == 2 and "2 of 2" in readme,
                "ERC-20 README: 2 of 2 is not the two ERC-20 suite tests")

    ordinal_n, ordinal_t = WORDS[n], WORDS[t]
    readme_flat = re.sub(r"\s+", " ", readme)
    readme_fragments = [
        "LEZ v0.4.0-rc3", "RISC Zero 3.1", f"ZB_N_MIN={params['nMin']} ZB_T_MIN={params['tMin']}",
        f"{params['checkpointImageIdB'][:10]}…", f"{params['releaseImageId'][:10]}…",
        f"{stem}` ({main['blocks']} blocks, {main['transactions']} transactions; two burns, two imported leaves)",
        f"{ordinal_t} of the {ordinal_n} attestations of tip {tip}", f"n = {n}, t = {t}",
        f"{execution['user_cycles']:,} user cycles", "260-byte Groth16 seal", "`0xe71e6620`",
        "NullifierAlreadyUsed` twice, `ParentNotFinal",
    ]
    seconds = [row["seconds"] for row in calls]
    readme_fragments.append(f"{seconds[0]} s, {seconds[1]} s and {seconds[2]} s; PROVER KILL-RESUME OK")
    unit = f"base units" if params.get("selectedToken") else "wei"
    readme_fragments.append(f"{expected_paid[0]:,} {unit}")
    for fragment in readme_fragments:
        _expect(problems, fragment in readme_flat, f"README: record-derived figure/text {fragment!r} is absent")
    if rotation:
        stale_case = load(f"checkpoint/refusals/{stem}-refuse-stale-tip.json")
        stale = next(a["tip_id"] for a in stale_case["attestations"] if a["tip_id"] != tip)
        for fragment in ("5 keys, threshold 4", "signed by key indices 0, 1 and 2", f"earlier round (tip {stale})",
                         "32 s after the stake was sent", "genesis over the four keys at T0", "9 of 9"):
            _expect(problems, fragment in readme_flat, f"rotation README: record-derived figure/text {fragment!r} is absent")
    else:
        human_denoms = [f"{d // 10**decimals:,}" for d in denominations]
        for fragment in (f"{decimals}-decimal", f"denominations {human_denoms[0]}, {human_denoms[1]} and {human_denoms[2]} TUSD",
                         f"relay fees {_percent(relay_fees[0], denominations[0])} %, checkpoint fees "
                         f"{_percent(checkpoint_fees[0], denominations[0])} %",
                         f"{expected_paid[0]:,} base units each** ({human_denoms[0]} TUSD less the "
                         f"{_percent(checkpoint_fees[0], denominations[0])} % checkpoint fee", "2 of 2"):
            _expect(problems, fragment in readme_flat, f"ERC-20 README: record-derived figure/text {fragment!r} is absent")
        claim_proof = _one(r"ClaimPrivately: deposit 1 at LEZ's private test account 0 \(real proof\): "
                           r"executed, proven and sent in ([0-9.]+) s", log, "ERC-20 private-claim timing", problems)
        if claim_proof:
            _expect(problems, f"{claim_proof.group(1)} s" in readme,
                    "ERC-20 README/log: private-claim proof seconds differ")
        burn_proof = _one(rf"Burn \(private\): {human_denoms[0]} TUSD of private test account 0, note 1 \(real proof\): "
                          r"executed, proven and sent in ([0-9.]+) s", log, "ERC-20 private-burn timing", problems)
        if burn_proof:
            _expect(problems, f"{burn_proof.group(1)} s" in readme,
                    "ERC-20 README/log: private-burn proof seconds differ")

    fail_if(problems, f"F.7 {run.rsplit('/', 1)[-1]} record")
    lines = [
        f"attestations: {len(attestations)} of n = {n}, t = {t}, all accredited and for tip {tip}; collector accepts {n}/{n} "
        f"and refuses {t - 1}; four per-case refusals agree natively and in the guest",
        f"checkpoint {stem}: {main['blocks']} blocks, {main['transactions']} transactions, two burns; journal bytes and SHA-256 "
        f"match native/GPU/crown; {execution['user_cycles']:,} user cycles",
        f"releases: derived from denomination/checkpoint schedule plus relay/recipient accounting: {expected_paid}; each is in "
        "vault-checks.json once; both replayed NullifierAlreadyUsed, checkpoint ParentNotFinal",
        f"prover: three successful calls in {', '.join(seconds)} s; only the checkpoint resumed the same call after SIGKILL",
        "README: denominations/fees, n/t/floors, tips, blocks, transactions, leaves, burns, payments, cycles, prover seconds, "
        "seal bytes/selector and recorded suite counts agree with the run/source records",
        "LIMIT: this reconciles retained verdicts/journals/calls; it does not verify attestation signatures or seals, re-run "
        "native/guest execution, or replay the vault calls on an EVM",
        "LIMIT: the README's Forge totals are checked against the current test functions (two ERC-20, seven base plus two "
        "rotation), not against a retained Forge execution log",
    ]
    if rotation:
        lines.insert(4, "rotation: one change signed by old indices 0,1,2; four earlier keys retained with one new key; "
                     "channel/checkpoint/attestor/fixture all give five keys and threshold four")
    else:
        lines.append("LIMIT: private claim/burn seconds (455.6/533.7 s) are log-to-README reconciliations, not new timings")
    if not params.get("selectedToken"):
        lines.append("LIMIT: identity.json has no ETH schedule; this check binds the fixed DeployVault.s.sol ETH denominations "
                     "and 0.1% checkpoint fees, then matches the selected burn; it does not read historical schedule 0 from anvil")
    return lines


_COMMON_INPUTS = (DEPLOY_SCRIPT, TEST_BASE)


@check(id="f7-zone-bridge-rc3-erc20", family="record", mode="offline", tier="quick", rows=(), evidence=(),
       title="F.7 rc3 ERC-20 return leg: per-case records recomputed offline",
       what="three-of-four attestations and every refusal, native/GPU journals, two burns, schedule-derived releases/replays, "
            "three prover calls and every run README figure",
       against=f"{ERC20}'s JSON/text records and the named Forge test source; no status row cites this check yet",
       inputs=(ERC20, TEST_ERC20) + _COMMON_INPUTS)
def f7_erc20(ctx):
    return _record(ctx, ERC20)


@check(id="f7-zone-bridge-rc3-rotation", family="record", mode="offline", tier="quick", rows=(), evidence=(),
       title="F.7 rc3 committee rotation and ETH return leg: per-case records recomputed offline",
       what="four-of-five attestations/refusals, native/GPU journals, two burns, ETH releases/replays, prover calls, "
            "committee change, rotatedKeys fixture and run README figures",
       against=f"{ROTATION}'s records, {FIXTURE_ROTATION} and the named Forge test sources; no status row cites this check yet",
       inputs=(ROTATION, FIXTURE_ROTATION, TEST_ROTATION) + _COMMON_INPUTS)
def f7_rotation(ctx):
    return _record(ctx, ROTATION, rotation=True)
