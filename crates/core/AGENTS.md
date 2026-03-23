# Agent notes (sift-core)

## Crate boundaries

Public API is re-exported from the `sift_core` lib root (`lib.rs`): `build_index`, `Index`, `CompiledSearch`, `SearchOptions`, `TrigramPlan`, `walk_file_paths`, storage helpers as needed.

## Source map

| Module / dir | Responsibility |
|--------------|----------------|
| `index/` | Walk corpus, extract trigrams, write/read `files.bin`; parallel per-file read+extract when `paths.len() >= parallel_candidate_min_files()` |
| `planner.rs` | `TrigramPlan::for_patterns` — literal/alternation → narrow arms or full scan |
| `query.rs` | `Index::candidate_paths`, sorted posting merges |
| `search.rs` | `CompiledSearch`, `search_files`, `scan_lines`, parallel candidate scans, `parallel_candidate_min_files()` |
| `prefilter.rs` | Regex HIR → necessary substring checks (skipped for `-F`/`-i`/`-v`) |
| `verify.rs` | `pattern_branch`, `compile_search_pattern` |
| `storage/` | Lexicon/postings/files binary layout |
| `bin/profile.rs` | `sift-profile` — feature `profile` only |

## Invariants worth preserving

- **Determinism:** parallel search merges hits sorted by `(file, line, text)`.
- **Index file order:** lexicographic relative paths after sort (stable file ids).
- **Rayon gating:** same effective-worker heuristic for parallel **search** (sorted candidates) and parallel **index** extraction (`RAYON_NUM_THREADS` + `available_parallelism`).

## Tests

Integration-style tests live in `lib.rs` `mod tests`; unit tests are co-located in modules (`search.rs`, `prefilter.rs`, etc.). Run `cargo test -p sift-core`.
