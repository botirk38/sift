# sift-core

Indexed, grep-style search over a directory tree: build a **trigram index** on disk, then run **regex** (Rust dialect) or **fixed-string** queries with optional narrowing from the index.

## Main API

- **`build_index(corpus_root, index_dir)`** — walk corpus (ignore rules via `ignore`), write `files.bin` / `lexicon.bin` / `postings.bin` + metadata.
- **`Index::open(index_dir)`** — load tables; holds corpus root path and file list.
- **`CompiledSearch::new(patterns, SearchOptions)`** — compile once; then **`search_index`** (indexed) or **`search_walk`** (walk + match, optional path subset).

## Internals (high level)

- **`planner`** — trigram plan: narrow candidates vs full scan.
- **`query`** — posting intersections over mmap-friendly byte slices.
- **`search`** — line scan, `-F` substring fast path, optional regex prefilter (`prefilter`), Rayon when candidate count and thread count justify it.
- **`index`** — trigram extraction (`trigram`), `build_trigram_index`.
- **`verify`** — pattern shaping (`-w`/`-x`/`-F`) and regex compilation.

## Features

- **`profile`** — enables `sift-profile` binary and `tempfile` for scripted benchmarks (`./scripts/profile.sh`).

## Dev

```bash
cargo test -p sift-core
cargo bench -p sift-core --bench search   # or ./scripts/bench.sh
```

See **`crates/core/benches/README.md`** for benchmark and profiling entry points.
