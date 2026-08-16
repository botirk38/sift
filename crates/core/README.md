# sift-core

Core engine for indexed code search. Build on-disk indexes over a codebase, then
run regex or fixed-string queries with automatic candidate narrowing.

## Index architecture

`StoreMeta` defines the corpus and catalog. `Indexes::build()` walks that
corpus, writes one shared `files.bin`, builds each `IndexRecord` beneath its
snapshot namespace, and atomically publishes `CURRENT`. Query dispatch uses the
private `Kind` enum in `record.rs`; `Indexes` intersects kind results.

The current store format is version 2. Each committed snapshot has
`files.bin` (`SIFTFIL2`) and `manifest.json` at its root; each kind writes only
under `snapshots/<id>/<kind-name>/`.

```
StoreMeta + IndexRecord ──Indexes::build──> snapshot on disk
                                        │
                                Indexes::open/load
                                        │
                                    Indexes
                                        │
                  Plan::new → Plan::resolve
                                        │
                    Searcher::execute / stream
```

| Type | Role |
|------|------|
| `StoreMeta` | Persistent corpus, walk, filter, coverage, and catalog configuration |
| `IndexRecord` | Typed catalog entry; builds and privately opens a kind |
| `Files` | Snapshot-owned `FileId → File` map |
| `Indexes` | Open/load/build + query/hydrate |
| `Query` / `Searcher` | Patterns + `execute` / `stream` |
| `Bytes` / `Lines` | Resident bytes; `Lines` iterates `Line` |
| `FileReport` / `SearchReport` | Per-input result; listing + stats |
| `Io` | File read backend (`Sync` / `Mmap` / `Uring`; default `Mmap`) |
| `Plan` / `Candidates` | Pure plan then resolve |

## Modules

| Module | Description |
|--------|-------------|
| [`index/`](src/index/) | Metadata, records, files, disk snapshots, Indexes |
| [`index/ngram/`](src/index/ngram/) | N-gram index implementation |
| [`search/`](src/search/) | Query, Searcher, inputs, events |
| [`candidates/`](src/candidates/) | Planning and resolution |

## Search API

```rust
use sift_core::{
    Scan, Input, Inputs, Plan, Query, ScanScope, Hit, SearchMode, SearchOptions, Searcher,
    SnapshotFreshness,
};

let searcher = Searcher::new(Query::new(vec!["pattern".into()], SearchOptions::default())?)?;
let source = Scan::new(
    indexes.as_ref(),
    &filter,
    store_meta.as_ref(),
    ScanScope::Index {
        order: Default::default(),
        freshness: SnapshotFreshness::Current,
    },
);
let candidates = Plan::new(&source, searcher.query(), SearchMode::Print(Hit::Line).coverage())
    .resolve(&source)?;
let mut inputs = Inputs::with_capacity(candidates.bound());
for file in candidates.into_vec() {
    inputs.push(Input::from_file(file, &[]));
}
let report = searcher.execute(inputs, SearchMode::Print(Hit::Line))?;
```

Formatting lives in `sift-grep`.

## Testing

```bash
cargo test -p sift-core
cargo bench -p sift-core --bench index
```
