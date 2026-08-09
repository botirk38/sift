pub mod output;
pub mod plan;
pub mod scan;
pub mod scope;

pub use output::Candidates;
pub(crate) use output::Inner as CandidatesInner;
pub use plan::Plan;
pub use scan::Scan;
pub use scope::{Coverage, ScanScope, SnapshotFreshness};
