# AGENTS.md

Guidelines for AI agents working on the sift codebase.

## Project Overview

Sift is an indexed code search engine written in Rust, built around **composable on-disk indexes**. It builds indexes tuned to the search workload, then uses them to narrow candidate files before running the full regex engine.

The core architecture treats code search like database query execution: every kind implements one `Index` trait; `Indexes` orchestrates build/update/search and intersects `query` results. Candidate resolution goes through `Grep::resolve_candidates` / `Plan`. Today the default index is runtime-width N-gram (trigram default).

The candidate pipeline is **plan (pure) → resolve (I/O) → search**: `Plan::new` decides discovery without querying indexes; `Plan::resolve` is the single I/O boundary (query + walk + order); `Searcher` consumes lazy `Candidates` (`into_vec()` materializes all).

## Build & Test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Run all three before pushing. CI enforces the same checks on Linux, macOS, and Windows.

## Profiling

Use system profilers on Criterion workloads (`[profile.bench]` keeps `debug = 1`), not
ad-hoc `/tmp` harnesses. Prefer samply; on macOS fall back to xctrace Time Profiler.

```bash
cargo bench -p sift-core --bench grep -- --profile-time 30 grep_search/full_scan
# wrap the same argv with samply / xctrace / heaptrack / perf as needed
samply record -- cargo bench -p sift-core --bench grep -- --profile-time 30 grep_search/full_scan
```

Log findings in `crates/core/benches/PROFILING.md`. Prefer paired before/after
evidence for performance PRs.

## Layout

| Path | Role |
|------|------|
| `crates/core/` | `sift-core`: composable index registry, query planning, candidate narrowing, search engine |
| `crates/core/src/candidates/` | Index-agnostic candidate description, planning, and resolution |
| `crates/core/src/index/` | `Index` trait, `IndexRecord`, `Indexes` orchestrator, `Snapshot` |
| `crates/core/src/index/ngram/` | N-gram `Index` impl (first shipped kind) |
| `crates/core/src/grep/` | Grep search API and matcher execution |
| `crates/cli/` | `sift-cli`: `sift` binary (clap CLI over core) |
| `fuzz/` | `cargo-fuzz` targets (standalone package, nightly) |
| `benchsuite/` | Comparative `rg` vs `sift` benchmarks |
| `scripts/` | `fuzz.sh`, `install.sh`, `release.sh` |
| `skills/` | Agent usage skill for searching with `sift` (`npx skills`); CLI development → `crates/cli/AGENTS.md` |
| `docs/` | Performance snapshots, compatibility matrix |

## Key Conventions

- **No `unsafe`** except in `index/mmap.rs` (documented safety invariant).
- **Strict clippy:** workspace uses `pedantic + nursery + cargo` warnings; CI uses `-D warnings`.
- Fix lints at the root cause. `#[allow]` is **never** permitted.
- **Never** add free helper functions or callback/`FnOnce` APIs (see Function
  Evolution).
- Prefer small, focused commits when the design is already right. When the design
  is wrong, make the sweeping change — do not paper over it with a local patch.
- Follow existing patterns in the crate you touch when they match these rules;
  redesign when they do not.
- Do not commit `target/`, `.cursor/`, local `.sift/` directories.

## Branch Names

Use short, descriptive kebab-case with a type prefix:

| Prefix | Use for |
|--------|---------|
| `feat/` | New behavior, flags, or API |
| `fix/` | Bug fixes, regressions |
| `docs/` | Documentation only |
| `chore/` | Tooling, CI, refactors with no user-visible change |

## Core API Entry Points

`Indexes::open(dir, meta)` (lifecycle) / `Indexes::load(dir) -> Result<Option<Indexes>>` (search) → `build` / `update` → `Grep::resolve_candidates` → `Searcher::execute`. CLI: `IndexJob::run` / `SnapshotRefresh::run` for lifecycle; `Run::execute` for search. See `crates/core/README.md`.

## Index layer

| Type | Role |
|------|------|
| `Index` trait | Opened kind only: `query` / `coverage` / `all_file_ids` / `update` |
| `IndexRecord` | Typed catalog knobs; `build` / `open` |
| `IndexConfig` | Corpus/walk/visibility for a write |
| `IndexDestination` | Directory or snapshot write target |
| `Indexes` | Build/update + query/hydrate orchestrator |
| `Files` | Snapshot-owned `FileId → Candidate` map |
| `StoreMeta` | Store metadata |

**Do not add to core:** `from_single`, `Indexes::candidates(SearchQuery)`, `reconcile`, `unindexed_hit_paths`, or other caller-specific helpers. Callers compose `Indexes::open`, `indexed_corpus().retain_unindexed`, and `Grep::resolve_candidates`.

## Architecture & Design

**No backward-compatibility bias.** Prefer the best current design. Do not
preserve old APIs, signatures, names, structures, call sites, or tests by
default when a cleaner architecture is available. Rename, delete, and reshape
freely. Preserve compatibility only when explicitly requested or when there is a
concrete persisted-data, shipped-behavior, external-consumer, or migration
requirement.

**Prefer sweeping architecture fixes over incremental patches.** If a change
reveals a weak abstraction, a parallel API, a boolean fork, or a use-case-shaped
helper, fix the design across the affected surface in the same change. Do not
leave the old shape behind "for compatibility" or defer the cleanup to a follow-up
when the right design is already clear. A larger, coherent diff is better than a
small diff that entrenches a bad API.

**Keep the design general, and keep the code simple.** Prefer the smallest API
that expresses the domain concept. Do not add layers, wrappers, or special-case
branches for one caller, one test, one benchmark, or one feature flag.

### Idiomatic Rust

Write idiomatic, best-practice Rust:

- Strong domain types over primitives and boolean flags.
- Explicit ownership and lifetimes; avoid unnecessary `clone`, `RefCell`, or
  interior mutability when a clearer ownership boundary exists.
- Clear `Result` / error boundaries; prefer typed errors at API edges.
- Small composable interfaces; one responsibility per type and method.
- Prefer iterators, `match`, and enums over ad-hoc boolean control flow.
- Redesign weak abstractions instead of layering new behavior on top of them.
- No `unsafe` except the documented mmap invariant; no `#[allow]` for clippy.

### Composition over specialization

Callers compose domain operations. Callees expose general operations; they do
not grow boolean forks or parallel code paths for each use case.

- Model real alternatives with domain types (enums/structs), then let the caller
  pass the choice.
- Do not bake a use case into a callee when the caller can compose existing
  operations (`extract` → `lookup` → `intersect`, walk → filter → materialize).
- Avoid helpers, method names, or signatures that overfit one caller or one
  implementation detail.

### Naming

Name types and functions after the **domain concept**, with short simple words.
Do not name things after the mechanism, the caller, or how they differ from a
sibling (`*_casei_*`, `*_with_*`, `*_for_ascii_*`, `helper_*`, `utils`).

Do not use `_for_*` in method names to restate an argument
(`posting_ids_for_literal(lit, …)` → `posting_ids(lit, …)`). The parameter
already says what was passed; the method name should say what is returned or
done.

When adding request/config structs, name them after the domain decision they
represent, not the mechanical data they carry. Avoid vague bundles such as
`Context`, `State`, `Read`, or `Options` unless those are the actual domain
terms. Prefer names like `CandidateSource`, `ScanScope`, and
`IndexCoverage` that tell callers how to reason about the API.

Do not expose low-level planner knobs through higher-level APIs as loose fields.
Group related inputs behind a domain type owned by the layer making the
decision, and make each field describe a stable concept rather than a temporary
implementation detail.

When behavior has distinct cases, model those cases directly with domain types.
Use enums for real alternatives, structs for coherent grouped data, and options
structs for configurable behavior. Avoid boolean flags when a named domain type
would make intent clearer.

Separate domain decisions from side effects. Prefer pure, testable logic that
returns decisions or actions, with I/O, filesystem access, spawning, logging,
locking, and channel communication kept at clear orchestration boundaries.

**Query pipeline:** plan (pure) → resolve (I/O) → search. Planners return
inspectable plan values; `resolve()` (consuming `self`) is the only
side-effectful step in candidate resolution. Never interleave I/O inside planning.

**Short domain names** over stage/mechanism names (`Candidates`, not
`ResolvedCandidates` / `ProgressiveCandidates`). If two types are a near-duplicate
across a layer boundary, merge or delete one.

**`Option<T>` models absence.** Do not add custom enums whose only second arm means
"nothing"; reserve enums for two or more meaningful alternatives.

**Single-phase construction.** Build values complete at construction time; no
post-construction mutators (`disable_*`, `set_*`) when the input is known upfront.

**Collections** follow Rust conventions: a named type with `IntoIterator`, `into_vec`,
and `is_empty`; no eager/lazy API pairs or load flags; no `len()` when iteration
filters rows and an exact count would lie.

## Function Evolution

**Evolve existing functions and APIs. Do not create new ones alongside them.**

Do not create `*_with_*`, `*_locked`, `*_async`, `*_new`, `*_casei_*`, or
similarly named parallel variants when the new function is the old function plus
one extra feature, mode, lock, flag, or parameter. That duplicates execution
paths and weakens the domain model.

If a different signature is needed:
- Change the original function to take a domain type for the new concept.
- Put the behavior in that one function body (match on the domain type).
- Delete the old shape rather than leaving a wrapper.

### No free helper functions

**Never** add module-level free functions to share logic — not `fn helper_*`,
not `fn intersect_sorted_ids(...)`, not `fn resolve_*_from_args(...)`, not
`const fn plan_*(...)` extracted “just for reuse”. Put behavior on the type that
owns the data (methods), or inline it at the single call site.

Nested closures or tiny blocks inside one function are fine when they remove
local duplication. A separate free function or a second method named after how
it differs from the first is not.

### No callback / `FnOnce` APIs

**Never** design APIs around callbacks, `impl FnOnce`, `impl Fn`, or
`impl FnMut` parameters to defer work or avoid constructing values. That hides
control flow and fights the domain model.

Prefer an explicit `match` on a domain enum at the call site (construct only in
the arms that need the value), or a method that returns a decision the caller
acts on. Do not pass “build the event/value later” closures into callees.

```rust
// Do this:
match collection {
    EventCollection::Discard => {}
    EventCollection::Collect => events.push(SearchEvent::Match(...)),
}

// NOT this:
collection.push(events, || SearchEvent::Match(...));
```

Examples of **bad** names that flag the pattern:
- `build_locked` (the variant adds a lock)
- `current_with_lease` (the variant adds a lease)
- `run_search_with_index` (the variant adds an index)
- `open_with_lease`
- `open_or_create` / `get_or_create` / `create_if_missing` (same op + missing branch)
- `posting_ids_for_ascii_casei_literal` (parallel path for one mode)
- `intersect_sorted_ids` (free helper instead of a type method / inline)
- `push(events, || SearchEvent::...)` (callback/`FnOnce` instead of match)

Examples of **good** names that describe the domain action:
- `publish_snapshot` (it writes files and commits)
- `resolve_candidates` (it looks up matching files)
- `build_index_metadata`
- `posting_ids` with a `GramMatch` (or similar) argument

When a lifecycle function needs to write to either a directory or a
snapshot store, use a destination enum instead of `*_to_dir` / `*_into` variants:

```rust
// Do this:
fn build(&self, dest: IndexDestination<'_>, config: &IndexConfig<'_>) -> Result<()>;

// NOT this (parallel variants):
fn build(config, output_dir) -> Result;     // directory
fn build_into(config, writer, ns) -> Result; // snapshot
```

## IndexDestination

Index writes take a destination and corpus config:

- `IndexDestination` — `Directory(&Path)` or `Snapshot { writer, namespace }`.
- `build(dest, config)` / `update(dest, config)` — full corpus from `config`.
- Open is path-only: `IndexRecord::open(dir, root, corpus_kind)`.

See `crates/core/src/index/artifacts.rs`, `record.rs`, and `ngram/`.

## Module Organization

Organize modules by domain responsibility, not by Rust item category. Avoid
catch-all files such as `types.rs`, `traits.rs`, `helpers.rs`, or `utils.rs`
unless the domain itself is genuinely that narrow. Prefer file/module names that
describe the behavior or concept they own. Use nested modules when a domain has
clear subdomains, such as `snapshot/store/disk.rs` and
`snapshot/store/memory.rs`.

## CLI Crate

The shipped binary lives in `crates/cli/` (`sift-grep`). It follows the same
domain-type rules as core; see [`crates/cli/AGENTS.md`](crates/cli/AGENTS.md).
Clap parses `*Decl` flag groups; **`Argv` resolves effective runtime values**
(ripgrep last-wins). Do not add `resolve_*_from_args` free-function helpers.

## Do NOT

- Skip CI checks (`fmt`, `clippy`, `test`) before pushing.
- Add dependencies without justification.
- Commit secrets, `.env` files, or editor-specific directories.
- Use `#[allow]` attributes.
- Preserve old APIs or shapes out of habit — redesign when the architecture is
  better served by a breaking change (see Architecture & Design).
- **Never** add free helper functions — put logic on the owning type or inline
  it (see Function Evolution / No free helper functions).
- **Never** add callback / `FnOnce` / `Fn` / `FnMut` parameters to defer
  construction — `match` on a domain enum at the call site instead (see
  Function Evolution / No callback APIs).
- Do not add parallel `*_with_*` / use-case-specific APIs — evolve the existing
  domain API instead (see Architecture & Design / Function Evolution).
- Overfit an API to one caller or test; keep operations general and let callers
  compose.
- Ship a local workaround when the right fix is a broader redesign of the
  surrounding types or call sites.

## Cursor Cloud specific instructions

- **Toolchain:** the workspace is `edition = "2024"`, so it needs Rust ≥ 1.85. The
  cloud VM's default was pinned to an older `1.83.0`; the environment now defaults
  to `stable` (`rustup default stable`). If a build fails with
  `feature edition2024 is required`, run `rustup default stable`.
- **Build / lint / test:** use the commands in `README.md` / the "Build & Test"
  section above (`cargo build --workspace`, `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo test --workspace --all-features`). No services or external deps needed.
- **Running the CLI:** the dev binary is `target/debug/sift` (bin name `sift`,
  crate `sift-grep`). You must build an index before searching, and search paths
  must sit under the indexed corpus root.
  - `index build` is async via a background daemon by default; pass `--wait` to
    build synchronously, or set `SIFT_NO_DAEMON=1` to disable the daemon.
  - Point `--sift-dir` at a writable index dir, e.g.:
    `target/debug/sift --sift-dir /tmp/demo/.sift index build --wait /tmp/demo`
    then `target/debug/sift --sift-dir /tmp/demo/.sift "pattern" /tmp/demo`.

## Learned User Preferences

- When work is split across multiple PRs, stop after each PR and pull from master before starting the next.
- Prefer unifying types across layers over adapter or translation layers between near-duplicates.
- Treat narrowly crate-restricted `pub(in crate::...)` wrapper enums as a smell; prefer domain types with clear ownership.
