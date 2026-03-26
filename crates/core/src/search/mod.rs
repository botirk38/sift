//! Indexed search execution built on ripgrep's public grep crates.

mod execute;
mod filter;
mod matcher;
mod types;

pub use execute::{parallel_candidate_min_files, walk_file_paths};
pub use filter::{
    CandidateInfo, GlobConfig, HiddenMode, IgnoreConfig, IgnoreSources, SearchFilter,
    SearchFilterConfig, VisibilityConfig,
};
pub use types::{
    CaseMode, CompiledSearch, FilenameMode, Match, OutputEmission, SearchMatchFlags, SearchMode,
    SearchOptions, SearchOutput,
};
