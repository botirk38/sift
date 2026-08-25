# AGENTS.md

Guidelines for AI agents working on the sift codebase.

## Project Overview

Sift is an indexed code search engine written in Rust, built around **composable on-disk indexes**. It builds indexes tuned to the search workload, then uses them to narrow candidate files before running the full regex engine.

The core architecture treats code search like database query execution:
`StoreMeta` configures a catalog of `IndexRecord`s; `Indexes` builds snapshots
and intersects private kind queries. File resolution goes through
`Plan::resolve`. Today the default index is runtime-width N-gram (trigram
default).

The candidate pipeline is **plan (pure) → resolve (I/O) → search**: `Plan::new` decides discovery without querying indexes; `Plan::resolve` is the single I/O boundary (query + walk + order); `Searcher` consumes lazy `Candidates` (`into_vec()` materializes all).

## Build & Test

```bash
cargo fmt -p sift-core -p sift-grep -- --check
cargo clippy -p sift-core -p sift-grep --all-targets --all-features --no-deps -- -D warnings
cargo test --workspace --all-features \
  --exclude regex --exclude regex-automata --exclude regex-syntax --exclude pcre2
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
| `crates/core/src/index/` | `StoreMeta`, `IndexRecord`, `Files`, disk snapshots, `Indexes` |
| `crates/core/src/index/ngram/` | N-gram kind implementation (first shipped kind) |
| `crates/core/src/index/ast/` | Tree-sitter languages and ast-grep patterns |
| `crates/core/src/search/` | Query, Searcher, Origin, SearchMode, report/events |
| `crates/core/src/corpus/` | `File`, `FileFilter`, `FileOrder`, walk |
| `crates/cli/` | `sift-grep`: `sift` / `sift-daemon` binaries (clap CLI over core) |
| `crates/regex/` | Vendored `regex` 1.13.1 (pristine) |
| `crates/regex-automata/` | Vendored `regex-automata` 0.4.18 (`Config::pool`, `is_match_with`) |
| `crates/regex-syntax/` | Vendored `regex-syntax` 0.8.11 (pristine) |
| `crates/pcre2/` | Vendored `pcre2` 0.2.11 (`MatchData` search; `pcre2-sys` stays crates.io) |
| `fuzz/` | `cargo-fuzz` targets (standalone package, nightly) |
| `benchsuite/` | Comparative `rg` vs `sift` benchmarks |
| `scripts/` | `fuzz.sh`, `install.sh`, `release.sh` |
| `skills/` | Agent usage skill for searching with `sift` (`npx skills`); CLI development → `crates/cli/AGENTS.md` |
| `docs/` | Performance snapshots, compatibility matrix |

## Domain nouns

| Type | Module | Role |
|------|--------|------|
| `Indexes` | `index` | `.sift` directory: meta + current snapshot; `open` / `load` / `build` |
| `Files` | `index` | Snapshot-owned `FileId → File` map |
| `StoreMeta` | `index` | Persistent corpus, walk, filter, coverage, and catalog configuration |
| `IndexRecord` | `index` | Typed catalog record; builds kind artifacts and privately opens a kind |
| `SnapshotId` | `index` | Opaque committed snapshot identity |
| `Plan` | `candidates` | Pure discovery decision |
| `Candidates` | `candidates` | Output of `Plan::resolve` |
| `Query` | `search` | Patterns + options |
| `Searcher` | `search` | `Searcher::new(Query)` + `execute` / `stream` |
| `Bytes` | `search` | Resident searchable bytes for one input (`Slice` or `Memory`) |
| `Events` | `search` | Stream output: `Iterator<SearchEvent>` + `into_report` |
| `Lines` | `search` | Iterator that materializes `Line` |
| `FileReport` | `search` | Per-input result data (built by execute / `Events::into_report`) |
| `Reports` | `search` | Result of searching a set of inputs |
| `Mention` | `search` | How an input was requested (`Explicit` / `Discovered`) |
| `Io` | `search` | How file bytes are read (`Sync` / `Mmap` / `Uring`; default `Mmap`) |
| `Hit` | `search` | Listing unit (`Line` or `Span`) |
| `SearchMode` | `search` | How results are listed (`Print` / `Count` / path modes) |
| `SearchReport` | `search` | Listing + `Stats` |
| `File` | `corpus` | Indexed path identity |
| `Origin` | `search` | `File` or `Stream { label }` search identity |
| `Run` | `cli/grep` | Resolved search intent; `execute` (no `Argv`) |
| `IndexJob` | `cli/index` | Resolved index lifecycle; `run` |
| `Daemon` | `cli/index/daemon` | Background work; modules `ipc`, `watcher`, `refresh` |

Values (not aggregates): `StoreMeta`, `SearchMode`, `Scan` / `ScanScope`,
`FileFilter`, `FileOrder`, `Coverage`. Printing stays under `cli/format`.

## Vendored regex crates

Sift vendors `regex` 1.13.1, `regex-automata` 0.4.18, `regex-syntax` 0.8.11,
and `pcre2` 0.2.11 as workspace members under `crates/`. `[patch.crates-io]`
redirects `ignore` / `globset` to the same automata. Those crates keep the
default cache pool **on**. Sift compiles `meta::Regex` with `pool(false)` and
passes a `FileScan`-owned `Cache`.

Vendor crates do **not** inherit `[workspace.package]` or `[lints] workspace =
true`. They keep upstream `unsafe` and formatting. Sift clippy/fmt/test stay on
sift packages (`default-members` is `crates/core` and `crates/cli`; clippy uses
`-p sift-core -p sift-grep --no-deps` so path-dep vendor crates are not relinted). Overlay
tests: `cargo test -p regex-automata` and `cargo test -p pcre2` locally after
engine changes; do not put the full regex suite on the 3-OS CI matrix.

**Overlay only** `regex-automata` and `pcre2`. `regex` and `regex-syntax` stay
pristine.

| Overlay | Files |
|---------|-------|
| `regex-automata` | `src/meta/regex.rs` (`Config::pool`, `is_match_with`, pool-less `Regex`) |
| `pcre2` | `src/ffi.rs` (`pub MatchData`), `src/bytes.rs` (`create_match_data`, `is_match_with`, `find_at_with`) |

**Re-import:** copy the published crate trees from crates.io over
`crates/{regex,regex-automata,regex-syntax,pcre2}`, then rebase the overlay
commits. Do not inherit workspace package/lints. Drop `Cargo.lock` from `pcre2`.

## Key Conventions

- **No `unsafe` in sift crates** except in `index/mmap.rs` (documented safety invariant). Workspace does not deny `unsafe_code` so mmap needs no `#[allow]`. Vendored regex crates keep upstream `unsafe`.
- **Strict clippy on sift packages:** workspace uses `pedantic + nursery + cargo` warnings; CI uses `-D warnings` with vendor crates excluded.
- Fix lints at the root cause. `#[allow]` is **never** permitted in sift crates.
- **Never** add free helper functions or callback/`FnOnce` APIs (see API
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

`Indexes::open(dir, meta)` (lifecycle) / `Indexes::load(dir) ->
Result<Option<Indexes>>` (search) → `build()` → `Plan::resolve` →
`Searcher::execute` / `Searcher::stream`. CLI: `IndexJob::run` / `ReconcileOutcome::rebuild` for
lifecycle; `Run::execute` for search; `Daemon` / `DaemonOrchestrator` for
background refresh. See `crates/core/README.md`.

## Index layer

| Type | Role |
|------|------|
| `StoreMeta` | Persistent corpus, walk, filtering, coverage, and catalog configuration |
| `IndexRecord` | Typed catalog record; builds kind artifacts and privately opens a kind |
| `Indexes` | Open/load/build + query/hydrate orchestrator |
| `Files` | Snapshot-owned `FileId → File` map |
| `SnapshotId` | Opaque committed snapshot identity |

`record.rs` owns the private `Kind` enum that dispatches queries. There is no
public `Index` trait or public `Snapshot` type. Snapshot-root `files.bin`
(`SIFTFIL2`) is shared by all kinds; kind artifacts live beneath
`snapshots/<id>/<kind-name>/`.

**Do not add to core:** `from_single`, `Indexes::candidates(Query)`, `reconcile`,
`unindexed_hit_paths`, or other caller-specific helpers. Callers compose
`Indexes::open`, `Files::retain_unindexed`, and `Plan::resolve`.

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

- Enums for real alternatives; `bool` for on/off. Do not wrap a bool in an
  entity (`Off`/`On`, `StatsMode`).
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

### Actors and data

Types are either actors or data.

- **Actors** own responsibilities and capabilities. Name them after the thing
  that acts (`Searcher`, `FileScan`, `Indexes`). Verbs are methods on that
  actor. Do not add a second actor for a job the first already owns.
- **Data** are domain values (`Origin`, `Line`, `Hit`, `Listing`, `FileReport`).
  Methods on data belong to the value (`display`, `bytes`). Data does not
  orchestrate I/O or other entities.

Not entities:

- On/off switches. `bool` on the owner (`invert_match`).
- Walk state. Enums are fine (`Break`, `Nul`, `Item`). They are not actors or data.
- Attributes of an existing value (`ListedFile` over `Origin`). Use the value.
- Probe/cursor bags (`At`, `Context`, `State`) whose fields the actor already
  holds. Pass the current `Line`.
- Enums whose only second arm is none. `Option`.

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
terms. Prefer names like `Scan`, `ScanScope`, and
`IndexCoverage` that tell callers how to reason about the API.

Do not expose low-level planner knobs through higher-level APIs as loose fields.
Group related inputs behind a domain type owned by the layer making the
decision, and make each field describe a stable concept rather than a temporary
implementation detail.

When behavior has distinct cases, model those cases directly with domain types.
Use enums for real alternatives, structs for coherent grouped data, and options
structs for configurable behavior. On/off switches stay `bool`.

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

## API Evolution

Evolve the existing API. Do not add a parallel one.

A new need is a new argument, a new enum arm, or a match in the function
that already owns the behavior. It is not a second function, type, or
module named after how it differs from the first.

If a different signature is needed:
- Put the new concept in a domain type (enum/struct) on the existing API.
- Match on that type in one function body.
- Delete the old shape. Do not leave a wrapper.

### Smells

Sibling functions that only change args, bound, mode, or flag
(`search` + `search_first`, `execute` + `list_paths`, `push` + `push_bytes`,
`open` + `open_with_lease`, `*_with_*`, `*_for_*`). Match on `SearchBound`,
`SearchMode`, `Input`, or `SearchOptions` in the existing function.
`execute` vs `stream` are distinct I/O shapes (materialize vs iterate), not a
sink `Option`.

Adapter types between near-duplicates (`SearchInputs` over `Inputs` +
`Candidates`). Unify to the domain type callers already have.

Argument bags whose only job is dodging `too_many_arguments`
(`IndexedFiles`). A struct is valid when it is a domain concept (`Scan`,
`Input`), not a clippy workaround.

Cursor bags for `too_many_arguments` when the actor already holds the fields
(`At` on `FileScan`). Pass the current `Line`.

Wrapper whose only field is another data type (`FileEvent { origin }`).
Use the inner value (`Begin(Origin)`).

Result types named as actors (`FileSearch` while `Searcher` searches).
Results are data; the actor writes them (`FileReport` is written by `FileScan`).

`Off`/`On` enums (`StatsMode`, `Quiet::{Off,On}`). Use `bool`.

Iterator items that are not the domain value (`Span` / `Held` + `as_line`
instead of `Line`). `next()` materializes the type the rest of the API
uses.

Invalid states (`omitted: bool`, empty-second-arm enums). Absence is
`Option`. Real alternatives are enums. Complete values at construction.

Typed insert variants on collections (`push_path`, `push_bytes`,
`with_stream`). One `push(Input)`.

Test-only helpers in production modules (`search_bytes`, `memory()`).
Tests use the public API.

`as_*` / `to_*` converters that exist because two types are the same
value (`Held::as_line`). Delete one type.

Callbacks (`FnOnce` / `FnMut`) to defer construction. Match on a domain
enum at the call site.

```rust
// Do this:
match collection {
    EventCollection::Discard => {}
    EventCollection::Collect => events.push(SearchEvent::Match(...)),
}

// NOT this:
collection.push(events, || SearchEvent::Match(...));
```

Free helpers (`helper_*`, `intersect_sorted_ids`, `resolve_*_from_args`).
Methods on the owning type, or inline at the one call site.

### Names that flag the pattern

`search_first`, `list_paths` (as a Searcher sibling), `push_bytes`,
`build_locked`, `current_with_lease`, `run_search_with_index`,
`open_or_create`, `posting_ids_for_ascii_casei_literal`

## Module Organization

Organize modules by domain responsibility, not by Rust item category. Avoid
catch-all files such as `types.rs`, `traits.rs`, `helpers.rs`, or `utils.rs`
unless the domain itself is genuinely that narrow. Prefer file/module names that
describe the behavior or concept they own. Use nested modules when a domain has
clear subdomains, such as `index/ngram/storage/`.

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
  it (see API Evolution).
- **Never** add callback / `FnOnce` / `Fn` / `FnMut` parameters to defer
  construction — `match` on a domain enum at the call site instead (see
  API Evolution).
- Do not add parallel `*_with_*` / use-case-specific APIs — evolve the existing
  domain API instead (see Architecture & Design / API Evolution).
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
  section above (`cargo build --workspace`, `cargo fmt -p sift-core -p sift-grep -- --check`,
  `cargo clippy -p sift-core -p sift-grep --all-targets --all-features --no-deps -- -D warnings`,
  `cargo test --workspace --all-features --exclude regex --exclude regex-automata --exclude regex-syntax --exclude pcre2`). No services or external deps needed.
- **Running the CLI:** the dev binary is `target/debug/sift` (bin name `sift`,
  crate `sift-grep`). You must build an index before searching, and search paths
  must sit under the indexed corpus root.
  - `index build` is async via a background daemon by default; pass `--wait` to
    build synchronously. Leave the daemon enabled unless the environment cannot
    run one; `SIFT_NO_DAEMON=1` is an escape hatch, not the default workflow.
  - Point `--sift-dir` at a writable index dir, e.g.:
    `target/debug/sift --sift-dir /tmp/demo/.sift index build --wait /tmp/demo`
    then `target/debug/sift --sift-dir /tmp/demo/.sift "pattern" /tmp/demo`.

## Learned User Preferences

- When work is split across multiple PRs, stop after each PR and pull from master before starting the next.
- Prefer unifying types across layers over adapter or translation layers between near-duplicates. Do not re-expose a library's already-unified type as parallel enum arms.
- Treat narrowly crate-restricted `pub(in crate::...)` wrapper enums as a smell; prefer domain types with clear ownership.
- Prefer search identity as `Origin::{File, Stream}` (not `Candidate`); stream identity is a string `label`, not a filesystem `Path`.
- Prefer printer/JSON rendering via match on `Origin` variants; do not Path-force stream labels for API uniformity.
- Prefer enums over bools when modeling domain data with distinct cases (`Hit`, `ZeroCounts`, `SearchMode`) and walk state (`Break`, `Nul`). Enums are not entities. On/off switches stay `bool`. Do not reify `Option` / emptiness checks into parallel state enums.
- Name types and methods after the domain concept with short, clear words; avoid mechanism names, `_for_*` restatements, and probe/context bags.
- Minimize helper methods as well as free functions—only when absolutely justified.
- Types are actors or data. Actors own capabilities (`Searcher::execute`, `Searcher::stream`, `FileScan::next_item`). Data holds values (`Origin`, `Line`, `FileReport`). Do not split one entity across multiple files. Prefer named structs over unnamed tuples when the value models an entity. Treat extra code and abstractions as liability unless explicitly justified.
- The search walk does not take an output method. `execute` materializes a report; `stream` returns `Events`. Do not add a sink/`FnMut` callback. Do not add test-only helpers to production modules; tests use the public API.
- Keep index orchestration and on-disk storage/versioning index-kind-agnostic; kind-specific logic stays under the kind module (e.g. `ngram/`) so new indexes are easy to add.
- When planning architecture work, prefer deep critique and a cleaned plan with code snippets for easy review before implementation. If implementation starts making many design decisions, go back to planning. Run the same critique loop on PRs and keep fixing until the design is right.

## Learned Workspace Facts

- Core search lives under `crates/core/src/search/` (`Query`, `Searcher`, `Bytes`, `Lines`, `Origin`, `SearchError`); `Plan` lives under `candidates/`. `Searcher::execute(inputs, mode)` materializes a report (Rayon on exhaustive). `Searcher::stream(inputs, mode)` returns `Events` (`Iterator<SearchEvent>` + `into_report`). `FileScan` walks one input's bytes and does not know listing or events. `SearchReport.stats` is always `Stats`. `Lines` iterates `Line`. `SearchBound` selects exhaustive vs first-match. `Inputs` is the unified input collection. `Io` chooses how `Bytes` fills `fastio::OwnedBytes` (sync / mmap / uring `read_all`); there is no windowed haystack. There is no `sift_core::grep` module or `Grep` facade. The CLI keeps a local `grep` module for `Run`.
- Search file I/O uses `fastio`. Default `Io` is mmap; `uring` is opt-in on Linux (`read_all` after `std::fs` open). Batched reads per file, Rayon across files.
- CLI no longer depends on `grep-cli`, `grep-printer`, or `termcolor`; colors/hyperlinks live in `format/`, and `--search-zip` spawns gzip/xz/zstd by extension. Core still uses `grep-matcher`, `grep-regex`, and `grep-pcre2`.
- Daemon IPC is enum-shaped (`DaemonRequest` / `DaemonResponse`); accept loop forwards `Event::Client` — no `FnMut` handler API.
- Snapshot composition is meant to share one corpus `FileId` → path table per snapshot; kinds return `FileId`s and write only kind artifacts under their namespace.
