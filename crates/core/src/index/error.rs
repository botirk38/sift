use std::path::PathBuf;

use super::ngram::NGramIndexError;

/// Errors specific to the index layer.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("invalid index layout: {path}")]
    InvalidLayout { path: PathBuf },

    #[error(transparent)]
    NGram(#[from] NGramIndexError),

    #[error("IO error inspecting index path {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("unsupported store version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },

    #[error("invalid snapshot manifest at {path}: {source}")]
    InvalidManifest {
        path: PathBuf,
        source: serde_json::Error,
    },
}
