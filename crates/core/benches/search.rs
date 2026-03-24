//! Criterion benchmarks: index build vs narrow vs full-scan search.
//!
//! Run: `cargo bench -p sift-core --bench search` or `./scripts/bench.sh`.
//! Pass Criterion flags after `--`: `cargo bench -p sift-core --bench search -- --save-baseline main`

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use sift_core::{
    CaseMode, CompiledSearch, FilenameMode, Index, IndexBuilder, OutputEmission, SearchMatchFlags,
    SearchMode, SearchOptions, SearchOutput,
};

fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha beta\n").unwrap();
    fs::write(root.join("b/y.txt"), "gamma delta\n").unwrap();
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

fn open_parity_index() -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);
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

const fn make_output() -> SearchOutput {
    SearchOutput {
        mode: SearchMode::Standard,
        emission: OutputEmission::Quiet,
        filename_mode: FilenameMode::Never,
        line_number: false,
    }
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

fn bench_search_literal_narrow(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions::default();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_literal_narrow");
    g.bench_function("beta_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_literal_narrow_large(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let opts = SearchOptions::default();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_literal_narrow_large");
    g.bench_function("beta_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_word_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::WORD_REGEXP,
        case_mode: CaseMode::Sensitive,
        max_results: None,
    };
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_word_literal");
    g.bench_function("beta_word_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_casei_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::default(),
        case_mode: CaseMode::Insensitive,
        max_results: None,
    };
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_casei_literal");
    g.bench_function("beta_casei_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_required_literal(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let opts = SearchOptions::default();
    let pat = vec!["[A-Z]+_RESUME".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_required_literal");
    g.bench_function("RESUME_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_unicode_class(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions::default();
    let pat = vec![r"\p{Greek}".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_unicode_class");
    g.bench_function("greek_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_no_literal(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions::default();
    let pat = vec![r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_no_literal");
    g.bench_function("word_boundary_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_alternation(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let opts = SearchOptions::default();
    let pat = vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_alternation");
    g.bench_function("err_codes_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_alternation_casei(c: &mut Criterion) {
    let (_tmp, index) = open_large_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::default(),
        case_mode: CaseMode::Insensitive,
        max_results: None,
    };
    let pat = vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_alternation_casei");
    g.bench_function("err_codes_ci_8k_files", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_line_regexp(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::LINE_REGEXP,
        case_mode: CaseMode::Sensitive,
        max_results: None,
    };
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_line_regexp");
    g.bench_function("beta_line_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_fixed_string(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::FIXED_STRINGS,
        case_mode: CaseMode::Sensitive,
        max_results: None,
    };
    let pat = vec!["beta.gamma".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_fixed_string");
    g.bench_function("beta_gamma_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_full_scan(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts_default = SearchOptions::default();
    let pat_dot = vec![".*".to_string()];
    let query_dot = CompiledSearch::new(&pat_dot, opts_default).unwrap();
    let mut g = c.benchmark_group("search_full_scan");
    g.bench_function("dot_star_parity", |b| {
        b.iter(|| {
            black_box(
                query_dot
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_smart_case_lowercase(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::default(),
        case_mode: CaseMode::Smart,
        max_results: None,
    };
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_smart_case_lowercase");
    g.bench_function("beta_smart_lower_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_smart_case_uppercase(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions {
        flags: SearchMatchFlags::default(),
        case_mode: CaseMode::Smart,
        max_results: None,
    };
    let pat = vec!["Beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_smart_case_uppercase");
    g.bench_function("Beta_smart_upper_parity", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(black_box(&index), &[], None, make_output())
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
        bench_search_literal_narrow,
        bench_search_literal_narrow_large,
        bench_search_word_literal,
        bench_search_casei_literal,
        bench_search_required_literal,
        bench_search_unicode_class,
        bench_search_no_literal,
        bench_search_alternation,
        bench_search_alternation_casei,
        bench_search_line_regexp,
        bench_search_fixed_string,
        bench_search_full_scan,
        bench_search_smart_case_lowercase,
        bench_search_smart_case_uppercase,
}
criterion_main!(benches);
