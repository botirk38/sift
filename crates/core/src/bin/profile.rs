//! Hot-loop timings (tab-separated `metric` lines) and `cargo flamegraph` target for sift-core.
//!
//! Built only with `--features profile`. Prefer **`./scripts/profile.sh`** from the repo root.
//!
//! ## Scenario categories
//!
//! **Query-planning** — exercises different trigram/verify paths; filter/output are minimal:
//!   `literal_narrow` · `word_literal` · `line_literal` · `fixed_string`
//!   `casei_literal` · `smart_case_lower` · `smart_case_upper`
//!   `required_literal` · `no_literal` · `alternation` · `alternation_casei`
//!   `unicode_class`
//!
//! **Filter + query** — exercises `SearchFilter` paths on top of query planning:
//!   `glob_include` · `glob_exclude` · `glob_casei`
//!   `hidden_default` · `hidden_include`
//!   `ignore_default` · `ignore_custom`
//!   `scoped_search`
//!
//! **Output-mode** — exercises `run_index` mode branches:
//!   `only_matching` · `count` · `count_matches`
//!   `files_with_matches` · `files_without_match`
//!   `max_count_1`
//!
//! ## Corpus fixtures
//!
//! **parity**: 2 files (`a/x.txt`, `b/y.txt`) — narrow + full-scan alike.
//! **large**: ~8k files × 100 lines across 256 crate dirs (set `SIFT_LARGE=1`).
//! **`filter_corpus`**: mixed extensions + hidden files + scoped subdirs + ignore files.
//!
//! ## Environment
//!
//! | Variable | Effect |
//! |---|---|
//! | `SIFT_LARGE=1` | Use large corpus instead of parity |
//! | `SIFT_CORPUS_FILES=N` | Custom file count |
//! | `SIFT_CORPUS_LINES=N` | Lines per file (large corpus) |
//! | `SIFT_CORPUS_DIRS=N` | Directory fan-out (large corpus) |
//! | `SIFT_ITERS=N` | Fixed iteration count |
//! | `SIFT_LOOP_SECS=N` | Run each scenario for N seconds |
//! | `SIFT_PROFILE_CORPUS` | Use external corpus path (skips materialisation) |
//! | `SIFT_PROFILE_INDEX` | Index directory for external corpus (default: `<corpus>.sift`) |

use std::fs;
use std::hint::black_box;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sift_core::{
    CaseMode, CompiledSearch, FilenameMode, GlobConfig, HiddenMode, IgnoreConfig, IgnoreSources,
    Index, IndexBuilder, OutputEmission, SearchFilter, SearchFilterConfig, SearchMatchFlags,
    SearchMode, SearchOptions, SearchOutput, TrigramPlan, VisibilityConfig,
};

#[derive(Clone, Debug)]
enum CorpusKind {
    Parity,
    Filter,
    Large {
        files: usize,
        lines_per_file: usize,
        dir_fanout: usize,
    },
}

fn corpus_kind() -> CorpusKind {
    let large = std::env::var("SIFT_LARGE")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let files = std::env::var("SIFT_CORPUS_FILES")
        .ok()
        .and_then(|s| s.parse().ok());

    if large && files.is_none() {
        return CorpusKind::Large {
            files: 8_000,
            lines_per_file: std::env::var("SIFT_CORPUS_LINES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            dir_fanout: std::env::var("SIFT_CORPUS_DIRS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(256),
        };
    }

    match files {
        None | Some(0) => {
            if std::env::var("SIFT_FILTER_CORPUS").is_ok() {
                CorpusKind::Filter
            } else {
                CorpusKind::Parity
            }
        }
        Some(n) => CorpusKind::Large {
            files: n,
            lines_per_file: std::env::var("SIFT_CORPUS_LINES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            dir_fanout: std::env::var("SIFT_CORPUS_DIRS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(256),
        },
    }
}

/// Parity corpus: a/x.txt ("alpha beta"), b/y.txt ("gamma delta").
fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha beta\n").unwrap();
    fs::write(root.join("b/y.txt"), "gamma delta\n").unwrap();
}

/// Filter-testing corpus with mixed file types, hidden files, scoped subdirs,
/// and ignore markers.
///
/// Structure:
///   a/x.txt          — visible, contains "beta"
///   a/.hidden.txt    — hidden, contains "beta"
///   a/data.rs        — visible, no match (for glob exclude)
///   a/.secret/log    — hidden file in hidden dir (respect hidden → skip)
///   subdir/a.txt     — in subdir, contains "beta" (for scoped search)
///   subdir/b.log     — in subdir, no match
///   root.txt         — at corpus root, contains "beta" (outside scope)
///   skip/ignored.txt — gitignored, contains "beta"
///   `also_skip/omit.txt` — .ignored, contains "beta"
///   keep.txt         — outside any ignore rule, contains "beta"
fn make_filter_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("a/.secret")).unwrap();
    fs::create_dir_all(root.join("subdir")).unwrap();
    fs::create_dir_all(root.join("skip")).unwrap();
    fs::create_dir_all(root.join("also_skip")).unwrap();

    fs::write(root.join("a/x.txt"), "alpha beta gamma\n").unwrap();
    fs::write(root.join("a/.hidden.txt"), "beta in hidden file\n").unwrap();
    fs::write(root.join("a/data.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("a/.secret/log"), "beta in hidden dir\n").unwrap();
    fs::write(root.join("subdir/a.txt"), "beta in subdir\n").unwrap();
    fs::write(root.join("subdir/b.log"), "no match here\n").unwrap();
    fs::write(root.join("root.txt"), "beta at root level\n").unwrap();
    fs::write(root.join("skip/ignored.txt"), "beta gitignored\n").unwrap();
    fs::write(root.join("also_skip/omit.txt"), "beta in .ignore\n").unwrap();
    fs::write(root.join("keep.txt"), "beta outside ignore rules\n").unwrap();

    fs::write(root.join(".gitignore"), "skip/\n").unwrap();
    fs::write(root.join(".ignore"), "also_skip/\n").unwrap();
}

/// Monorepo-shaped tree: `crates/cNNNN/src/module_M.rs` with many lines of pseudo-Rust.
fn materialize_large_corpus(root: &Path, files: usize, lines_per_file: usize, dir_fanout: usize) {
    let fanout = dir_fanout.max(1);
    for i in 0..files {
        let c = i % fanout;
        let path = root
            .join("crates")
            .join(format!("c{c:04}"))
            .join("src")
            .join(format!("module_{i}.rs"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(&path).unwrap();
        for line in 0..lines_per_file {
            let mid = if line % 47 == 3 {
                " beta "
            } else if line % 91 == 7 {
                " RESUME "
            } else if line % 31 == 11 {
                " ERR_SYS "
            } else {
                " xval "
            };
            writeln!(
                f,
                "// {i}:{line} fn sym_{line}(){mid} struct Row{{ id: u32 }}"
            )
            .unwrap();
        }
    }
}

fn make_many_files_corpus(root: &Path, n: usize) {
    for i in 0..n {
        let dir = root.join(format!("d{}", i % 10));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("f{i}.txt")),
            format!("line one line two content {i}\n"),
        )
        .unwrap();
    }
}

fn materialize_search_corpus(root: &Path, kind: &CorpusKind) {
    match kind {
        CorpusKind::Parity => make_parity_corpus(root),
        CorpusKind::Filter => make_filter_corpus(root),
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => materialize_large_corpus(root, *files, *lines_per_file, *dir_fanout),
    }
}

fn materialize_build_corpus(root: &Path, kind: &CorpusKind) {
    match kind {
        CorpusKind::Parity | CorpusKind::Filter => make_many_files_corpus(root, 32),
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => materialize_large_corpus(root, *files, *lines_per_file, *dir_fanout),
    }
}

fn external_corpus_paths() -> Option<(PathBuf, PathBuf)> {
    let corpus = std::env::var_os("SIFT_PROFILE_CORPUS").map(PathBuf::from)?;
    let index = std::env::var_os("SIFT_PROFILE_INDEX").map_or_else(
        || PathBuf::from(format!("{}.sift", corpus.display())),
        PathBuf::from,
    );
    Some((corpus, index))
}

fn open_corpus_index(kind: &CorpusKind) -> (tempfile::TempDir, Index) {
    if let Some((corpus, index_dir)) = external_corpus_paths() {
        let t_open = Instant::now();
        let index = Index::open(&index_dir).unwrap();
        let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
        println!("metric\tcorpus_kind\texternal");
        println!("metric\tcorpus_root\t{}", corpus.display());
        println!("metric\tindex_root\t{}", index_dir.display());
        println!("metric\tphase_open_index_ms\t{open_ms:.3}");
        return (tempfile::tempdir().unwrap(), index);
    }

    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");

    let t_mat = Instant::now();
    materialize_search_corpus(&corpus, kind);
    let mat_ms = t_mat.elapsed().as_secs_f64() * 1e3;
    println!("metric\tphase_materialize_corpus_ms\t{mat_ms:.3}");

    match kind {
        CorpusKind::Parity => {
            println!("metric\tcorpus_kind\tparity");
            println!("metric\tcorpus_files\t2");
        }
        CorpusKind::Filter => {
            println!("metric\tcorpus_kind\tfilter");
            println!("metric\tcorpus_files\t12");
        }
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => {
            println!("metric\tcorpus_kind\tlarge");
            println!("metric\tcorpus_files\t{files}");
            println!("metric\tcorpus_lines_per_file\t{lines_per_file}");
            println!("metric\tcorpus_dir_fanout\t{dir_fanout}");
        }
    }

    let idx = tmp.path().join(".sift");
    let t0 = Instant::now();
    let _ = IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    let build_ms = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    let index = Index::open(&idx).unwrap();
    let open_ms = t1.elapsed().as_secs_f64() * 1e3;
    println!("metric\tphase_build_index_ms\t{build_ms:.3}");
    println!("metric\tphase_open_index_ms\t{open_ms:.3}");
    (tmp, index)
}

enum Loop {
    Timed(Duration),
    Iters(usize),
}

fn ns_per_iter(elapsed: Duration, iters: usize) -> u128 {
    if iters == 0 {
        return 0;
    }
    let iters_u128 = u128::try_from(iters).unwrap_or(u128::MAX);
    elapsed.as_nanos() / iters_u128
}

const fn default_search_iters(kind: &CorpusKind) -> usize {
    match kind {
        CorpusKind::Parity | CorpusKind::Filter => 2_000_000,
        CorpusKind::Large { .. } => 5_000,
    }
}

fn loop_config(kind: &CorpusKind) -> Loop {
    if let Ok(s) = std::env::var("SIFT_LOOP_SECS") {
        let secs: u64 = s.parse().unwrap_or(15);
        return Loop::Timed(Duration::from_secs(secs));
    }
    let iters: usize = std::env::var("SIFT_ITERS")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or_else(|| default_search_iters(kind));
    Loop::Iters(iters)
}

#[derive(Clone, Debug)]
struct Scenario {
    name: &'static str,
    patterns: Vec<String>,
    opts: SearchOptions,
    filter_config: SearchFilterConfig,
    output: SearchOutput,
}

impl Scenario {
    const fn new(
        name: &'static str,
        patterns: Vec<String>,
        opts: SearchOptions,
        filter_config: SearchFilterConfig,
        output: SearchOutput,
    ) -> Self {
        Self {
            name,
            patterns,
            opts,
            filter_config,
            output,
        }
    }

    fn default_filter() -> SearchFilterConfig {
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        }
    }
}

const fn make_output(mode: SearchMode, emission: OutputEmission) -> SearchOutput {
    SearchOutput {
        mode,
        emission,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    }
}

const fn default_output() -> SearchOutput {
    make_output(SearchMode::Standard, OutputEmission::Quiet)
}

fn literal_narrow() -> Scenario {
    Scenario::new(
        "literal_narrow",
        vec!["beta".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        default_output(),
    )
}

fn word_literal() -> Scenario {
    Scenario::new(
        "word_literal",
        vec!["beta".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::WORD_REGEXP,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn line_literal() -> Scenario {
    Scenario::new(
        "line_literal",
        vec!["beta".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::LINE_REGEXP,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn fixed_string() -> Scenario {
    Scenario::new(
        "fixed_string",
        vec!["beta.gamma".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::FIXED_STRINGS,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn casei_literal() -> Scenario {
    Scenario::new(
        "casei_literal",
        vec!["beta".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Insensitive,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn smart_case_lower() -> Scenario {
    Scenario::new(
        "smart_case_lower",
        vec!["beta".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Smart,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn smart_case_upper() -> Scenario {
    Scenario::new(
        "smart_case_upper",
        vec!["Beta".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Smart,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn required_literal() -> Scenario {
    Scenario::new(
        "required_literal",
        vec!["[A-Z]+_RESUME".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        default_output(),
    )
}

fn no_literal() -> Scenario {
    Scenario::new(
        "no_literal",
        vec![r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        default_output(),
    )
}

fn alternation() -> Scenario {
    Scenario::new(
        "alternation",
        vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        default_output(),
    )
}

fn alternation_casei() -> Scenario {
    Scenario::new(
        "alternation_casei",
        vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Insensitive,
            max_results: None,
        },
        Scenario::default_filter(),
        default_output(),
    )
}

fn unicode_class() -> Scenario {
    Scenario::new(
        "unicode_class",
        vec![r"\p{Greek}".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        default_output(),
    )
}

fn glob_include() -> Scenario {
    Scenario::new(
        "glob_include",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig {
                patterns: vec!["**/*.txt".to_string()],
                case_insensitive: false,
            },
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        default_output(),
    )
}

fn glob_exclude() -> Scenario {
    Scenario::new(
        "glob_exclude",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig {
                patterns: vec!["!**/*.txt".to_string()],
                case_insensitive: false,
            },
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        default_output(),
    )
}

fn glob_casei() -> Scenario {
    Scenario::new(
        "glob_casei",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig {
                patterns: vec!["**/*.TXT".to_string()],
                case_insensitive: true,
            },
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        default_output(),
    )
}

fn hidden_default() -> Scenario {
    Scenario::new(
        "hidden_default",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        default_output(),
    )
}

fn hidden_include() -> Scenario {
    Scenario::new(
        "hidden_include",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        default_output(),
    )
}

fn ignore_default() -> Scenario {
    Scenario::new(
        "ignore_default",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        default_output(),
    )
}

fn ignore_custom() -> Scenario {
    Scenario::new(
        "ignore_custom",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::empty(),
                    custom_files: vec![PathBuf::from(".ignore")],
                    require_git: false,
                },
            },
        },
        default_output(),
    )
}

fn scoped_search() -> Scenario {
    Scenario::new(
        "scoped_search",
        vec!["beta".to_string()],
        SearchOptions::default(),
        SearchFilterConfig {
            scopes: vec![PathBuf::from("subdir")],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Respect,
                ignore: IgnoreConfig {
                    sources: IgnoreSources::DOT | IgnoreSources::VCS | IgnoreSources::EXCLUDE,
                    custom_files: Vec::new(),
                    require_git: true,
                },
            },
        },
        make_output(SearchMode::FilesWithMatches, OutputEmission::Normal),
    )
}

fn only_matching() -> Scenario {
    Scenario::new(
        "only_matching",
        vec!["beta".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        make_output(SearchMode::OnlyMatching, OutputEmission::Normal),
    )
}

fn count() -> Scenario {
    Scenario::new(
        "count",
        vec!["beta".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        make_output(SearchMode::Count, OutputEmission::Normal),
    )
}

fn count_matches() -> Scenario {
    Scenario::new(
        "count_matches",
        vec!["beta".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        make_output(SearchMode::CountMatches, OutputEmission::Normal),
    )
}

fn files_with_matches() -> Scenario {
    Scenario::new(
        "files_with_matches",
        vec!["beta".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        make_output(SearchMode::FilesWithMatches, OutputEmission::Normal),
    )
}

fn files_without_match() -> Scenario {
    Scenario::new(
        "files_without_match",
        vec!["beta".to_string()],
        SearchOptions::default(),
        Scenario::default_filter(),
        make_output(SearchMode::FilesWithoutMatch, OutputEmission::Normal),
    )
}

fn max_count_1() -> Scenario {
    Scenario::new(
        "max_count_1",
        vec!["beta".to_string()],
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Sensitive,
            max_results: Some(1),
        },
        Scenario::default_filter(),
        make_output(SearchMode::Standard, OutputEmission::Normal),
    )
}

#[allow(clippy::type_complexity)]
const ALL_SCENARIOS: &[(&str, fn() -> Scenario)] = &[
    ("literal_narrow", literal_narrow),
    ("word_literal", word_literal),
    ("line_literal", line_literal),
    ("fixed_string", fixed_string),
    ("casei_literal", casei_literal),
    ("smart_case_lower", smart_case_lower),
    ("smart_case_upper", smart_case_upper),
    ("required_literal", required_literal),
    ("no_literal", no_literal),
    ("alternation", alternation),
    ("alternation_casei", alternation_casei),
    ("unicode_class", unicode_class),
    ("glob_include", glob_include),
    ("glob_exclude", glob_exclude),
    ("glob_casei", glob_casei),
    ("hidden_default", hidden_default),
    ("hidden_include", hidden_include),
    ("ignore_default", ignore_default),
    ("ignore_custom", ignore_custom),
    ("scoped_search", scoped_search),
    ("only_matching", only_matching),
    ("count", count),
    ("count_matches", count_matches),
    ("files_with_matches", files_with_matches),
    ("files_without_match", files_without_match),
    ("max_count_1", max_count_1),
];

fn find_scenario(name: &str) -> Option<Scenario> {
    for (n, f) in ALL_SCENARIOS {
        if *n == name {
            return Some(f());
        }
    }
    None
}

fn run_scenario(index: &Index, scenario: &Scenario, loop_cfg: &Loop) {
    let t_plan = Instant::now();
    let query = CompiledSearch::new(&scenario.patterns, scenario.opts).unwrap();
    let plan_us = t_plan.elapsed().as_micros();

    let plan_kind = match &query.plan {
        TrigramPlan::Narrow { .. } => "narrow",
        TrigramPlan::FullScan => "full_scan",
    };

    let total_files = index.file_count();

    let t_candidates = Instant::now();
    let filter = SearchFilter::new(&scenario.filter_config, &index.root).unwrap();
    let candidate_ids = query.candidate_file_ids(index, &filter, false);
    let candidates_us = t_candidates.elapsed().as_micros();
    let candidate_count = candidate_ids.len();

    let t_matcher = Instant::now();
    let _matcher = query.build_matcher().unwrap();
    let matcher_us = t_matcher.elapsed().as_micros();

    let t_search = Instant::now();
    let iters = match loop_cfg {
        Loop::Timed(d) => {
            let deadline = Instant::now() + *d;
            let mut n = 0usize;
            let filter = SearchFilter::new(&scenario.filter_config, &index.root).unwrap();
            while Instant::now() < deadline {
                black_box(query.run_index(index, &filter, scenario.output).unwrap());
                n += 1;
            }
            n
        }
        Loop::Iters(n) => {
            let filter = SearchFilter::new(&scenario.filter_config, &index.root).unwrap();
            for _ in 0..*n {
                black_box(query.run_index(index, &filter, scenario.output).unwrap());
            }
            *n
        }
    };
    let search_elapsed = t_search.elapsed();
    let search_us = search_elapsed.as_micros();
    let ns = ns_per_iter(search_elapsed, iters);

    println!("metric\tscenario\t{}", scenario.name);
    println!("metric\titers\t{iters}");
    println!("metric\tplan_kind\t{plan_kind}");
    println!("metric\tsearch_mode\t{:?}", scenario.output.mode);
    println!("metric\ttotal_files\t{total_files}");
    println!("metric\tcandidate_files\t{candidate_count}");
    println!("metric\tphase_plan_us\t{plan_us}");
    println!("metric\tphase_candidate_us\t{candidates_us}");
    println!("metric\tphase_matcher_us\t{matcher_us}");
    println!("metric\tphase_search_us\t{search_us}");
    println!(
        "metric\ttotal_ms\t{:.3}",
        search_elapsed.as_secs_f64() * 1e3
    );
    println!("metric\tns_per_iter\t{ns}");
}

const fn default_build_iters(kind: &CorpusKind) -> usize {
    match kind {
        CorpusKind::Parity | CorpusKind::Filter => 500,
        CorpusKind::Large { .. } => 2,
    }
}

fn run_build(iters: usize, kind: &CorpusKind) {
    let max_cap = match kind {
        CorpusKind::Parity | CorpusKind::Filter => 500,
        CorpusKind::Large { .. } => 20,
    };
    let iters = iters.clamp(1, max_cap);
    let t0 = Instant::now();
    for _ in 0..iters {
        let tmp = tempfile::tempdir().unwrap();
        let corpus = tmp.path().join("corpus");
        materialize_build_corpus(&corpus, kind);
        let idx = tmp.path().join(".sift");
        let _ = IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    }
    let elapsed = t0.elapsed();
    let ns = ns_per_iter(elapsed, iters);
    match kind {
        CorpusKind::Parity => {
            println!("metric\tcorpus_kind\tparity");
            println!("metric\tcorpus_files\t32");
            println!("metric\tmode\tbuild_small_32files");
        }
        CorpusKind::Filter => {
            println!("metric\tcorpus_kind\tfilter");
            println!("metric\tcorpus_files\t32");
            println!("metric\tmode\tbuild_small_32files");
        }
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => {
            println!("metric\tcorpus_kind\tlarge");
            println!("metric\tcorpus_files\t{files}");
            println!("metric\tcorpus_lines_per_file\t{lines_per_file}");
            println!("metric\tcorpus_dir_fanout\t{dir_fanout}");
            println!("metric\tmode\tbuild_large");
        }
    }
    println!("metric\titers\t{iters}");
    println!("metric\ttotal_ms\t{:.3}", elapsed.as_secs_f64() * 1e3);
    println!("metric\tns_per_iter\t{ns}");
}

fn list_scenarios() {
    for (name, _) in ALL_SCENARIOS {
        println!("{name}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map_or("literal_narrow", |s| s.as_str());
    let kind = corpus_kind();

    if mode == "list" {
        list_scenarios();
        return;
    }

    if mode == "build" {
        if std::env::var("SIFT_LOOP_SECS").is_ok() {
            eprintln!("build mode: use SIFT_ITERS (SIFT_LOOP_SECS not supported)");
            std::process::exit(2);
        }
        let iters: usize = std::env::var("SIFT_ITERS")
            .ok()
            .and_then(|x| x.parse().ok())
            .unwrap_or_else(|| default_build_iters(&kind));
        run_build(iters, &kind);
        return;
    }

    let scenario = find_scenario(mode).unwrap_or_else(|| {
        eprintln!(
            "usage: sift-profile [list|build|{}]",
            ALL_SCENARIOS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join("|")
        );
        std::process::exit(2);
    });

    let (_tmp, index) = open_corpus_index(&kind);
    let loop_cfg = loop_config(&kind);
    run_scenario(&index, &scenario, &loop_cfg);
}
