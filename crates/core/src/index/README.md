# index/

Store metadata, snapshot persistence, and search orchestration.

## Layers

| Layer | Types | Role |
|-------|-------|------|
| Metadata | `StoreMeta`, `IndexRecord` | Corpus configuration and kind catalog |
| Orchestrator | `Indexes` | `open` / `load` / `build`, query, and hydration |
| Snapshot storage | `SnapshotId`, `Files` | Committed artifact layout and shared file IDs |
| Kind | `ngram::Index` | First shipped kind |

```
index/
  record.rs    -- IndexRecord and private Kind query dispatch
  indexes.rs   -- Indexes: open/load/build + query/hydrate
  files.rs     -- Snapshot-owned FileId → File map
  disk.rs      -- atomic persistence, leases, manifests
  postings.rs  -- posting-list container shared by kinds (SIFTPST3)
  ngram/       -- runtime-width N-gram index (default width 3)
```

## Modules

| Module | Description |
|--------|-------------|
| [`record.rs`](record.rs) | `IndexRecord`, private `Kind` |
| [`indexes.rs`](indexes.rs) | `Indexes` orchestrator |
| [`files.rs`](files.rs) | Snapshot `Files` |
| [`disk.rs`](disk.rs) | Snapshot persistence |
| [`kinds.rs`](kinds.rs) | `FileId` |
| [`meta.rs`](meta.rs) | `StoreMeta` |
| [`postings.rs`](postings.rs) | Posting-list container shared by kinds |
| [`ngram/`](ngram/) | N-gram implementation |

## API

```rust
use sift_core::{GramWidth, IndexRecord, Indexes, StoreMeta};

let mut indexes = Indexes::open(&sift_dir, &meta)?;
indexes.build()?;
```

`build()` walks the corpus configured in `StoreMeta`, writes shared
`files.bin` at the new snapshot root, builds every `IndexRecord` beneath its
own namespace, and atomically publishes `CURRENT`. `files.bin` uses
`SIFTFIL2`; the store format version is 2.

`Indexes::load` opens an existing store for search. Querying is dispatched
through the private `Kind` enum in `record.rs`; `Files` hydrates the returned
IDs. Resolve candidates through `Plan::resolve`.

## Adding a New Index Kind

1. Add a typed arm on `IndexRecord` with `build` / `open`.
2. Add a private `Kind` arm and its query dispatch.
3. Add a sibling module under `index/`.
