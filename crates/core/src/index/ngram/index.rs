use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::corpus::Candidate;
use crate::index::{CorpusKind, FileId, IndexedCorpus};
use crate::search::{Case, SearchQuery};

use super::files::{FileFingerprint, FileTable};
use super::gram::{GramMatch, GramNorm, GramWidth};
use super::storage::grams::GramSets;
use super::storage::lexicon::Lexicon;
use super::storage::postings::Postings;

/// Errors specific to opening or persisting an N-gram index.
#[derive(Debug, thiserror::Error)]
pub enum NGramIndexError {
    #[error("index component missing: {0}")]
    MissingComponent(PathBuf),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Catalog handle or opened runtime-width N-gram index.
#[derive(Debug)]
pub struct Index {
    pub(crate) width: GramWidth,
    pub(crate) norm: GramNorm,
    pub(crate) storage: Option<Storage>,
}

impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.norm == other.norm
    }
}

impl Eq for Index {}

impl Hash for Index {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.hash(state);
        self.norm.hash(state);
    }
}

#[derive(Debug)]
pub struct Storage {
    pub(crate) root: PathBuf,
    pub(crate) files: IndexedFiles,
    pub(crate) gram_sets: GramSets,
    pub(crate) lexicon: Lexicon,
    pub(crate) postings: Postings,
    pub(crate) corpus_kind: CorpusKind,
}

#[derive(Debug)]
pub struct IndexedFiles {
    table: FileTable,
    fingerprints: OnceLock<Vec<FileFingerprint>>,
    coverage: OnceLock<IndexedCorpus>,
}

/// Whether [`IndexedFiles`] is loaded from disk or already in memory.
pub enum IndexedFilesLocation {
    /// `files.bin` on disk; fingerprints decode on first use.
    Disk(FileTable),
    /// Fingerprints already in memory (just-built index).
    Memory {
        table: FileTable,
        fingerprints: Vec<FileFingerprint>,
    },
}

impl IndexedFiles {
    pub(crate) fn new(location: IndexedFilesLocation) -> std::io::Result<Self> {
        match location {
            IndexedFilesLocation::Disk(table) => {
                table.validate_paths()?;
                Ok(Self {
                    table,
                    fingerprints: OnceLock::new(),
                    coverage: OnceLock::new(),
                })
            }
            IndexedFilesLocation::Memory {
                table,
                fingerprints,
            } => {
                let decoded = OnceLock::new();
                let _ = decoded.set(fingerprints);
                Ok(Self {
                    table,
                    fingerprints: decoded,
                    coverage: OnceLock::new(),
                })
            }
        }
    }

    fn fingerprints(&self) -> &[FileFingerprint] {
        self.fingerprints.get_or_init(|| {
            self.table
                .to_fingerprints()
                .expect("paths validated at open")
        })
    }

    pub(crate) fn as_slice(&self) -> &[FileFingerprint] {
        self.fingerprints()
    }

    pub(crate) fn get(&self, id: FileId) -> Option<&FileFingerprint> {
        self.fingerprints().get(id.get())
    }

    /// Borrow a single file row without decoding the full fingerprint table.
    pub(crate) fn row(&self, id: FileId) -> Option<super::files::FileRow<'_>> {
        self.table.row(id.get()).ok()
    }

    pub(crate) const fn len(&self) -> usize {
        self.table.len()
    }

    pub(crate) fn coverage(&self) -> IndexedCorpus {
        self.coverage
            .get_or_init(|| {
                IndexedCorpus::new(self.fingerprints().iter().map(|fp| fp.path.clone()))
            })
            .clone()
    }
}

impl Storage {
    pub(crate) const fn new(
        root: PathBuf,
        files: IndexedFiles,
        gram_sets: GramSets,
        lexicon: Lexicon,
        postings: Postings,
        corpus_kind: CorpusKind,
    ) -> Self {
        Self {
            root,
            files,
            gram_sets,
            lexicon,
            postings,
            corpus_kind,
        }
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Index {
    pub const DEFAULT: Self = Self {
        width: GramWidth::TRIGRAM,
        norm: GramNorm::Identity,
        storage: None,
    };

    /// Case-folded width-5 index for selective case-insensitive narrowing.
    pub const ASCII_LOWER_5: Self = Self {
        width: GramWidth::new(5),
        norm: GramNorm::AsciiLower,
        storage: None,
    };

    #[must_use]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    #[must_use]
    pub const fn width(mut self, width: GramWidth) -> Self {
        self.width = width;
        self
    }

    #[must_use]
    pub const fn norm(mut self, norm: GramNorm) -> Self {
        self.norm = norm;
        self
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
    pub const fn kind(&self) -> &'static str {
        "ngram"
    }

    #[must_use]
    pub fn params(&self) -> serde_json::Value {
        match self.norm {
            GramNorm::Identity => serde_json::json!({ "width": self.width.get() }),
            GramNorm::AsciiLower => {
                serde_json::json!({ "width": self.width.get(), "norm": "ascii-lower" })
            }
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        match self.norm {
            GramNorm::Identity => format!("{}-{}", self.kind(), self.width.get()),
            GramNorm::AsciiLower => {
                format!("{}-{}-ascii-lower", self.kind(), self.width.get())
            }
        }
    }

    #[must_use]
    pub const fn artifact_names(&self) -> &'static [&'static str] {
        &[
            crate::FILES_BIN,
            crate::LEXICON_BIN,
            crate::POSTINGS_BIN,
            crate::GRAMS_BIN,
        ]
    }

    /// Parse an N-gram catalog name.
    ///
    /// Accepts `ngram-N`, `ngram:N`, and `ngram-N-ascii-lower`.
    ///
    /// # Errors
    ///
    /// Returns an error if `value` is not a known catalog name, or if `N`
    /// is not a valid width.
    pub fn parse_name(value: &str) -> Result<Self, String> {
        let rest = value
            .strip_prefix("ngram-")
            .or_else(|| value.strip_prefix("ngram:"))
            .ok_or_else(|| format!("unknown index: {value}"))?;
        let (width_str, norm) = rest
            .strip_suffix("-ascii-lower")
            .map_or((rest, GramNorm::Identity), |w| (w, GramNorm::AsciiLower));
        let width = width_str
            .parse::<u8>()
            .map_err(|_| format!("invalid ngram width: {width_str}"))?;
        Ok(Self::new().width(GramWidth::new(width)).norm(norm))
    }

    /// Parse persisted params for registry/config reconstruction.
    ///
    /// # Errors
    ///
    /// Returns an error if params are not a width object or bare number.
    pub fn from_params(params: &serde_json::Value) -> crate::Result<Self> {
        let width = if let Some(width) = params.as_u64() {
            width
        } else if let Some(width) = params.get("width").and_then(serde_json::Value::as_u64) {
            width
        } else {
            return Err(crate::Error::Index(
                crate::index::IndexError::UnknownIndexConfig(format!(
                    "invalid ngram params: {params}"
                )),
            ));
        };
        let width = u8::try_from(width).map_err(|_| {
            crate::Error::Index(crate::index::IndexError::UnknownIndexConfig(format!(
                "invalid ngram width: {width}"
            )))
        })?;
        let norm = match params.get("norm") {
            None if params.as_u64().is_some() => GramNorm::Identity,
            None => GramNorm::Identity,
            Some(value) => {
                let raw = value.as_str().ok_or_else(|| {
                    crate::Error::Index(crate::index::IndexError::UnknownIndexConfig(format!(
                        "invalid ngram norm: {value}"
                    )))
                })?;
                raw.parse::<GramNorm>().map_err(|e| {
                    crate::Error::Index(crate::index::IndexError::UnknownIndexConfig(e))
                })?
            }
        };
        Ok(Self::new().width(GramWidth::new(width)).norm(norm))
    }

    pub(crate) const fn with_storage(&self, storage: Storage) -> Self {
        Self {
            width: self.width,
            norm: self.norm,
            storage: Some(storage),
        }
    }

    pub(crate) const fn storage(&self) -> Option<&Storage> {
        self.storage.as_ref()
    }

    #[must_use]
    pub fn file_path(&self, id: FileId) -> Option<&Path> {
        self.storage()?.files.get(id).map(|fp| fp.path.as_path())
    }

    #[must_use]
    pub fn file_abs_path(&self, id: FileId) -> Option<PathBuf> {
        let storage = self.storage()?;
        storage.files.get(id).map(|fp| storage.root.join(&fp.path))
    }

    /// Corpus root of an opened index.
    ///
    /// # Panics
    ///
    /// Panics if this index has not been opened (has no storage).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.storage.as_ref().expect("opened ngram index").root
    }

    /// Corpus kind of an opened index.
    ///
    /// # Panics
    ///
    /// Panics if this index has not been opened (has no storage).
    #[must_use]
    pub const fn corpus_kind(&self) -> CorpusKind {
        self.storage
            .as_ref()
            .expect("opened ngram index")
            .corpus_kind
    }

    /// Resolve candidate file ids for the query. Falls back to every indexed
    /// file when the query cannot be narrowed.
    ///
    /// `AsciiLower` indexes only narrow case-insensitive queries (Exact on
    /// folded grams). Case-sensitive queries cannot be proven from a folded
    /// lexicon, so they return every covered id.
    #[must_use]
    pub(crate) fn query_file_ids(&self, query: &SearchQuery) -> Vec<FileId> {
        let Some(storage) = self.storage() else {
            return Vec::new();
        };
        let all_ids = || {
            (0..storage.files.len())
                .map(FileId::new)
                .collect::<Vec<_>>()
        };
        let Some(arms) = self.extract_literal_arms(query) else {
            return all_ids();
        };
        let (arms, gram_match) = match (self.norm, query.case()) {
            (GramNorm::Identity, Case::Insensitive) => (arms, GramMatch::AsciiCase),
            (GramNorm::Identity, Case::Sensitive) => (arms, GramMatch::Exact),
            (GramNorm::AsciiLower, Case::Sensitive) => return all_ids(),
            (GramNorm::AsciiLower, Case::Insensitive) => {
                let folded = arms
                    .into_iter()
                    .map(|arm| GramNorm::AsciiLower.normalize_literal(&arm))
                    .collect();
                (folded, GramMatch::Exact)
            }
        };
        let ids = self.candidate_file_ids(&arms, gram_match);
        ids.into_iter()
            .filter_map(|id| usize::try_from(id).ok().map(FileId::new))
            .collect()
    }

    /// Returns an explanation of how a query would be handled.
    #[must_use]
    pub fn explain(&self, query: &SearchQuery) -> crate::index::QueryPlanOutput {
        let mode = match self.extract_literal_arms(query) {
            Some(_) => crate::index::PlanMode::IndexedCandidates,
            None => crate::index::PlanMode::FullScan,
        };
        crate::index::QueryPlanOutput {
            pattern: query.patterns().join("|"),
            mode,
        }
    }

    #[must_use]
    pub(crate) fn all_file_ids(&self) -> Vec<FileId> {
        let Some(storage) = self.storage() else {
            return Vec::new();
        };
        (0..storage.files.len()).map(FileId::new).collect()
    }

    #[must_use]
    pub fn candidate(&self, id: FileId) -> Option<Candidate> {
        let storage = self.storage()?;
        let row = storage.files.row(id)?;
        let rel = PathBuf::from(row.path);
        let abs = storage.root.join(&rel);
        Some(Candidate::with_metadata(rel, abs, Some(row.size), None))
    }

    #[must_use]
    pub(crate) fn coverage(&self) -> IndexedCorpus {
        self.storage().map_or_else(
            || IndexedCorpus::new([]),
            |storage| storage.files.coverage(),
        )
    }

    pub(crate) fn merge_partial_fingerprints(
        existing: &[FileFingerprint],
        root: &Path,
        paths: &[PathBuf],
    ) -> crate::Result<Vec<FileFingerprint>> {
        use std::collections::HashMap;

        let mut by_path: HashMap<PathBuf, FileFingerprint> = existing
            .iter()
            .map(|fp| (fp.path.clone(), fp.clone()))
            .collect();
        for rel in paths {
            let abs = root.join(rel);
            let meta = std::fs::metadata(&abs).map_err(crate::Error::Io)?;
            let mtime_secs = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
            let fp = FileFingerprint {
                path: rel.clone(),
                mtime_secs,
                size: meta.len(),
            };
            by_path.insert(rel.clone(), fp);
        }
        let mut merged: Vec<_> = by_path.into_values().collect();
        merged.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(merged)
    }

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

    pub(crate) fn validate_file_paths(
        fingerprints: &[FileFingerprint],
    ) -> Result<(), NGramIndexError> {
        for fp in fingerprints {
            if fp.path.as_os_str().is_empty()
                || fp.path.is_absolute()
                || fp
                    .path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid file path in index: {}", fp.path.display()),
                )));
            }
        }
        Ok(())
    }
}
