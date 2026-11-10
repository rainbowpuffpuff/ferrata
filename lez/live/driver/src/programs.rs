//! The guest binaries the run deploys: RISC Zero `ProgramBinary` files (`.bin`), as risc0-build
//! writes them and as LEZ's wallet takes them for `program-loader deploy`.

use std::{fs, path::Path};

use anyhow::{anyhow, ensure, Context, Result};
use lee_core::program::ProgramHeader;
use program_loader_core::MAX_SEGMENT_DATA_LEN;
use sha2::{Digest, Sha256};

/// The program header a loader actor state holds: a tagged `LoaderEntry` from LEZ v0.4.0-rc2 on
/// (#943).
#[cfg(feature = "lez-rc2")]
pub fn program_header(loader_actor_state: &[u8]) -> Option<ProgramHeader> {
    ProgramHeader::from_loader_actor_state(loader_actor_state)
}

/// The program header a loader actor state holds (LEZ v0.4.0-rc1: untagged).
#[cfg(not(feature = "lez-rc2"))]
pub fn program_header(loader_actor_state: &[u8]) -> Option<ProgramHeader> {
    ProgramHeader::from_bytes(loader_actor_state)
}

/// The image id as hex (little-endian words, as risc0 prints a `Digest`).
pub fn image_id_hex(id: [u32; 8]) -> String {
    hex::encode(id.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Clone)]
pub struct GuestBinary {
    pub name: String,
    /// The whole `ProgramBinary` (user ELF and RISC Zero's v1compat kernel).
    pub bytes: Vec<u8>,
    /// The user ELF: what LEZ's loader stores, in segments.
    pub user_elf: Vec<u8>,
    pub image_id: [u32; 8],
}

impl GuestBinary {
    pub fn load(name: &str, path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_bytes(name, bytes)
    }

    pub fn from_bytes(name: &str, bytes: Vec<u8>) -> Result<Self> {
        let binary = risc0_binfmt::ProgramBinary::decode(&bytes)
            .map_err(|e| anyhow!("{name}: not a RISC Zero program binary: {e}"))?;
        ensure!(
            binary.kernel_elf == risc0_zkos_v1compat::V1COMPAT_ELF,
            "{name}: built with another kernel than RISC Zero's v1compat, which LEZ's loader assumes"
        );
        let image_id: [u32; 8] =
            binary.compute_image_id().map_err(|e| anyhow!("{name}: computing the image id: {e}"))?.into();
        let user_elf = binary.user_elf.to_vec();
        Ok(Self { name: name.to_string(), bytes, user_elf, image_id })
    }

    pub fn image_hex(&self) -> String {
        image_id_hex(self.image_id)
    }

    pub fn sha256(&self) -> String {
        sha256_hex(&self.bytes)
    }

    /// The chunks LEZ's wallet uploads, one per segment account.
    pub fn chunks(&self) -> Vec<&[u8]> {
        self.user_elf.chunks(MAX_SEGMENT_DATA_LEN).collect()
    }
}
