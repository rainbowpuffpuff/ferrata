"""Every check of scripts/verify.py, the coverage of the requirement rows, and the context the checks run in.

The repository's own re-check scripts are wrapped here as they are (each was already the re-check command of an
EVIDENCE row); the families written for verify.py live in the checks_*.py modules; the named tests come from
scripts/ci/row-tests.toml (tests_map.py). A check names the requirement rows it backs only where the row's own
Evidence cell cites one of its EVIDENCE rows or files (scripts/test_verify.py holds every check to that).
"""
from __future__ import annotations

import importlib
import json
from pathlib import Path

from . import tests_map
from .core import REGISTRY, Ctx, command
from .rows import status_rows

CHECK_MODULES = ("checks_records", "checks_hashes", "checks_seals", "checks_sigs", "checks_lc", "checks_online",
                 "runrecord")


def _py(id, script, *args, **kw):
    """A repository script run with `python3 -I`; passes when it exits 0."""
    return command(id=id, argv=("python3", "-I", script) + tuple(args), inputs=(script,) + tuple(kw.pop("inputs", ())),
                   **kw)


def _register_wrappers() -> None:
    # ---- records: numbers re-derived from the committed records ---------------------------------------------------
    _py("lab-roots", "lez/lab/roots.py", "lez/lab/results", family="record", mode="offline", evidence=(21,),
        rows=("RFP-021 F.14", "RFP-021 F.15", "RFP-021 S.1"),
        title="The lab's sessions: each applied finality update's finalized root equals the second provider's",
        what="every lab session's applied finality update (its kept SSZ), the finalized header hashed here",
        against="the block root the session's second beacon provider reported for that slot (sessions/*/fetch-*.json)",
        inputs=("lez/lab/results",))
    _py("day1-rehearsal", "lez/testnet04/recheck.py", "lez/testnet04/results/rehearsal-1", family="record",
        mode="offline", evidence=(148,), rows=("RFP-021 S.1", "RFP-022 S.1"),
        title="The day-one kit's rehearsal on our rc3 zone, re-checked from its records",
        what="each stage's transactions, blocks, execution and storage gas, the attestation of the vault's slots",
        against="the sequencer's log and eth_getProof's recorded answer in lez/testnet04/results/rehearsal-1",
        inputs=("lez/testnet04/results/rehearsal-1",))
    _py("m0-real-proofs", "lez/scripts/real-proofs-summary.py", "lez/results/v0.4.0-rc3/real-proofs",
        family="record", mode="offline", evidence=(123,),
        rows=("RFP-021 F.1", "RFP-021 F.8", "RFP-021 P.2", "RFP-022 F.4", "RFP-022 F.6"),
        title="M0 check 4: the private update, attestation, claim and burn proven for real and applied on rc3",
        what="the four private transactions' session records: applied, in a block final on Bedrock, without dev mode",
        against="the sequencer's own logs and blocks recorded in lez/results/v0.4.0-rc3/real-proofs",
        inputs=("lez/results/v0.4.0-rc3/real-proofs",))
    _py("m0-gas-drop", "lez/scripts/gas-drop-summary.py", "lez/results/v0.4.0-rc3/gas-drop", family="record",
        mode="offline", evidence=(124,), rows=("RFP-022 U.5", "RFP-022 R.9", "RFP-022 Priv.6"),
        title="M0 check 2: the gas drop's sealed credit swept by a private claim on rc3, real proofs",
        what="the relayer's credit, the claim that pays its fee from it and the refused claim without it",
        against="the session's recorded blocks and logs in lez/results/v0.4.0-rc3/gas-drop",
        inputs=("lez/results/v0.4.0-rc3/gas-drop",))
    _py("m0", "scripts/m0.py", family="record", mode="offline", evidence=(50, 123, 124),
        rows=("RFP-021 P.4", "RFP-022 P.4"),
        title="Every M0 check of the proposal with its measured value and GO / NO-GO, read from the result files",
        what="each M0 value as its result file records it (never typed in)",
        against="the thresholds the proposal sets for M0 (scripts/m0.py prints the NO-GO ones)")
    _py("perf-records", "scripts/test_perf_records.py", family="record", mode="offline", evidence=(227,),
        rows=("RFP-022 P.5", "RFP-022 P.6"),
        title="RFP-022 P.5 and P.6: every documented figure recomputed from the raw records",
        what="lez/results/perf-latency-bridge.md, perf-daemons.md, perf-gate-sequencer.md byte for byte and "
             "docs/performance.md §P.5/§P.6 figure by figure",
        against="crown-live-3's stages and driver log, the daemons' GNU time and samples, the GPU and laptop runs")
    for cid, gen, args, out, ev, rows, title in (
            ("perf-tracking", "bench/perf/tracking_cost.py", ("lez/lab/results/record",), "lez/results/perf-tracking.md",
             (167,), ("RFP-021 P.3",), "RFP-021 P.3: the tracking cost over the lab's eight days"),
            ("perf-latency-e2e", "bench/perf/latency_e2e.py", ("lez/updater/results/live-sepolia",),
             "lez/results/perf-latency-e2e.md", (169,), ("RFP-021 P.5",),
             "RFP-021 P.5: end-to-end attestation latency from the live run's records"),
            ("perf-precompile", "bench/perf/precompile_model.py", ("bench/perf/results",),
             "lez/results/perf-precompile.md", (171,), ("RFP-021 P.8",),
             "RFP-021 P.8: the BLS precompile delta modelled from the measured costs")):
        command(id=cid, shell=f"python3 -I {gen} {' '.join(args)} | cmp - {out}", family="record", mode="offline",
                evidence=ev, rows=rows, title=title, needs=("python3", "bash"),
                what=f"{out}, regenerated by {gen} from the raw records",
                against=f"the committed {out}, byte for byte", inputs=(gen, out) + args)
    _py("perf-proofs-summary", "bench/perf/test_summarize_proofs.py", family="record", mode="offline",
        evidence=(166,), rows=("RFP-021 P.2",),
        title="RFP-021 P.2: the proving records' summary and its image-id checks",
        what="bench/perf/summarize_proofs.py over its test cases and the committed proving runs",
        against="the image ids each record names and the figures lez/results/perf-proofs.md gives")
    _py("v03-stage2", "lez-v03-return/stage2.py", "check", family="record", mode="offline", evidence=(135,),
        title="How far a real checkpoint gets on the public LEZ testnet's own (v0.3.1) data",
        what="real LEZ v0.3.1 blocks and their L1 inscriptions, recorded from the node's RPC",
        against="the Mantle hashes recomputed here and the inscriptions' signers")
    # ---- docs: the documents' own consistency ----------------------------------------------------------------------
    _py("row-tests-map", "scripts/ci/check-row-tests.py", family="docs", mode="offline", evidence=(162, 207),
        rows=("RFP-022 R.11",),
        title="Every hard requirement row has a test, and CI really runs the ones it names",
        what="scripts/ci/row-tests.toml against docs/traceability.md's rows and the workflows' commands",
        against="the tracked files and the CI jobs that select each test (scripts/ci/ci_model.py)")
    _py("decisions", "scripts/check-decisions.py", family="docs", mode="offline", evidence=(181,),
        rows=("RFP-021 S.11", "RFP-022 S.4"),
        title="The decision log: every record's form and references",
        what="docs/decisions/*.md: context, options, decision, rationale, trade-offs, sources",
        against="the form docs/decisions/README.md sets; every cited path tracked")
    _py("guides", "scripts/check-guides.py", family="docs", mode="offline", evidence=(180,),
        rows=("RFP-021 S.4", "RFP-022 U.7", "RFP-022 Sec.7"),
        title="The guides' generated tables and cited files",
        what="the consumer, user, operator and deployer guides' generated parts",
        against="the IDL's codes and the tracked files they cite")
    command(id="links", argv=("python3", "-I", "scripts/check-links.py", "README.md", "docs", "cli/README.md",
                              "monitor/README.md", "lez/ATTEST-CLI.md", "lez/updater/README.md", "e2e/seqnet/README.md"),
            family="docs", mode="offline", title="Every relative link in the README and docs resolves",
            what="every relative link in README.md, docs/ and the READMEs CI names",
            against="the tracked files", inputs=("scripts/check-links.py",))
    _py("idl", "idl/check.py", family="docs", mode="offline", evidence=(55, 58),
        rows=("RFP-021 U.4", "RFP-022 U.4"),
        title="The IDLs in SPEL's format: decoded by SPEL's crate shapes, every recorded message read",
        what="idl/*.idl.json and the recorded messages they decode",
        against="lez/wire and lez/bridge-wire's types (the programs' own encodings)")
    # ---- unit: the repository's own tools under test (Python and Node, no toolchain) -----------------------------
    for cid, script, ev, rows, title in (
            ("unit-image-ids", "scripts/test_image_ids.py", (37, 82), (), "The image-id tools"),
            ("unit-image-id-tools", "scripts/reproduce/test_image_id_tools.py", (37, 82, 201), (),
             "The reproduction's image-id comparison"),
            ("unit-check-network", "scripts/reproduce/test_check_network.py", (238,), (),
             "The deployed-programs check against a network (its refusals)"),
            ("unit-verify-live-roundtrip", "scripts/test_verify_live_roundtrip.py", (250,), (),
             "The live round trip's checker against altered answers"),
            ("unit-site", "site/test_site.py", (), (), "The site's requirement pages and their evidence links"),
            ("unit-trust-pages", "deploy/testnet/web/trust/test_trust.py", (240,), (),
             "The test network's trust pages"),
            ("unit-live-testnet", "modules/scripts/test_live_testnet.py", (237,), (), "The modules' live-testnet driver"),
            ("unit-gateway", "deploy/testnet/tests/test_gateway.py", (249,), (), "The test network's gateway limits"),
            ("unit-gateway-gasdrop", "deploy/testnet/tests/test_gateway_gasdrop.py", (258,), ("RFP-022 U.5",),
             "The gateway's gas-drop inbox"),
            ("unit-prover-client", "deploy/testnet/tests/test_prover_client.py", (259,), (),
             "The checkpoint prover client under faults"),
            ("unit-ci-model", "scripts/ci/test_ci_model.py", (162,), (), "The CI model the row-to-test map relies on"),
            ("unit-scrub-records", "scripts/test_scrub_records.py", (218,), (), "The records' scrubber"),
            ("unit-release-manifest", "scripts/test_release_manifest.py", (201,), (), "The release manifest"),
            ("unit-release-scripts", "scripts/test_release_scripts.py", (201,), (), "The release scripts"),
            ("unit-v03-standin", "lez-v03-return/test_standin.py", (134,), (), "The v0.3.1 return leg's stand-in"),
    ):
        _py(cid, script, family="unit", mode="offline", evidence=ev, rows=rows, title=title,
            what=f"{script}'s cases", against="the behaviour the script's docstring states")
    for cid, shell, ev, rows, title in (
            ("unit-admin", "node admin/dump.mjs --selftest", (57,), ("RFP-022 U.2",),
             "The admin view's decoder (235 checks)"),
            ("unit-msg", "node --test deploy/testnet/web/msg/test/*.test.mjs", (236,), (),
             "The encrypted messages' library and CLI"),
            ("unit-names", "node --test names/js/test/names.test.mjs names/js/test/review.test.mjs", (235,), (),
             "The names resolver and CLI"),
            ("unit-wallet", "cd deploy/testnet/web/wallet && node --test test/*.test.mjs && node test/pwa-files.mjs --check",
             (243, 245), ("RFP-022 Soft.5",), "The web wallet's unit tests and its offline build's file list"),
    ):
        command(id=cid, shell=shell, family="unit", mode="offline", evidence=ev, rows=rows, title=title,
                needs=("node", "bash"), what=f"`{shell}`", against="the test files' expectations")


_LOADED = False


def load(root: Path) -> list:
    """Register every check once; return the requirement rows."""
    global _LOADED
    if not _LOADED:
        _register_wrappers()
        for name in CHECK_MODULES:
            try:
                importlib.import_module(f"verifylib.{name}")
            except ModuleNotFoundError as e:
                if e.name != f"verifylib.{name}":
                    raise
        tests_map.register_test_checks(root)
        _LOADED = True
    return status_rows(root)


def context(root: Path, args) -> Ctx:
    ep = json.loads((root / "scripts/pubnet-endpoints.json").read_text())
    net = json.loads((root / "scripts/reproduce/deployed-programs.json").read_text())["network"]
    return Ctx(root=root, sepolia_rpc=list(args.sepolia_rpc) + ep["sepolia"]["rpc"],
               mainnet_rpc=list(args.mainnet_rpc) + ep["mainnet"]["rpc"],
               beacon=list(args.beacon) + ep["sepolia"]["beacon"], lez_url=(args.lez or net).rstrip("/"),
               verbose=args.verbose, cache=root / ".scratch" / "verify-cache")


def coverage(root: Path, rows: list) -> dict:
    out = {}
    for r in rows:
        checks = [c for c in REGISTRY.values() if r.key in c.rows]
        out[r.key] = {
            "rfp": r.rfp, "id": r.id, "status": r.status, "soft": r.soft,
            "quick_offline": sorted(c.id for c in checks if c.mode == "offline" and c.tier == "quick"),
            "online": sorted(c.id for c in checks if c.mode == "online"),
            "tests": sorted(c.id for c in checks if c.tier == "tests"),
        }
    return out


def print_coverage(root: Path, rows: list, as_json: str | None = None) -> int:
    cov = coverage(root, rows)
    if as_json:
        Path(as_json).write_text(json.dumps(cov, indent=1))
    lines = []
    for rfp in ("RFP-021", "RFP-022"):
        for soft in (False, True):
            sel = [v for v in cov.values() if v["rfp"] == rfp and v["soft"] == soft]
            if not sel:
                continue
            lines.append(f"{rfp} {'soft' if soft else 'hard'} rows")
            lines.append(f"  {'status':16} {'rows':>4} {'any':>4} {'quick':>5} {'online':>6} {'tests':>5}")
            for st in ("shown", "in part", "not yet", "waits on Logos"):
                s = [v for v in sel if v["status"] == st]
                if not s:
                    continue
                anyc = sum(1 for v in s if v["quick_offline"] or v["online"] or v["tests"])
                q = sum(1 for v in s if v["quick_offline"])
                o = sum(1 for v in s if v["online"])
                t = sum(1 for v in s if v["tests"])
                lines.append(f"  {st:16} {len(s):>4} {anyc:>4} {q:>5} {o:>6} {t:>5}")
    lines.append("")
    lines.append("any: at least one check; quick: an offline check of the quick tier (Python or Node, no toolchain);")
    lines.append("online: a read-only check against public endpoints; tests: a named test run with its toolchain.")
    lines.append("")
    for key, v in cov.items():
        if v["status"] in ("shown", "in part"):
            lines.append(f"{key:14} {v['status']:8} quick: {', '.join(v['quick_offline']) or '-'}"
                         f" | online: {', '.join(v['online']) or '-'} | tests: {len(v['tests'])}")
    print("\n".join(lines))
    return 0
