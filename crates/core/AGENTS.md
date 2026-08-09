# AGENTS.md -- sift-core

## Responsibility

Composable indexed code search: index lifecycle, candidate planning, and grep-style matching.

## Architecture

```
Indexes::open (lifecycle) / Indexes::load → Option (search)
Plan::new (pure) → Plan::resolve (query I/O) → Searcher::execute
```

- `Indexes` — build/update/publish and query/hydrate over one store (`load` is `None` when absent)
- `Snapshot` — shared `Files` + opened `Box<dyn Index>` vec for a committed snapshot
- `Plan` — pure discovery decision; `resolve` owns index query I/O
- `Searcher` — match execution over resolved candidates and streams
- `Query` — patterns + options; owns narrowing policy
- `File` / `Origin` — path identity (`Origin::{File, Stream { label }}`)

Today the default index is `ngram::Index` opened from an `IndexRecord` (trigram width).

## Public API

Search (re-exported from `lib.rs`):

- `Query`, `Searcher`, `Report`, `Origin`, `SearchMode`
- `Indexes`, `IndexedCorpus`, `SnapshotId`, `Files`
- `Index`, `IndexRecord`, `IndexConfig`, `IndexDestination`, `ngram::Index`, `GramWidth`
- `Candidates`, `Plan`, `Scan`, `ScanScope`, `SnapshotFreshness`, `Coverage`

## Source map

| Module | Responsibility |
|--------|----------------|
| `index/indexes.rs` | `Indexes` build/update + query/hydrate |
| `index/record.rs` | `IndexRecord`, opened `Index` |
| `index/files.rs` | Snapshot-owned `Files` |
| `index/snapshot/` | `Snapshot`, persistence |
| `index/ngram/` | N-gram implementation (artifact names live here) |
| `index/mmap.rs` | Sole `unsafe` in the crate (`mmap_open`) |
| `search/` | `Query`, `Searcher`, `Report`, events |
| `candidates/plan.rs` | `Plan` (plan + resolve) |
| `candidates/candidates.rs` | `Candidates` collection |
| `corpus/` | `File`, `FileFilter`, `FileOrder`, walk |

## Search flow

```text
Searcher::execute
  1. coverage   ← caller maps SearchMode → Coverage
  2. plan       ← Plan::new(source, query, coverage)
  3. candidates ← plan.resolve(source)
  4. search     ← Searcher::execute(...)
```

Planning is pure; `Plan::resolve` is the only candidate I/O boundary.

## Invariants

- Conservative narrowing: indexes may over-return, never under-return.
- Multi-index intersection in `Indexes::query`, not per-caller.
- No free helper functions — logic lives on the owning type.
- No callback/`FnOnce` APIs.
- No `unsafe` outside `index/mmap.rs`.

## Testing

```bash
cargo test -p sift-core
```

## Do NOT

- Break public API without updating CLI.
- Add `unsafe` outside `index/mmap.rs`.
- Put stdout formatting in core.
- Expose `Indexes::candidates` or test-only constructors.
- Mix planning with I/O.
- Reintroduce `Grep`, `IndexStore`, or `open_or_create`.
