# Profiling log

Living record of system-profiler findings for sift. Criterion selects **which**
functions are slow; `perf` (Linux) / samply / xctrace decide **what** is hot.

Search/planner fixtures in `common/mod.rs` currently rematerialize an 8k×100
temp corpus per process (README scale table is stale). See [`README.md`](README.md).

## Entry template

```markdown
### YYYY-MM-DD — <criterion id>

- **Tool:** perf | samply | xctrace | Instruments | sample
- **Command:** `perf record -F 999 --call-graph fp -e task-clock -o <file>.perf -- <argv>`
  (Linux; wrap Criterion with `--profile-time 30`)
- **Wall-clock context:** <Criterion mean, if any>
- **Top stacks / attribution:**
  1. …
- **Attributed module:** `crates/…`
- **Proposed change:** …
- **Before / after:** …
```

## Sessions

### 2026-08-20 — PR1 exhaustive print via `execute`

- **Tool:** benchsuite + `perf record -F 999 --call-graph fp -e task-clock`
- **Change:** CLI Normal listing (exhaustive, no context/passthru) uses `Searcher::execute` then `EventRenderer::files`. `FileReport` keeps `MatchEvent`s.
- **Wall-clock (benchsuite, 1 warmup + 3 iter, same host/corpus as session below):**

| Id | before | after | rg |
|----|--------|-------|-----|
| `linux_no_literal` | 4.034 s | 2.254 s | 1.560 s |
| `linux_unicode_greek` | 4.394 s | 2.764 s | 1.885 s |
| `linux_unicode_greek_casei` | 4.333 s | 2.602 s | 1.090 s |
| `linux_alternates_casei` | 1.406 s | 0.814 s | 0.751 s |
| `linux_literal` | 0.423 s | 0.269 s | 0.338 s |

Correctness 11/11. `linux_literal_default` showed high variance (1.711 ± 2.551); `-n` literal is stable.

- **CLI `linux_no_literal` after:** 2.71 s elapsed, **2.059 CPUs** (was 0.995). Leaves: `find_fwd` 49.5%; `Rust::matched` 10.9%; `Pool::get_slow` 6.5%; `Pool::put_value` 6.0%. Caller is `Searcher::execute` / Rayon (was `Events::next`).
- **CLI `linux_alternates_casei` after:** 0.71 s, **2.335 CPUs**. Top leaf `Pool::put_value` 20.6% (PR4).
- **CLI `linux_literal` after:** 0.24 s, **1.513 CPUs**. Leaves: `Rust::matched` 19.7%; `Pool::put_value` 16.1%; `validate_lexicon_postings` 9.5%; `decode_sorted` 6.1% (PR2).
- **Before / after:** full-scan listing is now parallel. Remaining CLI gap vs rg is regex (`find_fwd`) plus matcher cache pool.

### 2026-08-20 — linux benchsuite + Criterion (`perf` on modal-dev)

- **Host (verified this machine):** Linux 4× Xeon Platinum 8375C @ 2.90 GHz, NVMe. `perf_event_paranoid=2`; hardware `cycles`/`instructions` unsupported in the guest (`task-clock` sampling used). Corpus is BurntSushi/linux source-only (`--depth 1`, **no `vmlinux`**). Do not mix with `docs/perf/linux-summary.csv` (macOS, different corpus).
- **Build:** `CARGO_PROFILE_RELEASE_DEBUG=1 RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release -p sift-grep`
- **Wall-clock (benchsuite, 1 warmup + 3 iter, `/usr/bin/rg` 14.1.1, sift 0.8.1):**

| Id | sift mean | rg mean | sift/rg |
|----|-----------|---------|---------|
| `linux_unicode_greek` | 4.394 s | 1.513 s | 2.90× |
| `linux_unicode_greek_casei` | 4.333 s | 0.950 s | 4.56× |
| `linux_no_literal` | 4.034 s | 1.486 s | 2.71× |
| `linux_alternates_casei` | 1.406 s | 0.699 s | 2.01× |
| `linux_alternates` | 0.808 s | 0.503 s | 1.61× |
| `linux_unicode_word` | 0.669 s | 0.323 s | 2.07× |
| `linux_re_literal_suffix` | 0.598 s | 0.463 s | 1.29× |
| `linux_literal` | 0.423 s | 0.527 s | 0.80× |
| `linux_word` | 0.291 s | 0.430 s | 0.68× |

Correctness 11/11 vs rg. `linux_literal_casei` in the Python runner omits `-i` on both tools (not a product bug).

#### CLI `linux_no_literal` / `linux_unicode_greek` (full-scan fallback)

- **Tool:** `perf record -F 999 --call-graph fp` + `perf stat`
- **Command:** `target/release/sift --sift-dir /tmp/benchsuite/linux.sift -n '<pattern>'` (cwd linux tree)
- **Wall-clock context:** ~4.09 s / ~4.36 s elapsed. **0.995 CPUs utilized.** ~104k page-faults. 3.2–3.5 s user, 0.80–0.84 s sys.
- **Top stacks / attribution (leaf, `no_literal`):**
  1. `regex_automata::hybrid::search::find_fwd` 63.7%
  2. `sift_core::search::scan::FileScan::next_item` 6.5%
  3. `memchr` AVX2 5.0%
  4. `sift_core::search::matcher::rust::Rust::matched` 4.2%
  5. `__memmove_evex_unaligned_erms` 3.7% (under `PrintSpec::print`)
- **Callers:** `Rust::matched` → `FileScan::next_item` → `Events::next` → `sift_grep::format::output::PrintSpec::print` → `Run::execute`. **No rayon** on this path.
- **Attributed module:** `crates/cli/src/format/output/mod.rs` (`OutputEmission::Normal` pulls `Searcher::stream`). `Searcher::execute` already `par_iter`s exhaustive mode (`crates/core/src/search/searcher.rs`) but the default CLI listing does not use it.
- **Proposed change:** Parallelize default listing (or print from `execute` reports) so full-scan uses the 4 cores rg already uses. Secondary: regex-automata hybrid DFA cost is expected for `\w{5}\s+…` / `\p{Greek}` (no n-gram literal).
- **Before / after:** not changed this session.

#### CLI `linux_alternates_casei`

- **Tool:** `perf record --call-graph fp`
- **Wall-clock context:** 1.09 s elapsed, **0.999 CPUs**, 17k page-faults.
- **Top stacks:**
  1. `aho_corasick::packed::teddy` FatAVX2 `Searcher::find` 23.8%
  2. `memchr` AVX2 11.8%
  3. `FileScan::next_item` 9.3%
  4. `regex_automata::hybrid::search::find_fwd` 7.0%
  5. `Index::validate_lexicon_postings` 3.2%
- **Attributed module:** still the serial `PrintSpec` / `Events` loop, scanning many files (case-insensitive alt narrowing is weak). Teddy/aho-corasick is the matcher, not a planner bug.
- **Proposed change:** Same listing parallelism as full-scan; plus tighter case-insensitive n-gram candidate narrowing so fewer files reach teddy.

#### CLI `linux_literal` (control, 12× loop)

- **Tool:** `perf record --call-graph fp`
- **Wall-clock context:** single run 0.275 s, 0.998 CPUs, 6.8k page-faults.
- **Top stacks:**
  1. `Postings::decode_sorted` 13.9%
  2. `Index::validate_lexicon_postings` 12.5%
  3. `integer_encoding::VarInt::decode_var` 11.3%
  4. `__memmove` 9.2% (print + validation)
  5. `memchr` 7.8% (actual search)
- **Attributed module:** `crates/core/src/index/ngram/index.rs` — `Index::open` calls `validate_lexicon_postings`, which decodes **every** posting list and checks `decoded_count == entry.len`. That runs on every CLI `Indexes::load`.
- **Proposed change:** Keep validation at `write_tables` / build; on open, trust checksum or a cheap length check. This is the leftover cost on already-narrowed literals.

#### Criterion `grep` (`--profile-time 30`, ids from `--list`)

Actual ids: `grep_indexed/{literal,required_literal,alternation,case_insensitive,case_insensitive_alternation,full_scan_fallback,invert_match}`, `grep_walk/{literal,full_scan}`. Setup rematerializes 8k×100 files each process.

`perf stat` (8 s profile): **2.9–3.5 CPUs utilized** (Rayon `Searcher::execute`). Contrast with CLI 1.0 CPU.

| Id | Top leaves | Notes |
|----|------------|-------|
| `grep_indexed/full_scan_fallback` | `find_fwd` 50.6%; `regex_automata::util::pool::Pool::put_value` ~20% combined; `Rust::matched` 6.8% | Regex + matcher cache pool under rayon |
| `grep_indexed/case_insensitive_alternation` | `Pool::put_value` 15%+10%; `Rust::matched` 12.6%; teddy 8.1%; `find_fwd` 5.1% | Same pool; teddy as CLI |
| `grep_indexed/literal` | `Pool::put_value` 16%+13%; `Rust::matched` 12.5%; memchr 10.3% | No `validate_lexicon_postings` (index already open outside `b.iter`) |
| `grep_indexed/invert_match` | `search_one` 10.3%; `Rust::matched` 10.0%; `FileScan::next_item` 10.0%; `malloc`/`_int_free` ~16% | Alloc-heavy invert listing; **not** `__open` / `grep-searcher` |

- **Proposed change (library path):** thread-local / less contended regex cache (`Pool::put_value`); invert path: fewer per-line `MatchEvent` allocations. Do not chase 2026-07-09 `__open`/`grep-searcher` — that code is gone and was not hot here.
- **Before / after:** not changed this session.

#### Heap

Skipped `heaptrack`/`DHAT`. Invert Criterion leaves are malloc-bound (~25% libc alloc/free); CLI full-scan is not (`memmove` 4%, `find_fwd` 64%).

### 2026-08-16 — own scan loop (grep-searcher removed)

- **Tool:** none yet (implementation change; re-profile before claiming wins)
- **Change:** `Searcher` reads via `Bytes` + `fastio` (`Io::{Sync,Mmap,Uring}` all `read_all` / `map`). No windowed uring. Rayon over files.
- **Expected still:** per-file `__open` dominates on macOS (fastio uring open is still `std::fs` open; mmap/sync also open per file).
- **Before / after:** not measured this session. Re-run `grep_search/full_scan` and `invert_match` with samply/xctrace after the next dedicated profiling pass.

### 2026-07-09 — grep_search/invert_match

- **Tool:** xctrace Time Profiler (samply failed: macOS debugger entitlement / codesign blocked in agent environment)
- **Command:** `xctrace record --template "Time Profiler" --time-limit 25s --no-prompt --launch -- <grep-bench> --bench --profile-time 20 -- grep_search/invert_match`
- **Trace:** `target/sift-invert.trace`
- **Wall-clock context:** Criterion mean ~124–126 ms (`pre-opt`)
- **Top stacks / attribution (leaf weight, Running):**
  1. `__open` ~61600 ms sample-weight — file open in `grep_searcher::Searcher::search_path`
  2. `_xzm_xzone_malloc_tiny` / `_xzm_free` / `_platform_memmove` — alloc/copy
  3. `sift_core::search::task::MatchSink::matched` ~2378 ms — per-line match recording
  4. `read` ~4109 ms — file read
  5. `grep_searcher::lines::count` / `match_by_line` / `memchr::memmem` — line scan
- **Attributed module:** `crates/core/src/search/task.rs` + `grep-searcher` path I/O
- **Proposed change:** Tried lazy `SearchEvent` construction / skip span scan on Discard+Lines; also considered `memory_map(Auto)` on `RegexSearcherBuilder`.
- **Before / after:**
  - Lazy Discard path: Criterion change not significant (`invert` +1% p=0.77; `literal` +2% p=0.38). Reverted — wall-clock dominated by `__open`.
  - `grep-searcher` `MmapChoice::auto` is a **no-op on macOS** (`mmap.rs` returns `None` under `cfg!(target_os = "macos")`), so enabling mmap cannot help this host.
- **Status:** Deferred. Remaining cost is per-file open/read across ~8k invert hits; needs a different strategy (batching, reuse, or platform-specific I/O) with a fresh profile before coding.

### 2026-07-09 — grep_pipeline/full_scan

- **Tool:** xctrace Time Profiler
- **Command:** `xctrace record --template "Time Profiler" --time-limit 25s --no-prompt --launch -- <grep-bench> --bench --profile-time 20 -- grep_pipeline/full_scan`
- **Trace:** `target/sift-pipeline-fullscan.trace`
- **Wall-clock context:** Criterion mean ~100 ms (`pre-opt` family)
- **Top stacks / attribution (leaf weight, Running):**
  1. `__open` ~74300 ms — per-file open (full corpus scan)
  2. `regex_automata::hybrid::search::find_fwd` ~14300 ms — regex engine
  3. `read` ~7300 ms — file read
  4. `close` / `madvise` / unlink — FS churn
- **Attributed module:** `grep-searcher` path I/O + `regex-automata` (full-scan pattern has no index narrowing)
- **Proposed change:** None cheap in sift; same open-bound story as invert. Regex cost is secondary and lives in the automata crate.
- **Before / after:** Deferred with evidence — no product change.

### 2026-07-09 — Criterion queue (grep + candidates)

Slowest first after redesign baseline:

| Id | Mean (approx) | Profile status |
|----|---------------|----------------|
| `grep_search/invert_match` | ~125 ms | Profiled; deferred (`__open`) |
| `grep_pipeline/full_scan` | ~100 ms | Profiled; deferred (`__open` + regex) |
| `grep_search/*` (literal family) | ~77–86 ms | Not yet (same open path likely) |
| `candidate_planner/*` | µs–ms | Planner-only; not wall-clock bottleneck |
