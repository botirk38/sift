# AGENTS.md -- index/

## Responsibility

Uniform index kinds, snapshot persistence, and search orchestration via
[`Indexes`](indexes.rs).

## Layer split

| Layer | Types | Owns |
|-------|-------|------|
| Record | `IndexRecord`, `Index` trait | Catalog knobs + opened query/update |
| Orchestrator | `Indexes`, `StoreMeta` | `open` / `build` / `update` + query/hydrate |
| Snapshot | `Snapshot`, `Files`, `SnapshotId` | Shared `Files`, opened indexes, artifact I/O |
| Kind impl | `ngram::Index` | Knobs + storage + trait impl |

CLI owns daemon orchestration (`SnapshotRefresh`, path debouncing). Core does
not expose `reconcile`, `unindexed_hit_paths`, or walk-merge helpers on
`Indexes`.

## Key types

- `Index` — opened trait only: `query` / `coverage` / `all_file_ids` / `update`
- `IndexRecord` — typed catalog entry; `build` / `open` → `Box<dyn Index>`
- `IndexConfig` — corpus/walk/visibility inputs for a write
- `IndexDestination` — directory or snapshot write target
- `Indexes` — store orchestrator over one `.sift` directory
- `Files` — snapshot-owned `FileId → File` hydration
- `IndexedCorpus` — covered rel-paths; `retain_unindexed` filters paths

## Conventions

- `grep/` and `candidates/` talk to `Indexes` and the `Index` trait, never
  `ngram/` internals.
- `Index::query` returns `Vec<FileId>` (may over-return; must not under-return;
  cannot narrow → every covered id). No `AllIndexed` / `Unavailable` status.
- Hydration uses `Snapshot::files()`, never a lead index.
- `Indexes::open(dir, meta)` writes meta when the store is new — no
  `open_or_create`. Search uses `Indexes::load(dir) -> Result<Option<_>>`,
  which never creates a store (`None` when absent).
- Catalog knobs live on `IndexRecord`; opened indexes cannot be queried before
  `open`.

## Adding a new index kind

1. Add a typed `IndexRecord` arm with `build` / `open` / `name`.
2. Implement opened `Index` on the kind's type.
3. Sibling module under `index/`.

## Do NOT

- Add N-gram logic outside `ngram/`.
- Add daemon or CLI orchestration to core.
- Add free functions — use methods on the owning type.
- Add parallel `open` / `open_or_create` (or mode enums that recreate that
  split).
- Add `#[allow]`.
- Use vague module names (`contract`, `lifecycle`, `search` for `Indexes`).
