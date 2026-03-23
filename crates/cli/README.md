# sift-cli

Command-line **`sift`**: index a corpus, then search it with grep-like flags (`-e`, `-F`, `-i`, `-v`, `-w`, `-x`, `-o`, `-m`, …).

## Relationship to core

Depends only on **`sift-core`**. The binary parses flags with **clap**, maps them to `SearchOptions` / `SearchMatchFlags`, opens the index from `--index`, and prints matches.

## Run

```bash
cargo run -p sift-cli -- --help
cargo run -p sift-cli -- --index .index build /path/to/corpus
cargo run -p sift-cli -- --index .index PATTERN [PATH...]
```

Release binary name: **`sift`** (`Cargo.toml` `[[bin]]`).

## Tests

Integration smoke tests: `crates/cli/tests/cli_smoke.rs` — `cargo test -p sift-cli`.
