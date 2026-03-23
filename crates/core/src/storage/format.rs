//! Shared magic bytes and little-endian helpers.

use std::io::{Read, Write};

pub const FILES_MAGIC: [u8; 8] = *b"SIFTFIL1";
pub const LEXICON_MAGIC: [u8; 8] = *b"SIFTLEX1";
pub const POSTINGS_MAGIC: [u8; 8] = *b"SIFTPST1";

/// # Errors
///
/// Propagates IO errors from `w`.
pub fn write_magic<W: Write>(w: &mut W, magic: [u8; 8]) -> std::io::Result<()> {
    w.write_all(&magic)
}

/// # Errors
///
/// Returns an error if the file does not start with `magic`.
pub fn read_exact_magic<R: Read>(r: &mut R, magic: [u8; 8]) -> std::io::Result<()> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    if buf != magic {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unexpected index file magic",
        ));
    }
    Ok(())
}
