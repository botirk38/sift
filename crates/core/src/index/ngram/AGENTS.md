# AGENTS.md -- index/ngram/

Runtime-width N-gram kind implementation: gram extraction, kind-artifact
building, persistence, and candidate narrowing. Corpus walking and the shared
file table belong to the parent index layer.

## Key Types

- `Index`: opened N-gram kind; `build(width, norm, dir, &Files)` /
  `open(width, norm, dir, file_count)`
- `IndexRecord::Ngram { width, norm }`: catalog entry; private dispatch in
  `record.rs` builds and opens this kind
- `GramWidth`, `GramNorm`, `Gram`, `GramWindows`: runtime-width gram domain primitives
- `GramNorm::AsciiLower`: fold ASCII letters at index time; query-time Exact on folded literals for `-i` only
- `IndexTables`: table builder output

## Conventions

- `Files` owns the shared, snapshot-wide `FileId` space. This kind reads corpus
  bytes through it but does not persist paths or file metadata.
- Gram extraction is parallelized via Rayon.
- A kind writes only `lexicon.bin` and `postings.bin` in its own snapshot
  namespace. Do not add `files.bin`, `grams.bin`, or incremental-update state.
- `Index` is opened only. `Opened` is private to `record.rs`; there is no
  shared `Index` trait.
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
- Reintroduce knobs-only builders or optional storage.
