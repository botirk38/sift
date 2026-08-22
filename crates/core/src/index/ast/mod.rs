//! AST index kind: tree-sitter parsing and ast-grep pattern matching.
//!
//! This module owns every `ast-grep-core` and `tree-sitter` type in the
//! crate. [`AstLanguage`] is the language registry (a sift-owned enum with
//! persisted numeric codes) and [`AstPattern`] is the compiled-pattern
//! wrapper, so a future in-house matcher is a single-module replacement.
//!
//! The kind persists two artifacts per snapshot namespace (`ast/`):
//!
//! | File | Purpose |
//! |---|---|
//! | `kinds.bin` | language table with grammar fingerprints, per-file language map, `(lang, kind)` directory |
//! | `postings.bin` | the shared posting container the directory addresses |
//!
//! Wiring it into the catalog as a queryable kind lands in a follow-up.

mod build;
mod index;
mod language;
mod pattern;
mod storage;

pub use index::{AstIndexError, Index};
pub use language::AstLanguage;
pub use pattern::AstPattern;
