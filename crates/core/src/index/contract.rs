//! Uniform index kind contract: one [`Index`] trait for every kind.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::config::{CorpusKind, IndexConfig};
use super::kinds::FileId;
use super::paths::IndexedCorpus;
use super::{IndexDestination, IndexSource};

use crate::corpus::Candidate;
use crate::search::SearchQuery;

/// Serializable catalog entry for meta and snapshot manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRecord {
    pub kind: String,
    pub params: serde_json::Value,
}

impl IndexRecord {
    /// Build a catalog handle from a persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if `kind` is unknown or `params` are invalid.
    pub fn to_index(&self) -> crate::Result<Box<dyn Index>> {
        match self.kind.as_str() {
            "ngram" => Ok(Box::new(super::ngram::Index::from_params(&self.params)?)),
            other => Err(crate::Error::Index(super::IndexError::UnknownIndexConfig(
                other.to_string(),
            ))),
        }
    }

    /// Persisted display name (`kind-...` for compat with snapshot directories).
    #[must_use]
    pub fn name(&self) -> String {
        match self.kind.as_str() {
            "ngram" => super::ngram::Index::from_params(&self.params).map_or_else(
                |_| {
                    let width = self
                        .params
                        .get("width")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_else(|| self.params.as_u64().unwrap_or(0));
                    format!("ngram-{width}")
                },
                |index| index.name(),
            ),
            other => other.to_string(),
        }
    }

    /// Parse a short catalog name (`trigram`, `ngram-3`, `ngram:3`,
    /// `ngram-5-ascii-lower`).
    ///
    /// # Errors
    ///
    /// Returns an error if `value` is not a known catalog name.
    pub fn from_name(value: &str) -> Result<Self, String> {
        match value {
            "trigram" => Ok(Self::ngram(super::ngram::GramWidth::TRIGRAM)),
            other => {
                let index = super::ngram::Index::parse_name(other)?;
                Ok(index.to_record())
            }
        }
    }

    /// N-gram catalog record for the given width (identity norm).
    #[must_use]
    pub fn ngram(width: super::ngram::GramWidth) -> Self {
        super::ngram::Index::new().width(width).to_record()
    }

    /// Default catalog of records shipped with the engine.
    #[must_use]
    pub fn default_catalog() -> Vec<Self> {
        vec![Self::ngram(super::ngram::GramWidth::TRIGRAM)]
    }
}

impl FromStr for IndexRecord {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_name(value)
    }
}

/// Write artifacts from a corpus scan (create or refresh).
pub struct IndexWrite<'a> {
    pub dest: IndexDestination<'a>,
    pub config: &'a IndexConfig<'a>,
    pub paths: &'a [PathBuf],
}

/// Uniform interface for every index kind (catalog knobs and opened data).
pub trait Index: Send + Sync {
    fn kind(&self) -> &'static str;
    fn params(&self) -> serde_json::Value;
    fn name(&self) -> String;

    /// Persisted catalog entry describing this index's configuration.
    fn to_record(&self) -> IndexRecord {
        IndexRecord {
            kind: self.kind().to_string(),
            params: self.params(),
        }
    }

    /// Create artifacts at `write.dest`.
    ///
    /// # Errors
    ///
    /// Returns an error if walking, extraction, or encoding fails.
    fn build(&self, write: IndexWrite<'_>) -> crate::Result<()>;

    /// Load a previously persisted index.
    ///
    /// # Errors
    ///
    /// Returns an error if artifacts are missing or malformed.
    fn open(
        &self,
        source: IndexSource<'_>,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Box<dyn Index>>;

    /// File ids that may match. May over-return; must not under-return.
    /// Cannot narrow → every covered id.
    fn query(&self, query: &SearchQuery) -> Vec<FileId>;

    fn coverage(&self) -> IndexedCorpus;

    /// One indexed row as a [`Candidate`] (no filter). Missing id → `None`.
    fn candidate(&self, id: FileId) -> Option<Candidate>;

    /// Enumerate every indexed file id known to this opened index.
    fn all_file_ids(&self) -> Vec<FileId>;

    /// Incremental rewrite into `write.dest`. `true` if artifacts were written.
    ///
    /// # Errors
    ///
    /// Returns an error if walking, extraction, or encoding fails.
    fn update(&self, write: IndexWrite<'_>) -> crate::Result<bool>;
}
