# sift-core

Core engine for indexed code search. Build on-disk indexes over a codebase, then
run regex or fixed-string queries with automatic candidate narrowing.

## Index architecture

Every kind implements the `Index` trait; `Indexes` builds/updates snapshots and
intersects `query` results at search time.

```
IndexRecord / Box<dyn Index> ──Indexes::build──> snapshot on disk
                                      │
                              Indexes::open
                                      │
                                 Indexes
                                      │
                Plan::new → Plan::resolve
                                      │
                            Searcher::execute
```

| Type | Role |
|------|------|
| `Index` | Opened kind (`query` / `coverage` / `all_file_ids` / `update`) |
| `IndexRecord` | Typed catalog knobs (`build` / `open`) |
| `Files` | Snapshot-owned `FileId → File` map |
| `IndexConfig` | Corpus/walk/visibility for a write |
| `Indexes` | Build/update + query/hydrate |
| `Query` / `Searcher` | Patterns + execute |
| `Plan` / `Candidates` | Pure plan then resolve |

## Modules

| Module | Description |
|--------|-------------|
| [`index/`](src/index/) | Record, Files, Snapshot, Indexes |
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
