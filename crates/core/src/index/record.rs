//! Catalog [`IndexRecord`] and opened [`Index`].

use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::IndexDestination;
use super::config::{CorpusKind, IndexConfig};
use super::kinds::FileId;
use super::ngram::{GramNorm, GramWidth};
use super::paths::IndexedCorpus;

use crate::search::SearchQuery;

/// Persisted catalog entry (meta + snapshot manifest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IndexRecord {
    Ngram {
        width: GramWidth,
        #[serde(default)]
        norm: GramNorm,
    },
}

impl IndexRecord {
    /// N-gram catalog record (identity norm).
    #[must_use]
    pub const fn ngram(width: GramWidth) -> Self {
        Self::Ngram {
            width,
            norm: GramNorm::Identity,
        }
    }

    /// N-gram catalog record with an explicit norm.
    #[must_use]
    pub const fn ngram_norm(width: GramWidth, norm: GramNorm) -> Self {
        Self::Ngram { width, norm }
    }

    /// Default catalog shipped with the engine.
    #[must_use]
    pub fn default_catalog() -> Vec<Self> {
        vec![Self::ngram(GramWidth::TRIGRAM)]
    }

    /// Snapshot namespace / display name (`ngram-3`, `ngram-5-ascii-lower`).
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Ngram {
                width,
                norm: GramNorm::Identity,
            } => format!("ngram-{}", width.get()),
            Self::Ngram {
                width,
                norm: GramNorm::AsciiLower,
            } => format!("ngram-{}-ascii-lower", width.get()),
        }
    }

    /// Parse a short catalog name (`trigram`, `ngram-3`, `ngram:3`,
    /// `ngram-5-ascii-lower`).
    ///
    /// # Errors
    ///
    /// Returns an error if `value` is not a known catalog name.
    pub fn from_name(value: &str) -> Result<Self, String> {
        if value == "trigram" {
            return Ok(Self::ngram(GramWidth::TRIGRAM));
        }
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
        let width = GramWidth::try_new(width).map_err(str::to_string)?;
        Ok(Self::Ngram { width, norm })
    }

    /// Create artifacts at `dest` from a full corpus scan under `config`.
    ///
    /// # Errors
    ///
    /// Returns an error if walking, extraction, or encoding fails.
    pub fn build(self, dest: IndexDestination<'_>, config: &IndexConfig<'_>) -> crate::Result<()> {
        match self {
            Self::Ngram { width, norm } => super::ngram::Index::build(width, norm, dest, config),
        }
    }

    /// Load a previously persisted index from a namespace directory.
    ///
    /// # Errors
    ///
    /// Returns an error if artifacts are missing or malformed.
    pub fn open(
        self,
        dir: &Path,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Box<dyn Index>> {
        match self {
            Self::Ngram { width, norm } => Ok(Box::new(super::ngram::Index::open(
                width,
                norm,
                dir,
                root,
                corpus_kind,
            )?)),
        }
    }
}

impl FromStr for IndexRecord {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_name(value)
    }
}

/// Opened index: query and incremental update only.
pub trait Index: Send + Sync {
    /// File ids that may match. May over-return; must not under-return.
    /// Cannot narrow → every covered id.
    fn query(&self, query: &SearchQuery) -> Vec<FileId>;

    fn coverage(&self) -> IndexedCorpus;

    /// Enumerate every indexed file id known to this opened index.
    fn all_file_ids(&self) -> Vec<FileId>;

    /// Incremental rewrite into `dest`. `true` if artifacts were written.
    ///
    /// # Errors
    ///
    /// Returns an error if walking, extraction, or encoding fails.
    fn update(&self, dest: IndexDestination<'_>, config: &IndexConfig<'_>) -> crate::Result<bool>;
}
