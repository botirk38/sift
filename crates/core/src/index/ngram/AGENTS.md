# AGENTS.md -- index/ngram/

N-gram index implementation: corpus walk, runtime-width gram extraction, index
building, file table, and persistence. A single [`Index`](index.rs) type holds
knobs (default width-3) and optional opened storage; it implements
`crate::index::Index`.

## Key Types

- `Index`: knobs (`width`, `norm`) + optional storage; `Index::new()` / `.width(...)` / `.norm(...)`
- `GramWidth`, `GramNorm`, `Gram`, `GramWindows`: runtime-width gram domain primitives
- `GramNorm::AsciiLower`: fold ASCII letters at index time; query-time Exact on folded literals for `-i` only
- `IndexTables`: table builder output; reuses cached grams for unchanged files
- `FileFingerprint`: per-file change detection data (path, mtime, size)

## Conventions

- File paths are always relative to the corpus root.
- Filesystem discovery uses `walk::FileWalk`; do not add N-gram-specific walk visitors.
- Gram extraction is parallelized via Rayon.
- Lifecycle goes through the `Index` trait (`build` / `open` / `update` with
  `IndexDestination` + config); no parallel `*_into` / `*_from` variants.
- Generic behavior is runtime-width. Do not add specialization layers until a
  measured hot path justifies them.
- The only `unsafe` in the index crate lives in `index/mmap.rs`.

## Do NOT

- Change the file-path sort order; it defines stable file IDs.
- Add new persistence files without updating N-gram persistence, storage docs,
  and snapshot tests.
- Add specialization logic before the general runtime-width implementation has
  a measured need.
- Add `unsafe` outside `index/mmap.rs`.
- Reintroduce a separate `Config` / builder type for knobs.
