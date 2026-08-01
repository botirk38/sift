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
                                 Grep::search
```

| Type | Role |
|------|------|
| `Index` | Kind contract (`build` / `open` / `query` / `candidate` / `update`) |
| `IndexRecord` | Persisted `{ kind, params }` |
| `IndexConfig` | Corpus/walk/visibility for a write |
| `Indexes` | Lifecycle + search |
| `Grep` | Resolve candidates + run search |

## Modules

| Module | Description |
|--------|-------------|
| [`index/`](src/index/) | Contract, snapshot, orchestrator |
| [`index/ngram/`](src/index/ngram/) | N-gram index implementation |
| [`grep/`](src/grep/) | Public search API |
| [`candidates/`](src/candidates/) | Planning and resolution |

## Search API

```rust
use sift_core::{
    CandidateSource, Grep, GrepRequest, Indexes, IndexNarrowing, Inputs, InputConversion,
    PathDisplay, ScanScope, SnapshotFreshness, SearchMode, SearchOptions, SearchQuery, StatsMode,
    StoreMeta,
};

let indexes = Indexes::open(&sift_dir, &meta)?;
let source = CandidateSource::new(
    &indexes,
    &filter,
    Some(indexes.meta()),
    ScanScope::Index {
        order: Default::default(),
        freshness: SnapshotFreshness::Current,
    },
    IndexNarrowing::Allowed,
);

let grep = Grep::new(source);
let request = GrepRequest {
    query: SearchQuery::new(vec!["pattern".into()])?.options(SearchOptions::default()),
    streams: Inputs::empty(),
    conversion: InputConversion::new(&[], PathDisplay::Relative, None),
    mode: SearchMode::Lines,
    stats: StatsMode::Off,
};

let report = grep.search(request)?;
```

Formatting lives in `sift-grep`.

## Testing

```bash
cargo test -p sift-core
cargo bench -p sift-core --bench index
cargo bench -p sift-core --bench grep
cargo bench -p sift-core --bench candidates
```
