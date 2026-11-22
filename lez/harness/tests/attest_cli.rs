//! The attestation CLI (`ferrata-attest`, `harness/src/bin/ferrata-attest.rs`, `ATTEST-CLI.md`)
//! as the tool a user runs, answering RFP-021 Usability #2: "Provide a CLI that covers producing
//! an attestation for a given (chain, contract, storage slot or log, predicate) tuple, verifying
//! one off-chain as a dry run, submitting a consensus-state update, and reading an instance's
//! configuration. The dry run must report the same typed error codes the on-chain path returns."
//!
//! The CLI runs offline on the recorded Sepolia fixtures, but every invocation follows Sepolia in
//! the harness's zone first (about 100 s), so each test is one invocation chaining its commands
//! with `--then` — three in all. No asserted fragment is a guess: each is printed by
//! `ferrata-attest.rs` where the comment beside it says, and the committed record of the same
//! commands is `lez/results/attest-cli/` (`README.md` gives the invocations; `run.txt`,
//! `advance.txt`, `config.txt` and `config-usage.txt` the output; `exit.txt` and `advance-exit.txt`
//! the exit statuses). No timing line is asserted.
//!
//! The exit status is the contract (`main` keeps the maximum of the commands' codes): 0 attested
//! or advanced (or would be), 3 refused as the dry run predicted, 1 the dry run and the execution
//! disagree, 2 a usage error. Where a command is refused the tests assert 3 — never 1 — and the
//! same typed code everywhere it appears, in the dry run's verdict, in the reversion the zone's
//! scratch settlement returns, and in the settled execution's exit: 130 `PredicateFailed` for a
//! predicate that does not hold, 52 `NotNewer` for a consensus-state update submitted twice (the
//! names and messages come from `idl/ferrata_attestation.idl.json`, which `code_names()` checks
//! against the wire's `code::ALL` when the binary starts).

use std::process::Command;

/// The attestation fixture's facts (`sepolia-attest-2026-11-10`, pinned by its `SHA256SUMS`), as
/// the CLI's default requests name them (`request`: `Op::Slot` and `Op::Account`): USDC and its
/// slot 11 (`totalSupply_`) and Multicall3. Recorded: `results/attest-cli/run.txt` prints them.
const USDC: &str = "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238";
const SLOT: &str = "0x000000000000000000000000000000000000000000000000000000000000000b";
const MULTICALL3: &str = "0xca11bde05977b3631167028862be2a173976ca11";

/// The instance the CLI follows Sepolia with, derived as `Stage5`'s replay creates it: `instance_id` over
/// `sepolia_config(2)` and the Sepolia checkpoint. Not copied from the record of 11 November, whose
/// configuration encoding has changed since (its `run.txt` names another id).
fn instance() -> String {
    let config = harness::sepolia::sepolia_config(2);
    let id = ferrata_lez_wire::instance_id(&config, &harness::root(harness::sepolia::SEPOLIA_CHECKPOINT));
    format!("0x{}", hex::encode(id))
}

/// What `slot --expect-eq 0x01` asks for (`word_arg` right-pads the byte): a value the supply
/// never was, so the condition the program checks (`Field::Value ==`) cannot hold.
const NOT_THE_SUPPLY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

/// What `main` prints on standard error before it follows Sepolia (`stderr.txt`), and what
/// `--follow-to 1408` prints instead (`advance-stderr.txt`).
const FOLLOW_FULL: &str = concat!(
    "following Sepolia in the harness's zone: Init, the updates of periods 1378-1409, ",
    "then the attestation fixture's finality update ..."
);
const FOLLOW_TO_1408: &str = "following Sepolia in the harness's zone: Init, the updates of periods 1378-1408 ...";

/// A command line's words (none of them holds a space).
fn words(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

/// The built `ferrata-attest`: cargo's `CARGO_BIN_EXE_ferrata-attest` where the compiler was given it (`cargo
/// check` gives it; `cargo test` 1.98.1 on the build box did not), else the binary beside this test's own executable
/// (`target/<profile>/`), where `cargo test -p harness` builds every binary of the package for its integration tests.
fn cli() -> std::path::PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_ferrata-attest") {
        return path.into();
    }
    let exe = std::env::current_exe().expect("the test's own path");
    let dir = exe.parent().and_then(|deps| deps.parent()).expect("the test runs from target/<profile>/deps");
    let bin = dir.join(format!("ferrata-attest{}", std::env::consts::EXE_SUFFIX));
    assert!(bin.is_file(), "{} is not built (cargo build -p harness --bin ferrata-attest)", bin.display());
    bin
}

/// One run of the built CLI, in the zkVM's dev mode (`RISC0_DEV_MODE=1`: the guests are executed,
/// not proven, as the harness's runs are) and the test's own environment. It writes nothing: no
/// `--message-out` is given.
fn attest(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(cli())
        .args(args)
        .env("RISC0_DEV_MODE", "1")
        .output()
        .unwrap_or_else(|e| panic!("starting ferrata-attest {args:?}: {e}"));
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// One command's output block: `main` prints each `--then` command's output and then a blank
/// line, so a block is delimited by blank lines and its first line is the command's header.
fn blocks(out: &str) -> Vec<&str> {
    out.split("\n\n").filter(|b| !b.trim().is_empty()).collect()
}

/// The `n`-th (from 0) command block whose header line is `header`: the same command appears
/// twice, "dry run: WHAT" then "send: WHAT" (printed by `run` and `advance`), and the advance
/// test runs the same command four times.
fn block<'a>(out: &'a str, header: &str, n: usize) -> &'a str {
    let found: Vec<&str> = blocks(out).into_iter().filter(|b| b.lines().next() == Some(header)).collect();
    assert!(found.len() > n, "only {} block(s) headed {header:?} in:\n{out}", found.len());
    found[n]
}

/// The one line of `block` that starts with `prefix` — `ferrata-attest.rs` prints every field
/// under a fixed label — and it must contain every fragment of `parts`.
fn field(block: &str, prefix: &str, parts: &[&str]) {
    let found: Vec<&str> = block.lines().filter(|l| l.starts_with(prefix)).collect();
    assert_eq!(found.len(), 1, "expected one line {prefix:?} in:\n{block}");
    for part in parts {
        assert!(found[0].contains(part), "no {part:?} in:\n{}", found[0]);
    }
}

/// `text` must contain `part`: for the standard-error lines, which have no labelled fields.
fn has(text: &str, part: &str) {
    assert!(text.contains(part), "no {part:?} in:\n{text}");
}

#[test]
fn attesting_a_slot_an_account_and_a_log_and_refusing_a_false_predicate() {
    // One invocation, the full follow (the default: the updates of periods 1378-1409 and then the
    // attestation fixture's finality update, so the attested block is finalized and the zone
    // holds its record — `run` refuses to attest without it), nine commands chained by `--then`:
    // every kind of attestation dry, then sent; the slot again with a condition its value does
    // not meet, dry, then sent; and the configuration. `run.txt` records the same commands (in
    // another order, which changes only the block numbers and base fees asserted nowhere) and
    // `exit.txt` this exit status.
    let (code, out, err) = attest(&words(concat!(
        "slot",
        " --then slot --send",
        " --then slot --expect-eq 0x01",
        " --then slot --expect-eq 0x01 --send",
        " --then account",
        " --then account --send",
        " --then log",
        " --then log --send",
        " --then config",
    )));
    // 3: the `--expect-eq` pair is refused as its dry runs predicted; 1 would be a dry run its
    // execution contradicted, 2 a usage error. The four sent commands all say their executions
    // equalled their dry runs ("run" prints "NO" only after comparing exit code, cycles, both
    // gases, the base fee, the byte size and the events).
    assert_eq!(code, Some(3), "the run's exit status\nstdout:\n{out}\nstderr:\n{err}");
    assert_eq!(out.matches("equal to the dry run: yes").count(), 4, "{out}");
    assert!(!out.contains("equal to the dry run: NO"), "{out}");
    has(&err, FOLLOW_FULL);

    // A storage slot, USDC's `totalSupply_`, dry: the message that would be sent, the record
    // actor `header(instance, slot)` it would go to — the only actor that holds the record it
    // attests under — the request's parts, and the verdict of `ferrata_lez_wire::attest::evaluate`,
    // the function the program's guest runs: two statements, the account itself and the slot's
    // value.
    let slot = format!("dry run: storage slot {SLOT} of {USDC}");
    let b = block(&out, &slot, 0);
    field(b, "  message", &["Attest (variant 7 of ferrata_lez_wire::Message)"]);
    field(b, "  to", &["header(instance, slot 11549664)"]);
    field(b, "  request", &["chain 11155111", &format!("instance {}", instance())]);
    field(
        b,
        "  verdict",
        &["attested: 2 statement(s) (ferrata_lez_wire::attest::evaluate, the program's own function)"],
    );
    field(
        b,
        "    [0] account",
        &[USDC, "nonce 1", "storage root 0x4d959de7c4a8ad2ee46885228fc3e38d4eb350772e70ab50e5b06538a2768550"],
    );
    field(
        b,
        "    [1] storage",
        &[USDC, &format!("slot {SLOT} = 0x0000000000000000000000000000000000000000000000009513ef76177584ee")],
    );
    field(b, "  events", &["FRT-ATT1"]);
    field(b, "  nothing was sent", &["(add --send to settle it in the harness's zone)"]);
    // ... and sent: applied, equal to the dry run, the verdict unchanged.
    let b = block(&out, &format!("send: storage slot {SLOT} of {USDC}"), 0);
    field(b, "  verdict", &["attested: 2 statement(s)"]);
    field(b, "  result", &["applied in block", "; exit None; 1 event(s); equal to the dry run: yes"]);

    // A predicate that does not hold (`--expect-eq 0x01`, the supply is not 1): the same typed
    // code, 130 `PredicateFailed`, in the dry run's verdict — `s.expected` runs the program's own
    // evaluate — and in the reversion LEZ's scratch settlement returns: the requirement's "the
    // dry run must report the same typed error codes the on-chain path returns". Then sent: the
    // zone's execution reverts with the same code, and the fee is charged.
    let refused = format!("dry run: storage slot {SLOT} of {USDC}, condition value == {NOT_THE_SUPPLY}");
    let b = block(&out, &refused, 0);
    field(
        b,
        "  verdict",
        &["refused with exit code 130 PredicateFailed (the core): A condition of the statement does not hold."],
    );
    field(
        b,
        "  outcome",
        &[
            "the transaction would revert with exit code 130 PredicateFailed",
            "(the core): A condition of the statement does not hold. (the fee is charged)",
        ],
    );
    let b = block(&out, &format!("send: storage slot {SLOT} of {USDC}, condition value == {NOT_THE_SUPPLY}"), 0);
    field(b, "  result", &["reverted in block", "; exit Some(130); 0 event(s); equal to the dry run: yes"]);

    // An account, Multicall3, dry then sent: one statement carrying its nonce, balance, storage
    // root and code hash (`print_attestation`'s `Fact::Account`), applied and equal.
    let b = block(&out, &format!("dry run: account {MULTICALL3}"), 0);
    field(b, "  verdict", &["attested: 1 statement(s)"]);
    field(
        b,
        "    [0] account",
        &[MULTICALL3, "nonce 1", "code hash 0xd5c15df687b16f2ff992fc8d767b4216323184a2bbc6ee2f9c398c318e770891"],
    );
    let b = block(&out, &format!("send: account {MULTICALL3}"), 0);
    field(b, "  result", &["applied in block", "; exit None; 1 event(s); equal to the dry run: yes"]);

    // A log, the block's first ERC-20 `Transfer` with its emitter and recipient checked (a
    // predicate that holds), dry then sent.
    let log = "log 0 of transaction 0's receipt (the block's first ERC-20 Transfer: emitter and recipient checked)";
    let b = block(&out, &format!("dry run: {log}"), 0);
    field(b, "  verdict", &["attested: 1 statement(s)"]);
    field(
        b,
        "    [0] log",
        &[
            "log tx 0 log 0 emitter 0x5543ca71ed8c88daf217488b1808994e080d925b",
            "topic0 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef (3 topics, 32 data bytes)",
        ],
    );
    let b = block(&out, &format!("send: {log}"), 0);
    field(b, "  result", &["applied in block", "; exit None; 1 event(s); equal to the dry run: yes"]);

    // The instance's configuration as the zone holds it (`config`): the chain id, the checkpoint
    // of `Init`, the committee and finality parameters, and the id derived again from the
    // configuration and the checkpoint. `config.txt` records this command word for word.
    let b = block(&out, "config (instance(id)'s state on the zone)", 0);
    field(b, "  chain id", &["11155111"]);
    field(
        b,
        "  checkpoint",
        &[
            "0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d",
            "(the trusted beacon block root of Init)",
        ],
    );
    field(b, "  instance id", &[&instance(), "derived again, equal"]);
    field(b, "  sync committee", &["512 keys, at least 1 signing; 32 slots per epoch, 256 epochs per period"]);
    field(
        b,
        "  genesis",
        &[
            "time 1655733600, 12 s per slot",
            "validators root 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078",
        ],
    );
}

#[test]
fn an_update_is_advanced_and_the_same_one_again_is_refused() {
    // The second invocation, following one period short of the last: 1408 is the shortest (and
    // only) `--follow-to` that leaves work for `advance` — 1409's update verifies against the
    // sync committee that 1408's update adopts, and following further would apply it already.
    // `advance.txt` records exactly this run (`README.md`'s second invocation; `advance-exit.txt`:
    // exit 3).
    let (code, out, err) = attest(&words(concat!(
        "--follow-to 1408",
        " advance --period 1409",
        " --then advance --period 1409 --send",
        " --then advance --period 1409",
        " --then advance --period 1409 --send",
    )));
    // The second submission is refused as its dry run predicted; the first was applied; neither
    // dry run disagreed with its execution (that would be exit 1).
    assert_eq!(code, Some(3), "the run's exit status\nstdout:\n{out}\nstderr:\n{err}");
    assert_eq!(out.matches("equal to the dry run: yes").count(), 2, "{out}");
    assert!(!out.contains("equal to the dry run: NO"), "{out}");
    has(&err, FOLLOW_TO_1408);
    let dry = "dry run: advance with the update of period 1409";
    let sent = "send: advance with the update of period 1409";

    // Accepted the first time: step 1 (`ferrata_core::steps::step1`, the program's own function)
    // takes the update; the job advances the finalized header, adopts the next committee and
    // rotates; `job_plan` sends it as five transactions (`Open`, `Miller`, two `Adopt`s,
    // `Finish`). Sent: applied, the finalized slot advances, equal to the dry run.
    let b = block(&out, dry, 0);
    field(
        b,
        "  verdict",
        &[
            "step 1 accepts (ferrata_core::steps::step1, the program's own function); ",
            "the job advances the finalized header, adopts the next committee, rotates to the next period",
        ],
    );
    field(b, "  cost", &["5 transaction(s)"]);
    field(b, "  nothing was sent", &["(add --send to settle it in the harness's zone)"]);
    let b = block(&out, sent, 0);
    field(
        b,
        "  result",
        &["5 transaction(s) settled, all applied; finalized slot 11537760 -> 11542528; ", "equal to the dry run: yes"],
    );

    // The same update again: refused with the typed code 52 `NotNewer`, predicted by the dry run
    // — step 1's verdict and the `Open` transaction's reversion — and confirmed by what was
    // settled.
    let b = block(&out, dry, 1);
    field(
        b,
        "  verdict",
        &[
            "refused at step 1 with exit code 52 NotNewer (the core): Ferrata policy: ",
            "the update does not advance the finalized header (duplicate or stale).",
        ],
    );
    field(
        b,
        "  Open (step 1)",
        &[
            "reverts with exit code 52 NotNewer (the core): Ferrata policy: ",
            "the update does not advance the finalized header (duplicate or stale).",
        ],
    );
    let b = block(&out, sent, 1);
    field(
        b,
        "  result",
        &["1 transaction(s) settled, refused; finalized slot 11542528 -> 11542528; ", "equal to the dry run: yes"],
    );
}

#[test]
fn the_configuration_cannot_be_sent() {
    // `parse` refuses `--send` on a command that sends nothing before anything is followed or
    // built: exit 2, the error and the usage on standard error, nothing on stdout. Recorded as
    // `config-usage.txt`.
    let (code, out, err) = attest(&["config", "--send"]);
    assert_eq!(code, Some(2), "the run's exit status\nstdout:\n{out}\nstderr:\n{err}");
    assert_eq!(out, "", "a usage error prints nothing on stdout");
    has(&err, "ferrata-attest: config sends nothing");
    has(
        &err,
        concat!(
            "usage: ferrata-attest [--follow-to P] [--message-out DIR] COMMAND [ARGS] [--send] ",
            "[--then COMMAND ...]"
        ),
    );
}
