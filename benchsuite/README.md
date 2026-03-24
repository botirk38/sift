# Benchsuite

Adapted from the ripgrep benchsuite. Benchmarks `sift` against `ripgrep` on
real-world code-search workloads.

## Prerequisites

- `ripgrep` (`rg`) in PATH
- `sift` binary at `../target/release/sift` (or set `--sift-binary`)
- Corpus downloads require `git`, `curl`, `gunzip`

## Download corpora

```bash
# Linux kernel (~1 GB clone + build artifacts)
python3 benchsuite/benchsuite --download linux

# English OpenSubtitles sample (~500 MB)
python3 benchsuite/benchsuite --download subtitles-en

# Russian OpenSubtitles (~1 GB)
python3 benchsuite/benchsuite --download subtitles-ru

# All three
python3 benchsuite/benchsuite --download all
```

## Run benchmarks

```bash
# List all benchmarks
python3 benchsuite/benchsuite --list

# Run all benchmarks with default settings (1 warmup, 3 iterations)
python3 benchsuite/benchsuite --dir /tmp/benchsuite

# Run a specific benchmark family
python3 benchsuite/benchsuite --dir /tmp/benchsuite linux_literal

# More warmup/iteration runs
python3 benchsuite/benchsuite --dir /tmp/benchsuite --warmup-iter 3 --bench-iter 5

# Save raw CSV results
python3 benchsuite/benchsuite --dir /tmp/benchsuite --raw /tmp/results.csv
```

## How indexing works

`sift` requires a per-corpus index. The benchsuite builds each index once on first
use (Linux kernel → `linux/.sift/`, subtitles → `subtitles/.sift/`). The index
is cached — subsequent benchmarks on the same corpus reuse it without rebuilding.

## Custom sift binary

```bash
python3 benchsuite/benchsuite --sift-binary /path/to/sift --dir /tmp/benchsuite
```
