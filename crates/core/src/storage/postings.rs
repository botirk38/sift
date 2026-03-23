//! Contiguous `u32` LE file-id payloads referenced by the lexicon.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use crate::storage::format::{read_exact_magic, write_magic, POSTINGS_MAGIC};

/// Write postings blob: header + concatenated little-endian `u32` ids.
///
/// # Errors
///
/// Propagates IO errors from writing `out_path`.
pub fn write_postings(out_path: &Path, payload: &[u8]) -> std::io::Result<()> {
    let f = File::create(out_path)?;
    let mut w = BufWriter::new(f);
    write_magic(&mut w, POSTINGS_MAGIC)?;
    let plen: u32 = payload
        .len()
        .try_into()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "postings too large"))?;
    w.write_all(&plen.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()?;
    Ok(())
}

/// Read full postings payload (bytes after header); caller interprets as `u32` slices.
///
/// # Errors
///
/// Returns [`std::io::Error`] on read failure or malformed data.
pub fn read_postings(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    read_exact_magic(&mut f, POSTINGS_MAGIC)?;
    let mut len_buf = [0u8; 4];
    f.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf)?;
    Ok(buf)
}
