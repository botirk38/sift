# AGENTS.md -- sift-core

## Responsibility

Composable indexed code search: index lifecycle, candidate planning, and grep-style matching.

## Architecture

```
Indexes::open (lifecycle) / Indexes::load → Option (search)
Plan::new (pure) → Plan::resolve (query I/O) → Searcher::execute
```

- `Indexes` — builds, publishes, queries, and hydrates one store (`load` is `None` when absent)
- `Files` — shared `FileId → File` table for the current committed snapshot
- `Plan` — pure discovery decision; `resolve` owns index query I/O
- `Searcher` — match execution over resolved candidates and streams
- `Query` — patterns + options; owns narrowing policy
- `File` / `Origin` — path identity (`Origin::{File, Stream { label }}`)

Today the default catalog record is N-gram width 3. `record.rs` privately opens
kinds through its `Kind` enum.

## Public API

Search (re-exported from `lib.rs`):

- `Query`, `Searcher`, `SearchReport`, `Origin`, `SearchMode`
- `StoreMeta`, `IndexRecord`, `Indexes`, `SnapshotId`, `Files`
- `ngram::Index`, `GramWidth`, `GramNorm`
- `Candidates`, `Plan`, `Scan`, `ScanScope`, `SnapshotFreshness`, `Coverage`

## Source map

| Module | Responsibility |
|--------|----------------|
| `index/indexes.rs` | `Indexes` open/load/build + query/hydrate |
| `index/record.rs` | `IndexRecord`, private `Kind` dispatch |
| `index/files.rs` | Snapshot-owned `Files` |
| `index/disk.rs` | Snapshot persistence |
| `index/ngram/` | N-gram implementation (artifact names live here) |
| `index/mmap.rs` | Sole `unsafe` in the crate (`mmap_open`) |
| `search/` | `Query`, `Searcher`, `SearchReport`, events |
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
- Multi-kind intersection happens in `Indexes::query`, not per-caller.
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
