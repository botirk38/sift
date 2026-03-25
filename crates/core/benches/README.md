# Benchmarks

`crates/core/benches/search.rs` contains the Criterion benchmark suite for sift-core.
`crates/core/src/bin/profile.rs` contains the `sift-profile` binary for hot-loop profiling.

## Scenario matrix

The benchmark matrix is divided into three categories that reflect the runtime
execution paths:

### Query-planning (trigram/verify paths)

| Scenario | Pattern | SearchOptions |
|---|---|---|
| `literal_narrow` | `beta` | default (narrowable) |
| `literal_narrow_large` | `beta` | default, 8k files |
| `word_literal` | `beta` | `WORD_REGEXP` |
| `line_literal` | `beta` | `LINE_REGEXP` |
| `fixed_string` | `beta.gamma` | `FIXED_STRINGS` |
| `casei_literal` | `beta` | case-insensitive |
| `smart_case_lower` | `beta` | smart-case (lowercase → ci) |
| `smart_case_upper` | `Beta` | smart-case (uppercase → cs) |
| `required_literal` | `[A-Z]+_RESUME` | default (requires trigram) |
| `no_literal` | `\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}` | full scan |
| `alternation` | `ERR_SYS\|...` | default |
| `alternation_casei` | `ERR_SYS\|...` | case-insensitive |
| `unicode_class` | `\p{Greek}` | default |

### Filter + query (SearchFilter paths)

| Scenario | Filter | Corpus |
|---|---|---|
| `glob_include` | `**/*.txt` glob | filter_corpus |
| `glob_exclude` | `!**/*.txt` glob | filter_corpus |
| `glob_casei` | `**/*.TXT` ci-glob | filter_corpus |
| `hidden_default` | `HiddenMode::Respect` | filter_corpus |
| `hidden_include` | `HiddenMode::Include` | filter_corpus |
| `ignore_default` | DOT+VCS+EXCLUDE | filter_corpus |
| `ignore_custom` | custom `.ignore` file | filter_corpus |
| `scoped_search` | scope: `subdir/` | filter_corpus |

### Output-mode (run_index mode branches)

| Scenario | SearchMode | Notes |
|---|---|---|
| `only_matching` | `OnlyMatching` | `-o` equivalent |
| `count` | `Count` | `-c` equivalent |
| `count_matches` | `CountMatches` | `--count-matches` |
| `files_with_matches` | `FilesWithMatches` | `-l` equivalent |
| `files_without_match` | `FilesWithoutMatch` | `-L` equivalent |
| `max_count_1` | `Standard` | `-m 1` per-file cap |

## Corpus fixtures

- **parity**: 2 files (`a/x.txt`, `b/y.txt`) — fast turnaround for quick iteration
- **filter_corpus**: 12 files with mixed extensions, hidden files, scoped subdirs,
  `.gitignore`, and `.ignore` markers — exercises all filter branches
- **large**: ~8k files × 100 lines across 256 crate dirs — for statistical significance
  on warm caches; enable with `SIFT_LARGE=1`

## Running

```bash
# Criterion (statistical)
cargo bench -p sift-core --bench search
./scripts/bench.sh

# Save / compare baselines
./scripts/bench.sh -- --save-baseline main
./scripts/bench.sh -- --baseline main

# sift-profile (hot-loop, tab-separated metrics)
cargo run -p sift-core --features profile --bin sift-profile -- list
./scripts/profile.sh list

./scripts/profile.sh literal_narrow
./scripts/profile.sh glob_include
./scripts/profile.sh count
./scripts/profile.sh files_with_matches

# Use large corpus
SIFT_LARGE=1 ./scripts/profile.sh literal_narrow

# Flamegraph
./scripts/profile.sh flamegraph literal_narrow

# Custom corpus
SIFT_PROFILE_CORPUS=/path/to/repo ./scripts/profile.sh literal_narrow
```

## Environment variables

| Variable | Effect |
|---|---|
| `SIFT_LARGE=1` | Use large corpus (8k files) |
| `SIFT_CORPUS_FILES=N` | Custom file count |
| `SIFT_CORPUS_LINES=N` | Lines per file (large corpus) |
| `SIFT_CORPUS_DIRS=N` | Directory fan-out (large corpus) |
| `SIFT_FILTER_CORPUS=1` | Force filter_corpus in profile (parity default) |
| `SIFT_ITERS=N` | Fixed iteration count |
| `SIFT_LOOP_SECS=N` | Run each scenario for N seconds |
| `SIFT_PROFILE_CORPUS` | Use external corpus path |
| `SIFT_PROFILE_INDEX` | Index directory (default: `<corpus>.sift`) |
