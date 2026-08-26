# AGENTS.md -- sift-core

## Responsibility

Composable indexed code search: index lifecycle, candidate planning, and grep-style matching.

## Architecture

```
Indexes::open (lifecycle) / Indexes::load → Option (search)
Plan::new (pure) → Plan::resolve (query I/O) → Searcher::execute / stream
```

- `Indexes` — builds, publishes, queries, and hydrates one store (`load` is `None` when absent)
- `Files` — shared `FileId → File` table for the current committed snapshot
- `Plan` — pure discovery decision; `resolve` owns index query I/O
- `Searcher` — `execute(inputs, mode)` materializes; `stream(inputs, mode)` returns `Events`; `FileScan` walks one input's bytes
- `Query` — patterns + options; owns narrowing policy
- `File` / `Origin` — path identity (`Origin::{File, Stream { label }}`)

Today the default catalog is identity and ascii-lower trigram. `record.rs`
privately opens kinds through its `Kind` enum.

## Public API

Search (re-exported from `lib.rs`):

- `Query`, `Searcher`, `SearchReport`, `Events`, `Origin`, `Mention`, `SearchMode`, `Hit`
- `StoreMeta`, `IndexRecord`, `Indexes`, `SnapshotId`, `Files`
- `ngram::Index`, `GramWidth`, `GramNorm`
- `AstLanguage`, `AstPattern`
- `Candidates`, `Plan`, `Scan`, `ScanScope`, `SnapshotFreshness`, `Coverage`

## Source map

| Module | Responsibility |
|--------|----------------|
| `index/indexes.rs` | `Indexes` open/load/build + query/hydrate |
| `index/record.rs` | `IndexRecord`, private `Kind` dispatch |
| `index/files.rs` | Snapshot-owned `Files` |
| `index/disk.rs` | Snapshot persistence |
| `index/postings.rs` | Posting-list container shared by kinds |
| `index/ngram/` | N-gram implementation (artifact names live here) |
| `index/ast/` | Tree-sitter languages and ast-grep patterns |
| `index/mmap.rs` | Sole `unsafe` in the crate (`mmap_open`) |
| `search/` | `Query`, `Searcher`, `FileScan`, `Bytes`, `SearchReport`, `Events` |
| `candidates/plan.rs` | `Plan` (plan + resolve) |
| `candidates/candidates.rs` | `Candidates` collection |
| `corpus/` | `File`, `FileFilter`, `FileOrder`, walk |

## Search flow

```text
Searcher::execute(inputs, mode)  or  Searcher::stream(inputs, mode)
  1. coverage   ← caller maps SearchMode → Coverage
  2. plan       ← Plan::new(source, query, coverage)
  3. candidates ← plan.resolve(source)
  4. search     ← execute (report) or stream (Events)
```

Planning is pure; `Plan::resolve` is the only candidate I/O boundary.

## Invariants

- Conservative narrowing: indexes may over-return, never under-return.
- Multi-kind intersection happens in `Indexes::query`, not per-caller.
- No free helper functions — logic lives on the owning type.
- No callback/`FnOnce` / sink APIs. Stream output is `Events` (`Iterator` + `into_report`).
- The search walk does not take an output method.
- No `unsafe` outside `index/mmap.rs`.

## Testing

```bash
cargo test -p sift-core
```

## Do NOT

- Change core search without updating CLI in the same change.
- Add `unsafe` outside `index/mmap.rs`.
- Put stdout formatting in core.
- Expose `Indexes::candidates` or test-only constructors.
- Mix planning with I/O.
- Reintroduce `Grep`, `IndexStore`, or `open_or_create`.
