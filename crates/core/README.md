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
                              Searcher::execute
```

| Type | Role |
|------|------|
| `StoreMeta` | Persistent corpus, walk, filter, coverage, and catalog configuration |
| `IndexRecord` | Typed catalog entry; builds and privately opens a kind |
| `Files` | Snapshot-owned `FileId → File` map |
| `Indexes` | Open/load/build + query/hydrate |
| `Query` / `Searcher` | Patterns + execute |
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
    Scan, Events, Inputs, Plan, Query, ScanScope, SearchInputs, SearchMode,
    SearchOptions, Searcher, SnapshotFreshness, StatsMode,
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
let candidates = Plan::new(&source, searcher.query(), SearchMode::Lines.coverage())
    .resolve(&source)?;
let report = searcher.execute(
    SearchInputs {
        candidates,
        streams: Inputs::empty(),
        explicit: &[],
    },
    StatsMode::Off,
    SearchMode::Lines,
    Events::Discard,
)?;
```

Formatting lives in `sift-grep`.

## Testing

```bash
cargo test -p sift-core
cargo bench -p sift-core --bench index
```
