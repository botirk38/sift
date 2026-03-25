//! Criterion benchmarks for sift-core search performance.
//!
//! ## Scenario matrix
//!
//! **Query-planning** — exercises trigram/verify paths with minimal filter/output:
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
//! ## Running
//!
//! ```bash
//! cargo bench -p sift-core --bench search
//! ./scripts/bench.sh
//! ./scripts/bench.sh -- --save-baseline main   # save baseline
//! ./scripts/bench.sh -- --baseline main        # compare to saved
//! ```
//!
//! Pass Criterion flags after `--`: `cargo bench -p sift-core --bench search -- --help`

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use sift_core::{
    CaseMode, CompiledSearch, FilenameMode, GlobConfig, HiddenMode, IgnoreConfig, IgnoreSources,
    Index, IndexBuilder, OutputEmission, SearchFilter, SearchFilterConfig, SearchMatchFlags,
    SearchMode, SearchOptions, SearchOutput, VisibilityConfig,
};

fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha beta\n").unwrap();
    fs::write(root.join("b/y.txt"), "gamma delta\n").unwrap();
}

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
        let f = fs::File::create(&path).unwrap();
        let mut f = std::io::BufWriter::new(f);
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

fn open_parity_index() -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);
    let idx = tmp.path().join(".sift");
    IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    let index = Index::open(&idx).unwrap();
    (tmp, index)
}

fn open_filter_index() -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    make_filter_corpus(&corpus);
    let idx = tmp.path().join(".sift");
    IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    let index = Index::open(&idx).unwrap();
    (tmp, index)
}

fn open_large_index() -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    materialize_large_corpus(&corpus, 8_000, 100, 256);
    let idx = tmp.path().join(".sift");
    IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    let index = Index::open(&idx).unwrap();
    (tmp, index)
}

fn sift_criterion() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(10))
        .sample_size(150)
        .significance_level(0.05)
        .noise_threshold(0.05)
        .configure_from_args()
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

const fn output_std() -> SearchOutput {
    SearchOutput {
        mode: SearchMode::Standard,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    }
}

const fn output_quiet(mode: SearchMode) -> SearchOutput {
    SearchOutput {
        mode,
        emission: OutputEmission::Quiet,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    }
}

// ─── Build benchmarks ────────────────────────────────────────────────────────

fn bench_build_index(c: &mut Criterion) {
    let mut g = c.benchmark_group("build_index");
    g.bench_function("32_files_parity", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_many_files_corpus(&corpus, 32);
            let idx = tmp.path().join(".sift");
            IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
        });
    });
    g.bench_function("8k_files_large", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            materialize_large_corpus(&corpus, 8_000, 100, 256);
            let idx = tmp.path().join(".sift");
            IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
        });
    });
    g.finish();
}

// ─── Query-planning benchmarks ───────────────────────────────────────────────

fn bench_literal_narrow(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_literal_narrow");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_literal_narrow_large(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_literal_narrow_large");
    g.bench_function("beta_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_word_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::WORD_REGEXP,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_word_literal");
    g.bench_function("beta_word_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_line_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::LINE_REGEXP,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_line_literal");
    g.bench_function("beta_line_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_fixed_string(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta.gamma".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::FIXED_STRINGS,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_fixed_string");
    g.bench_function("beta_gamma_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_casei_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Insensitive,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_casei_literal");
    g.bench_function("beta_casei_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_smart_case_lower(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Smart,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_smart_case_lower");
    g.bench_function("beta_smart_lower_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_smart_case_upper(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["Beta".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Smart,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_smart_case_upper");
    g.bench_function("Beta_smart_upper_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_required_literal(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let pat = vec!["[A-Z]+_RESUME".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_required_literal");
    g.bench_function("RESUME_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_unicode_class(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec![r"\p{Greek}".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_unicode_class");
    g.bench_function("greek_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_no_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec![r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_no_literal");
    g.bench_function("word_boundary_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_alternation(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let pat = vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_alternation");
    g.bench_function("err_codes_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_alternation_casei(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let pat = vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Insensitive,
            max_results: None,
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_alternation_casei");
    g.bench_function("err_codes_ci_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

// ─── Filter + query benchmarks ───────────────────────────────────────────────

fn glob_include_filter() -> SearchFilterConfig {
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
    }
}

fn glob_exclude_filter() -> SearchFilterConfig {
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
    }
}

fn glob_casei_filter() -> SearchFilterConfig {
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
    }
}

fn hidden_include_filter() -> SearchFilterConfig {
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
    }
}

fn ignore_custom_filter() -> SearchFilterConfig {
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
    }
}

fn scoped_filter(subdir: &str) -> SearchFilterConfig {
    SearchFilterConfig {
        scopes: vec![PathBuf::from(subdir)],
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

fn bench_glob_include(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&glob_include_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_glob_include");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_glob_exclude(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&glob_exclude_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_glob_exclude");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_glob_casei(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&glob_casei_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_glob_casei");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_hidden_default(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_hidden_default");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_hidden_include(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&hidden_include_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_hidden_include");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_ignore_default(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_ignore_default");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_ignore_custom(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&ignore_custom_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_ignore_custom");
    g.bench_function("beta_filter_corpus", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &filter,
                        output_quiet(SearchMode::Standard),
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_scoped_search(c: &mut Criterion) {
    let (_tmp, index) = open_filter_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&scoped_filter("subdir"), &index.root).unwrap();
    let output = SearchOutput {
        mode: SearchMode::FilesWithMatches,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    };
    let mut g = c.benchmark_group("search_scoped");
    g.bench_function("beta_subdir_filter_corpus", |b| {
        b.iter(|| {
            black_box(query.run_index(black_box(&index), &filter, output).unwrap());
        });
    });
    g.finish();
}

// ─── Output-mode benchmarks ───────────────────────────────────────────────────

fn bench_only_matching(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let output = SearchOutput {
        mode: SearchMode::OnlyMatching,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    };
    let mut g = c.benchmark_group("search_only_matching");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(query.run_index(black_box(&index), &filter, output).unwrap());
        });
    });
    g.finish();
}

fn bench_count(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let output = SearchOutput {
        mode: SearchMode::Count,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    };
    let mut g = c.benchmark_group("search_count");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(query.run_index(black_box(&index), &filter, output).unwrap());
        });
    });
    g.finish();
}

fn bench_count_matches(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let output = SearchOutput {
        mode: SearchMode::CountMatches,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    };
    let mut g = c.benchmark_group("search_count_matches");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(query.run_index(black_box(&index), &filter, output).unwrap());
        });
    });
    g.finish();
}

fn bench_files_with_matches(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let output = SearchOutput {
        mode: SearchMode::FilesWithMatches,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    };
    let mut g = c.benchmark_group("search_files_with_matches");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(query.run_index(black_box(&index), &filter, output).unwrap());
        });
    });
    g.finish();
}

fn bench_files_without_match(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let output = SearchOutput {
        mode: SearchMode::FilesWithoutMatch,
        emission: OutputEmission::Normal,
        filename_mode: FilenameMode::Auto,
        line_number: false,
    };
    let mut g = c.benchmark_group("search_files_without_match");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(query.run_index(black_box(&index), &filter, output).unwrap());
        });
    });
    g.finish();
}

fn bench_max_count_1(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(
        &pat,
        SearchOptions {
            flags: SearchMatchFlags::default(),
            case_mode: CaseMode::Sensitive,
            max_results: Some(1),
        },
    )
    .unwrap();
    let filter = SearchFilter::new(&default_filter(), &index.root).unwrap();
    let mut g = c.benchmark_group("search_max_count_1");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &filter, output_std())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

criterion_group! {
    name = benches;
    config = sift_criterion();
    targets =
        bench_build_index,
        bench_literal_narrow,
        bench_literal_narrow_large,
        bench_word_literal,
        bench_line_literal,
        bench_fixed_string,
        bench_casei_literal,
        bench_smart_case_lower,
        bench_smart_case_upper,
        bench_required_literal,
        bench_unicode_class,
        bench_no_literal,
        bench_alternation,
        bench_alternation_casei,
        bench_glob_include,
        bench_glob_exclude,
        bench_glob_casei,
        bench_hidden_default,
        bench_hidden_include,
        bench_ignore_default,
        bench_ignore_custom,
        bench_scoped_search,
        bench_only_matching,
        bench_count,
        bench_count_matches,
        bench_files_with_matches,
        bench_files_without_match,
        bench_max_count_1,
}
criterion_main!(benches);
