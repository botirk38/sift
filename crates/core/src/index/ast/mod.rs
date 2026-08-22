//! AST index kind: tree-sitter parsing and ast-grep pattern matching.
//!
//! This module owns every `ast-grep-core` and `tree-sitter` type in the
//! crate. [`AstLanguage`] is the language registry (a sift-owned enum with
//! persisted numeric codes) and [`AstPattern`] is the compiled-pattern
//! wrapper, so a future in-house matcher is a single-module replacement.
//!
//! The on-disk kind that consumes these lands in a follow-up.

mod language;
mod pattern;

pub use language::AstLanguage;
pub use pattern::AstPattern;
