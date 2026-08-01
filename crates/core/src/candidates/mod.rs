pub mod output;
pub mod plan;
pub mod scope;
pub mod source;

pub use output::Candidates;
pub(crate) use output::Inner as CandidatesInner;
pub use plan::Plan;
pub use scope::{Coverage, ScanScope, SnapshotFreshness};
pub use source::CandidateSource;
