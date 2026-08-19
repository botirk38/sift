//! Runtime AST index: language map and kind postings over a shared
//! [`Files`] id space.

use std::path::{Path, PathBuf};

use super::build::KindTables;
use super::language::AstLanguage;
use super::storage::kinds::Kinds;
use crate::index::postings::{POSTINGS_BIN, Postings};
use crate::index::{FileId, Files};
use crate::search::Query;

pub const KINDS_BIN: &str = "kinds.bin";

/// Errors specific to opening or persisting an AST index.
#[derive(Debug, thiserror::Error)]
pub enum AstIndexError {
    #[error("index component missing: {0}")]
    MissingComponent(PathBuf),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Runtime AST index (kind artifacts only).
#[derive(Debug)]
pub struct Index {
    storage: Storage,
}

#[derive(Debug)]
struct Storage {
    kinds: Kinds,
    file_count: usize,
    /// Languages whose persisted grammar fingerprint differs from this
    /// build's. Their kind postings are unusable (kind ids belong to another
    /// grammar build); the files themselves stay covered.
    stale: Vec<AstLanguage>,
}

impl Index {
    /// Build AST artifacts into `dir` over shared `files`.
    ///
    /// Artifacts are always written, including for an empty corpus: a
    /// manifest-listed namespace with missing artifacts would make the whole
    /// snapshot unopenable.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing, encoding, or writing fails.
    pub fn build(dir: &Path, files: &Files) -> crate::Result<()> {
        let tables = KindTables::assemble(files)?;
        std::fs::create_dir_all(dir)?;
        let kinds_path = dir.join(KINDS_BIN);
        let postings_path = dir.join(POSTINGS_BIN);

        let (kr, pr) = rayon::join(
            || Kinds::create(&kinds_path, &tables),
            || Postings::create(&postings_path, &tables.postings),
        );
        let kinds = kr.map_err(AstIndexError::Io)?;
        let postings = pr.map_err(AstIndexError::Io)?;
        let stale = Self::validate_kinds_postings(&kinds, &postings, files.len())?;
        debug_assert!(stale.is_empty(), "fresh build cannot have stale grammars");
        Ok(())
    }

    /// Open a previously persisted AST index from `index_dir`.
    ///
    /// A persisted grammar fingerprint that differs from this build marks the
    /// language stale rather than erroring: search stays correct (stale
    /// languages never narrow) and `sift index update` can rebuild the store.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence files are missing or malformed.
    pub fn open(index_dir: &Path, file_count: usize) -> crate::Result<Self> {
        let kinds_path = index_dir.join(KINDS_BIN);
        let postings_path = index_dir.join(POSTINGS_BIN);

        for p in [&kinds_path, &postings_path] {
            if !p.is_file() {
                return Err(AstIndexError::MissingComponent(p.clone()).into());
            }
        }

        let kinds = Kinds::open(&kinds_path).map_err(AstIndexError::Io)?;
        let postings = Postings::open(&postings_path).map_err(AstIndexError::Io)?;
        let stale = Self::validate_kinds_postings(&kinds, &postings, file_count)?;
        // The postings mmap is only cross-validated for now; the structural
        // query arm that consumes it lands with the `--ast` search surface.
        drop(postings);

        Ok(Self {
            storage: Storage {
                kinds,
                file_count,
                stale,
            },
        })
    }

    /// Cross-validate the two artifacts and return the stale language set.
    fn validate_kinds_postings(
        kinds: &Kinds,
        postings: &Postings,
        file_count: usize,
    ) -> Result<Vec<AstLanguage>, AstIndexError> {
        let malformed = |msg: String| {
            AstIndexError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
        };

        if kinds.file_count() != file_count {
            return Err(malformed(format!(
                "kinds table covers {} files but the snapshot has {file_count}",
                kinds.file_count(),
            )));
        }

        let mut stale = Vec::new();
        for i in 0..kinds.lang_count() {
            let (code, fingerprint) = kinds.lang_entry(i).expect("validated at open");
            // Codes minted by a newer sift are unknown here; their postings
            // are simply never consulted, so they need no staleness entry.
            if let Some(lang) = AstLanguage::from_code(code)
                && lang.grammar_fingerprint() != fingerprint
            {
                stale.push(lang);
            }
        }

        let payload_len = postings.payload_len();
        let mut next_offset = 0u64;
        for i in 0..kinds.dir_count() {
            let entry = kinds.dir_entry(i).expect("validated at open");
            if entry.offset != next_offset {
                return Err(malformed(format!(
                    "kind directory entry {i} offset {} is not contiguous (expected {next_offset})",
                    entry.offset,
                )));
            }
            let start = usize::try_from(entry.offset)
                .map_err(|_| malformed(format!("kind directory entry {i} offset exceeds usize")))?;
            let end = start.checked_add(entry.len as usize).ok_or_else(|| {
                malformed(format!("kind directory entry {i} posting range overflows"))
            })?;
            if end > payload_len {
                return Err(malformed(format!(
                    "kind directory entry {i} posting range [{start},{end}) exceeds payload_len {payload_len}",
                )));
            }
            let ids = Postings::decode_sorted(postings.slice(start, entry.len as usize))
                .map_err(|e| malformed(format!("posting list for directory entry {i}: {e}")))?;
            if ids.iter().any(|&id| id as usize >= file_count) {
                return Err(malformed(format!(
                    "posting list for directory entry {i} names a file beyond the shared table",
                )));
            }
            next_offset = u64::try_from(end).expect("payload length fits u64");
        }
        if next_offset != u64::try_from(payload_len).expect("payload length fits u64") {
            return Err(malformed(format!(
                "postings payload has {payload_len} bytes but the directory covers {next_offset}",
            )));
        }

        Ok(stale)
    }

    #[must_use]
    pub const fn file_count(&self) -> usize {
        self.storage.file_count
    }

    /// Languages recorded in the persisted table and known to this build.
    #[must_use]
    pub fn languages(&self) -> Vec<AstLanguage> {
        (0..self.storage.kinds.lang_count())
            .filter_map(|i| self.storage.kinds.lang_entry(i))
            .filter_map(|(code, _)| AstLanguage::from_code(code))
            .collect()
    }

    /// Languages whose persisted grammar differs from this build's.
    #[must_use]
    pub fn stale_languages(&self) -> Vec<AstLanguage> {
        self.storage.stale.clone()
    }

    /// Every shared file id whose language is `lang`, ascending.
    #[must_use]
    pub fn files_of(&self, lang: AstLanguage) -> Vec<FileId> {
        let code = lang.code();
        (0..self.storage.file_count)
            .filter(|&id| self.storage.kinds.file_lang(id) == code)
            .map(FileId::new)
            .collect()
    }

    /// Resolve candidate file ids for the query.
    ///
    /// Until structural queries exist, every query falls back to the full
    /// shared id space: this kind cannot narrow, so it must not under-return.
    #[must_use]
    pub(crate) fn query(&self, _query: &Query) -> Vec<FileId> {
        (0..self.storage.file_count).map(FileId::new).collect()
    }
}
