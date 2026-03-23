//! File table: sequential file id → relative path (UTF-8).

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::storage::format::{read_exact_magic, write_magic, FILES_MAGIC};

/// Write `paths` in order (id = index in slice).
///
/// # Errors
///
/// Propagates IO errors from writing `out_path`.
pub fn write_files_table(out_path: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    let f = File::create(out_path)?;
    let mut w = BufWriter::new(f);
    write_magic(&mut w, FILES_MAGIC)?;
    let count: u32 = paths
        .len()
        .try_into()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "too many files"))?;
    w.write_all(&count.to_le_bytes())?;
    for p in paths {
        let s = p.to_string_lossy();
        let bytes = s.as_bytes();
        let len: u32 = bytes
            .len()
            .try_into()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path too long"))?;
        w.write_all(&len.to_le_bytes())?;
        w.write_all(bytes)?;
    }
    w.flush()?;
    Ok(())
}

/// Read file table: ordered paths (id = index).
///
/// # Errors
///
/// Returns [`std::io::Error`] on read failure or malformed data.
pub fn read_files_table(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut f = File::open(path)?;
    read_exact_magic(&mut f, FILES_MAGIC)?;
    let mut count_buf = [0u8; 4];
    f.read_exact(&mut count_buf)?;
    let count = u32::from_le_bytes(count_buf) as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut len_buf = [0u8; 4];
        f.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf)?;
        let s = std::str::from_utf8(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        out.push(PathBuf::from(s));
    }
    Ok(out)
}
