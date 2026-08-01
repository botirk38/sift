# index/ngram/

Runtime-width N-gram index implementation. The default configured index is `ngram-3`, which maps overlapping 3-byte sequences to candidate files, but the implementation is width-aware and can build/open other configured widths.

## How It Works

An N-gram index is an inverted index mapping each fixed-width byte sequence found in the corpus to the set of files that contain it. At query time, the planner extracts required literal sequences from the regex pattern, looks up their grams in the index, and intersects the resulting file sets to produce a narrow candidate list. Only those candidate files are scanned with the full regex engine.

## Modules

| File | Description |
|------|-------------|
| [`index.rs`](index.rs) | Opened `Index` (required storage); build/open/update |
| [`gram.rs`](gram.rs) | `GramWidth`, `Gram`, runtime-width gram window iteration |
| [`build.rs`](build.rs) | `IndexTables`: shared `FileWalk` usage, gram extraction, incremental table construction |
| [`files.rs`](files.rs) | File ID to relative path + fingerprint mapping |
| [`storage/`](storage/) | Binary persistence format (lexicon, postings, gram sets, file table) |

## On-Disk Format

Each table file starts with an 8-byte magic header:

| File | Magic | Contents |
|------|-------|----------|
| `files.bin` | `SIFTFIL1` | Offset table + length-prefixed UTF-8 paths with fingerprints (mtime, size) |
| `lexicon.bin` | `SIFTLEX2` | Width-aware sorted gram entries with postings offsets |
| `postings.bin` | `SIFTPST1` | Flat array of `u32` file IDs referenced by lexicon |
| `grams.bin` | `SIFTGRM1` | Per-file sorted unique gram sets for incremental rebuild |

All integers are little-endian. Width-bearing files reject mismatched gram widths at open time.

## API

```rust
use sift_core::{
    GramNorm, GramWidth, IndexDestination, IndexRecord, Indexes, NGramIndex,
};

// Preferred: build through Indexes + IndexRecord.
indexes.build(&[IndexRecord::ngram(GramWidth::TRIGRAM)], &config)?;

// Lower-level directory write (tests/benches).
NGramIndex::build(
    GramWidth::TRIGRAM,
    GramNorm::Identity,
    IndexDestination::Directory(&index_dir),
    &config,
)?;
let reopened = NGramIndex::open(
    GramWidth::TRIGRAM,
    GramNorm::Identity,
    &index_dir,
    &root,
    corpus_kind,
)?;
```
