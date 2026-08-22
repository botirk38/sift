# AGENTS.md -- fuzz/

## Isolation

Standalone package excluded from the root workspace (`Cargo.toml` `exclude = ["fuzz"]`). Use `./scripts/fuzz.sh` or `cd fuzz && cargo fuzz ...` so `fuzz/rust-toolchain.toml` (nightly) applies.

## Targets

- **`search_usage`**: shared tiny index per process (`OnceLock`); fuzzes patterns + `SearchOptions` via `Query::candidates` / `Query::search`.
- **`compile_only`**: fuzzes `Query::compile` only (no filesystem).
- **`decode_postings`**: arbitrary `postings.bin` payloads; open errors, never panics.
- **`decode_kinds`**: arbitrary AST `kinds.bin` bytes; open errors, never panics.

## Scope

Fuzz targets cover **`sift-core` only**, not the CLI.

## Do NOT

- Add the fuzz crate to the main workspace `members` (breaks `cargo-fuzz` layout).
- Assume `sift-cli` is fuzzed here.
- Run without nightly toolchain (sanitizers require it).
