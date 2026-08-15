# AGENTS.md -- index/

## Responsibility

Store metadata, snapshot persistence, and query orchestration via
[`Indexes`](indexes.rs).

## Layer split

| Layer | Types | Owns |
|-------|-------|------|
| Metadata | `StoreMeta`, `IndexRecord` | Corpus configuration and index catalog |
| Orchestrator | `Indexes` | `open` / `load` / `build`, query, and hydration |
| Snapshot storage | `SnapshotId`, `Files` | Committed artifact layout and shared file IDs |
| Kind impl | `ngram::Index` | Kind artifacts and query narrowing |

CLI owns daemon orchestration (`ReconcileOutcome::rebuild`, path debouncing). Core does
not expose `reconcile`, `unindexed_hit_paths`, or walk-merge helpers on
`Indexes`.

## Key types

- `StoreMeta` — persistent corpus, walk, filtering, coverage, and catalog configuration
- `IndexRecord` — typed catalog entry; builds kind artifacts and privately opens a kind
- `Indexes` — store orchestrator over one `.sift` directory
- `Files` — snapshot-owned mmap-backed `FileId → File` hydration
- `SnapshotId` — opaque committed snapshot identity

`Kind` in `record.rs` is private. It dispatches queries to runtime kinds; there
is no public index trait or public snapshot type.

## Conventions

- Search and candidates use `Indexes`, never N-gram internals.
- Kind queries may over-return but must not under-return; when narrowing is not
  possible, they return every shared file ID.
- `Files` is the single shared mmap-backed `FileId` space and lives at the
  snapshot root.
- `Indexes::open(dir, meta)` writes meta when the store is new — no
  `open_or_create`. Search uses `Indexes::load(dir) -> Result<Option<_>>`,
  which never creates a store (`None` when absent).
- `Indexes::build()` rebuilds from stored metadata: walk → `Files` → root
  `files.bin` → each `IndexRecord` namespace → publish `CURRENT`.
- Each kind writes only `snapshots/<id>/<kind-name>/`; shared `files.bin` and
  `manifest.json` remain at the snapshot root.

## Adding a new index kind

1. Add a typed `IndexRecord` arm with `build` / `open` / `name`.
2. Add a `Kind` arm and dispatch its query operation in `record.rs`.
3. Sibling module under `index/`.

## Do NOT

- Add N-gram logic outside `ngram/`.
- Add daemon or CLI orchestration to core.
- Add free functions — use methods on the owning type.
- Add parallel `open` / `open_or_create` (or mode enums that recreate that
  split).
- Add `#[allow]`.
- Use vague module names (`contract`, `lifecycle`, `search` for `Indexes`).
