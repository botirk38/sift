mod bytes;
pub mod error;
pub mod event;
mod hit;
pub mod input;
mod line;
mod matcher;
pub mod mode;
pub mod options;
pub mod query;
pub mod report;
mod scan;
mod searcher;
pub mod stats;

pub use error::Error;
pub use event::{BinaryEvent, ContextEvent, ContextKind, MatchEvent, SearchEvent};
pub use hit::{Count, Listing, Match, MatchedFile};
pub use input::{Input, Inputs, Mention, Origin};
pub use mode::{Hit, SearchMode, ZeroCounts};
pub use options::{
    BinaryMode, Case, CaseMode, InputEncoding, Io, Narrowing, RegexEngine, SearchBound,
    SearchFlags, SearchOptions,
};
pub use query::Query;
pub use report::SearchReport;
pub use searcher::{Events, Searcher};
pub use stats::Stats;
