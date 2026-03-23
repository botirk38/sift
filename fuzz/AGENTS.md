# Agent notes (fuzz/)

## Isolation

This directory is a **standalone** package: root `Cargo.toml` lists `exclude = ["fuzz"]`. Use **`./scripts/fuzz.sh`** or `cd fuzz && cargo fuzz …` so `fuzz/rust-toolchain.toml` (nightly) applies.

## Targets

- **`search_usage`** — one shared tiny index per process (`OnceLock`); fuzzes pattern strings + `SearchOptions` against `CompiledSearch` + `search_index`.
- **`compile_only`** — fuzzes `compile_search_pattern` only (no FS).

## Do not

- Add the fuzz crate to the main workspace `members` without a strong reason (breaks `cargo-fuzz` layout expectations).
- Assume `sift-cli` is fuzzed here; scope is **`sift-core`** only.

See **`README.md`** in this directory for install and run examples.
