//! The manifest of a run (`run.tsv`): one row per transaction sent, with what the sequencer did
//! with it, and the harness's figure for the same step where there is one.

use std::{collections::HashMap, fs, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::follower::Receipt;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Phase of the run (deploy, replay, scenarios, capacity, private).
    pub phase: String,
    pub step: String,
    pub tx_hash: String,
    /// Block and position within it; empty if the sequencer never included the transaction.
    pub block: String,
    pub position: String,
    /// applied | refused (a program exit code) | reverted (another charged failure) |
    /// dropped (accepted by the RPC, never included) | rejected (refused by the RPC) |
    /// unfinished (a proof that was stopped).
    pub status: String,
    /// The program's exit code and its name, for a refusal.
    pub exit: String,
    pub exec_gas: String,
    pub stor_gas: String,
    pub gas_limit: String,
    pub base_fee_exec: String,
    pub base_fee_stor: String,
    /// Base fee charged by LEZ's settlement.
    pub fee: String,
    /// What the payer's balance fell by, read from the sequencer before and after.
    pub payer_debit: String,
    /// User transactions in the block, and the gas totals in the block's own fee transaction.
    pub block_txs: String,
    pub block_exec_gas: String,
    pub block_stor_gas: String,
    /// The same step in the harness tables (`lez/results/*.md`), where there is one.
    pub harness_exec_gas: String,
    pub harness_stor_gas: String,
    pub harness_result: String,
    /// LEZ's reason for a revert, or the sequencer's log line for a dropped transaction.
    pub note: String,
}

pub const COLUMNS: [&str; 21] = [
    "phase",
    "step",
    "tx_hash",
    "block",
    "position",
    "status",
    "exit",
    "exec_gas",
    "stor_gas",
    "gas_limit",
    "base_fee_exec",
    "base_fee_stor",
    "fee",
    "payer_debit",
    "block_txs",
    "block_exec_gas",
    "block_stor_gas",
    "harness_exec_gas",
    "harness_stor_gas",
    "harness_result",
    "note",
];

fn clean(s: &str) -> String {
    let s = s.replace(['\t', '\n', '\r'], " ");
    if s.is_empty() {
        "-".to_string()
    } else {
        s
    }
}

impl Row {
    fn fields(&self) -> [&String; 21] {
        [
            &self.phase,
            &self.step,
            &self.tx_hash,
            &self.block,
            &self.position,
            &self.status,
            &self.exit,
            &self.exec_gas,
            &self.stor_gas,
            &self.gas_limit,
            &self.base_fee_exec,
            &self.base_fee_stor,
            &self.fee,
            &self.payer_debit,
            &self.block_txs,
            &self.block_exec_gas,
            &self.block_stor_gas,
            &self.harness_exec_gas,
            &self.harness_stor_gas,
            &self.harness_result,
            &self.note,
        ]
    }

    pub fn line(&self) -> String {
        self.fields().iter().map(|f| clean(f)).collect::<Vec<_>>().join("\t")
    }

    pub fn parse(line: &str) -> Result<Self> {
        let f: Vec<String> = line.split('\t').map(|s| if s == "-" { String::new() } else { s.to_string() }).collect();
        if f.len() != COLUMNS.len() {
            bail!("run.tsv: expected {} fields, got {}: {line}", COLUMNS.len(), f.len());
        }
        let mut it = f.into_iter();
        let mut next = || it.next().expect("counted");
        Ok(Self {
            phase: next(),
            step: next(),
            tx_hash: next(),
            block: next(),
            position: next(),
            status: next(),
            exit: next(),
            exec_gas: next(),
            stor_gas: next(),
            gas_limit: next(),
            base_fee_exec: next(),
            base_fee_stor: next(),
            fee: next(),
            payer_debit: next(),
            block_txs: next(),
            block_exec_gas: next(),
            block_stor_gas: next(),
            harness_exec_gas: next(),
            harness_stor_gas: next(),
            harness_result: next(),
            note: next(),
        })
    }

    /// Fill the columns LEZ's settlement determines.
    pub fn with_receipt(mut self, receipt: &Receipt, exit_name: impl Fn(u32) -> String) -> Self {
        self.block = receipt.block_id.to_string();
        self.position = receipt.position.to_string();
        self.status = receipt.status().to_string();
        self.exit = receipt.exit_code.map(|c| format!("{c} ({})", exit_name(c))).unwrap_or_default();
        self.exec_gas = receipt.gas_exec.to_string();
        self.stor_gas = receipt.gas_stor.to_string();
        self.gas_limit = receipt.gas_limit.map(|g| g.to_string()).unwrap_or_default();
        self.base_fee_exec = receipt.base_fee_exec.to_string();
        self.base_fee_stor = receipt.base_fee_stor.to_string();
        self.fee = receipt.fee_base.clone();
        self.block_txs = receipt.block_user_txs.to_string();
        self.block_exec_gas = receipt.block_gas_exec.to_string();
        self.block_stor_gas = receipt.block_gas_stor.to_string();
        if let Some(reason) = &receipt.reason {
            if receipt.exit_code.is_none() {
                self.note = if self.note.is_empty() { reason.clone() } else { format!("{reason}; {}", self.note) };
            }
        }
        self
    }
}

pub fn write(path: &Path, rows: &[Row]) -> Result<()> {
    let mut text = COLUMNS.join("\t");
    text.push('\n');
    for row in rows {
        text.push_str(&row.line());
        text.push('\n');
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

pub fn read(path: &Path) -> Result<Vec<Row>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines = text.lines();
    let header = lines.next().ok_or_else(|| anyhow!("{} is empty", path.display()))?;
    if header != COLUMNS.join("\t") {
        bail!("{}: unexpected header", path.display());
    }
    lines.filter(|l| !l.trim().is_empty()).map(Row::parse).collect()
}

/// One row of a harness table (`lez/results/stage1-*.md`).
#[derive(Clone, Debug)]
pub struct HarnessRow {
    pub exec_gas: u64,
    pub stor_gas: u64,
    pub result: String,
}

/// The harness tables, keyed by (file stem, transaction label).
#[derive(Default)]
pub struct HarnessTables {
    rows: HashMap<(String, String), HarnessRow>,
}

fn number(cell: &str) -> Option<u64> {
    cell.trim().replace(',', "").parse().ok()
}

impl HarnessTables {
    /// Reads the first table of each `stage1-*.md` in `dir` (the one `harness::render` writes).
    pub fn load(dir: &Path) -> Result<Self> {
        let mut rows = HashMap::new();
        for stem in ["stage1-replay", "stage1-finality", "stage1-refusals", "stage1-one-transaction"] {
            let path = dir.join(format!("{stem}.md"));
            let Ok(text) = fs::read_to_string(&path) else { continue };
            for line in text.lines() {
                if line.starts_with("## ") {
                    break; // the per-handler table follows
                }
                let cells: Vec<&str> = line.split('|').map(str::trim).collect();
                // | # | transaction | handlers | execution gas | of 5M target | storage gas | base fee | result |
                if cells.len() < 10 || cells[1].parse::<u32>().is_err() {
                    continue;
                }
                let (Some(exec_gas), Some(stor_gas)) = (number(cells[4]), number(cells[6])) else { continue };
                rows.insert(
                    (stem.to_string(), cells[2].to_string()),
                    HarnessRow { exec_gas, stor_gas, result: cells[8].to_string() },
                );
            }
        }
        Ok(Self { rows })
    }

    pub fn get(&self, table: &str, label: &str) -> Option<&HarnessRow> {
        self.rows.get(&(table.to_string(), label.to_string()))
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }
}
