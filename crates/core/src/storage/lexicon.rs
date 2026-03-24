//! Sorted trigram → postings slice descriptor.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use memmap2::Mmap;

use crate::storage::format::{write_magic, LEXICON_MAGIC};
use crate::storage::mmap::open_mmap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconEntry {
    pub trigram: [u8; 3],
    pub offset: u64,
    pub len: u32,
}

/// Write sorted `entries` to `out_path`.
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

/// Memory-mapped lexicon view with owning storage.
///
/// Binary search over on-disk sorted trigram table.
#[derive(Debug)]
pub struct MappedLexicon {
    backing: Backing,
    count: usize,
}

#[derive(Debug)]
enum Backing {
    Mmap(Mmap),
    Owned(Vec<u8>),
}

impl MappedLexicon {
    const ENTRY_SIZE: usize = 15;

    fn bytes(&self) -> &[u8] {
        match &self.backing {
            Backing::Mmap(m) => m.as_ref(),
            Backing::Owned(v) => v.as_slice(),
        }
    }

    /// Construct from a list of lexicon entries (used by the builder for in-memory index).
    ///
    /// # Panics
    ///
    /// Panics if `entries.len()` exceeds `u32::MAX`.
    #[must_use]
    pub fn from_entries(entries: &[LexiconEntry]) -> Self {
        let count = entries.len();
        let size = LEXICON_MAGIC.len() + 4 + count * Self::ENTRY_SIZE;
        let mut data = vec![0u8; size];
        let mut cursor = 0;

        data[cursor..cursor + LEXICON_MAGIC.len()].copy_from_slice(&LEXICON_MAGIC);
        cursor += LEXICON_MAGIC.len();

        data[cursor..cursor + 4].copy_from_slice(&u32::try_from(count).unwrap().to_le_bytes());
        cursor += 4;

        for e in entries {
            data[cursor..cursor + 3].copy_from_slice(&e.trigram);
            cursor += 3;
            data[cursor..cursor + 8].copy_from_slice(&e.offset.to_le_bytes());
            cursor += 8;
            data[cursor..cursor + 4].copy_from_slice(&e.len.to_le_bytes());
            cursor += 4;
        }

        Self {
            backing: Backing::Owned(data),
            count,
        }
    }

    /// Open a lexicon from a memory-mapped file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is malformed.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mmap = open_mmap(path)?;
        let bytes = mmap.as_ref();
        let count = Self::validate(bytes)?;
        Ok(Self {
            backing: Backing::Mmap(mmap),
            count,
        })
    }

    fn validate(bytes: &[u8]) -> std::io::Result<usize> {
        let magic_len = LEXICON_MAGIC.len();
        if bytes.len() < magic_len + 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "lexicon too short for magic+count",
            ));
        }
        if bytes[..magic_len] != LEXICON_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected lexicon magic",
            ));
        }
        let n = u32::from_le_bytes(bytes[magic_len..magic_len + 4].try_into().unwrap()) as usize;
        let expected_bytes = n * Self::ENTRY_SIZE;
        if bytes.len() < magic_len + 4 + expected_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "lexicon truncated",
            ));
        }
        Ok(n)
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Binary search for `tri` in the mapped lexicon. Returns the entry if found.
    ///
    /// # Panics
    ///
    /// Panics if the internal data layout is corrupted (out of bounds read).
    #[must_use]
    pub fn get(&self, tri: [u8; 3]) -> Option<LexiconEntry> {
        if self.count == 0 {
            return None;
        }
        let bytes = self.bytes();
        let magic_len = LEXICON_MAGIC.len();
        let data_start = magic_len + 4;

        let mut lo = 0;
        let mut hi = self.count;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let offset = data_start + mid * Self::ENTRY_SIZE;
            let entry_tri: [u8; 3] = bytes[offset..offset + 3].try_into().unwrap();

            match entry_tri.cmp(&tri) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let off =
                        u64::from_le_bytes(bytes[offset + 3..offset + 11].try_into().unwrap());
                    let len =
                        u32::from_le_bytes(bytes[offset + 11..offset + 15].try_into().unwrap());
                    return Some(LexiconEntry {
                        trigram: entry_tri,
                        offset: off,
                        len,
                    });
                }
            }
        }
        None
    }

    #[must_use]
    pub const fn iter(&self) -> MappedLexiconIter<'_> {
        MappedLexiconIter {
            lexicon: self,
            pos: 0,
        }
    }

    #[must_use]
    pub fn backing_slice(&self) -> &[u8] {
        self.bytes()
    }
}

impl<'a> IntoIterator for &'a MappedLexicon {
    type Item = LexiconEntry;
    type IntoIter = MappedLexiconIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        MappedLexiconIter {
            lexicon: self,
            pos: 0,
        }
    }
}

pub struct MappedLexiconIter<'_a> {
    lexicon: &'_a MappedLexicon,
    pos: usize,
}

impl Iterator for MappedLexiconIter<'_> {
    type Item = LexiconEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.lexicon.count {
            return None;
        }
        let bytes = self.lexicon.bytes();
        let magic_len = LEXICON_MAGIC.len();
        let offset = magic_len + 4 + self.pos * MappedLexicon::ENTRY_SIZE;
        let tri: [u8; 3] = bytes[offset..offset + 3].try_into().unwrap();
        let off = u64::from_le_bytes(bytes[offset + 3..offset + 11].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[offset + 11..offset + 15].try_into().unwrap());
        self.pos += 1;
        Some(LexiconEntry {
            trigram: tri,
            offset: off,
            len,
        })
    }
}
