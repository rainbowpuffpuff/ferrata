//! A strict, bounds-checked cursor over borrowed bytes. Every read either returns exactly what the
//! encoding says or fails; nothing panics on malformed input.

use crate::Error;

/// A cursor over `buf`. Slices it hands out borrow from `buf`.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Offset of the next unread byte.
    pub const fn pos(&self) -> usize {
        self.pos
    }

    pub const fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// The bytes from `start` up to the current position.
    pub fn since(&self, start: usize) -> &'a [u8] {
        &self.buf[start..self.pos]
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).filter(|&end| end <= self.buf.len());
        let end = end.ok_or(Error::Truncated { at: self.pos })?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn skip(&mut self, n: usize) -> Result<(), Error> {
        self.take(n).map(|_| ())
    }

    pub fn array<const N: usize>(&mut self) -> Result<&'a [u8; N], Error> {
        let at = self.pos;
        self.take(N)?.try_into().map_err(|_| Error::Truncated { at })
    }

    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16_le(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(*self.array()?))
    }

    pub fn u32_le(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(*self.array()?))
    }

    pub fn u64_le(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(*self.array()?))
    }

    /// A borsh `u32` length, as `usize`, checked against the remaining input when every element is
    /// at least `min_elem` bytes (so a hostile length cannot make a caller loop for nothing).
    pub fn len_u32(&mut self, what: &'static str, min_elem: usize) -> Result<usize, Error> {
        let len = usize::try_from(self.u32_le()?).map_err(|_| Error::BadLength { what })?;
        let need = len.checked_mul(min_elem).ok_or(Error::BadLength { what })?;
        if need > self.remaining() {
            return Err(Error::BadLength { what });
        }
        Ok(len)
    }

    /// A borsh `Vec<u8>` (u32 length, then the bytes).
    pub fn bytes_u32(&mut self, what: &'static str) -> Result<&'a [u8], Error> {
        let len = self.len_u32(what, 1)?;
        self.take(len)
    }

    /// A borsh `bool`: exactly 0 or 1.
    pub fn bool(&mut self, what: &'static str) -> Result<bool, Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(Error::BadTag { what, tag }),
        }
    }

    /// A borsh `Option` tag: exactly 0 (None) or 1 (Some).
    pub fn option(&mut self, what: &'static str) -> Result<bool, Error> {
        self.bool(what)
    }

    /// Fails unless every byte was read.
    pub fn finish(&self) -> Result<(), Error> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(Error::TrailingBytes { at: self.pos })
        }
    }
}
