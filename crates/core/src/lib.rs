//! Composable indexed code search engine.
//!
//! `sift-core` builds on-disk indexes over codebases and uses them to narrow
//! candidate files before running the full regex engine.

pub mod candidates;
pub(crate) mod corpus;
pub mod grep;
pub mod index;
pub mod search;

pub use candidates::{CandidateSource, Candidates, ScanScope, SnapshotFreshness};
pub use corpus::Candidate;
pub use grep::{
    ByteInput, CandidateFilter, CandidateFilterConfig, CandidateOrder, CandidateTransform,
    Error as GrepError, FilterAdmission, GlobConfig, Grep, GrepRequest, HiddenMode, IgnoreConfig,
    IgnoreSources, TypeFilterRule, VisibilityConfig,
};
pub use search::{
    BinaryEvent, BinaryMode, Case, CaseMode, ContextEvent, ContextKind, EventEmission, FileEvent,
    HitPath, Input, InputConversion, InputEncoding, InputIdentity, Inputs, LineCount, ListedFile,
    Listing, Match, MatchEvent, MatchTotals, MatchedFile, Narrowing, RegexEngine, Report,
    SearchBound, SearchEvent, SearchFlags, SearchInputs, SearchMode, SearchOptions, SearchQuery,
    SearchQueryBuilder, SearchSink, Searcher, SpanCount, Stats, StatsMode, ZeroCounts,
};

pub use ignore::{Walk, WalkBuilder};

pub use index::config::IndexWalkConfig;
pub use index::meta::StoreMeta;
pub use index::ngram::{
    Gram, GramNorm, GramWidth, GramWindows, Index as NGramIndex, NGramIndexError,
};
pub use index::{
    CorpusKind, CorpusMeta, CorpusSpec, FileId, FilterMeta, Index, IndexConfig, IndexCoverage,
    IndexError, IndexId, IndexRecord, IndexWrite, IndexedCorpus, Indexes, PlanMode,
    QueryPlanOutput, SnapshotId, WalkMeta,
};

use thiserror::Error;

pub const SIFT_DIR: &str = ".sift";
pub const FILES_BIN: &str = "files.bin";
pub const LEXICON_BIN: &str = "lexicon.bin";
pub const POSTINGS_BIN: &str = "postings.bin";
pub const GRAMS_BIN: &str = "grams.bin";

/// Top-level umbrella error for all core operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Index(#[from] IndexError),

    #[error(transparent)]
    Search(#[from] grep::Error),

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
