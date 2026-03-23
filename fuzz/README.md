# Fuzz Testing

## Introduction

Fuzz testing feeds pseudo-random inputs to library code to surface panics, memory issues (with sanitizers), and logic bugs. The `sift` fuzz crate targets **`sift-core`**: index-backed search, pattern compilation (`compile_search_pattern`), and related paths—not the full CLI. (Upstream **ripgrep** ships a separate fuzz crate that only stress-tests **`globset::Glob`** parsing; scopes differ.)

## Toolchain

`cargo-fuzz` uses nightly-only sanitizer flags. From the **repository root**, prefer the wrapper so `fuzz/rust-toolchain.toml` applies:

```bash
./scripts/fuzz.sh build search_usage
./scripts/fuzz.sh run search_usage -- -max_total_time=30
```

Or work inside this directory (same effect):

```bash
cd fuzz
cargo fuzz run search_usage -- -max_total_time=30
```

If you run `cargo fuzz` from the repo root without the wrapper, use an explicit nightly:

```bash
cargo +nightly fuzz run search_usage --manifest-path fuzz/Cargo.toml -- -max_total_time=30
```

## Installation

Install the `cargo-fuzz` subcommand once:

```bash
cargo install cargo-fuzz
```

## Targets

| Target           | Role |
|------------------|------|
| `search_usage`   | Opens a fixed tiny index once per process, then fuzzes patterns and `SearchOptions` against `Index::search` and `compile_search_pattern`. |
| `compile_only`   | Fuzzes `compile_search_pattern` with static and dynamic branches (no filesystem). |

List binaries:

```bash
cargo fuzz list
```

## Running fuzz tests

Run a named target:

```bash
cargo fuzz run search_usage
```

That runs until stopped. Bound wall time (recommended):

```bash
cargo fuzz run search_usage -- -max_total_time=5
```

From the repo root, a short smoke run (~20 seconds) is:

```bash
scripts/fuzz.sh quick
scripts/fuzz.sh quick compile_only
```

Pass extra libFuzzer flags after `--`:

```bash
scripts/fuzz.sh run search_usage -- -runs=10000
```

On success, libFuzzer prints execution stats. On failure (crash, abort, or failed `assert` in a target), it exits non-zero and prints a reproducer; minimize with `cargo fuzz tmin` as needed.

## Workspace layout

The `fuzz/` crate is excluded from the root workspace (`exclude = ["fuzz"]` in the workspace `Cargo.toml`) so it stays a standalone `cargo-fuzz` package, similar in spirit to ripgrep’s `fuzz/` crate using its own `[workspace]` table.
