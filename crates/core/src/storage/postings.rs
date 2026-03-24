//! Contiguous `u32` LE file-id payloads referenced by the lexicon.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use memmap2::Mmap;

use crate::storage::format::{write_magic, POSTINGS_MAGIC};
use crate::storage::mmap::open_mmap;

/// Write postings blob to `out_path`.
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

#[derive(Debug)]
pub struct MappedPostings {
    backing: Backing,
    payload_len: usize,
}

#[derive(Debug)]
enum Backing {
    Mmap(Mmap),
    Owned(Vec<u8>),
}

impl MappedPostings {
    fn bytes(&self) -> &[u8] {
        match &self.backing {
            Backing::Mmap(m) => m.as_ref(),
            Backing::Owned(v) => v.as_slice(),
        }
    }

    #[must_use]
    pub fn from_bytes(payload: &[u8]) -> Self {
        let mut data = Vec::with_capacity(POSTINGS_MAGIC.len() + 4 + payload.len());
        data.extend_from_slice(&POSTINGS_MAGIC);
        let plen = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        data.extend_from_slice(&plen.to_le_bytes());
        data.extend_from_slice(payload);
        Self {
            backing: Backing::Owned(data),
            payload_len: payload.len(),
        }
    }

    /// Open postings from a memory-mapped file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is malformed.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mmap = open_mmap(path)?;
        let bytes = mmap.as_ref();
        let payload_len = Self::validate(bytes)?;
        Ok(Self {
            backing: Backing::Mmap(mmap),
            payload_len,
        })
    }

    fn validate(bytes: &[u8]) -> std::io::Result<usize> {
        let magic_len = POSTINGS_MAGIC.len();
        if bytes.len() < magic_len + 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "postings too short for magic+len",
            ));
        }
        if bytes[..magic_len] != POSTINGS_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected postings magic",
            ));
        }
        let plen = u32::from_le_bytes(bytes[magic_len..magic_len + 4].try_into().unwrap()) as usize;
        if bytes.len() < magic_len + 4 + plen {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "postings payload shorter than declared length",
            ));
        }
        Ok(plen)
    }

    #[must_use]
    pub fn slice(&self, start: usize, len: usize) -> &[u8] {
        let payload_start = POSTINGS_MAGIC.len() + 4;
        let start = payload_start + start;
        self.bytes().get(start..start + len).unwrap_or(&[])
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        let payload_start = POSTINGS_MAGIC.len() + 4;
        &self.bytes()[payload_start..payload_start + self.payload_len]
    }

    #[must_use]
    pub fn backing_slice(&self) -> &[u8] {
        self.bytes()
    }
}
