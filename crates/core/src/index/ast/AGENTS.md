# AGENTS.md -- index/ast/

## Responsibility

AST index kind: per-file language map and node-kind postings over the shared
snapshot `Files` id space. Pattern compilation and matching semantics come
from `ast-grep-core`; grammars come from the pinned `tree-sitter-*` crates.

## Conventions

- `AstLanguage` codes are persisted in `kinds.bin` and must never be
  renumbered; new languages append new codes.
- Per-language settings (extensions, expando char, `pre_process_pattern`) are
  vendored from `ast-grep-language` 0.45.0 and must be re-diffed against
  upstream whenever the `ast-grep-core` pin moves.
- Every `ast-grep-core` type stays confined to this module behind sift-owned
  wrappers (`AstLanguage`, `AstPattern`); nothing outside `index/ast/` may
  name an ast-grep type.
- A kind writes only `kinds.bin` and `postings.bin` in its own snapshot
  namespace. Do not add `files.bin` or incremental-update state.
- `postings.bin` reuses the shared `Postings` container from
  `index/postings.rs` — no second codec.
- Queries may over-return but must never under-return. "Cannot narrow" means
  returning every shared file id, ascending. Non-UTF-8 files carry no
  language because verification refuses them identically.
- A grammar-fingerprint mismatch marks the language stale (no narrowing), it
  is never an open error: `sift index update` reopens the store to rebuild it.
- `Index` is opened only; knobs, if any ever exist, live on `IndexRecord`.

## Do NOT

- Depend on `ast-grep-language` (it cannot be scoped to this language set).
- Persist parse trees or tree-sitter kind names (only ids + fingerprints).
- Reintroduce `grams.bin`-style update caches.
- Add `#[allow]`; `unsafe` stays in `index/mmap.rs` (`mmap_open` only).
