# index/

Uniform index kinds, snapshot persistence, and search orchestration.

## Layers

| Layer | Types | Role |
|-------|-------|------|
| Record | `IndexRecord`, opened `Index` | Catalog knobs + opened query/update |
| Orchestrator | `Indexes`, `StoreMeta` | `open` / `build` / `update` + query/hydrate |
| Snapshot | `Snapshot`, `Files`, `SnapshotId` | Shared file table + opened indexes |
| Kind | `ngram::Index` | First shipped impl |

```
index/
  record.rs    -- IndexRecord, Index trait
  indexes.rs   -- Indexes: build/update + query/hydrate
  files.rs     -- Snapshot-owned FileId → File map
  snapshot/    -- atomic persistence, leases, manifests
  ngram/       -- runtime-width N-gram index (default width 3)
```

## Modules

| Module | Description |
|--------|-------------|
| [`record.rs`](record.rs) | `IndexRecord`, opened `Index` |
| [`indexes.rs`](indexes.rs) | `Indexes` orchestrator |
| [`files.rs`](files.rs) | Snapshot `Files` |
| [`snapshot/`](snapshot/) | Snapshot persistence |
| [`kinds.rs`](kinds.rs) | `FileId` |
| [`paths.rs`](paths.rs) | `IndexedCorpus` |
| [`config.rs`](config.rs) | `IndexConfig` (corpus write inputs), `CorpusSpec` |
| [`meta.rs`](meta.rs) | `StoreMeta` |
| [`artifacts.rs`](artifacts.rs) | `IndexDestination` |
| [`ngram/`](ngram/) | N-gram implementation |

## API

```rust
use sift_core::{GramWidth, IndexRecord, Indexes, StoreMeta};

let mut indexes = Indexes::open(&sift_dir, &meta)?;
let catalog = [IndexRecord::ngram(GramWidth::TRIGRAM)];
indexes.build(&catalog, &config)?;
```

Resolve candidates through `Plan::resolve`.

## Adding a New Index Kind

1. Add a typed arm on `IndexRecord` with `build` / `open`.
2. Implement opened `Index` (`query` / `coverage` / `all_file_ids` / `update`).
3. Add a sibling module under `index/`.
