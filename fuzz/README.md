# Fuzz

LibFuzzer targets for `sift-core`. Exercises `Query`, indexed search, and query compilation.

## Setup

```bash
cargo install cargo-fuzz   # one-time
```

Requires **nightly** Rust (sanitizers). The `fuzz/rust-toolchain.toml` file handles this automatically.

## Usage

```bash
# From repo root (recommended, uses fuzz/rust-toolchain.toml)
./scripts/fuzz.sh build search_usage
./scripts/fuzz.sh run search_usage -- -max_total_time=30

# Quick smoke test
./scripts/fuzz.sh quick

# Or directly
cd fuzz && cargo fuzz run search_usage -- -max_total_time=30
```

## Targets

| Target | Description |
|--------|-------------|
| `search_usage` | Tiny index per process (`OnceLock`); fuzzes patterns + `SearchOptions` via `Query::candidates` / `Query::search` |
| `compile_only` | Fuzzes `Query::compile` only (no filesystem) |
| `decode_postings` | Overwrites `postings.bin` with arbitrary payloads; reopening must error, never panic |
| `decode_kinds` | Overwrites the AST kind's `kinds.bin` with arbitrary bytes; reopening must error, never panic |

## Layout

`fuzz/` is **excluded** from the root workspace so it stays a standard `cargo-fuzz` package. See `AGENTS.md` for contributor guidelines.
