//! The attestation program's messages in recorded LEZ blocks, as IDL vectors (`idl/README.md`).
//!
//! `idl-recorded OUT.json RUN BLOCKS PROGRAMS [RUN BLOCKS PROGRAMS ...]` reads each recorded block
//! log (`blocks.borsh`, a borsh list of LEZ v0.4 blocks as the sequencer served them) and its
//! `programs.tsv`, takes every public transaction whose root message goes to a program named
//! there (Ferrata's attestation program; not LEZ's `scripted` test program), decodes the message with `ferrata_lez_wire::Message`, and writes its bytes and its Rust
//! `Debug` text. `idl/check.py --recorded OUT.json` then decodes the bytes with the IDL alone.
//! Messages of other programs (the fee program, the loader) are counted, not written.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use common::{block::Block, transaction::LeeTransaction};
use ferrata_lez_wire::Message;

fn label(m: &Message) -> &'static str {
    match m {
        Message::Init { .. } => "Init",
        Message::Open { .. } => "Open",
        Message::Miller => "Miller",
        Message::Adopt { .. } => "Adopt",
        Message::Finish => "Finish",
        Message::Read => "Read",
        Message::ReadPadded { .. } => "ReadPadded",
        Message::Attest { .. } => "Attest",
        Message::VerifyAll { .. } => "VerifyAll",
        Message::JobOpen { .. } => "JobOpen",
        Message::Apply { .. } => "Apply",
        Message::ApplyVerified { .. } => "ApplyVerified",
        Message::Record { .. } => "Record",
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 || (args.len() - 1) % 3 != 0 {
        bail!("usage: idl-recorded OUT.json RUN BLOCKS PROGRAMS [RUN BLOCKS PROGRAMS ...]");
    }
    let mut vectors = Vec::new();
    let mut summary = Vec::new();
    for run in args[1..].chunks(3) {
        let (name, blocks_path, programs_path) = (&run[0], Path::new(&run[1]), Path::new(&run[2]));
        let programs: BTreeMap<String, String> = fs::read_to_string(programs_path)
            .with_context(|| format!("reading {}", programs_path.display()))?
            .lines()
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() > 1).then(|| (f[1].to_string(), f[0].to_string()))
            })
            // LEZ's own `scripted` test program, deployed beside Ferrata's in the live run.
            .filter(|(_, name)| name != "scripted")
            .collect();
        let blocks: Vec<Block> = borsh::from_slice(&fs::read(blocks_path)?)
            .map_err(|e| anyhow!("{}: not a borsh list of LEZ blocks: {e}", blocks_path.display()))?;
        let (mut public, mut others, mut ours) = (0usize, 0usize, BTreeMap::<&str, usize>::new());
        let (mut older, mut first_older) = (0usize, String::new());
        for block in &blocks {
            for (position, tx) in block.body.transactions.iter().enumerate() {
                let LeeTransaction::Public(p) = tx else { continue };
                public += 1;
                let root = &p.message().execution.root;
                let program = root.to.program_account_id.to_string();
                if !programs.contains_key(&program) {
                    others += 1;
                    continue;
                }
                // A run recorded with an older image of the program may hold messages of an
                // older wire: those are counted, not written.
                let message: Message = match borsh::from_slice(&root.message) {
                    Ok(m) => m,
                    Err(e) => {
                        if older == 0 {
                            first_older = format!("block {} tx {position}: {e}", block.header.block_id);
                        }
                        older += 1;
                        continue;
                    }
                };
                let l = label(&message);
                *ours.entry(l).or_default() += 1;
                let debug = format!("{message:?}");
                if debug.contains('"') || debug.contains('\\') {
                    bail!("Debug text needs escaping");
                }
                vectors.push(format!(
                    "    {{\"name\": \"{name}/block {}/tx {position}: {l}\", \"type\": \"Message\", \"hex\": \"{}\",\n     \"debug\": \"{debug}\"}}",
                    block.header.block_id,
                    hex::encode(&root.message)
                ));
            }
        }
        summary.push(format!(
            "{name}: {} blocks, {public} public transactions, {} to the attestation program decoded by today's wire ({}), {older} of an older wire{}, {others} to other programs",
            blocks.len(),
            ours.values().sum::<usize>(),
            ours.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "),
            if older > 0 { format!(" (first: {first_older})") } else { String::new() }
        ));
    }
    let out = format!(
        "{{\n  \"program\": \"ferrata_attestation\",\n  \"generator\": \"lez/live/driver/src/bin/idl-recorded.rs\",\n  \"sources\": {},\n  \"vectors\": [\n{}\n  ]\n}}\n",
        serde_json::to_string(&summary)?,
        vectors.join(",\n")
    );
    fs::write(&args[0], out)?;
    for line in &summary {
        println!("{line}");
    }
    Ok(())
}
