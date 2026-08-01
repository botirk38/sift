# Profiling log

Living record of system-profiler findings for sift. Criterion selects **which**
functions are slow; samply / Instruments / `sample` / xctrace decide **what** is
hot.

Search/planner fixtures default to monorepo scale (~32k files / ~5M lines;
`SIFT_BENCH_SCALE=stress` for ~64k / ~13M). See [`README.md`](README.md).

## Entry template

```markdown
### YYYY-MM-DD — <criterion id>

- **Tool:** samply | xctrace | Instruments | sample
- **Command:** `samply record -- cargo bench -p sift-core --bench <name> -- --profile-time 30 <id>`
  (or xctrace equivalent)
- **Wall-clock context:** <Criterion mean, if any>
- **Top stacks / attribution:**
  1. …
- **Attributed module:** `crates/…`
- **Proposed change:** …
- **Before / after:** …
```

## Sessions

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
