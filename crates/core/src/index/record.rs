//! Catalog [`IndexRecord`] and private runtime kinds.

use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::FileId;
use super::Files;
use super::ngram::{GramNorm, GramWidth};

use crate::search::Query;

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

    /// Create kind artifacts under `dir` over shared `files`.
    ///
    /// # Errors
    ///
    /// Returns an error if extraction or encoding fails.
    pub fn build(self, dir: &Path, files: &Files) -> crate::Result<()> {
        match self {
            Self::Ngram { width, norm } => super::ngram::Index::build(width, norm, dir, files),
        }
    }

    /// Load a previously persisted kind from its namespace directory.
    ///
    /// # Errors
    ///
    /// Returns an error if artifacts are missing or malformed.
    pub(crate) fn open(self, dir: &Path, file_count: usize) -> crate::Result<Kind> {
        match self {
            Self::Ngram { width, norm } => Ok(Kind::Ngram(super::ngram::Index::open(
                width, norm, dir, file_count,
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

/// Runtime index kind ready to query.
pub(crate) enum Kind {
    Ngram(super::ngram::Index),
}

impl Kind {
    pub(crate) fn query(&self, query: &Query) -> crate::Result<Vec<FileId>> {
        match self {
            Self::Ngram(index) => index.query(query),
        }
    }
}
