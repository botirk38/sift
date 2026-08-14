# AGENTS.md -- sift-grep (CLI crate)

## Responsibility

Thin CLI binary over `sift-core`. Parses flags with clap, maps them to `SearchOptions`/`SearchFlags`, and dispatches to core.

## Architecture

Two-layer flag model:

1. **`*Decl` structs (clap)** — declare flags for help and parsing.
2. **Resolved domain types** — effective runtime values from raw argv (ripgrep last-wins ordering).

`Argv` is collected once in `main_entry`. `Cli::run_config(argv)` resolves argv once into `RunConfig` / `IndexJob` (no Decl bags or `Argv` at execute). `Run::execute(daemon)` and `IndexJob::run(daemon)` take no `Argv`. Domain modules never import `Cli`.

### Module pairing (decl → config → resolve/run)

| Module | Clap decls | Config / resolved type | Entry point |
|--------|------------|------------------------|-------------|
| `grep/argv.rs` | — | `Argv` | `Argv::from_env`, `Argv::new` |
| `grep/ignore.rs` | `Ignore*Decl`, … | `IgnoreResolution` | `IgnoreResolution::resolve` |
| `grep/pattern.rs` | `PatternArgs`, … | `PatternDecl`, `PatternArgv`, `ResolvedPatterns` | `ResolvedPatterns::resolve`, `PatternDecl::query` |
| `grep/output.rs` | `LineNumberDecl`, … | `OutputDecl`, `OutputArgv` | `OutputArgv::resolve`, `OutputDecl::print_spec` |
| `grep/filter.rs` | `FilterDecl`, … | `FilterConfig`, `TypeCatalog` | `FilterConfig::file_config` |
| `grep/paths.rs` | `PathArgs` | `CorpusScope` | `CorpusScope::resolve` |
| `grep/input.rs` | — | `InputSources`, `ContentTransform` | `InputSources::resolve`, `stdin_streams` |
| `grep/run.rs` | — | `RunConfig`, `Run`, `RunResult` | `Run::execute(daemon)` |
| `format/printer.rs` | — | `SearchPrinter`, `PrintSpec` (`SearchMode`) | `SearchPrinter::print` → `Report` |
| `index/mod.rs` | — | `IndexRequest`, `IndexJob`, `SnapshotRefresh` | `IndexJob::resolve`, `IndexJob::run(daemon)`, `SnapshotRefresh::run` |
| `index/selection.rs` | `IndexDecl` | `IndexSelection` | `IndexSelection::resolve` (argv order for `--index`/`--width`/`--norm`) |
| `index/daemon/` | — | `Daemon`, `DaemonOrchestrator`, `ServeConfig` | `Daemon::{index,validate_snapshot}`, `DaemonOrchestrator::{start,serve}` (`ipc` / `watcher` / `refresh`) |

## Search pipeline (CLI)

```text
RunConfig → Run::execute
InputSources → stdin_streams → Inputs
Scan + Plan::resolve → Candidates
Searcher::execute → SearchPrinter::print → Report
```

Index lifecycle: `IndexJob::run` → `SnapshotRefresh::run` (full rebuild from
stored metadata). `SnapshotRefresh::run(&mut Indexes)` always calls
`Indexes::build()`; daemon debouncing and IPC stay in `index/daemon/`.

## Structure

- **`src/lib.rs`**: `main_entry`; re-exports `grep::*` for tests/benches.
- **`src/cli.rs`**: `Cli` parser, `run_config`, `dispatch`.
- **`src/format/`**: `SearchPrinter`, sinks (`LinePrinter`, `AggregatePrinter`, `JsonPrinter`).
- **`src/grep/`**: search domain — `argv`, `run`, `pattern`, `filter`, `output`, `paths`, `input`, `ignore`, `engine`.

## Behavior Notes

- Global options (e.g. `--sift-dir`) must appear **before** `index` subcommands.
- Search paths are resolved and must sit under the corpus root in the index metadata.
- Extend flags by threading new `SearchFlags`/`SearchOptions` fields through to `Query` in core. Do not duplicate regex logic here.
- Mixed paths + stdin: resolve corpus candidates, append stdin in `InputSources::search_inputs`.

## Testing

```bash
cargo test -p sift-grep
cargo build --release -p sift-grep
```

## Do NOT

- Add duplicate config builders (`grep_config` vs `into_run`) — use `run_config(argv)` only.
- Add `resolve_*_from_args` free functions — use domain type `resolve` methods on `Argv`.
- Import `Cli` from domain modules — build configs in `cli.rs` and pass them in.
- Duplicate regex or search logic from `sift-core`.
- Put stdout formatting in `sift-core`.
