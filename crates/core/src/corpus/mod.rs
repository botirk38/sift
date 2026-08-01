//! Shared corpus foundation: candidates, filters, and filesystem walk.

pub mod candidate;
pub mod filter;
pub mod order;
pub mod walk;

pub use candidate::{Candidate, PathDisplay};
pub use filter::{
    CandidateFilter, CandidateFilterConfig, FilterAdmission, GlobConfig, HiddenMode, IgnoreConfig,
    IgnoreSources, TypeFilterRule, VisibilityConfig,
};
pub use order::{CandidateOrder, CandidateOrderDirection, CandidateOrderKey};
pub use walk::{AllFiles, FileWalk, WalkFile, WalkMetadata, WalkSelector};
