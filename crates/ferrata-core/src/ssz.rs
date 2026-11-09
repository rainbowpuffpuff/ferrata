//! Minimal, strict SSZ decoding for the light-client containers.
//!
//! Only what the light client needs. Every length and offset is checked: a container must be
//! consumed exactly, the first offset must equal the fixed-part length, and offsets must be
//! non-decreasing and in bounds.

use alloc::vec::Vec;

use crate::{Error, Root};

pub(crate) fn u64_at(b: &[u8], at: usize) -> u64 {
    let mut le = [0u8; 8];
    le.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(le)
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> u32 {
    let mut le = [0u8; 4];
    le.copy_from_slice(&b[at..at + 4]);
    u32::from_le_bytes(le)
}

pub(crate) fn root_at(b: &[u8], at: usize) -> Root {
    let mut r = [0u8; 32];
    r.copy_from_slice(&b[at..at + 32]);
    r
}

/// One field of an SSZ container.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Field {
    /// Fixed-size field of this many bytes.
    Fixed(usize),
    /// Variable-size field, encoded as a 4-byte offset in the fixed part.
    Variable,
}

/// Split a container encoding into per-field slices.
pub(crate) fn split_container<'a>(bytes: &'a [u8], fields: &[Field]) -> Result<Vec<&'a [u8]>, Error> {
    let fixed_len: usize = fields
        .iter()
        .map(|f| match f {
            Field::Fixed(n) => *n,
            Field::Variable => 4,
        })
        .sum();
    if bytes.len() < fixed_len {
        return Err(Error::Malformed);
    }
    let mut parts: Vec<&[u8]> = Vec::with_capacity(fields.len());
    let mut offsets: Vec<(usize, usize)> = Vec::new(); // (field index, offset)
    let mut pos = 0;
    for (i, field) in fields.iter().enumerate() {
        match *field {
            Field::Fixed(n) => {
                parts.push(&bytes[pos..pos + n]);
                pos += n;
            }
            Field::Variable => {
                offsets.push((i, u32_at(bytes, pos) as usize));
                parts.push(&[]);
                pos += 4;
            }
        }
    }
    match offsets.first() {
        None if bytes.len() != fixed_len => return Err(Error::Malformed),
        Some(&(_, first)) if first != fixed_len => return Err(Error::Malformed),
        _ => {}
    }
    for k in 0..offsets.len() {
        let (field, start) = offsets[k];
        let end = offsets.get(k + 1).map_or(bytes.len(), |next| next.1);
        if start > end || end > bytes.len() {
            return Err(Error::Malformed);
        }
        parts[field] = &bytes[start..end];
    }
    Ok(parts)
}

/// Decode `Vector[Bytes32, len]`.
pub(crate) fn decode_branch(b: &[u8], len: usize) -> Result<Vec<Root>, Error> {
    if b.len() != len * 32 {
        return Err(Error::Malformed);
    }
    Ok(b.chunks_exact(32).map(|c| root_at(c, 0)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_offsets() {
        let fields = [Field::Fixed(2), Field::Variable, Field::Variable];
        // fixed part = 2 + 4 + 4 = 10
        let mut ok = [0u8; 13];
        ok[2..6].copy_from_slice(&10u32.to_le_bytes());
        ok[6..10].copy_from_slice(&11u32.to_le_bytes());
        let parts = split_container(&ok, &fields).unwrap();
        assert_eq!(parts[1].len(), 1);
        assert_eq!(parts[2].len(), 2);

        let mut first_wrong = ok;
        first_wrong[2..6].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(split_container(&first_wrong, &fields), Err(Error::Malformed));

        let mut decreasing = ok;
        decreasing[6..10].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(split_container(&decreasing, &fields), Err(Error::Malformed));

        let mut beyond = ok;
        beyond[6..10].copy_from_slice(&14u32.to_le_bytes());
        assert_eq!(split_container(&beyond, &fields), Err(Error::Malformed));

        assert_eq!(split_container(&ok[..9], &fields), Err(Error::Malformed));
        // Fixed-only containers must be consumed exactly.
        assert_eq!(split_container(&[0u8; 3], &[Field::Fixed(2)]), Err(Error::Malformed));
    }
}
