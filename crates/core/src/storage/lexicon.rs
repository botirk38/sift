//! Sorted trigram → postings slice descriptor.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use crate::storage::format::{read_exact_magic, write_magic, LEXICON_MAGIC};

/// One lexicon row: trigram and location inside `postings.bin` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconEntry {
    pub trigram: [u8; 3],
    /// Byte offset from start of postings **payload** (after magic + length prefix).
    pub offset: u64,
    /// Number of `u32` file ids in this slice.
    pub len: u32,
}

/// Write sorted `entries` (caller must sort by `trigram`).
///
/// # Errors
///
/// Propagates IO errors from writing `out_path`.
pub fn write_lexicon(out_path: &Path, entries: &[LexiconEntry]) -> std::io::Result<()> {
    let f = File::create(out_path)?;
    let mut w = BufWriter::new(f);
    write_magic(&mut w, LEXICON_MAGIC)?;
    let n: u32 = entries
        .len()
        .try_into()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "lexicon too large"))?;
    w.write_all(&n.to_le_bytes())?;
    for e in entries {
        w.write_all(&e.trigram)?;
        w.write_all(&e.offset.to_le_bytes())?;
        w.write_all(&e.len.to_le_bytes())?;
    }
    w.flush()?;
    Ok(())
}

/// Read lexicon entries (sorted on disk).
///
/// # Errors
///
/// Returns [`std::io::Error`] on read failure or malformed data.
pub fn read_lexicon(path: &Path) -> std::io::Result<Vec<LexiconEntry>> {
    let mut f = File::open(path)?;
    read_exact_magic(&mut f, LEXICON_MAGIC)?;
    let mut nbuf = [0u8; 4];
    f.read_exact(&mut nbuf)?;
    let n = u32::from_le_bytes(nbuf) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut t = [0u8; 3];
        f.read_exact(&mut t)?;
        let mut ob = [0u8; 8];
        f.read_exact(&mut ob)?;
        let mut lb = [0u8; 4];
        f.read_exact(&mut lb)?;
        out.push(LexiconEntry {
            trigram: t,
            offset: u64::from_le_bytes(ob),
            len: u32::from_le_bytes(lb),
        });
    }
    Ok(out)
}
