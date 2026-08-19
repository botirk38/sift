//! On-disk language map and node-kind directory (`kinds.bin`).
//!
//! ```text
//! magic      : 8 bytes  "SIFTAKN1"
//! lang_count : u32 LE   (every language compiled into this build)
//! file_count : u32 LE   (the shared snapshot FileId space)
//! lang_table : lang_count x { code u16 LE | pad u16 | grammar_fp u64 LE }
//! file_lang  : file_count x u16 LE  (0xFFFF = no language)
//! dir_count  : u32 LE
//! directory  : dir_count x { lang u16 LE | kind u16 LE | offset u64 LE | len u32 LE }
//! ```
//!
//! Directory entries are sorted by `(lang, kind)`; `offset`/`len` address the
//! sibling `postings.bin` payload. Postings themselves are validated by
//! [`crate::index::ast::Index::open`], which sees both artifacts.

use std::path::Path;

use crate::index::ast::build::{DirEntry, KindTables};
use crate::index::ast::language::AstLanguage;
use crate::index::mmap::mmap_open;

pub const KINDS_MAGIC: [u8; 8] = *b"SIFTAKN1";

/// A file with no supported language (unknown extension, non-UTF-8, or
/// unreadable at build time). Present in the shared table, never a
/// structural candidate.
pub const NO_LANGUAGE: u16 = u16::MAX;

const HEADER_LEN: usize = KINDS_MAGIC.len() + 4 + 4;
const LANG_ENTRY_LEN: usize = 2 + 2 + 8;
const DIR_ENTRY_LEN: usize = 2 + 2 + 8 + 4;

#[derive(Debug)]
pub struct Kinds {
    data: memmap2::Mmap,
    lang_count: usize,
    file_count: usize,
    dir_count: usize,
}

impl Kinds {
    /// Encode the tables, stamping every compiled-in language fingerprint.
    ///
    /// # Errors
    ///
    /// Returns an error if a table dimension exceeds its on-disk width.
    pub fn encode(tables: &KindTables) -> std::io::Result<Vec<u8>> {
        let lang_count = u32::try_from(AstLanguage::ALL.len())
            .expect("language set is a small compile-time constant");
        let file_count = u32::try_from(tables.file_lang.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "too many indexed files")
        })?;
        let dir_count = u32::try_from(tables.directory.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "too many kind directory entries",
            )
        })?;

        let mut out = Vec::with_capacity(
            HEADER_LEN
                + AstLanguage::ALL.len() * LANG_ENTRY_LEN
                + tables.file_lang.len() * 2
                + 4
                + tables.directory.len() * DIR_ENTRY_LEN,
        );
        out.extend_from_slice(&KINDS_MAGIC);
        out.extend_from_slice(&lang_count.to_le_bytes());
        out.extend_from_slice(&file_count.to_le_bytes());
        for lang in AstLanguage::ALL {
            out.extend_from_slice(&lang.code().to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&lang.grammar_fingerprint().to_le_bytes());
        }
        for &lang in &tables.file_lang {
            out.extend_from_slice(&lang.to_le_bytes());
        }
        out.extend_from_slice(&dir_count.to_le_bytes());
        for entry in &tables.directory {
            out.extend_from_slice(&entry.lang.to_le_bytes());
            out.extend_from_slice(&entry.kind.to_le_bytes());
            out.extend_from_slice(&entry.offset.to_le_bytes());
            out.extend_from_slice(&entry.len.to_le_bytes());
        }
        Ok(out)
    }

    /// Write `kinds.bin` and return an mmap-backed instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written or reopened.
    pub fn create(path: &Path, tables: &KindTables) -> std::io::Result<Self> {
        let bytes = Self::encode(tables)?;
        std::fs::write(path, bytes)?;
        Self::open(path)
    }

    /// Open `kinds.bin` from a memory-mapped file.
    ///
    /// # Errors
    ///
    /// Returns an error if the table is malformed.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mmap = mmap_open(path)?;
        let (lang_count, file_count, dir_count) = Self::validate(&mmap)?;
        Ok(Self {
            data: mmap,
            lang_count,
            file_count,
            dir_count,
        })
    }

    fn malformed(msg: String) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
    }

    fn validate(bytes: &[u8]) -> std::io::Result<(usize, usize, usize)> {
        let oversized = || Self::malformed("kinds table sizes overflow".into());
        let section = |start: usize, count: usize, width: usize| {
            count
                .checked_mul(width)
                .and_then(|len| start.checked_add(len))
                .ok_or_else(oversized)
        };

        if bytes.len() < HEADER_LEN {
            return Err(Self::malformed("kinds table too short for header".into()));
        }
        if bytes[..KINDS_MAGIC.len()] != KINDS_MAGIC {
            return Err(Self::malformed("unexpected kinds table magic".into()));
        }
        let lang_count = Self::read_u32_le(bytes, KINDS_MAGIC.len()) as usize;
        let file_count = Self::read_u32_le(bytes, KINDS_MAGIC.len() + 4) as usize;

        let file_lang_start = section(HEADER_LEN, lang_count, LANG_ENTRY_LEN)?;
        let dir_count_at = section(file_lang_start, file_count, 2)?;
        if bytes.len() < dir_count_at.checked_add(4).ok_or_else(oversized)? {
            return Err(Self::malformed(
                "kinds table too short for language map".into(),
            ));
        }

        let mut codes = vec![false; usize::from(u16::MAX) + 1];
        let mut prev_code = 0u16;
        for i in 0..lang_count {
            let at = HEADER_LEN + i * LANG_ENTRY_LEN;
            let code = Self::read_u16_le(bytes, at);
            if code <= prev_code {
                return Err(Self::malformed(format!(
                    "language table entry {i} code {code} is not strictly increasing"
                )));
            }
            codes[usize::from(code)] = true;
            prev_code = code;
        }

        for id in 0..file_count {
            let lang = Self::read_u16_le(bytes, file_lang_start + id * 2);
            if lang != NO_LANGUAGE && !codes[usize::from(lang)] {
                return Err(Self::malformed(format!(
                    "file {id} maps to language code {lang} missing from the table"
                )));
            }
        }

        let dir_count = Self::read_u32_le(bytes, dir_count_at) as usize;
        let dir_start = dir_count_at + 4;
        let expected_len = section(dir_start, dir_count, DIR_ENTRY_LEN)?;
        if bytes.len() < expected_len {
            return Err(Self::malformed(
                "kinds table too short for kind directory".into(),
            ));
        }
        if bytes.len() > expected_len {
            return Err(Self::malformed(
                "kinds table has trailing bytes after kind directory".into(),
            ));
        }

        let mut prev_key = None;
        for i in 0..dir_count {
            let at = dir_start + i * DIR_ENTRY_LEN;
            let lang = Self::read_u16_le(bytes, at);
            let kind = Self::read_u16_le(bytes, at + 2);
            if !codes[usize::from(lang)] {
                return Err(Self::malformed(format!(
                    "kind directory entry {i} names language code {lang} missing from the table"
                )));
            }
            let key = (lang, kind);
            if prev_key.is_some_and(|prev| prev >= key) {
                return Err(Self::malformed(format!(
                    "kind directory entry {i} is not strictly sorted by (lang, kind)"
                )));
            }
            prev_key = Some(key);
        }

        Ok((lang_count, file_count, dir_count))
    }

    #[must_use]
    pub const fn file_count(&self) -> usize {
        self.file_count
    }

    /// Language table row `i` as `(code, grammar fingerprint)`.
    #[must_use]
    pub fn lang_entry(&self, i: usize) -> Option<(u16, u64)> {
        if i >= self.lang_count {
            return None;
        }
        let at = HEADER_LEN + i * LANG_ENTRY_LEN;
        Some((
            Self::read_u16_le(&self.data, at),
            Self::read_u64_le(&self.data, at + 4),
        ))
    }

    #[must_use]
    pub const fn lang_count(&self) -> usize {
        self.lang_count
    }

    /// Persisted language code for `id` (`NO_LANGUAGE` when absent).
    #[must_use]
    pub fn file_lang(&self, id: usize) -> u16 {
        if id >= self.file_count {
            return NO_LANGUAGE;
        }
        let file_lang_start = HEADER_LEN + self.lang_count * LANG_ENTRY_LEN;
        Self::read_u16_le(&self.data, file_lang_start + id * 2)
    }

    #[must_use]
    pub const fn dir_count(&self) -> usize {
        self.dir_count
    }

    /// Kind directory row `i`.
    #[must_use]
    pub fn dir_entry(&self, i: usize) -> Option<DirEntry> {
        if i >= self.dir_count {
            return None;
        }
        let dir_start = HEADER_LEN + self.lang_count * LANG_ENTRY_LEN + self.file_count * 2 + 4;
        let at = dir_start + i * DIR_ENTRY_LEN;
        Some(DirEntry {
            lang: Self::read_u16_le(&self.data, at),
            kind: Self::read_u16_le(&self.data, at + 2),
            offset: Self::read_u64_le(&self.data, at + 4),
            len: Self::read_u32_le(&self.data, at + 12),
        })
    }

    fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(
            bytes[offset..offset + 2]
                .try_into()
                .expect("slice is exactly 2 bytes"),
        )
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("slice is exactly 4 bytes"),
        )
    }

    fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .expect("slice is exactly 8 bytes"),
        )
    }
}
