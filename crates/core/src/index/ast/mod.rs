//! AST index kind: a per-file language map and node-kind postings over the
//! shared snapshot [`Files`](crate::index::Files) id space.
//!
//! Narrowing model (structural queries land with the `--ast` search surface):
//! a structural match requires a file whose language the pattern parses in
//! and whose parse tree contains the pattern's root node kinds. Both signals
//! are necessary conditions, so this kind may over-return candidates but
//! never under-returns; queries it cannot narrow receive every shared id.
//!
//! Artifacts, one namespace directory per snapshot (`ast/`):
//!
//! | File | Purpose |
//! |---|---|
//! | `kinds.bin` | language table + per-file language map + kind directory |
//! | `postings.bin` | shared [`Postings`](crate::index::postings::Postings) payload addressed by the directory |

mod build;
mod index;
mod language;
mod pattern;
mod storage;

pub use index::{AstIndexError, Index};
pub use language::AstLanguage;
pub use pattern::AstPattern;
