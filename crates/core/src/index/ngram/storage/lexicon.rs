//! Sorted N-gram key -> postings slice descriptor.

use std::path::Path;

use crate::index::mmap::mmap_open;
use crate::index::ngram::gram::{Gram, GramWidth};
use crate::index::ngram::storage::format::LEXICON_MAGIC;

use super::{read_u32_le, read_u64_le};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconEntry {
    pub gram: Gram,
    pub offset: u64,
    pub len: u32,
}

/// Memory-mapped lexicon.
#[derive(Debug)]
pub struct Lexicon {
    width: GramWidth,
    data: memmap2::Mmap,
    count: usize,
}

impl Lexicon {
    const HEADER_SIZE: usize = 12;

    const fn entry_size(width: GramWidth) -> usize {
        width.get() + 12
    }

    /// Encode lexicon entries into the on-disk file format.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry count exceeds `u32::MAX`.
    pub fn encode(width: GramWidth, entries: &[LexiconEntry]) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::with_capacity(
            LEXICON_MAGIC.len() + Self::HEADER_SIZE + entries.len() * Self::entry_size(width),
        );
        data.extend_from_slice(&LEXICON_MAGIC);
        data.extend_from_slice(&width.as_u32().to_le_bytes());
        let count = u32::try_from(entries.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "lexicon entry count exceeds u32::MAX",
            )
        })?;
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        for e in entries {
            e.gram.write_bytes(width, &mut data);
            data.extend_from_slice(&e.offset.to_le_bytes());
            data.extend_from_slice(&e.len.to_le_bytes());
        }
        Ok(data)
    }

    fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Write a lexicon to a file and return a memory-mapped instance.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding, writing, or reopening fails.
    pub fn create(
        path: &Path,
        width: GramWidth,
        entries: &[LexiconEntry],
    ) -> std::io::Result<Self> {
        let data = Self::encode(width, entries)?;
        std::fs::write(path, &data)?;
        Self::open(path, width)
    }

    /// Open a lexicon from a memory-mapped file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or the format is invalid.
    pub fn open(path: &Path, width: GramWidth) -> std::io::Result<Self> {
        let mmap = mmap_open(path)?;
        let count = Self::validate(&mmap, width)?;
        Ok(Self {
            width,
            data: mmap,
            count,
        })
    }

    fn validate(bytes: &[u8], width: GramWidth) -> std::io::Result<usize> {
        let magic_len = LEXICON_MAGIC.len();
        if bytes.len() < magic_len + Self::HEADER_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "lexicon too short for header",
            ));
        }
        if bytes[..magic_len] != LEXICON_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected lexicon magic",
            ));
        }
        let stored_width = read_u32_le(bytes, magic_len);
        if stored_width != width.as_u32() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "lexicon gram width {stored_width} does not match expected {}",
                    width.get()
                ),
            ));
        }
        let n = read_u32_le(bytes, magic_len + 4) as usize;
        let expected_bytes = n * Self::entry_size(width);
        if bytes.len() < magic_len + Self::HEADER_SIZE + expected_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "lexicon truncated",
            ));
        }
        let entries = &bytes[magic_len + Self::HEADER_SIZE..];
        let mut prev: Option<(Gram, u64)> = None;
        for chunk in entries.chunks_exact(Self::entry_size(width)) {
            let gram = Gram::read_bytes(width, &chunk[..width.get()])?;
            let posting_off = read_u64_le(chunk, width.get());
            if let Some((prev_gram, prev_off)) = prev {
                if gram <= prev_gram {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "lexicon grams out of order",
                    ));
                }
                if posting_off < prev_off {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "lexicon posting offsets are not monotonic",
                    ));
                }
            }
            prev = Some((gram, posting_off));
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

    /// Look up a lexicon entry for `gram` via binary search.
    ///
    /// # Panics
    ///
    /// Panics if an entry key cannot be decoded; keys are validated when the lexicon is opened.
    #[must_use]
    pub fn get(&self, gram: Gram) -> Option<LexiconEntry> {
        if self.count == 0 {
            return None;
        }
        let bytes = self.bytes();
        let data_start = LEXICON_MAGIC.len() + Self::HEADER_SIZE;
        let gram_width = self.width.get();
        let entry_size = Self::entry_size(self.width);

        let mut lo = 0;
        let mut hi = self.count;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let offset = data_start + mid * entry_size;
            let entry_gram = Gram::read_bytes(self.width, &bytes[offset..offset + gram_width])
                .expect("gram key validated at open");

            match entry_gram.cmp(&gram) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let off = read_u64_le(bytes, offset + gram_width);
                    let len = read_u32_le(bytes, offset + gram_width + 8);
                    return Some(LexiconEntry {
                        gram: entry_gram,
                        offset: off,
                        len,
                    });
                }
            }
        }
        None
    }

    #[must_use]
    pub fn posting_byte_end(&self, offset: u64, payload_len: usize) -> usize {
        if self.count == 0 {
            return payload_len;
        }
        let bytes = self.bytes();
        let data_start = LEXICON_MAGIC.len() + Self::HEADER_SIZE;
        let entry_size = Self::entry_size(self.width);
        let off_delta = self.width.get();
        let mut lo = 0usize;
        let mut hi = self.count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let off = data_start + mid * entry_size + off_delta;
            let entry_off = read_u64_le(bytes, off);
            if entry_off <= offset {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < self.count {
            let off = data_start + lo * entry_size + off_delta;
            usize::try_from(read_u64_le(bytes, off)).unwrap_or(payload_len)
        } else {
            payload_len
        }
    }

    #[must_use]
    pub fn iter(&self) -> LexiconIter<'_> {
        self.into_iter()
    }
}

pub struct LexiconIter<'a> {
    lexicon: &'a Lexicon,
    idx: usize,
}

impl<'a> IntoIterator for &'a Lexicon {
    type Item = LexiconEntry;
    type IntoIter = LexiconIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        LexiconIter {
            lexicon: self,
            idx: 0,
        }
    }
}

impl Iterator for LexiconIter<'_> {
    type Item = LexiconEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.lexicon.count {
            return None;
        }
        let bytes = self.lexicon.bytes();
        let data_start = LEXICON_MAGIC.len() + Lexicon::HEADER_SIZE;
        let entry_size = Lexicon::entry_size(self.lexicon.width);
        let offset = data_start + self.idx * entry_size;
        self.idx += 1;
        let gram = Gram::read_bytes(
            self.lexicon.width,
            &bytes[offset..offset + self.lexicon.width.get()],
        )
        .expect("gram key validated at open");
        let posting_offset = read_u64_le(bytes, offset + self.lexicon.width.get());
        let len = read_u32_le(bytes, offset + self.lexicon.width.get() + 8);
        Some(LexiconEntry {
            gram,
            offset: posting_offset,
            len,
        })
    }
}
