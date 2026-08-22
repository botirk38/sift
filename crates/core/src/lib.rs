//! Composable indexed code search engine.
//!
//! `sift-core` builds on-disk indexes over codebases and uses them to narrow
//! candidate files before running the full regex engine.

pub mod candidates;
pub(crate) mod corpus;
pub mod index;
pub mod search;

pub use candidates::{Candidates, Coverage, Plan, Scan, ScanScope, SnapshotFreshness};
pub use corpus::{
    File, FileFilter, FileFilterConfig, FileOrder, FileOrderDirection, FileOrderKey, FileWalk,
    FilterAdmission, GlobConfig, HiddenMode, IgnoreConfig, IgnoreSources, PathDisplay,
    TypeFilterRule, VisibilityConfig, WalkFile, WalkMetadata,
};
pub use search::{
    BinaryEvent, BinaryMode, Case, CaseMode, ContextEvent, ContextKind, Count,
    Error as SearchError, Events, FileReport, Hit, Input, InputEncoding, Inputs, Io, Listing,
    Match, MatchEvent, MatchedFile, Mention, Narrowing, Origin, Query, RegexEngine, SearchBound,
    SearchEvent, SearchFlags, SearchMode, SearchOptions, SearchReport, Searcher, Span, Stats,
    ZeroCounts,
};

pub use index::ast::{AstLanguage, AstPattern};
pub use index::meta::StoreMeta;
pub use index::ngram::{
    Gram, GramNorm, GramWidth, GramWindows, Index as NGramIndex, NGramIndexError,
};
pub use index::{
    CorpusMeta, FileId, Files, FilterMeta, IndexCoverage, IndexError, IndexRecord, Indexes,
    SnapshotId, WalkMeta,
};

use thiserror::Error;

pub const SIFT_DIR: &str = ".sift";

/// Top-level umbrella error for all core operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Index(#[from] IndexError),

    #[error(transparent)]
    Search(#[from] search::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ignore walk error: {0}")]
    Ignore(#[from] ignore::Error),
}

impl From<crate::index::ngram::NGramIndexError> for Error {
    fn from(e: crate::index::ngram::NGramIndexError) -> Self {
        Self::Index(IndexError::NGram(e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
