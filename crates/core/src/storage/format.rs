//! Shared magic bytes and little-endian helpers.

use std::io::Write;

pub const FILES_MAGIC: [u8; 8] = *b"SIFTFIL2";
pub const LEXICON_MAGIC: [u8; 8] = *b"SIFTLEX1";
pub const POSTINGS_MAGIC: [u8; 8] = *b"SIFTPST1";

/// # Errors
///
/// Propagates IO errors from `w`.
pub fn write_magic<W: Write>(w: &mut W, magic: [u8; 8]) -> std::io::Result<()> {
    w.write_all(&magic)
}
