# index/ngram/

Runtime-width N-gram index implementation. The default configured index is `ngram-3`, which maps overlapping 3-byte sequences to candidate files, but the implementation is width-aware and can build/open other configured widths.

## How It Works

An N-gram index is an inverted index mapping each fixed-width byte sequence found in the corpus to the set of files that contain it. At query time, the planner extracts required literal sequences from the regex pattern, looks up their grams in the index, and intersects the resulting file sets to produce a narrow candidate list. Only those candidate files are scanned with the full regex engine.

## Modules

| File | Description |
|------|-------------|
| [`index.rs`](index.rs) | Opened N-gram kind; build/open |
| [`gram.rs`](gram.rs) | `GramWidth`, `Gram`, runtime-width gram window iteration |
| [`build.rs`](build.rs) | `IndexTables`: gram extraction and postings construction |
| [`storage/`](storage/) | Binary persistence format (lexicon and postings) |

## On-Disk Format

`Files` is shared by every kind and is written once at the snapshot root:

| File | Magic | Contents |
|------|-------|----------|
| `files.bin` | `SIFTFIL2` | Offset table + length-prefixed UTF-8 relative paths and file sizes |

This N-gram kind writes only its namespace artifacts:

| File | Magic | Contents |
|------|-------|----------|
| `lexicon.bin` | `SIFTLEX2` | Width-aware sorted gram entries with postings offsets |
| `postings.bin` | `SIFTPST3` | Encoded sorted file-ID posting lists referenced by the lexicon |

All integers are little-endian. Width-bearing files reject mismatched gram widths at open time.

## API

```rust
use sift_core::Indexes;

let mut indexes = Indexes::open(&sift_dir, &meta)?;
indexes.build()?;
```

`IndexRecord` invokes `ngram::Index::build(width, norm, dir, &Files)` for its
namespace. At search time the parent layer privately opens the kind with
`Index::open(width, norm, dir, file_count)`.
