pub mod plan;
pub mod scope;
pub mod source;

#[path = "candidates.rs"]
mod collection;

pub use collection::{Candidates, IndexedCandidates};
pub use scope::{ScanScope, SnapshotFreshness};
pub use source::CandidateSource;
