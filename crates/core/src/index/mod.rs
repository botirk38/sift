//! Trigram index construction (walk, extract, assign file ids).

mod builder;
pub mod files;
pub mod trigram;

pub use builder::build_trigram_index;
