//! Runtime N-gram index: lexicon and postings over a shared [`Files`] id space.

use std::path::{Path, PathBuf};

use crate::index::FileId;
use crate::index::Files;
use crate::search::{Case, Query};

use super::build::IndexTables;
use super::gram::{GramNorm, GramWidth};
use super::storage::lexicon::Lexicon;
use crate::index::postings::{POSTINGS_BIN, Postings};

/// Errors specific to opening or persisting an N-gram index.
#[derive(Debug, thiserror::Error)]
pub enum NGramIndexError {
    #[error("index component missing: {0}")]
    MissingComponent(PathBuf),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Runtime-width N-gram index (kind artifacts only).
#[derive(Debug)]
pub struct Index {
    pub(crate) width: GramWidth,
    pub(crate) norm: GramNorm,
    pub(crate) storage: Storage,
}

#[derive(Debug)]
pub struct Storage {
    pub(crate) lexicon: Lexicon,
    pub(crate) postings: Postings,
    pub(crate) file_count: usize,
}

impl Storage {
    pub(crate) const fn new(lexicon: Lexicon, postings: Postings, file_count: usize) -> Self {
        Self {
            lexicon,
            postings,
            file_count,
        }
    }
}

impl Index {
    const fn from_parts(width: GramWidth, norm: GramNorm, storage: Storage) -> Self {
        Self {
            width,
            norm,
            storage,
        }
    }

    #[must_use]
    pub const fn gram_width(&self) -> GramWidth {
        self.width
    }

    #[must_use]
    pub const fn gram_norm(&self) -> GramNorm {
        self.norm
    }

    #[must_use]
    pub const fn file_count(&self) -> usize {
        self.storage.file_count
    }

    /// Restrict candidate file ids when this kind can narrow the query.
    ///
    /// `None` means this kind does not restrict the file set.
    ///
    /// # Errors
    ///
    /// Returns an error if a posting list is corrupt.
    pub(crate) fn query(&self, query: &Query) -> crate::Result<Option<Vec<FileId>>> {
        let Some(arms) = Self::extract_literal_arms(self.width, query) else {
            return Ok(None);
        };
        let arms = match (self.norm, query.case()) {
            (GramNorm::Identity, Case::Sensitive) => arms,
            (GramNorm::AsciiLower, Case::Insensitive) => arms
                .into_iter()
                .map(|arm| GramNorm::AsciiLower.normalize_literal(&arm))
                .collect(),
            (GramNorm::Identity, Case::Insensitive) | (GramNorm::AsciiLower, Case::Sensitive) => {
                return Ok(None);
            }
        };
        let ids = self.candidate_file_ids(&arms)?;
        Ok(Some(
            ids.into_iter()
                .filter_map(|id| usize::try_from(id).ok().map(FileId::new))
                .collect(),
        ))
    }

    /// Decode-and-check every posting list. Used at write only; open trusts
    /// the snapshot this already proved.
    pub(crate) fn validate_lexicon_postings(
        lexicon: &Lexicon,
        postings: &Postings,
    ) -> Result<(), NGramIndexError> {
        let payload_len = postings.payload_len();
        for entry in lexicon {
            let start = usize::try_from(entry.offset).map_err(|_| {
                NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "lexicon entry {:?} offset {} exceeds usize",
                        entry.gram, entry.offset
                    ),
                ))
            })?;
            let end = lexicon.posting_byte_end(entry.offset, payload_len);
            if start > end || end > payload_len {
                return Err(NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "lexicon entry {:?} posting range [{start},{end}) exceeds payload_len {payload_len}",
                        entry.gram,
                    ),
                )));
            }
            let slice = postings.slice(start, end.saturating_sub(start));
            let decoded_count = Postings::decode_sorted(slice)
                .map_err(|e| {
                    NGramIndexError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("posting list for gram {:?}: {e}", entry.gram),
                    ))
                })?
                .len();
            if decoded_count != entry.len as usize {
                return Err(NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "lexicon entry {:?} claims len {} but posting list has {decoded_count} entries",
                        entry.gram, entry.len,
                    ),
                )));
            }
        }
        Ok(())
    }

    /// Build N-gram artifacts into `dir` over shared `files`.
    ///
    /// # Errors
    ///
    /// Returns an error if extraction or encoding fails.
    pub fn build(width: GramWidth, norm: GramNorm, dir: &Path, files: &Files) -> crate::Result<()> {
        let tables = IndexTables::assemble(width, norm, files)?;
        Self::write_tables(width, &tables, dir)?;
        Ok(())
    }

    fn write_tables(width: GramWidth, tables: &IndexTables, dir: &Path) -> crate::Result<()> {
        std::fs::create_dir_all(dir)?;
        let lexicon_path = dir.join(super::LEXICON_BIN);
        let postings_path = dir.join(POSTINGS_BIN);

        let (lr, pr) = rayon::join(
            || Lexicon::create(&lexicon_path, width, &tables.lexicon),
            || Postings::create(&postings_path, &tables.postings),
        );

        let lexicon = lr.map_err(crate::Error::Io)?;
        let postings = pr.map_err(crate::Error::Io)?;
        Self::validate_lexicon_postings(&lexicon, &postings)?;
        Ok(())
    }

    /// Open a previously persisted N-gram index from `index_dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence files are missing or malformed.
    pub fn open(
        width: GramWidth,
        norm: GramNorm,
        index_dir: &Path,
        file_count: usize,
    ) -> crate::Result<Self> {
        let lexicon_path = index_dir.join(super::LEXICON_BIN);
        let postings_path = index_dir.join(POSTINGS_BIN);

        for p in [&lexicon_path, &postings_path] {
            if !p.is_file() {
                return Err(NGramIndexError::MissingComponent(p.clone()).into());
            }
        }

        let lexicon = Lexicon::open(&lexicon_path, width).map_err(NGramIndexError::Io)?;
        let postings = Postings::open(&postings_path).map_err(NGramIndexError::Io)?;

        Ok(Self::from_parts(
            width,
            norm,
            Storage::new(lexicon, postings, file_count),
        ))
    }
}
