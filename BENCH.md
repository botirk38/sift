# Benchmarks and profiling

## Criterion (`sift-core`)

Statistical benchmarks live in [`crates/core/benches/search.rs`](crates/core/benches/search.rs):

- **`build_index/32_files`** — synthetic 32-file corpus, cold index each iteration.
- **`search_literal_narrow/beta_trigram_narrow`** — trigram narrowing.
- **`search_full_scan`** — `.*` and case-insensitive `beta` (full-tree scan).

```bash
./scripts/bench.sh
# or: cargo bench -p sift-core --bench search
```

**Settings:** 150 samples, 5 s warm-up, 10 s measurement window, 5% significance / noise thresholds. Baselines:

```bash
./scripts/bench.sh -- --save-baseline main
./scripts/bench.sh -- --baseline main
```

Reports under `target/criterion/`. The workspace **[`bench` profile](Cargo.toml)** uses `debug = 1` for symbolicated stacks.

### Indexer UTF-8 extraction (recent delta)

Index build uses [`extract_trigrams_utf8_lossy`](crates/core/src/index/trigram.rs): valid UTF-8 files take a fast path (`str::from_utf8` + trigrams) while invalid UTF-8 still matches `String::from_utf8_lossy` semantics (see unit tests there). After collecting sorted relative paths, **per-file read + trigram extraction** can run in **Rayon** when the file count is at least the same threshold as parallel candidate search (`parallel_candidate_min_files`, from logical CPUs and optional `RAYON_NUM_THREADS`). On one developer machine, Criterion **`build_index/32_files`** moved from about **12.6 ms** to about **10.2 ms** median between consecutive runs (noise applies; use `--save-baseline` / `--baseline` for rigorous comparisons).

### Search hot path (profiling-guided)

[`CompiledSearch`](crates/core/src/search.rs) compiles the regex once; [`search_files`](crates/core/src/search.rs) (used by [`CompiledSearch::search_walk`](crates/core/src/search.rs) / [`search_index`](crates/core/src/search.rs)) avoids cloning the full candidate path list when it is already lexicographically sorted. Line scanning uses a single reused `String` with [`BufRead::read_line`](https://doc.rust-lang.org/std/io/trait.BufRead.html#method.read_line) instead of [`lines`](https://doc.rust-lang.org/std/io/trait.BufRead.html#method.lines), so non-matching lines do not allocate. Criterion **`search_full_scan`** cases improved by roughly **10–17%** median vs the prior `lines()` + always-`to_vec` implementation on the same machine (variance applies).

**Also:** `-F` / fixed-string search without `-i`/`-w`/`-x` uses a **substring fast path** (skip `Regex::is_match` when emitting whole lines; `-o` still uses the regex for spans). In regex mode (not `-F`/`-i`/`-v`), a conservative **required-substring prefilter** (from `regex_syntax` HIR) skips `is_match` on lines that cannot match. **Posting intersections** merge directly over `postings.bin` slices instead of allocating a `Vec<u32>` per trigram in the hot path. **Candidate-based scans** (indexed full scan and narrowed file lists) use **Rayon** when the candidate count is at least the **effective worker count** `min(logical CPUs, RAYON_NUM_THREADS)` when `RAYON_NUM_THREADS` is a positive integer, otherwise the logical CPU count from [`std::thread::available_parallelism`](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html), **`max_results` is `None`**, and that effective count is greater than one — then hits are sorted for deterministic order.

Remaining time in search is still dominated by **regex** (when used) and **IO**; use Linux `perf` to pick the next change.

### Linux `perf` / symbols (recommended for hotspot names)

For readable stacks (often better than macOS Time Profiler alone), build with frame pointers and record with `perf`:

```bash
export RUSTFLAGS='-C force-frame-pointers=yes'
cargo build --profile profiling -p sift-core --features profile --bin sift-profile
perf record -g -- ./target/profiling/sift-profile narrow
perf report
```

Use a fixed workload (`SIFT_LOOP_SECS`, `SIFT_LARGE`, etc.) when comparing before/after refactors.

## Profiling (single binary + script)

There is **one** profiling binary: [`crates/core/src/bin/profile.rs`](crates/core/src/bin/profile.rs) (Cargo name **`sift-profile`**, built with `--features profile`). Use **`./scripts/profile.sh`** for everything:

```bash
# Tab-separated metrics on stdout (default mode: narrow; default SIFT_ITERS=2_000_000 for search)
./scripts/profile.sh

# Shorthand: mode only
./scripts/profile.sh full_dotstar

# Explicit
./scripts/profile.sh metrics narrow
./scripts/profile.sh metrics build

# Flamegraph (~30 s hot loop; requires: cargo install flamegraph)
./scripts/profile.sh flamegraph narrow
```

**Loop control:** `SIFT_ITERS` (default 2_000_000 tiny corpus / 5000 large search), `SIFT_LOOP_SECS` (overrides `SIFT_ITERS` for search; `flamegraph` sets 30 s unless set; not used for `build`).

**Large codebase simulation** (for search and `build`):

| Variable | Meaning |
|----------|---------|
| `SIFT_LARGE=1` | Use defaults: ~8000 files, ~100 lines each, 256 crate dirs (`crates/cNNNN/src/module_*.rs`). |
| `SIFT_CORPUS_FILES` | Set explicitly to enable large layout (omit or `0` for tiny 2-file search / 32-file `build` parity). |
| `SIFT_CORPUS_LINES` | Lines per synthetic file (default 120 if `SIFT_CORPUS_FILES` set, 100 if `SIFT_LARGE` only). |
| `SIFT_CORPUS_DIRS` | Crate fan-out: files spread across this many `crates/cXXXX/` trees (default 256). |

Default `SIFT_ITERS` drops to **5000** on large search corpora (still override). `build` defaults to **2** iterations on large (cap **20**); parity `build` defaults to **500** (cap **500**).

**Metric lines:** include `phase_materialize_corpus_ms`, `corpus_kind`, `corpus_files`, and for large runs `corpus_lines_per_file`, `corpus_dir_fanout`, plus `phase_build_index_ms`, `phase_open_index_ms`, `mode`, `iters`, `total_ms`, `ns_per_iter`.

`flamegraph.svg` / `cargo-flamegraph.trace` at the repo root are gitignored.

### CLI end-to-end

```bash
cargo build --release -p sift-cli
cargo flamegraph --bin sift -- --index /path/to/.idx 'pattern'
```

### Where time goes

1. **`build_index`** — trigram extraction + lexicon/postings IO.
2. **Full-scan search** — [`CompiledSearch::search_index`](crates/core/src/search.rs) / [`search_walk`](crates/core/src/search.rs) over the full tree: `ignore` walk + per-line regex.
3. **Narrow search** — posting intersection, then scan on a file subset only.
