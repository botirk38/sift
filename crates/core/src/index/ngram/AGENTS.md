# AGENTS.md -- index/ngram/

N-gram index implementation: corpus walk, runtime-width gram extraction, index
building, file table, and persistence. [`Index`](index.rs) is always opened
(required `Storage`); catalog knobs live on `IndexRecord`. Implements
`crate::index::Index` (`query` / `coverage` / `all_file_ids` / `update`).

## Key Types

- `Index`: opened only; `Index::build(width, norm, dest, config)` /
  `Index::open(width, norm, dir, root, kind)`
- `IndexRecord::Ngram { width, norm }`: catalog entry; `build` / `open` dispatch here
- `GramWidth`, `GramNorm`, `Gram`, `GramWindows`: runtime-width gram domain primitives
- `GramNorm::AsciiLower`: fold ASCII letters at index time; query-time Exact on folded literals for `-i` only
- `IndexTables`: table builder output; reuses cached grams for unchanged files
- `FileFingerprint`: per-file change detection data (path, mtime, size)

## Conventions

- File paths are always relative to the corpus root.
- Filesystem discovery uses `walk::FileWalk`; do not add N-gram-specific walk visitors.
- Gram extraction is parallelized via Rayon.
- Catalog vs opened: knobs on `IndexRecord`; runtime on `ngram::Index`. No
  knobs-only handle, no `to_record` adapter.
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
- Reintroduce `Index::new()` / knobs-only builders or optional storage.
