//! On-disk tables for N-gram indexes.

use std::sync::Arc;

pub mod format;
pub mod lexicon;
pub mod postings;

/// Backing bytes for an opened table (owned or mmap).
#[derive(Debug)]
pub enum ArtifactData {
    Memory(Arc<[u8]>),
    Mmap(memmap2::Mmap),
}

impl AsRef<[u8]> for ArtifactData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Memory(bytes) => bytes,
            Self::Mmap(mmap) => mmap.as_ref(),
        }
    }
}

pub(super) fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("slice is exactly 4 bytes"),
    )
}

pub(super) fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("slice is exactly 8 bytes"),
    )
}
