//! On-disk tables for N-gram indexes.

pub mod format;
pub mod lexicon;

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
