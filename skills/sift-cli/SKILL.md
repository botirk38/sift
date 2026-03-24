---
name: sift-cli
description: >-
  Develops and debugs the sift grep-like CLI (sift-cli crate, `sift` binary).
  Use when changing CLI flags, subcommands, output, path handling, or
  integration tests; when the user mentions sift CLI, clap, crates/cli, or
  `cargo run -p sift-cli`.
---

# sift CLI

## Scope

- **Crate:** `crates/cli` (package `sift-cli`), binary **`sift`** (`src/main.rs`).
- **Engine:** `sift-core` only — no regex/index logic in the CLI; map flags → `SearchOptions` / `SearchMatchFlags` → `CompiledSearch::new` → `search_index` or `walk_file_paths` + `search_walk`.

## Invariants

1. **Global options before `build`:** e.g. `sift --sift-dir .sift build [corpus]` — `--sift-dir` is global on `Cli`, not on the `build` subcommand.
2. **Search paths** must resolve under the **indexed corpus root** (metadata in the index dir); see `corpus_path_prefixes` and related errors in `main.rs`.
3. **Patterns:** Rust `regex` unless `-F`. Literal `build`: `sift -- build` or `-e build` (documented in clap `about`).

## Where to edit

| Change | Location |
|--------|----------|
| Flags / help text | `PatternArgs`, `RegexFlags*`, `OutputFlags*`, `PathArgs`, `Subcommand` in `main.rs` |
| Exit codes / output | `run_search`, `print_matches`, `ExitCode` paths |
| Index path / open | `--sift-dir` default `.sift`, `Index::open` |
| E2E behavior | `crates/cli/tests/integration_*.rs` (spawn the real `sift` binary) |

## Commands

```bash
cargo run -p sift-cli -- --help
cargo test -p sift-cli
cargo build --release -p sift-cli
```

After core API changes, run **`cargo test --workspace --all-features`** — CLI depends on `sift-core`.

## Docs in repo

- `crates/cli/README.md`, `crates/cli/AGENTS.md`
- Root `README.md` (user-facing quick start vs ripgrep)

## Checklist for new flags

- [ ] Thread through clap field → `SearchOptions` / `SearchMatchFlags` (or explicit branch) consistently with `sift-core`.
- [ ] Update `--help` strings; add or extend the relevant CLI integration test if behavior is user-visible.
- [ ] No duplicate regex/planner logic — keep it in **`sift-core`**.
