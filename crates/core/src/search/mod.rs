pub mod event;
mod hit;
pub mod input;
mod matcher;
pub mod mode;
pub mod options;
pub mod query;
pub mod report;
mod searcher;
pub mod stats;
mod task;

pub use event::{
    BinaryEvent, ContextEvent, ContextKind, Events, FileEvent, MatchEvent, SearchEvent, SearchSink,
};
pub use hit::{LineCount, ListedFile, Listing, Match, MatchedFile, SpanCount};
pub use input::{
    CandidateTransform, HitPath, Input, InputConversion, InputIdentity, Inputs, SearchInputs,
};
pub use mode::{SearchMode, ZeroCounts};
pub use options::{
    BinaryMode, Case, CaseMode, InputEncoding, Narrowing, RegexEngine, SearchBound, SearchFlags,
    SearchOptions,
};
pub use query::{SearchQuery, SearchQueryBuilder};
pub use report::Report;
pub use searcher::Searcher;
pub use stats::{MatchTotals, Stats, StatsMode};
