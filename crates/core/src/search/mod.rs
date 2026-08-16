pub mod error;
pub mod event;
mod haystack;
mod hit;
pub mod input;
mod line;
mod matcher;
pub mod mode;
pub mod options;
pub mod query;
pub mod report;
mod searcher;
pub mod stats;

pub use error::Error;
pub use event::{
    BinaryEvent, ContextEvent, ContextKind, Events, FileEvent, MatchEvent, SearchEvent, SearchSink,
};
pub use hit::{LineCount, ListedFile, Listing, Match, MatchedFile, SpanCount};
pub use input::{Input, Inputs, Origin};
pub use mode::{SearchMode, ZeroCounts};
pub use options::{
    BinaryMode, Case, CaseMode, InputEncoding, Io, Narrowing, RegexEngine, SearchBound,
    SearchFlags, SearchOptions,
};
pub use query::Query;
pub use report::SearchReport;
pub use searcher::Searcher;
pub use stats::{MatchTotals, Stats, StatsMode};
