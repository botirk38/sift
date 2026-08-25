# AGENTS.md -- index/ast/

## Responsibility

Tree-sitter parsing and ast-grep pattern matching for the AST index kind.
This module owns every `ast-grep-core` and `tree-sitter` type in the crate.

## Conventions

- `AstLanguage` codes are persisted in kind artifacts and must never be
  renumbered; new languages append new codes.
- Per-language settings (extensions, expando char, `pre_process_pattern`) are
  vendored from `ast-grep-language` 0.45.0 and must be re-diffed against
  upstream whenever the `ast-grep-core` pin moves.
- Grammar crates are `=`-pinned: a grammar bump changes node-kind ids and
  therefore every persisted fingerprint, so it is a deliberate, reviewable
  event rather than a `cargo update` side effect.
- Nothing outside `index/ast/` may name an ast-grep or tree-sitter type.
- Extension matching is case-sensitive, matching ast-grep. Index and
  verification apply the same `AstLanguage::from_path` rule, so a file the
  index calls "no language" is one the matcher would also refuse.

## Do NOT

- Depend on `ast-grep-language` (it cannot be scoped to this language set).
- Persist tree-sitter kind names (only ids plus a grammar fingerprint).
- Add `#[allow]`; `unsafe` stays in `index/mmap.rs`.
