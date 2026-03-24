# Agent notes (sift-core)

## Crate boundaries

Public API is re-exported from the `sift_core` lib root (`lib.rs`): `Index`, `IndexBuilder`, `QueryPlan`, `CompiledSearch`, `SearchOptions`, `TrigramPlan`, `walk_file_paths`, storage helpers as needed.

## Source map

| Module / dir | Responsibility |
|--------------|----------------|
| `index/` | `Index`, `IndexBuilder`, walk corpus, extract trigrams, write/read persistence files; posting-list helpers |
| `index/builder.rs` | `build_index_tables` — in-memory trigram table construction from corpus |
| `index/trigram.rs` | `extract_trigrams`, `extract_trigrams_utf8_lossy` |
| `index/files.rs` | read/write `files.bin` |
| `planner.rs` | `TrigramPlan::for_patterns` — literal/alternation → narrow arms or full scan |
| `search.rs` | `CompiledSearch`, `search_files`, `scan_lines`, parallel candidate scans, `parallel_candidate_min_files()` |
| `prefilter.rs` | Regex HIR → necessary substring checks (skipped for `-F`/`-i`/`-v`) |
| `verify.rs` | `pattern_branch`, `compile_search_pattern` |
| `storage/` | Lexicon/postings binary layout |
| `bin/profile.rs` | `sift-profile` — feature `profile` only |

## Invariants worth preserving

- **Determinism:** parallel search merges hits sorted by `(file, line, text)`.
- **Index file order:** lexicographic relative paths after sort (stable file ids).
- **Rayon gating:** same effective-worker heuristic for parallel **search** (sorted candidates) and parallel **index** extraction (`RAYON_NUM_THREADS` + `available_parallelism`).

## Tests

Integration-style tests live in `lib.rs` `mod tests`; unit tests are co-located in modules (`search.rs`, `prefilter.rs`, etc.). Run `cargo test -p sift-core`.
