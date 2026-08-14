# AGENTS.md -- index/ngram/storage/

Binary persistence format for N-gram kind artifacts. Read/write `lexicon.bin`
and `postings.bin` with zero-copy memory-mapped access.

## Key Types

- `LexiconEntry`: gram + postings offset + length.
- `Lexicon`: memory-mapped lexicon with binary-search lookup.
- `Postings`: memory-mapped postings blob.

The shared `files.bin` belongs to `index/files.rs` at the snapshot root, not
this module. There is no `grams.bin`, `GramSet`, or incremental update format.

## Conventions

- All integers are little-endian.
- Each file starts with an 8-byte magic header for format identification.
- Width-bearing files persist and validate gram width.
- Lexicon entries are sorted by gram ordinal, enabling binary search.
- The only `unsafe` in the index crate lives in `index/mmap.rs` with a documented safety invariant.

## Do NOT

- Add `unsafe` without documenting the safety invariant in `index/mmap.rs`.
- Add backward-compatible reads for removed formats unless a concrete migration requirement exists.
