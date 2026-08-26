# index/ngram/storage/

On-disk binary format for N-gram index tables. All access is mmap-only and zero-copy.

## Modules

| File | Description |
|------|-------------|
| [`mod.rs`](mod.rs) | Module re-exports |
| [`format.rs`](format.rs) | Magic bytes (`SIFTLEX2`, `SIFTPST3`) |
| [`lexicon.rs`](lexicon.rs) | `LexiconEntry`, `Lexicon`: sorted gram to postings slice descriptor |
| [`postings.rs`](postings.rs) | `Postings`: encoded sorted file-ID lists |

## Format Overview

Each table file starts with an 8-byte magic header:

| File | Magic | Contents |
|------|-------|----------|
| `lexicon.bin` | `SIFTLEX2` | Width-aware sorted gram entries with postings offsets |
| `postings.bin` (container in `index/postings.rs`) | `SIFTPST3` | Encoded sorted file-ID posting lists referenced by lexicon |

All integers are little-endian. Lexicon entries are sorted by gram ordinal for
binary search. Width-bearing files store the gram width in the header and reject
mismatched widths at open time.

The shared snapshot-root `files.bin` is owned by `index/files.rs`, uses
`SIFTFIL2`, and is also mmap-only. This module has no `grams.bin`, `GramSet`,
or incremental-update artifacts.
