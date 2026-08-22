# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Features

- own the per-file scan loop (`Bytes` + `Lines`); `--io sync|mmap|uring` replaces `--mmap`/`--no-mmap`

### Refactor

- the posting-list container moved to `index/postings.rs` so index kinds can share it; the `sift_core::index::ngram::storage::postings` path is gone

- `Searcher::execute` materializes a report; `Searcher::stream` returns `Events`. Quiet/invert-match are `bool`.
- compile `Matcher::Pcre2` with the `pcre2` crate; drop `grep-pcre2` and `grep-matcher` (#279)
- `Matcher::spans` is the match walk; `MatchEvent` carries `Span`; `Query::case()` is caseless for both engines

## [0.8.1](https://github.com/botirk38/sift/releases/tag/v0.8.1) — 2026-08-15

### Bug Fixes

- scope --files PATH like ripgrep (#276)
- --debug scan-scope walk when index absent (#275)
- piped stdin with no paths searches stdin (rg parity) (#274)
- omit JSON begin/end for zero-match files (#273)

### Documentation

- stop steering agents toward SIFT_NO_DAEMON (#268)

## [0.8.0](https://github.com/botirk38/sift/releases/tag/v0.8.0) — 2026-08-15

### Bug Fixes

- emit ripgrep-style single-line binary match notice (#266)
- --files --stats must not inflate match counts (#265)
- keep --json --stats quiet on stderr (#264)
- -e/-f leftover positionals are search paths (#255)
- keep --json quiet unless --stats is set (#253)
- print NUL in binary match notice (#249)
- report match tally for -l in --stats (#254)
- add -P alias and correct PCRE2 skill docs (#252)

### Documentation

- drop rg-compat-matrix links; track parity as issues (#257)
- update README architecture nouns for 0.7 (#250)
- revamp sift skill for 0.7 index rebuild semantics (#240)

### Features

- add --debug stderr diagnostics (#267)

### Refactor

- tighten CorpusWatcher entity boundaries (#263)
- native-or-poll daemon watcher backend (#262)

### Deps

- bump globset from 0.4.19 to 0.4.20
- bump ignore from 0.4.31 to 0.4.33
- bump clap from 4.6.5 to 4.6.6

## [0.7.0](https://github.com/botirk38/sift/releases/tag/v0.7.0) — 2026-08-15

### Bug Fixes

- search load must not create store; error on dangling CURRENT
- default focused tools; restore ref after --ab (#218)
- narrow -c count when zero files are omitted (#170)
- keep index narrowing under default Auto encoding (#169)
- avoid unused matcher clone (#142)
- delegate matcher fast paths (#141)

### Documentation

- never allow free helpers or FnOnce callback APIs (#188)
- prefer sweeping architecture fixes over backward compat (#185)
- strengthen composition and simplicity rules in AGENTS.md (#168)
- add Cursor Cloud setup notes to AGENTS.md (#145)

### Features

- pure Plan resolve and Option Indexes::load (#229)
- typed IndexRecord, Snapshot Files, domain index modules
- pure Plan resolve and Option Indexes::load
- SearchQuery owns index narrowing (#228)
- opt-in GramNorm::AsciiLower on ngram Index (#216)
- unify Index trait and merge IndexStore into Indexes
- mode-shaped Listing report + deferred hydrate + Arc event paths (#219)
- generalize trigram index to ngram (#125)
- add lazy index coverage (#112)

### Miscellaneous

- drop profile/update/bench wrappers; keep essential scripts (#227)
- backup local WIP before Mac handoff
- expand profile.sh for cloud VM system profilers (#212)
- add scripts/profile.sh for system profiler workflow (#206)
- remove junk markdown

### Performance

- borrow Index::rel_path instead of allocating PathBuf (#217)
- reuse display path for relative InputIdentity hit paths (#213)
- skip per-file path filter for single-index AllIndexed ids (#211)
- skip Searcher compile in Grep::resolve_candidates (#210)
- avoid HashSet clone in single-index IndexedCorpus intersection (#209)
- attach IndexedCorpus coverage to candidate plans (#199)
- count match spans without building Match values when discarding (#201)
- drop merge dedup HashSet for unindexed walk paths (#200)
- move query into Searcher without cloning in Grep::execute (#197)
- defer indexed candidate materialization for FirstMatch (#194)
- materialize FirstMatch inputs progressively (#193)
- discard search events for summary print modes (#192)
- skip MatchSink path clone for discarded presence counts (#191)
- filter during indexed materialize in one pass (#189)
- reuse RegexSearcher per rayon worker (#190)
- skip search event construction when discarding (#187)
- skip ignore re-check for trusted indexed candidates (#186)
- defer candidate materialization to resolve (#184)
- narrow short literals via covering N-grams (#183)
- skip MatchSink span scan for presence and line counts (#181)
- defer files.bin fingerprint decode until needed (#180)
- parallelize ngram candidate materialization (#179)
- bitset-union ASCII case-fold posting windows (#178)
- halt file scan after first hit for -l/-L (#173)
- stop quiet search after the first selected hit (#172)
- defer indexed path coverage until needed (#171)
- ASCII case-fold grams for -i candidate narrowing (#167)
- pack posting sort key into u64 for width<=4 (#158)
- dedup grams before sorting in GramSet::collect (#157)
- SIMD-bitpack posting lists in 128-value blocks (#156)
- decode posting varints by index, presize output (#155)
- borrow matcher in sink instead of cloning per file (#154)
- parallelize posting assembly with packed sort key (#153)
- skip walk for validated index snapshots (#111)

### Refactor

- daemon ipc/watcher/refresh split and surface cull (#233)
- Origin{File,Stream}, resolve-once Run, SearchMode::Paths (#232)
- sole FileIdentity; drop InputIdentity and PrintExtras
- delete Grep; Query and Candidate file identity
- ngram::Index always opened; knobs only on IndexRecord
- drop IndexWrite; build/update take dest and config
- collapse candidate/search adapter layers
- rename hydrate helpers to candidate
- split grep search pipeline (#144)
- remove grep wrapper clutter (#143)
- revamp grep pipeline with entity-based search API (#140)
- rework grep architecture (#136)

### Testing

- remove index walk parity harness (#137)

### Compat

- report binary matches like ripgrep (#139)
- match ripgrep type semantics (#138)
- support ripgrep decoration buffering flags (#133)
- support compressed and preprocessed search (#131)
- support ripgrep sort flags (#130)
- support PCRE2 regex engine (#129)
- support stdin search semantics (#128)
- support ripgrep config files (#127)
- support encoding and null-data search (#126)

### Deps

- bump serde from 1.0.228 to 1.0.229 (#225)
- bump serde_json from 1.0.150 to 1.0.151 (#223)
- bump grep-matcher from 0.1.8 to 0.1.9 (#222)
- bump bstr from 1.12.3 to 1.13.0 (#224)
- bump grep-pcre2 from 0.1.9 to 0.1.10 (#226)
- bump ignore from 0.4.27 to 0.4.28 (#208)
- bump memchr from 2.8.2 to 2.8.3 (#207)
- bump ignore from 0.4.26 to 0.4.27 (#159)
- bump bstr from 1.12.1 to 1.12.3 (#134)
- bump anyhow from 1.0.102 to 1.0.103 (#135)

### Merge

- bring master into search-pipeline stack

## [0.6.0](https://github.com/botirk38/sift/releases/tag/v0.6.0) — 2026-06-24

### Bug Fixes

- install.sh success output for legacy releases and cargo fallback

### Documentation

- benchsuite fix, benchmark charts, README revamp, agent/human install (#107)
- communicate composable index vision across README, AGENTS.md, and rustdoc (#104)

### Features

- make `sift index build` async by default (remove --lazy) (#103)

### Miscellaneous

- release v0.6.0
- bump actions/checkout from 6 to 7 (#101)
- Rust best-practices and architecture improvements (#106)
- release v0.5.1 (#105)

### Performance

- domain types, metadata caching, lazy stats (#108)

### Deps

- bump memmap2 from 0.9.10 to 0.9.11 (#102)

## [0.4.0](https://github.com/botirk38/sift/releases/tag/v0.4.0) — 2026-06-19

### Bug Fixes

- stabilize `integration_update` CI tests (Windows + Ubuntu) (#92)

### Features

- v0.4 with CLI-only daemon and lazy index build (#99)
- add `sift index build --lazy` for deferred index construction (#94)
- sift update for binary upgrades and sift index namespace (#85)
- revamp agent skill for using sift (not developing it) (#84)

### Miscellaneous

- remove banned #[allow] attributes from bench modules (#93)
- remove dead SnapshotRead::id and SnapshotWriterSession::current_id trait methods (#89)
- consolidate duplicated `mmap_open` into `storage::mmap` (#90)
- replace once_cell with std::sync::OnceLock (#91)
- refactor sift-grep CLI around domain types (#86)

### Deps

- bump bitflags from 2.12.1 to 2.13.0 (#95)
- bump ignore from 0.4.25 to 0.4.26 (#96)
- bump regex-syntax from 0.8.10 to 0.8.11 (#97)
- bump memchr from 2.8.1 to 2.8.2 (#98)
- bump memchr from 2.8.0 to 2.8.1 (#88)
- bump bitflags from 2.11.1 to 2.12.1 (#87)

## [0.3.0](https://github.com/botirk38/sift/releases/tag/v0.3.0) — 2026-06-02

### Bug Fixes

- stage sift-core path dep bump in release script
- reconcile on startup to catch changes made while daemon was down (#71)
- configure WalkBuilder from VisibilityConfig in no-index search (#59)

### Features

- add idle timeout to daemon + redesign coordinator state machine (#72)
- concurrent daemon and multi-reader-single-writer index store (#68)
- add QueryPlanner, clean up index API, split regression tests (#60)

### Miscellaneous

- bump sift-core path dep during release

### Performance

- 2-pass trigram-only radix sort for posting assembly (#83)
- use incremental update in CLI when index exists (#80)
- use thread-local bitset for trigram dedup in from_bytes (#81)
- stream-decode posting lists during intersection (#82)
- defer content-level validation on index open (#78)
- drop redundant result sort in scan workers (#65)
- skip path work when no ignore rules apply (#66)
- read files for trigram extraction instead of mmap (#67)

### Refactor

- reorganize index and trigram modules by domain (#64)

## [0.2.0](https://github.com/botirk38/sift/releases/tag/v0.2.0) — 2026-05-29

### Bug Fixes

- rename sift-cli to sift-grep for crates.io publish
- add version to sift-core dep, split publish jobs
- prune gitignored directories during index build (#58)
- redesign daemon event loop with RefreshState, remove is_relevant_event (#57)
- optimize index build with varint postings and unified visibility (#54)
- eliminate #[allow], split benchmarks per module, add profiling (#22)
- rustfmt import formatting
- remove unused rel_match_context helper, use string literals
- Windows clippy, context prefix formatting, and expanded tests

### Documentation

- rewrite READMEs and AGENTS.md for clarity and index generality (#32)
- mark no-op flags (line-buffered, block-buffered, mmap, no-mmap) explicitly
- rewrite READMEs and add AGENTS.md to all projects and modules (#18)
- update Linux benchsuite snapshot with fresh results and chart
- tighten agent notes — scannable layout, no policy essays
- branch-per-phase workflow before roadmap slices

### Features

- redesign daemon architecture with explicit config, spawn lock, and --once mode (#56)
- unified Index trait, auto-init, incremental updates with fingerprint-based change detection (#31)
- modular public-API-only benchmarks (#26)
- comprehensive unit and integration test coverage for sift-core (#25)
- add --no-config, --unicode/--no-unicode, --colors, --regex-size-limit, --dfa-size-limit, -M/--max-columns, --max-columns-preview flags
- add --no-config, --unicode/--no-unicode, --colors, --regex-size-limit, --dfa-size-limit, -M/--max-columns, --max-columns-preview flags
- add -j/--threads, --line-buffered, --block-buffered, --path-separator, --one-file-system, -U/--multiline, --multiline-dotall, --crlf, --mmap/--no-mmap flags
- add -j/--threads, --line-buffered, --block-buffered, --path-separator, --one-file-system, -U/--multiline, --multiline-dotall, --crlf, --mmap/--no-mmap flags
- add -r/--replace, --trim, -b/--byte-offset, --passthru, --include-zero flags
- add -r/--replace, --trim, -b/--byte-offset, --passthru, --include-zero flags
- add --no-ignore-parent, --no-ignore-global, --no-ignore-exclude, --no-messages, --no-ignore-messages, --no-ignore-files, --ignore-file flags
- add --no-ignore-parent, --no-ignore-global, --no-ignore-exclude, --no-messages, --no-ignore-messages, --no-ignore-files, --ignore-file flags
- add -a/--text and --binary flags for binary file handling
- add -a/--text and --binary flags for binary file handling
- add filter flags for max-depth, max-filesize, types, iglob, files, sort
- add --max-depth, --max-filesize, --iglob, --ignore-file, --files, -t/--type, -T/--type-not, --type-list, --type-add, --type-clear, --sort/--sortr filter flags
- add --context-separator, --no-context-separator, --field-match-separator, --field-context-separator flags (#10)
- add --column, --vimgrep, --pretty, -N/--no-line-number, --version flags (#9)
- implement scope-based path display resolution
- --json JSON Lines output (ripgrep-compatible) (#6)
- bytes searched in SearchStats and --stats (#5)
- elapsed time in SearchStats and --stats output (#4)
- --stats and SearchStats counters (#3)
- --color, --null, grouped output structs (#2)
- context lines (-A/-B/-C) for standard search
- search parity — paths, ignores, follow, filter pipeline

### Miscellaneous

- bump softprops/action-gh-release from 2 to 3 (#34)
- bump actions/upload-artifact from 4 to 7 (#35)
- bump actions/download-artifact from 4 to 8 (#36)
- add release infrastructure — changelog, release script, ARM64, checksums, dependabot (#33)
- remove sift-profile binary from core crate (#23)
- fix pre-existing clippy lints (map_or, is_ok_and, byte str literals) (#17)
- remove fff.nvim
- remove useless scripts
- remove unused #[allow(dead_code)] from rel_match_context

### Performance

- optimized trigram index build with packed sort and codec removal (#55)
- parallel corpus walk with WalkBuilder::build_parallel() (#47)
- extract trigrams from raw bytes instead of lossy UTF-8 (#48)
- reduce PathBuf allocations in resolve_candidates (#49)
- parallelize save_to_dir index file writes (#50)
- avoid materializing Vec in all_file_ids (#51)
- sift-profile revamp, matcher/searcher caches, parallel and index tuning

### Refactor

- split search and grep modules, add index intersection planning (#30)
- remove parallel threshold, always use Rayon (#28)
- split grep module into domain folders (#27)
- restructure core into index/, grep/, and query/ modules (#24)
- harden integration test suite with TestProject helper (#21)
- organize CLI into domain-oriented modules (#19)
- replace Option<bool> with ColumnAction enum for max_columns
- avoid needless String allocation and double trim_start()
- add doc comments to ignore-granular structs
- rename parse_filesize to parse_size_suffix for consistency

### Testing

- add 200 inline unit tests, convert CLI to lib+bin layout (#20)

### Deps

- bump clap from 4.6.0 to 4.6.1 (#37)
- bump serde_json from 1.0.149 to 1.0.150 (#38)
- bump rayon from 1.11.0 to 1.12.0 (#39)
- bump bitflags from 2.11.0 to 2.11.1 (#40)

## [0.1.2](https://github.com/botirk38/sift/releases/tag/v0.1.2) — 2026-04-02

### Bug Fixes

- remove double-filtering bug in candidate pipeline
- use line_path for path extraction in glob integration tests
- wire IgnoreSources into SearchFilter with ripgrep defaults
- correct glob filter semantics and add integration tests
- separate quiet from output mode via OutputEmission enum
- reject -m 0 with error exit code (ripgrep-compatible)
- make -m/--max-count per-file (ripgrep-compatible semantics)

### Features

- add --glob-case-insensitive flag
- add --no-filename with ripgrep-compatible semantics
- add --count-matches, fix -c/-o normalization, omit zero-count files
- add -g/--glob path filtering with ignore::overrides
- add -h/--no-filename and --help flags
- add -s/--case-sensitive, -S/--smart-case with ripgrep-compatible precedence

### Performance

- preallocate postings buffer; add perf-baseline script
- parallel filter+prep pipeline, CandidateInfo, P0 bytes fix

### Refactor

- redesign benchmark suite with filter, mode, and output scenarios
- typed SearchFilter abstraction for search-time filtering
- output modes use ripgrep last-flag-wins semantics
- move output mode resolution into run_search, add conflict detection

### Reverted

- restore -h to ripgrep-compatible help, remove broken -g

### Audit

- align planner precedence with verify, add -w/-x combination tests

## [0.1.1](https://github.com/botirk38/sift/releases/tag/v0.1.1) — 2026-03-24

### Bug Fixes

- use serde_json to serialize index metadata in test
- normalize test paths across platforms
- normalize test paths across platforms
- normalize cli path tests and chart from csv
- skip binary files and symlinked files by default
- support single-file corpora with JSON index metadata
- update bench/profile to use .sift layout

### Documentation

- replace remote chart with local asset
- add generated performance chart

### Features

- tune parallel threshold

### Miscellaneous

- bump version to 0.1.1
- initialize benchsuite with uv

### Performance

- refactor search runtime for faster scans
- rewrite planner with Unicode-aware HIR extraction
- add profiling infrastructure (criterion benches, benchsuite upgrades)

### Testing

- add comprehensive integration test suite (28 new tests)

### Index

- migrate storage layout to .sift/.index
- switch to mmap-backed storage with O(1) file lookup

### Search

- use cached paths and id-based candidates
- reduce candidate path and printer overhead
- align execution with ripgrep printer pipeline
- normalize CLI output and migrate scanning to grep stack

## [0.1.0](https://github.com/botirk38/sift/releases/tag/v0.1.0) — 2026-03-24

### Documentation

- README/AGENTS per crate; CI on Linux/macOS/Windows

### Features

- indexed search with prefilter, parallel index build, and clippy-clean profile

### Miscellaneous

- simplify publish workflow — rely on ci.yml, not own validate job
- add sift-core publish workflow on tag push
- remove plan.md
- add skills.sh-installable sift-cli skill under skills/

### Refactor

- move Index into index/ module with IndexBuilder

### Testing

- reorganize CLI integration coverage

### Search

- use regex-automata with explicit cache management
- skip redundant canonicalize in indexed search
- cache parallel scan threshold with OnceLock
- byte-first scanning with regex::bytes::Regex, remove prefilter


