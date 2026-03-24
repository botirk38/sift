//! Indexed search execution built on ripgrep's public grep crates.

mod execute;
mod matcher;
mod types;

pub use execute::{parallel_candidate_min_files, walk_file_paths};
pub use types::{
    CaseMode, CompiledSearch, Match, SearchMatchFlags, SearchMode, SearchOptions, SearchOutput,
};
