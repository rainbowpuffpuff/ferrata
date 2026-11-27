"""Quick offline checks for bridge rows whose committed records have per-case data.

These checks reconcile records; they do not rerun Solidity, Rust, Logos or network jobs. Every repository read goes
through ctx.root so scripts/test_verify_bridge_rows.py can alter one value on a minimal copy. LIMIT lines identify
parts of a status row that these records cannot independently establish.
"""
from __future__ import annotations

import csv
import io
import json
import re

from .core import check, fail_if
from .rows import status_rows

STATUS = "docs/requirements-status.md"
CATALOG = "modules/catalog/install-test-landclimber-2026-11-25"
FIRST_CATALOG = "modules/catalog/install-test-landclimber-2026-11-24"
INTEGRITY = "releases/RELEASE-INTEGRITY.md"
DEPLOYER = "docs/guides/runs/2026-11-21-second-deployer"
DEPLOYER_RUN = "e2e/runs/seqnet-second-deployer-2"
CLI_RUN = "e2e/runs/seqnet-cli-1"
MODULES = (
    "ferrata_attest_module", "ferrata_attestor_module", "ferrata_bridge_module", "ferrata_monitor_module",
    "ferrata_relayer_module", "ferrata_updater_module", "ferrata_bridge_ui",
)


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _json(ctx, rel: str):
    return json.loads(_text(ctx, rel))


def _rows(ctx, row_id: str) -> str:
    found = [r for r in status_rows(ctx.root) if r.rfp == "RFP-022" and r.id == row_id]
    if len(found) != 1:
        raise ValueError(f"expected one RFP-022 {row_id} row, found {len(found)}")
    return found[0].text


def _expect(problems: list, condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def _phrases(problems: list, text: str, where: str, *fragments: str) -> None:
    for fragment in fragments:
        _expect(problems, fragment in text, f"{where} no longer states {fragment!r}")


def _int(value: str, base: int = 10) -> int:
    return int(value.replace(",", "").replace("**", "").strip(), base)


def _tsv(ctx, rel: str) -> list[dict]:
    return list(csv.DictReader(io.StringIO(_text(ctx, rel)), delimiter="\t"))


def _release_packages(ctx) -> dict:
    """The integrity manifest's per-release rows, keyed (module, version): root hash and .lgx SHA-256."""
    out = {}
    for line in _text(ctx, INTEGRITY).splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 11 or not re.fullmatch(r"`ferrata_\w+`", cells[0]) or not re.fullmatch(r"`\d+\.\d+\.\d+`", cells[1]):
            continue
        module, version = cells[0].strip("`"), cells[1].strip("`")
        out[(module, version)] = {"root": cells[5].strip("`"), "sha256": cells[6].strip("`")}
    return out


@check(id="bridge-f1-live-deposits", title="F.1: both live runs' four ETH deposit receipts and recomputed commitments",
       family="record", mode="offline", tier="quick", rows=("RFP-022 F.1",), evidence=(41, 91),
       what="two depositETH receipts per live run: status, vault, block, gas and hash against the run's receipt table, plus its verifier's per-leaf amount/kind result",
       against="e2e/runs/crown-live-{1,3}/{manifest.json,deploy.json,README.md,verify.txt} and RFP-022 F.1",
       inputs=(STATUS, *(f"e2e/runs/crown-live-{n}/{name}" for n in (1, 3)
                         for name in ("manifest.json", "deploy.json", "README.md", "verify.txt"))))
def bridge_f1_live_deposits(ctx):
    problems = []
    receipts = 0
    for run in ("crown-live-1", "crown-live-3"):
        base = f"e2e/runs/{run}"
        manifest = _json(ctx, f"{base}/manifest.json")
        deploy = _json(ctx, f"{base}/deploy.json")
        readme = _text(ctx, f"{base}/README.md")
        verified = _text(ctx, f"{base}/verify.txt")
        _expect(problems, manifest["chainId"] == deploy["chainId"] == 11155111, f"F.1: {run}: not Sepolia's chain id")
        _expect(problems, deploy["complete"] is True and deploy["lastStep"] == "done", f"F.1: {run}: deployment incomplete")
        leaves = re.search(r"PASS deposits: two logs, commitments recomputed: j0 leaf 0 (\d+) wei public; j1 leaf 1 \1 wei private", verified)
        _expect(problems, leaves is not None and int(leaves.group(1)) == 1_000_000_000_000_000,
                f"F.1: {run}: the verifier's two 0.001-ETH public/private leaf checks changed")
        _expect(problems, len(re.findall(r"^PASS ", verified, re.M)) == 14 and "14 of 14 checks passed" in verified
                and "FAIL" not in verified, f"F.1: {run}: verifier suite is not 14 of 14")
        for i, kind in enumerate(("public", "private")):
            entries = manifest["stages"].get(f"deposit-{i}", [])
            _expect(problems, len(entries) == 1, f"F.1: {run}: deposit-{i} has {len(entries)} receipts")
            if len(entries) != 1:
                continue
            receipt = entries[0]
            receipts += 1
            _expect(problems, receipt["status"] == "0x1", f"F.1: {run} deposit-{i}: status {receipt['status']}")
            _expect(problems, receipt["to"].lower() == deploy["vault"].lower(), f"F.1: {run} deposit-{i}: wrong vault")
            tx_line = next((line for line in readme.splitlines() if receipt["hash"] in line), "")
            _expect(problems, bool(tx_line) and "`depositETH`" in tx_line and kind in tx_line,
                    f"F.1: {run} deposit-{i}: no matching depositETH/{kind} receipt-table row")
            gas = _int(receipt["gasUsed"], 16) if isinstance(receipt["gasUsed"], str) else receipt["gasUsed"]
            block = _int(receipt["blockNumber"], 16) if isinstance(receipt["blockNumber"], str) else receipt["blockNumber"]
            _expect(problems, f"{gas:,}" in tx_line and f"{block:,}" in tx_line,
                    f"F.1: {run} deposit-{i}: manifest gas/block {gas:,}/{block:,} differs from README")
            if i == 0:
                _expect(problems, "1,002,000,000,000,000 wei escrowed" in tx_line,
                        f"F.1: {run} deposit-0: escrow including relay fee changed")
            else:
                _expect(problems, "the same, leaf 1" in tx_line,
                        f"F.1: {run} deposit-1: no longer explicitly the same escrow as deposit 0")
    row = _rows(ctx, "F.1")
    _phrases(problems, row, "F.1", "ETH deposited live on Sepolia", "ETH, 6- and 18-decimal and USDT-style deposits")
    fail_if(problems, "F.1 live-deposit record/status mismatch")
    return [f"{receipts} depositETH receipts over crown-live-1 and crown-live-3: all status 1 and to their recorded vault; each hash, block and gas matches its run's README",
            "each verifier output recomputes two deposit commitments and holds leaf 0 public / leaf 1 private at 0.001 ETH; 14 of 14 checks in each run",
            "LIMIT: F.1's local 6/18-decimal and USDT-style Solidity cases have no committed per-case output here; these are archived receipt cross-checks, not fresh Sepolia reads"]


@check(id="bridge-u10-catalog-install", title="U.10: 74-step signed catalog install and Basecamp's two installed packages",
       family="record", mode="offline", tier="quick", rows=("RFP-022 U.10",), evidence=(253, 254, 261, 262, 263),
       what="all 74 Logos Core steps and their exits, four signature/tamper refusals, seven packages against the release manifest, 212 read-back verdicts, and Basecamp's installed roots and module probe",
       against=f"{CATALOG}'s steps/read-back/Basecamp records, {INTEGRITY} and RFP-022 U.10",
       inputs=(STATUS, INTEGRITY, f"{CATALOG}/README.md", f"{CATALOG}/steps.tsv", f"{CATALOG}/read-back-checks.txt",
               f"{CATALOG}/basecamp-0.4.0-rc.2/actions.tsv", f"{CATALOG}/basecamp-0.4.0-rc.2/installed.tsv",
               f"{CATALOG}/basecamp-0.4.0-rc.2/basecamp.log", f"{CATALOG}/basecamp/probe.log"))
def bridge_u10_catalog_install(ctx):
    problems = []
    packages = _release_packages(ctx)
    # The record is headerless: name, expected, exit, milliseconds, as-expected, output.
    steps = [dict(zip(("name", "expected", "exit", "ms", "as_expected", "output"), row))
             for row in csv.reader(io.StringIO(_text(ctx, f"{CATALOG}/steps.tsv")), delimiter="\t")]
    by_name = {s["name"]: (i, s) for i, s in enumerate(steps, 1)}
    _expect(problems, len(steps) == 74 and len(by_name) == 74, "U.10: duplicate or malformed step names")
    for i, step in enumerate(steps, 1):
        expected_refusal = step["expected"].startswith("refused:")
        _expect(problems, int(step["exit"]) == (1 if expected_refusal else 0),
                f"U.10: step {i} {step['name']}: exit {step['exit']} does not match {step['expected']!r}")
        _expect(problems, step["as_expected"] == "1" and step["ms"].isdigit(),
                f"U.10: step {i} {step['name']}: verdict/timing cell changed")
    refusals = {
        "refuse-catalog-before-key": "refused: Package signed by untrusted key",
        "refuse-unsigned": "refused: Package is unsigned",
        "refuse-other-signer": "refused: Package signed by untrusted key",
        "refuse-tampered": "refused: Content hash mismatch",
    }
    for name, expected in refusals.items():
        _expect(problems, name in by_name and by_name[name][1]["expected"] == expected,
                f"U.10: refusal {name} changed")
    _expect(problems, "nothing-installed-after-refusals" in by_name, "U.10: no post-refusal empty-install check")
    _expect(problems, '"signature_policy":"require"' in by_name.get("config-show", (0, {"output": ""}))[1]["output"],
            "U.10: signature_policy require not recorded")
    for module in MODULES:
        version = "0.1.1" if module == "ferrata_bridge_module" else "0.1.0"
        package = packages.get((module, version))
        _expect(problems, package is not None, f"U.10: integrity manifest lacks {module} {version}")
        for prefix in ("download", "lgx-verify", "install", "roothash"):
            _expect(problems, f"{prefix}-{module}" in by_name, f"U.10: missing {prefix}-{module}")
        if package:
            download = by_name.get(f"download-{module}", (0, {"output": ""}))[1]["output"]
            _expect(problems, download.startswith(package["sha256"] + "  "), f"U.10: {module} .lgx SHA-256 differs")
            roots = re.findall(r"[0-9a-f]{64}", by_name.get(f"roothash-{module}", (0, {"output": ""}))[1]["output"])
            _expect(problems, len(roots) >= 2 and set(roots) == {package["root"]},
                    f"U.10: {module} installed/sidecar roots differ")
            verified = by_name.get(f"lgx-verify-{module}", (0, {"output": ""}))[1]["output"]
            _expect(problems, "Signature is valid" in verified and "Signer is trusted" in verified,
                    f"U.10: {module} lgx verdict changed")
    _expect(problems, "installed-as-ui-ferrata_bridge_ui" in by_name, "U.10: mini-app is not recorded as a ui package")
    for module in MODULES[:-1]:
        _expect(problems, f"installed-as-core-{module}" in by_name and f"load-{module}" in by_name
                and f"version-{module}" in by_name, f"U.10: {module} not installed/loaded/called as core")
    readback = _text(ctx, f"{CATALOG}/read-back-checks.txt")
    _expect(problems, sum(line.startswith("ok") for line in readback.splitlines()) == 212
            and sum(line.startswith("unknown") for line in readback.splitlines()) == 1
            and "passed 212, failed 0, not readable here 1" in readback, "U.10: 212/0/1 read-back totals changed")
    _phrases(problems, readback, "U.10 read-back", "index: 7 packages", "index: 8 versions",
             "ferrata_bridge_module: metadata.json version (0.1.1)", "ferrata_bridge_ui: manifest type ui_qml")
    installed = _tsv(ctx, f"{CATALOG}/basecamp-0.4.0-rc.2/installed.tsv")
    _expect(problems, len(installed) == 2 and {r["name"] for r in installed} == {"ferrata_bridge_module", "ferrata_bridge_ui"},
            "U.10: Basecamp's two installed packages changed")
    for record in installed:
        version = "0.1.1" if record["name"] == "ferrata_bridge_module" else "0.1.0"
        package = packages.get((record["name"], version))
        _expect(problems, record["version"] == version and record["hashes.root"] == (package or {}).get("root")
                and record["type"] == ("core" if record["name"] == "ferrata_bridge_module" else "ui_qml"),
                f"U.10: Basecamp's {record['name']} row differs from the release")
    shots = re.findall(r"\tshot (\d+)-", _text(ctx, f"{CATALOG}/basecamp-0.4.0-rc.2/actions.tsv"))
    _expect(problems, shots == [f"{i:02d}" for i in range(1, 22)], f"U.10: Basecamp screens {shots}")
    basecamp_log = _text(ctx, f"{CATALOG}/basecamp-0.4.0-rc.2/basecamp.log")
    _phrases(problems, basecamp_log, "U.10 Basecamp log", "Downloaded \"ferrata_bridge_module\"", "Module loaded: ferrata_bridge_module",
             "Successfully loaded UI module: \"ferrata_bridge_ui\"")
    probe = _text(ctx, f"{CATALOG}/basecamp/probe.log")
    _phrases(problems, probe, "U.10 Basecamp probe", "the view opens in Basecamp", "the view calls ferrata_bridge_module",
             "3 passed, 0 failed")
    readme = _text(ctx, f"{CATALOG}/README.md")
    _phrases(problems, readme, "U.10 record README", "Cancel: every package still \"Install\", nothing installed",
             "Trust & install", "the view asks the module for its `info`", "the module's answer")
    _phrases(problems, _rows(ctx, "U.10"), "U.10", "seven packages, eight versions", "installed all seven", "0.1.1",
             "unsigned, other-signed and tampered copies", "Cancel installed nothing", "Trust & install", "Linux x86-64",
             "monitor module's package has no `darwin-arm64`", "updater module's is `linux-amd64` alone")
    fail_if(problems, "U.10 catalog-install record/status mismatch")
    return ["74/74 Logos Core steps as expected; four refusals before/after key trust; seven packages downloaded, signature-valid, installed, root-matched; six core modules loaded and version-called",
            "read-back: 212 ok, 0 failed and the one documented missing-macOS unknown; index declares seven packages/eight versions; Basecamp installed bridge 0.1.1 core + UI 0.1.0 ui_qml at the same roots over 21 recorded screens",
            "Basecamp portable probe: 3 passed, 0 failed, including loading the core module and the view's call",
            "LIMIT: package payloads and screenshots are not decoded here, and the Basecamp trust prompt/Cancel screens are checked as the run's narrative/log, not by OCR; publication and variant assertions are archived read-backs, not fresh network reads"]


@check(id="bridge-s3-release-action", title="S.3: pinned-action releases and per-leg workflow outcomes",
       family="record", mode="offline", tier="quick", rows=("RFP-022 S.3",), evidence=(253, 262),
       what="six original signed immutable releases and their 18 build legs, the two update releases and all six build/two release legs, action pin, signer DID, variants and seven/eight index counts",
       against=f"both catalog read-back transcripts, {INTEGRITY} and RFP-022 S.3",
       inputs=(STATUS, INTEGRITY, f"{FIRST_CATALOG}/read-back-checks.txt", f"{FIRST_CATALOG}/integrity-commands.log",
               f"{CATALOG}/read-back-checks.txt", f"{CATALOG}/integrity-commands.log"))
def bridge_s3_release_action(ctx):
    problems = []
    old_log = _text(ctx, f"{FIRST_CATALOG}/integrity-commands.log")
    old_sidecars = {}
    for line in old_log.splitlines():
        try:
            value = json.loads(line)
        except (ValueError, json.JSONDecodeError):
            continue
        if isinstance(value, dict) and {"rootHash", "sha256", "builtVariants", "signerDid"} <= set(value):
            old_sidecars[value["sha256"]] = value
    _expect(problems, len(old_sidecars) == 6, f"S.3: {len(old_sidecars)} original sidecars, not six")
    variants = sorted(tuple(v["builtVariants"]) for v in old_sidecars.values())
    _expect(problems, variants.count(("darwin-arm64", "linux-amd64", "linux-arm64")) == 4
            and ("linux-amd64", "linux-arm64") in variants and ("linux-amd64",) in variants,
            f"S.3: original variant set changed: {variants}")
    signer_dids = {v["signerDid"] for v in old_sidecars.values()}
    _expect(problems, len(signer_dids) == 1 and next(iter(signer_dids), "").startswith("did:jwk:"),
            "S.3: original releases do not have one DID signer")
    jobs = re.findall(r"^(success|failure|cancelled)\trelease \(submodules/(\w+)\) / release / build \(([\w-]+)\)\t", old_log, re.M)
    _expect(problems, len(jobs) == 18 and len({(m, v) for _, m, v in jobs}) == 18,
            f"S.3: original build matrix is not 6x3 distinct legs ({len(jobs)})")
    per_module = {}
    for verdict, module, variant in jobs:
        per_module.setdefault(module, {})[variant] = verdict
    expected_exceptions = {
        "ferrata_monitor_module": {"darwin-arm64": "cancelled", "linux-amd64": "success", "linux-arm64": "success"},
        "ferrata_updater_module": {"darwin-arm64": "failure", "linux-amd64": "success", "linux-arm64": "failure"},
    }
    for module, legs in per_module.items():
        expected = expected_exceptions.get(module, {v: "success" for v in ("darwin-arm64", "linux-amd64", "linux-arm64")})
        _expect(problems, legs == expected, f"S.3: {module} build outcomes changed: {legs}")
    first_readback = _text(ctx, f"{FIRST_CATALOG}/read-back-checks.txt")
    _expect(problems, sum(line.startswith("ok") for line in first_readback.splitlines()) == 123
            and "passed 123, failed 0, not readable here 1" in first_readback, "S.3: original 123/0/1 read-back changed")
    _expect(problems, len(re.findall(r"^ok\s+ferrata_\w+-v0\.1\.0: published", first_readback, re.M)) == 6
            and len(re.findall(r"^ok\s+ferrata_\w+-v0\.1\.0: Immutable", first_readback, re.M)) == 6
            and len(re.findall(r"^ok\s+ferrata_\w+-v0\.1\.0: signed by the catalog's DID", first_readback, re.M)) == 6,
            "S.3: original published/immutable/signed per-release rows changed")
    new_log = _text(ctx, f"{CATALOG}/integrity-commands.log")
    for run, module in (("45543214624", "ferrata_bridge_module"), ("45543219299", "ferrata_bridge_ui")):
        headline = re.search(rf"^Release {module} ([0-9a-f]{{40}}) workflow_dispatch (\w+) ", new_log, re.M)
        _expect(problems, headline is not None and headline.group(2) == "success", f"S.3: run {run} conclusion changed")
        job_block = re.search(rf"gh run view {run} .*?\n(setup: .*?)\n", new_log)
        _expect(problems, job_block is not None, f"S.3: run {run} job list missing")
        if job_block:
            outcomes = {name.strip(): verdict for name, verdict in re.findall(r"([^,]+): (\w+)(?:,|$)", job_block.group(1))}
            for variant in ("darwin-arm64", "linux-amd64", "linux-arm64"):
                _expect(problems, outcomes.get(f"build ({variant})") == "success",
                        f"S.3: run {run} build {variant} did not succeed: {outcomes}")
            _expect(problems, outcomes.get("release") == "success", f"S.3: run {run} release job did not succeed")
        _phrases(problems, new_log, "S.3 update transcript", run)
    _phrases(problems, new_log, "S.3 action pin",
             "logos-modules-release-action/.github/workflows/release.yml@bbdc52871dc4305c8007923e1b08738ad2dd6b50")
    new_sidecars = {}
    for line in new_log.splitlines():
        try:
            value = json.loads(line)
        except (ValueError, json.JSONDecodeError):
            continue
        if isinstance(value, dict) and {"rootHash", "sha256", "size", "builtVariants", "type", "signerDid"} <= set(value):
            new_sidecars[value["sha256"]] = value
    _expect(problems, len(new_sidecars) == 2 and {v["type"] for v in new_sidecars.values()} == {"core", "ui_qml"}
            and all(v["builtVariants"] == ["darwin-arm64", "linux-amd64", "linux-arm64"] for v in new_sidecars.values())
            and {v["signerDid"] for v in new_sidecars.values()} == signer_dids,
            "S.3: update sidecars/types/variants/signer changed")
    readback = _text(ctx, f"{CATALOG}/read-back-checks.txt")
    _expect(problems, sum(line.startswith("ok") for line in readback.splitlines()) == 212
            and "passed 212, failed 0, not readable here 1" in readback, "S.3: update 212/0/1 read-back changed")
    _phrases(problems, readback, "S.3 update read-back", "index: 7 packages", "index: 8 versions")
    _expect(problems, len(re.findall(r"^ok\s+index lists ferrata_\w+-v[0-9.]+$", readback, re.M)) == 8
            and len(re.findall(r"^ok\s+ferrata_\w+-v[0-9.]+: Immutable$", readback, re.M)) == 8
            and len(re.findall(r"^ok\s+ferrata_\w+-v[0-9.]+: signed by the catalog's DID$", readback, re.M)) == 8,
            "S.3: updated index has not eight listed immutable signed versions")
    _phrases(problems, _rows(ctx, "S.3"), "S.3", "six core modules on 24 November", "ferrata_bridge_module` 0.1.1",
             "ferrata_bridge_ui` 0.1.0", "45543214624 and 45543219299", "every build leg",
             "signed by the catalog's key", "in the catalog's index")
    fail_if(problems, "S.3 release-action record/status mismatch")
    return ["original: six signed immutable packages; 18 distinct build legs = 15 success plus the recorded monitor cancel and two updater link failures, matching the documented partial variants",
            "update: runs 45543214624/45543219299 both success; each three build legs + release success; action pinned at bbdc5287; two three-variant packages signed by the original DID",
            "updated read-back: 212 ok / 0 failed / 1 known unknown; eight listed versions in seven packages, all immutable and DID-signed",
            "LIMIT: these are the recorded gh/git read-backs, not fresh GitHub API reads; package signature cryptography is checked separately by the install record, and the original run's aggregate failure is not misreported as success"]


@check(id="bridge-r13-cli-operator", title="R.13: the recorded CLI, monitor and relayer journeys",
       family="record", mode="offline", tier="quick", rows=("RFP-022 R.13",), evidence=(43, 46, 96, 102, 106, 107),
       what="the CLI's 18 commands and 97+15 transaction ledgers, the monitor's 46 case verdicts and timing table, and the relayer's checkpoint/release/refusal/clean-stop cases",
       against="e2e/runs/seqnet-cli-1, monitor/results/anvil-e2e/summary.md, e2e/runs/relayer-fork and RFP-022 R.13",
       inputs=(STATUS, f"{CLI_RUN}/summary.json", f"{CLI_RUN}/user/INDEX", f"{CLI_RUN}/lez/transactions.tsv",
               f"{CLI_RUN}/eth/transactions.tsv", f"{CLI_RUN}/SUMMARY.md", "monitor/results/anvil-e2e/summary.md",
               "e2e/runs/relayer-fork/results.json", "e2e/runs/relayer-fork/test.log"))
def bridge_r13_cli_operator(ctx):
    problems = []
    summary = _json(ctx, f"{CLI_RUN}/summary.json")
    lez = _tsv(ctx, f"{CLI_RUN}/lez/transactions.tsv")
    eth = _tsv(ctx, f"{CLI_RUN}/eth/transactions.tsv")
    _expect(problems, len(lez) == summary["lez_transactions"] == 97 and len(eth) == summary["eth_transactions"] == 15,
            f"R.13: CLI summary/ledger counts differ ({len(lez)}/{len(eth)})")
    _expect(problems, all(r["reverted"] == "false" for r in lez)
            and all(r["status"] == "ok" for r in eth) and summary["lez_reverted"] == summary["eth_reverted"] == 0,
            "R.13: a CLI-run transaction verdict changed")
    _expect(problems, any(r["kind"] == "private" and "Read,Issue" in r["visible_bridge_calls"] for r in lez)
            and any(r["kind"] == "private" and "Burn" in r["visible_bridge_calls"] for r in lez),
            "R.13: the private claim/burn applications are absent")
    calls = [r["call"] for r in eth]
    _expect(problems, all(name in calls for name in ("depositETH", "submitCandidate", "finalizeAttested", "release", "claimOwed")),
            "R.13: the user's deposit/checkpoint/release/fee operations changed")
    index = _text(ctx, f"{CLI_RUN}/user/INDEX")
    commands = re.findall(r"^([^\t]+)\texit (\d+)\t", index, re.M)
    _expect(problems, len(commands) == summary["user_commands"] == 18, f"R.13: {len(commands)} user commands, not 18")
    _expect(problems, [name for name, code in commands if code == "1"] == ["08-claim-held", "15-release-held"]
            and all(code == "0" for name, code in commands if name not in ("08-claim-held", "15-release-held")),
            "R.13: expected command successes/two intentional holds changed")
    cli_doc = _text(ctx, f"{CLI_RUN}/SUMMARY.md")
    _phrases(problems, cli_doc, "R.13 CLI record", "applied: block 28, private transaction", "applied: block 29, private transaction",
             "the fresh address", "997000000000000 wei", "driver exit 0")
    monitor = _text(ctx, "monitor/results/anvil-e2e/summary.md")
    _expect(problems, len(re.findall(r"^- PASS ", monitor, re.M)) == 46 and "Failures: 0" in monitor,
            "R.13: monitor is not 46 PASS / 0 failures")
    cancellations = re.findall(r"^\| (\w+) \| (\w+) \| (\d+) \| (\d+) \| (\d+) \|$", monitor, re.M)
    _expect(problems, len(cancellations) == 10, f"R.13: {len(cancellations)} monitor timing cases")
    for scenario, _reason, after, before, _gas in cancellations:
        _expect(problems, int(after) + int(before) == {"A2": 11760, "A3": 12660}.get(scenario, 3600),
                f"R.13: monitor {scenario} deadline arithmetic changed")
    _phrases(problems, monitor, "R.13 monitor record", "all 23 critical alerts reached the pager command",
             "SIGTERM: exit 0", "a second monitor on the same state file is refused")
    relayer = _json(ctx, "e2e/runs/relayer-fork/results.json")
    _expect(problems, relayer["candidate"]["burnCount"] == 2 and relayer["candidate"]["gasUsed"] == 375281
            and relayer["finalized"]["log"]["gasUsed"] == 218385 and relayer["finalized"]["wait"] == 3600,
            "R.13: relayer candidate/finalisation facts changed")
    _expect(problems, "BurnRootNotAccepted" in relayer["earlyRelease"] and relayer["cleanStop"]["reason"] == "SIGTERM",
            "R.13: relayer early-release/clean-stop facts changed")
    paid = relayer["paidRelease"]
    _expect(problems, paid["result"]["wrapped"] is True and paid["result"]["gasUsed"] == 407052
            and int(paid["gained"]) == int(paid["relayFee"]) == int(paid["result"]["paid"]) == 2_000_000_000_000
            and paid["redirectedRefused"] is True, "R.13: paid/wrapped release arithmetic or refusal changed")
    _expect(problems, len(relayer["releases"]["done"]) == 1 and relayer["releases"]["done"][0]["gasUsed"] == 358908
            and int(relayer["releases"]["feesCollected"]["amount"]) == 2_000_000_000_000
            and relayer["status"]["owedWei"] == "0", "R.13: second release/fee collection changed")
    _expect(problems, len(relayer["tamperedReleases"]) == 3 and all(r["refused"] for r in relayer["tamperedReleases"])
            and relayer["forgedCheckpoint"]["refused"] is True and "already released" in relayer["replay"]["daemon"],
            "R.13: relayer forgery/tamper/replay refusals changed")
    testlog = _text(ctx, "e2e/runs/relayer-fork/test.log")
    _phrases(problems, testlog, "R.13 relayer test log", "test relayer_on_a_sepolia_fork_with_crown_live_1_proofs ... ok",
             "test result: ok. 1 passed; 0 failed", "clean stop:")
    _phrases(problems, _rows(ctx, "R.13"), "R.13", "user's journey command by command", "operator guide §0, §4, §6, §7",
             "each component run")
    fail_if(problems, "R.13 CLI/operator record/status mismatch")
    return ["CLI: 18 commands (16 success + intentional claim/release holds), 97 unreverted LEZ + 15 ok Ethereum ledger rows, private claim/burn, checkpoint, paid release and fee claim reconciled",
            "monitor: 46 PASS / 0 failures, ten cancellation timing rows satisfy after+before=deadline; 23/23 critical pager alerts, lock/SIGTERM cases retained",
            "relayer: candidate 375,281 -> finalise 218,385 after 3,600 s; wrapped paid release 407,052 + second release 358,908; 2e12 fee collected; four forgery/tamper classes and replay refused; SIGTERM",
            "LIMIT: these records do not independently exercise every CLI option/refusal or the updater README's journey; they are archived local/fork runs, not toolchain reruns, and the monitor-module 46/46 and relayer-module 11/11 sister runs are not replayed"]


@check(id="bridge-soft3-release-delay", title="Soft.3: early CLI release refused at its derived opening time, then sent",
       family="record", mode="offline", tier="quick", rows=("RFP-022 Soft.3",), evidence=(85, 102),
       what="the held release's exit/code/opening time derived from finalize time + 120 seconds, the following command, and the successful on-chain release",
       against="e2e/runs/seqnet-cli-1's user commands, release messages and Ethereum transaction ledger; RFP-022 Soft.3",
       inputs=(STATUS, f"{CLI_RUN}/user/INDEX", f"{CLI_RUN}/user/15-release-held.txt",
               f"{CLI_RUN}/user/16-release.txt", f"{CLI_RUN}/eth/transactions.tsv", f"{CLI_RUN}/run.log"))
def bridge_soft3_release_delay(ctx):
    problems = []
    held = _text(ctx, f"{CLI_RUN}/user/15-release-held.txt")
    opening = re.search(r"ReleaseDelayNotElapsed\(\).*?first opens at Ethereum time (\d+), in (\d+) s", held)
    _expect(problems, opening is not None, "Soft.3: early refusal no longer names its opening and remaining seconds")
    transactions = _tsv(ctx, f"{CLI_RUN}/eth/transactions.tsv")
    finalized = [r for r in transactions if r["call"] == "finalizeAttested"]
    released = [r for r in transactions if r["call"] == "release"]
    _expect(problems, len(finalized) == len(released) == 1, "Soft.3: expected one finalisation and one release")
    if opening and len(finalized) == len(released) == 1:
        delay = 120
        derived_open = int(finalized[0]["time"]) + delay
        _expect(problems, int(opening.group(1)) == derived_open,
                f"Soft.3: refusal opens at {opening.group(1)}, finalise {finalized[0]['time']} + {delay} = {derived_open}")
        _expect(problems, int(opening.group(2)) == delay + 1,
                f"Soft.3: early call is not one second before the {delay}-second boundary")
        _expect(problems, released[0]["status"] == "ok" and int(released[0]["time"]) >= derived_open,
                f"Soft.3: release at {released[0]['time']} is not ok or is before its {derived_open} opening")
    index = _text(ctx, f"{CLI_RUN}/user/INDEX")
    _phrases(problems, index, "Soft.3 command ledger", "15-release-held\texit 1\t", "16-release\texit 0\t")
    sent = _text(ctx, f"{CLI_RUN}/user/16-release.txt")
    _phrases(problems, sent, "Soft.3 later command", "release delay (120 s) has passed", "relayer job written")
    run_log = _text(ctx, f"{CLI_RUN}/run.log")
    clock = re.search(r"anvil's clock moved on 121 s: the release delay \(120 s after finalizeAttested\) \(Ethereum time now (\d+)\)", run_log)
    _expect(problems, clock is not None and opening is not None and int(clock.group(1)) == int(opening.group(1)) + 1,
            "Soft.3: run log's 121-second clock move/end time differs from the refusal's opening")
    _phrases(problems, _rows(ctx, "Soft.3"), "Soft.3", "Per-token release delay", "the tool's early release is refused, then sent")
    fail_if(problems, "Soft.3 release-delay record/status mismatch")
    return ["15-release-held exits 1 with ReleaseDelayNotElapsed and first-opens time = finalizeAttested time + 120 s (one second before the boundary in the CLI's rounding)",
            "16-release exits 0 after the delay; the actual release receipt is ok at/after that opening; run.log records the 121-second clock move",
            "LIMIT: the per-token minimum/bounds/raising-and-lowering matrix is vault/test/ReleaseDelay.t.sol, with no committed per-case output; this check covers only the cited CLI early-refusal-then-sent record"]


@check(id="bridge-r14-deployer-walkthrough", title="R.14: 20 image-id comparisons and 29 read-back settings from the fresh-clone run",
       family="record", mode="offline", tier="quick", rows=("RFP-022 R.14",), evidence=(182,),
       what="20 MATCH rows, 29 PASS settings with role/cap/configId/delta cross-checks, both step exits, fresh-clone marker and actual 83+18 transaction ledgers",
       against=f"{DEPLOYER}/second-run, {DEPLOYER_RUN} and RFP-022 R.14",
       inputs=(STATUS, f"{DEPLOYER}/README.md", f"{DEPLOYER}/second-run/clone.txt", f"{DEPLOYER}/second-run/exits.txt",
               f"{DEPLOYER}/second-run/image-ids-compare.txt", f"{DEPLOYER_RUN}/checks.txt",
               f"{DEPLOYER_RUN}/summary.json", f"{DEPLOYER_RUN}/lez/transactions.tsv", f"{DEPLOYER_RUN}/eth/transactions.tsv",
               f"{DEPLOYER_RUN}/identity.json", f"{DEPLOYER_RUN}/lez/config.json"))
def bridge_r14_deployer_walkthrough(ctx):
    problems = []
    comparisons = _text(ctx, f"{DEPLOYER}/second-run/image-ids-compare.txt")
    matches = re.findall(r"^MATCH\s+(\S+)\s+rebuilt (0x[0-9a-f]{64})\s+(README|docs/image-ids\.md) = rebuilt", comparisons, re.M)
    _expect(problems, len(matches) == 20 and len([ln for ln in comparisons.splitlines() if ln.strip()]) == 20,
            f"R.14: {len(matches)} image comparisons, not 20")
    _expect(problems, not re.search(r"^(?!MATCH|$).+", comparisons, re.M), "R.14: an image comparison verdict changed")
    _expect(problems, sum(source == "README" for _, _, source in matches) == 6
            and sum(source == "docs/image-ids.md" for _, _, source in matches) == 14,
            "R.14: six README + fourteen image-ids.md comparisons changed")
    by_image = {}
    for name, image, _source in matches:
        by_image.setdefault(name, set()).add(image)
    _expect(problems, all(len(images) == 1 for images in by_image.values()), "R.14: a repeated program's rebuilt ids differ")
    checks = _text(ctx, f"{DEPLOYER_RUN}/checks.txt")
    settings = re.findall(r"^PASS (.+)$", checks, re.M)
    _expect(problems, len(settings) == 29 and "FAIL" not in checks, f"R.14: {len(settings)} settings PASS, not 29")
    for expected in ("vault admin (handed over)", "vault freeze authority", "ETH fee schedule in force",
                     "second token: denominations", "vault finality depth", "LEZ global cap read from the zone",
                     "LEZ configuration initialised under this configId"):
        _expect(problems, any(s.startswith(expected) for s in settings), f"R.14: setting {expected!r} absent")
    identity = _json(ctx, f"{DEPLOYER_RUN}/identity.json")
    config = _json(ctx, f"{DEPLOYER_RUN}/lez/config.json")
    _phrases(problems, checks, "R.14 settings cross-check", f"delta (the candidate's wait, s): {identity['delta']}",
             identity["configId"], config["controls"]["admin"], config["controls"]["freezeAuthority"],
             f"LEZ token cap in configId (value units): {config['controls']['tokenCap']['capacity']}",
             f"LEZ global cap in configId (value units): {config['controls']['globalCap']['capacity']}",
             f"the gate's mint and burn buckets): {config['controls']['globalCap']['capacity']} {config['controls']['globalCap']['capacity']}")
    _expect(problems, identity["configId"] == config["configId"], "R.14: identity/LEZ configId differ")
    summary = _json(ctx, f"{DEPLOYER_RUN}/summary.json")
    lez = _tsv(ctx, f"{DEPLOYER_RUN}/lez/transactions.tsv")
    eth = _tsv(ctx, f"{DEPLOYER_RUN}/eth/transactions.tsv")
    _expect(problems, len(lez) == summary["lez_transactions"] == 83 and len(eth) == summary["eth_transactions"] == 18,
            "R.14: 83/18 summary counts differ from actual ledgers")
    _expect(problems, all(r["reverted"] == "false" for r in lez) and all(r["status"] == "ok" for r in eth)
            and summary["lez_reverted"] == summary["eth_reverted"] == 0 and summary["user_commands"] == 7,
            "R.14: a transaction/user-command verdict changed")
    _phrases(problems, _text(ctx, f"{DEPLOYER}/second-run/clone.txt"), "R.14 clone record", "changed files: 0", "<the clone>")
    _phrases(problems, _text(ctx, f"{DEPLOYER}/second-run/exits.txt"), "R.14 step exits",
             "step0: exit 0 after 3696 s", "step12: exit 0 after 699 s")
    _phrases(problems, _text(ctx, f"{DEPLOYER}/README.md"), "R.14 run README", "29 settings read back, 29 PASS, 0 FAIL",
             "nothing was placed beside the clone", "20 of 20 MATCH", "neither was a stranger following them")
    _phrases(problems, _rows(ctx, "R.14"), "R.14", "fresh clone with nothing beside it", "20 of 20", "29 of 29 settings PASS",
             "outside-deployer gate (B4) is still open", "bridge program `0xccde6171…`")
    fail_if(problems, "R.14 deployer record/status mismatch")
    return [f"20/20 MATCH rows: six against the run README and fourteen against docs/image-ids.md; repeated program ids agree, with {len(by_image)} distinct image ids",
            "29/29 settings PASS; roles, token/global caps, configId and finality depth cross-checked against identity.json + lez/config.json; fresh-clone and step0/step12 exit records retained",
            "83 actual unreverted LEZ + 18 actual ok Ethereum transactions equal summary.json (zero reverted, seven user commands)",
            "LIMIT: this does not rebuild any image or re-query the old local chains; most vault read-back expected values are the recorded checker assertions, and the run was by the team, so the outside-deployer B4 gate remains open"]


