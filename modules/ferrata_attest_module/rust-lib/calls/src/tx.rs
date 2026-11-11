//! The attestation program's `Attest` message and the account it is sent to, and the program's
//! `Attested` event, encoded as the program encodes them (`lez/wire/src/lib.rs`). The module does
//! not link the program's crate (it links LEZ and the RISC Zero client, which the module builder
//! cannot build from the module's directory), so the Borsh layout is written out here; the
//! end-to-end test (`modules/scripts/e2e.sh`, `lez/harness/src/bin/module_attest.rs`) decodes these
//! bytes with the program's own types and executes them.

use sha2::{Digest, Sha256};

/// `Message::Attest`'s variant index: `Init`, `Open`, `Miller`, `Adopt`, `Finish`, `Read`,
/// `ReadPadded`, `Attest`.
pub const ATTEST_VARIANT: u8 = 7;
/// The program's event selector for an attestation (`ATTESTED_SELECTOR`).
pub const ATTESTED_SELECTOR: [u8; 8] = *b"FRT-ATT1";
/// The program's domain for its account seeds (`DOMAIN` in `lez/wire`).
const DOMAIN: &[u8] = b"ferrata-lez/v1/";
/// LEZ v0.4's prefix for a program-derived account id (`AccountId::for_public_pda`).
const PDA_PREFIX: &[u8; 32] = b"/LEE/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00\x00";

/// One log of the request (`LogQuery` without conditions).
pub struct LogQuery {
    pub tx_index: u64,
    pub log_index: u64,
    pub proof: Vec<Vec<u8>>,
}

/// One storage slot of an account (`SlotQuery` without conditions).
pub struct SlotQuery {
    pub slot: [u8; 32],
    pub proof: Vec<Vec<u8>>,
}

/// One account and its slots (`AccountQuery` without conditions).
pub struct AccountQuery {
    pub address: [u8; 20],
    pub proof: Vec<Vec<u8>>,
    pub slots: Vec<SlotQuery>,
}

/// `attest::Request` with no RLP header, hops, leaves or relations.
pub struct Request {
    pub instance_id: [u8; 32],
    pub chain_id: u64,
    pub accounts: Vec<AccountQuery>,
    pub logs: Vec<LogQuery>,
}

fn len(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn byte_lists(out: &mut Vec<u8>, items: &[Vec<u8>]) {
    len(out, items.len());
    for item in items {
        len(out, item.len());
        out.extend_from_slice(item);
    }
}

/// Borsh of `Message::Attest { request }`.
pub fn attest_message(r: &Request) -> Vec<u8> {
    let mut out = vec![ATTEST_VARIANT];
    out.extend_from_slice(&r.instance_id);
    out.extend_from_slice(&r.chain_id.to_le_bytes());
    out.push(0); // header: None
    len(&mut out, 0); // hops
    len(&mut out, r.accounts.len());
    for a in &r.accounts {
        out.extend_from_slice(&a.address);
        byte_lists(&mut out, &a.proof);
        len(&mut out, 0); // conditions
        len(&mut out, a.slots.len());
        for s in &a.slots {
            out.extend_from_slice(&s.slot);
            byte_lists(&mut out, &s.proof);
            len(&mut out, 0); // conditions
        }
    }
    len(&mut out, r.logs.len());
    for l in &r.logs {
        out.extend_from_slice(&l.tx_index.to_le_bytes());
        out.extend_from_slice(&l.log_index.to_le_bytes());
        byte_lists(&mut out, &l.proof);
        len(&mut out, 0); // conditions
    }
    len(&mut out, 0); // leaves
    len(&mut out, 0); // relations
    out
}

/// `header_seed(instance_id, slot)`: SHA-256(`ferrata-lez/v1/` ‖ `header` ‖ id ‖ slot LE).
pub fn header_seed(instance_id: &[u8; 32], slot: u64) -> [u8; 32] {
    Sha256::new()
        .chain_update(DOMAIN)
        .chain_update(b"header")
        .chain_update(instance_id)
        .chain_update(slot.to_le_bytes())
        .finalize()
        .into()
}

/// LEZ's public program-derived account id: SHA-256(prefix ‖ program ‖ seed).
pub fn public_pda(program: &[u8; 32], seed: &[u8; 32]) -> [u8; 32] {
    Sha256::new().chain_update(PDA_PREFIX).chain_update(program).chain_update(seed).finalize().into()
}

/// The account `header(id, slot)` of the program at `program`: where `Attest` is sent.
pub fn header_account(program: &[u8; 32], instance_id: &[u8; 32], slot: u64) -> [u8; 32] {
    public_pda(program, &header_seed(instance_id, slot))
}

/// The `Attested` event's data: instance id, chain id (LE), block number (LE), then one 32-byte
/// statement id per fact.
pub struct Attested {
    pub instance_id: [u8; 32],
    pub chain_id: u64,
    pub block_number: u64,
    pub statements: Vec<[u8; 32]>,
}

pub fn decode_attested(data: &[u8]) -> Result<Attested, String> {
    if data.len() < 48 || (data.len() - 48) % 32 != 0 {
        return Err(format!("event data of {} bytes is not 48 + 32k", data.len()));
    }
    let mut instance_id = [0u8; 32];
    instance_id.copy_from_slice(&data[..32]);
    Ok(Attested {
        instance_id,
        chain_id: u64::from_le_bytes(data[32..40].try_into().unwrap()),
        block_number: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        statements: data[48..].chunks(32).map(|c| c.try_into().unwrap()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        let r = Request {
            instance_id: [1; 32],
            chain_id: 1,
            accounts: vec![],
            logs: vec![LogQuery { tx_index: 2, log_index: 3, proof: vec![vec![0xaa, 0xbb]] }],
        };
        let m = attest_message(&r);
        // variant, id, chain, None, hops, accounts, logs(1): tx, log, proof(1: len 2, 2 bytes), conditions; leaves; relations
        assert_eq!(m.len(), 1 + 32 + 8 + 1 + 4 + 4 + 4 + 8 + 8 + 4 + 4 + 2 + 4 + 4 + 4);
        assert_eq!(m[0], ATTEST_VARIANT);
        let event = [[7u8; 32].as_slice(), &1u64.to_le_bytes(), &5u64.to_le_bytes(), &[9u8; 32]].concat();
        let a = decode_attested(&event).unwrap();
        assert_eq!((a.chain_id, a.block_number, a.statements.len()), (1, 5, 1));
        assert!(decode_attested(&event[..50]).is_err());
    }
}
