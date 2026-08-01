# AGENTS.md -- index/

## Responsibility

Uniform index kinds, snapshot persistence, and search orchestration via
[`Indexes`](search.rs).

## Layer split

| Layer | Types | Owns |
|-------|-------|------|
| Contract | `Index` trait, `IndexWrite`, `IndexRecord` | Kind interface + persistable identity |
| Orchestrator | `Indexes`, `StoreMeta` | `open` / `build` / `update` + query/hydrate |
| Snapshot | `Snapshot`, `SnapshotId` | Opened `Box<dyn Index>` vec, artifact I/O |
| Kind impl | `ngram::Index` | Knobs + storage + trait impl |

CLI owns daemon orchestration (`SnapshotRefresh`, path debouncing). Core does
not expose `reconcile`, `unindexed_hit_paths`, or walk-merge helpers on
`Indexes`.

## Key types

- `Index` — uniform trait: `build` / `open` / `query` / `candidate` / `update`
- `IndexRecord` — persisted catalog entry (`kind` + `params`)
- `IndexConfig` — corpus/walk/visibility inputs for a write
- `IndexWrite` — `{ dest, config, paths }` for `build` and `update`
- `Indexes` — lifecycle + search over one store
- `IndexedCorpus` — covered rel-paths; `retain_unindexed` filters paths
- `IndexSource` / `IndexDestination` — directory vs snapshot I/O

## Conventions

- `grep/` and `candidates/` talk to `Indexes` and the `Index` trait, never
  `ngram/` internals.
- `Index::query` returns `Vec<FileId>` (may over-return; must not under-return;
  cannot narrow → every covered id). No `AllIndexed` / `Unavailable` status.
- Filtering happens in `Indexes` hydrate (`candidate(id)` then filter), not on
  the trait.
- `Indexes::open(dir, meta)` writes meta when the store is new — no
  `open_or_create`. Search uses `Indexes::load(dir) -> Result<Option<_>>`,
  which never creates a store (`None` when absent).
- Kind knobs live on the kind's `Index` (`Index::new()` + optional setters);
  no separate builder/config types.

## Adding a new index kind

1. Implement `Index` on the kind's type (`kind` / `params` / `name` / lifecycle /
   `query` / `candidate` / `coverage`).
2. Register `"kind"` in `IndexRecord::to_index`.
3. Sibling module under `index/`.

No enum arms in a central dispatcher. Search/build loops stay match-free.

## Do NOT

- Add N-gram logic outside `ngram/`.
- Add daemon or CLI orchestration to core.
- Add free functions — use methods on the owning type.
- Add parallel `open` / `open_or_create` (or mode enums that recreate that
  split).
- Add `#[allow]`.
