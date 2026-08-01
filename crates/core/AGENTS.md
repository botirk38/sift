# AGENTS.md -- sift-core

## Responsibility

Composable indexed code search: index lifecycle, candidate planning, and grep-style matching.

## Architecture

```
Indexes::open (lifecycle) / Indexes::load → Option (search)
Plan::new (pure) → Plan::resolve (query I/O) → Grep::search
```

- `Indexes` — build/update/publish and query/hydrate over one store (`load` is `None` when absent)
- `Snapshot` — opened `Box<dyn Index>` vec for a committed snapshot
- `Plan` — pure discovery decision; `resolve` owns index query I/O
- `Grep` — public entry for resolve + search

Today the default index is `ngram::Index::new()` (trigram width).

## Public API

Search (re-exported from `lib.rs`):

- `Grep`, `GrepRequest`, `Grep::resolve_candidates`, `Searcher`, `Report`
- `Indexes`, `IndexedCorpus`, `SnapshotId`
- `Index`, `IndexRecord`, `IndexConfig`, `IndexWrite`, `ngram::Index`, `GramWidth`
- `Candidates`, `CandidateSource`, `ScanScope`, `SnapshotFreshness`

Internal: `Plan`.

## Source map

| Module | Responsibility |
|--------|----------------|
| `index/search.rs` | `Indexes` lifecycle + search |
| `index/contract.rs` | `Index` trait, `IndexWrite`, `IndexRecord` |
| `index/snapshot/` | `Snapshot`, persistence |
| `index/ngram/` | N-gram implementation |
| `grep/` | Public search API |
| `candidates/plan.rs` | `Plan` (plan + resolve) |
| `candidates/candidates.rs` | `Candidates` collection |
| `corpus/` | `Candidate`, filters, walk |

## Search flow

```text
Grep::execute
  1. coverage   ← Coverage::from_mode(mode)
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

## Testing

```bash
cargo test -p sift-core
```

## Do NOT

- Break public API without updating CLI.
- Add `unsafe` outside `index/mmap.rs`.
- Import `index::ngram` from `grep/`.
- Put stdout formatting in core.
- Expose `Indexes::candidates` or test-only constructors.
- Mix planning with I/O.
- Reintroduce `IndexStore` or `open_or_create`.
